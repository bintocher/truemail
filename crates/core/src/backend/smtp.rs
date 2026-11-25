//! Отправка почты через SMTP XOAUTH2.

use crate::model::Security;
use crate::{Error, Result};
use lettre::message::header::{HeaderName, HeaderValue};
use lettre::message::{Attachment, Mailbox, Message, MultiPart, SinglePart, header::ContentType};
use lettre::transport::smtp::authentication::{Credentials, DEFAULT_MECHANISMS, Mechanism};
use lettre::transport::smtp::client::{AsyncSmtpConnection, TlsParameters};
use lettre::transport::smtp::commands::{Data, Mail, Rcpt};
use lettre::transport::smtp::extension::{ClientId, Extension, MailBodyParameter, MailParameter};
use lettre::transport::smtp::response::Severity;
use std::time::Duration;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OutgoingAttachment {
    pub filename: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct OutgoingMessage {
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body_text: String,
    pub body_html: Option<String>,
    pub attachments: Vec<OutgoingAttachment>,
    /// Закреплённый идентификатор письма: очередь отправки выбирает его один
    /// раз и повторяет при каждой попытке, иначе повторная доставка выглядела
    /// бы для сервера получателя новым письмом (specs/undo-send.md, S-045).
    #[serde(default)]
    pub message_id: Option<String>,
    /// Дополнительные заголовки служебного письма: ими автоответ помечает себя
    /// и связывается с исходным письмом (specs/out-of-office.md, S-056 - S-058).
    #[serde(default)]
    pub headers: Vec<(String, String)>,
}

/// Поддерживаемые типы встроенных картинок (S-004, S-035): svg+xml намеренно
/// не входит - это разметка, которая может содержать исполняемый код.
pub(crate) const SUPPORTED_INLINE_IMAGE_TYPES: [&str; 5] = [
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "image/bmp",
];

/// Whether the letter builder can put a picture of this type into the letter
/// as an embedded part.
pub fn is_inline_image_type(mime_type: &str) -> bool {
    SUPPORTED_INLINE_IMAGE_TYPES.contains(&mime_type.to_ascii_lowercase().as_str())
}

/// Картинка, вынесенная из текста письма в отдельную часть (S-028).
struct InlineImagePart {
    content_id: String,
    mime_type: String,
    data: Vec<u8>,
}

/// Ищет следующий тег `<img` начиная с `from`, независимо от регистра букв
/// (S-041). Возвращает байтовое смещение символа `<`.
/// The elements whose content is text for the parser, not markup: a tag named
/// inside them is not a tag.
const RAW_TEXT_TAGS: [&[u8]; 4] = [b"script", b"style", b"textarea", b"title"];

fn starts_with(bytes: &[u8], at: usize, text: &[u8]) -> bool {
    bytes
        .get(at..at + text.len())
        .is_some_and(|part| part.eq_ignore_ascii_case(text))
}

fn find_bytes(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    bytes[from.min(bytes.len())..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|position| from + position)
}

/// Returns the position of the closing tag's `>`, wrapped in `Some` for a raw
/// text element. An unclosed raw text element returns `Some(None)`.
fn raw_text_end(bytes: &[u8], name_start: usize) -> Option<Option<usize>> {
    let name = RAW_TEXT_TAGS.iter().find(|name| {
        starts_with(bytes, name_start, name)
            && bytes
                .get(name_start + name.len())
                .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'>')
    })?;
    let mut from = name_start;
    loop {
        let Some(close) = find_bytes(bytes, from + 1, b"</") else {
            return Some(None);
        };
        if starts_with(bytes, close + 2, name) {
            return Some(find_bytes(bytes, close + 2, b">"));
        }
        from = close;
    }
}

/// Ищет начало `<img` тега: за ним идёт пробел, `/` или `>`. Комментарий,
/// содержимое script, style, textarea и title и другой тег целиком пропускаются,
/// поэтому имя тега в них или в значении атрибута тегом не считается.
pub(crate) fn find_img_tag_start(bytes: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let next = bytes.get(i + 1).copied().unwrap_or(0);
        if next == b'!' {
            i = if starts_with(bytes, i + 2, b"--") {
                find_bytes(bytes, i + 2, b"-->")? + 3
            } else {
                find_tag_end(bytes, i)? + 1
            };
            continue;
        }
        if next != b'/' && !next.is_ascii_alphabetic() {
            i += 1;
            continue;
        }
        let name_start = if next == b'/' { i + 2 } else { i + 1 };
        let is_img = starts_with(bytes, name_start, b"img")
            && bytes
                .get(name_start + 3)
                .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'/' || *byte == b'>');
        if is_img && next != b'/' {
            return Some(i);
        }
        if next != b'/' {
            match raw_text_end(bytes, name_start) {
                Some(Some(end)) => {
                    i = end + 1;
                    continue;
                }
                Some(None) => return None,
                None => {}
            }
        }
        // Another tag is skipped whole: a tag named inside the value of one of
        // its attributes is not a tag.
        i = find_tag_end(bytes, i)? + 1;
    }
    None
}

/// Ищет закрывающий `>` тега, начатого в `start`, не обрывая тег на `>`
/// внутри значения атрибута в кавычках (одинарных или двойных, S-041).
pub(crate) fn find_tag_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) if b == q => quote = None,
            Some(_) => {}
            None if b == b'"' || b == b'\'' => quote = Some(b),
            None if b == b'>' => return Some(i),
            None => {}
        }
        i += 1;
    }
    None
}

