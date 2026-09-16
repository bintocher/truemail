//! Единый тип ошибок ядра.

/// Устойчивый вид ошибки для решений интерфейса.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    InvalidCredentials,
    NeedsReauth,
    RateLimited,
    Timeout,
    NetworkUnavailable,
    CertificateError,
    ServerUnavailable,
    Forbidden,
    AccountConfig,
    StorageError,
    SecretStoreError,
    CryptoError,
    Unknown,
}

impl ErrorKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidCredentials => "invalid_credentials",
            Self::NeedsReauth => "needs_reauth",
            Self::RateLimited => "rate_limited",
            Self::Timeout => "timeout",
            Self::NetworkUnavailable => "network_unavailable",
            Self::CertificateError => "certificate_error",
            Self::ServerUnavailable => "server_unavailable",
            Self::Forbidden => "forbidden",
            Self::AccountConfig => "account_config",
            Self::StorageError => "storage_error",
            Self::SecretStoreError => "secret_store_error",
            Self::CryptoError => "crypto_error",
            Self::Unknown => "unknown",
        }
    }

    pub const fn requires_reauth(self) -> bool {
        matches!(self, Self::InvalidCredentials | Self::NeedsReauth)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("хранилище: {0}")]
    Db(#[from] sqlx::Error),

    #[error("ввод-вывод: {0}")]
    Io(#[from] std::io::Error),

    #[error("сериализация: {0}")]
    Json(#[from] serde_json::Error),

    #[error("keychain: {0}")]
    Keyring(String),

    #[error("шифрование хранилища: {0}")]
    Crypto(String),

    #[error("транспорт ({backend}): {message}")]
    Backend { backend: String, message: String },

    /// Типизированная транспортная ошибка. Старый Backend остается для мест,
    /// где библиотека не дает достаточно данных для точной классификации.
    #[error("транспорт ({backend}): {message}")]
    ClassifiedBackend {
        backend: String,
        kind: ErrorKind,
        message: String,
    },

    /// Сервер запретил повторные запросы до указанного абсолютного момента.
    /// Отдельный вариант позволяет сохранить deadline в БД и пережить
    /// перезапуск desktop-приложения, не продлевая блокировку.
    #[error("транспорт ({backend}) временно ограничен до {retry_at}: {message}")]
    RateLimited {
        backend: String,
        retry_at: chrono::DateTime<chrono::Utc>,
        message: String,
    },

    #[error("аккаунт не настроен: {0}")]
    AccountConfig(String),

    #[error("нет доступа: право '{0}' не выдано")]
    Forbidden(String),

    #[error("{0}")]
    Other(String),
}

impl Error {
    /// Устойчивый код для интерфейса. Интерфейс не должен разбирать текст.
    pub fn code(&self) -> &'static str {
        self.kind().code()
    }

    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Db(_) => ErrorKind::StorageError,
            Self::Io(error) => match error.kind() {
                std::io::ErrorKind::TimedOut => ErrorKind::Timeout,
                std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::ConnectionRefused
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::HostUnreachable
                | std::io::ErrorKind::NetworkDown
                | std::io::ErrorKind::NetworkUnreachable
                | std::io::ErrorKind::NotConnected
                | std::io::ErrorKind::BrokenPipe
                | std::io::ErrorKind::UnexpectedEof => ErrorKind::NetworkUnavailable,
                _ => ErrorKind::StorageError,
            },
            Self::Json(_) | Self::Backend { .. } | Self::Other(_) => ErrorKind::Unknown,
            Self::Keyring(_) => ErrorKind::SecretStoreError,
            Self::Crypto(_) => ErrorKind::CryptoError,
            Self::ClassifiedBackend { kind, .. } => *kind,
            Self::RateLimited { .. } => ErrorKind::RateLimited,
            Self::AccountConfig(_) => ErrorKind::AccountConfig,
            Self::Forbidden(_) => ErrorKind::Forbidden,
        }
    }

    pub fn classified_backend(
        backend: impl Into<String>,
        kind: ErrorKind,
        message: impl Into<String>,
    ) -> Self {
        Self::ClassifiedBackend {
            backend: backend.into(),
            kind,
            message: message.into(),
        }
    }

    /// Классификация reqwest основана на типизированных признаках исключения.
    pub fn from_reqwest(backend: impl Into<String>, error: reqwest::Error) -> Self {
        use std::error::Error as _;
        let mut source = error.source();
        let mut certificate_error = false;
        while let Some(cause) = source {
            if cause
                .downcast_ref::<tokio_rustls::rustls::Error>()
                .is_some()
            {
                certificate_error = true;
                break;
            }
            source = cause.source();
        }
        let kind = if certificate_error {
            ErrorKind::CertificateError
        } else if error.is_timeout() {
            ErrorKind::Timeout
        } else if error.is_connect() {
            ErrorKind::NetworkUnavailable
        } else {
            ErrorKind::Unknown
        };
        Self::classified_backend(backend, kind, error.to_string())
    }

    /// Вид ошибки по коду ответа HTTP: единое правило для всех транспортов,
    /// которые общаются по HTTP (JMAP, Gmail API, CalDAV и CardDAV, отправка
    /// Gmail). Успешные коды сюда не приходят.
    pub fn from_http_status(backend: impl Into<String>, status: u16, message: impl Into<String>) -> Self {
        let backend = backend.into();
        let message = message.into();
        match status {
            401 => Self::classified_backend(backend, ErrorKind::InvalidCredentials, message),
            403 => Self::classified_backend(backend, ErrorKind::Forbidden, message),
            408 => Self::classified_backend(backend, ErrorKind::Timeout, message),
            429 => Self::RateLimited {
                backend,
                retry_at: chrono::Utc::now() + chrono::Duration::minutes(1),
                message,
            },
            500..=599 => Self::classified_backend(backend, ErrorKind::ServerUnavailable, message),
            _ => Self::classified_backend(backend, ErrorKind::Unknown, message),
        }
    }

    pub fn retry_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        match self {
            Self::RateLimited { retry_at, .. } => Some(*retry_at),
            _ => None,
        }
    }

    pub fn requires_reauth(&self) -> bool {
        self.kind().requires_reauth()
    }
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Error::Other(e.to_string())
    }
}

