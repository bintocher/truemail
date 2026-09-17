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

/// F3, error-kinds-and-messages.md S-014: временный разбор текста для
/// варианта `Error::Backend`, которым по-прежнему пользуются многие места
/// IMAP, EWS, Gmail и DAV, включая обрывы соединения и отказы сервера, и
/// который иначе всегда давал бы `Unknown` - интерфейс тогда предлагал бы
/// диагностику вместо повтора или переподключения. Типизированная
/// классификация (`ClassifiedBackend`, `from_http_status`, `from_reqwest`)
/// остаётся главной; этот разбор применяется только к тексту `Backend` и
/// только внутри ядра - интерфейс результат такого разбора не видит иначе,
/// чем через уже назначенный `ErrorKind`.
fn classify_backend_message(message: &str) -> ErrorKind {
    let text = message.to_ascii_lowercase();
    if text.contains("authenticationfailed")
        || text.contains("invalid credentials")
        || text.contains("login failed")
        || text.contains("authentication failed")
        || text.contains("неверн")
        || contains_number_token(&text, "401")
    {
        return ErrorKind::InvalidCredentials;
    }
    if contains_number_token(&text, "403") {
        return ErrorKind::Forbidden;
    }
    if contains_number_token(&text, "429") {
        return ErrorKind::RateLimited;
    }
    if contains_http_5xx(&text)
        || text.contains("connection refused")
        || text.contains("os error 10061")
    {
        return ErrorKind::ServerUnavailable;
    }
    if text.contains("connection reset")
        || text.contains("os error 10054")
        || text.contains("broken pipe")
        || text.contains("unexpected eof")
    {
        return ErrorKind::NetworkUnavailable;
    }
    if text.contains("timed out") || text.contains("timeout") {
        return ErrorKind::Timeout;
    }
    if text.contains("certificate")
        || text.contains("tls handshake")
        || text.contains("invalid peer certificate")
    {
        return ErrorKind::CertificateError;
    }
    ErrorKind::Unknown
}

/// Ищет `token` (короткое число вроде кода ответа HTTP) в `text` как
/// отдельное число - соседние байты по обе стороны не должны быть цифрами,
/// иначе, например, "403" нашёлся бы внутри произвольного "14035".
fn contains_number_token(text: &str, token: &str) -> bool {
    let bytes = text.as_bytes();
    let mut start = 0;
    while let Some(relative) = text[start..].find(token) {
        let index = start + relative;
        let before_ok = index == 0 || !bytes[index - 1].is_ascii_digit();
        let after = index + token.len();
        let after_ok = after >= bytes.len() || !bytes[after].is_ascii_digit();
        if before_ok && after_ok {
            return true;
        }
        start = index + 1;
    }
    false
}