/// Ищет значение атрибута `src` внутри тега `[tag_start..=tag_end]`,
/// независимо от регистра имени атрибута, при одинарных и двойных кавычках
/// и при других атрибутах в том же теге (S-041). Возвращает байтовые границы
/// значения без кавычек.
pub(crate) fn find_src_attr(
    bytes: &[u8],
    tag_start: usize,
    tag_end: usize,
) -> Option<(usize, usize)> {
    let mut i = tag_start + 4;
    while i < tag_end {
        if bytes[i].is_ascii_whitespace() || bytes[i] == b'/' {
            i += 1;
            continue;
        }
        let name_start = i;
        while i < tag_end && bytes[i] != b'=' && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let name_end = i;
        if name_start == name_end {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < tag_end && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= tag_end || bytes[j] != b'=' {
            i = name_end;
            continue;
        }
        j += 1;
        while j < tag_end && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        let is_src = bytes[name_start..name_end].eq_ignore_ascii_case(b"src");
        if j < tag_end && (bytes[j] == b'"' || bytes[j] == b'\'') {
            let quote = bytes[j];
            let value_start = j + 1;
            let mut k = value_start;
            while k < tag_end && bytes[k] != quote {
                k += 1;
            }
            if is_src {
                return Some((value_start, k));
            }
            i = (k + 1).min(tag_end);
        } else {
            let value_start = j;
            let mut k = value_start;
            while k < tag_end && !bytes[k].is_ascii_whitespace() {
                k += 1;
            }
            if is_src {
                return Some((value_start, k));
            }
            i = k;
        }
    }
    None
}

/// Разбирает строку `data:` на тип и байты (S-008, S-028, S-035, S-041 по
/// слову `data`): `None`, если тип вне перечня поддерживаемых или данные не
/// разбираются как base64.
fn parse_data_url(value: &str) -> Option<(String, Vec<u8>)> {
    use base64::Engine as _;
    if value.len() < 5 || !value.as_bytes()[..5].eq_ignore_ascii_case(b"data:") {
        return None;
    }
    let rest = &value[5..];
    let comma = rest.find(',')?;
    let meta = &rest[..comma];
    let data_part = &rest[comma + 1..];
    let mut is_base64 = false;
    let mut mime = String::new();
    for segment in meta.split(';') {
        let segment = segment.trim();
        if segment.eq_ignore_ascii_case("base64") {
            is_base64 = true;
        } else if !segment.is_empty() && mime.is_empty() {
            // Тип - первый непустой кусок до данных, остальные куски это
            // параметры вида name=... Интерфейс читает строку так же, иначе
            // он бы посчитал картинку своей, а сборка письма - чужой.
            mime = segment.to_ascii_lowercase();
        }
    }
    if !is_base64 || !SUPPORTED_INLINE_IMAGE_TYPES.contains(&mime.as_str()) {
        return None;
    }
    // Читаем строку теми же послаблениями, что и браузерный atob в интерфейсе:
    // без хвостовых "=", с необязательными битами в последнем символе, с
    // выброшенными пробелами и невидимой меткой порядка байтов. Иначе картинка,
    // которую интерфейс посчитал своей, ушла бы получателю строкой data:.
    let cleaned: String = data_part
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{feff}')
        .collect();
    let config = base64::engine::general_purpose::GeneralPurposeConfig::new()
        .with_decode_allow_trailing_bits(true)
        .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent);
    let engine = base64::engine::GeneralPurpose::new(&base64::alphabet::STANDARD, config);
    let bytes = engine.decode(cleaned.as_bytes()).ok()?;
    Some((mime, bytes))
}

/// Выносит встроенные картинки из текста письма в отдельные части (S-028) и
/// заменяет их ссылки на `cid:` с той же меткой без скобок (S-031). Строки
/// `data:` с полностью совпадающими данными выносятся одной частью с одной
/// меткой (S-032). Тег с непонятной строкой `data:` (неподдерживаемый тип,
/// неразбираемый base64) или с внешней ссылкой `http`/`https` остаётся без
/// изменений (S-035, S-036).
fn extract_inline_images(html: &str) -> (String, Vec<InlineImagePart>) {
    let bytes = html.as_bytes();
    let mut out = String::with_capacity(html.len());
    let mut parts: Vec<InlineImagePart> = Vec::new();
    let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut pos = 0usize;
    let mut next_index = 0usize;
    loop {
        let Some(tag_start) = find_img_tag_start(bytes, pos) else {
            out.push_str(&html[pos..]);
            break;
        };
        out.push_str(&html[pos..tag_start]);
        let Some(tag_end) = find_tag_end(bytes, tag_start) else {
            out.push_str(&html[tag_start..]);
            break;
        };
        match find_src_attr(bytes, tag_start, tag_end) {
            Some((value_start, value_end)) => {
                let src_value = &html[value_start..value_end];
                match parse_data_url(src_value) {
                    Some((mime_type, data)) => {
                        let content_id = if let Some(existing) = seen.get(src_value) {
                            existing.clone()
                        } else {
                            let id = format!("truemailimg{next_index}");
                            next_index += 1;
                            seen.insert(src_value.to_string(), id.clone());
                            parts.push(InlineImagePart {
                                content_id: id.clone(),
                                mime_type,
                                data,
                            });
                            id
                        };
                        out.push_str(&html[tag_start..value_start]);
                        out.push_str("cid:");
                        out.push_str(&content_id);
                        out.push_str(&html[value_end..=tag_end]);
                    }
                    None => out.push_str(&html[tag_start..=tag_end]),
                }
            }
            None => out.push_str(&html[tag_start..=tag_end]),
        }
        pos = tag_end + 1;
    }
    (out, parts)
}

/// Goes over the `img` tags of the markup and gives the source of each one to
/// `replace`. A source gets the returned text in its place; `None` leaves it.
/// Returns the new markup and the number of replaced sources. The scan is the
/// one of the letter builder, so a link named in the text or in another
/// attribute is left alone.
pub fn replace_img_sources(
    html: &str,
    mut replace: impl FnMut(&str) -> Option<String>,
) -> (String, usize) {
    let bytes = html.as_bytes();
    let mut out = String::with_capacity(html.len());
    let mut replaced = 0usize;
    let mut pos = 0usize;
    loop {
        let Some(tag_start) = find_img_tag_start(bytes, pos) else {
            out.push_str(&html[pos..]);
            break;
        };
        let Some(tag_end) = find_tag_end(bytes, tag_start) else {
            out.push_str(&html[pos..]);
            break;
        };
        let source = find_src_attr(bytes, tag_start, tag_end).and_then(|(start, end)| {
            let value = replace(html.get(start..end)?.trim())?;
            Some((start, end, value))
        });
        match source {
            Some((start, end, value)) => {
                out.push_str(&html[pos..start]);
                out.push_str(&value);
                out.push_str(&html[end..=tag_end]);
                replaced += 1;
            }
            None => out.push_str(&html[pos..=tag_end]),
        }
        pos = tag_end + 1;
    }
    (out, replaced)
}

