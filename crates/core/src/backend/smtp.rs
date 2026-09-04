//! Отправка почты через SMTP XOAUTH2.

use crate::model::Security;
use crate::{Error, Result};
use lettre::message::{Attachment, Mailbox, Message, MultiPart, SinglePart, header::ContentType};
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OutgoingAttachment {
    pub filename: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OutgoingMessage {
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body_text: String,
    pub body_html: Option<String>,
    pub attachments: Vec<OutgoingAttachment>,
}

/// Поддерживаемые типы встроенных картинок (S-004, S-035): svg+xml намеренно
/// не входит - это разметка, которая может содержать исполняемый код.
const SUPPORTED_INLINE_IMAGE_TYPES: [&str; 5] = [
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "image/bmp",
];

/// Картинка, вынесенная из текста письма в отдельную часть (S-028).
struct InlineImagePart {
    content_id: String,
    mime_type: String,
    data: Vec<u8>,
}

/// Ищет следующий тег `<img` начиная с `from`, независимо от регистра букв
/// (S-041). Возвращает байтовое смещение символа `<`.
fn find_img_tag_start(bytes: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i + 4 <= bytes.len() {
        if bytes[i] == b'<'
            && bytes[i + 1].eq_ignore_ascii_case(&b'i')
            && bytes[i + 2].eq_ignore_ascii_case(&b'm')
            && bytes[i + 3].eq_ignore_ascii_case(&b'g')
            && bytes
                .get(i + 4)
                .is_some_and(|b| b.is_ascii_whitespace() || *b == b'/' || *b == b'>')
        {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Ищет закрывающий `>` тега, начатого в `start`, не обрывая тег на `>`
/// внутри значения атрибута в кавычках (одинарных или двойных, S-041).
fn find_tag_end(bytes: &[u8], start: usize) -> Option<usize> {
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
fn find_src_attr(bytes: &[u8], tag_start: usize, tag_end: usize) -> Option<(usize, usize)> {
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

pub(crate) fn build_message(message: OutgoingMessage) -> Result<Message> {
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
    if total_size > 25 * 1024 * 1024 {
        return Err(Error::AccountConfig(
            "суммарный размер вложений и встроенных картинок превышает 25 МБ".into(),
        ));
    }
    let mut builder = Message::builder()
        .from(mailbox(&message.from)?)
        .message_id(None)
        .subject(message.subject);
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
    builder.multipart(mixed).map_err(|error| Error::Backend {
        backend: "smtp-message".into(),
        message: error.to_string(),
    })
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
) -> Result<Vec<u8>> {
    let from = message.from.clone();
    let email = build_message(message)?;
    let raw = email.formatted();
    let credentials = Credentials::new(from, access_token.to_owned());
    let builder = if security == Security::Starttls {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::relay(host)
    };
    let transport = builder
        .map_err(|error| Error::Backend {
            backend: "smtp".into(),
            message: error.to_string(),
        })?
        .port(port)
        .credentials(credentials)
        .authentication(vec![Mechanism::Xoauth2])
        .timeout(Some(std::time::Duration::from_secs(30)))
        .build();
    transport
        .send_raw(email.envelope(), &raw)
        .await
        .map_err(|error| Error::Backend {
            backend: "smtp".into(),
            message: error.to_string(),
        })?;
    Ok(raw)
}

pub async fn send_yandex(message: OutgoingMessage, access_token: &str) -> Result<()> {
    send_oauth(message, access_token, "smtp.yandex.com", 465, Security::Ssl).await
}

pub async fn send_gmail(message: OutgoingMessage, access_token: &str) -> Result<()> {
    use base64::Engine as _;
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(build_message(message)?.formatted());
    let response = reqwest::Client::new()
        .post("https://gmail.googleapis.com/gmail/v1/users/me/messages/send")
        .bearer_auth(access_token)
        .json(&serde_json::json!({"raw":raw}))
        .send()
        .await
        .map_err(|error| Error::Backend {
            backend: "gmail-send".into(),
            message: error.to_string(),
        })?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(Error::Backend {
            backend: "gmail-send".into(),
            message: format!("HTTP {status}: {body}"),
        });
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
) -> Result<()> {
    send_password_with_raw(message, username, password, host, port, security)
        .await
        .map(|_| ())
}

pub(crate) async fn send_password_with_raw(
    message: OutgoingMessage,
    username: &str,
    password: &str,
    host: &str,
    port: u16,
    security: Security,
) -> Result<Vec<u8>> {
    if security == Security::None {
        return Err(Error::AccountConfig(
            "незашифрованный SMTP не поддерживается; выберите SSL/TLS или STARTTLS".into(),
        ));
    }
    let email = build_message(message)?;
    let raw = email.formatted();
    let builder = if security == Security::Starttls {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::relay(host)
    }
    .map_err(|error| Error::Backend {
        backend: "smtp".into(),
        message: error.to_string(),
    })?;
    let transport = builder
        .port(port)
        .credentials(Credentials::new(username.to_owned(), password.to_owned()))
        .timeout(Some(std::time::Duration::from_secs(30)))
        .build();
    transport
        .send_raw(email.envelope(), &raw)
        .await
        .map_err(|error| Error::Backend {
            backend: "smtp".into(),
            message: error.to_string(),
        })?;
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        }
    }

    fn data_url(mime: &str, bytes: &[u8]) -> String {
        use base64::Engine as _;
        format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )
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
