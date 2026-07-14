//! Отправка почты через SMTP XOAUTH2.

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

fn mailbox(value: &str) -> Result<Mailbox> {
    value
        .trim()
        .parse()
        .map_err(|error| Error::AccountConfig(format!("некорректный адрес {value:?}: {error}")))
}

/// Отправить письмо через официальный SMTP endpoint Яндекса с тем же OAuth
/// access token, что используется для IMAP.
pub async fn send_yandex(message: OutgoingMessage, access_token: &str) -> Result<()> {
    if message.to.is_empty() && message.cc.is_empty() && message.bcc.is_empty() {
        return Err(Error::AccountConfig("не указан получатель".into()));
    }
    let total_size: usize = message.attachments.iter().map(|item| item.data.len()).sum();
    if total_size > 25 * 1024 * 1024 {
        return Err(Error::AccountConfig(
            "суммарный размер вложений превышает 25 МБ".into(),
        ));
    }

    let mut builder = Message::builder()
        .from(mailbox(&message.from)?)
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

    let alternative = if let Some(html) = message.body_html.filter(|html| !html.trim().is_empty()) {
        MultiPart::alternative()
            .singlepart(SinglePart::plain(message.body_text))
            .singlepart(SinglePart::html(html))
    } else {
        MultiPart::alternative().singlepart(SinglePart::plain(message.body_text))
    };
    let mut mixed = MultiPart::mixed().multipart(alternative);
    for item in message.attachments {
        let content_type = item
            .mime_type
            .parse::<ContentType>()
            .unwrap_or(ContentType::parse("application/octet-stream").expect("valid MIME"));
        mixed = mixed.singlepart(Attachment::new(item.filename).body(item.data, content_type));
    }
    let email = builder.multipart(mixed).map_err(|error| Error::Backend {
        backend: "smtp-message".into(),
        message: error.to_string(),
    })?;

    let credentials = Credentials::new(message.from, access_token.to_owned());
    let transport = AsyncSmtpTransport::<Tokio1Executor>::relay("smtp.yandex.com")
        .map_err(|error| Error::Backend {
            backend: "smtp".into(),
            message: error.to_string(),
        })?
        .port(465)
        .credentials(credentials)
        .authentication(vec![Mechanism::Xoauth2])
        .timeout(Some(std::time::Duration::from_secs(30)))
        .build();
    transport
        .send(email)
        .await
        .map_err(|error| Error::Backend {
            backend: "smtp".into(),
            message: error.to_string(),
        })?;
    Ok(())
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
}
