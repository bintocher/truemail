//! Менеджер аккаунтов и автоконфигурация провайдеров.

mod autoconfig;
mod auxiliary;
mod dav;
mod google_services;
mod oauth;
pub use autoconfig::{ProviderConfig, autoconfig, discover_provider};
pub use auxiliary::{
    ContactInput, EventInput, RemoteObject, delete_contact, delete_event, write_contact,
    write_event,
};
pub use dav::{
    AuxiliarySyncCursors, CollectionCursor, DavAuth, DavAuthScheme, DavCalendar, DavCollection,
    DavContact, DavEvent, DavSyncResult, SRV_CALDAVS, SRV_CARDDAVS, SyncScope, WELL_KNOWN_CALDAV,
    WELL_KNOWN_CARDDAV, YANDEX_CALDAV_BASE, YANDEX_CARDDAV_BASE, dav_auth_scheme, discover_srv,
    discover_well_known, resolve_yandex_bases, sync_dav_account, validate_dav,
};
pub use google_services::sync_google_services;
pub use oauth::{
    GOOGLE_SCOPES, MICROSOFT_SCOPES, OAuthToken, PkcePair, StoredOAuthCredential, YANDEX_SCOPES,
    configured_google_client_id, configured_google_client_secret, configured_microsoft_client_id,
    configured_microsoft_tenant, configured_yandex_client_id, configured_yandex_redirect_uri,
    exchange_google_code, exchange_microsoft_code, exchange_yandex_code, generate_pkce,
    generate_state, google_authorize_url, microsoft_authorize_url, refresh_google_token,
    refresh_microsoft_token, refresh_yandex_token, yandex_authorize_url,
};

use crate::ErrorKind;
use crate::Result;
use crate::backend::{
    EwsBackend, EwsTimeouts, GenericImapBackend, GmailBackend, JmapBackend, MailBackend,
    OutlookBackend, SendOutcome, YandexBackend,
};
use crate::model::{
    Account, AuthKind, BackendKind, FolderRole, NewAccount, Provider, Security, ServerConfig,
};
use crate::storage::Db;
use crate::storage::repo::AuxiliarySaveResult;
use base64::Engine as _;
use zeroize::Zeroizing;

/// Состояние сервера в том виде, в каком его хранит локальная база.
fn server_state_input(
    account_id: i64,
    state: &crate::backend::OofState,
) -> crate::model::OutOfOfficeInput {
    crate::model::OutOfOfficeInput {
        account_id,
        enabled: state.enabled,
        starts_at: state.starts_at.clone(),
        ends_at: state.ends_at.clone(),
        internal_text: state.internal_text.clone(),
        external_text: state.external_text.clone(),
        internal_domains: Vec::new(),
    }
}

/// Время во всемирном времени: сервер Exchange принимает период только в нём
/// (specs/out-of-office.md, S-013).
fn utc_stamp(value: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(value.trim())
        .map(|parsed| {
            parsed
                .with_timezone(&chrono::Utc)
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string()
        })
        .unwrap_or_else(|_| value.trim().to_owned())
}

/// Передать захваченное письмо готовым серверным модулем и довести операцию до
/// итога. Отдельная функция от `AccountsService` нужна проверкам: они проходят
/// весь путь передачи с поддельным серверным модулем, не заводя учётных данных
/// (specs/undo-send.md, S-047 - S-052).
pub(crate) async fn transmit_with_backend(
    db: &Db,
    account_email: &str,
    backend: &dyn MailBackend,
    token: &str,
    operation: &crate::storage::repo::OutboxOperation,
) -> usize {
    let payload = match db.read_send_payload(operation.id, &operation.payload).await {
        Ok(payload) => payload,
        Err(error) => {
            fail_send(db, operation.id, &error).await;
            return 0;
        }
    };
    let message = match db.outgoing_from_payload(&payload).await {
        Ok(message) => message,
        Err(error) => {
            // Большой объект письма недоступен: письмо без вложения не уходит, а
            // операция требует вмешательства пользователя.
            fail_send(db, operation.id, &error).await;
            return 0;
        }
    };
    let outcome = match backend.send(message, token).await {
        Ok(outcome) => outcome,
        Err(failure) if !failure.after_handoff => {
            defer_send(db, operation.id, &failure.error).await;
            return 0;
        }
        Err(failure) => {
            // S-049: письмо уже ушло серверу, и ответа на него нет. Повтор без
            // решения пользователя доставил бы получателю второй экземпляр.
            if let Err(error) = db
                .mark_send_uncertain(operation.id, &failure.error.to_string())
                .await
            {
                warn_send(
                    account_email,
                    operation.id,
                    &error,
                    "неопределённый итог не записан",
                );
            }
            tracing::warn!(
                account = %crate::logging::mask_email(account_email),
                operation = operation.id,
                error_kind = failure.error.code(),
                "итог передачи письма неизвестен, решение за пользователем"
            );
            return 0;
        }
    };
    // recipient-history.md S-009, S-015: обращения записываются в момент
    // подтверждения сервером и до превращения операции в дозапись копии - её
    // данные перечня адресатов уже не хранят.
    record_send_history(db, operation.account_id, &payload).await;
    if let Err(error) = db
        .mark_out_of_office_reply_state(operation.id, "sent")
        .await
    {
        warn_send(
            account_email,
            operation.id,
            &error,
            "состояние автоответа не записано",
        );
    }
    let raw = match outcome {
        SendOutcome::SavedOnServer => None,
        SendOutcome::NeedsSentAppend(raw) => Some(raw),
    };
    let Some(raw) = raw else {
        complete_send(db, account_email, operation.id).await;
        return 1;
    };
    // S-052: операция становится дозаписью копии до попытки добавить копию.
    // Аварийное завершение между подтверждением сервера и этим переводом
    // оставило бы её отправкой, и повтор ушёл бы получателю второй раз.
    if let Err(error) = db.convert_send_to_sent_append(operation.id, &raw).await {
        warn_send(
            account_email,
            operation.id,
            &error,
            "письмо доставлено, но операция осталась отправкой: итог решит пользователь",
        );
        return 1;
    }
    match backend.append_sent(account_email, token, &raw).await {
        Ok(()) => complete_send(db, account_email, operation.id).await,
        Err(error) => {
            // S-052: письмо получателю уже доставлено, поэтому повторяется
            // только добавление копии.
            if let Err(write) = db
                .fail_outbox_operation(operation.id, &error.to_string())
                .await
            {
                warn_send(
                    account_email,
                    operation.id,
                    &write,
                    "отказ дозаписи копии не записан",
                );
            }
            // S-063: в журнал идёт вид ошибки, а не её текст - сервер повторяет
            // в нём адрес и тему письма.
            tracing::warn!(
                account = %crate::logging::mask_email(account_email),
                operation = operation.id,
                error_kind = error.code(),
                "письмо доставлено; повтор продолжит только сохранение копии"
            );
        }
    }
    1
}

/// Закрыть успешную отправку. Отказ записи письма второй раз не отправляет:
/// операция остаётся в состоянии передачи и на следующем запуске получает
/// неопределённый итог (S-027).
async fn complete_send(db: &Db, account_email: &str, operation_id: i64) {
    if let Err(error) = db.complete_send_operation(operation_id).await {
        warn_send(
            account_email,
            operation_id,
            &error,
            "успешная отправка не закрыта",
        );
    }
}

/// Отсрочка после отказа, не дошедшего до передачи письма (S-028, S-048).
async fn defer_send(db: &Db, operation_id: i64, reason: &crate::Error) {
    if let Err(error) = db
        .defer_send_operation(operation_id, &reason.to_string())
        .await
    {
        tracing::warn!(
            operation = operation_id,
            error_kind = error.code(),
            "отсрочка отправки не записана"
        );
    }
}

/// Отказ, зависящий от содержимого письма: попытку он расходует (S-048).
async fn fail_send(db: &Db, operation_id: i64, reason: &crate::Error) {
    if let Err(error) = db
        .fail_outbox_operation(operation_id, &reason.to_string())
        .await
    {
        tracing::warn!(
            operation = operation_id,
            error_kind = error.code(),
            "отказ отправки не записан"
        );
    }
}

/// Запись в журнал об отправке: адрес маскируется, а от ошибки остаётся только
/// её вид - текст сервера повторяет адреса и тему письма (S-063).
fn warn_send(account_email: &str, operation_id: i64, error: &crate::Error, message: &str) {
    tracing::warn!(
        account = %crate::logging::mask_email(account_email),
        operation = operation_id,
        error_kind = error.code(),
        "{message}"
    );
}

/// Записать обращения к адресатам подтверждённой отправки и отметку своего
/// письма (recipient-history.md S-009, S-010, S-014).
async fn record_send_history(db: &Db, account_id: i64, payload: &crate::model::SendPayload) {
    let used_at = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S+00:00")
        .to_string();
    let touches = payload
        .to
        .iter()
        .chain(&payload.cc)
        .chain(&payload.bcc)
        .map(|address| {
            // S-024, S-038: адресат записан одной строкой вместе с именем. Без
            // разбора имя терялось бы, а ключом записи становилась бы вся
            // строка целиком.
            let (name, email) = crate::model::split_display_address(address);
            crate::storage::recipient_history::RecipientTouch {
                email,
                name,
                message_key: payload.message_id.clone(),
                used_at: used_at.clone(),
            }
        })
        .collect::<Vec<_>>();
    if let Err(error) = db.record_own_send(account_id, &payload.message_id).await {
        // S-058: в журнал уходит только маскированный текст ошибки - она охотно
        // повторяет адрес письма.
        tracing::warn!(
            error = %crate::logging::mask_error_text(&error.to_string()),
            "отметка собственной отправки не записана"
        );
    }
    if let Err(error) = db
        .record_recipient_touches(
            account_id,
            &touches,
            crate::storage::recipient_history::TouchOrigin::OwnSend,
        )
        .await
    {
        // Отказ записи обращения доставку письма не отменяет, а текст ошибки
        // маскируется (S-058).
        tracing::warn!(
            error = %crate::logging::mask_error_text(&error.to_string()),
            "обращения к адресатам письма не записаны"
        );
    }
}

fn sent_append_raw(payload: &str) -> Result<Vec<u8>> {
    let payload: serde_json::Value = serde_json::from_str(payload)?;
    let raw = payload["raw"].as_str().ok_or_else(|| {
        crate::Error::AccountConfig("append_sent outbox: нет MIME payload".into())
    })?;
    base64::engine::general_purpose::STANDARD
        .decode(raw)
        .map_err(|error| crate::Error::AccountConfig(format!("append_sent outbox: {error}")))
}

/// Насколько далеко от текущего момента может отстоять дата письма, чтобы о нём
/// ещё имело смысл уведомлять.
/// Локально новый remote_id сам по себе не значит "письмо только что пришло":
/// так же выглядит догруженная история, только что заведённая папка и повторная
/// выкачка после смены UIDVALIDITY.
pub const NOTIFICATION_MAX_AGE_HOURS: i64 = 24;

/// Границы даты письма для уведомлений в том виде, в каком дата лежит в базе
/// (UTC, см. миграцию 0034). Верхняя граница отсекает письма с испорченной
/// датой далеко в будущем: без неё такое письмо считалось бы свежим всегда, в
/// том числе при догрузке истории.
fn notification_date_borders() -> (String, String) {
    let now = chrono::Utc::now();
    let format =
        |value: chrono::DateTime<chrono::Utc>| value.format("%Y-%m-%dT%H:%M:%S+00:00").to_string();
    (
        format(now - chrono::Duration::hours(NOTIFICATION_MAX_AGE_HOURS)),
        format(now + chrono::Duration::hours(NOTIFICATION_MAX_AGE_HOURS)),
    )
}

#[cfg(test)]
mod notification_border_tests {
    use super::{NOTIFICATION_MAX_AGE_HOURS, notification_date_borders};

    /// Границы пишутся в том же виде, в каком дата письма лежит в базе, и
    /// стоят симметрично вокруг текущего момента. Разъехавшийся формат делает
    /// сравнение строк бессмысленным, а сдвиг окна - либо молчание об
    /// пришедшем письме, либо уведомление о догруженной истории.
    #[test]
    fn borders_frame_the_current_moment_the_way_dates_are_stored() {
        let now = chrono::Utc::now();
        let (not_before, not_after) = notification_date_borders();
        for border in [&not_before, &not_after] {
            assert_eq!(border.len(), 25, "длина границы: {border}");
            assert!(border.ends_with("+00:00"), "смещение границы: {border}");
            assert_eq!(border.as_bytes()[10], b'T', "разделитель даты: {border}");
        }
        let before = chrono::DateTime::parse_from_rfc3339(&not_before)
            .expect("нижняя граница разбирается")
            .with_timezone(&chrono::Utc);
        let after = chrono::DateTime::parse_from_rfc3339(&not_after)
            .expect("верхняя граница разбирается")
            .with_timezone(&chrono::Utc);
        assert!(before < now && now < after, "текущий момент внутри окна");
        // Обе границы отстоят от "сейчас" ровно на предел свежести: сдвиг
        // всего окна в прошлое или будущее иначе остался бы незамеченным.
        let limit = chrono::Duration::hours(NOTIFICATION_MAX_AGE_HOURS);
        let slack = chrono::Duration::seconds(5);
        assert!((now - before - limit).abs() < slack, "нижняя граница: {not_before}");
        assert!((after - now - limit).abs() < slack, "верхняя граница: {not_after}");
    }
}

/// Результат короткой синхронизации Входящих. `new_messages` считает только
/// remote ID, которых не было в локальной БД до этого прохода; повторно
/// полученные EWS Modified-события поэтому не создают уведомления.
/// `new_message_ids` - локальные id этих же писем (только из папки Входящие и
/// только со свежей датой, см. NOTIFICATION_MAX_AGE_HOURS), отсортированные по
/// дате по возрастанию: используются для карточки уведомления, чтобы показывать
/// именно новое письмо, а не самое свежее в БД.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxSyncResult {
    pub downloaded: usize,
    pub new_messages: usize,
    pub had_baseline: bool,
    pub new_message_ids: Vec<i64>,
    /// Синхронизация что-то изменила в базе: скачаны письма, пришли удаления,
    /// обновления флагов или сменившиеся проекции писем. Плановая переустановка
    /// наблюдения без изменений даёт false - интерфейсу нечего перезагружать.
    pub changed: bool,
}

// Сколько тел писем догружаем за один проход синхронизации (S-005) и с какого
// размера письмо в фоне не качаем (S-006) - настройки: ключи
// LIMIT_BODY_PREFETCH_MESSAGES и LIMIT_BODY_PREFETCH_SIZE_MB
// (crates/core/src/model/limits.rs, gmail-local-body-prefetch.md).

/// Мегабайт числом: порог размера письма пользователь задаёт в мегабайтах, а
/// запрос сравнивает с полем `size` в байтах.
const BYTES_IN_MEGABYTE: i64 = 1024 * 1024;

/// Итог фоновой догрузки тел писем за один проход.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BodyPrefetch {
    /// Скачано тел писем.
    pub fetched: usize,
    /// Письма, тело которых скачать не удалось.
    pub failed: usize,
    /// Осталось писем без тела в этом окне отбора.
    pub remaining: usize,
}

/// Чем закончился проход, в котором часть папок пропущена из-за обрыва связи
/// (imap-reconnect-resilience.md, S-007 и S-008).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkippedFolders {
    /// Пропусков не было.
    None,
    /// Часть папок пропущена: письма из остальных сохраняются, пользователь
    /// видит предупреждение.
    Warn(String),
    /// Не прочитана ни одна папка: пустой результат нельзя выдавать за успех.
    Failed(String),
}

/// Решение по пропущенным папкам. Вынесено отдельно, чтобы проверять правило
/// без сети и без базы.
pub fn skipped_folders_outcome(skipped: &[String], total_folders: usize) -> SkippedFolders {
    let Some(first) = skipped.first() else {
        return SkippedFolders::None;
    };
    let count = skipped.len();
    if count >= total_folders {
        return SkippedFolders::Failed(format!(
            "связь обрывалась, ни одна папка не прочитана (пропущено {count}, первая: {first})"
        ));
    }
    SkippedFolders::Warn(format!(
        "Из-за обрыва связи пропущено папок: {count} (первая: {first})"
    ))
}

/// Применить решение по пропущенным папкам к проходу синхронизации: частичный
/// проход продолжается с предупреждением, проход, в котором не прочитана ни
/// одна папка, заканчивается ошибкой. Пустой результат обрыва нельзя выдавать
/// за успех: интерфейс показал бы "синхронизировано", а писем не прибавилось
/// бы вовсе (imap-reconnect-resilience.md, S-007, S-008).
fn apply_skipped_folders(
    skipped: &[String],
    total_folders: usize,
    warnings: &mut Vec<SyncWarning>,
) -> Result<()> {
    match skipped_folders_outcome(skipped, total_folders) {
        SkippedFolders::None => Ok(()),
        SkippedFolders::Warn(text) => {
            warnings.push(SyncWarning::classified(
                ErrorKind::NetworkUnavailable,
                text,
                Some("imap"),
            ));
            Ok(())
        }
        // Проход, в котором не прочитана ни одна папка, заканчивается отказом,
        // а не предупреждением: иначе интерфейс показал бы "синхронизировано"
        // при пустом результате обрыва. Предупреждения при этом не заводим -
        // причина уже названа самой ошибкой (S-008).
        SkippedFolders::Failed(text) => Err(crate::Error::classified_backend(
            "imap-sync",
            ErrorKind::NetworkUnavailable,
            text,
        )),
    }
}

/// Результат догрузки старых писем папки: сколько пришло с сервера и какая
/// теперь самая старая дата в папке локально. Дату фронтенд использует курсором
/// следующего прохода - по своему списку писем он её вычислить не может, если
/// догруженные письма не подошли фильтру открытой умной папки.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct BackfillPage {
    pub fetched: usize,
    pub oldest: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SyncKind {
    Mail,
    Auxiliary,
}

#[cfg(test)]
mod skipped_folders_tests {
    //! imap-reconnect-resilience.md, S-006 - S-008.
    use super::{SkippedFolders, SyncWarning, apply_skipped_folders, skipped_folders_outcome};