/// Ищет в `text` трёхзначное число 500-599 (код ответа HTTP 5xx) как
/// отдельный токен, не являющийся частью более длинного числа.
fn contains_http_5xx(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        if bytes[i] == b'5' && bytes[i + 1].is_ascii_digit() && bytes[i + 2].is_ascii_digit() {
            let before_ok = i == 0 || !bytes[i - 1].is_ascii_digit();
            let after_ok = i + 3 == bytes.len() || !bytes[i + 3].is_ascii_digit();
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
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
        /// Код ответа HTTP, если он известен месту, где создана ошибка -
        /// напрямую, а не разбором текста (G2, error-kinds-and-messages.md).
        /// `response_code()` полагается на это поле и только при его
        /// отсутствии откатывается на разбор текста.
        response_code: Option<u16>,
    },

    /// Сервер запретил повторные запросы до указанного абсолютного момента.
    /// Отдельный вариант позволяет сохранить deadline в БД и пережить
    /// перезапуск desktop-приложения, не продлевая блокировку.
    #[error("транспорт ({backend}) временно ограничен до {retry_at}: {message}")]
    RateLimited {
        backend: String,
        retry_at: chrono::DateTime<chrono::Utc>,
        message: String,
        /// См. ClassifiedBackend::response_code - тот же смысл.
        response_code: Option<u16>,
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
            Self::Json(_) | Self::Other(_) => ErrorKind::Unknown,
            Self::Backend { message, .. } => classify_backend_message(message),
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
        Self::classified_backend_with_code(backend, kind, message, None)
    }

    /// То же самое, но код ответа HTTP уже известен вызывающему коду - его
    /// не нужно (и не всегда можно надёжно) вычислять разбором текста
    /// сообщения (G2, error-kinds-and-messages.md).
    pub fn classified_backend_with_code(
        backend: impl Into<String>,
        kind: ErrorKind,
        message: impl Into<String>,
        response_code: Option<u16>,
    ) -> Self {
        Self::ClassifiedBackend {
            backend: backend.into(),
            kind,
            message: message.into(),
            response_code,
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
    pub fn from_http_status(
        backend: impl Into<String>,
        status: u16,
        message: impl Into<String>,
    ) -> Self {
        let backend = backend.into();
        let message = message.into();
        match status {
            401 => Self::classified_backend_with_code(
                backend,
                ErrorKind::InvalidCredentials,
                message,
                Some(status),
            ),
            403 => Self::classified_backend_with_code(
                backend,
                ErrorKind::Forbidden,
                message,
                Some(status),
            ),
            408 => Self::classified_backend_with_code(
                backend,
                ErrorKind::Timeout,
                message,
                Some(status),
            ),
            429 => Self::RateLimited {
                backend,
                retry_at: chrono::Utc::now() + chrono::Duration::minutes(1),
                message,
                response_code: Some(status),
            },
            500..=599 => Self::classified_backend_with_code(
                backend,
                ErrorKind::ServerUnavailable,
                message,
                Some(status),
            ),
            _ => Self::classified_backend_with_code(
                backend,
                ErrorKind::Unknown,
                message,
                Some(status),
            ),
        }
    }

    pub fn retry_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        match self {
            Self::RateLimited { retry_at, .. } => Some(*retry_at),
            _ => None,
        }
    }

    /// Имя транспорта, если ошибка пришла от удалённого сервера.
    pub fn backend(&self) -> Option<&str> {
        match self {
            Self::Backend { backend, .. }
            | Self::ClassifiedBackend { backend, .. }
            | Self::RateLimited { backend, .. } => Some(backend),
            _ => None,
        }
    }

    /// Код ответа HTTP. Сначала - поле, заполненное на месте создания
    /// ошибки (G2: там код известен точно), и только если поля нет
    /// (старый Backend или ClassifiedBackend/RateLimited, для которых код не
    /// передали) - запасной разбор текста сообщения.
    pub fn response_code(&self) -> Option<u16> {
        match self {
            Self::ClassifiedBackend {
                response_code,
                message,
                ..
            }
            | Self::RateLimited {
                response_code,
                message,
                ..
            } => response_code.or_else(|| http_response_code(message)),
            Self::Backend { message, .. } => http_response_code(message),
            _ => None,
        }
    }

    pub fn requires_reauth(&self) -> bool {
        self.kind().requires_reauth()
    }
}

/// Запасной разбор текста - только для ошибок без отдельного поля кода.
/// Ищет не любое вхождение подстроки "http" (она находится и внутри адреса,
/// например в порте "https://host:443/...", откуда раньше и брался
/// ошибочный код), а токен "HTTP" с ровно одним пробелом и трёхзначным
/// числом сразу после него - как в реальных сообщениях транспорта вида
/// "HTTP 503: ...".
fn http_response_code(message: &str) -> Option<u16> {
    let upper = message.to_ascii_uppercase();
    let bytes = upper.as_bytes();
    let mut start = 0;
    while let Some(relative) = upper[start..].find("HTTP") {
        let index = start + relative;
        let after_marker = index + 4;
        if bytes.get(after_marker) == Some(&b' ')
            && let Some(code) = parse_three_digit_code(bytes, after_marker + 1)
        {
            return Some(code);
        }
        start = index + 4;
    }
    None
}

/// Разбирает ровно три цифры начиная с `start` как код ответа HTTP -
/// соседние байты по обе стороны не должны быть цифрами, иначе число длиннее
/// трёх знаков (например, порт "44300").
fn parse_three_digit_code(bytes: &[u8], start: usize) -> Option<u16> {
    let digits = bytes.get(start..start + 3)?;
    if !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    if bytes.get(start + 3).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let value: u16 = std::str::from_utf8(digits).ok()?.parse().ok()?;
    (100..=599).contains(&value).then_some(value)
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
                    response_code: None,
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
    fn f3_backend_message_text_is_classified_when_no_typed_kind_exists() {
        // F3: разбор текста только для Backend - типизированная классификация
        // (ClassifiedBackend) остаётся главной и разбору не подвергается.
        let cases = [
            ("AuthenticationFailed", "invalid_credentials"),
            ("Invalid credentials for user", "invalid_credentials"),
            ("Login failed: bad password", "invalid_credentials"),
            ("Authentication failed", "invalid_credentials"),
            ("HTTP 401 Unauthorized", "invalid_credentials"),
            ("сервер вернул: неверный пароль", "invalid_credentials"),
            ("HTTP 403 Forbidden", "forbidden"),
            ("HTTP 429 Too Many Requests", "rate_limited"),
            ("HTTP 503 Service Unavailable", "server_unavailable"),
            ("HTTP 500 Internal Server Error", "server_unavailable"),
            ("connect: Connection refused", "server_unavailable"),
            ("os error 10061", "server_unavailable"),
            ("Connection reset by peer", "network_unavailable"),
            ("os error 10054", "network_unavailable"),
            ("Broken pipe", "network_unavailable"),
            ("unexpected eof", "network_unavailable"),
            ("operation timed out", "timeout"),
            ("read timeout", "timeout"),
            ("certificate verify failed", "certificate_error"),
            ("TLS handshake failed", "certificate_error"),
            (
                "invalid peer certificate: UnknownIssuer",
                "certificate_error",
            ),
            ("что-то совсем непонятное", "unknown"),
        ];
        for (message, expected) in cases {
            let error = Error::Backend {
                backend: "imap".into(),
                message: message.into(),
            };
            assert_eq!(error.code(), expected, "текст: {message}");
        }
    }

    #[test]
    fn response_code_is_returned_only_when_http_code_is_known() {
        let http = Error::Backend {
            backend: "ews-http".into(),
            message: "HTTP 500 Internal Server Error".into(),
        };
        let network = Error::Backend {
            backend: "imap-idle".into(),
            message: "os error 10054".into(),
        };
        assert_eq!(http.response_code(), Some(500));
        assert_eq!(network.response_code(), None);
        assert_eq!(http.backend(), Some("ews-http"));
    }

    // G2: response_code() берётся из поля, а не разбором текста. Раньше
    // запасной разбор искал любую подстроку "http" и находил её внутри
    // адреса из https://..., откуда брал порт (443) вместо настоящего кода
    // ответа (503) чуть дальше в том же сообщении.
    #[test]
    fn from_http_status_carries_the_real_code_even_with_a_port_in_the_message() {
        let error = Error::from_http_status(
            "dav",
            503,
            "GET https://caldav.example.com:443/dav/calendars/: HTTP 503: Service Unavailable",
        );
        assert_eq!(error.response_code(), Some(503));
    }

    // Тот же случай для запасного разбора текста (ошибки без поля кода) -
    // он должен опираться на "HTTP <код>", а не на любое вхождение "http".
    #[test]
    fn fallback_text_parsing_ignores_a_port_that_looks_like_a_response_code() {
        let error = Error::Backend {
            backend: "dav".into(),
            message:
                "GET https://caldav.example.com:443/dav/calendars/: HTTP 503: Service Unavailable"
                    .into(),
        };
        assert_eq!(error.response_code(), Some(503));
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
