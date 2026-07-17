//! Локальное хранилище (SQLCipher через sqlx). Весь файл БД, включая WAL,
//! метаданные и FTS, зашифрован; чувствительные значения дополнительно
//! шифруются на уровне приложения.

mod blobs;
pub mod repo;

pub use blobs::BlobStore;

use crate::Result;
use crate::crypto::{DatabaseKey, StorageCrypto};
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection, SqlitePoolOptions};
use sqlx::{AssertSqlSafe, ConnectOptions, Connection, SqlitePool};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone)]
pub struct Db {
    /// Пул для чтения. Читатели в WAL друг другу не мешают и идут параллельно.
    pub pool: SqlitePool,
    /// Пул для записи, ровно одно соединение. SQLite физически допускает только
    /// одного писателя: пул сам выстраивает конкурирующие записи в очередь и
    /// отдаёт соединение по мере освобождения. Без него параллельные mail-sync
    /// и aux-sync дрались за блокировку и падали с "database is locked".
    pub write_pool: SqlitePool,
    pub blobs: BlobStore,
    crypto: Arc<StorageCrypto>,
}

const ENCRYPTED_SETTING_PREFIX: &[u8] = b"TMSET1\0";

impl Db {
    /// Открыть/создать базу в data_dir/truemail.db и blob-store в data_dir/blobs.
    pub async fn open(data_dir: &Path, crypto: Arc<StorageCrypto>) -> Result<Self> {
        let database_key = DatabaseKey::open()?;
        Self::open_with_database_key(data_dir, crypto, &database_key).await
    }

    async fn open_with_database_key(
        data_dir: &Path,
        crypto: Arc<StorageCrypto>,
        database_key: &DatabaseKey,
    ) -> Result<Self> {
        let db_path = data_dir.join("truemail.db");
        prepare_encrypted_database(&db_path, database_key).await?;

        // Читателей столько, сколько ядер реально может читать параллельно.
        // Больше смысла нет: чтение упирается в CPU и диск, а каждое лишнее
        // соединение SQLCipher стоит дорогого key derivation при открытии.
        // Меньше двух - параллельные чтения UI выстраиваются в очередь.
        let readers = std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(4)
            .clamp(2, 8) as u32;
        let pool = SqlitePoolOptions::new()
            .max_connections(readers)
            .connect_with(encrypted_options(&db_path, database_key, true))
            .await?;
        // Единственное соединение = очередь записи. Ждать в ней можно сколько
        // угодно: это ожидание своей очереди, а не блокировки, и оно не падает.
        let write_pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(encrypted_options(&db_path, database_key, true))
            .await?;

        verify_sqlcipher(&pool).await?;

        let blobs = BlobStore::new(data_dir.join("blobs"), crypto.clone())?;
        Ok(Self {
            pool,
            write_pool,
            blobs,
            crypto,
        })
    }

    /// Закрыть оба пула и отпустить файл БД. Закрывать только `pool` мало:
    /// writer-соединение продолжит удерживать файл.
    pub async fn close(&self) {
        self.pool.close().await;
        self.write_pool.close().await;
    }