// Вид ошибки отправки определяется по типу исключения lettre и коду ответа
// сервера, а не по тексту: 535 и 534 - отказ входа, 4xx - сервер занят или
// временно недоступен, обрыв соединения и таймаут - проблема связи
// (specs/error-kinds-and-messages.md, S-002).
fn smtp_error(backend: &str, error: lettre::transport::smtp::Error) -> Error {
    use crate::ErrorKind;
    let kind = if error.is_timeout() {
        ErrorKind::Timeout
    } else if error.is_tls() {
        ErrorKind::CertificateError
    } else if let Some(code) = error.status() {
        match (code.severity, code.to_string().as_str()) {
            // 535 и 534 - отказ проверки подлинности, 530 - сервер требует входа.
            (_, "535" | "534" | "530") => ErrorKind::InvalidCredentials,
            (Severity::TransientNegativeCompletion, _) => ErrorKind::ServerUnavailable,
            _ => ErrorKind::Unknown,
        }
    } else if error.is_client() || error.is_response() {
        ErrorKind::AccountConfig
    } else {
        ErrorKind::NetworkUnavailable
    };
    Error::classified_backend(backend, kind, error.to_string())
}

/// После DATA отсутствие явного отказа означает, что сервер мог принять письмо.
fn smtp_failure(backend: &str, error: lettre::transport::smtp::Error) -> super::SendFailure {
    let after_handoff = !error.is_transient() && !error.is_permanent();
    let error = smtp_error(backend, error);
    if after_handoff {
        super::SendFailure::after_handoff(error)
    } else {
        super::SendFailure::before_handoff(error)
    }
}

pub(crate) fn build_message(message: OutgoingMessage) -> Result<Message> {
    build_letter(message, false)
}

/// The letter for a server interface that takes the whole MIME text (Gmail,
/// EWS, JMAP): the hidden copy travels inside the letter, so its header stays.
pub(crate) fn build_message_with_bcc(message: OutgoingMessage) -> Result<Message> {
    build_letter(message, true)
}

fn build_letter(message: OutgoingMessage, keep_bcc: bool) -> Result<Message> {
    let extra_headers = message.headers.clone();
    if message.to.is_empty() && message.cc.is_empty() && message.bcc.is_empty() {
        return Err(Error::AccountConfig("не указан получатель".into()));
    }
    // Встроенные картинки (S-028) выносятся из текста письма до подсчёта
    // предела размера - их байты входят в тот же предел, что и вложения (S-038).
    let (html_body, inline_images) = match message.body_html.filter(|html| !html.trim().is_empty())
    {
        Some(html) => {
            let (replaced, images) = extract_inline_images(&html);
            (Some(replaced), images)
        }
        None => (None, Vec::new()),
    };
    let total_size: usize = message
        .attachments
        .iter()
        .map(|item| item.data.len())
        .sum::<usize>()
        + inline_images
            .iter()
            .map(|item| item.data.len())
            .sum::<usize>();
    if total_size > MAX_ATTACHMENTS_BYTES {
        return Err(Error::AccountConfig(
            "суммарный размер вложений и встроенных картинок превышает 25 МБ".into(),
        ));
    }
    // S-045: закреплённый идентификатор письма приходит из очереди отправки.
    // Без него повторная попытка собрала бы письмо с новым Message-ID, и
    // сервер получателя принял бы его как второе письмо.
    let builder = Message::builder()
        .from(mailbox(&message.from)?)
        .message_id(message.message_id.clone())
        .subject(message.subject);
    let mut builder = if keep_bcc {
        builder.keep_bcc()
    } else {
        builder
    };
    for address in &message.to {
        builder = builder.to(mailbox(address)?);
    }
    for address in &message.cc {
        builder = builder.cc(mailbox(address)?);
    }
    for address in &message.bcc {
        builder = builder.bcc(mailbox(address)?);
    }
    let alternative = match html_body {
        // S-029: письмо со встроенными картинками собирает html вместе с их
        // частями в multipart/related внутри multipart/alternative.
        Some(html) if !inline_images.is_empty() => {
            let mut related = MultiPart::related().singlepart(SinglePart::html(html));
            for image in inline_images {
                let content_type = image
                    .mime_type
                    .parse::<ContentType>()
                    .unwrap_or(ContentType::parse("application/octet-stream").expect("valid MIME"));
                related = related.singlepart(
                    Attachment::new_inline(image.content_id).body(image.data, content_type),
                );
            }
            MultiPart::alternative()
                .singlepart(SinglePart::plain(message.body_text))
                .multipart(related)
        }
        // S-034: без вынесенных картинок строение письма не меняется.
        Some(html) => MultiPart::alternative()
            .singlepart(SinglePart::plain(message.body_text))
            .singlepart(SinglePart::html(html)),
        None => MultiPart::alternative().singlepart(SinglePart::plain(message.body_text)),
    };
    let mut mixed = MultiPart::mixed().multipart(alternative);
    for item in message.attachments {
        let content_type = item
            .mime_type
            .parse::<ContentType>()
            .unwrap_or(ContentType::parse("application/octet-stream").expect("valid MIME"));
        mixed = mixed.singlepart(Attachment::new(item.filename).body(item.data, content_type));
    }
    let mut email = builder.multipart(mixed).map_err(|error| Error::Backend {
        backend: "smtp-message".into(),
        message: error.to_string(),
    })?;
    // Служебные заголовки автоответа типизированных представлений в lettre не
    // имеют, поэтому добавляются сырыми значениями уже к собранному письму.
    for (name, value) in extra_headers {
        let Ok(name) = HeaderName::new_from_ascii(name) else {
            continue;
        };
        email
            .headers_mut()
            .insert_raw(HeaderValue::new(name, value));
    }
    Ok(email)
}

