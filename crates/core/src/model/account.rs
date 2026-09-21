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
    /// Сертификат этого сервера не проверяется (issue #118). Признак живёт
    /// рядом с адресом сервера, а не отдельным перечнем: решение принимает
    /// владелец ящика, и соседний ящик на том же сервере его не наследует.
    /// Для старых записей и для всех путей, где признак не задан, проверка
    /// остаётся включённой.
    #[serde(default)]
    pub tls_insecure: bool,
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
    pub jmap_url: Option<String>,
    /// Базовый адрес CalDAV: задан вручную или обнаружен по RFC 6764
    /// (.well-known/caldav) и закэширован здесь, чтобы не искать заново.
    pub caldav_url: Option<String>,
    /// Базовый адрес CardDAV; см. caldav_url.
    pub carddav_url: Option<String>,
    pub username: Option<String>,
    /// Имя записи в системном keychain; сам секрет в SQLite не хранится.
    #[serde(skip_serializing)]
    pub secret_ref: Option<String>,
    pub include_in_unified: bool,
    pub color: Option<String>,
    /// Глубина локального кэша писем в днях; 0 - без ограничений.
    pub retention_days: i64,
    pub enabled: bool,
    /// Время последнего успешного прохода синхронизации почты
    /// (mail-sync-visible-state.md). None - успешных проходов ещё не было.
    pub last_sync_at: Option<String>,
    /// Безопасный текст последней ошибки синхронизации почты; очищается при
    /// следующем успешном проходе.
    pub last_sync_error: Option<String>,
    /// Машиночитаемый вид последней ошибки (error-kinds-and-messages.md).
    pub last_sync_error_kind: Option<String>,
    /// Синхронизация не сможет продолжиться без обновления учётных данных
    /// пользователем - вид последней ошибки входит в список требующих
    /// повторного входа.
    pub needs_reauth: bool,
    /// Сертификат сервера этого ящика не проверяется (issue #118). Нужно для
    /// серверов организации с самоподписанным сертификатом: без этого ящик
    /// не подключить вовсе. Признак принадлежит ящику, а не программе -
    /// соседние ящики проверяются как обычно. По умолчанию проверка включена.
    pub tls_insecure: bool,
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
    pub jmap_url: Option<String>,
    /// См. Account::caldav_url. При создании аккаунта обычно None -
    /// обнаруживается позже, при первой синхронизации календаря/контактов.
    pub caldav_url: Option<String>,
    pub carddav_url: Option<String>,
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
