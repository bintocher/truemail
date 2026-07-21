//! Полная и инкрементальная синхронизация календарей и контактов по
//! CalDAV/CardDAV и WebDAV Sync (RFC 6578). Работает с любым сервером,
//! реализующим эти RFC, а не только с Яндексом: адреса серверов приходят
//! параметром (заданы вручную или найдены через RFC 6764 - SRV-записи в
//! discover_srv, .well-known-редирект в discover_well_known), а схема
//! авторизации выбирается через DavAuth.
use crate::model::{
    Alarm, Attendee, AuthKind, ContactAddress, ContactPhone, Provider, clean_contact_name,
};
use crate::{Error, Result};
use hickory_resolver::proto::rr::RData;
use reqwest::{Client, Method, StatusCode};
use roxmltree::Document;
use std::collections::{HashMap, HashSet};
use url::Url;

/// Базовые адреса Яндекса по умолчанию - используются, если на аккаунте не
/// задан свой caldav_url/carddav_url. Для Яндекса RFC 6764-обнаружение не
/// запускается: эти адреса и так известны и стабильны, а лишний сетевой
/// запрос перед каждой синхронизацией не нужен.
pub const YANDEX_CALDAV_BASE: &str = "https://caldav.yandex.ru/";
pub const YANDEX_CARDDAV_BASE: &str = "https://carddav.yandex.ru/";

/// RFC 6764: стандартные пути обнаружения адреса CalDAV/CardDAV на домене.
pub const WELL_KNOWN_CALDAV: &str = "/.well-known/caldav";
pub const WELL_KNOWN_CARDDAV: &str = "/.well-known/carddav";

/// RFC 6764, раздел 3: имена SRV-сервисов для обнаружения DAV по DNS.
/// Берём только TLS-варианты: незашифрованные _caldav._tcp/_carddav._tcp
/// увели бы нас на http, а по http мы не ходим вовсе - там уходит
/// Authorization с паролем или OAuth-токеном.
pub const SRV_CALDAVS: &str = "_caldavs._tcp";
pub const SRV_CARDDAVS: &str = "_carddavs._tcp";

/// Потолок ожидания одного DNS-запроса. Обнаружение через SRV - механизм
/// опциональный и стоит в цепочке перед остальными источниками, поэтому
/// зависший или молчащий резолвер не должен задерживать подключение больше
/// чем на несколько секунд: лучше пойти дальше по цепочке, чем ждать.
const DNS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Подставить известные адреса Яндекса там, где на аккаунте своих ещё нет.
/// Вынесено в чистую функцию, чтобы не тянуть Account/AccountManager ради
/// теста "Яндекс продолжает использовать прежние адреса после обнаружения
/// DAV для остальных провайдеров".
pub fn resolve_yandex_bases(
    caldav_url: Option<&str>,
    carddav_url: Option<&str>,
) -> (String, String) {
    (
        caldav_url
            .map(str::to_owned)
            .unwrap_or_else(|| YANDEX_CALDAV_BASE.to_owned()),
        carddav_url
            .map(str::to_owned)
            .unwrap_or_else(|| YANDEX_CARDDAV_BASE.to_owned()),
    )
}

/// Схема авторизации DAV-запроса. Явный enum вместо цепочки if по
/// провайдеру внутри каждого запроса - выбор делается один раз в
/// dav_auth_scheme, а исполнение (какой заголовок поставить) - в одном
/// месте (apply_dav_auth).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DavAuthScheme {
    /// Basic с логином и OAuth-токеном вместо пароля. Так работает Яндекс:
    /// его DAV-серверы принимают OAuth access token через обычный Basic.
    BasicToken,
    /// Basic с обычным логином и паролем/app-specific password. Так
    /// работают iCloud, Mail.ru и подавляющее большинство generic-серверов.
    BasicPassword,
    /// Authorization: Bearer <token> - RFC 6750, стандартный способ отдать
    /// OAuth2-токен CalDAV/CardDAV-серверу, если он не завязан на Basic,
    /// как Яндекс.
    Bearer,
}

/// Данные для авторизации DAV-запроса. identity - логин для Basic
/// (игнорируется для Bearer); secret - пароль/app-password/OAuth-токен.
#[derive(Debug, Clone)]
pub struct DavAuth {
    pub scheme: DavAuthScheme,
    pub identity: String,
    pub secret: String,
}

impl DavAuth {
    pub fn new(
        scheme: DavAuthScheme,
        identity: impl Into<String>,
        secret: impl Into<String>,
    ) -> Self {
        Self {
            scheme,
            identity: identity.into(),
            secret: secret.into(),
        }
    }
}

/// Выбрать схему авторизации DAV по провайдеру и способу аутентификации
/// аккаунта. Яндекс - особый случай независимо от auth_kind (у него всегда
/// Oauth2, но токен идёт через Basic, а не Bearer). Остальные провайдеры с
/// Oauth2 (например Outlook) получают стандартный Bearer; Password/
/// AppPassword/Ntlm - обычный Basic с логином и секретом из keychain.
pub fn dav_auth_scheme(provider: Provider, auth_kind: AuthKind) -> DavAuthScheme {
    match (provider, auth_kind) {
        (Provider::Yandex, _) => DavAuthScheme::BasicToken,
        (_, AuthKind::Oauth2) => DavAuthScheme::Bearer,
        _ => DavAuthScheme::BasicPassword,
    }
}

