//! truemail-core — ядро почтового клиента.
//!
//! Принцип: одно ядро, тонкие клиенты (десктоп-GUI и внешний API).
//! RFC — каноническая модель данных; backend-адаптеры канонизируют проприетарное.
//! Всё локальное хранилище шифруется (см. `crypto`).

pub mod account;
pub mod api;
pub mod backend;
pub mod crypto;
pub mod error;
pub mod i18n;
pub mod model;
pub mod search;
pub mod storage;

pub use error::{Error, Result};

use std::path::PathBuf;
use std::sync::Arc;

/// Точка входа в ядро: держит хранилище, поиск, крипто и менеджер аккаунтов.
pub struct Core {
    pub db: storage::Db,
    pub search: Arc<dyn search::SearchIndex>,
    pub crypto: Arc<crypto::StorageCrypto>,
    pub accounts: account::AccountManager,
}

impl Core {
    /// Инициализация ядра: открыть/создать зашифрованное хранилище в `data_dir`,
    /// прогнать миграции, поднять поисковый индекс.
    pub async fn bootstrap(data_dir: PathBuf) -> Result<Self> {
        // Every TLS consumer in the dependency graph uses aws-lc-rs. Installing
        // it here also protects non-Tauri users of the core crate from the
        // rustls 0.23 ambiguous-provider panic.
        let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();
        std::fs::create_dir_all(&data_dir)?;

        let crypto = Arc::new(crypto::StorageCrypto::open(&data_dir)?);
        let db = storage::Db::open(&data_dir, crypto.clone()).await?;
        db.migrate().await?;
        let (removed_blobs, missing_blobs) = db.garbage_collect_blobs().await?;
        if removed_blobs > 0 {
            tracing::info!(removed_blobs, "удалены потерянные blob-файлы");
        }
        if !missing_blobs.is_empty() {
            tracing::warn!(
                count = missing_blobs.len(),
                "в БД обнаружены ссылки на отсутствующие blob-файлы"
            );
        }

        let search: Arc<dyn search::SearchIndex> = Arc::new(search::Fts5Index::new(db.clone()));

        let accounts = account::AccountManager::new(db.clone());

        Ok(Self {
            db,
            search,
            crypto,
            accounts,
        })
    }
}
