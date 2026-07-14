//! Аккаунт и его конфигурация подключения.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Yandex,
    Mailru,
    Icloud,
    Exchange,
    Gmail,
    Outlook,
    Generic,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BackendKind {
    Imap,
    Ews,
    Jmap,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthKind {
    Oauth2,
    AppPassword,
    Password,
    Ntlm,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Security {
    Ssl,
    Starttls,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub security: Security,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: i64,
    pub uuid: String,
    pub email: String,
    pub display_name: String,
    pub provider: Provider,
    pub backend_kind: BackendKind,
    pub auth_kind: AuthKind,
    pub imap: Option<ServerConfig>,
    pub smtp: Option<ServerConfig>,
    pub ews_url: Option<String>,
    pub username: Option<String>,
    /// Имя записи в системном keychain; сам секрет в SQLite не хранится.
    #[serde(skip_serializing)]
    pub secret_ref: Option<String>,
    pub include_in_unified: bool,
    pub color: Option<String>,
    pub enabled: bool,
}

/// Provider-neutral account configuration accepted by the storage layer.
/// Protocol adapters own their defaults; SQLite only persists the values.
#[derive(Debug, Clone)]
pub struct NewAccount {
    pub email: String,
    pub display_name: String,
    pub provider: Provider,
    pub backend_kind: BackendKind,
    pub auth_kind: AuthKind,
    pub imap: Option<ServerConfig>,
    pub smtp: Option<ServerConfig>,
    pub ews_url: Option<String>,
    pub username: Option<String>,
    pub secret_ref: String,
    pub color: Option<String>,
}

/// Подпись аккаунта (раздельно для новых писем и ответов).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    /// "new" | "reply"
    pub kind: String,
    pub body_html: String,
    pub enabled: bool,
}