/// pub(crate), а не private: используется и здесь (все PROPFIND/REPORT), и в
/// auxiliary.rs (PUT/DELETE события и контакта) - одна точка применения
/// схемы авторизации к запросу вместо двух похожих матчей по DavAuthScheme.
pub(crate) fn apply_dav_auth(
    request: reqwest::RequestBuilder,
    scheme: DavAuthScheme,
    auth: &DavAuth,
) -> reqwest::RequestBuilder {
    match scheme {
        DavAuthScheme::Bearer => request.bearer_auth(&auth.secret),
        DavAuthScheme::BasicToken | DavAuthScheme::BasicPassword => {
            request.basic_auth(&auth.identity, Some(&auth.secret))
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AuxiliarySyncCursors {
    pub calendars: HashMap<String, CollectionCursor>,
    pub contact_collections: HashMap<String, CollectionCursor>,
    pub contacts_sync_token: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CollectionCursor {
    pub ctag: Option<String>,
    pub sync_token: Option<String>,
    /// Последний известный ETag каждого ресурса коллекции. Нужен для
    /// безопасного fallback, если сервер не поддерживает RFC 6578 или
    /// отклонил устаревший sync-token.
    pub resource_etags: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SyncScope {
    #[default]
    Full,
    Delta,
    Unchanged,
}

#[derive(Debug, Default)]
pub struct DavSyncResult {
    pub calendars: Vec<DavCalendar>,
    /// Календарные коллекции действительно были доступны и прочитаны. При
    /// временной ошибке старые календари и события удалять нельзя.
    pub calendars_available: bool,
    pub contacts: Vec<DavContact>,
    /// Доступные CardDAV-коллекции. Нужны для создания первого контакта,
    /// когда адресная книга ещё пуста и URL нельзя вывести из vCard.
    pub contact_collections: Vec<DavCollection>,
    /// CardDAV-коллекция действительно была доступна и прочитана. Если Яндекс
    /// ещё не создал адресную книгу и вернул 404, локальные контакты удалять нельзя.
    pub contacts_available: bool,
    pub contacts_scope: SyncScope,
    pub contacts_sync_token: Option<String>,
    pub deleted_contact_urls: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DavCollection {
    pub url: String,
    pub ctag: Option<String>,
    pub sync_token: Option<String>,
}

#[derive(Debug)]
pub struct DavCalendar {
    pub url: String,
    pub name: String,
    pub ctag: Option<String>,
    pub sync_token: Option<String>,
    pub sync_scope: SyncScope,
    pub deleted_event_urls: Vec<String>,
    pub events: Vec<DavEvent>,
}

#[derive(Debug)]
pub struct DavEvent {
    pub remote_url: Option<String>,
    pub uid: String,
    pub summary: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub dtstart: String,
    pub dtend: Option<String>,
    pub rrule: Option<String>,
    pub recurrence_id: Option<String>,
    pub exdates: Option<String>,
    pub rdates: Option<String>,
    pub status: Option<String>,
    pub attendees: Vec<Attendee>,
    pub alarms: Vec<Alarm>,
    pub timezone: Option<String>,
    pub transp: Option<String>,
    pub class: Option<String>,
    pub categories: Vec<String>,
    pub url: Option<String>,
    pub organizer: Option<String>,
    pub sequence: i64,
    pub raw: String,
    pub etag: Option<String>,
}

#[derive(Debug)]
pub struct DavContact {
    pub remote_url: Option<String>,
    pub uid: String,
    pub display_name: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub organization: Option<String>,
    pub emails: Vec<String>,
    pub phones: Vec<ContactPhone>,
    pub addresses: Vec<ContactAddress>,
    pub raw: String,
    pub etag: Option<String>,
}

const PRINCIPAL_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?><d:propfind xmlns:d="DAV:"><d:prop><d:current-user-principal/></d:prop></d:propfind>"#;

const COLLECTIONS_BODY: &str = r#"<?xml version="1.0"?><d:propfind xmlns:d="DAV:" xmlns:cs="http://calendarserver.org/ns/"><d:prop><d:displayname/><d:resourcetype/><d:supported-report-set/><d:sync-token/><cs:getctag/></d:prop></d:propfind>"#;

const ETAG_LIST_BODY: &str = r#"<?xml version="1.0"?><d:propfind xmlns:d="DAV:"><d:prop><d:getetag/><d:resourcetype/></d:prop></d:propfind>"#;

#[derive(Debug)]
struct DavHttpResponse {
    status: StatusCode,
    body: String,
}

async fn dav_send(
    client: &Client,
    method: &Method,
    url: &str,
    depth: &str,
    body: &str,
    scheme: DavAuthScheme,
    auth: &DavAuth,
) -> Result<DavHttpResponse> {
    let request = apply_dav_auth(client.request(method.clone(), url), scheme, auth)
        .header("Depth", depth)
        .header("Content-Type", "application/xml; charset=utf-8")
        .body(body.to_owned());
    let response = request.send().await.map_err(|e| Error::Backend {
        backend: "dav".into(),
        message: e.to_string(),
    })?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Ok(DavHttpResponse { status, body })
}

async fn dav_request_response(
    client: &Client,
    method: &str,
    url: &str,
    depth: &str,
    body: &str,
    auth: &DavAuth,
) -> Result<DavHttpResponse> {
    let method = Method::from_bytes(method.as_bytes()).map_err(|e| Error::Other(e.to_string()))?;
    let response = dav_send(client, &method, url, depth, body, auth.scheme, auth).await?;
    // Bearer поддерживают не все серверы, объявляющие OAuth2 (некоторые
    // CalDAV-реализации, как Яндекс, ждут тот же токен через Basic). Один
    // молчаливый фолбэк на 401 - не перебор схем на каждый запрос.
    if response.status == StatusCode::UNAUTHORIZED && auth.scheme == DavAuthScheme::Bearer {
        return dav_send(
            client,
            &method,
            url,
            depth,
            body,
            DavAuthScheme::BasicToken,
            auth,
        )
        .await;
    }
    Ok(response)
}

async fn dav_request(
    client: &Client,
    method: &str,
    url: &str,
    depth: &str,
    body: &str,
    auth: &DavAuth,
) -> Result<String> {
    dav_request_optional(client, method, url, depth, body, auth)
        .await?
        .ok_or_else(|| Error::Backend {
            backend: "dav".into(),
            message: format!("{method} {url}: HTTP 404 Not Found"),
        })
}

/// Выполняет DAV-запрос, но позволяет вызывающему отличить отсутствующую
/// коллекцию от ошибки транспорта. Часть серверов (например, Яндекс) создаёт
/// CardDAV-книгу лениво, поэтому объявленный addressbook-home-set может
/// законно отвечать 404 до появления первой синхронизируемой адресной книги.
async fn dav_request_optional(
    client: &Client,
    method: &str,
    url: &str,
    depth: &str,
    body: &str,
    auth: &DavAuth,
) -> Result<Option<String>> {
    let response = dav_request_response(client, method, url, depth, body, auth).await?;
    if response.status == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if response.status != StatusCode::MULTI_STATUS && !response.status.is_success() {
        return Err(Error::Backend {
            backend: "dav".into(),
            message: format!(
                "{method} {url}: HTTP {}: {}",
                response.status, response.body
            ),
        });
    }
    Ok(Some(response.body))
}

fn resolve(base: &str, href: &str) -> Result<String> {
    Url::parse(base)
        .and_then(|url| url.join(href))
        .map(String::from)
        .map_err(|e| Error::Backend {
            backend: "dav-url".into(),
            message: e.to_string(),
        })
}

async fn discover_home(
    client: &Client,
    base: &str,
    home_tag: &str,
    auth: &DavAuth,
) -> Result<String> {
    let principal_xml = dav_request(client, "PROPFIND", base, "0", PRINCIPAL_BODY, auth).await?;
    let principal_doc = Document::parse(&principal_xml).map_err(|e| Error::Backend {
        backend: "dav-xml".into(),
        message: e.to_string(),
    })?;
    // Нельзя брать первый <href>: это обычно URL самого multistatus-response,
    // а не current-user-principal. Из-за этого calendar-home-set искался не там.
    let principal = principal_doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "current-user-principal")
        .and_then(|node| {
            node.descendants()
                .find(|n| n.is_element() && n.tag_name().name() == "href")
        })
        .and_then(|node| node.text())
        .map(str::to_owned)
        .ok_or_else(|| Error::Backend {
            backend: "dav".into(),
            message: "current-user-principal не найден".into(),
        })?;
    let principal_url = resolve(base, &principal)?;
    let body = r#"<?xml version="1.0" encoding="utf-8"?><d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav" xmlns:a="urn:ietf:params:xml:ns:carddav"><d:prop><c:calendar-home-set/><a:addressbook-home-set/></d:prop></d:propfind>"#;
    let xml = dav_request(client, "PROPFIND", &principal_url, "0", body, auth).await?;
    let doc = Document::parse(&xml).map_err(|e| Error::Backend {
        backend: "dav-xml".into(),
        message: e.to_string(),
    })?;
    let home = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == home_tag)
        .and_then(|node| {
            node.descendants()
                .find(|n| n.is_element() && n.tag_name().name() == "href")
        })
        .and_then(|node| node.text())
        .ok_or_else(|| Error::Backend {
            backend: "dav".into(),
            message: format!("{home_tag} не найден"),
        })?;
    resolve(base, home)
}

/// Проверить доступ к CalDAV и CardDAV без скачивания коллекций.
pub async fn validate_dav(auth: &DavAuth, caldav_base: &str, carddav_base: &str) -> Result<()> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| Error::Backend {
            backend: "dav".into(),
            message: e.to_string(),
        })?;
    discover_home(&client, caldav_base, "calendar-home-set", auth).await?;
    discover_home(&client, carddav_base, "addressbook-home-set", auth).await?;
    Ok(())
}

/// RFC 6764: обнаружение адреса CalDAV/CardDAV по well-known пути на
/// домене - сервер отвечает HTTP-редиректом (301/302/303/307/308) на
/// настоящий базовый URL коллекций; редиректы (до 5 переходов) допускаются,
/// т.к. некоторые серверы редиректят well-known на промежуточный путь.
/// `origin` - это схема+хост(+порт), например "https://icloud.com"; вынесен
/// отдельным параметром (а не захардкожен как https://) для тестируемости
/// на локальном mock-сервере по http.
///
/// Дополняется обнаружением через SRV-записи (см. discover_srv), которое
/// идёт раньше: well-known работает только если DAV живёт на том же хосте,
/// что и веб-сайт домена, а SRV позволяет владельцу домена явно указать
/// чужой хост и нестандартный порт.
pub async fn discover_well_known(origin: &str, path: &str) -> Option<String> {
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;
    let mut current = format!("{origin}{path}");
    let mut redirected = false;
    for _ in 0..5 {
        let response = client.get(&current).send().await.ok()?;
        let status = response.status();
        if status.is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)?
                .to_str()
                .ok()?
                .to_owned();
            current = resolve(&current, &location).ok()?;
            redirected = true;
            continue;
        }
        // Редирект с .well-known и есть ответ сервера на вопрос "где твой DAV"
        // (RFC 6764, раздел 5) - дальше идёт PROPFIND с авторизацией, а GET
        // сюда никто не обещал обслуживать. Требовать от цели успешного GET
        // нельзя: iCloud и Fastmail отвечают на неавторизованный запрос 401,
        // Radicale - 405, и discovery ломался бы ровно там, где нужен.
        // Если же редиректа не было вовсе, .well-known должен отдать сам себя
        // осмысленно - иначе это сервер без поддержки discovery.
        return (redirected
            || status.is_success()
            || status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::METHOD_NOT_ALLOWED)
            .then_some(current);
    }
    None
}

/// Кандидат из SRV-записи, приведённый к тому, что нам реально нужно:
/// хост без завершающей точки, порт и поля выбора. Отдельный тип, а не
/// hickory-шный SRV, чтобы выбор записи и сборка URL были чистыми функциями
/// и тестировались без DNS и без сети.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SrvTarget {
    host: String,
    port: u16,
    priority: u16,
    weight: u16,
}

/// Какой .well-known путь спрашивать у хоста, найденного через SRV, если у
/// него нет TXT-записи с путём. Пара service -> path зафиксирована RFC 6764
/// и не выводится алгоритмически, поэтому это явный матч; неизвестный
/// сервис (в т.ч. нешифрованные _caldav/_carddav) осознанно не
/// поддерживается.
fn well_known_for_service(service: &str) -> Option<&'static str> {
    match service {
        SRV_CALDAVS => Some(WELL_KNOWN_CALDAV),
        SRV_CARDDAVS => Some(WELL_KNOWN_CARDDAV),
        _ => None,
    }
}

/// Выбрать одну запись из набора SRV. RFC 2782 требует случайного выбора
/// внутри одного приоритета пропорционально весу; здесь выбор сделан
/// детерминированным (наименьший priority, при равенстве - наибольший
/// weight, при полном равенстве - лексикографически меньший host:port).
/// Причина: балансировка нагрузки нам не нужна - к DAV ходит один клиент,
/// и стабильный выбор важнее, потому что найденный базовый URL сохраняется
/// на аккаунте, а прыгающий между синхронизациями адрес означал бы
/// бессмысленную перезапись настроек и разъезжающиеся sync-token/ctag.
/// Записи с target "." (RFC 2782: сервис на домене не предоставляется) и с
/// нулевым портом отбрасываются.
fn pick_srv_target(records: &[SrvTarget]) -> Option<SrvTarget> {
    records
        .iter()
        .filter(|record| !record.host.is_empty() && record.host != "." && record.port != 0)
        .min_by(|left, right| {
            left.priority
                .cmp(&right.priority)
                .then(right.weight.cmp(&left.weight))
                .then(left.host.cmp(&right.host))
                .then(left.port.cmp(&right.port))
        })
        .cloned()
}

/// Разобрать TXT-запись имени сервиса (RFC 6764, раздел 6): её содержимое -
/// key=value пары в формате RFC 6763, из которых нас интересует только
/// "path=/dav/" - путь контекста DAV на найденном хосте. TXT-запись может
/// приходить несколькими character-string, поэтому на вход идёт срез строк,
/// а не одна строка. Регистр ключа игнорируется, значение без ведущего
/// слэша нормализуется - серверы пишут и "path=/dav/", и "path=dav/".
fn parse_srv_txt_path(chunks: &[String]) -> Option<String> {
    chunks
        .iter()
        .flat_map(|chunk| chunk.split_whitespace())
        .find_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            key.trim().eq_ignore_ascii_case("path").then_some(())?;
            let value = value.trim();
            if value.is_empty() {
                return None;
            }
            Some(if value.starts_with('/') {
                value.to_owned()
            } else {
                format!("/{value}")
            })
        })
}

