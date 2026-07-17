//! Письмо (RFC 5322 / MIME). Оригинал храним неизменным (нужен для DKIM/PGP).

use super::{Addr, AuthResults};
use serde::{Deserialize, Serialize};

/// Флаги письма (IMAP + пользовательские).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Flags {
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
    pub draft: bool,
}

/// Метаданные письма для списка и треда (без тела).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMeta {
    pub id: i64,
    pub account_id: i64,
    pub folder_id: i64,
    pub thread_id: Option<i64>,
    pub uid: u32,
    pub message_id: Option<String>,
    pub from: Addr,
    pub to: Vec<Addr>,
    pub cc: Vec<Addr>,
    pub subject: String,
    pub preview: String,
    /// ISO 8601
    pub date: Option<String>,
    pub size: Option<i64>,
    pub flags: Flags,
    pub has_attachments: bool,
    pub auth: AuthResults,
    pub labels: Vec<String>,
}

/// Вложение (метаданные; содержимое подгружается по запросу).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: i64,
    pub filename: String,
    pub mime_type: Option<String>,
    pub size: Option<i64>,
    pub is_inline: bool,
    pub content_id: Option<String>,
    pub fetched: bool,
}

/// Полное письмо с телом (для просмотра).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageFull {
    pub meta: MessageMeta,
    pub body_html: Option<String>,
    pub body_text: Option<String>,
    pub attachments: Vec<Attachment>,
    /// True, если письмо содержит внешние ресурсы (для плашки блокировки).
    pub has_remote_content: bool,
    /// True, если это рассылка (есть заголовок List-Unsubscribe).
    pub is_newsletter: bool,
    pub unsubscribe: Option<Unsubscribe>,
}

/// Сохраняемый пользователем шаблон нового письма.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageTemplate {
    pub id: i64,
    pub account_id: i64,
    pub name: String,
    pub subject: String,
    pub body_html: String,
}

/// Данные для отписки от рассылки (RFC 2369 / 8058).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unsubscribe {
    pub one_click_url: Option<String>,
    pub mailto: Option<String>,
    pub http: Option<String>,
}

/// Черновик исходящего письма.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Draft {
    pub account_id: i64,
    pub from: Option<Addr>,
    pub to: Vec<Addr>,
    pub cc: Vec<Addr>,
    pub bcc: Vec<Addr>,
    pub subject: String,
    pub body_html: String,
    pub attachments: Vec<DraftAttachment>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftAttachment {
    pub filename: String,
    pub mime_type: String,
    pub bytes_ref: String,
}