    /// Правило целиком: сколько папок пропущено и сколько их было всего -
    /// таким проход и объявляется. Слитая граница (count >= total) означает
    /// пустой проход, выданный за успешный.
    #[test]
    fn a_pass_is_named_by_how_many_folders_it_missed() {
        let cases: [(&str, &[&str], usize, SkippedFolders); 5] = [
            ("пропусков не было", &[], 5, SkippedFolders::None),
            (
                "часть папок",
                &["INBOX", "Archive"],
                7,
                SkippedFolders::Warn(
                    "Из-за обрыва связи пропущено папок: 2 (первая: INBOX)".into(),
                ),
            ),
            (
                "все папки",
                &["INBOX", "Sent"],
                2,
                SkippedFolders::Failed(
                    "связь обрывалась, ни одна папка не прочитана (пропущено 2, первая: INBOX)"
                        .into(),
                ),
            ),
            (
                "одна папка из одной",
                &["INBOX"],
                1,
                SkippedFolders::Failed(
                    "связь обрывалась, ни одна папка не прочитана (пропущено 1, первая: INBOX)"
                        .into(),
                ),
            ),
            (
                // Перечень папок успел укоротиться на самом проходе: пропусков
                // больше, чем папок. Это тот же полностью потерянный проход.
                "пропущено больше, чем папок",
                &["INBOX", "Sent", "Archive"],
                2,
                SkippedFolders::Failed(
                    "связь обрывалась, ни одна папка не прочитана (пропущено 3, первая: INBOX)"
                        .into(),
                ),
            ),
        ];
        for (name, skipped, total, expected) in cases {
            let skipped: Vec<String> = skipped.iter().map(|value| value.to_string()).collect();
            assert_eq!(
                skipped_folders_outcome(&skipped, total),
                expected,
                "случай: {name}"
            );
        }
    }

    /// Как решение применяется в самом проходе синхронизации: частичный проход
    /// продолжается с предупреждением нужного вида, а проход без единой
    /// прочитанной папки обязан стать ошибкой. Без этого обрыв связи выглядел
    /// бы успешной синхронизацией, в которой просто "нет новых писем".
    #[test]
    fn a_pass_without_a_single_folder_fails_instead_of_reporting_success() {
        let mut warnings: Vec<SyncWarning> = Vec::new();
        apply_skipped_folders(&[], 3, &mut warnings).expect("проход без пропусков успешен");
        assert!(warnings.is_empty());

        let partial = vec!["INBOX".to_string()];
        apply_skipped_folders(&partial, 3, &mut warnings).expect("частичный проход успешен");
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].kind, "network_unavailable");
        assert_eq!(warnings[0].backend.as_deref(), Some("imap"));
        assert!(warnings[0].message.contains("INBOX"));

        let all = vec!["INBOX".to_string(), "Sent".to_string()];
        let error = apply_skipped_folders(&all, 2, &mut warnings)
            .expect_err("проход без прочитанных папок не может быть успехом");
        assert_eq!(error.backend(), Some("imap-sync"));
        assert!(error.to_string().contains("ни одна папка"));
        // Отказ прохода не превращается ещё и в предупреждение.
        assert_eq!(warnings.len(), 1);
    }
}

#[cfg(test)]
mod sync_registry_tests {
    use super::*;

    fn gmail_account(id: i64) -> Account {
        Account {
            id,
            uuid: uuid::Uuid::new_v4().to_string(),
            email: "test@gmail.com".into(),
            display_name: "Gmail".into(),
            provider: Provider::Gmail,
            backend_kind: BackendKind::Imap,
            auth_kind: AuthKind::Oauth2,
            imap: None,
            smtp: None,
            ews_url: None,
            caldav_url: None,
            carddav_url: None,
            jmap_url: None,
            username: None,
            secret_ref: Some("unused-in-test".into()),
            include_in_unified: true,
            color: None,
            retention_days: 30,
            enabled: true,
            last_sync_at: None,
            last_sync_error: None,
            last_sync_error_kind: None,
            needs_reauth: false,
            tls_insecure: false,
        }
    }

    #[tokio::test]
    async fn serializes_same_account_and_scope_inside_core() {
        let registry = std::sync::Arc::new(SyncRegistry::default());
        let started = std::sync::Arc::new(tokio::sync::Notify::new());
        let release = std::sync::Arc::new(tokio::sync::Notify::new());
        let first_registry = registry.clone();
        let first_started = started.clone();
        let first_release = release.clone();
        let first = tokio::spawn(async move {
            first_registry
                .exclusive(7, SyncKind::Mail, async move {
                    first_started.notify_one();
                    first_release.notified().await;
                    Ok::<_, crate::Error>(())
                })
                .await
        });
        started.notified().await;

        assert!(
            registry
                .exclusive(7, SyncKind::Mail, async { Ok::<_, crate::Error>(()) })
                .await
                .is_err()
        );
        assert!(
            registry
                .exclusive(7, SyncKind::Auxiliary, async { Ok::<_, crate::Error>(()) })
                .await
                .is_err()
        );

        release.notify_one();
        first.await.expect("join first sync").expect("first sync");
        assert!(
            registry
                .exclusive(7, SyncKind::Mail, async { Ok::<_, crate::Error>(()) })
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn gmail_retry_after_survives_a_new_account_manager() {
        let root = std::env::temp_dir().join(format!(
            "truemail-gmail-retry-after-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("create temp data dir");
        let crypto = std::sync::Arc::new(crate::crypto::StorageCrypto::from_key(rand::random()));
        let database_key = crate::crypto::DatabaseKey::from_key(rand::random());
        let db = Db::open_with_database_key(&root, crypto, &database_key)
            .await
            .expect("open database");
        db.migrate().await.expect("migrate database");
        let account = gmail_account(42);
        let retry_at = chrono::Utc::now() + chrono::Duration::minutes(15);

        let first = AccountManager::new(db.clone());
        first
            .remember_gmail_rate_limit(
                &account,
                &crate::Error::RateLimited {
                    backend: "gmail-api".into(),
                    retry_at,
                    message: "test quota".into(),
                    response_code: Some(429),
                },
            )
            .await;
        drop(first);

        let restarted = AccountManager::new(db.clone());
        let error = restarted
            .ensure_gmail_mail_allowed(&account)
            .await
            .expect_err("persisted deadline must block HTTP after restart");
        match error {
            crate::Error::RateLimited {
                backend,
                retry_at: stored,
                message,
                ..
            } => {
                assert_eq!(backend, "gmail-api");
                assert_eq!(stored.timestamp_millis(), retry_at.timestamp_millis());
                assert!(message.contains("HTTP-запрос не отправлен"));
            }
            other => panic!("unexpected error: {other}"),
        }
        assert_eq!(
            restarted
                .process_mail_outbox(&account)
                .await
                .expect("empty outbox must not touch Gmail transport"),
            0
        );

        drop(restarted);
        db.close().await;
        drop(db);
        std::fs::remove_dir_all(root).expect("remove temp data dir");
    }

    /// Без secret_ref обе ветки auxiliary_credential падают на первом же шаге,
    /// ещё до обращения к системному keychain - этого достаточно, чтобы по
    /// тексту ошибки отличить маршрут OAuth2 (oauth_access_token) от
    /// Password/Ntlm, не завязываясь на keychain, которого на CI-раннере
    /// может не быть. Регрессия: раньше create_event/create_contact и
    /// подобные команды в commands.rs всегда звали oauth_access_token
    /// напрямую, и Exchange-аккаунт с Password получал невнятную ошибку про
    /// OAuth-токен вместо честной "нет ссылки на пароль".
    #[tokio::test]
    async fn auxiliary_credential_routes_by_auth_kind() {
        let root = std::env::temp_dir().join(format!(
            "truemail-auxiliary-credential-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("create temp data dir");
        let crypto = std::sync::Arc::new(crate::crypto::StorageCrypto::from_key(rand::random()));
        let database_key = crate::crypto::DatabaseKey::from_key(rand::random());
        let db = Db::open_with_database_key(&root, crypto, &database_key)
            .await
            .expect("open database");
        db.migrate().await.expect("migrate database");
        let manager = AccountManager::new(db.clone());

        let mut oauth_account = gmail_account(1);
        oauth_account.secret_ref = None;
        let error = manager
            .auxiliary_credential(&oauth_account)
            .await
            .expect_err("oauth account without secret_ref must fail before keychain access");
        assert!(
            matches!(&error, crate::Error::AccountConfig(message) if message.contains("OAuth-токен")),
            "unexpected error: {error}"
        );

        for auth_kind in [AuthKind::Password, AuthKind::Ntlm, AuthKind::AppPassword] {
            let mut password_account = gmail_account(2);
            password_account.provider = Provider::Exchange;
            password_account.auth_kind = auth_kind;
            password_account.secret_ref = None;
            let error = manager
                .auxiliary_credential(&password_account)
                .await
                .expect_err(
                    "password-family account without secret_ref must fail before keychain access",
                );
            assert!(
                matches!(&error, crate::Error::AccountConfig(message) if message.contains("пароль")),
                "unexpected error for {auth_kind:?}: {error}"
            );
        }

        db.close().await;
        drop(db);
        std::fs::remove_dir_all(root).expect("remove temp data dir");
    }
}

#[derive(Default)]
struct SyncRegistry {
    locks:
        tokio::sync::Mutex<std::collections::HashMap<i64, std::sync::Arc<tokio::sync::Semaphore>>>,
}

impl SyncRegistry {
    async fn exclusive<T>(
        &self,
        account_id: i64,
        _kind: SyncKind,
        operation: impl std::future::Future<Output = Result<T>>,
    ) -> Result<T> {
        let semaphore = self
            .locks
            .lock()
            .await
            .entry(account_id)
            .or_insert_with(|| std::sync::Arc::new(tokio::sync::Semaphore::new(1)))
            .clone();
        let _permit = semaphore.try_acquire_owned().map_err(|_| {
            crate::Error::Other(format!(
                "синхронизация аккаунта {account_id} уже выполняется"
            ))
        })?;
        operation.await
    }
}

/// Абстракция системного хранилища секретов (OS keychain) для смены пароля
/// (accounts-accordion-password.md). Отдельно от прямых вызовов `keyring::Entry`
/// в остальном файле - только эта операция допускает подмену хранилища в
/// тестах (S-014): CI-раннер не всегда имеет системный keychain.
pub trait SecretStore: Send + Sync {
    /// `None` - записи нет или чтение не удалось; смене пароля это не мешает (S-041).
    /// Прочитанный секрет возвращается в обнуляемом буфере: он живёт до конца
    /// сравнения и не должен оставаться в памяти после него (S-018).
    fn read(&self, secret_ref: &str) -> Option<Zeroizing<String>>;
    /// Возвращает `true`, если `keyring` подтвердил запись без ошибки транспорта.
    /// Итог смены пароля определяется контрольным чтением (S-014), а не этим
    /// значением - оно только для журнала.
    fn write(&self, secret_ref: &str, value: &str) -> bool;
}

/// Боевая реализация - системный keychain через уже используемый в файле `keyring`.
pub struct SystemSecretStore;

impl SecretStore for SystemSecretStore {
    fn read(&self, secret_ref: &str) -> Option<Zeroizing<String>> {
        keyring::Entry::new("truemail", secret_ref)
            .ok()?
            .get_password()
            .ok()
            .map(Zeroizing::new)
    }

    fn write(&self, secret_ref: &str, value: &str) -> bool {
        match keyring::Entry::new("truemail", secret_ref) {
            Ok(entry) => entry.set_password(value).is_ok(),
            Err(_) => false,
        }
    }
}

/// Коды ошибок тихой смены пароля (S-010, accounts-accordion-password.md).
/// Отдельный тип, а не `crate::Error`: прежний контракт смены пароля сохраняет
/// собственные более подробные коды.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChangePasswordError {
    #[error("неверные учётные данные: {0}")]
    InvalidCredentials(String),
    #[error("для этого способа входа пароль не меняется")]
    UnsupportedAuthKind,
    #[error("аккаунт не найден")]
    AccountNotFound,
    #[error("у аккаунта нет сохранённой ссылки на пароль")]
    MissingSecretRef,
    #[error("запись в хранилище секретов не удалась")]
    SecretStoreWriteFailed,
    #[error("состояние хранилища секретов не определено")]
    SecretStoreStateUnknown,
    #[error("смена пароля для этого аккаунта уже выполняется")]
    ChangeInProgress,
    #[error("конфигурация аккаунта изменилась во время проверки")]
    AccountChanged,
    #[error("сервер недоступен: {0}")]
    BackendUnavailable(String),
}

impl ChangePasswordError {
    /// Машиночитаемый код для интерфейса (S-010): UI различает случаи по нему,
    /// а не по тексту сообщения.
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidCredentials(_) => "invalid_credentials",
            Self::UnsupportedAuthKind => "unsupported_auth_kind",
            Self::AccountNotFound => "account_not_found",
            Self::MissingSecretRef => "missing_secret_ref",
            Self::SecretStoreWriteFailed => "secret_store_write_failed",
            Self::SecretStoreStateUnknown => "secret_store_state_unknown",
            Self::ChangeInProgress => "change_in_progress",
            Self::AccountChanged => "account_changed",
            Self::BackendUnavailable(_) => "backend_unavailable",
        }
    }

    /// Соответствие общему набору видов для других экранов приложения.
    pub fn general_error_code(&self) -> Option<&'static str> {
        match self {
            Self::InvalidCredentials(_) => Some("invalid_credentials"),
            Self::UnsupportedAuthKind | Self::AccountChanged => Some("account_config"),
            Self::AccountNotFound => Some("unknown"),
            Self::MissingSecretRef => Some("needs_reauth"),
            Self::SecretStoreWriteFailed | Self::SecretStoreStateUnknown => {
                Some("secret_store_error")
            }
            Self::ChangeInProgress => None,
            Self::BackendUnavailable(_) => Some("server_unavailable"),
        }
    }
}

/// Запасная классификация старой нетипизированной транспортной ошибки.
/// Типизированные ошибки проверяются по коду, а текст остается только для
/// библиотек, которые пока не дают отдельного вида отказа входа.
fn classify_validation_error(message: &str) -> bool {
    let lower = message.to_lowercase();
    const AUTH_MARKERS: [&str; 9] = [
        "401",
        "unauthorized",
        "authenticationfailed",
        "authentication failed",
        "invalid credentials",
        "invalid login",
        "login failed",
        "access denied",
        "неверн", // "неверный пароль/логин" в локализованных ответах некоторых серверов
    ];
    AUTH_MARKERS.iter().any(|marker| lower.contains(marker))
}

/// Сравнение параметров подключения между двумя снимками одного аккаунта
/// (S-042, accounts-accordion-password.md): `secret_ref` и все поля, которые
/// `mail_backend` использует для построения соединения. Цвет, глубина кэша и
/// название аккаунта к подключению отношения не имеют и не считаются сменой.
fn connection_config_matches(before: &Account, after: &Account) -> bool {
    fn server_matches(a: &Option<ServerConfig>, b: &Option<ServerConfig>) -> bool {
        match (a, b) {
            (None, None) => true,
            (Some(a), Some(b)) => a.host == b.host && a.port == b.port && a.security == b.security,
            _ => false,
        }
    }
    before.secret_ref == after.secret_ref
        && before.email == after.email
        && before.username == after.username
        && before.provider == after.provider
        && before.backend_kind == after.backend_kind
        && before.auth_kind == after.auth_kind
        && before.ews_url == after.ews_url
        && before.jmap_url == after.jmap_url
        && server_matches(&before.imap, &after.imap)
        && server_matches(&before.smtp, &after.smtp)
}

/// Блокировка на аккаунт для смены пароля (S-017 accounts-accordion-password.md).
/// Отдельный набор, а не `AppState.syncing` из Tauri-слоя: иначе смена пароля
/// во время фоновой синхронизации ошибочно отклонялась бы (S-016).
#[derive(Default)]
struct PasswordChangeLocks {
    active: tokio::sync::Mutex<std::collections::HashSet<i64>>,
}

impl PasswordChangeLocks {
    async fn try_lock(&self, account_id: i64) -> bool {
        self.active.lock().await.insert(account_id)
    }

    async fn unlock(&self, account_id: i64) {
        self.active.lock().await.remove(&account_id);
    }
}

pub struct AccountManager {
    db: Db,
    // Сериализует обновление OAuth-токена: параллельные mail/aux-sync иначе
    // одновременно видят "истёк" и рефрешат по нескольку раз за минуту.
    refresh_lock: tokio::sync::Mutex<()>,
    sync_registry: SyncRegistry,
    exchange_outbox_repaired: tokio::sync::Mutex<std::collections::HashSet<i64>>,
    password_change_locks: PasswordChangeLocks,
}

#[derive(Debug)]
pub struct ConnectedAccountSync {
    pub account: Account,
    pub mail_folders: usize,
    pub calendars: usize,
    pub events: usize,
    pub contacts: usize,
    pub warnings: Vec<SyncWarning>,
}

/// Структурированное предупреждение частично успешного прохода. Вид ошибки
/// нужен интерфейсу для общей таблицы текста, а исходная причина остаётся
/// только в подробностях.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncWarning {
    pub kind: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_code: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_phase: Option<&'static str>,
}

impl SyncWarning {
    fn from_error(prefix: &str, error: &crate::Error, sync_phase: Option<&'static str>) -> Self {
        let raw = if prefix.is_empty() {
            error.to_string()
        } else {
            format!("{prefix}: {error}")
        };
        Self {
            kind: error.code().to_owned(),
            message: crate::error::sanitize_error_message(&raw),
            backend: error.backend().map(str::to_owned),
            response_code: error.response_code(),
            sync_phase,
        }
    }

    fn classified(kind: ErrorKind, message: impl Into<String>, backend: Option<&str>) -> Self {
        Self {
            kind: kind.code().to_owned(),
            message: message.into(),
            backend: backend.map(str::to_owned),
            response_code: None,
            sync_phase: None,
        }
    }
}

fn mail_sync_phase(initial: bool) -> &'static str {
    if initial { "initial" } else { "regular" }
}

#[cfg(test)]
mod sync_warning_tests {
    use super::{SyncWarning, mail_sync_phase};
    use crate::{Error, ErrorKind};

    #[test]
    fn warning_keeps_error_kind_and_transport_details() {
        let error = Error::classified_backend("ews-http", ErrorKind::ServerUnavailable, "HTTP 500");
        let warning = SyncWarning::from_error("", &error, Some(mail_sync_phase(true)));
        assert_eq!(warning.kind, "server_unavailable");
        assert_eq!(warning.backend.as_deref(), Some("ews-http"));
        assert_eq!(warning.response_code, Some(500));
        assert!(warning.message.contains("HTTP 500"));
    }

}

impl AccountManager {
    pub fn new(db: Db) -> Self {
        Self {
            db,
            refresh_lock: tokio::sync::Mutex::new(()),
            sync_registry: SyncRegistry::default(),
            exchange_outbox_repaired: tokio::sync::Mutex::new(std::collections::HashSet::new()),
            password_change_locks: PasswordChangeLocks::default(),
        }
    }