/// Проверить адресатов письма до записи операции отправки: непригодный адрес
/// не должен создавать ожидающее письмо вовсе (specs/undo-send.md, S-005).
/// Предел размера проверяется здесь же: письмо, которое не собрать, не должно
/// попадать в очередь.
pub fn validate_outgoing(message: &OutgoingMessage) -> Result<()> {
    if message.to.is_empty() && message.cc.is_empty() && message.bcc.is_empty() {
        return Err(Error::AccountConfig("не указан получатель".into()));
    }
    for address in message.to.iter().chain(&message.cc).chain(&message.bcc) {
        mailbox(address)?;
    }
    check_outgoing_size(message)
}

const MAX_ATTACHMENTS_BYTES: usize = 25 * 1024 * 1024;

fn check_outgoing_size(message: &OutgoingMessage) -> Result<()> {
    let inline_bytes: usize = message
        .body_html
        .as_deref()
        .filter(|html| !html.trim().is_empty())
        .map(|html| extract_inline_images(html).1)
        .unwrap_or_default()
        .iter()
        .map(|item| item.data.len())
        .sum();
    let total_size: usize = message
        .attachments
        .iter()
        .map(|item| item.data.len())
        .sum::<usize>()
        + inline_bytes;
    if total_size > MAX_ATTACHMENTS_BYTES {
        return Err(Error::AccountConfig(
            "суммарный размер вложений и встроенных картинок превышает 25 МБ".into(),
        ));
    }
    Ok(())
}

fn mailbox(value: &str) -> Result<Mailbox> {
    value
        .trim()
        .parse()
        .map_err(|error| Error::AccountConfig(format!("некорректный адрес {value:?}: {error}")))
}

/// Отправить письмо через официальный SMTP endpoint Яндекса с тем же OAuth
/// access token, что используется для IMAP.
pub async fn send_oauth(
    message: OutgoingMessage,
    access_token: &str,
    host: &str,
    port: u16,
    security: Security,
) -> Result<()> {
    send_oauth_with_raw(message, access_token, host, port, security)
        .await
        .map(|_| ())
        .map_err(|failure| failure.error)
}

/// Отправить MIME через SMTP и вернуть ровно те байты, которые были переданы
/// серверу. Они нужны IMAP-клиентам для APPEND той же копии в `\Sent`: повторная
/// сборка MIME дала бы другой Message-ID и другие границы multipart.
pub(crate) async fn send_oauth_with_raw(
    message: OutgoingMessage,
    access_token: &str,
    host: &str,
    port: u16,
    security: Security,
) -> std::result::Result<Vec<u8>, super::SendFailure> {
    let credentials = Credentials::new(message.from.clone(), access_token.to_owned());
    send_smtp(
        message,
        host,
        port,
        security,
        false,
        credentials,
        &[Mechanism::Xoauth2],
    )
    .await
}

const SMTP_TIMEOUT: Duration = Duration::from_secs(30);

async fn send_smtp(
    message: OutgoingMessage,
    host: &str,
    port: u16,
    security: Security,
    tls_insecure: bool,
    credentials: Credentials,
    mechanisms: &[Mechanism],
) -> std::result::Result<Vec<u8>, super::SendFailure> {
    if security == Security::None {
        return Err(Error::AccountConfig(
            "незашифрованный SMTP не поддерживается; выберите SSL/TLS или STARTTLS".into(),
        )
        .into());
    }
    let email = build_message(message)?;
    let raw = email.formatted();
    let hello = ClientId::default();
    let mut tls = TlsParameters::builder(host.to_owned());
    if tls_insecure {
        tracing::warn!(%host, "сертификат сервера отправки не проверяется по решению пользователя");
        tls = tls
            .dangerous_accept_invalid_certs(true)
            .dangerous_accept_invalid_hostnames(true);
    }
    let tls = tls.build().map_err(|error| smtp_error("smtp", error))?;
    let wrapper = (security == Security::Ssl).then(|| tls.clone());
    // Подключение, приветствие, TLS и вход происходят до команд отправки.
    let connect = async {
        let mut connection = AsyncSmtpConnection::connect_tokio1(
            (host, port),
            Some(SMTP_TIMEOUT),
            &hello,
            wrapper,
            None,
        )
        .await?;
        if security == Security::Starttls {
            connection.starttls(tls, &hello).await?;
        }
        connection.auth(mechanisms, &credentials).await?;
        Ok::<_, lettre::transport::smtp::Error>(connection)
    };
    let mut connection = tokio::time::timeout(SMTP_TIMEOUT, connect)
        .await
        .map_err(|_| {
            Error::classified_backend(
                "smtp",
                crate::ErrorKind::NetworkUnavailable,
                "сервер не ответил при подключении",
            )
        })?
        .map_err(|error| {
            if error.is_timeout() {
                Error::classified_backend(
                    "smtp",
                    crate::ErrorKind::NetworkUnavailable,
                    error.to_string(),
                )
            } else {
                smtp_error("smtp", error)
            }
        })?;
    submit_smtp(&mut connection, email.envelope(), &raw).await?;
    Ok(raw)
}