/// Схема+хост(+порт) найденного через SRV сервера. Порт 443 в URL не
/// указываем: для https он подразумевается, а лишний ":443" сделал бы
/// сохранённый на аккаунте адрес отличным от того же адреса, полученного
/// через .well-known, и разные ветки обнаружения давали бы разные строки на
/// один и тот же сервер.
fn srv_origin(host: &str, port: u16) -> String {
    if port == 443 {
        format!("https://{host}")
    } else {
        format!("https://{host}:{port}")
    }
}

/// Принадлежит ли цель SRV тому же домену, что и адрес почты.
///
/// Проверка обязательна, потому что дальше по этому адресу уходит пароль
/// пользователя (DavAuthScheme::BasicPassword - это логин и пароль в
/// заголовке). SRV читается из обычного DNS без DNSSEC, то есть его может
/// подменить кто угодно на пути: провайдер, чужая точка доступа, отравленный
/// кэш. Без этой проверки одна подделанная запись увела бы учётные данные на
/// сервер злоумышленника, причём молча - подключение выглядело бы удачным.
/// У .well-known такой проблемы нет: там адрес и есть домен из почты.
///
/// Плата за это - домены, у которых DAV вынесен к стороннему хостеру
/// (mail.example.com -> dav.provider.net), через SRV не определятся. Такие
/// случаи закрываются ручным вводом адреса в настройках аккаунта, и это
/// честный размен: потерянное удобство против утечки пароля.
fn srv_target_is_trusted(host: &str, domain: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let domain = domain.trim_matches('.').to_ascii_lowercase();
    if host.is_empty() || domain.is_empty() {
        return false;
    }
    host == domain || host.ends_with(&format!(".{domain}"))
}

/// Базовый URL DAV из хоста, порта и пути контекста, взятого из TXT.
fn srv_base_url(host: &str, port: u16, path: &str) -> String {
    let origin = srv_origin(host, port);
    format!("{origin}{path}")
}

/// RFC 6764: обнаружение базового адреса CalDAV/CardDAV через DNS.
/// `service` - это SRV_CALDAVS или SRV_CARDDAVS, `domain` - домен из адреса
/// почты. Порядок действий по разделу 6 стандарта: SRV даёт хост и порт,
/// затем TXT того же имени может дать путь контекста ("path=/dav/"). Если
/// TXT нет - путь спрашиваем у самого найденного хоста через .well-known,
/// потому что SRV сообщает только транспорт, но не место коллекций.
///
/// Никогда не возвращает ошибку: DNS может быть недоступен, перехвачен
/// провайдером или просто не содержать записей - это нормальное состояние,
/// а не сбой подключения.
pub async fn discover_srv(domain: &str, service: &str) -> Option<String> {
    let well_known = well_known_for_service(service)?;
    let domain = domain.trim().trim_matches('.');
    if domain.is_empty() {
        return None;
    }
    // Завершающая точка делает имя полностью квалифицированным: без неё
    // резолвер перебирал бы ещё и search-домены системы, что и медленнее, и
    // может подсунуть чужой DAV из корпоративного search-суффикса.
    let name = format!("{service}.{domain}.");

    // Любая неудача ниже (нет резолвера, NXDOMAIN, таймаут, мусор в ответе)
    // гасится в None: SRV-обнаружение опционально и обязано просто передать
    // ход следующему источнику в цепочке, а не сорвать подключение.
    let resolver = hickory_resolver::Resolver::builder_tokio()
        .ok()?
        .build()
        .ok()?;

    let srv_answers = tokio::time::timeout(DNS_TIMEOUT, resolver.srv_lookup(name.clone()))
        .await
        .ok()?
        .ok()?;
    let targets = srv_answers
        .answers()
        .iter()
        .filter_map(|record| match &record.data {
            RData::SRV(srv) => Some(SrvTarget {
                host: srv.target.to_utf8().trim_end_matches('.').to_owned(),
                port: srv.port,
                priority: srv.priority,
                weight: srv.weight,
            }),
            _ => None,
        })
        .collect::<Vec<_>>();
    let target = pick_srv_target(&targets)?;
    if !srv_target_is_trusted(&target.host, domain) {
        // Отказ логируем: молчаливое игнорирование выглядело бы как "SRV не
        // настроен", и владелец домена искал бы ошибку не там.
        tracing::warn!(
            service,
            domain,
            target = %target.host,
            "SRV указывает на хост вне домена почты - пропускаем, чтобы не отправить учётные данные чужому серверу"
        );
        return None;
    }

    // Отсутствие TXT - штатный случай (RFC 6764 делает её необязательной),
    // поэтому здесь ошибка резолва не прерывает обнаружение, а превращается
    // в пустой список и уводит в ветку с .well-known.
    let chunks = match tokio::time::timeout(DNS_TIMEOUT, resolver.txt_lookup(name)).await {
        Ok(Ok(txt_answers)) => txt_answers
            .answers()
            .iter()
            .filter_map(|record| match &record.data {
                RData::TXT(txt) => Some(txt),
                _ => None,
            })
            .flat_map(|txt| {
                txt.txt_data
                    .iter()
                    .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
            })
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };

    let base = match parse_srv_txt_path(&chunks) {
        Some(path) => srv_base_url(&target.host, target.port, &path),
        // TXT нет - у найденного хоста ещё есть шанс ответить редиректом на
        // свой .well-known. Спрашиваем именно найденный хост, а не домен
        // почты: SRV для того и существует, что DAV живёт не там, где сайт.
        None => discover_well_known(&srv_origin(&target.host, target.port), well_known).await?,
    };
    tracing::debug!(service, domain, %base, "DAV обнаружен через SRV");
    Some(base)
}

fn response_parts(xml: &str, data_tag: &str) -> Result<Vec<(String, Option<String>, String)>> {
    let doc = Document::parse(xml).map_err(|e| Error::Backend {
        backend: "dav-xml".into(),
        message: e.to_string(),
    })?;
    let mut out = Vec::new();
    for response in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "response")
    {
        let href = response
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == "href")
            .and_then(|n| n.text())
            .unwrap_or("")
            .to_owned();
        let etag = response
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == "getetag")
            .and_then(|n| n.text())
            .map(str::to_owned);
        if let Some(data) = response
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == data_tag)
            .and_then(|n| n.text())
        {
            out.push((href, etag, data.to_owned()));
        }
    }
    Ok(out)
}

#[derive(Debug, Clone)]
struct DiscoveredCollection {
    url: String,
    name: String,
    ctag: Option<String>,
    sync_token: Option<String>,
    supports_sync_collection: bool,
}

fn parse_collections(
    xml: &str,
    base: &str,
    resource_type: &str,
    default_name: &str,
) -> Result<Vec<DiscoveredCollection>> {
    let doc = Document::parse(xml).map_err(|e| Error::Backend {
        backend: "dav-xml".into(),
        message: e.to_string(),
    })?;
    let mut collections = Vec::new();
    for response in doc
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "response")
    {
        if !response
            .descendants()
            .any(|node| node.is_element() && node.tag_name().name() == resource_type)
        {
            continue;
        }
        let Some(href) = response
            .descendants()
            .find(|node| node.is_element() && node.tag_name().name() == "href")
            .and_then(|node| node.text())
        else {
            continue;
        };
        let url = resolve(base, href)?;
        let name = response
            .descendants()
            .find(|node| node.is_element() && node.tag_name().name() == "displayname")
            .and_then(|node| node.text())
            .unwrap_or(default_name)
            .to_owned();
        let ctag = response
            .descendants()
            .find(|node| node.is_element() && node.tag_name().name() == "getctag")
            .and_then(|node| node.text())
            .map(str::to_owned);
        let sync_token = response
            .descendants()
            .find(|node| node.is_element() && node.tag_name().name() == "sync-token")
            .and_then(|node| node.text())
            .map(str::to_owned);
        let supports_sync_collection = response.descendants().any(|node| {
            node.is_element()
                && node.tag_name().name() == "sync-collection"
                && node.ancestors().any(|ancestor| {
                    ancestor.is_element() && ancestor.tag_name().name() == "supported-report-set"
                })
        });
        collections.push(DiscoveredCollection {
            url,
            name,
            ctag,
            sync_token,
            supports_sync_collection,
        });
    }
    Ok(collections)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResourceRef {
    href: String,
    url: String,
    etag: Option<String>,
}

#[derive(Debug)]
struct SyncCollectionDelta {
    sync_token: Option<String>,
    changed: Vec<ResourceRef>,
    deleted_urls: Vec<String>,
}

fn response_status<'a, 'input>(response: roxmltree::Node<'a, 'input>) -> Option<&'a str> {
    response
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "status")
        .and_then(|node| node.text())
}