    fn gmail_rate_limit_key(account_id: i64) -> String {
        format!("gmail_api_retry_at:{account_id}")
    }

    /// secret_ref уже подключённого аккаунта с этим email, если он есть -
    /// снимок состояния ДО апсерта. save_account делает UPSERT по email, так
    /// что после сохранения старое значение уже не прочитать: смотреть
    /// нужно заранее, чтобы потом понять, какую запись keychain подчищать.
    async fn existing_secret_ref(&self, email: &str) -> Option<String> {
        match self.db.list_accounts().await {
            Ok(accounts) => accounts
                .into_iter()
                .find(|account| account.email.eq_ignore_ascii_case(email))
                .and_then(|account| account.secret_ref),
            Err(error) => {
                tracing::warn!(%error, "не удалось прочитать список аккаунтов перед подключением");
                None
            }
        }
    }

    /// Если email раньше был подключён другим способом (был пароль IMAP,
    /// стал OAuth и т.п.), secret_ref в БД сменился на новый, а старая
    /// запись в системном keychain так и осталась висеть с прежним секретом.
    /// Подчищаем её здесь. Вызывать строго ПОСЛЕ того, как новый секрет и
    /// аккаунт уже успешно сохранены: сбой этой очистки не должен ронять
    /// само подключение, а порядок исключает риск остаться совсем без
    /// секрета при сбое на середине.
    fn cleanup_stale_secret(previous: Option<String>, new_secret_ref: &str) {
        let Some(previous) = previous else {
            return;
        };
        if previous == new_secret_ref {
            return;
        }
        match keyring::Entry::new("truemail", &previous) {
            Ok(entry) => {
                if let Err(error) = entry.delete_credential() {
                    tracing::warn!(
                        secret_ref = %previous,
                        %error,
                        "не удалось удалить осиротевшую запись keychain (возможно, уже отсутствует)"
                    );
                }
            }
            Err(error) => {
                tracing::warn!(
                    secret_ref = %previous,
                    %error,
                    "не удалось открыть осиротевшую запись keychain для удаления"
                );
            }
        }
    }

    /// Не выпускать Gmail HTTP-запрос раньше серверного Retry-After даже после
    /// перезапуска приложения. Значение хранится в зашифрованной settings.
    async fn ensure_gmail_mail_allowed(&self, account: &Account) -> Result<()> {
        if account.provider != Provider::Gmail {
            return Ok(());
        }
        let key = Self::gmail_rate_limit_key(account.id);
        let Some(value) = self.db.setting(&key).await? else {
            return Ok(());
        };
        let Ok(retry_at) = chrono::DateTime::parse_from_rfc3339(&value) else {
            tracing::warn!(account = %crate::logging::mask_email(&account.email), value, "повреждён сохранённый Gmail Retry-After; значение проигнорировано");
            return Ok(());
        };
        let retry_at = retry_at.with_timezone(&chrono::Utc);
        let now = chrono::Utc::now();
        if retry_at <= now {
            return Ok(());
        }
        let seconds = (retry_at - now).num_seconds().max(1);
        Err(crate::Error::RateLimited {
            backend: "gmail-api".into(),
            retry_at,
            message: format!(
                "сохранённый Retry-After ещё действует ({seconds} с); HTTP-запрос не отправлен"
            ),
            // Локальная проверка сохранённого дедлайна - без нового запроса к
            // серверу, поэтому актуального кода ответа тут нет.
            response_code: None,
        })
    }

