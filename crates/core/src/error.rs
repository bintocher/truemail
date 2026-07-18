//! Единый тип ошибок ядра.

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

    #[error("нет доступа: право «{0}» не выдано")]
    Forbidden(String),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Error::Other(e.to_string())
    }
}