fn parse_sync_collection(xml: &str, collection_url: &str) -> Result<SyncCollectionDelta> {
    let doc = Document::parse(xml).map_err(|e| Error::Backend {
        backend: "dav-xml".into(),
        message: e.to_string(),
    })?;
    let sync_token = doc
        .root_element()
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "sync-token")
        .and_then(|node| node.text())
        .map(str::to_owned);
    let mut changed = Vec::new();
    let mut deleted_urls = Vec::new();
    for response in doc
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "response")
    {
        let Some(href) = response
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "href")
            .and_then(|node| node.text())
        else {
            continue;
        };
        let url = resolve(collection_url, href)?;
        if let Some(status) = response_status(response) {
            if status.contains(" 404 ") {
                deleted_urls.push(url);
                continue;
            }
            if !status.contains(" 200 ") {
                return Err(Error::Backend {
                    backend: "dav".into(),
                    message: format!("sync-collection {href}: {status}"),
                });
            }
        }
        let etag = response
            .descendants()
            .find(|node| node.is_element() && node.tag_name().name() == "getetag")
            .and_then(|node| node.text())
            .map(str::to_owned);
        changed.push(ResourceRef {
            href: href.to_owned(),
            url,
            etag,
        });
    }
    changed.sort_by(|left, right| left.url.cmp(&right.url));
    changed.dedup_by(|left, right| left.url == right.url);
    deleted_urls.sort();
    deleted_urls.dedup();
    Ok(SyncCollectionDelta {
        sync_token,
        changed,
        deleted_urls,
    })
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn sync_collection_body(sync_token: Option<&str>) -> String {
    format!(
        r#"<?xml version="1.0"?><d:sync-collection xmlns:d="DAV:"><d:sync-token>{}</d:sync-token><d:sync-level>1</d:sync-level><d:prop><d:getetag/></d:prop></d:sync-collection>"#,
        sync_token.map(xml_escape).unwrap_or_default()
    )
}

#[derive(Debug)]
enum SyncReportOutcome {
    Success(SyncCollectionDelta),
    InvalidToken,
    Unsupported,
}

fn response_has_element(xml: &str, name: &str) -> bool {
    Document::parse(xml).is_ok_and(|doc| {
        doc.descendants()
            .any(|node| node.is_element() && node.tag_name().name() == name)
    })
}

async fn request_sync_collection(
    client: &Client,
    collection_url: &str,
    sync_token: Option<&str>,
    auth: &DavAuth,
) -> Result<SyncReportOutcome> {
    let body = sync_collection_body(sync_token);
    let response = dav_request_response(client, "REPORT", collection_url, "0", &body, auth).await?;
    if response.status == StatusCode::MULTI_STATUS || response.status.is_success() {
        return parse_sync_collection(&response.body, collection_url)
            .map(SyncReportOutcome::Success);
    }
    let invalid_token = sync_token.is_some()
        && (response.status == StatusCode::GONE
            || ((response.status == StatusCode::FORBIDDEN
                || response.status == StatusCode::CONFLICT)
                && response_has_element(&response.body, "valid-sync-token")));
    if invalid_token {
        return Ok(SyncReportOutcome::InvalidToken);
    }
    let unsupported = response.status == StatusCode::METHOD_NOT_ALLOWED
        || response.status == StatusCode::NOT_IMPLEMENTED
        || ((response.status == StatusCode::FORBIDDEN || response.status == StatusCode::CONFLICT)
            && response_has_element(&response.body, "supported-report"));
    if unsupported {
        return Ok(SyncReportOutcome::Unsupported);
    }
    Err(Error::Backend {
        backend: "dav".into(),
        message: format!(
            "REPORT {collection_url}: HTTP {}: {}",
            response.status, response.body
        ),
    })
}

fn parse_etag_listing(xml: &str, collection_url: &str) -> Result<Vec<ResourceRef>> {
    let doc = Document::parse(xml).map_err(|e| Error::Backend {
        backend: "dav-xml".into(),
        message: e.to_string(),
    })?;
    let mut resources = Vec::new();
    for response in doc
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "response")
    {
        if response
            .descendants()
            .any(|node| node.is_element() && node.tag_name().name() == "collection")
        {
            continue;
        }
        let Some(href) = response
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "href")
            .and_then(|node| node.text())
        else {
            continue;
        };
        let etag = response
            .descendants()
            .find(|node| node.is_element() && node.tag_name().name() == "getetag")
            .and_then(|node| node.text())
            .map(str::to_owned);
        resources.push(ResourceRef {
            href: href.to_owned(),
            url: resolve(collection_url, href)?,
            etag,
        });
    }
    resources.sort_by(|left, right| left.url.cmp(&right.url));
    resources.dedup_by(|left, right| left.url == right.url);
    Ok(resources)
}

fn reconcile_etags(
    current: Vec<ResourceRef>,
    known: &HashMap<String, String>,
) -> (Vec<ResourceRef>, Vec<String>, SyncScope) {
    if known.is_empty() {
        return (current, Vec::new(), SyncScope::Full);
    }
    let current_urls: HashSet<String> = current
        .iter()
        .map(|resource| resource.url.clone())
        .collect();
    let changed = current
        .into_iter()
        .filter(|resource| {
            resource
                .etag
                .as_deref()
                .is_none_or(|etag| known.get(&resource.url).is_none_or(|old| old != etag))
        })
        .collect();
    let mut deleted = known
        .keys()
        .filter(|url| !current_urls.contains(*url))
        .cloned()
        .collect::<Vec<_>>();
    deleted.sort();
    (changed, deleted, SyncScope::Delta)
}

#[derive(Debug)]
struct CollectionSync {
    sync_token: Option<String>,
    scope: SyncScope,
    changed: Vec<ResourceRef>,
    deleted_urls: Vec<String>,
}

async fn etag_fallback(
    client: &Client,
    collection: &DiscoveredCollection,
    cursor: Option<&CollectionCursor>,
    auth: &DavAuth,
) -> Result<CollectionSync> {
    let xml = dav_request(
        client,
        "PROPFIND",
        &collection.url,
        "1",
        ETAG_LIST_BODY,
        auth,
    )
    .await?;
    let resources = parse_etag_listing(&xml, &collection.url)?;
    let known = cursor
        .map(|cursor| &cursor.resource_etags)
        .cloned()
        .unwrap_or_default();
    let (changed, deleted_urls, scope) = reconcile_etags(resources, &known);
    Ok(CollectionSync {
        sync_token: collection.sync_token.clone(),
        scope,
        changed,
        deleted_urls,
    })
}

async fn sync_collection_resources(
    client: &Client,
    collection: &DiscoveredCollection,
    cursor: Option<&CollectionCursor>,
    auth: &DavAuth,
) -> Result<CollectionSync> {
    if let Some(cursor) = cursor {
        let token_unchanged = cursor.sync_token.is_some()
            && cursor.sync_token.as_deref() == collection.sync_token.as_deref();
        let ctag_unchanged =
            collection.ctag.is_some() && collection.ctag.as_deref() == cursor.ctag.as_deref();
        if token_unchanged || ctag_unchanged {
            return Ok(CollectionSync {
                sync_token: collection
                    .sync_token
                    .clone()
                    .or_else(|| cursor.sync_token.clone()),
                scope: SyncScope::Unchanged,
                changed: Vec::new(),
                deleted_urls: Vec::new(),
            });
        }
    }

    if collection.supports_sync_collection {
        if let Some(previous_token) = cursor.and_then(|cursor| cursor.sync_token.as_deref()) {
            match request_sync_collection(client, &collection.url, Some(previous_token), auth)
                .await?
            {
                SyncReportOutcome::Success(delta) => {
                    return Ok(CollectionSync {
                        sync_token: delta.sync_token.or_else(|| collection.sync_token.clone()),
                        scope: SyncScope::Delta,
                        changed: delta.changed,
                        deleted_urls: delta.deleted_urls,
                    });
                }
                SyncReportOutcome::InvalidToken => {
                    // Пустой token даёт согласованный снимок href+ETag и новый
                    // cursor. С локальными ETag это не требует повторной загрузки
                    // неизменившихся calendar-data/address-data.
                }
                SyncReportOutcome::Unsupported => {
                    return etag_fallback(client, collection, cursor, auth).await;
                }
            }
        }

        match request_sync_collection(client, &collection.url, None, auth).await? {
            SyncReportOutcome::Success(snapshot) => {
                let known = cursor
                    .map(|cursor| &cursor.resource_etags)
                    .cloned()
                    .unwrap_or_default();
                let (changed, deleted_urls, scope) = reconcile_etags(snapshot.changed, &known);
                return Ok(CollectionSync {
                    sync_token: snapshot
                        .sync_token
                        .or_else(|| collection.sync_token.clone()),
                    scope,
                    changed,
                    deleted_urls,
                });
            }
            SyncReportOutcome::InvalidToken => {}
            SyncReportOutcome::Unsupported => {}
        }
    }
    etag_fallback(client, collection, cursor, auth).await
}

#[derive(Debug, Clone, Copy)]
enum MultigetKind {
    Calendar,
    AddressBook,
}

fn multiget_body(kind: MultigetKind, resources: &[ResourceRef]) -> String {
    let (prefix, namespace, report, data) = match kind {
        MultigetKind::Calendar => (
            "c",
            "urn:ietf:params:xml:ns:caldav",
            "calendar-multiget",
            "calendar-data",
        ),
        MultigetKind::AddressBook => (
            "a",
            "urn:ietf:params:xml:ns:carddav",
            "addressbook-multiget",
            "address-data",
        ),
    };
    let hrefs = resources
        .iter()
        .map(|resource| format!("<d:href>{}</d:href>", xml_escape(&resource.href)))
        .collect::<String>();
    format!(
        r#"<?xml version="1.0"?><{prefix}:{report} xmlns:d="DAV:" xmlns:{prefix}="{namespace}"><d:prop><d:getetag/><{prefix}:{data}/></d:prop>{hrefs}</{prefix}:{report}>"#
    )
}

async fn multiget_changed(
    client: &Client,
    collection_url: &str,
    resources: &[ResourceRef],
    kind: MultigetKind,
    auth: &DavAuth,
) -> Result<Vec<(String, Option<String>, String)>> {
    let data_tag = match kind {
        MultigetKind::Calendar => "calendar-data",
        MultigetKind::AddressBook => "address-data",
    };
    let mut parts = Vec::new();
    // Ограничиваем размер XML и число возвращаемых тяжёлых тел в одном REPORT.
    for chunk in resources.chunks(100) {
        let body = multiget_body(kind, chunk);
        let xml = dav_request(client, "REPORT", collection_url, "0", &body, auth).await?;
        parts.extend(response_parts(&xml, data_tag)?);
    }
    Ok(parts)
}