    async fn remember_gmail_rate_limit(&self, account: &Account, error: &crate::Error) {
        if account.provider != Provider::Gmail {
            return;
        }
        let crate::Error::RateLimited {
            backend, retry_at, ..
        } = error
        else {
            return;
        };
        if backend != "gmail-api" {
            return;
        }
        let key = Self::gmail_rate_limit_key(account.id);
        let existing = match self.db.setting(&key).await {
            Ok(value) => value,
            Err(read_error) => {
                tracing::warn!(account = %crate::logging::mask_email(&account.email), %read_error, "Gmail Retry-After не прочитан перед обновлением");
                None
            }
        };
        let existing_retry_at = existing
            .as_deref()
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&chrono::Utc));
        if existing_retry_at.is_some_and(|stored| stored >= *retry_at) {
            return;
        }
        let value = retry_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        match self.db.set_setting(&key, &value).await {
            Ok(()) => tracing::info!(
                account = %crate::logging::mask_email(&account.email),
                retry_at = %value,
                "Gmail Retry-After сохранён и переживёт перезапуск"
            ),
            Err(write_error) => tracing::warn!(
                account = %crate::logging::mask_email(&account.email),
                %write_error,
                "Gmail Retry-After не удалось сохранить"
            ),
        }
    }

    /// `parent_folder_id` - локальный id уже существующей папки, внутри
    /// которой создаётся новая (None - верхний уровень аккаунта). В отличие
    /// от rename/delete здесь нет защиты системных папок: создать подпапку
    /// внутри Входящих - обычная операция, ничего не переименовывает и не
    /// удаляет.
    pub async fn create_folder(
        &self,
        account_id: i64,
        parent_folder_id: Option<i64>,
        name: &str,
    ) -> Result<()> {
        let account = self
            .db
            .list_accounts()
            .await?
            .into_iter()
            .find(|account| account.id == account_id)
            .ok_or_else(|| crate::Error::AccountConfig("аккаунт не найден".into()))?;
        let parent_remote_path = match parent_folder_id {
            Some(id) => {
                let parent = self.db.folder(id).await?;
                if parent.account_id != account_id {
                    return Err(crate::Error::AccountConfig(
                        "родительская папка принадлежит другому аккаунту".into(),
                    ));
                }
                Some(parent.remote_path)
            }
            None => None,
        };
        let token = self.mail_credential(&account).await?;
        let backend = Self::mail_backend(&account)?;
        backend
            .create_folder(&account.email, &token, parent_remote_path.as_deref(), name)
            .await?;
        // Локально папка заводится обычным циклом синхронизации списка папок
        // (как в sync_mail_account_inner) - так UI сразу видит новую папку,
        // а не только remote_path, о котором знает только что созданный backend.
        if let Ok(folders) = backend.discover_folders(&account.email, &token).await {
            self.db
                .save_discovered_folders(account.id, &folders)
                .await?;
        }
        Ok(())
    }

    pub async fn rename_folder(&self, folder_id: i64, new_name: &str) -> Result<()> {
        let folder = self.db.folder(folder_id).await?;
        if folder.role.is_some() {
            return Err(crate::Error::AccountConfig(
                "системную папку нельзя переименовать".into(),
            ));
        }
        let account = self
            .db
            .list_accounts()
            .await?
            .into_iter()
            .find(|account| account.id == folder.account_id)
            .ok_or_else(|| crate::Error::AccountConfig("аккаунт папки не найден".into()))?;
        let token = self.mail_credential(&account).await?;
        let backend = Self::mail_backend(&account)?;
        let remote = backend
            .rename_folder(&account.email, &token, &folder.remote_path, new_name)
            .await?;
        self.db
            .rename_folder_local(folder.id, &remote, new_name.trim())
            .await
    }

    pub async fn delete_folder(&self, folder_id: i64) -> Result<()> {
        let folder = self.db.folder(folder_id).await?;
        if folder.role.is_some() {
            return Err(crate::Error::AccountConfig(
                "системную папку нельзя удалить".into(),
            ));
        }
        let account = self
            .db
            .list_accounts()
            .await?
            .into_iter()
            .find(|account| account.id == folder.account_id)
            .ok_or_else(|| crate::Error::AccountConfig("аккаунт папки не найден".into()))?;
        let token = self.mail_credential(&account).await?;
        let backend = Self::mail_backend(&account)?;
        backend
            .delete_folder(&account.email, &token, &folder.remote_path)
            .await?;
        self.db.delete_folder_local(folder.id).await
    }

    pub async fn list(&self) -> Result<Vec<Account>> {
        self.db.list_accounts().await
    }

    fn mail_backend(account: &Account) -> Result<Box<dyn MailBackend>> {
        if account.backend_kind == BackendKind::Jmap {
            return Ok(Box::new(JmapBackend {
                session_url: account.jmap_url.clone().ok_or_else(|| {
                    crate::Error::AccountConfig("для аккаунта не настроен JMAP Session URL".into())
                })?,
                username: account
                    .username
                    .clone()
                    .unwrap_or_else(|| account.email.clone()),
            }));
        }
        match account.provider {
            Provider::Yandex => Ok(Box::new(YandexBackend)),
            Provider::Gmail => Ok(Box::new(GmailBackend)),
            Provider::Outlook => Ok(Box::new(OutlookBackend)),
            Provider::Mailru | Provider::Icloud | Provider::Generic => {
                let imap = account.imap.clone().ok_or_else(|| {
                    crate::Error::AccountConfig("для аккаунта не настроен IMAP-сервер".into())
                })?;
                Ok(Box::new(GenericImapBackend {
                    username: account
                        .username
                        .clone()
                        .unwrap_or_else(|| account.email.clone()),
                    imap,
                    smtp: account.smtp.clone(),
                }))
            }
            Provider::Exchange => Ok(Box::new(EwsBackend {
                endpoint: account.ews_url.clone().ok_or_else(|| {
                    crate::Error::AccountConfig("для Exchange не настроен адрес EWS".into())
                })?,
                username: account
                    .username
                    .clone()
                    .unwrap_or_else(|| account.email.clone()),
                // Фоновая синхронизация - прежние 10/30/30 (S-014).
                timeouts: crate::backend::EwsTimeouts::background(),
            })),
        }
    }

    async fn mail_credential(&self, account: &Account) -> Result<Zeroizing<String>> {
        self.ensure_gmail_mail_allowed(account).await?;
        self.auxiliary_credential(account).await
    }

    /// Секрет для вспомогательных операций (запись события/контакта из
    /// commands.rs) и для Exchange EWS вне почтового rate-limit'а Gmail:
    /// для OAuth2 - access token с обновлением по refresh (см.
    /// oauth_access_token), для Password/Ntlm/AppPassword - обычный пароль
    /// из системного keychain. Раньше эти команды всегда звали
    /// oauth_access_token напрямую, из-за чего Exchange-аккаунт с паролем
    /// падал на serde_json::from_str ещё до выбора провайдера - секрет в
    /// keychain у него лежит строкой, а не JSON StoredOAuthCredential.
    pub async fn auxiliary_credential(&self, account: &Account) -> Result<Zeroizing<String>> {
        if account.auth_kind == AuthKind::Oauth2 {
            return self.oauth_access_token(account).await;
        }
        let secret_ref = account
            .secret_ref
            .as_deref()
            .ok_or_else(|| crate::Error::AccountConfig("нет ссылки на пароль аккаунта".into()))?;
        keyring::Entry::new("truemail", secret_ref)
            .map_err(|error| crate::Error::Keyring(error.to_string()))?
            .get_password()
            .map(Zeroizing::new)
            .map_err(|error| crate::Error::Keyring(error.to_string()))
    }

    /// Прочитать сохранённый OAuth access token из системного keychain.
    pub async fn oauth_access_token(&self, account: &Account) -> Result<Zeroizing<String>> {
        let secret_ref = account
            .secret_ref
            .as_deref()
            .ok_or_else(|| crate::Error::AccountConfig("нет ссылки на OAuth-токен".into()))?;
        let entry = keyring::Entry::new("truemail", secret_ref)
            .map_err(|e| crate::Error::Keyring(e.to_string()))?;
        let serialized = Zeroizing::new(
            entry
                .get_password()
                .map_err(|e| crate::Error::Keyring(e.to_string()))?,
        );
        let mut credential: StoredOAuthCredential = serde_json::from_str(&serialized)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        if credential
            .expires_at
            .is_some_and(|expires| expires <= now + 60)
        {
            // Под мьютексом перечитываем токен: пока ждали блокировку, другой
            // поток мог уже обновить его - тогда повторный refresh не нужен.
            let _guard = self.refresh_lock.lock().await;
            if let Ok(serialized) = entry.get_password() {
                let serialized = Zeroizing::new(serialized);
                if let Ok(fresh) = serde_json::from_str::<StoredOAuthCredential>(&serialized) {
                    credential = fresh;
                }
            }
            // Токен ещё живой (или бессрочный) - обновлять нечего.
            if credential
                .expires_at
                .is_none_or(|expires| expires > now + 60)
            {
                return Ok(Zeroizing::new(credential.access_token.clone()));
            }
            let refresh_token =
                Zeroizing::new(credential.refresh_token.clone().ok_or_else(|| {
                    crate::Error::AccountConfig(
                        "OAuth-токен истёк и не содержит refresh_token".into(),
                    )
                })?);
            let refreshed = match account.provider {
                Provider::Yandex => {
                    let client_id = configured_yandex_client_id().ok_or_else(|| {
                        crate::Error::AccountConfig(
                            "для обновления OAuth-токена не задан TRUEMAIL_YANDEX_CLIENT_ID".into(),
                        )
                    })?;
                    refresh_yandex_token(&client_id, &refresh_token).await?
                }
                Provider::Gmail => {
                    let client_id = configured_google_client_id().ok_or_else(|| {
                        crate::Error::AccountConfig(
                            "для обновления OAuth-токена не задан TRUEMAIL_GOOGLE_CLIENT_ID".into(),
                        )
                    })?;
                    let client_secret =
                        Zeroizing::new(configured_google_client_secret().ok_or_else(|| {
                            crate::Error::AccountConfig(
                                "для обновления OAuth-токена не задан TRUEMAIL_GOOGLE_CLIENT_SECRET"
                                    .into(),
                            )
                        })?);
                    refresh_google_token(&client_id, &client_secret, &refresh_token).await?
                }
                Provider::Outlook => {
                    let client_id = configured_microsoft_client_id().ok_or_else(|| {
                        crate::Error::AccountConfig(
                            "для обновления OAuth-токена не задан TRUEMAIL_MICROSOFT_CLIENT_ID"
                                .into(),
                        )
                    })?;
                    refresh_microsoft_token(
                        &client_id,
                        &configured_microsoft_tenant(),
                        &refresh_token,
                    )
                    .await?
                }
                _ => {
                    return Err(crate::Error::AccountConfig(
                        "обновление OAuth-токена для провайдера не настроено".into(),
                    ));
                }
            };
            let mut updated = StoredOAuthCredential::from_refresh(refreshed, &refresh_token);
            // Google при refresh обычно не возвращает scope - сохраняем прежний,
            // иначе информация о выданных разрешениях теряется.
            if updated.scope.is_none() {
                updated.scope = credential.scope.clone();
            }
            let serialized = Zeroizing::new(serde_json::to_string(&updated)?);
            entry
                .set_password(&serialized)
                .map_err(|e| crate::Error::Keyring(e.to_string()))?;
            tracing::info!(email = %crate::logging::mask_email(&account.email), provider = ?account.provider, scope = ?updated.scope, "OAuth-токен обновлён через refresh");
            credential = updated;
        }
        Ok(Zeroizing::new(credential.access_token.clone()))
    }

    /// Лёгкая проверка последних ID Gmail Входящих без загрузки писем.
    /// Сравнение с предыдущим снимком выполняет цикл уведомлений: локальная БД
    /// может быть ещё не заполнена во время стартовой синхронизации.
    pub async fn gmail_latest_message_ids(&self, account: &Account) -> Result<Vec<String>> {
        if account.provider != Provider::Gmail {
            return Ok(Vec::new());
        }
        self.ensure_gmail_mail_allowed(account).await?;
        let token = self.oauth_access_token(account).await?;
        let result = crate::backend::gmail_latest_ids(&token, 25).await;
        if let Err(error) = &result {
            self.remember_gmail_rate_limit(account, error).await;
        }
        result
    }

    /// Дозагрузить только последние входящие и отдельно посчитать действительно
    /// новые письма. Exchange использует этот результат для watchdog-уведомлений.
    pub async fn sync_mail_inbox_delta(&self, account: &Account) -> Result<InboxSyncResult> {
        let access_token = self.mail_credential(account).await?;
        let backend = Self::mail_backend(account)?;
        let cursors = self.db.folder_sync_cursors(account.id).await?;
        let discovery_result = backend
            .discover_inbox(&account.email, &access_token, &cursors)
            .await;
        if let Err(error) = &discovery_result {
            self.remember_gmail_rate_limit(account, error).await;
        }
        let discovery = discovery_result?;
        let had_baseline = discovery
            .folders
            .iter()
            .find(|folder| folder.role == Some(FolderRole::Inbox))
            .and_then(|folder| cursors.get(&folder.remote_path))
            .and_then(|cursor| cursor.sync_token.as_deref())
            .is_some_and(|value| !value.is_empty());
        let downloaded = discovery.messages.len();
        let mut remote_ids = discovery
            .messages
            .iter()
            .filter_map(|message| message.remote_id.clone())
            .collect::<Vec<_>>();
        remote_ids.sort();
        remote_ids.dedup();
        // До этого момента письма ещё не в БД - список остаётся полным набором
        // действительно новых remote_id, а не тем, что уже успело устареть.
        let unknown_remote_ids = self.db.unknown_remote_ids(account.id, &remote_ids).await?;
        let new_messages = unknown_remote_ids.len();
        let folders_before = self.db.folder_state_signature(account.id).await?;
        self.db
            .save_discovered_folders(account.id, &discovery.folders)
            .await?;
        let folders_changed = self.db.folder_state_signature(account.id).await? != folders_before;
        let snapshot_removed = self
            .db
            .reconcile_imap_snapshot(account.id, &discovery.server_uids, &discovery.reset_folders)
            .await?;
        // Счётчики примененных изменений собираем по ходу: по ним решается,
        // поднимать ли перезагрузку данных в интерфейсе.
        let folders_reconciled = self
            .db
            .reconcile_discovered_folders(account.id, &discovery.folders)
            .await?;
        let vanished = self
            .db
            .apply_imap_vanished(account.id, &discovery.deleted_uids)
            .await?;
        let flags_applied = self
            .db
            .apply_imap_flag_updates(account.id, &discovery.flag_updates)
            .await?;
        let projections = self
            .db
            .reconcile_remote_projections(
                account.id,
                &discovery.messages,
                &discovery.changed_remote_ids,
                discovery.remote_snapshot.as_deref(),
            )
            .await?;
        self.db
            .save_discovered_messages(account.id, &discovery.messages, false)
            .await?;
        self.db
            .save_folder_sync_tokens(account.id, &discovery.folders)
            .await?;
        let rules_applied = match self.db.process_sync_batch_stages().await {
            Ok(count) => count,
            Err(error) => {
                tracing::warn!(%error, "правила обработки будут повторены при следующей синхронизации");
                0
            }
        };
        // S-010, S-011: список для уведомления собирается после стадий
        // обработки и по письмам этой же синхронизационной пачки, поэтому
        // уведённое правилом письмо в уведомление уже не попадает. Только
        // Входящие (роль 'inbox'): другие папки уведомлению не нужны.
        let new_message_ids = if unknown_remote_ids.is_empty() {
            Vec::new()
        } else {
            let (not_before, not_after) = notification_date_borders();
            self.db
                .inbox_message_ids_by_remote_ids(
                    account.id,
                    &unknown_remote_ids,
                    Some(&not_before),
                    Some(&not_after),
                )
                .await?
        };
        let changed = downloaded > 0
            || folders_changed
            || snapshot_removed > 0
            || folders_reconciled > 0
            || vanished > 0
            || flags_applied > 0
            || projections > 0
            || rules_applied > 0
            || !discovery.reset_folders.is_empty();
        Ok(InboxSyncResult {
            downloaded,
            new_messages,
            had_baseline,
            new_message_ids,
            changed,
        })
    }

    /// Совместимый интерфейс для IMAP/Gmail: полный результат синхронизации
    /// Входящих, включая новые id для уведомлений.
    pub async fn sync_mail_inbox(&self, account: &Account) -> Result<InboxSyncResult> {
        self.sync_mail_inbox_delta(account).await
    }

    /// Гарантировать наличие сырого MIME письма локально. Если кэш был вычищен
    /// по глубине хранения, письмо докачивается с сервера и сохраняется, чтобы
    /// открыться мгновенно в этой сессии (prune действует только на старте).
    pub async fn ensure_message_raw(&self, message_id: i64) -> Result<()> {
        let Some((account_id, folder_path, uid, remote_id, has_raw)) =
            self.db.message_fetch_locator(message_id).await?
        else {
            return Ok(());
        };
        if has_raw {
            return Ok(());
        }
        let Some(account) = self
            .db
            .list_accounts()
            .await?
            .into_iter()
            .find(|item| item.id == account_id)
        else {
            return Ok(());
        };
        let access_token = self.mail_credential(&account).await?;
        let backend = Self::mail_backend(&account)?;
        let raw = backend
            .fetch_message_raw(
                &account.email,
                &access_token,
                &folder_path,
                uid as u32,
                remote_id.as_deref(),
            )
            .await?;
        self.db.store_fetched_raw(message_id, &raw).await?;
        tracing::info!(message_id, account = %crate::logging::mask_email(&account.email), "письмо докачано с сервера (вне кэша)");
        Ok(())
    }

    /// Фоновая догрузка тел писем аккаунта (gmail-local-body-prefetch.md).
    /// Письма Gmail приходят при синхронизации без тела, поэтому без этой
    /// догрузки первое открытие каждого письма упирается в сеть.
    /// Возвращает, сколько тел скачано, сколько писем не удалось скачать и
    /// сколько писем без тела осталось в этом окне отбора.
    pub async fn prefetch_missing_bodies(&self, account: &Account) -> Result<BodyPrefetch> {
        let pending = self
            .db
            .messages_missing_body(
                account.id,
                account.retention_days,
                self.db.limit(crate::model::LIMIT_BODY_PREFETCH_SIZE_MB) * BYTES_IN_MEGABYTE,
                self.db.limit(crate::model::LIMIT_BODY_PREFETCH_MESSAGES),
            )
            .await?;
        if pending.is_empty() {
            return Ok(BodyPrefetch::default());
        }
        let access_token = self.mail_credential(account).await?;
        let backend = Self::mail_backend(account)?;
        let mut result = BodyPrefetch {
            remaining: pending.len(),
            ..BodyPrefetch::default()
        };
        // Запросы идут по одному (S-007): фоновая догрузка не должна вытеснять
        // обычную синхронизацию из квоты почтового сервиса.
        for (message_id, folder_path, uid, remote_id) in pending {
            match backend
                .fetch_message_raw(
                    &account.email,
                    &access_token,
                    &folder_path,
                    uid as u32,
                    remote_id.as_deref(),
                )
                .await
            {
                Ok(raw) => {
                    self.db.store_fetched_raw(message_id, &raw).await?;
                    result.fetched += 1;
                    result.remaining -= 1;
                }
                Err(error) => {
                    // Ограничение частоты обращений: прекращаем до следующего
                    // прохода, иначе квота уйдёт на повторные отказы (S-009).
                    let rate_limited = matches!(error, crate::Error::RateLimited { .. });
                    self.remember_gmail_rate_limit(account, &error).await;
                    tracing::warn!(
                        message_id,
                        account = %crate::logging::mask_email(&account.email),
                        %error,
                        "фоновая догрузка тела письма не удалась"
                    );
                    result.failed += 1;
                    if rate_limited {
                        break;
                    }
                }
            }
        }
        tracing::info!(
            account = %crate::logging::mask_email(&account.email),
            fetched = result.fetched,
            failed = result.failed,
            remaining = result.remaining,
            "фоновая догрузка тел писем завершена"
        );
        Ok(result)
    }

    /// Догрузить с сервера письма папки старше даты `before` и сохранить в базу.
    /// Для бесконечной прокрутки: когда локальные письма кончились, а на сервере
    /// их больше. Возвращает число сохранённых. Провайдеры без поддержки вернут 0.
    pub async fn fetch_older_folder_messages(
        &self,
        folder_id: i64,
        before: &str,
        limit: usize,
    ) -> Result<BackfillPage> {
        let folder = self.db.folder(folder_id).await?;
        let Some(account) = self
            .db
            .list_accounts()
            .await?
            .into_iter()
            .find(|item| item.id == folder.account_id)
        else {
            tracing::warn!(folder_id, "догрузка: аккаунт папки не найден");
            return Ok(BackfillPage::default());
        };
        tracing::info!(folder_id, before, provider = ?account.provider, remote_path = %folder.remote_path, "догрузка: запрос старых писем с сервера");
        let credential = self.mail_credential(&account).await?;
        let backend = Self::mail_backend(&account)?;
        let messages = backend
            .fetch_older_messages(
                &account.email,
                &credential,
                &folder.remote_path,
                before,
                limit,
            )
            .await?;
        tracing::info!(
            folder_id,
            fetched = messages.len(),
            "догрузка: сервер вернул письма"
        );
        if messages.is_empty() {
            // Курсор всё равно отдаём: он двигает следующий проход даже когда
            // сервер на этой границе ничего не дал.
            return Ok(BackfillPage {
                fetched: 0,
                oldest: self.db.folder_oldest_message_date(folder_id).await?,
            });
        }
        let count = messages.len();
        // Правила не должны срабатывать на старую переписку, поднятую прокруткой:
        // по догруженным письмам выполняются только стадии пути догрузки (S-063).
        self.db
            .save_discovered_messages(account.id, &messages, true)
            .await?;
        self.db.process_backfill_stages().await?;
        tracing::info!(folder_id, count, account = %crate::logging::mask_email(&account.email), "догружены более старые письма папки");
        Ok(BackfillPage {
            fetched: count,
            oldest: self.db.folder_oldest_message_date(folder_id).await?,
        })
    }

    /// Очистить кэш всех аккаунтов по их глубине хранения. Вызывается ОДИН РАЗ
    /// при старте приложения: в течение сессии свежие письма не удаляются, а
    /// письма за рамками периода при открытии докачиваются с сервера.
    pub async fn prune_all_caches_on_start(&self) -> Result<()> {
        for account in self.db.list_accounts().await? {
            if account.retention_days <= 0 {
                continue;
            }
            match self
                .db
                .prune_cached_messages(account.id, account.retention_days)
                .await
            {
                Ok(pruned) if pruned > 0 => tracing::info!(
                    account = %crate::logging::mask_email(&account.email),
                    pruned,
                    retention_days = account.retention_days,
                    "кэш очищен по глубине хранения (старт)"
                ),
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(account = %crate::logging::mask_email(&account.email), %error, "автоочистка кэша не удалась")
                }
            }
        }
        Ok(())
    }

    /// Определить базовые адреса CalDAV/CardDAV для аккаунта. Источники
    /// перебираются в порядке убывания достоверности, первый давший ответ
    /// выигрывает:
    /// 1) адрес, уже сохранённый на аккаунте (ручная настройка или прошлое
    ///    обнаружение) - явное решение пользователя и результат прошлого
    ///    поиска не имеет смысла перепроверять на каждой синхронизации;
    /// 2) фиксированные адреса Яндекса - они известны и стабильны, сетевой
    ///    поиск для них лишний;
    /// 3) SRV-записи RFC 6764 (_caldavs._tcp/_carddavs._tcp) - это прямое
    ///    заявление владельца домена о том, где живёт DAV, и единственный
    ///    источник, умеющий указать чужой хост и нестандартный порт;
    ///    .well-known работает только когда DAV на том же хосте, что и сайт,
    ///    поэтому SRV идёт раньше;
    /// 4) .well-known-редирект по домену почты - более распространён, но
    ///    менее выразителен;
    /// 5) провайдерские дефолты - последнее средство, к разбору домена
    ///    отношения не имеющее.
    ///
    /// Найденные адреса сохраняются на аккаунте, чтобы не искать их заново
    /// при каждой синхронизации.
    async fn resolve_dav_bases(
        &self,
        account: &Account,
    ) -> Result<(Option<String>, Option<String>)> {
        if account.provider == Provider::Yandex {
            let (cal, card) = dav::resolve_yandex_bases(
                account.caldav_url.as_deref(),
                account.carddav_url.as_deref(),
            );
            return Ok((Some(cal), Some(card)));
        }
        let mut caldav_url = account.caldav_url.clone();
        let mut carddav_url = account.carddav_url.clone();
        if let Some((_, domain)) = account.email.rsplit_once('@') {
            let origin = format!("https://{domain}");
            if caldav_url.is_none() {
                caldav_url = dav::discover_srv(domain, dav::SRV_CALDAVS).await;
            }
            if caldav_url.is_none() {
                caldav_url = dav::discover_well_known(&origin, dav::WELL_KNOWN_CALDAV).await;
            }
            if carddav_url.is_none() {
                carddav_url = dav::discover_srv(domain, dav::SRV_CARDDAVS).await;
            }
            if carddav_url.is_none() {
                carddav_url = dav::discover_well_known(&origin, dav::WELL_KNOWN_CARDDAV).await;
            }
        }
        if caldav_url != account.caldav_url || carddav_url != account.carddav_url {
            self.db
                .set_dav_urls(account.id, caldav_url.as_deref(), carddav_url.as_deref())
                .await?;
        }
        Ok((caldav_url, carddav_url))
    }

    /// Обновить календарь и контакты по CalDAV/CardDAV, не запуская тяжёлую
    /// IMAP-синхронизацию. Работает для любого провайдера с известными или
    /// обнаруженными DAV-адресами (см. resolve_dav_bases) - раньше это было
    /// жёстко привязано к Яндексу.
    pub async fn sync_dav_auxiliary_account(
        &self,
        account: &Account,
    ) -> Result<AuxiliarySaveResult> {
        self.sync_registry
            .exclusive(
                account.id,
                SyncKind::Auxiliary,
                self.sync_dav_auxiliary_account_inner(account),
            )
            .await
    }

    async fn sync_dav_auxiliary_account_inner(
        &self,
        account: &Account,
    ) -> Result<AuxiliarySaveResult> {
        // auxiliary_credential (не oauth_access_token) - иначе аккаунт с
        // Password/AppPassword (iCloud, Mail.ru, generic) упадёт на разборе
        // JSON ещё до диспетчеризации по провайдеру.
        let secret = self.auxiliary_credential(account).await?;
        let auth = dav::DavAuth::new(
            dav::dav_auth_scheme(account.provider, account.auth_kind),
            account
                .username
                .clone()
                .unwrap_or_else(|| account.email.clone()),
            secret.as_str(),
        );
        let (caldav_base, carddav_base) = self.resolve_dav_bases(account).await?;
        let cursors = self.db.auxiliary_sync_cursors(account.id).await?;
        let dav = dav::sync_dav_account(
            &account.email,
            &auth,
            caldav_base.as_deref(),
            carddav_base.as_deref(),
            &cursors,
        )
        .await?;
        self.db.save_dav(account.id, &dav).await
    }

    /// Обновить Google Calendar, Contacts и Tasks отдельно от IMAP.
    pub async fn sync_google_auxiliary_account(
        &self,
        account: &Account,
    ) -> Result<AuxiliarySaveResult> {
        self.sync_registry
            .exclusive(
                account.id,
                SyncKind::Auxiliary,
                self.sync_google_auxiliary_account_inner(account),
            )
            .await
    }

    async fn sync_google_auxiliary_account_inner(
        &self,
        account: &Account,
    ) -> Result<AuxiliarySaveResult> {
        let access_token = self.oauth_access_token(account).await?;
        let cursors = self.db.auxiliary_sync_cursors(account.id).await?;
        let data = sync_google_services(&access_token, &cursors).await?;
        self.db.save_google_services(account.id, &data).await
    }

    /// Обновить календарь и контакты Exchange через EWS отдельно от почты.
    pub async fn sync_exchange_auxiliary_account(
        &self,
        account: &Account,
    ) -> Result<AuxiliarySaveResult> {
        self.sync_registry
            .exclusive(
                account.id,
                SyncKind::Auxiliary,
                self.sync_exchange_auxiliary_account_inner(account),
            )
            .await
    }

    async fn sync_exchange_auxiliary_account_inner(
        &self,
        account: &Account,
    ) -> Result<AuxiliarySaveResult> {
        let credential = self.mail_credential(account).await?;
        let endpoint = account.ews_url.clone().ok_or_else(|| {
            crate::Error::AccountConfig("для Exchange не настроен адрес EWS".into())
        })?;
        let username = account
            .username
            .clone()
            .unwrap_or_else(|| account.email.clone());
        let backend = EwsBackend {
            endpoint,
            username,
            // Фоновая синхронизация, наблюдение и вспомогательные операции -
            // прежние 10/30/30 (S-014).
            timeouts: crate::backend::EwsTimeouts::background(),
        };
        let cursors = self.db.auxiliary_sync_cursors(account.id).await?;
        let data = backend.auxiliary(&credential, &cursors).await?;
        self.db
            .save_auxiliary_data(account.id, "exchange", &data)
            .await
    }

    /// Обновить дополнительные сервисы поддерживаемого провайдера.
    /// Почтовый цикл вызывает только почтовую синхронизацию, а этот метод —
    /// единственная точка входа для календарей, контактов и задач.
    pub async fn sync_auxiliary_account(&self, account: &Account) -> Result<AuxiliarySaveResult> {
        // JMAP-аккаунт живёт как Provider::Generic (см. add_jmap_password), но
        // у JMAP нет календаря/контактов через DAV - без этой проверки он
        // попал бы в общую DAV-ветку и на каждой синхронизации безрезультатно
        // дёргал бы .well-known на своём домене.
        if account.backend_kind == BackendKind::Jmap {
            return Ok(AuxiliarySaveResult::default());
        }
        match account.provider {
            Provider::Gmail => self.sync_google_auxiliary_account(account).await,
            Provider::Exchange => self.sync_exchange_auxiliary_account(account).await,
            Provider::Yandex
            | Provider::Icloud
            | Provider::Mailru
            | Provider::Outlook
            | Provider::Generic => self.sync_dav_auxiliary_account(account).await,
        }
    }

    /// Ответить на приглашение (RSVP): единая точка входа, дальше ветвление
    /// по провайдеру - тот же приём, что у write_event/delete_event в
    /// auxiliary.rs, только здесь нужен доступ к self (учётные данные,
    /// SMTP-транспорт), поэтому это метод AccountManager, а не свободная
    /// функция в auxiliary.rs.
    /// - Google: events.patch с обновлённым responseStatus (sendUpdates=all) -
    ///   сервер сам уведомляет организатора.
    /// - CalDAV (Яндекс и остальные DAV-провайдеры): сервер не рассылает
    ///   приглашения сам - PUT своей копии события с новым PARTSTAT плюс
    ///   письмо-ответ организатору в формате iMIP (METHOD:REPLY, RFC 5546).
    /// - Exchange: штатные SOAP-операции AcceptItem/DeclineItem/
    ///   TentativelyAcceptItem - сервер сам формирует и рассылает ответ.
    pub async fn respond_to_event(
        &self,
        account: &Account,
        calendar_source: &str,
        remote: RemoteObject<'_>,
        event: &crate::model::Event,
        response: crate::model::RsvpResponse,
    ) -> Result<()> {
        match account.provider {
            Provider::Gmail => {
                let token = self.oauth_access_token(account).await?;
                let attendees =
                    auxiliary::updated_attendees(&event.attendees, &account.email, response);
                let remote_url = remote.remote_url.ok_or_else(|| {
                    crate::Error::AccountConfig("у события нет серверного идентификатора".into())
                })?;
                auxiliary::respond_to_google_event(calendar_source, remote_url, &attendees, &token)
                    .await
            }
            Provider::Yandex
            | Provider::Icloud
            | Provider::Mailru
            | Provider::Outlook
            | Provider::Generic => {
                self.respond_to_dav_event(account, calendar_source, remote, event, response)
                    .await
            }
            Provider::Exchange => {
                let credential = self.mail_credential(account).await?;
                let endpoint = account.ews_url.clone().ok_or_else(|| {
                    crate::Error::AccountConfig("для Exchange не настроен адрес EWS".into())
                })?;
                let username = account
                    .username
                    .clone()
                    .unwrap_or_else(|| account.email.clone());
                let backend = EwsBackend {
                    endpoint,
                    username,
                    // Фоновая синхронизация, наблюдение и вспомогательные операции -
                    // прежние 10/30/30 (S-014).
                    timeouts: crate::backend::EwsTimeouts::background(),
                };
                let remote_url = remote.remote_url.ok_or_else(|| {
                    crate::Error::AccountConfig("у события нет серверного идентификатора".into())
                })?;
                let item_id = remote_url.strip_prefix("ews-event:").ok_or_else(|| {
                    crate::Error::AccountConfig(
                        "неизвестный серверный идентификатор события".into(),
                    )
                })?;
                backend
                    .respond_to_calendar_item(&credential, item_id, response)
                    .await
            }
        }
    }

    /// Ветка respond_to_event для CalDAV (Яндекс и остальные DAV-провайдеры):
    /// сервер не знает про iMIP, поэтому отвечающий сам обновляет свою копию
    /// события (PUT с новым PARTSTAT) и сам же отправляет организатору
    /// письмо-ответ. Оба шага делаем последовательно; если PUT прошёл, а
    /// письмо не ушло - вернём ошибку письма (само событие уже несёт
    /// правильный статус локально после следующей синхронизации, и
    /// update_own_partstat в commands.rs применит его немедленно только если
    /// respond_to_event вернул Ok, так что тут именно так: без письма
    /// организатор не узнает об ответе).
    /// EwsBackend по данным аккаунта. Вынесено, чтобы шесть операций записи
    /// не повторяли сборку endpoint/username каждая по-своему.
    fn ews_backend(&self, account: &Account) -> Result<EwsBackend> {
        let endpoint = account.ews_url.clone().ok_or_else(|| {
            crate::Error::AccountConfig("для Exchange не настроен адрес EWS".into())
        })?;
        let username = account
            .username
            .clone()
            .unwrap_or_else(|| account.email.clone());
        Ok(EwsBackend {
            endpoint,
            username,
            // Фоновая синхронизация, наблюдение и вспомогательные операции -
            // прежние 10/30/30 (S-014).
            timeouts: crate::backend::EwsTimeouts::background(),
        })
    }

    /// Идентификатор элемента EWS из remote_url. Читающая сторона (ews.rs)
    /// сохраняет их с префиксом ews-event:/ews-contact:, поэтому чужой
    /// префикс - это признак, что объект пришёл от другого провайдера.
    fn ews_item_id<'a>(remote_url: &'a str, prefix: &str) -> Result<&'a str> {
        remote_url.strip_prefix(prefix).ok_or_else(|| {
            crate::Error::AccountConfig("неизвестный серверный идентификатор объекта".into())
        })
    }

    /// Создать или изменить событие. Для Exchange - через EWS, для остальных -
    /// прежним путём (Google REST или CalDAV).
    pub async fn write_event(
        &self,
        account: &Account,
        calendar_source: &str,
        remote: RemoteObject<'_>,
        input: &EventInput,
    ) -> Result<()> {
        let credential = self.auxiliary_credential(account).await?;
        if account.provider == Provider::Exchange {
            let backend = self.ews_backend(account)?;
            return match remote.remote_url {
                Some(url) => {
                    backend
                        .update_calendar_item(
                            &credential,
                            Self::ews_item_id(url, "ews-event:")?,
                            input,
                        )
                        .await
                }
                // ItemId созданного элемента намеренно отбрасываем: как и в
                // DAV-ветке, локальные записи получают серверные идентификаторы
                // ближайшим refresh_auxiliary, отдельного пути для них нет.
                None => backend
                    .create_calendar_item(&credential, input)
                    .await
                    .map(|_| ()),
            };
        }
        write_event(
            account.provider,
            account.auth_kind,
            &account.email,
            &credential,
            calendar_source,
            remote,
            input,
        )
        .await
    }

    /// Удалить событие.
    pub async fn delete_event(
        &self,
        account: &Account,
        calendar_source: &str,
        remote_url: &str,
        etag: Option<&str>,
    ) -> Result<()> {
        let credential = self.auxiliary_credential(account).await?;
        if account.provider == Provider::Exchange {
            return self
                .ews_backend(account)?
                .delete_calendar_item(&credential, Self::ews_item_id(remote_url, "ews-event:")?)
                .await;
        }
        delete_event(
            account.provider,
            account.auth_kind,
            &account.email,
            &credential,
            calendar_source,
            remote_url,
            etag,
        )
        .await
    }

    /// Создать или изменить контакт.
    pub async fn write_contact(
        &self,
        account: &Account,
        collection_url: Option<&str>,
        remote: RemoteObject<'_>,
        input: &ContactInput,
    ) -> Result<()> {
        let credential = self.auxiliary_credential(account).await?;
        if account.provider == Provider::Exchange {
            let backend = self.ews_backend(account)?;
            return match remote.remote_url {
                Some(url) => {
                    backend
                        .update_contact_item(
                            &credential,
                            Self::ews_item_id(url, "ews-contact:")?,
                            input,
                        )
                        .await
                }
                None => backend
                    .create_contact_item(&credential, input)
                    .await
                    .map(|_| ()),
            };
        }
        write_contact(
            account.provider,
            account.auth_kind,
            &account.email,
            &credential,
            collection_url,
            remote,
            input,
        )
        .await
    }

    /// Удалить контакт.
    pub async fn delete_contact(
        &self,
        account: &Account,
        remote_url: &str,
        etag: Option<&str>,
    ) -> Result<()> {
        let credential = self.auxiliary_credential(account).await?;
        if account.provider == Provider::Exchange {
            return self
                .ews_backend(account)?
                .delete_contact_item(&credential, Self::ews_item_id(remote_url, "ews-contact:")?)
                .await;
        }
        delete_contact(
            account.provider,
            account.auth_kind,
            &account.email,
            &credential,
            remote_url,
            etag,
        )
        .await
    }

    async fn respond_to_dav_event(
        &self,
        account: &Account,
        calendar_source: &str,
        remote: RemoteObject<'_>,
        event: &crate::model::Event,
        response: crate::model::RsvpResponse,
    ) -> Result<()> {
        let organizer = event.organizer.clone().ok_or_else(|| {
            crate::Error::AccountConfig(
                "у события не указан организатор - отправить ответ некому".into(),
            )
        })?;
        let uid = event
            .uid
            .clone()
            .or_else(|| remote.uid.map(str::to_owned))
            .ok_or_else(|| crate::Error::AccountConfig("у события нет UID".into()))?;
        let attendees = auxiliary::updated_attendees(&event.attendees, &account.email, response);
        // auxiliary_credential (не oauth_access_token) - Яндекс всегда Oauth2,
        // но остальные DAV-провайдеры (iCloud, Mail.ru, generic) обычно идут
        // по паролю/app-password, и oauth_access_token для них упал бы.
        let credential = self.auxiliary_credential(account).await?;
        let input = auxiliary::event_to_input(event, attendees.clone());
        auxiliary::write_event(
            account.provider,
            account.auth_kind,
            &account.email,
            &credential,
            calendar_source,
            remote,
            &input,
        )
        .await?;
        let own = attendees
            .iter()
            .find(|attendee| attendee.email.eq_ignore_ascii_case(&account.email))
            .ok_or_else(|| {
                crate::Error::AccountConfig(
                    "пользователь не найден среди участников события".into(),
                )
            })?;
        let ics = auxiliary::imip_reply_body(&uid, &organizer, event.sequence, own);
        let subject = format!("Re: {}", event.summary);
        let body_text = match response {
            crate::model::RsvpResponse::Accepted => "Приглашение принято.",
            crate::model::RsvpResponse::Declined => "Приглашение отклонено.",
            crate::model::RsvpResponse::Tentative => "Участие пока под вопросом.",
        }
        .to_owned();
        let message = crate::backend::OutgoingMessage {
            from: account.email.clone(),
            to: vec![organizer],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject,
            body_text,
            body_html: None,
            attachments: vec![crate::backend::OutgoingAttachment {
                filename: "reply.ics".into(),
                mime_type: "text/calendar; method=REPLY; charset=UTF-8".into(),
                data: ics.into_bytes(),
            }],
            ..Default::default()
        };
        self.send_outgoing(account.id, message).await
    }

    /// Принять письмо в очередь отправки; поле From задаёт ядро.
    ///
    /// Прямого обращения к серверному модулю здесь больше нет: письмо принято
    /// локально сразу, а передача начинается после окна отмены и повторяется по
    /// правилам очереди операций (specs/undo-send.md, S-001, S-057, S-059).
    pub async fn queue_outgoing(
        &self,
        account_id: i64,
        mut message: crate::backend::OutgoingMessage,
        origin: &str,
        request_key: Option<String>,
    ) -> Result<crate::model::SendQueued> {
        let account = self
            .db
            .list_accounts()
            .await?
            .into_iter()
            .find(|account| account.id == account_id)
            .ok_or_else(|| crate::Error::AccountConfig("аккаунт отправителя не найден".into()))?;
        message.from = account.email.clone();
        // S-011, S-055, S-057: окно отмены получает только обычная отправка
        // пользователя. Служебное письмо, письмо локального интерфейса
        // приложений и отправка по времени уходят без него.
        let undo_seconds = if origin == crate::model::SEND_ORIGIN_ORDINARY {
            self.db.undo_send_seconds().await?
        } else {
            0
        };
        let queued = self
            .db
            .queue_outgoing_send(account.id, message, origin, request_key, undo_seconds)
            .await?;
        tracing::info!(
            account = %crate::logging::mask_email(&account.email),
            operation = queued.operation_id,
            origin,
            undo_seconds,
            "письмо принято в очередь отправки"
        );
        Ok(queued)
    }

    /// Служебное письмо программы: ответ на приглашение календаря и автоответ
    /// уходят тем же путём, что и письмо пользователя (S-057).
    pub async fn send_outgoing(
        &self,
        account_id: i64,
        message: crate::backend::OutgoingMessage,
    ) -> Result<()> {
        self.queue_outgoing(
            account_id,
            message,
            crate::model::SEND_ORIGIN_AUTOMATIC,
            None,
        )
        .await?;
        Ok(())
    }

    /// Построить модуль Exchange отдельной ветвью: общее построение модуля
    /// отдаёт обобщённый объект интерфейса почтового модуля, у которого
    /// операций отсутствия нет вовсе (specs/out-of-office.md, S-010).
    fn exchange_backend(account: &Account) -> Result<crate::backend::EwsBackend> {
        if account.provider != Provider::Exchange {
            return Err(crate::Error::AccountConfig(
                "серверный автоответ доступен только ящику Exchange".into(),
            ));
        }
        Ok(crate::backend::EwsBackend {
            endpoint: account.ews_url.clone().ok_or_else(|| {
                crate::Error::AccountConfig("для Exchange не настроен адрес EWS".into())
            })?,
            username: account
                .username
                .clone()
                .unwrap_or_else(|| account.email.clone()),
            timeouts: crate::backend::EwsTimeouts::background(),
        })
    }

    async fn account_by_id(&self, account_id: i64) -> Result<Account> {
        self.db
            .list_accounts()
            .await?
            .into_iter()
            .find(|account| account.id == account_id)
            .ok_or_else(|| crate::Error::AccountConfig("ящик не найден".into()))
    }

    /// Настройка автоответа выбранного ящика. Для серверного режима состояние
    /// читается с сервера и показывается именно оно (S-011).
    pub async fn out_of_office(
        &self,
        account_id: i64,
    ) -> Result<crate::model::OutOfOfficeSettings> {
        let settings = self.db.out_of_office_settings(account_id).await?;
        if settings.mode != crate::model::OUT_OF_OFFICE_MODE_SERVER || !settings.available {
            return Ok(settings);
        }
        let account = self.account_by_id(account_id).await?;
        match self.read_server_out_of_office(&account).await {
            Ok(()) => self.db.out_of_office_settings(account_id).await,
            Err(error) => {
                // S-018: отказ чтения не подменяется ранее введённым значением
                // и не включает вместо серверного режима локальный.
                self.db
                    .save_out_of_office_error(account_id, &error.to_string())
                    .await?;
                self.db.out_of_office_settings(account_id).await
            }
        }
    }

    async fn read_server_out_of_office(&self, account: &Account) -> Result<()> {
        let credential = self.mail_credential(account).await?;
        let backend = Self::exchange_backend(account)?;
        let state = backend
            .read_oof_settings(&credential, &account.email)
            .await?;
        self.db
            .save_server_out_of_office_state(
                account.id,
                &server_state_input(account.id, &state),
                None,
            )
            .await
    }

    /// Сохранить настройку автоответа. Режим выбирает программа: ящик Exchange
    /// хранит её на сервере, остальные ящики - у себя (S-002, S-003, S-005).
    pub async fn save_out_of_office(
        &self,
        input: crate::model::OutOfOfficeInput,
    ) -> Result<crate::model::OutOfOfficeSettings> {
        let current = self.db.out_of_office_settings(input.account_id).await?;
        if !current.available {
            return Err(crate::Error::AccountConfig(
                current
                    .unavailable_reason
                    .unwrap_or_else(|| "автоответ для этого ящика недоступен".into()),
            ));
        }
        if current.mode != crate::model::OUT_OF_OFFICE_MODE_SERVER {
            return self.db.save_local_out_of_office(&input).await;
        }
        let account = self.account_by_id(input.account_id).await?;
        let credential = self.mail_credential(&account).await?;
        let backend = Self::exchange_backend(&account)?;
        let state = crate::backend::OofState {
            enabled: input.enabled,
            starts_at: utc_stamp(&input.starts_at),
            ends_at: utc_stamp(&input.ends_at),
            internal_text: input.internal_text.trim().to_owned(),
            external_text: input.external_text.trim().to_owned(),
        };
        // S-016, S-018: успех объявляется только по перечитанному состоянию, а
        // отказ записи показанное состояние не меняет.
        match backend
            .write_oof_settings(&credential, &account.email, &state)
            .await
        {
            Ok(confirmed) => {
                self.db
                    .save_server_out_of_office_state(
                        account.id,
                        &server_state_input(account.id, &confirmed),
                        None,
                    )
                    .await?;
                self.db.out_of_office_settings(account.id).await
            }
            Err(error) => {
                self.db
                    .save_out_of_office_error(account.id, &error.to_string())
                    .await?;
                Err(error)
            }
        }
    }

    /// Ждать серверное изменение через механизм выбранного транспорта.
    pub async fn wait_for_mail_change(&self, account: &Account) -> Result<()> {
        let credential = self.mail_credential(account).await?;
        Self::mail_backend(account)?
            .wait_for_change(&account.email, &credential)
            .await
    }

    /// Доставить накопленные локальные операции с ограниченным retry/backoff.
    pub async fn process_mail_outbox(&self, account: &Account) -> Result<usize> {
        if account.provider == Provider::Exchange
            && !self
                .exchange_outbox_repaired
                .lock()
                .await
                .contains(&account.id)
        {
            self.db
                .requeue_exchange_change_key_operations(account.id)
                .await?;
            self.exchange_outbox_repaired
                .lock()
                .await
                .insert(account.id);
        }
        // S-025: письма передаются по одному, и письмо, ещё не дошедшее до
        // своей очереди, остаётся ожидающим и доступным для отмены.
        let mut completed = 0;
        while let Some(operation) = self.db.claim_send_operation(account.id).await? {
            completed += self.transmit_queued_message(account, &operation).await?;
        }
        let operations = self.db.claim_outbox_operations(account.id, 50).await?;
        if operations.is_empty() {
            return Ok(completed);
        }
        // Не трогаем transport/OAuth/quota gate, когда отправлять нечего.
        let token = self.mail_credential(account).await?;
        let backend = Self::mail_backend(account)?;
        for operation in operations {
            let applied = if operation.op_kind == "append_sent" {
                match self.sent_append_bytes_of(&operation).await {
                    Ok(raw) => backend.append_sent(&account.email, &token, &raw).await,
                    Err(error) => Err(error),
                }
            } else {
                backend
                    .apply_operation(
                        &account.email,
                        &token,
                        &operation.op_kind,
                        &operation.payload,
                    )
                    .await
            };
            match applied {
                Ok(()) => {
                    self.db.complete_outbox_operation(&operation).await?;
                    completed += 1;
                }
                Err(error) => {
                    // S-084: текст ошибки сервера маскируется и в журнале, и в
                    // поле последней ошибки очереди - он повторяет адреса и
                    // тему письма.
                    let reason = crate::logging::mask_error_text(&error.to_string());
                    self.db.fail_outbox_operation(operation.id, &reason).await?;
                    tracing::warn!(
                        account = %crate::logging::mask_email(&account.email),
                        operation = operation.id,
                        attempts = operation.attempts + 1,
                        last_error = %reason,
                        "операция outbox будет повторена"
                    );
                }
            }
        }
        Ok(completed)
    }

    /// Передать одно письмо очереди отправки серверу.
    ///
    /// Отказ, случившийся после захвата операции, наверх не уходит: операция,
    /// оставленная в состоянии передачи, дождалась бы следующего запуска и была
    /// бы объявлена там неопределённым итогом, хотя серверу письма никто не
    /// передавал (specs/undo-send.md, S-027, S-048).
    async fn transmit_queued_message(
        &self,
        account: &Account,
        operation: &crate::storage::repo::OutboxOperation,
    ) -> Result<usize> {
        let token = match self.mail_credential(account).await {
            Ok(token) => token,
            Err(error) => {
                // S-028, S-048: отказ учётных данных и недоступность сети не
                // сжигают восемь попыток на заведомо неверный пароль и не
                // теряют письмо при суточном отсутствии связи.
                defer_send(&self.db, operation.id, &error).await;
                return Ok(0);
            }
        };
        let backend = match Self::mail_backend(account) {
            Ok(backend) => backend,
            Err(error) => {
                defer_send(&self.db, operation.id, &error).await;
                return Ok(0);
            }
        };
        Ok(transmit_with_backend(
            &self.db,
            &account.email,
            backend.as_ref(),
            &token,
            operation,
        )
        .await)
    }

    /// Точные байты письма для дозаписи копии: новые операции хранят их в
    /// хранилище больших объектов, прежние - строкой в данных операции.
    async fn sent_append_bytes_of(
        &self,
        operation: &crate::storage::repo::OutboxOperation,
    ) -> Result<Vec<u8>> {
        match serde_json::from_str::<crate::model::SendPayload>(&operation.payload) {
            Ok(payload) if payload.raw_ref.is_some() => self.db.sent_append_bytes(&payload),
            _ => sent_append_raw(&operation.payload),
        }
    }

    /// Предел проверки учётных данных на уже определённом сервере -
    /// account-connect-progress.md, S-013: 45 секунд независимо от
    /// транспорта (IMAP, EWS, JMAP используют одну и ту же функцию).
    const CREDENTIAL_CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

    /// Предел автопоиска адреса сервера Exchange - account-connect-progress.md,
    /// S-012: 60 секунд на перебор адресов и схем входа.
    const EWS_DISCOVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

    /// Общая проверка учётных данных с пределом времени (S-013): значение
    /// вида ошибки не переопределяется по тексту - истечение предела всегда
    /// даёт `timeout` (error-kinds-and-messages.md).
    async fn validate_with_timeout(
        backend: &dyn MailBackend,
        email: &str,
        credential: &str,
    ) -> Result<()> {
        match tokio::time::timeout(
            Self::CREDENTIAL_CHECK_TIMEOUT,
            backend.validate(email, credential),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(crate::Error::classified_backend(
                "connect",
                ErrorKind::Timeout,
                "проверка учётных данных заняла слишком много времени",
            )),
        }
    }

    /// Подключить обычный IMAP/SMTP-аккаунт по паролю приложения. Пароль
    /// проверяется на сервере и хранится только в системном keychain.
    pub async fn add_password_imap(
        &self,
        email: &str,
        display_name: &str,
        username: &str,
        password: &str,
        config: &ProviderConfig,
    ) -> Result<ConnectedAccountSync> {
        if password.is_empty() {
            return Err(crate::Error::AccountConfig("пароль не указан".into()));
        }
        let imap = config.imap.clone().ok_or_else(|| {
            crate::Error::AccountConfig("сервер IMAP не найден; укажите его вручную".into())
        })?;
        let backend = GenericImapBackend {
            username: username.to_owned(),
            imap: imap.clone(),
            smtp: config.smtp.clone(),
        };
        Self::validate_with_timeout(&backend, email, password).await?;

        let secret_ref = format!("mail-password:{}", email.to_lowercase());
        let previous_secret_ref = self.existing_secret_ref(email).await;
        let entry = keyring::Entry::new("truemail", &secret_ref)
            .map_err(|error| crate::Error::Keyring(error.to_string()))?;
        entry
            .set_password(password)
            .map_err(|error| crate::Error::Keyring(error.to_string()))?;
        let account = match self
            .db
            .save_account(&NewAccount {
                email: email.to_owned(),
                display_name: display_name.to_owned(),
                provider: config.provider,
                backend_kind: BackendKind::Imap,
                auth_kind: config.auth_kind,
                imap: Some(imap),
                smtp: config.smtp.clone(),
                ews_url: None,
                caldav_url: None,
                carddav_url: None,
                jmap_url: None,
                username: Some(username.to_owned()),
                secret_ref: secret_ref.clone(),
                color: Some("#3F7C85".into()),
            })
            .await
        {
            Ok(account) => account,
            Err(error) => {
                let _ = entry.delete_credential();
                return Err(error);
            }
        };
        Self::cleanup_stale_secret(previous_secret_ref, &secret_ref);
        Ok(ConnectedAccountSync {
            account,
            mail_folders: 0,
            calendars: 0,
            events: 0,
            contacts: 0,
            warnings: if config.smtp.is_none() {
                vec![SyncWarning::classified(
                    ErrorKind::AccountConfig,
                    "SMTP-сервер не найден: чтение работает, отправку нужно настроить",
                    Some("smtp"),
                )]
            } else {
                Vec::new()
            },
        })
    }

    /// Подключить on-premises Exchange через Autodiscover и EWS. WinHTTP
    /// выполняет Negotiate с откатом на NTLM; пароль остаётся в keychain.
    pub async fn add_exchange_ews(
        &self,
        email: &str,
        display_name: &str,
        username: &str,
        password: &str,
        server_hint: Option<&str>,
    ) -> Result<ConnectedAccountSync> {
        if password.is_empty() {
            return Err(crate::Error::AccountConfig("пароль не указан".into()));
        }
        // S-012: автопоиск адреса EWS ограничен 60 секундами - часть общего
        // предела в 120 секунд, не сверх него. Одно HTTP-соединение внутри
        // автопоиска использует сниженные пределы EwsTimeouts::connect() (S-014).
        let endpoint = match tokio::time::timeout(
            Self::EWS_DISCOVERY_TIMEOUT,
            crate::backend::discover_ews_url(
                email,
                username,
                password,
                server_hint,
                EwsTimeouts::connect(),
            ),
        )
        .await
        {
            Ok(result) => result?,
            Err(_) => {
                return Err(crate::Error::classified_backend(
                    "ews",
                    ErrorKind::Timeout,
                    "автопоиск адреса Exchange занял слишком много времени",
                ));
            }
        };
        let backend = EwsBackend {
            endpoint: endpoint.clone(),
            username: username.to_owned(),
            timeouts: EwsTimeouts::connect(),
        };
        Self::validate_with_timeout(&backend, email, password).await?;
        let secret_ref = format!("exchange-password:{}", email.to_lowercase());
        let previous_secret_ref = self.existing_secret_ref(email).await;
        let entry = keyring::Entry::new("truemail", &secret_ref)
            .map_err(|error| crate::Error::Keyring(error.to_string()))?;
        entry
            .set_password(password)
            .map_err(|error| crate::Error::Keyring(error.to_string()))?;
        let account = match self
            .db
            .save_account(&NewAccount {
                email: email.to_owned(),
                display_name: display_name.to_owned(),
                provider: Provider::Exchange,
                backend_kind: BackendKind::Ews,
                auth_kind: AuthKind::Ntlm,
                imap: None,
                smtp: None,
                ews_url: Some(endpoint),
                caldav_url: None,
                carddav_url: None,
                jmap_url: None,
                username: Some(username.to_owned()),
                secret_ref: secret_ref.clone(),
                color: Some("#0078D4".into()),
            })
            .await
        {
            Ok(account) => account,
            Err(error) => {
                let _ = entry.delete_credential();
                return Err(error);
            }
        };
        Self::cleanup_stale_secret(previous_secret_ref, &secret_ref);
        Ok(ConnectedAccountSync {
            account,
            mail_folders: 0,
            calendars: 0,
            events: 0,
            contacts: 0,
            warnings: Vec::new(),
        })
    }

    /// Подключить JMAP-сервер по отдельному паролю приложения.
    pub async fn add_jmap_password(
        &self,
        email: &str,
        display_name: &str,
        username: &str,
        password: &str,
        session_url: &str,
    ) -> Result<ConnectedAccountSync> {
        if password.is_empty() {
            return Err(crate::Error::AccountConfig("пароль не указан".into()));
        }
        let backend = JmapBackend {
            session_url: session_url.trim().to_owned(),
            username: username.to_owned(),
        };
        Self::validate_with_timeout(&backend, email, password).await?;
        let secret_ref = format!("jmap-password:{}", email.to_lowercase());
        let previous_secret_ref = self.existing_secret_ref(email).await;
        let entry = keyring::Entry::new("truemail", &secret_ref)
            .map_err(|error| crate::Error::Keyring(error.to_string()))?;
        entry
            .set_password(password)
            .map_err(|error| crate::Error::Keyring(error.to_string()))?;
        let account = match self
            .db
            .save_account(&NewAccount {
                email: email.to_owned(),
                display_name: display_name.to_owned(),
                provider: Provider::Generic,
                backend_kind: BackendKind::Jmap,
                auth_kind: AuthKind::AppPassword,
                imap: None,
                smtp: None,
                ews_url: None,
                caldav_url: None,
                carddav_url: None,
                jmap_url: Some(session_url.trim().to_owned()),
                username: Some(username.to_owned()),
                secret_ref: secret_ref.clone(),
                color: Some("#6B5DD3".into()),
            })
            .await
        {
            Ok(account) => account,
            Err(error) => {
                let _ = entry.delete_credential();
                return Err(error);
            }
        };
        Self::cleanup_stale_secret(previous_secret_ref, &secret_ref);
        Ok(ConnectedAccountSync {
            account,
            mail_folders: 0,
            calendars: 0,
            events: 0,
            contacts: 0,
            warnings: Vec::new(),
        })
    }

    /// Сохранить авторизованный аккаунт Яндекса. OAuth-токены никогда не попадают в SQLite.
    pub async fn add_yandex_oauth(
        &self,
        email: &str,
        display_name: &str,
        token: OAuthToken,
    ) -> Result<ConnectedAccountSync> {
        let access_token = Zeroizing::new(token.access_token.clone());
        let secret_ref = format!("yandex-oauth:{}", email.to_lowercase());
        let previous_secret_ref = self.existing_secret_ref(email).await;
        let entry = keyring::Entry::new("truemail", &secret_ref)
            .map_err(|e| crate::Error::Keyring(e.to_string()))?;
        let credential = StoredOAuthCredential::from(token);
        let serialized = Zeroizing::new(serde_json::to_string(&credential)?);
        entry
            .set_password(&serialized)
            .map_err(|e| crate::Error::Keyring(e.to_string()))?;

        let account = match self
            .db
            .save_account(&NewAccount {
                email: email.to_owned(),
                display_name: display_name.to_owned(),
                provider: Provider::Yandex,
                backend_kind: BackendKind::Imap,
                auth_kind: AuthKind::Oauth2,
                imap: Some(ServerConfig {
                    host: "imap.yandex.com".into(),
                    port: 993,
                    security: Security::Ssl,
                }),
                smtp: Some(ServerConfig {
                    host: "smtp.yandex.com".into(),
                    port: 465,
                    security: Security::Ssl,
                }),
                ews_url: None,
                caldav_url: None,
                carddav_url: None,
                jmap_url: None,
                username: Some(email.to_owned()),
                secret_ref: secret_ref.clone(),
                color: Some("#5B63D3".into()),
            })
            .await
        {
            Ok(account) => account,
            Err(error) => {
                let _ = entry.delete_credential();
                return Err(error);
            }
        };
        Self::cleanup_stale_secret(previous_secret_ref, &secret_ref);

        // Код уже обменян и одноразовый, поэтому токен сначала надёжно
        // сохраняется. Проверки доступа быстрые; их временный сбой становится
        // предупреждением и не заставляет пользователя получать новый код.
        let dav_auth =
            dav::DavAuth::new(dav::DavAuthScheme::BasicToken, email, access_token.as_str());
        let (mail_access, dav_access) = tokio::join!(
            YandexBackend.validate(email, &access_token),
            validate_dav(&dav_auth, YANDEX_CALDAV_BASE, YANDEX_CARDDAV_BASE)
        );
        let mut warnings = Vec::new();
        if let Err(error) = mail_access {
            warnings.push(SyncWarning::from_error(
                "Проверка доступа к почте",
                &error,
                None,
            ));
        }
        if let Err(error) = dav_access {
            warnings.push(SyncWarning::from_error(
                "Проверка календаря и контактов",
                &error,
                None,
            ));
        }

        Ok(ConnectedAccountSync {
            account,
            mail_folders: 0,
            calendars: 0,
            events: 0,
            contacts: 0,
            warnings,
        })
    }

    /// Сохранить Gmail-аккаунт после desktop OAuth PKCE. Токены остаются в keychain.
    pub async fn add_gmail_oauth(
        &self,
        email: &str,
        display_name: &str,
        token: OAuthToken,
    ) -> Result<ConnectedAccountSync> {
        tracing::info!(email = %crate::logging::mask_email(email), scope = ?token.scope, "Gmail OAuth: провайдер вернул scope");
        if let Some(granted) = token.scope.as_deref() {
            let granted: std::collections::HashSet<_> = granted.split_whitespace().collect();
            let missing: Vec<_> = GOOGLE_SCOPES
                .split_whitespace()
                .filter(|scope| !granted.contains(scope))
                .collect();
            if !missing.is_empty() {
                tracing::warn!(email = %crate::logging::mask_email(email), missing = ?missing, "Gmail OAuth: Google выдал не все запрошенные scope");
                return Err(crate::Error::AccountConfig(format!(
                    "Google не выдал все разрешения truemail. Повторите подключение и подтвердите доступ к Gmail, Календарю, Контактам и Задачам. Не выданы: {}",
                    missing.join(", ")
                )));
            }
        } else {
            tracing::warn!(
                email = %crate::logging::mask_email(email),
                "Gmail OAuth: провайдер не вернул поле scope, проверку разрешений пропускаем"
            );
        }
        let access_token = Zeroizing::new(token.access_token.clone());
        let secret_ref = format!("google-oauth:{}", email.to_lowercase());
        let previous_secret_ref = self.existing_secret_ref(email).await;
        let entry = keyring::Entry::new("truemail", &secret_ref)
            .map_err(|e| crate::Error::Keyring(e.to_string()))?;
        let credential = StoredOAuthCredential::from(token);
        let serialized = Zeroizing::new(serde_json::to_string(&credential)?);
        entry
            .set_password(&serialized)
            .map_err(|e| crate::Error::Keyring(e.to_string()))?;

        let account = match self
            .db
            .save_account(&NewAccount {
                email: email.to_owned(),
                display_name: display_name.to_owned(),
                provider: Provider::Gmail,
                backend_kind: BackendKind::Imap,
                auth_kind: AuthKind::Oauth2,
                imap: Some(ServerConfig {
                    host: "imap.gmail.com".into(),
                    port: 993,
                    security: Security::Ssl,
                }),
                smtp: Some(ServerConfig {
                    host: "smtp.gmail.com".into(),
                    port: 465,
                    security: Security::Ssl,
                }),
                ews_url: None,
                caldav_url: None,
                carddav_url: None,
                jmap_url: None,
                username: Some(email.to_owned()),
                secret_ref: secret_ref.clone(),
                color: Some("#4285F4".into()),
            })
            .await
        {
            Ok(account) => account,
            Err(error) => {
                let _ = entry.delete_credential();
                return Err(error);
            }
        };
        Self::cleanup_stale_secret(previous_secret_ref, &secret_ref);

        let mut warnings = Vec::new();
        if let Err(error) = GmailBackend.validate(email, &access_token).await {
            warnings.push(SyncWarning::from_error(
                "Проверка доступа к Gmail",
                &error,
                None,
            ));
        }
        Ok(ConnectedAccountSync {
            account,
            mail_folders: 0,
            calendars: 0,
            events: 0,
            contacts: 0,
            warnings,
        })
    }

    /// Сохранить Outlook/Exchange Online после Microsoft desktop OAuth PKCE.
    pub async fn add_outlook_oauth(
        &self,
        email: &str,
        display_name: &str,
        token: OAuthToken,
    ) -> Result<ConnectedAccountSync> {
        if let Some(granted) = token.scope.as_deref() {
            let granted: std::collections::HashSet<_> = granted.split_whitespace().collect();
            let missing: Vec<_> = MICROSOFT_SCOPES
                .split_whitespace()
                .filter(|scope| !granted.contains(scope))
                .collect();
            if !missing.is_empty() {
                return Err(crate::Error::AccountConfig(format!(
                    "Microsoft не выдал все разрешения для почты. Не выданы: {}",
                    missing.join(", ")
                )));
            }
        }
        let access_token = Zeroizing::new(token.access_token.clone());
        let secret_ref = format!("microsoft-oauth:{}", email.to_lowercase());
        let previous_secret_ref = self.existing_secret_ref(email).await;
        let entry = keyring::Entry::new("truemail", &secret_ref)
            .map_err(|error| crate::Error::Keyring(error.to_string()))?;
        let credential = StoredOAuthCredential::from(token);
        let serialized = Zeroizing::new(serde_json::to_string(&credential)?);
        entry
            .set_password(&serialized)
            .map_err(|error| crate::Error::Keyring(error.to_string()))?;

        let account = match self
            .db
            .save_account(&NewAccount {
                email: email.to_owned(),
                display_name: display_name.to_owned(),
                provider: Provider::Outlook,
                backend_kind: BackendKind::Imap,
                auth_kind: AuthKind::Oauth2,
                imap: Some(ServerConfig {
                    host: "outlook.office365.com".into(),
                    port: 993,
                    security: Security::Ssl,
                }),
                smtp: Some(ServerConfig {
                    host: "smtp.office365.com".into(),
                    port: 587,
                    security: Security::Starttls,
                }),
                ews_url: None,
                caldav_url: None,
                carddav_url: None,
                jmap_url: None,
                username: Some(email.to_owned()),
                secret_ref: secret_ref.clone(),
                color: Some("#0078D4".into()),
            })
            .await
        {
            Ok(account) => account,
            Err(error) => {
                let _ = entry.delete_credential();
                return Err(error);
            }
        };
        Self::cleanup_stale_secret(previous_secret_ref, &secret_ref);

        let mut warnings = Vec::new();
        if let Err(error) = OutlookBackend.validate(email, &access_token).await {
            warnings.push(SyncWarning::from_error(
                "Проверка доступа к Outlook",
                &error,
                None,
            ));
        }
        Ok(ConnectedAccountSync {
            account,
            mail_folders: 0,
            calendars: 0,
            events: 0,
            contacts: 0,
            warnings,
        })
    }

    /// Тихая смена пароля (accounts-accordion-password.md): проверяет новый
    /// пароль на сервере тем же способом, что и подключение, и заменяет
    /// только значение в keyring - без upsert строки аккаунта и без
    /// синхронизации (S-013, S-015). Контракт минимален (S-010): адрес,
    /// `secret_ref` и серверные настройки берутся из сохранённого аккаунта.
    pub async fn change_account_password(
        &self,
        account_id: i64,
        new_password: &str,
    ) -> std::result::Result<(), ChangePasswordError> {
        // Своя копия пароля внутри ядра живёт в обнуляемом буфере: вызывающая
        // сторона затирает свою строку сама, а эта копия переживает весь путь
        // проверки и записи (S-018).
        self.change_account_password_with_store(
            account_id,
            Zeroizing::new(new_password.to_owned()),
            &SystemSecretStore,
        )
        .await
    }

    /// То же самое, но с подменяемым хранилищем секретов - используется в
    /// тестах на подменённом хранилище (S-014) без обращения к реальному OS
    /// keychain, которого может не быть на CI-раннере.
    pub async fn change_account_password_with_store(
        &self,
        account_id: i64,
        new_password: Zeroizing<String>,
        store: &dyn SecretStore,
    ) -> std::result::Result<(), ChangePasswordError> {
        if !self.password_change_locks.try_lock(account_id).await {
            return Err(ChangePasswordError::ChangeInProgress);
        }
        let result = self
            .change_account_password_inner(account_id, &new_password, store)
            .await;
        // Блокировка снимается при любом исходе: иначе после первой же смены
        // аккаунт остался бы занятым навсегда и следующая попытка отвечала бы
        // "смена уже идёт" до перезапуска программы (S-017).
        self.password_change_locks.unlock(account_id).await;
        result
    }

    async fn change_account_password_inner(
        &self,
        account_id: i64,
        new_password: &Zeroizing<String>,
        store: &dyn SecretStore,
    ) -> std::result::Result<(), ChangePasswordError> {
        let accounts_before = self
            .db
            .list_accounts()
            .await
            .map_err(|error| ChangePasswordError::BackendUnavailable(error.to_string()))?;
        let account = accounts_before
            .into_iter()
            .find(|account| account.id == account_id)
            .ok_or(ChangePasswordError::AccountNotFound)?;
        if !matches!(
            account.auth_kind,
            AuthKind::Password | AuthKind::AppPassword | AuthKind::Ntlm
        ) {
            return Err(ChangePasswordError::UnsupportedAuthKind);
        }
        let secret_ref = account
            .secret_ref
            .clone()
            .ok_or(ChangePasswordError::MissingSecretRef)?;

        // Проверка на сервере тем же способом, что и при подключении (S-011):
        // IMAP - вход и NOOP, EWS - GetFolder входящих, JMAP - Session.
        let backend = Self::mail_backend(&account)
            .map_err(|error| ChangePasswordError::BackendUnavailable(error.to_string()))?;
        if let Err(error) = backend.validate(&account.email, new_password).await {
            let invalid_credentials = error.requires_reauth()
                || matches!(&error, crate::Error::Backend { .. })
                    && classify_validation_error(&error.to_string());
            let message = error.to_string();
            tracing::info!(
                account = %crate::logging::mask_email(&account.email),
                "смена пароля: сервер отклонил новый пароль"
            );
            return Err(if invalid_credentials {
                ChangePasswordError::InvalidCredentials(message)
            } else {
                ChangePasswordError::BackendUnavailable(message)
            });
        }

        // Между проверкой и записью пользователь мог переподключить аккаунт
        // через мастер (S-042) - конфигурация перечитывается перед записью.
        let accounts_now = self
            .db
            .list_accounts()
            .await
            .map_err(|error| ChangePasswordError::BackendUnavailable(error.to_string()))?;
        let current = accounts_now
            .into_iter()
            .find(|item| item.id == account_id)
            .ok_or(ChangePasswordError::AccountNotFound)?;
        if !connection_config_matches(&account, &current) {
            return Err(ChangePasswordError::AccountChanged);
        }

        // Старое значение - только для сравнения при контрольном чтении;
        // отсутствие записи или нечитаемый секрет смене не мешают (S-041).
        let old_value = store.read(&secret_ref);
        let wrote = store.write(&secret_ref, new_password.as_str());
        if !wrote {
            tracing::warn!(
                account = %crate::logging::mask_email(&account.email),
                "хранилище секретов сообщило об ошибке записи, решает контрольное чтение"
            );
        }
        let read_back = store.read(&secret_ref);

        // Итог определяется контрольным чтением, а не тем, что вернул сам
        // write() (S-014): keyring на некоторых платформах сообщает об
        // ошибке уже после фактически состоявшейся записи.
        match read_back {
            Some(ref value) if value.as_str() == new_password.as_str() => {
                tracing::info!(
                    account = %crate::logging::mask_email(&account.email),
                    "пароль аккаунта заменён в хранилище секретов"
                );
                Ok(())
            }
            Some(ref value)
                if old_value
                    .as_ref()
                    .is_some_and(|old| old.as_str() == value.as_str()) =>
            {
                tracing::warn!(
                    account = %crate::logging::mask_email(&account.email),
                    "смена пароля: запись в хранилище секретов не удалась"
                );
                Err(ChangePasswordError::SecretStoreWriteFailed)
            }
            _ => {
                tracing::warn!(
                    account = %crate::logging::mask_email(&account.email),
                    "смена пароля: состояние хранилища секретов не определено"
                );
                Err(ChangePasswordError::SecretStoreStateUnknown)
            }
        }
    }

    /// Полная синхронизация уже сохранённого аккаунта; предназначена для фоновой задачи.
    pub async fn sync_mail_account(&self, account: &Account) -> Result<ConnectedAccountSync> {
        self.sync_registry
            .exclusive(
                account.id,
                SyncKind::Mail,
                self.sync_mail_account_inner(account, false),
            )
            .await
    }

    /// Первый проход сразу после подключения отличается только формулировкой
    /// предупреждения. Обычные фоновые проходы не должны называться первыми.
    pub async fn sync_initial_mail_account(
        &self,
        account: &Account,
    ) -> Result<ConnectedAccountSync> {
        self.sync_registry
            .exclusive(
                account.id,
                SyncKind::Mail,
                self.sync_mail_account_inner(account, true),
            )
            .await
    }

    async fn sync_mail_account_inner(
        &self,
        account: &Account,
        initial: bool,
    ) -> Result<ConnectedAccountSync> {
        let access_token = self.mail_credential(account).await?;
        let backend = Self::mail_backend(account)?;
        let cursors = self.db.folder_sync_cursors(account.id).await?;
        let mut warnings = Vec::new();
        // Имена и счётчики папок появляются в UI сразу, пока тела писем и DAV
        // коллекции загружаются параллельно.
        if let Ok(folders) = backend
            .discover_folders(&account.email, &access_token)
            .await
            && let Err(error) = self.db.save_discovered_folders(account.id, &folders).await
        {
            warnings.push(SyncWarning::from_error(
                "Папки почты не сохранились",
                &error,
                None,
            ));
        }
        let imap_result = backend
            .discover(
                &account.email,
                &access_token,
                &cursors,
                account.retention_days,
            )
            .await;
        let mail_folders = match imap_result {
            Ok(imap) => {
                // Обрыв связи посреди обхода папок (imap-reconnect-resilience.md).
                apply_skipped_folders(
                    &imap.skipped_folders,
                    imap.folders.len(),
                    &mut warnings,
                )?;
                let saved = match self
                    .db
                    .save_discovered_folders(account.id, &imap.folders)
                    .await
                {
                    Ok(()) => {
                        match self
                            .db
                            .reconcile_imap_snapshot(
                                account.id,
                                &imap.server_uids,
                                &imap.reset_folders,
                            )
                            .await
                        {
                            Ok(_) => {
                                if let Err(error) = self
                                    .db
                                    .reconcile_discovered_folders(account.id, &imap.folders)
                                    .await
                                {
                                    warnings.push(SyncWarning::from_error(
                                        "Удалённые папки не очищены",
                                        &error,
                                        None,
                                    ));
                                }
                                if let Err(error) = self
                                    .db
                                    .apply_imap_vanished(account.id, &imap.deleted_uids)
                                    .await
                                {
                                    warnings.push(SyncWarning::from_error(
                                        "Удаления IMAP не сохранились",
                                        &error,
                                        None,
                                    ));
                                }
                                if let Err(error) = self
                                    .db
                                    .apply_imap_flag_updates(account.id, &imap.flag_updates)
                                    .await
                                {
                                    warnings.push(SyncWarning::from_error(
                                        "Флаги писем не сохранились",
                                        &error,
                                        None,
                                    ));
                                }
                                match self
                                    .db
                                    .reconcile_remote_projections(
                                        account.id,
                                        &imap.messages,
                                        &imap.changed_remote_ids,
                                        imap.remote_snapshot.as_deref(),
                                    )
                                    .await
                                {
                                    Ok(_) => {
                                        match self
                                            .db
                                            .save_discovered_messages(
                                                account.id,
                                                &imap.messages,
                                                false,
                                            )
                                            .await
                                        {
                                            Ok(()) => {
                                                match self
                                                    .db
                                                    .save_folder_sync_tokens(
                                                        account.id,
                                                        &imap.folders,
                                                    )
                                                    .await
                                                {
                                                    Ok(()) => {
                                                        if let Err(error) = self
                                                            .db
                                                            .process_sync_batch_stages()
                                                            .await
                                                        {
                                                            tracing::warn!(%error, "правила обработки будут повторены при следующей синхронизации");
                                                        }
                                                        Ok(())
                                                    }
                                                    Err(error) => Err(error),
                                                }
                                            }
                                            Err(error) => Err(error),
                                        }
                                    }
                                    Err(error) => Err(error),
                                }
                            }
                            Err(error) => Err(error),
                        }
                    }
                    Err(error) => Err(error),
                };
                match saved {
                    Ok(()) => imap.folders.len(),
                    Err(error) => {
                        warnings.push(SyncWarning::from_error(
                            "Почта загружена, но не сохранилась",
                            &error,
                            None,
                        ));
                        0
                    }
                }
            }
            Err(error) => {
                self.remember_gmail_rate_limit(account, &error).await;
                warnings.push(SyncWarning::from_error(
                    "",
                    &error,
                    Some(mail_sync_phase(initial)),
                ));
                0
            }
        };
        Ok(ConnectedAccountSync {
            account: account.clone(),
            mail_folders,
            calendars: 0,
            events: 0,
            contacts: 0,
            warnings,
        })
    }
}