    /// Транзакция для записи. Всегда начинать записи через неё.
    ///
    /// Идёт через `write_pool`, поэтому писатель всегда один, а остальные ждут
    /// своей очереди в пуле. `BEGIN IMMEDIATE` вместо обычного deferred `BEGIN`:
    /// deferred стартует читателем и берёт снимок БД, а если к первой записи
    /// снимок устарел, SQLite отдаёт SQLITE_BUSY сразу, не дожидаясь ничего.
    pub async fn begin_write(&self) -> Result<sqlx::Transaction<'static, sqlx::Sqlite>> {
        Ok(self.write_pool.begin_with("BEGIN IMMEDIATE").await?)
    }

    /// Прогнать все миграции из crates/core/migrations.
    pub async fn migrate(&self) -> Result<()> {
        sqlx::migrate!("./migrations")
            .run(&self.write_pool)
            .await
            .map_err(|e| crate::Error::Other(format!("миграции: {e}")))?;
        self.encrypt_legacy_settings().await?;
        self.finalize_settings_encryption().await?;
        Ok(())
    }

    /// Прочитать и расшифровать настройку.
    pub async fn setting(&self, key: &str) -> Result<Option<String>> {
        let row: Option<(Vec<u8>,)> = sqlx::query_as("SELECT value FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        row.map(|(value,)| self.decrypt_setting(&value)).transpose()
    }

    /// Прочитать и расшифровать все настройки разом.
    ///
    /// UI восстанавливает состояние именно так: перечислять ключи на его стороне
    /// нельзя - забытый в списке ключ означает молча не восстановленную настройку.
    pub async fn all_settings(&self) -> Result<std::collections::HashMap<String, String>> {
        let rows: Vec<(String, Vec<u8>)> = sqlx::query_as("SELECT key, value FROM settings")
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|(key, value)| Ok((key, self.decrypt_setting(&value)?)))
            .collect()
    }

    /// Зашифровать и записать настройку. Открытое значение в SQLite не попадает.
    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let encrypted = self.encrypt_setting(value)?;
        sqlx::query(
            "INSERT INTO settings(key, value) VALUES(?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(encrypted)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    fn encrypt_setting(&self, value: &str) -> Result<Vec<u8>> {
        let encrypted = self.crypto.encrypt(value.as_bytes())?;
        let mut stored = Vec::with_capacity(ENCRYPTED_SETTING_PREFIX.len() + encrypted.len());
        stored.extend_from_slice(ENCRYPTED_SETTING_PREFIX);
        stored.extend_from_slice(&encrypted);
        Ok(stored)
    }

    fn decrypt_setting(&self, stored: &[u8]) -> Result<String> {
        let encrypted = stored
            .strip_prefix(ENCRYPTED_SETTING_PREFIX)
            .ok_or_else(|| crate::Error::Crypto("настройка хранится без шифрования".into()))?;
        let plaintext = self.crypto.decrypt(encrypted)?;
        String::from_utf8(plaintext)
            .map_err(|e| crate::Error::Crypto(format!("настройка не является UTF-8: {e}")))
    }

    /// Однократно зашифровать значения из старых версий и значения по умолчанию
    /// миграции. Выполняется до того, как настройки становятся доступны UI.
    async fn encrypt_legacy_settings(&self) -> Result<()> {
        let rows: Vec<(String, Vec<u8>)> = sqlx::query_as("SELECT key, value FROM settings")
            .fetch_all(&self.pool)
            .await?;
        let mut transaction = self.begin_write().await?;
        for (key, value) in rows {
            if value.starts_with(ENCRYPTED_SETTING_PREFIX) {
                continue;
            }
            let plaintext = String::from_utf8(value).map_err(|e| {
                crate::Error::Crypto(format!("открытая настройка {key} не является UTF-8: {e}"))
            })?;
            let encrypted = self.encrypt_setting(&plaintext)?;
            sqlx::query("UPDATE settings SET value = ? WHERE key = ?")
                .bind(encrypted)
                .bind(key)
                .execute(&mut *transaction)
                .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    /// Старые открытые значения могли остаться в свободных страницах SQLite или
    /// WAL. После первой зашифрованной миграции очищаем их и ставим маркер, чтобы
    /// не выполнять VACUUM при каждом запуске.
    async fn finalize_settings_encryption(&self) -> Result<()> {
        let completed: Option<(String,)> = sqlx::query_as(
            "SELECT value FROM storage_meta WHERE key = 'settings_encryption_v1_vacuumed'",
        )
        .fetch_optional(&self.pool)
        .await?;
        if completed.is_some() {
            return Ok(());
        }

        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&self.write_pool)
            .await?;
        sqlx::query("VACUUM").execute(&self.write_pool).await?;
        sqlx::query(
            "INSERT INTO storage_meta(key, value) VALUES('settings_encryption_v1_vacuumed', '1')",
        )
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }
}