/// Удаляет именованные секреты до записи в журнал или отправки интерфейсу.
/// Значение ищется только после известного имени и разделителя, поэтому
/// случайная строка похожего формата сама по себе не считается секретом.
pub fn sanitize_error_message(message: &str) -> String {
    const NAMES: &[&str] = &[
        "authorization",
        "password",
        "verification_code",
        "confirmation_code",
        "code",
        "access_token",
        "refresh_token",
        "token",
        "body_text",
        "body_html",
        "message_content",
        "raw",
    ];
    let mut sanitized = message.to_owned();
    for name in NAMES {
        sanitized = redact_named_value(&sanitized, name);
    }
    sanitized
}

fn redact_named_value(input: &str, name: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let mut ranges = Vec::new();
    let mut search_from = 0;
    while let Some(relative) = lower[search_from..].find(name) {
        let name_start = search_from + relative;
        let name_end = name_start + name.len();
        let before_ok =
            name_start == 0 || !lower.as_bytes()[name_start - 1].is_ascii_alphanumeric();
        let after_ok = lower
            .as_bytes()
            .get(name_end)
            .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_');
        if !before_ok || !after_ok {
            search_from = name_end;
            continue;
        }
        let bytes = input.as_bytes();
        let mut cursor = name_end;
        while matches!(bytes.get(cursor), Some(b' ' | b'\t' | b'"' | b'\'')) {
            cursor += 1;
        }
        let Some(separator) = bytes.get(cursor) else {
            break;
        };
        if !matches!(separator, b':' | b'=') {
            search_from = name_end;
            continue;
        }
        cursor += 1;
        while matches!(bytes.get(cursor), Some(b' ' | b'\t')) {
            cursor += 1;
        }
        let quote = bytes
            .get(cursor)
            .copied()
            .filter(|byte| matches!(byte, b'"' | b'\''));
        if quote.is_some() {
            cursor += 1;
        }
        let value_start = cursor;
        let value_end = if name.eq_ignore_ascii_case("authorization") {
            input[value_start..]
                .find(['\r', '\n'])
                .map(|end| value_start + end)
                .unwrap_or(input.len())
        } else if let Some(quote) = quote {
            input.as_bytes()[value_start..]
                .iter()
                .position(|byte| *byte == quote)
                .map(|end| value_start + end)
                .unwrap_or(input.len())
        } else {
            input.as_bytes()[value_start..]
                .iter()
                .position(|byte| {
                    matches!(
                        byte,
                        b'&' | b',' | b';' | b'}' | b'\r' | b'\n' | b' ' | b'\t'
                    )
                })
                .map(|end| value_start + end)
                .unwrap_or(input.len())
        };
        if value_end > value_start && &input[value_start..value_end] != "[скрыто]" {
            ranges.push((value_start, value_end));
        }
        search_from = value_end.max(name_end);
    }
    if let Some((start, end)) = ranges.into_iter().next() {
        let mut value = input.to_owned();
        value.replace_range(start..end, "[скрыто]");
        return redact_named_value(&value, name);
    }
    input.to_owned()
}