#[cfg(test)]
mod change_password_tests {
    //! Проверки тихой смены пароля (accounts-accordion-password.md). Сервер
    //! мокается локальным JMAP-эндпоинтом (тот же приём, что и в
    //! backend/jmap.rs), хранилище секретов - MockSecretStore (S-014):
    //! CI-раннер не всегда имеет системный keychain.
    use super::*;
    use axum::{
        Json, Router,
        extract::State,
        http::{HeaderMap, StatusCode},
        response::{IntoResponse, Response},
        routing::get,
    };
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[derive(Clone, Copy, Debug)]
    enum ReadAfterWrite {
        NewValue,
        OldValue,
        Unreadable,
        Other,
    }

    /// Хранилище секретов, полностью управляемое тестом: что вернуть на
    /// write() и что вернуть на read() после записи - независимые ручки, как
    /// того требует таблица исходов S-014.
    struct MockSecretStore {
        initial: std::collections::HashMap<String, String>,
        write_returns: bool,
        read_after_write: ReadAfterWrite,
        wrote: AtomicBool,
        writes: Mutex<Vec<(String, String)>>,
    }
    impl MockSecretStore {
        fn new(
            initial: &[(&str, &str)],
            write_returns: bool,
            read_after_write: ReadAfterWrite,
        ) -> Self {
            Self {
                initial: initial
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
                write_returns,
                read_after_write,
                wrote: AtomicBool::new(false),
                writes: Mutex::new(Vec::new()),
            }
        }
        fn write_count(&self) -> usize {
            self.writes.lock().unwrap().len()
        }
    }
    impl SecretStore for MockSecretStore {
        fn read(&self, secret_ref: &str) -> Option<Zeroizing<String>> {
            let value = if self.wrote.load(Ordering::SeqCst) {
                match self.read_after_write {
                    ReadAfterWrite::NewValue => {
                        self.writes.lock().unwrap().last().map(|(_, v)| v.clone())
                    }
                    ReadAfterWrite::OldValue => self.initial.get(secret_ref).cloned(),
                    ReadAfterWrite::Unreadable => None,
                    ReadAfterWrite::Other => Some("garbage-value-neither-old-nor-new".into()),
                }
            } else {
                self.initial.get(secret_ref).cloned()
            };
            value.map(Zeroizing::new)
        }
        fn write(&self, secret_ref: &str, value: &str) -> bool {
            self.writes
                .lock()
                .unwrap()
                .push((secret_ref.to_owned(), value.to_owned()));
            self.wrote.store(true, Ordering::SeqCst);
            self.write_returns
        }
    }