fn encrypted_options(
    path: &Path,
    database_key: &DatabaseKey,
    create: bool,
) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(create)
        // Sqlx гарантирует, что специальный SQLCipher PRAGMA key выполняется
        // первым для каждого соединения пула.
        .pragma("key", database_key.pragma_value())
        .foreign_keys(true)
        .pragma("secure_delete", "ON")
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        // Никаких ожиданий блокировки: писатель ровно один (см. Db::write_pool),
        // читатели в WAL ему не мешают, конкурировать некому. Если SQLite всё же
        // ответит "database is locked" - это баг очереди, и он должен падать
        // громко, а не прятаться за таймаутом. Ноль обязателен явно: у sqlx
        // значение по умолчанию - 5 секунд.
        .busy_timeout(std::time::Duration::ZERO)
        .disable_statement_logging()
}

async fn verify_sqlcipher(pool: &SqlitePool) -> Result<()> {
    let version: Option<(String,)> = sqlx::query_as("PRAGMA cipher_version")
        .fetch_optional(pool)
        .await?;
    let version = version
        .map(|row| row.0)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| crate::Error::Crypto("сборка SQLite не содержит SQLCipher".into()))?;
    let _: (i64,) = sqlx::query_as("SELECT count(*) FROM sqlite_master")
        .fetch_one(pool)
        .await
        .map_err(|error| {
            crate::Error::Crypto(format!("не удалось открыть SQLCipher БД: {error}"))
        })?;
    tracing::info!(sqlcipher_version = %version, "SQLCipher storage opened");
    Ok(())
}

async fn prepare_encrypted_database(path: &Path, database_key: &DatabaseKey) -> Result<()> {
    recover_interrupted_migration(path)?;
    if path.is_file() && has_plaintext_sqlite_header(path)? {
        migrate_plaintext_database(path, database_key).await?;
    }
    Ok(())
}

fn has_plaintext_sqlite_header(path: &Path) -> Result<bool> {
    let mut file = std::fs::File::open(path)?;
    let mut header = [0_u8; 16];
    if file.read(&mut header)? < header.len() {
        return Ok(false);
    }
    Ok(&header == b"SQLite format 3\0")
}

async fn migrate_plaintext_database(path: &Path, database_key: &DatabaseKey) -> Result<()> {
    let encrypted = migration_path(path, ".sqlcipher.tmp");
    let backup = migration_path(path, ".plaintext.migrating");
    remove_database_files(&encrypted)?;
    // Main connection deliberately has SQLITE_OPEN_CREATE disabled. ATTACH
    // inherits those flags, so create the empty destination ourselves first.
    std::fs::File::create(&encrypted)?;

    let plain_options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .foreign_keys(false)
        .disable_statement_logging();
    let mut connection = SqliteConnection::connect_with(&plain_options).await?;
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&mut connection)
        .await?;
    let (user_version,): (i64,) = sqlx::query_as("PRAGMA user_version")
        .fetch_one(&mut connection)
        .await?;

    let encrypted_path = escape_sql_literal(&encrypted.to_string_lossy());
    let attach = format!(
        "ATTACH DATABASE '{encrypted_path}' AS encrypted KEY {}",
        database_key.pragma_value()
    );
    // Путь экранируется как SQL-литерал, а key состоит только из 64 hex-цифр.
    sqlx::query(AssertSqlSafe(attach))
        .execute(&mut connection)
        .await?;
    let export_result = sqlx::query("SELECT sqlcipher_export('encrypted')")
        .execute(&mut connection)
        .await;
    if let Err(error) = export_result {
        let _ = sqlx::query("DETACH DATABASE encrypted")
            .execute(&mut connection)
            .await;
        connection.close().await?;
        remove_database_files(&encrypted)?;
        return Err(error.into());
    }
    // user_version прочитан как i64 из самой SQLite, поэтом инъекция невозможна.
    sqlx::query(AssertSqlSafe(format!(
        "PRAGMA encrypted.user_version = {user_version}"
    )))
    .execute(&mut connection)
    .await?;
    sqlx::query("DETACH DATABASE encrypted")
        .execute(&mut connection)
        .await?;
    connection.close().await?;

    remove_sidecars(path)?;
    verify_encrypted_file(&encrypted, database_key).await?;

    std::fs::rename(path, &backup)?;
    if let Err(error) = std::fs::rename(&encrypted, path) {
        let _ = std::fs::rename(&backup, path);
        return Err(error.into());
    }
    secure_remove_file(&backup)?;
    tracing::info!(database = %path.display(), "plaintext SQLite migrated to SQLCipher");
    Ok(())
}