async fn submit_smtp(
    connection: &mut AsyncSmtpConnection,
    envelope: &lettre::address::Envelope,
    raw: &[u8],
) -> std::result::Result<(), super::SendFailure> {
    let mut options = Vec::new();
    if envelope
        .from()
        .into_iter()
        .chain(envelope.to())
        .any(|address| !address.as_ref().is_ascii())
    {
        if !connection
            .server_info()
            .supports_feature(Extension::SmtpUtfEight)
        {
            return Err(Error::AccountConfig(
                "сервер не поддерживает SMTPUTF8 для адреса письма".into(),
            )
            .into());
        }
        options.push(MailParameter::SmtpUtfEight);
    }
    if !raw.is_ascii() {
        if !connection
            .server_info()
            .supports_feature(Extension::EightBitMime)
        {
            return Err(Error::AccountConfig(
                "сервер не поддерживает 8BITMIME для содержимого письма".into(),
            )
            .into());
        }
        options.push(MailParameter::Body(MailBodyParameter::EightBitMime));
    }
    let begin = async {
        connection
            .command(Mail::new(envelope.from().cloned(), options))
            .await
            .map_err(|error| smtp_error("smtp", error))?;
        for address in envelope.to() {
            connection
                .command(Rcpt::new(address.clone(), Vec::new()))
                .await
                .map_err(|error| smtp_error("smtp", error))?;
        }
        let response = connection
            .command(Data)
            .await
            .map_err(|error| smtp_error("smtp", error))?;
        if u16::from(response.code()) != 354 {
            return Err(Error::classified_backend(
                "smtp",
                crate::ErrorKind::ServerUnavailable,
                "неожиданный ответ на команду DATA",
            ));
        }
        Ok(())
    };
    tokio::time::timeout(SMTP_TIMEOUT, begin)
        .await
        .map_err(|_| {
            Error::classified_backend(
                "smtp",
                crate::ErrorKind::Timeout,
                "сервер не ответил до передачи письма",
            )
        })??;
    let response = tokio::time::timeout(SMTP_TIMEOUT, connection.message(raw))
        .await
        .map_err(|_| {
            super::SendFailure::after_handoff(Error::classified_backend(
                "smtp",
                crate::ErrorKind::Timeout,
                "сервер не подтвердил передачу письма",
            ))
        })?
        .map_err(|error| smtp_failure("smtp", error))?;
    if response.code().severity != Severity::PositiveCompletion {
        return Err(super::SendFailure::after_handoff(
            Error::classified_backend(
                "smtp",
                crate::ErrorKind::Unknown,
                "сервер не подтвердил приём письма",
            ),
        ));
    }
    // Ответ о приёме получен. Сбой при закрытии уже не меняет итог передачи.
    let _ = tokio::time::timeout(Duration::from_secs(2), connection.quit()).await;
    Ok(())
}

pub async fn send_yandex(message: OutgoingMessage, access_token: &str) -> Result<()> {
    send_oauth(message, access_token, "smtp.yandex.com", 465, Security::Ssl).await
}

pub async fn send_gmail(message: OutgoingMessage, access_token: &str) -> Result<()> {
    send_gmail_raw(&build_message_with_bcc(message)?.formatted(), access_token).await
}

/// Запрос отправки Gmail по уже собранным байтам письма. Сборка письма отделена
/// от запроса: отказ сборки случается до обращения к серверу, и очередь
/// отправки различает их (S-048, S-049).
pub(crate) async fn send_gmail_raw(raw: &[u8], access_token: &str) -> Result<()> {
    use base64::Engine as _;
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw);
    let response = reqwest::Client::new()
        .post("https://gmail.googleapis.com/gmail/v1/users/me/messages/send")
        .bearer_auth(access_token)
        .json(&serde_json::json!({"raw":raw}))
        .send()
        .await
        .map_err(|error| Error::from_reqwest("gmail-send", error))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(Error::from_http_status(
            "gmail-send",
            status.as_u16(),
            format!("HTTP {status}: {body}"),
        ));
    }
    Ok(())
}

pub async fn send_password(
    message: OutgoingMessage,
    username: &str,
    password: &str,
    host: &str,
    port: u16,
    security: Security,
    tls_insecure: bool,
) -> Result<()> {
    send_password_with_raw(
        message,
        username,
        password,
        host,
        port,
        security,
        tls_insecure,
    )
    .await
    .map(|_| ())
    .map_err(|failure| failure.error)
}