    fn basic_auth_password(headers: &HeaderMap) -> Option<String> {
        let value = headers
            .get(axum::http::header::AUTHORIZATION)?
            .to_str()
            .ok()?;
        let encoded = value.strip_prefix("Basic ")?;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .ok()?;
        let text = String::from_utf8(decoded).ok()?;
        text.split_once(':')
            .map(|(_, password)| password.to_owned())
    }

    fn jmap_session_body(base: &str) -> serde_json::Value {
        serde_json::json!({
            "capabilities": {"urn:ietf:params:jmap:core": {}, "urn:ietf:params:jmap:mail": {}},
            "accounts": {"a1": {"name": "Test", "isPersonal": true, "isReadOnly": false, "accountCapabilities": {"urn:ietf:params:jmap:mail": {}}}},
            "primaryAccounts": {"urn:ietf:params:jmap:mail": "a1"},
            "username": "user@example.test",
            "apiUrl": format!("{base}/api"),
            "downloadUrl": format!("{base}/download/{{accountId}}/{{blobId}}/{{name}}?accept={{type}}"),
            "uploadUrl": format!("{base}/upload/{{accountId}}"),
            "state": "session-1"
        })
    }

    #[derive(Clone)]
    struct JmapAuthState {
        base: String,
        expected_password: String,
    }