#[cfg(test)]
mod tests {
    use super::{Error, ErrorKind, sanitize_error_message};

    #[test]
    fn http_status_defines_the_error_kind() {
        // Общее правило для всех транспортов поверх HTTP: вид ошибки берётся из
        // кода ответа, а не из текста тела (S-002, S-012).
        let cases = [
            (401_u16, "invalid_credentials"),
            (403, "forbidden"),
            (408, "timeout"),
            (429, "rate_limited"),
            (500, "server_unavailable"),
            (503, "server_unavailable"),
            (418, "unknown"),
        ];
        for (status, expected) in cases {
            let error = Error::from_http_status("dav", status, "тело ответа");
            assert_eq!(error.code(), expected, "код ответа {status}");
        }
    }

    #[test]
    fn every_general_error_has_a_stable_code() {
        let cases = [
            (Error::Db(sqlx::Error::RowNotFound), "storage_error"),
            (
                Error::Io(std::io::Error::new(std::io::ErrorKind::TimedOut, "x")),
                "timeout",
            ),
            (
                Error::Io(std::io::Error::new(
                    std::io::ErrorKind::ConnectionReset,
                    "x",
                )),
                "network_unavailable",
            ),
            (
                Error::Json(serde_json::from_str::<serde_json::Value>("{").unwrap_err()),
                "unknown",
            ),
            (Error::Keyring("x".into()), "secret_store_error"),
            (Error::Crypto("x".into()), "crypto_error"),
            (
                Error::Backend {
                    backend: "x".into(),
                    message: "x".into(),
                },
                "unknown",
            ),
            (
                Error::RateLimited {
                    backend: "x".into(),
                    retry_at: chrono::Utc::now(),
                    message: "x".into(),
                },
                "rate_limited",
            ),
            (Error::AccountConfig("x".into()), "account_config"),
            (Error::Forbidden("x".into()), "forbidden"),
            (Error::Other("x".into()), "unknown"),
        ];
        for (error, code) in cases {
            assert_eq!(error.code(), code);
        }
        for kind in [
            ErrorKind::InvalidCredentials,
            ErrorKind::NeedsReauth,
            ErrorKind::RateLimited,
            ErrorKind::CertificateError,
            ErrorKind::ServerUnavailable,
        ] {
            assert_eq!(
                Error::classified_backend("test", kind, "x").code(),
                kind.code()
            );
        }
    }

    #[test]
    fn only_authentication_kinds_require_a_new_login() {
        assert!(ErrorKind::InvalidCredentials.requires_reauth());
        assert!(ErrorKind::NeedsReauth.requires_reauth());
        for kind in [
            ErrorKind::RateLimited,
            ErrorKind::Timeout,
            ErrorKind::NetworkUnavailable,
            ErrorKind::CertificateError,
            ErrorKind::ServerUnavailable,
            ErrorKind::Forbidden,
            ErrorKind::AccountConfig,
            ErrorKind::StorageError,
            ErrorKind::SecretStoreError,
            ErrorKind::CryptoError,
            ErrorKind::Unknown,
        ] {
            assert!(!kind.requires_reauth(), "{}", kind.code());
        }
    }

    #[test]
    fn named_secrets_and_message_content_are_redacted() {
        let source = "Authorization: Bearer abc\npassword=hunter2 code=confirm token='oauth-key' access_token=one refresh_token=two body_text=letter";
        let safe = sanitize_error_message(source);
        assert!(!safe.contains("abc"));
        assert!(!safe.contains("hunter2"));
        assert!(!safe.contains("confirm"));
        assert!(!safe.contains("oauth-key"));
        assert!(!safe.contains("letter"));
        assert_eq!(safe.matches("[скрыто]").count(), 7);
        assert_eq!(sanitize_error_message("opaque abc.def"), "opaque abc.def");
    }
}