fn unfolded(raw: &str) -> String {
    raw.replace("=\r\n", "")
        .replace("\r\n ", "")
        .replace("\r\n\t", "")
}
fn decode_property(key: &str, value: &str) -> String {
    if key
        .split(';')
        .skip(1)
        .any(|part| part.eq_ignore_ascii_case("ENCODING=QUOTED-PRINTABLE"))
    {
        quoted_printable::decode(value, quoted_printable::ParseMode::Robust)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_else(|_| value.to_owned())
    } else {
        value.to_owned()
    }
}
fn prop(raw: &str, name: &str) -> Option<String> {
    unfolded(raw).lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.split(';')
            .next()
            .filter(|key| key.eq_ignore_ascii_case(name))?;
        Some(
            decode_property(key, value)
                .replace("\\n", "\n")
                .replace("\\,", ",")
                .replace("\\;", ";"),
        )
    })
}

fn property_param(key: &str, name: &str) -> Option<String> {
    key.split(';').skip(1).find_map(|part| {
        let (param, value) = part.split_once('=')?;
        param
            .eq_ignore_ascii_case(name)
            .then(|| value.trim_matches('"').to_owned())
    })
}

fn parse_attendees(raw: &str) -> Vec<Attendee> {
    unfolded(raw)
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.split(';')
                .next()
                .is_some_and(|name| name.eq_ignore_ascii_case("ATTENDEE"))
                .then_some(())?;
            let value = value.trim();
            let email = value
                .get(..7)
                .filter(|prefix| prefix.eq_ignore_ascii_case("mailto:"))
                .map(|_| &value[7..])
                .unwrap_or(value)
                .trim()
                .to_owned();
            (!email.is_empty()).then(|| Attendee {
                email,
                name: property_param(key, "CN"),
                role: property_param(key, "ROLE"),
                partstat: property_param(key, "PARTSTAT"),
                rsvp: property_param(key, "RSVP")
                    .is_some_and(|value| value.eq_ignore_ascii_case("TRUE")),
            })
        })
        .collect()
}

fn parse_duration_minutes(value: &str) -> Option<i32> {
    let value = value.trim();
    let before_start = value.starts_with('-');
    let value = value.trim_start_matches(['-', '+']);
    let chars = value.strip_prefix('P')?.chars();
    let mut in_time = false;
    let mut number = String::new();
    let mut total_seconds: i64 = 0;
    for ch in chars {
        if ch == 'T' {
            in_time = true;
            continue;
        }
        if ch.is_ascii_digit() {
            number.push(ch);
            continue;
        }
        let amount: i64 = number.parse().ok()?;
        number.clear();
        total_seconds += match (ch, in_time) {
            ('W', false) => amount * 7 * 24 * 60 * 60,
            ('D', false) => amount * 24 * 60 * 60,
            ('H', true) => amount * 60 * 60,
            ('M', true) => amount * 60,
            ('S', true) => amount,
            _ => return None,
        };
    }
    if !number.is_empty() {
        return None;
    }
    let minutes = i32::try_from((total_seconds + 59) / 60).ok()?;
    Some(if before_start { minutes } else { -minutes })
}

fn parse_alarms(raw: &str) -> Vec<Alarm> {
    let mut alarms = Vec::new();
    let mut rest = raw;
    while let Some(start) = rest.find("BEGIN:VALARM") {
        rest = &rest[start..];
        let Some(relative_end) = rest.find("END:VALARM") else {
            break;
        };
        let block = &rest[..relative_end + "END:VALARM".len()];
        if let Some(trigger_minutes) = prop(block, "TRIGGER").and_then(|value| {
            // Absolute RFC5545 triggers are retained in raw data but cannot be
            // represented by the current minute-offset model.
            parse_duration_minutes(&value)
        }) {
            alarms.push(Alarm {
                trigger_minutes,
                action: prop(block, "ACTION").unwrap_or_else(|| "DISPLAY".into()),
            });
        }
        rest = &rest[relative_end + "END:VALARM".len()..];
    }
    alarms
}

fn parse_events(raw: String, etag: Option<String>, remote_url: Option<String>) -> Vec<DavEvent> {
    let mut events = Vec::new();
    let mut rest = raw.as_str();
    while let Some(relative_start) = rest.find("BEGIN:VEVENT") {
        rest = &rest[relative_start..];
        let Some(relative_end) = rest.find("END:VEVENT") else {
            break;
        };
        let end = relative_end + "END:VEVENT".len();
        let event = rest[..end].to_owned();
        if let (Some(uid), Some(dtstart)) = (prop(&event, "UID"), prop(&event, "DTSTART")) {
            events.push(DavEvent {
                remote_url: remote_url.clone(),
                uid,
                summary: prop(&event, "SUMMARY").unwrap_or_default(),
                description: prop(&event, "DESCRIPTION"),
                location: prop(&event, "LOCATION"),
                dtstart,
                dtend: prop(&event, "DTEND"),
                rrule: prop(&event, "RRULE"),
                recurrence_id: prop(&event, "RECURRENCE-ID"),
                exdates: prop(&event, "EXDATE"),
                rdates: prop(&event, "RDATE"),
                status: prop(&event, "STATUS"),
                attendees: parse_attendees(&event),
                alarms: parse_alarms(&event),
                timezone: prop(&event, "X-WR-TIMEZONE").or_else(|| {
                    unfolded(&event).lines().find_map(|line| {
                        let (key, _) = line.split_once(':')?;
                        key.split(';')
                            .next()
                            .is_some_and(|name| name.eq_ignore_ascii_case("DTSTART"))
                            .then(|| property_param(key, "TZID"))
                            .flatten()
                    })
                }),
                transp: prop(&event, "TRANSP"),
                class: prop(&event, "CLASS"),
                categories: prop(&event, "CATEGORIES")
                    .unwrap_or_default()
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
                    .collect(),
                url: prop(&event, "URL"),
                organizer: prop(&event, "ORGANIZER").map(|value| {
                    value
                        .strip_prefix("mailto:")
                        .or_else(|| value.strip_prefix("MAILTO:"))
                        .unwrap_or(&value)
                        .to_owned()
                }),
                sequence: prop(&event, "SEQUENCE")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or_default(),
                raw: event,
                etag: etag.clone(),
            });
        }
        rest = &rest[end..];
    }
    events
}

fn parse_contact(
    raw: String,
    etag: Option<String>,
    remote_url: Option<String>,
) -> Option<DavContact> {
    let name = prop(&raw, "N").unwrap_or_default();
    let mut names = name.split(';');
    let last_name = names.next().filter(|v| !v.is_empty()).map(str::to_owned);
    let first_name = names.next().filter(|v| !v.is_empty()).map(str::to_owned);
    let emails = unfolded(&raw)
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.split(';')
                .next()
                .filter(|key| key.eq_ignore_ascii_case("EMAIL"))?;
            Some(decode_property(key, value))
        })
        .collect();
    let phones = unfolded(&raw)
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.split(';')
                .next()
                .filter(|key| key.eq_ignore_ascii_case("TEL"))?;
            let kind = key
                .split(';')
                .skip(1)
                .flat_map(|part| {
                    part.split_once('=')
                        .filter(|(name, _)| name.eq_ignore_ascii_case("TYPE"))
                        .map(|(_, value)| value)
                        .unwrap_or(part)
                        .split(',')
                })
                .map(str::to_lowercase)
                .find(|value| matches!(value.as_str(), "cell" | "mobile" | "work" | "home" | "fax"))
                .map(|value| {
                    if value == "cell" {
                        "mobile".to_owned()
                    } else {
                        value
                    }
                });
            Some(ContactPhone::from_remote(
                &decode_property(key, value),
                kind,
            ))
        })
        .filter(|phone| !phone.number.is_empty())
        .collect();
    let addresses = unfolded(&raw)
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.split(';')
                .next()
                .filter(|key| key.eq_ignore_ascii_case("ADR"))?;
            Some(parse_adr(key, &decode_property(key, value)))
        })
        .filter(|address| !address.is_empty())
        .collect();
    Some(DavContact {
        remote_url,
        uid: prop(&raw, "UID")?,
        display_name: clean_contact_name(
            &prop(&raw, "FN").unwrap_or_else(|| name.replace(';', " ").trim().to_owned()),
        ),
        first_name,
        last_name,
        organization: prop(&raw, "ORG"),
        emails,
        phones,
        addresses,
        raw,
        etag,
    })
}

/// Разбить значение vCard на компоненты по неэкранированным точкам с запятой и
/// снять экранирование внутри каждой из них. Обычный split(';') здесь не
/// годится: "ул. Ленина\; дом 1" - одна компонента, а не две.
fn split_vcard_components(value: &str) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut escaped = false;
    for ch in value.chars() {
        let current = parts.last_mut().expect("хотя бы одна компонента есть");
        if escaped {
            escaped = false;
            match ch {
                'n' | 'N' => current.push('\n'),
                other => current.push(other),
            }
            continue;
        }
        match ch {
            '\\' => escaped = true,
            ';' => parts.push(String::new()),
            other => current.push(other),
        }
    }
    parts
}

/// ADR из RFC 6350: pobox;ext;street;city;region;postal;country. Недостающие
/// компоненты (серверы часто обрывают значение на середине) считаем пустыми.
fn parse_adr(key: &str, value: &str) -> ContactAddress {
    let parts = split_vcard_components(value);
    let part = |index: usize| {
        parts
            .get(index)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    };
    let kind = key
        .split(';')
        .skip(1)
        .flat_map(|part| {
            part.split_once('=')
                .filter(|(name, _)| name.eq_ignore_ascii_case("TYPE"))
                .map(|(_, value)| value)
                .unwrap_or(part)
                .split(',')
        })
        .map(str::to_lowercase)
        .find(|value| matches!(value.as_str(), "home" | "work" | "other"));
    ContactAddress {
        kind,
        street: part(2),
        city: part(3),
        region: part(4),
        postal_code: part(5),
        country: part(6),
    }
}