    /// Обычный мок: Session отдаётся только с верным паролем, иначе 401 -
    /// именно так `JmapBackend::validate` отличает верный пароль от неверного.
    async fn mock_jmap_session(State(state): State<JmapAuthState>, headers: HeaderMap) -> Response {
        if basic_auth_password(&headers).as_deref() != Some(state.expected_password.as_str()) {
            return (StatusCode::UNAUTHORIZED, "invalid credentials").into_response();
        }
        Json(jmap_session_body(&state.base)).into_response()
    }

    async fn spawn_mock_jmap(expected_password: &str) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let state = JmapAuthState {
            base: base.clone(),
            expected_password: expected_password.to_owned(),
        };
        let app = Router::new()
            .route("/.well-known/jmap", get(mock_jmap_session))
            .with_state(state);
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (base, server)
    }

    /// Тот же мок, но с задержкой ответа: одновременные запросы должны
    /// пересечься по времени, иначе проверять блокировку нечем.
    async fn mock_jmap_session_slow(
        State(state): State<JmapAuthState>,
        headers: HeaderMap,
    ) -> Response {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        mock_jmap_session(State(state), headers).await
    }

    async fn spawn_slow_mock_jmap(expected_password: &str) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let state = JmapAuthState {
            base: base.clone(),
            expected_password: expected_password.to_owned(),
        };
        let app = Router::new()
            .route("/.well-known/jmap", get(mock_jmap_session_slow))
            .with_state(state);
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (base, server)
    }

    /// Мок для S-042: в момент, когда сервер отвечает на проверку пароля, он
    /// же меняет secret_ref строки аккаунта в базе напрямую - имитирует
    /// пользователя, успевшего переподключить ящик через мастер, пока
    /// проверка нового пароля ещё шла по сети.
    #[derive(Clone)]
    struct RaceState {
        base: String,
        expected_password: String,
        db: Db,
        account_id: i64,
        new_secret_ref: String,
    }
    async fn mock_jmap_session_racing(
        State(state): State<RaceState>,
        headers: HeaderMap,
    ) -> Response {
        if basic_auth_password(&headers).as_deref() != Some(state.expected_password.as_str()) {
            return (StatusCode::UNAUTHORIZED, "invalid credentials").into_response();
        }
        sqlx::query("UPDATE accounts SET secret_ref = ? WHERE id = ?")
            .bind(&state.new_secret_ref)
            .bind(state.account_id)
            .execute(&state.db.write_pool)
            .await
            .unwrap();
        Json(jmap_session_body(&state.base)).into_response()
    }
    /// Порт слушается заранее (без запуска сервера), чтобы аккаунт можно было
    /// сохранить с настоящим jmap_url ДО того, как обработчику станет известен
    /// account_id, который тоже нужен в состоянии сервера.
    async fn bind_local() -> (tokio::net::TcpListener, String) {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        (listener, base)
    }

    async fn serve_racing_mock_jmap(
        listener: tokio::net::TcpListener,
        base: String,
        expected_password: &str,
        db: Db,
        account_id: i64,
        new_secret_ref: &str,
    ) -> tokio::task::JoinHandle<()> {
        let state = RaceState {
            base,
            expected_password: expected_password.to_owned(),
            db,
            account_id,
            new_secret_ref: new_secret_ref.to_owned(),
        };
        let app = Router::new()
            .route("/.well-known/jmap", get(mock_jmap_session_racing))
            .with_state(state);
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        })
    }

    async fn temp_db(label: &str) -> (Db, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "truemail-change-password-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let crypto = std::sync::Arc::new(crate::crypto::StorageCrypto::from_key(rand::random()));
        let database_key = crate::crypto::DatabaseKey::from_key(rand::random());
        let db = Db::open_with_database_key(&root, crypto, &database_key)
            .await
            .unwrap();
        db.migrate().await.unwrap();
        (db, root)
    }

    async fn save_jmap_account(
        db: &Db,
        base: &str,
        secret_ref: &str,
        auth_kind: AuthKind,
    ) -> Account {
        save_jmap_account_as(db, base, secret_ref, auth_kind, "user@example.test").await
    }

    async fn save_jmap_account_as(
        db: &Db,
        base: &str,
        secret_ref: &str,
        auth_kind: AuthKind,
        email: &str,
    ) -> Account {
        db.save_account(&NewAccount {
            email: email.into(),
            display_name: "JMAP test".into(),
            provider: Provider::Generic,
            backend_kind: BackendKind::Jmap,
            auth_kind,
            imap: None,
            smtp: None,
            ews_url: None,
            caldav_url: None,
            carddav_url: None,
            jmap_url: Some(format!("{base}/.well-known/jmap")),
            username: Some(email.to_owned()),
            secret_ref: secret_ref.into(),
            color: None,
        })
        .await
        .unwrap()
    }

    async fn cleanup(db: Db, root: std::path::PathBuf, server: tokio::task::JoinHandle<()>) {
        server.abort();
        db.close().await;
        drop(db);
        let _ = std::fs::remove_dir_all(root);
    }

    // ---------- классификация ошибок (без сети) ----------

    #[test]
    fn classifies_auth_failures_conservatively() {
        assert!(classify_validation_error("HTTP 401: Unauthorized"));
        assert!(classify_validation_error("Invalid credentials"));
        assert!(classify_validation_error("AUTHENTICATIONFAILED"));
        assert!(classify_validation_error("неверный пароль"));
        // Сетевые и прочие отказы не должны выдаваться за отказ авторизации:
        // ложное "неверный пароль" хуже честного "сервер недоступен".
        assert!(!classify_validation_error("connection refused"));
        assert!(!classify_validation_error("HTTP 503: Service Unavailable"));
        assert!(!classify_validation_error("timed out waiting for response"));
    }

    /// Коды смены пароля и их виды читает интерфейс: settings.js различает
    /// два кода команды дословно, а вид ошибки попадает в общую таблицу
    /// error-presentation.js. Перечень берётся из самого файла интерфейса -
    /// новый вариант ошибки с видом, которого там нет, обязан ронять проверку,
    /// а не показываться пользователю пустой карточкой.
    #[test]
    fn password_error_codes_and_kinds_are_known_to_the_interface() {
        const PRESENTATION: &str =
            include_str!("../../../../apps/desktop/ui/modules/error-presentation.js");
        const SETTINGS: &str = include_str!("../../../../apps/desktop/ui/modules/settings.js");

        // Виды ошибок, для которых в интерфейсе есть текст и действие.
        let known_kinds: Vec<&str> = PRESENTATION
            .lines()
            .skip_while(|line| !line.contains("const ERROR_KINDS"))
            .skip(1)
            .take_while(|line| !line.contains("});"))
            .filter_map(|line| line.trim().split_once(':').map(|(key, _)| key.trim()))
            .collect();
        assert!(
            known_kinds.len() >= 10,
            "перечень видов ошибок интерфейса не прочитан: {known_kinds:?}"
        );

        let cases = [
            (
                ChangePasswordError::InvalidCredentials("x".into()),
                "invalid_credentials",
                Some("invalid_credentials"),
            ),
            (
                ChangePasswordError::UnsupportedAuthKind,
                "unsupported_auth_kind",
                Some("account_config"),
            ),
            (
                ChangePasswordError::AccountNotFound,
                "account_not_found",
                Some("unknown"),
            ),
            (
                ChangePasswordError::MissingSecretRef,
                "missing_secret_ref",
                Some("needs_reauth"),
            ),
            (
                ChangePasswordError::SecretStoreWriteFailed,
                "secret_store_write_failed",
                Some("secret_store_error"),
            ),
            (
                ChangePasswordError::SecretStoreStateUnknown,
                "secret_store_state_unknown",
                Some("secret_store_error"),
            ),
            // Повторный запрос по тому же аккаунту вида не имеет: интерфейс
            // показывает его собственным текстом диалога, а не общей таблицей.
            (
                ChangePasswordError::ChangeInProgress,
                "change_in_progress",
                None,
            ),
            (
                ChangePasswordError::AccountChanged,
                "account_changed",
                Some("account_config"),
            ),
            (
                ChangePasswordError::BackendUnavailable("x".into()),
                "backend_unavailable",
                Some("server_unavailable"),
            ),
        ];
        for (error, code, kind) in cases {
            assert_eq!(error.code(), code, "код ошибки {error:?}");
            assert_eq!(error.general_error_code(), kind, "вид ошибки {error:?}");
            if let Some(kind) = kind {
                assert!(
                    known_kinds.contains(&kind),
                    "вид \"{kind}\" не описан в error-presentation.js"
                );
            }
        }

        // Два кода диалог смены пароля различает дословно.
        for code in ["invalid_credentials", "missing_secret_ref"] {
            assert!(
                SETTINGS.contains(&format!("error?.code==='{code}'")),
                "settings.js больше не различает код \"{code}\""
            );
        }
    }

    // ---------- S-017: блокировка на аккаунт ----------

    /// S-017: два настоящих одновременных запроса по одному аккаунту. Один
    /// проходит, второй получает отказ "смена уже идёт" и не трогает хранилище
    /// секретов. Блокировка при этом обязана сниматься: следующая смена того же
    /// аккаунта проходит обычным порядком - потеря разблокировки означала бы,
    /// что пароль этого ящика больше не сменить до перезапуска.
    #[tokio::test]
    async fn two_simultaneous_changes_of_one_account_leave_one_winner() {
        let (db, root) = temp_db("in-progress").await;
        // Сервер отвечает не мгновенно: иначе первый запрос успевал бы
        // завершиться раньше, чем второй доходит до блокировки.
        let (base, server) = spawn_slow_mock_jmap("new-password").await;
        let account = save_jmap_account(&db, &base, "secret-ref-1", AuthKind::AppPassword).await;
        let manager = std::sync::Arc::new(AccountManager::new(db.clone()));
        let store = MockSecretStore::new(
            &[("secret-ref-1", "old-password")],
            true,
            ReadAfterWrite::NewValue,
        );

        let (first, second) = tokio::join!(
            manager.change_account_password_with_store(
                account.id,
                Zeroizing::new("new-password".into()),
                &store,
            ),
            manager.change_account_password_with_store(
                account.id,
                Zeroizing::new("new-password".into()),
                &store,
            )
        );

        let mut outcomes = [first, second];
        outcomes.sort_by_key(|outcome| outcome.is_err());
        assert_eq!(outcomes[0], Ok(()), "один из запросов обязан пройти");
        assert_eq!(
            outcomes[1],
            Err(ChangePasswordError::ChangeInProgress),
            "второй запрос по тому же аккаунту обязан получить отказ"
        );
        assert_eq!(
            store.write_count(),
            1,
            "отклонённый запрос не должен писать в хранилище секретов"
        );

        // Блокировка снята: следующая смена того же аккаунта проходит.
        let again = manager
            .change_account_password_with_store(
                account.id,
                Zeroizing::new("new-password".into()),
                &store,
            )
            .await;
        assert_eq!(again, Ok(()), "после завершения смены аккаунт снова доступен");

        cleanup(db, root, server).await;
    }

    /// Блокировка идёт по аккаунту, а не по приложению: смена пароля одного
    /// ящика не должна мешать другому.
    #[tokio::test]
    async fn a_change_of_one_account_does_not_block_another() {
        let (db, root) = temp_db("two-accounts").await;
        let (base, server) = spawn_slow_mock_jmap("new-password").await;
        let first_account =
            save_jmap_account(&db, &base, "secret-ref-a", AuthKind::AppPassword).await;
        let second_account = save_jmap_account_as(
            &db,
            &base,
            "secret-ref-b",
            AuthKind::AppPassword,
            "second@example.test",
        )
        .await;
        let manager = std::sync::Arc::new(AccountManager::new(db.clone()));
        let store = MockSecretStore::new(
            &[
                ("secret-ref-a", "old-password"),
                ("secret-ref-b", "old-password"),
            ],
            true,
            ReadAfterWrite::NewValue,
        );

        let (first, second) = tokio::join!(
            manager.change_account_password_with_store(
                first_account.id,
                Zeroizing::new("new-password".into()),
                &store,
            ),
            manager.change_account_password_with_store(
                second_account.id,
                Zeroizing::new("new-password".into()),
                &store,
            )
        );

        assert_eq!(first, Ok(()));
        assert_eq!(second, Ok(()));
        cleanup(db, root, server).await;
    }

    // ---------- контракт и повреждённые состояния (S-010, S-041) ----------

    #[tokio::test]
    async fn unsupported_auth_kind_for_oauth2_account() {
        let (db, root) = temp_db("oauth").await;
        let (base, server) = spawn_mock_jmap("irrelevant").await;
        let account = save_jmap_account(&db, &base, "secret-ref-oauth", AuthKind::Oauth2).await;
        let manager = AccountManager::new(db.clone());
        let store = MockSecretStore::new(&[], true, ReadAfterWrite::NewValue);
        let result = manager
            .change_account_password_with_store(
                account.id,
                Zeroizing::new("new-password".into()),
                &store,
            )
            .await;
        assert_eq!(result, Err(ChangePasswordError::UnsupportedAuthKind));
        assert_eq!(store.write_count(), 0);
        cleanup(db, root, server).await;
    }

    #[tokio::test]
    async fn account_not_found_for_unknown_id() {
        let (db, root) = temp_db("not-found").await;
        let manager = AccountManager::new(db.clone());
        let store = MockSecretStore::new(&[], true, ReadAfterWrite::NewValue);
        let result = manager
            .change_account_password_with_store(
                999_999,
                Zeroizing::new("new-password".into()),
                &store,
            )
            .await;
        assert_eq!(result, Err(ChangePasswordError::AccountNotFound));
        db.close().await;
        drop(db);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn missing_secret_ref_is_rejected_before_any_network_or_store_access() {
        let (db, root) = temp_db("missing-secret-ref").await;
        let (base, server) = spawn_mock_jmap("correct-password").await;
        let account = save_jmap_account(&db, &base, "secret-ref-x", AuthKind::AppPassword).await;
        sqlx::query("UPDATE accounts SET secret_ref = NULL WHERE id = ?")
            .bind(account.id)
            .execute(&db.write_pool)
            .await
            .unwrap();
        let manager = AccountManager::new(db.clone());
        let store = MockSecretStore::new(&[], true, ReadAfterWrite::NewValue);
        let result = manager
            .change_account_password_with_store(
                account.id,
                Zeroizing::new("new-password".into()),
                &store,
            )
            .await;
        assert_eq!(result, Err(ChangePasswordError::MissingSecretRef));
        assert_eq!(store.write_count(), 0);
        cleanup(db, root, server).await;
    }

    #[tokio::test]
    async fn backend_unavailable_when_stored_config_is_incomplete() {
        let (db, root) = temp_db("incomplete-config").await;
        let (base, server) = spawn_mock_jmap("correct-password").await;
        let account = save_jmap_account(&db, &base, "secret-ref-y", AuthKind::AppPassword).await;
        // jmap_url потерян - mail_backend не может построить подключение.
        sqlx::query("UPDATE accounts SET jmap_url = NULL WHERE id = ?")
            .bind(account.id)
            .execute(&db.write_pool)
            .await
            .unwrap();
        let manager = AccountManager::new(db.clone());
        let store = MockSecretStore::new(&[], true, ReadAfterWrite::NewValue);
        let result = manager
            .change_account_password_with_store(
                account.id,
                Zeroizing::new("new-password".into()),
                &store,
            )
            .await;
        assert!(matches!(
            result,
            Err(ChangePasswordError::BackendUnavailable(_))
        ));
        assert_eq!(store.write_count(), 0);
        cleanup(db, root, server).await;
    }

    // ---------- S-011, S-012, S-013: проверка пароля перед записью ----------

    #[tokio::test]
    async fn wrong_password_is_rejected_and_secret_store_is_never_touched() {
        let (db, root) = temp_db("wrong-password").await;
        let (base, server) = spawn_mock_jmap("correct-password").await;
        let account =
            save_jmap_account(&db, &base, "secret-ref-wrong", AuthKind::AppPassword).await;
        let manager = AccountManager::new(db.clone());
        let store = MockSecretStore::new(
            &[("secret-ref-wrong", "correct-password")],
            true,
            ReadAfterWrite::NewValue,
        );
        let result = manager
            .change_account_password_with_store(
                account.id,
                Zeroizing::new("totally-wrong-password".into()),
                &store,
            )
            .await;
        assert_eq!(
            result,
            Err(ChangePasswordError::InvalidCredentials(
                "транспорт (jmap-session): HTTP 401 Unauthorized: invalid credentials".into()
            ))
        );
        assert_eq!(
            store.write_count(),
            0,
            "неверный пароль не должен трогать хранилище секретов (S-012)"
        );
        // S-018: сообщение об ошибке приходит от сервера, а не строится из
        // отправленного пароля - сам пароль в нём появиться не может.
        assert!(!format!("{result:?}").contains("totally-wrong-password"));
        // Строка аккаунта в базе не изменилась (S-013): читаем заново и сверяем secret_ref.
        let reread = db
            .list_accounts()
            .await
            .unwrap()
            .into_iter()
            .find(|a| a.id == account.id)
            .unwrap();
        assert_eq!(reread.secret_ref.as_deref(), Some("secret-ref-wrong"));
        cleanup(db, root, server).await;
    }

    // ---------- S-014: таблица исходов записи ----------

    #[tokio::test]
    async fn success_when_readback_matches_new_value() {
        let (db, root) = temp_db("ok-new").await;
        let (base, server) = spawn_mock_jmap("correct-password").await;
        let account = save_jmap_account(&db, &base, "secret-ref-ok", AuthKind::Password).await;
        let manager = AccountManager::new(db.clone());
        for write_returns in [true, false] {
            let store = MockSecretStore::new(
                &[("secret-ref-ok", "old-password")],
                write_returns,
                ReadAfterWrite::NewValue,
            );
            let result = manager
                .change_account_password_with_store(
                account.id,
                Zeroizing::new("correct-password".into()),
                &store,
            )
                .await;
            assert_eq!(
                result,
                Ok(()),
                "write_returns={write_returns}: контрольное чтение важнее сырого результата write()"
            );
        }
        cleanup(db, root, server).await;
    }

    #[tokio::test]
    async fn write_failed_when_readback_still_shows_old_value() {
        let (db, root) = temp_db("write-failed").await;
        let (base, server) = spawn_mock_jmap("correct-password").await;
        let account = save_jmap_account(&db, &base, "secret-ref-wf", AuthKind::Password).await;
        let manager = AccountManager::new(db.clone());
        for write_returns in [true, false] {
            let store = MockSecretStore::new(
                &[("secret-ref-wf", "old-password")],
                write_returns,
                ReadAfterWrite::OldValue,
            );
            let result = manager
                .change_account_password_with_store(
                account.id,
                Zeroizing::new("correct-password".into()),
                &store,
            )
                .await;
            assert_eq!(result, Err(ChangePasswordError::SecretStoreWriteFailed));
        }
        cleanup(db, root, server).await;
    }

    #[tokio::test]
    async fn state_unknown_when_readback_fails_or_matches_neither_value() {
        let (db, root) = temp_db("state-unknown").await;
        let (base, server) = spawn_mock_jmap("correct-password").await;
        let account = save_jmap_account(&db, &base, "secret-ref-su", AuthKind::Password).await;
        let manager = AccountManager::new(db.clone());
        // Обе ветки результата записи: итог определяет только контрольное чтение,
        // поэтому неизвестное состояние остаётся неизвестным в любом случае (S-014).
        for wrote in [true, false] {
            for outcome in [ReadAfterWrite::Unreadable, ReadAfterWrite::Other] {
                let store =
                    MockSecretStore::new(&[("secret-ref-su", "old-password")], wrote, outcome);
                let result = manager
                    .change_account_password_with_store(
                account.id,
                Zeroizing::new("correct-password".into()),
                &store,
            )
                    .await;
                assert_eq!(result, Err(ChangePasswordError::SecretStoreStateUnknown));
            }
        }
        cleanup(db, root, server).await;
    }

    #[tokio::test]
    async fn missing_old_secret_does_not_block_change_and_compares_only_to_new_value() {
        // S-041: записи в keyring нет (initial пуст) - смене это не мешает.
        let (db, root) = temp_db("no-old-secret").await;
        let (base, server) = spawn_mock_jmap("correct-password").await;
        let account = save_jmap_account(&db, &base, "secret-ref-none", AuthKind::Password).await;
        let manager = AccountManager::new(db.clone());
        let store = MockSecretStore::new(&[], true, ReadAfterWrite::NewValue);
        let result = manager
            .change_account_password_with_store(
                account.id,
                Zeroizing::new("correct-password".into()),
                &store,
            )
            .await;
        assert_eq!(result, Ok(()));
        cleanup(db, root, server).await;
    }

    // ---------- S-042: конфигурация перечитывается перед записью ----------

    #[tokio::test]
    async fn account_changed_between_validation_and_write_aborts_without_writing() {
        let (db, root) = temp_db("account-changed").await;
        let (listener, base) = bind_local().await;
        let account = save_jmap_account(&db, &base, "secret-ref-race", AuthKind::Password).await;
        let server = serve_racing_mock_jmap(
            listener,
            base,
            "correct-password",
            db.clone(),
            account.id,
            "secret-ref-race-NEW",
        )
        .await;
        let manager = AccountManager::new(db.clone());
        let store = MockSecretStore::new(
            &[
                ("secret-ref-race", "old-password"),
                ("secret-ref-race-NEW", "old-password"),
            ],
            true,
            ReadAfterWrite::NewValue,
        );
        let result = manager
            .change_account_password_with_store(
                account.id,
                Zeroizing::new("correct-password".into()),
                &store,
            )
            .await;
        assert_eq!(result, Err(ChangePasswordError::AccountChanged));
        assert_eq!(
            store.write_count(),
            0,
            "secret_ref поменялся во время проверки - запись не должна выполняться (S-042)"
        );
        cleanup(db, root, server).await;
    }

    /// Приёмник журнала: буфер, в который подписчик tracing складывает все
    /// записи, сделанные на проверяемом пути.
    #[derive(Clone, Default)]
    struct LogBuffer(std::sync::Arc<Mutex<Vec<u8>>>);

    impl LogBuffer {
        fn text(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().expect("журнал")).into_owned()
        }
    }

    impl std::io::Write for LogBuffer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("журнал").extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogBuffer {
        type Writer = LogBuffer;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    /// S-018 (issue #113): значение пароля не попадает в журнал ни на одной
    /// ветке смены. Путь проходится целиком - успех, отказ сервера и отказ
    /// хранилища секретов, - а записи журнала собираются настоящим подписчиком
    /// tracing: ревью журналов такую утечку пропускает, а проверка нет.
    #[tokio::test]
    async fn password_never_reaches_the_log_on_any_branch() {
        const PASSWORD: &str = "sup3r-secret-value";
        let logs = LogBuffer::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(logs.clone())
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);

        let (db, root) = temp_db("log-leak").await;
        let (base, server) = spawn_mock_jmap(PASSWORD).await;
        let account = save_jmap_account(&db, &base, "secret-ref-log", AuthKind::Password).await;
        let manager = AccountManager::new(db.clone());

        // Успех: сервер принял пароль, хранилище подтвердило запись.
        let store = MockSecretStore::new(
            &[("secret-ref-log", "old-password")],
            true,
            ReadAfterWrite::NewValue,
        );
        assert_eq!(
            manager
                .change_account_password_with_store(
                    account.id,
                    Zeroizing::new(PASSWORD.into()),
                    &store,
                )
                .await,
            Ok(())
        );

        // Отказ сервера: пароль не подошёл.
        let (other_base, other_server) = spawn_mock_jmap("совсем другой пароль").await;
        let rejected_account =
            save_jmap_account_as(&db, &other_base, "secret-ref-log-2", AuthKind::Password, "other@example.test")
                .await;
        let store = MockSecretStore::new(&[], true, ReadAfterWrite::NewValue);
        assert!(matches!(
            manager
                .change_account_password_with_store(
                    rejected_account.id,
                    Zeroizing::new(PASSWORD.into()),
                    &store,
                )
                .await,
            Err(ChangePasswordError::InvalidCredentials(_))
        ));

        // Отказ хранилища: запись не состоялась, контрольное чтение видит старое.
        let store = MockSecretStore::new(
            &[("secret-ref-log", "old-password")],
            false,
            ReadAfterWrite::OldValue,
        );
        assert_eq!(
            manager
                .change_account_password_with_store(
                    account.id,
                    Zeroizing::new(PASSWORD.into()),
                    &store,
                )
                .await,
            Err(ChangePasswordError::SecretStoreWriteFailed)
        );

        let text = logs.text();
        assert!(
            !text.is_empty(),
            "подписчик не собрал ни одной записи - проверять нечего"
        );
        assert!(
            !text.contains(PASSWORD),
            "пароль попал в журнал:\n{text}"
        );
        // Адрес ящика в журнале тоже маскируется - иначе утечка просто
        // переезжает с пароля на владельца.
        assert!(
            !text.contains("user@example.test"),
            "адрес ящика попал в журнал без маски:\n{text}"
        );

        other_server.abort();
        cleanup(db, root, server).await;
    }
}