pub(crate) async fn send_password_with_raw(
    message: OutgoingMessage,
    username: &str,
    password: &str,
    host: &str,
    port: u16,
    security: Security,
    tls_insecure: bool,
) -> std::result::Result<Vec<u8>, super::SendFailure> {
    send_smtp(
        message,
        host,
        port,
        security,
        tls_insecure,
        Credentials::new(username.to_owned(), password.to_owned()),
        DEFAULT_MECHANISMS,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn smtp_loss_of_final_reply_and_explicit_rejections_are_distinct() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        use tokio::net::TcpListener;

        for (drop_before_data, reply, ambiguous, kind) in [
            (
                true,
                None,
                false,
                Some(crate::ErrorKind::NetworkUnavailable),
            ),
            (false, None, true, None),
            (
                false,
                Some("451 Temporary failure\r\n"),
                false,
                Some(crate::ErrorKind::ServerUnavailable),
            ),
            (
                false,
                Some("550 Rejected\r\n"),
                false,
                Some(crate::ErrorKind::Unknown),
            ),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (connection, _) = listener.accept().await.unwrap();
                let mut stream = BufReader::new(connection);
                stream
                    .get_mut()
                    .write_all(b"220 test SMTP\r\n")
                    .await
                    .unwrap();
                let mut in_data = false;
                let mut line = String::new();
                loop {
                    line.clear();
                    assert!(stream.read_line(&mut line).await.unwrap() > 0);
                    if drop_before_data && line.starts_with("MAIL FROM:") {
                        break;
                    }
                    if in_data {
                        if line == ".\r\n" {
                            if let Some(reply) = reply {
                                stream.get_mut().write_all(reply.as_bytes()).await.unwrap();
                            }
                            break;
                        }
                    } else {
                        let response: &[u8] = if line == "DATA\r\n" {
                            in_data = true;
                            b"354 Continue\r\n"
                        } else {
                            b"250 OK\r\n"
                        };
                        stream.get_mut().write_all(response).await.unwrap();
                    }
                }
            });
            let mut connection = AsyncSmtpConnection::connect_tokio1(
                ("127.0.0.1", port),
                Some(Duration::from_secs(5)),
                &ClientId::default(),
                None,
                None,
            )
            .await
            .unwrap();
            let letter = build_message(sample_message(None, Vec::new())).unwrap();
            let failure = submit_smtp(&mut connection, letter.envelope(), &letter.formatted())
                .await
                .unwrap_err();
            server.await.unwrap();
            assert_eq!(failure.after_handoff, ambiguous);
            if let Some(kind) = kind {
                assert_eq!(failure.error.kind(), kind);
            }
        }
    }

    #[test]
    fn rejects_message_without_recipients_before_network() {
        let message = OutgoingMessage {
            from: "me@example.com".into(),
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: String::new(),
            body_text: String::new(),
            body_html: None,
            attachments: vec![],
            ..Default::default()
        };
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        assert!(runtime.block_on(send_yandex(message, "token")).is_err());
    }

    #[test]
    fn built_message_has_stable_id_for_append_deduplication() {
        let raw = build_message(OutgoingMessage {
            from: "me@example.com".into(),
            to: vec!["you@example.com".into()],
            cc: vec![],
            bcc: vec![],
            subject: "test".into(),
            body_text: "body".into(),
            body_html: None,
            attachments: vec![],
            ..Default::default()
        })
        .expect("message")
        .formatted();
        let headers = String::from_utf8_lossy(&raw);
        assert!(headers.lines().any(|line| {
            line.get(..11)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("Message-ID:"))
        }));
    }

    fn sample_message(
        body_html: Option<String>,
        attachments: Vec<OutgoingAttachment>,
    ) -> OutgoingMessage {
        OutgoingMessage {
            from: "me@example.com".into(),
            to: vec!["you@example.com".into()],
            cc: vec![],
            bcc: vec![],
            subject: "test".into(),
            body_text: "plain text version".into(),
            body_html,
            attachments,
            ..Default::default()
        }
    }

    fn data_url(mime: &str, bytes: &[u8]) -> String {
        use base64::Engine as _;
        format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )
    }

    // A server interface that takes the whole MIME text gets the hidden copy
    // inside the letter; SMTP keeps it in the envelope only.
    #[test]
    fn a_hidden_copy_stays_inside_the_letter_for_server_interfaces() {
        let mut message = sample_message(None, vec![]);
        message.bcc = vec!["hidden@example.com".into()];
        let for_server = build_message_with_bcc(message.clone())
            .expect("letter for a server interface")
            .formatted();
        assert!(
            String::from_utf8_lossy(&for_server).contains("Bcc: hidden@example.com"),
            "the hidden copy is lost between the queue and the server request"
        );
        let for_smtp = build_message(message).expect("letter for SMTP").formatted();
        assert!(
            !String::from_utf8_lossy(&for_smtp).contains("Bcc:"),
            "SMTP carries the hidden copy in the envelope, not in the letter"
        );
    }

    // A letter that cannot be assembled by its size must not reach the queue.
    #[test]
    fn an_oversized_letter_is_refused_before_the_queue() {
        let big = |fill: u8| OutgoingAttachment {
            filename: "big.bin".into(),
            mime_type: "application/octet-stream".into(),
            data: vec![fill; 13 * 1024 * 1024],
        };
        let fits = sample_message(None, vec![big(0)]);
        assert!(
            validate_outgoing(&fits).is_ok(),
            "a letter within the limit is refused"
        );
        let mut too_big = sample_message(None, vec![big(0), big(1)]);
        assert!(
            validate_outgoing(&too_big).is_err(),
            "attachments above the limit are accepted into the queue"
        );
        too_big.attachments.clear();
        too_big.body_html = Some(format!(
            "<p>text</p><img src=\"{}\"><img src=\"{}\">",
            data_url("image/png", &vec![0u8; 13 * 1024 * 1024]),
            data_url("image/png", &vec![1u8; 13 * 1024 * 1024])
        ));
        assert!(
            validate_outgoing(&too_big).is_err(),
            "inline pictures above the limit are accepted into the queue"
        );
    }

    // extract_inline_images - юнит-проверки чистого разбора текста (без сети и MIME).

    #[test]
    fn s028_s031_extract_inline_images_replaces_src_with_cid_without_brackets() {
        let bytes = vec![1u8, 2, 3, 4, 5];
        let html = format!(
            r#"<p>text</p><img src="{}">"#,
            data_url("image/png", &bytes)
        );
        let (replaced, parts) = extract_inline_images(&html);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].mime_type, "image/png");
        assert_eq!(parts[0].data, bytes);
        assert!(!parts[0].content_id.contains('<') && !parts[0].content_id.contains('>'));
        assert_eq!(
            replaced,
            format!(r#"<p>text</p><img src="cid:{}">"#, parts[0].content_id)
        );
    }

    #[test]
    fn s030_extract_inline_images_gives_different_ids_to_different_images() {
        let html = format!(
            r#"<img src="{}"><img src="{}">"#,
            data_url("image/png", &[1, 2, 3]),
            data_url("image/jpeg", &[4, 5, 6])
        );
        let (_, parts) = extract_inline_images(&html);
        assert_eq!(parts.len(), 2);
        assert_ne!(parts[0].content_id, parts[1].content_id);
    }

    #[test]
    fn s032_extract_inline_images_reuses_one_part_for_identical_data() {
        let url = data_url("image/gif", &[9, 9, 9]);
        let html = format!(r#"<img src="{url}"><p>middle</p><img src="{url}">"#);
        let (replaced, parts) = extract_inline_images(&html);
        assert_eq!(parts.len(), 1);
        let expected = format!(
            r#"<img src="cid:{0}"><p>middle</p><img src="cid:{0}">"#,
            parts[0].content_id
        );
        assert_eq!(replaced, expected);
    }

    #[test]
    fn s035_extract_inline_images_leaves_unsupported_type_untouched() {
        let html = format!(r#"<img src="{}">"#, data_url("image/svg+xml", b"<svg/>"));
        let (replaced, parts) = extract_inline_images(&html);
        assert!(parts.is_empty());
        assert_eq!(replaced, html);
    }

    #[test]
    fn parse_data_url_reads_base64_without_padding_like_the_interface_does() {
        // Интерфейс читает такую запись как картинку; ядро обязано читать так же,
        // иначе картинка ушла бы получателю строкой data:.
        let padded = parse_data_url("data:image/png;base64,QQ==").expect("с дополнением");
        let bare = parse_data_url("data:image/png;base64,QQ").expect("без дополнения");
        assert_eq!(padded.1, bare.1);
    }

    #[test]
    fn parse_data_url_takes_first_meta_segment_as_type() {
        let (mime, _) = parse_data_url("data:image/png;name=pic.png;base64,QQ==")
            .expect("тип из первого куска");
        assert_eq!(mime, "image/png");
    }

    #[test]
    fn s035_extract_inline_images_leaves_broken_base64_untouched() {
        let html = r#"<img src="data:image/png;base64,***not-base64***">"#;
        let (replaced, parts) = extract_inline_images(html);
        assert!(parts.is_empty());
        assert_eq!(replaced, html);
    }

    #[test]
    fn s036_extract_inline_images_leaves_http_link_untouched() {
        let html = r#"<img src="https://example.com/pic.png">"#;
        let (replaced, parts) = extract_inline_images(html);
        assert!(parts.is_empty());
        assert_eq!(replaced, html);
    }

    #[test]
    fn s041_extract_inline_images_ignores_case_and_accepts_single_quotes_and_extra_attrs() {
        let bytes = vec![7u8, 8, 9];
        let html = format!(
            r#"<IMG ALT='pic' SRC='{}' WIDTH="10">"#,
            data_url("image/bmp", &bytes)
        );
        let (replaced, parts) = extract_inline_images(&html);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].data, bytes);
        assert!(replaced.contains(&format!("cid:{}", parts[0].content_id)));
        assert!(replaced.contains("ALT='pic'"));
    }

    #[test]
    fn s041_extract_inline_images_uppercase_data_word_is_recognized() {
        let bytes = vec![1u8];
        let url = data_url("image/png", &bytes).to_ascii_uppercase();
        let html = format!(r#"<img src="{url}">"#);
        let (_, parts) = extract_inline_images(&html);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].data, bytes);
    }

    #[test]
    fn extract_inline_images_survives_multibyte_text_around_tag() {
        let bytes = vec![1u8, 2, 3];
        let html = format!(
            r#"<p>Привет, мир!</p><img src="{}"><p>Ещё текст.</p>"#,
            data_url("image/png", &bytes)
        );
        let (replaced, parts) = extract_inline_images(&html);
        assert_eq!(parts.len(), 1);
        assert!(replaced.starts_with("<p>Привет, мир!</p>"));
        assert!(replaced.ends_with("<p>Ещё текст.</p>"));
    }

    #[test]
    fn extract_inline_images_finds_image_after_document_type() {
        let html = r#"<!DOCTYPE html><body><img src="data:image/png;base64,AA=="> </body>"#;
        let (_, parts) = extract_inline_images(html);
        assert_eq!(parts.len(), 1);
    }

    #[test]
    fn extract_inline_images_skips_document_type_and_quoted_markup() {
        let html = r#"<p>Сначала</p><!DOCTYPE html><img src="data:image/png;base64,AA=="><p title="<img src='x'>">t</p>"#;
        let (replaced, parts) = extract_inline_images(html);
        assert_eq!(parts.len(), 1);
        assert!(replaced.ends_with(r#"<p title="<img src='x'>">t</p>"#));
    }

    #[test]
    fn extract_inline_images_ignores_unclosed_script() {
        let html = r#"<script>var s="<img src='data:image/png;base64,AA=='>";"#;
        let (replaced, parts) = extract_inline_images(html);
        assert!(parts.is_empty());
        assert_eq!(replaced, html);
    }

    #[test]
    fn extract_inline_images_no_images_returns_input_unchanged() {
        let html = "<p>просто текст без картинок</p>";
        let (replaced, parts) = extract_inline_images(html);
        assert!(parts.is_empty());
        assert_eq!(replaced, html);
    }

    // build_message - сквозные проверки собранного письма через mail_parser.

    #[test]
    fn s028_s029_build_message_wraps_inline_image_in_related_multipart() {
        use mail_parser::{MessageParser, MimeHeaders, PartType};
        let bytes = vec![10u8, 20, 30, 40];
        let html = format!(
            r#"<p>hello</p><img src="{}">"#,
            data_url("image/png", &bytes)
        );
        let raw = build_message(sample_message(Some(html), vec![]))
            .expect("message")
            .formatted();
        let raw_text = String::from_utf8_lossy(&raw);
        assert!(raw_text.contains("multipart/related"));
        let parsed = MessageParser::default().parse(&raw).expect("parse");
        let inline: Vec<_> = parsed
            .attachments()
            .filter(|part| part.content_id().is_some())
            .collect();
        assert_eq!(inline.len(), 1);
        let inline_bytes = match &inline[0].body {
            PartType::Binary(value) | PartType::InlineBinary(value) => value.as_ref(),
            _ => panic!("inline part is not binary"),
        };
        assert_eq!(inline_bytes, bytes.as_slice());
        let content_id = inline[0].content_id().expect("content id");
        let html_part = parsed.body_html(0).expect("html body");
        assert!(html_part.contains(&format!("cid:{content_id}")));
        assert!(!html_part.contains("data:image/png"));
    }

    #[test]
    fn s030_s032_build_message_gives_each_image_its_own_part_and_reuses_repeats() {
        use mail_parser::{MessageParser, MimeHeaders, PartType};
        let first = vec![1u8, 2, 3];
        let second = vec![9u8, 8, 7, 6];
        // Первая картинка повторяется дважды, вторая одна: в письме должно
        // оказаться две части, а повтор ссылается на ту же метку.
        let html = format!(
            r#"<img src="{a}"><img src="{b}"><img src="{a}">"#,
            a = data_url("image/png", &first),
            b = data_url("image/gif", &second)
        );
        let raw = build_message(sample_message(Some(html), vec![]))
            .expect("message")
            .formatted();
        let parsed = MessageParser::default().parse(&raw).expect("parse");
        let inline: Vec<_> = parsed
            .attachments()
            .filter(|part| part.content_id().is_some())
            .collect();
        assert_eq!(inline.len(), 2);
        let ids: Vec<String> = inline
            .iter()
            .map(|part| part.content_id().expect("content id").to_owned())
            .collect();
        assert_ne!(ids[0], ids[1]);
        let bodies: Vec<&[u8]> = inline
            .iter()
            .map(|part| match &part.body {
                PartType::Binary(value) | PartType::InlineBinary(value) => value.as_ref(),
                _ => panic!("inline part is not binary"),
            })
            .collect();
        assert!(bodies.contains(&first.as_slice()));
        assert!(bodies.contains(&second.as_slice()));
        let html_part = parsed.body_html(0).expect("html body");
        let repeated = ids
            .iter()
            .find(|id| html_part.matches(&format!("cid:{id}")).count() == 2)
            .expect("одна метка встречается дважды");
        assert_eq!(html_part.matches(&format!("cid:{repeated}")).count(), 2);
        assert!(!html_part.contains("data:image/"));
    }

    #[test]
    fn s033_build_message_keeps_ordinary_attachment_alongside_inline_image() {
        use mail_parser::{MessageParser, MimeHeaders, PartType};
        let inline_bytes = vec![1u8, 2, 3];
        let html = format!(r#"<img src="{}">"#, data_url("image/png", &inline_bytes));
        let attachment = OutgoingAttachment {
            filename: "report.pdf".into(),
            mime_type: "application/pdf".into(),
            data: vec![5u8, 6, 7, 8, 9],
        };
        let raw = build_message(sample_message(Some(html), vec![attachment.clone()]))
            .expect("message")
            .formatted();
        let parsed = MessageParser::default().parse(&raw).expect("parse");
        let ordinary: Vec<_> = parsed
            .attachments()
            .filter(|part| part.content_id().is_none())
            .collect();
        assert_eq!(ordinary.len(), 1);
        assert_eq!(ordinary[0].attachment_name(), Some("report.pdf"));
        let ordinary_bytes = match &ordinary[0].body {
            PartType::Binary(value) | PartType::InlineBinary(value) => value.as_ref(),
            _ => panic!("attachment is not binary"),
        };
        assert_eq!(ordinary_bytes, attachment.data.as_slice());
        let inline_count = parsed
            .attachments()
            .filter(|part| part.content_id().is_some())
            .count();
        assert_eq!(inline_count, 1);
    }

    #[test]
    fn s034_build_message_without_inline_images_keeps_previous_structure() {
        let raw = build_message(sample_message(Some("<p>без картинок</p>".into()), vec![]))
            .expect("message")
            .formatted();
        let raw_text = String::from_utf8_lossy(&raw);
        assert!(!raw_text.contains("multipart/related"));
        assert!(!raw_text.to_ascii_lowercase().contains("content-id:"));
    }

    #[test]
    fn s037_build_message_text_part_same_with_and_without_inline_image() {
        use mail_parser::MessageParser;
        let plain_html = "<p>без картинок</p>".to_string();
        let with_image_html = format!(
            r#"<p>без картинок</p><img src="{}">"#,
            data_url("image/png", &[1, 2, 3])
        );
        let raw_plain = build_message(sample_message(Some(plain_html), vec![]))
            .expect("message")
            .formatted();
        let raw_with_image = build_message(sample_message(Some(with_image_html), vec![]))
            .expect("message")
            .formatted();
        let text_plain = MessageParser::default()
            .parse(&raw_plain)
            .expect("parse")
            .body_text(0)
            .expect("text")
            .into_owned();
        let text_with_image = MessageParser::default()
            .parse(&raw_with_image)
            .expect("parse")
            .body_text(0)
            .expect("text")
            .into_owned();
        assert_eq!(text_plain, text_with_image);
        assert_eq!(text_plain, "plain text version");
    }

    #[test]
    fn s038_build_message_size_limit_counts_inline_image_bytes() {
        let big = vec![0u8; 25 * 1024 * 1024 + 1];
        let html = format!(r#"<img src="{}">"#, data_url("image/png", &big));
        let result = build_message(sample_message(Some(html), vec![]));
        assert!(result.is_err());
    }

    #[test]
    fn s038_build_message_size_limit_exact_boundary_passes() {
        let exact = vec![0u8; 25 * 1024 * 1024];
        let html = format!(r#"<img src="{}">"#, data_url("image/png", &exact));
        let result = build_message(sample_message(Some(html), vec![]));
        assert!(result.is_ok());
    }

    #[test]
    fn s042_build_message_with_unparsable_data_url_still_sends() {
        let html = r#"<p>text</p><img src="data:image/png;base64,***bad***">"#.to_string();
        let raw = build_message(sample_message(Some(html), vec![]))
            .expect("message")
            .formatted();
        let raw_text = String::from_utf8_lossy(&raw);
        assert!(raw_text.contains("data:image/png;base64,***bad***"));
    }

    #[test]
    fn s036_build_message_external_image_link_untouched() {
        use mail_parser::MessageParser;
        let html = r#"<img src="http://example.com/pic.png">"#.to_string();
        let raw = build_message(sample_message(Some(html), vec![]))
            .expect("message")
            .formatted();
        let parsed = MessageParser::default().parse(&raw).expect("parse");
        assert_eq!(parsed.attachments().count(), 0);
        let html_part = parsed.body_html(0).expect("html body");
        assert!(html_part.contains("http://example.com/pic.png"));
    }
}