#[cfg(test)]
fn collection_unchanged(
    cursors: &HashMap<String, CollectionCursor>,
    url: &str,
    ctag: Option<&str>,
) -> bool {
    ctag.is_some() && cursors.get(url).and_then(|cursor| cursor.ctag.as_deref()) == ctag
}

#[cfg(test)]
fn collections_unchanged(
    cursors: &HashMap<String, CollectionCursor>,
    collections: &[DavCollection],
) -> bool {
    collections.len() == cursors.len()
        && collections.iter().all(|collection| {
            collection_unchanged(cursors, &collection.url, collection.ctag.as_deref())
        })
}

async fn sync_calendars(
    client: &Client,
    email: &str,
    auth: &DavAuth,
    cal_base: &str,
    cursors: &AuxiliarySyncCursors,
) -> Result<Vec<DavCalendar>> {
    let cal_home = discover_home(client, cal_base, "calendar-home-set", auth).await?;
    let cal_xml = dav_request(client, "PROPFIND", &cal_home, "1", COLLECTIONS_BODY, auth).await?;
    let discovered_calendars = parse_collections(&cal_xml, cal_base, "calendar", "Календарь")?;
    let mut calendars = Vec::new();
    for collection in discovered_calendars {
        let collection_started = std::time::Instant::now();
        let cursor = cursors.calendars.get(&collection.url);
        let sync = sync_collection_resources(client, &collection, cursor, auth).await?;
        let events: Vec<_> = multiget_changed(
            client,
            &collection.url,
            &sync.changed,
            MultigetKind::Calendar,
            auth,
        )
        .await?
        .into_iter()
        .flat_map(|(href, etag, raw)| {
            let remote_url = resolve(&collection.url, &href).ok();
            parse_events(raw, etag, remote_url)
        })
        .collect();
        let changed = events.len();
        let deleted = sync.deleted_urls.len();
        let unchanged = changed == 0 && deleted == 0;
        if unchanged {
            tracing::debug!(
                provider = "caldav",
                account = %crate::logging::mask_email(email),
                collection = %collection.url,
                scope = ?sync.scope,
                changed,
                deleted,
                network_ms = collection_started.elapsed().as_millis() as u64,
                "DAV collection delta fetched"
            );
        } else {
            tracing::info!(
                provider = "caldav",
                account = %crate::logging::mask_email(email),
                collection = %collection.url,
                scope = ?sync.scope,
                changed,
                deleted,
                network_ms = collection_started.elapsed().as_millis() as u64,
                "DAV collection delta fetched"
            );
        }
        calendars.push(DavCalendar {
            url: collection.url,
            name: collection.name,
            ctag: collection.ctag,
            sync_token: sync.sync_token,
            sync_scope: sync.scope,
            deleted_event_urls: sync.deleted_urls,
            events,
        });
    }
    Ok(calendars)
}

/// Промежуточный результат sync_contacts - отдельная структура, а не
/// напрямую DavSyncResult, потому что в нём нет полей календаря.
struct ContactsSyncOutcome {
    contacts: Vec<DavContact>,
    contact_collections: Vec<DavCollection>,
    contacts_available: bool,
    contacts_scope: SyncScope,
    deleted_contact_urls: Vec<String>,
}

async fn sync_contacts(
    client: &Client,
    email: &str,
    auth: &DavAuth,
    card_base: &str,
    cursors: &AuxiliarySyncCursors,
) -> Result<ContactsSyncOutcome> {
    let card_home = discover_home(client, card_base, "addressbook-home-set", auth).await?;
    let Some(card_xml) =
        dav_request_optional(client, "PROPFIND", &card_home, "1", COLLECTIONS_BODY, auth).await?
    else {
        // Часть серверов (например, Яндекс) создаёт CardDAV-книгу лениво -
        // 404 здесь означает "адресной книги ещё нет", а не ошибку.
        return Ok(ContactsSyncOutcome {
            contacts: Vec::new(),
            contact_collections: Vec::new(),
            contacts_available: false,
            contacts_scope: SyncScope::Unchanged,
            deleted_contact_urls: Vec::new(),
        });
    };
    let mut discovered_addressbooks =
        parse_collections(&card_xml, card_base, "addressbook", "Контакты")?;
    if discovered_addressbooks.is_empty() {
        discovered_addressbooks.push(DiscoveredCollection {
            url: card_home,
            name: "Контакты".into(),
            ctag: None,
            sync_token: None,
            supports_sync_collection: false,
        });
    }
    let mut contacts = Vec::new();
    let mut addressbooks = Vec::new();
    let mut collection_scopes = Vec::new();
    let mut deleted_contact_urls = Vec::new();
    let current_collection_urls: HashSet<String> = discovered_addressbooks
        .iter()
        .map(|collection| collection.url.clone())
        .collect();
    for (old_url, cursor) in &cursors.contact_collections {
        if !current_collection_urls.contains(old_url.as_str()) {
            deleted_contact_urls.extend(cursor.resource_etags.keys().cloned());
        }
    }
    for collection in discovered_addressbooks {
        let collection_started = std::time::Instant::now();
        let cursor = cursors.contact_collections.get(&collection.url);
        let sync = sync_collection_resources(client, &collection, cursor, auth).await?;
        collection_scopes.push(sync.scope);
        let deleted_count = sync.deleted_urls.len();
        deleted_contact_urls.extend(sync.deleted_urls);
        let changed_contacts: Vec<_> = multiget_changed(
            client,
            &collection.url,
            &sync.changed,
            MultigetKind::AddressBook,
            auth,
        )
        .await?
        .into_iter()
        .filter_map(|(href, etag, raw)| {
            let remote_url = resolve(&collection.url, &href).ok();
            parse_contact(raw, etag, remote_url)
        })
        .collect();
        let changed = changed_contacts.len();
        let deleted = deleted_count;
        let unchanged = changed == 0 && deleted == 0;
        if unchanged {
            tracing::debug!(
                provider = "carddav",
                account = %crate::logging::mask_email(email),
                collection = %collection.url,
                scope = ?sync.scope,
                changed,
                deleted,
                network_ms = collection_started.elapsed().as_millis() as u64,
                "DAV collection delta fetched"
            );
        } else {
            tracing::info!(
                provider = "carddav",
                account = %crate::logging::mask_email(email),
                collection = %collection.url,
                scope = ?sync.scope,
                changed,
                deleted,
                network_ms = collection_started.elapsed().as_millis() as u64,
                "DAV collection delta fetched"
            );
        }
        contacts.extend(changed_contacts);
        addressbooks.push(DavCollection {
            url: collection.url,
            ctag: collection.ctag,
            sync_token: sync.sync_token,
        });
    }
    deleted_contact_urls.sort();
    deleted_contact_urls.dedup();
    let contacts_scope = if collection_scopes
        .iter()
        .all(|scope| *scope == SyncScope::Full)
    {
        SyncScope::Full
    } else if deleted_contact_urls.is_empty()
        && collection_scopes
            .iter()
            .all(|scope| *scope == SyncScope::Unchanged)
    {
        SyncScope::Unchanged
    } else {
        // Full для одной книги нельзя поднимать до общего Full: иначе storage
        // удалит контакты из другой, не изменившейся CardDAV-коллекции.
        SyncScope::Delta
    };
    Ok(ContactsSyncOutcome {
        contacts,
        contact_collections: addressbooks,
        contacts_available: true,
        contacts_scope,
        deleted_contact_urls,
    })
}