async fn verify_encrypted_file(path: &Path, database_key: &DatabaseKey) -> Result<()> {
    if has_plaintext_sqlite_header(path)? {
        return Err(crate::Error::Crypto(
            "результат миграции SQLCipher остался открытым SQLite".into(),
        ));
    }
    let options = encrypted_options(path, database_key, false);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    verify_sqlcipher(&pool).await?;
    pool.close().await;
    remove_sidecars(path)?;
    Ok(())
}

fn recover_interrupted_migration(path: &Path) -> Result<()> {
    let encrypted = migration_path(path, ".sqlcipher.tmp");
    let backup = migration_path(path, ".plaintext.migrating");
    if !path.exists() && backup.exists() {
        std::fs::rename(&backup, path)?;
    }
    if path.exists() {
        remove_database_files(&encrypted)?;
    }
    Ok(())
}

fn migration_path(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(suffix);
    PathBuf::from(value)
}

fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(suffix);
    PathBuf::from(value)
}

fn remove_sidecars(path: &Path) -> Result<()> {
    for suffix in ["-wal", "-shm"] {
        let sidecar = sidecar_path(path, suffix);
        if sidecar.exists() {
            std::fs::remove_file(sidecar)?;
        }
    }
    Ok(())
}

fn remove_database_files(path: &Path) -> Result<()> {
    remove_sidecars(path)?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

fn secure_remove_file(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let mut file = std::fs::OpenOptions::new().write(true).open(path)?;
    let len = file.metadata()?.len();
    file.seek(SeekFrom::Start(0))?;
    let zeros = [0_u8; 64 * 1024];
    let mut remaining = len;
    while remaining > 0 {
        let count = remaining.min(zeros.len() as u64) as usize;
        file.write_all(&zeros[..count])?;
        remaining -= count as u64;
    }
    file.sync_all()?;
    drop(file);
    std::fs::remove_file(path)?;
    Ok(())
}

fn escape_sql_literal(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    fn random_key() -> [u8; 32] {
        let mut key = [0_u8; 32];
        rand::rng().fill_bytes(&mut key);
        key
    }

    #[tokio::test]
    async fn settings_are_encrypted_in_sqlite_and_round_trip() {
        let root = std::env::temp_dir().join(format!("truemail-settings-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create temp data dir");
        let crypto = Arc::new(StorageCrypto::from_key(random_key()));
        let database_key = DatabaseKey::from_key(random_key());
        let db = Db::open_with_database_key(&root, crypto, &database_key)
            .await
            .expect("open database");
        db.migrate().await.expect("migrate database");

        db.set_setting("test_secret", "never-store-this-in-plaintext")
            .await
            .expect("store setting");
        assert_eq!(
            db.setting("test_secret").await.expect("read setting"),
            Some("never-store-this-in-plaintext".into())
        );

        let (stored,): (Vec<u8>,) =
            sqlx::query_as("SELECT value FROM settings WHERE key = 'test_secret'")
                .fetch_one(&db.pool)
                .await
                .expect("read raw setting");
        assert!(stored.starts_with(ENCRYPTED_SETTING_PREFIX));
        assert!(!String::from_utf8_lossy(&stored).contains("never-store-this-in-plaintext"));

        db.close().await;
        drop(db);
        let database_path = root.join("truemail.db");
        assert!(!has_plaintext_sqlite_header(&database_path).expect("read database header"));
        std::fs::remove_dir_all(root).expect("remove temp data dir");
    }

    #[tokio::test]
    async fn plaintext_database_is_migrated_without_losing_data() {
        let root =
            std::env::temp_dir().join(format!("truemail-migration-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create temp data dir");
        let database_path = root.join("truemail.db");
        let options = SqliteConnectOptions::new()
            .filename(&database_path)
            .create_if_missing(true);
        let mut plaintext = SqliteConnection::connect_with(&options)
            .await
            .expect("open plaintext database");
        sqlx::query("CREATE TABLE legacy(value TEXT NOT NULL)")
            .execute(&mut plaintext)
            .await
            .expect("create legacy table");
        sqlx::query("INSERT INTO legacy(value) VALUES('preserved')")
            .execute(&mut plaintext)
            .await
            .expect("insert legacy value");
        sqlx::query("PRAGMA user_version = 7")
            .execute(&mut plaintext)
            .await
            .expect("set user version");
        plaintext.close().await.expect("close plaintext database");
        assert!(has_plaintext_sqlite_header(&database_path).expect("read plaintext header"));

        let crypto = Arc::new(StorageCrypto::from_key(random_key()));
        let database_key = DatabaseKey::from_key(random_key());
        let db = Db::open_with_database_key(&root, crypto, &database_key)
            .await
            .expect("migrate and open database");
        let (value,): (String,) = sqlx::query_as("SELECT value FROM legacy")
            .fetch_one(&db.pool)
            .await
            .expect("read migrated value");
        let (user_version,): (i64,) = sqlx::query_as("PRAGMA user_version")
            .fetch_one(&db.pool)
            .await
            .expect("read migrated user version");
        assert_eq!(value, "preserved");
        assert_eq!(user_version, 7);
        db.close().await;
        assert!(!has_plaintext_sqlite_header(&database_path).expect("read encrypted header"));
        std::fs::remove_dir_all(root).expect("remove temp data dir");
    }

    #[tokio::test]
    async fn database_cannot_be_opened_with_another_key() {
        let root =
            std::env::temp_dir().join(format!("truemail-wrong-key-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create temp data dir");
        let crypto = Arc::new(StorageCrypto::from_key(random_key()));
        let first_key = DatabaseKey::from_key(random_key());
        let db = Db::open_with_database_key(&root, crypto.clone(), &first_key)
            .await
            .expect("create encrypted database");
        sqlx::query("CREATE TABLE protected(value TEXT)")
            .execute(&db.write_pool)
            .await
            .expect("write encrypted database");
        db.close().await;

        let wrong_key = DatabaseKey::from_key(random_key());
        assert!(
            Db::open_with_database_key(&root, crypto, &wrong_key)
                .await
                .is_err()
        );
        std::fs::remove_dir_all(root).expect("remove temp data dir");
    }

    #[tokio::test]
    async fn migrations_and_repository_preserve_integrity() {
        use crate::model::{AuthKind, BackendKind, NewAccount, Provider, Security, ServerConfig};

        let root = std::env::temp_dir().join(format!("truemail-repo-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create temp data dir");
        let db = Db::open_with_database_key(
            &root,
            Arc::new(StorageCrypto::from_key(random_key())),
            &DatabaseKey::from_key(random_key()),
        )
        .await
        .expect("open database");
        db.migrate().await.expect("migrate database");
        let account = db
            .save_account(&NewAccount {
                email: "repo@example.test".into(),
                display_name: "Repository test".into(),
                provider: Provider::Generic,
                backend_kind: BackendKind::Imap,
                auth_kind: AuthKind::Oauth2,
                imap: Some(ServerConfig {
                    host: "imap.example.test".into(),
                    port: 993,
                    security: Security::Ssl,
                }),
                smtp: None,
                ews_url: None,
                username: Some("repo@example.test".into()),
                secret_ref: "test-keychain-ref".into(),
                color: None,
            })
            .await
            .expect("save generic account");
        assert_eq!(account.provider, Provider::Generic);

        let indexes: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM sqlite_master WHERE type='index' AND name IN ('idx_messages_folder_date','uq_events_calendar_uid','idx_attachments_message') ORDER BY name",
        )
        .fetch_all(&db.pool)
        .await
        .expect("list integrity indexes");
        assert_eq!(indexes.len(), 3);

        sqlx::query("INSERT INTO folders(account_id, remote_path, display_name, role) VALUES(?, 'INBOX', 'Inbox', 'inbox')")
            .bind(account.id).execute(&db.write_pool).await.expect("insert folder");
        let (folder_id,): (i64,) = sqlx::query_as("SELECT id FROM folders WHERE account_id=?")
            .bind(account.id)
            .fetch_one(&db.pool)
            .await
            .expect("folder id");
        sqlx::query("INSERT INTO messages(account_id, folder_id, uid, subject, preview) VALUES(?, ?, 1, 'secret subject', 'secret preview')")
            .bind(account.id).bind(folder_id).execute(&db.write_pool).await.expect("insert message");
        let (before,): (i64,) = sqlx::query_as("SELECT count(*) FROM messages_fts")
            .fetch_one(&db.pool)
            .await
            .expect("fts count");
        assert_eq!(before, 1);

        use crate::backend::{DiscoveredFolder, DiscoveredMessage};
        use crate::model::FolderRole;
        let token_folder = DiscoveredFolder {
            remote_path: "INBOX".into(),
            display_name: "Inbox".into(),
            role: Some(FolderRole::Inbox),
            unread_count: 0,
            total_count: 2,
            uidvalidity: None,
            uidnext: None,
            highestmodseq: None,
            sync_token: Some("history-123".into()),
        };
        db.save_discovered_folders(account.id, std::slice::from_ref(&token_folder))
            .await
            .expect("save folder metadata");
        let (token_before_commit,): (Option<String>,) =
            sqlx::query_as("SELECT sync_token FROM folders WHERE id=?")
                .bind(folder_id)
                .fetch_one(&db.pool)
                .await
                .expect("read pending sync token");
        assert_eq!(token_before_commit, None);
        db.save_folder_sync_tokens(account.id, std::slice::from_ref(&token_folder))
            .await
            .expect("commit sync token");
        let (token_after_commit,): (Option<String>,) =
            sqlx::query_as("SELECT sync_token FROM folders WHERE id=?")
                .bind(folder_id)
                .fetch_one(&db.pool)
                .await
                .expect("read committed sync token");
        assert_eq!(token_after_commit.as_deref(), Some("history-123"));

        sqlx::query("INSERT INTO folders(account_id, remote_path, display_name) VALUES(?, 'ALL', 'All Mail')")
            .bind(account.id)
            .execute(&db.write_pool)
            .await
            .expect("insert all-mail folder");
        let (all_folder_id,): (i64,) =
            sqlx::query_as("SELECT id FROM folders WHERE account_id=? AND remote_path='ALL'")
                .bind(account.id)
                .fetch_one(&db.pool)
                .await
                .expect("all-mail folder id");
        for (target_folder, uid, remote_id) in [
            (folder_id, 2_i64, "remote-1"),
            (all_folder_id, 2_i64, "remote-1"),
            (all_folder_id, 3_i64, "remote-deleted"),
        ] {
            sqlx::query(
                "INSERT INTO messages(account_id, folder_id, uid, remote_id) VALUES(?, ?, ?, ?)",
            )
            .bind(account.id)
            .bind(target_folder)
            .bind(uid)
            .bind(remote_id)
            .execute(&db.write_pool)
            .await
            .expect("insert Gmail projection");
        }
        let desired = DiscoveredMessage {
            folder_path: "INBOX".into(),
            uid: 2,
            remote_id: Some("remote-1".into()),
            size: None,
            seen: false,
            flagged: false,
            answered: false,
            draft: false,
            raw: Vec::new(),
        };
        let removed = db
            .reconcile_remote_projections(
                account.id,
                &[desired],
                &["remote-1".into(), "remote-deleted".into()],
                None,
            )
            .await
            .expect("reconcile Gmail projections");
        assert_eq!(removed, 2);
        let remaining: Vec<(String, String)> = sqlx::query_as(
            "SELECT m.remote_id, f.remote_path FROM messages m
             JOIN folders f ON f.id=m.folder_id WHERE m.remote_id IS NOT NULL",
        )
        .fetch_all(&db.pool)
        .await
        .expect("read remaining projections");
        assert_eq!(remaining, vec![("remote-1".into(), "INBOX".into())]);

        sqlx::query("DELETE FROM accounts WHERE id=?")
            .bind(account.id)
            .execute(&db.write_pool)
            .await
            .expect("delete account");
        let (after,): (i64,) = sqlx::query_as("SELECT count(*) FROM messages_fts")
            .fetch_one(&db.pool)
            .await
            .expect("fts count after cascade");
        assert_eq!(after, 0);

        db.close().await;
        std::fs::remove_dir_all(root).expect("remove temp data dir");
    }
}
