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

fn build_message(message: OutgoingMessage) -> Result<Message> {
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
) -> Result<()> {
    let from = message.from.clone();
    let email = build_message(message)?;
    let credentials = Credentials::new(from, access_token.to_owned());
    let transport = AsyncSmtpTransport::<Tokio1Executor>::relay(host)
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
        .send(email)
        .await
        .map_err(|error| Error::Backend {
            backend: "smtp".into(),
            message: error.to_string(),
        })?;
    Ok(())
}

pub async fn send_yandex(message: OutgoingMessage, access_token: &str) -> Result<()> {
    send_oauth(message, access_token, "smtp.yandex.com", 465).await
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
    if security == Security::None {
        return Err(Error::AccountConfig(
            "незашифрованный SMTP не поддерживается; выберите SSL/TLS или STARTTLS".into(),
        ));
    }
    let email = build_message(message)?;
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