/// Полная и инкрементальная синхронизация календарей и контактов по
/// CalDAV/CardDAV для любого сервера, реализующего эти RFC (раньше эта
/// функция называлась sync_yandex_dav и была жёстко привязана к адресам
/// Яндекса). caldav_base/carddav_base опциональны: не у каждого сервера
/// есть оба протокола (например, CardDAV может не поддерживаться), и в этом
/// случае соответствующая часть просто не синхронизируется
/// (calendars_available/contacts_available = false), а не считается
/// ошибкой. Если не задан и не обнаружен ни один из адресов - синхронизация
/// невозможна в принципе, это единственный настоящий сбой.
pub async fn sync_dav_account(
    email: &str,
    auth: &DavAuth,
    caldav_base: Option<&str>,
    carddav_base: Option<&str>,
    cursors: &AuxiliarySyncCursors,
) -> Result<DavSyncResult> {
    if caldav_base.is_none() && carddav_base.is_none() {
        return Err(Error::AccountConfig(
            "календарь для этого аккаунта не найден: адрес CalDAV/CardDAV не задан и не обнаружен автоматически".into(),
        ));
    }
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .map_err(|e| Error::Backend {
            backend: "dav".into(),
            message: e.to_string(),
        })?;

    let calendars = match caldav_base {
        Some(cal_base) => sync_calendars(&client, email, auth, cal_base, cursors).await?,
        None => Vec::new(),
    };

    let contacts = match carddav_base {
        Some(card_base) => sync_contacts(&client, email, auth, card_base, cursors).await?,
        None => ContactsSyncOutcome {
            contacts: Vec::new(),
            contact_collections: Vec::new(),
            contacts_available: false,
            contacts_scope: SyncScope::Unchanged,
            deleted_contact_urls: Vec::new(),
        },
    };

    Ok(DavSyncResult {
        calendars,
        calendars_available: caldav_base.is_some(),
        contacts: contacts.contacts,
        contact_collections: contacts.contact_collections,
        contacts_available: contacts.contacts_available,
        contacts_scope: contacts.contacts_scope,
        contacts_sync_token: None,
        deleted_contact_urls: contacts.deleted_contact_urls,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, response::Redirect, routing::get};

    #[test]
    fn parses_every_vevent_and_recurrence_override() {
        let raw = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:a\r\nDTSTART;TZID=Europe/Moscow:20260714T100000\r\nSUMMARY:Base\r\nRRULE:FREQ=DAILY\r\nEXDATE:20260715T100000Z\r\nTRANSP:TRANSPARENT\r\nCLASS:PRIVATE\r\nCATEGORIES:Team,Demo\r\nURL:https://example.test/meeting\r\nORGANIZER:mailto:owner@example.test\r\nSEQUENCE:4\r\nATTENDEE;CN=Guest;ROLE=REQ-PARTICIPANT;PARTSTAT=ACCEPTED;RSVP=FALSE:mailto:guest@example.test\r\nBEGIN:VALARM\r\nTRIGGER:-PT15M\r\nACTION:DISPLAY\r\nEND:VALARM\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:a\r\nRECURRENCE-ID:20260716T100000Z\r\nDTSTART:20260716T120000Z\r\nSUMMARY:Moved\r\nEND:VEVENT\r\nEND:VCALENDAR";
        let events = parse_events(raw.to_owned(), Some("etag".to_owned()), None);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].exdates.as_deref(), Some("20260715T100000Z"));
        assert_eq!(events[0].attendees[0].email, "guest@example.test");
        assert_eq!(events[0].attendees[0].partstat.as_deref(), Some("ACCEPTED"));
        assert_eq!(events[0].alarms[0].trigger_minutes, 15);
        assert_eq!(events[0].timezone.as_deref(), Some("Europe/Moscow"));
        assert_eq!(events[0].transp.as_deref(), Some("TRANSPARENT"));
        assert_eq!(events[0].class.as_deref(), Some("PRIVATE"));
        assert_eq!(events[0].categories, ["Team", "Demo"]);
        assert_eq!(
            events[0].url.as_deref(),
            Some("https://example.test/meeting")
        );
        assert_eq!(events[0].organizer.as_deref(), Some("owner@example.test"));
        assert_eq!(events[0].sequence, 4);
        assert_eq!(events[1].recurrence_id.as_deref(), Some("20260716T100000Z"));
    }

    #[test]
    fn decodes_vcard_quoted_printable_properties() {
        let raw = "BEGIN:VCARD\r\nVERSION:2.1\r\nUID:1\r\nFN;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:=D0=98=D0=B2=D0=B0=D0=BD\r\nEMAIL:test@example.com\r\nTEL;TYPE=CELL:+79990000000;ext=123\r\nEND:VCARD";
        let contact = parse_contact(raw.to_owned(), None, None).expect("valid contact");
        assert_eq!(contact.display_name, "Иван");
        assert_eq!(contact.emails, ["test@example.com"]);
        assert_eq!(contact.phones[0].number, "+79990000000");
        assert_eq!(contact.phones[0].kind.as_deref(), Some("mobile"));
        assert_eq!(contact.phones[0].extension.as_deref(), Some("123"));
    }

    #[test]
    fn parses_vcard_addresses_with_escaping_and_missing_components() {
        let raw = "BEGIN:VCARD\r\nVERSION:3.0\r\nUID:1\r\nFN:Иван\r\n\
             ADR;TYPE=HOME:;;ул. Ленина\\, 1\\; корп. 2;Москва;;101000;Россия\r\n\
             ADR;TYPE=WORK:;;;Казань\r\n\
             ADR:;;;;;;\r\n\
             END:VCARD";
        let contact = parse_contact(raw.to_owned(), None, None).expect("valid contact");
        // Полностью пустой ADR в модель не попадает.
        assert_eq!(contact.addresses.len(), 2);
        let home = &contact.addresses[0];
        assert_eq!(home.kind.as_deref(), Some("home"));
        assert_eq!(home.street.as_deref(), Some("ул. Ленина, 1; корп. 2"));
        assert_eq!(home.city.as_deref(), Some("Москва"));
        assert_eq!(home.region, None);
        assert_eq!(home.postal_code.as_deref(), Some("101000"));
        assert_eq!(home.country.as_deref(), Some("Россия"));
        // Оборванное значение: недостающие компоненты пусты, а не ошибка.
        let work = &contact.addresses[1];
        assert_eq!(work.kind.as_deref(), Some("work"));
        assert_eq!(work.street, None);
        assert_eq!(work.city.as_deref(), Some("Казань"));
        assert_eq!(work.country, None);
    }

    #[test]
    fn splits_vcard_components_only_on_unescaped_separators() {
        assert_eq!(
            split_vcard_components("a\\;b;c\\,d;\\\\e"),
            ["a;b", "c,d", "\\e"]
        );
        assert_eq!(split_vcard_components(";;"), ["", "", ""]);
    }

    #[test]
    fn skips_only_collections_with_a_matching_ctag() {
        let cursors = HashMap::from([(
            "https://dav.test/calendar/".into(),
            CollectionCursor {
                ctag: Some("42".into()),
                sync_token: None,
                resource_etags: HashMap::new(),
            },
        )]);
        assert!(collection_unchanged(
            &cursors,
            "https://dav.test/calendar/",
            Some("42")
        ));
        assert!(!collection_unchanged(
            &cursors,
            "https://dav.test/calendar/",
            Some("43")
        ));
        assert!(!collection_unchanged(
            &cursors,
            "https://dav.test/calendar/",
            None
        ));
        assert!(!collections_unchanged(
            &cursors,
            &[
                DavCollection {
                    url: "https://dav.test/calendar/".into(),
                    ctag: Some("42".into()),
                    sync_token: None,
                },
                DavCollection {
                    url: "https://dav.test/new/".into(),
                    ctag: Some("1".into()),
                    sync_token: None,
                },
            ]
        ));
    }

    #[test]
    fn discovers_sync_collection_capability_and_opaque_token() {
        let xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav" xmlns:cs="http://calendarserver.org/ns/">
  <d:response>
    <d:href>/users/me/calendar/</d:href>
    <d:propstat><d:prop>
      <d:displayname>Работа</d:displayname>
      <d:resourcetype><d:collection/><c:calendar/></d:resourcetype>
      <cs:getctag>ctag-2</cs:getctag>
      <d:sync-token>https://dav.test/token/opaque-2</d:sync-token>
      <d:supported-report-set><d:supported-report><d:report><d:sync-collection/></d:report></d:supported-report></d:supported-report-set>
    </d:prop></d:propstat>
  </d:response>
</d:multistatus>"#;
        let collections =
            parse_collections(xml, "https://dav.test/", "calendar", "Календарь").unwrap();
        assert_eq!(collections.len(), 1);
        assert_eq!(collections[0].url, "https://dav.test/users/me/calendar/");
        assert_eq!(collections[0].name, "Работа");
        assert_eq!(collections[0].ctag.as_deref(), Some("ctag-2"));
        assert_eq!(
            collections[0].sync_token.as_deref(),
            Some("https://dav.test/token/opaque-2")
        );
        assert!(collections[0].supports_sync_collection);
    }

    #[test]
    fn parses_rfc6578_changed_and_deleted_members() {
        let xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:">
  <d:response>
    <d:href>/cal/changed.ics</d:href>
    <d:propstat><d:prop><d:getetag>&quot;etag-2&quot;</d:getetag></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>
  </d:response>
  <d:response><d:href>/cal/deleted.ics</d:href><d:status>HTTP/1.1 404 Not Found</d:status></d:response>
  <d:sync-token>urn:sync:next</d:sync-token>
</d:multistatus>"#;
        let delta = parse_sync_collection(xml, "https://dav.test/cal/").unwrap();
        assert_eq!(delta.sync_token.as_deref(), Some("urn:sync:next"));
        assert_eq!(delta.changed.len(), 1);
        assert_eq!(delta.changed[0].url, "https://dav.test/cal/changed.ics");
        assert_eq!(delta.changed[0].etag.as_deref(), Some("\"etag-2\""));
        assert_eq!(delta.deleted_urls, ["https://dav.test/cal/deleted.ics"]);
    }

    #[test]
    fn etag_fallback_fetches_only_new_or_changed_and_finds_deletions() {
        let known = HashMap::from([
            ("https://dav.test/cal/same.ics".into(), "same".into()),
            ("https://dav.test/cal/changed.ics".into(), "old".into()),
            ("https://dav.test/cal/deleted.ics".into(), "gone".into()),
        ]);
        let current = vec![
            ResourceRef {
                href: "/cal/same.ics".into(),
                url: "https://dav.test/cal/same.ics".into(),
                etag: Some("same".into()),
            },
            ResourceRef {
                href: "/cal/changed.ics".into(),
                url: "https://dav.test/cal/changed.ics".into(),
                etag: Some("new".into()),
            },
            ResourceRef {
                href: "/cal/created.ics".into(),
                url: "https://dav.test/cal/created.ics".into(),
                etag: Some("created".into()),
            },
        ];
        let (changed, deleted, scope) = reconcile_etags(current, &known);
        assert_eq!(scope, SyncScope::Delta);
        assert_eq!(
            changed
                .iter()
                .map(|resource| resource.url.as_str())
                .collect::<Vec<_>>(),
            [
                "https://dav.test/cal/changed.ics",
                "https://dav.test/cal/created.ics"
            ]
        );
        assert_eq!(deleted, ["https://dav.test/cal/deleted.ics"]);

        let body = multiget_body(MultigetKind::Calendar, &changed);
        assert!(body.contains("/cal/changed.ics"));
        assert!(body.contains("/cal/created.ics"));
        assert!(!body.contains("/cal/same.ics"));
        assert!(body.contains("calendar-multiget"));
    }

    #[test]
    fn empty_etag_snapshot_requires_collection_full_sync() {
        let current = vec![ResourceRef {
            href: "/book/one.vcf".into(),
            url: "https://dav.test/book/one.vcf".into(),
            etag: Some("one".into()),
        }];
        let (changed, deleted, scope) = reconcile_etags(current, &HashMap::new());
        assert_eq!(scope, SyncScope::Full);
        assert_eq!(changed.len(), 1);
        assert!(deleted.is_empty());
        let body = multiget_body(MultigetKind::AddressBook, &changed);
        assert!(body.contains("addressbook-multiget"));
        assert!(body.contains("address-data"));
    }

    #[test]
    fn recognizes_rfc6578_invalid_token_precondition() {
        let xml = r#"<d:error xmlns:d="DAV:"><d:valid-sync-token/></d:error>"#;
        assert!(response_has_element(xml, "valid-sync-token"));
        assert!(!response_has_element(xml, "supported-report"));
    }

    #[test]
    fn picks_dav_auth_scheme_by_provider_and_auth_kind() {
        // Яндекс - особый случай независимо от auth_kind: токен всегда через
        // Basic, никогда через Bearer, т.к. его DAV-серверы Bearer не понимают.
        assert_eq!(
            dav_auth_scheme(Provider::Yandex, AuthKind::Oauth2),
            DavAuthScheme::BasicToken
        );
        // Остальные OAuth2-провайдеры (например, Outlook) - стандартный Bearer.
        assert_eq!(
            dav_auth_scheme(Provider::Outlook, AuthKind::Oauth2),
            DavAuthScheme::Bearer
        );
        // Password/AppPassword у любого не-Яндекс провайдера - обычный Basic
        // с логином и секретом из keychain (iCloud, Mail.ru, generic-серверы).
        assert_eq!(
            dav_auth_scheme(Provider::Icloud, AuthKind::AppPassword),
            DavAuthScheme::BasicPassword
        );
        assert_eq!(
            dav_auth_scheme(Provider::Mailru, AuthKind::AppPassword),
            DavAuthScheme::BasicPassword
        );
        assert_eq!(
            dav_auth_scheme(Provider::Generic, AuthKind::Password),
            DavAuthScheme::BasicPassword
        );
    }

    #[test]
    fn yandex_keeps_default_bases_when_account_has_none_configured() {
        // Без обнаружения и без ручной настройки - те же адреса, что были
        // жёстко зашиты раньше. Обобщение DAV на остальных провайдеров не
        // должно ничего сломать для Яндекса.
        let (cal, card) = resolve_yandex_bases(None, None);
        assert_eq!(cal, YANDEX_CALDAV_BASE);
        assert_eq!(card, YANDEX_CARDDAV_BASE);

        // Если на аккаунте уже сохранён свой адрес (например, задан вручную) -
        // используется именно он, дефолт не перетирает явную настройку.
        let (cal, card) = resolve_yandex_bases(Some("https://custom.test/dav/"), None);
        assert_eq!(cal, "https://custom.test/dav/");
        assert_eq!(card, YANDEX_CARDDAV_BASE);
    }

    #[tokio::test]
    async fn sync_dav_account_without_any_base_fails_clearly_instead_of_panicking() {
        let auth = DavAuth::new(DavAuthScheme::BasicPassword, "user@example.test", "secret");
        let error = sync_dav_account(
            "user@example.test",
            &auth,
            None,
            None,
            &AuxiliarySyncCursors::default(),
        )
        .await
        .expect_err("both bases missing must be a clear config error, not a panic or silent no-op");
        assert!(
            matches!(&error, Error::AccountConfig(message) if message.contains("не найден")),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn reads_context_path_from_srv_txt_record() {
        // Каноническая форма из RFC 6764, раздел 6.
        assert_eq!(
            parse_srv_txt_path(&["path=/dav/".to_owned()]).as_deref(),
            Some("/dav/")
        );
        // Несколько key=value в одной записи: берём только path, остальное
        // (RFC 6763 разрешает произвольные ключи) игнорируем.
        assert_eq!(
            parse_srv_txt_path(&["ttl=3600 path=/caldav/user/".to_owned()]).as_deref(),
            Some("/caldav/user/")
        );
        // TXT приходит несколькими character-string - path может быть в любой.
        assert_eq!(
            parse_srv_txt_path(&["v=1".to_owned(), "PATH=dav".to_owned()]).as_deref(),
            Some("/dav")
        );
        // Ни пустое значение, ни отсутствие ключа не должны давать путь -
        // иначе получился бы URL вида "https://host" без пути контекста,
        // который сервер не обязан обслуживать.
        assert!(parse_srv_txt_path(&["path=".to_owned()]).is_none());
        assert!(parse_srv_txt_path(&["v=spf1 -all".to_owned()]).is_none());
        assert!(parse_srv_txt_path(&[]).is_none());
    }

    #[test]
    fn picks_lowest_priority_then_highest_weight_srv_record() {
        let record = |host: &str, port, priority, weight| SrvTarget {
            host: host.to_owned(),
            port,
            priority,
            weight,
        };
        let records = [
            record("backup.dav.test", 443, 20, 100),
            record("light.dav.test", 443, 10, 1),
            record("main.dav.test", 8443, 10, 50),
        ];
        let picked = pick_srv_target(&records).expect("есть пригодная запись");
        assert_eq!(picked.host, "main.dav.test");
        assert_eq!(picked.port, 8443);

        // RFC 2782: target "." означает, что сервис на домене не
        // предоставляется; нулевой порт бессмыслен. Обе записи должны быть
        // отброшены, а не превращены в неработающий URL.
        assert!(pick_srv_target(&[record(".", 443, 0, 0)]).is_none());
        assert!(pick_srv_target(&[record("dav.test", 0, 0, 0)]).is_none());
        assert!(pick_srv_target(&[]).is_none());

        // При полном равенстве priority и weight выбор обязан быть
        // детерминированным: найденный адрес сохраняется на аккаунте, и
        // прыгающий между синхронизациями хост означал бы бессмысленную
        // перезапись настроек.
        let tie = [
            record("b.dav.test", 443, 0, 0),
            record("a.dav.test", 443, 0, 0),
        ];
        assert_eq!(
            pick_srv_target(&tie).map(|target| target.host),
            Some("a.dav.test".to_owned())
        );
    }

    #[test]
    fn builds_https_base_url_omitting_the_implied_port_443() {
        assert_eq!(
            srv_base_url("dav.example.test", 443, "/dav/"),
            "https://dav.example.test/dav/"
        );
        assert_eq!(
            srv_base_url("dav.example.test", 8443, "/dav/"),
            "https://dav.example.test:8443/dav/"
        );
        // Origin без пути нужен для ветки "TXT нет, спрашиваем .well-known
        // у найденного хоста" - там тоже нельзя приписывать :443.
        assert_eq!(
            srv_origin("dav.example.test", 443),
            "https://dav.example.test"
        );
        assert_eq!(
            srv_origin("dav.example.test", 8443),
            "https://dav.example.test:8443"
        );
    }

    #[test]
    fn maps_only_tls_srv_services_to_well_known_paths() {
        assert_eq!(well_known_for_service(SRV_CALDAVS), Some(WELL_KNOWN_CALDAV));
        assert_eq!(
            well_known_for_service(SRV_CARDDAVS),
            Some(WELL_KNOWN_CARDDAV)
        );
        // Нешифрованные варианты не поддерживаются осознанно: по http ушли бы
        // Authorization с паролем или OAuth-токеном.
        assert!(well_known_for_service("_caldav._tcp").is_none());
    }

    #[tokio::test]
    async fn srv_discovery_rejects_useless_input_before_touching_dns() {
        // Пустой домен и неподдерживаемый сервис отсекаются до создания
        // резолвера, поэтому тест не ходит в сеть. Результат - None, а не
        // ошибка: обнаружение через SRV опционально и обязано передавать ход
        // дальше по цепочке источников.
        assert!(discover_srv("", SRV_CALDAVS).await.is_none());
        assert!(discover_srv("   ", SRV_CALDAVS).await.is_none());
        assert!(discover_srv("example.test", "_caldav._tcp").await.is_none());
    }

    #[test]
    fn srv_target_outside_the_mail_domain_is_not_trusted() {
        assert!(srv_target_is_trusted("example.test", "example.test"));
        assert!(srv_target_is_trusted("dav.example.test.", "example.test"));
        assert!(srv_target_is_trusted("DAV.Example.Test", "example.test"));
        // Подделанная запись уводит на чужой сервер, куда ушёл бы пароль.
        assert!(!srv_target_is_trusted("evil.test", "example.test"));
        // Классическая уловка: чужой домен, оканчивающийся на наш как на суффикс
        // без разделяющей точки.
        assert!(!srv_target_is_trusted("notexample.test", "example.test"));
        assert!(!srv_target_is_trusted("", "example.test"));
        assert!(!srv_target_is_trusted("example.test", ""));
    }

    async fn well_known_redirect() -> Redirect {
        Redirect::permanent("/dav/principal/")
    }

    #[tokio::test]
    async fn resolves_well_known_redirect_to_the_real_dav_base() {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new().route(WELL_KNOWN_CALDAV, get(well_known_redirect));
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let discovered = discover_well_known(&origin, WELL_KNOWN_CALDAV).await;

        server.abort();
        assert_eq!(
            discovered.as_deref(),
            Some(format!("{origin}/dav/principal/").as_str())
        );
    }

    #[tokio::test]
    async fn well_known_without_redirect_or_success_yields_no_discovery() {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        // Ни одного маршрута не зарегистрировано - сервер отвечает 404, что не
        // является ни редиректом, ни успехом; discover_well_known должен
        // вернуть None, а не запаниковать или зациклиться.
        let app = Router::new();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let discovered = discover_well_known(&origin, WELL_KNOWN_CALDAV).await;

        server.abort();
        assert!(discovered.is_none());
    }

    #[tokio::test]
    async fn well_known_answering_unauthorized_still_counts_as_discovery() {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        // Так отвечают iCloud и Fastmail на GET без авторизации. Ресурс есть,
        // просто он закрыт - discovery обязано его принять, иначе подключение
        // к этим провайдерам не состоится вовсе.
        let app = Router::new().route(
            WELL_KNOWN_CALDAV,
            get(|| async { axum::http::StatusCode::UNAUTHORIZED }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let discovered = discover_well_known(&origin, WELL_KNOWN_CALDAV).await;

        server.abort();
        assert_eq!(
            discovered.as_deref(),
            Some(format!("{origin}{WELL_KNOWN_CALDAV}").as_str())
        );
    }
}
