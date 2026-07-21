//! Менеджер аккаунтов и автоконфигурация провайдеров.

mod autoconfig;
mod auxiliary;
mod dav;
mod google_services;
mod oauth;
pub use autoconfig::{ProviderConfig, autoconfig, discover_provider};
pub use auxiliary::{
    ContactInput, EventInput, RemoteObject, delete_contact, delete_event, write_contact,
    write_event,
};
pub use dav::{
    AuxiliarySyncCursors, CollectionCursor, DavAuth, DavAuthScheme, DavCalendar, DavCollection,
    DavContact, DavEvent, DavSyncResult, SyncScope, WELL_KNOWN_CALDAV, WELL_KNOWN_CARDDAV,
    YANDEX_CALDAV_BASE, YANDEX_CARDDAV_BASE, dav_auth_scheme, discover_well_known,
    resolve_yandex_bases, sync_dav_account, validate_dav,
};
pub use google_services::sync_google_services;
pub use oauth::{
    GOOGLE_SCOPES, MICROSOFT_SCOPES, OAuthToken, PkcePair, StoredOAuthCredential, YANDEX_SCOPES,
    configured_google_client_id, configured_google_client_secret, configured_microsoft_client_id,
    configured_microsoft_tenant, configured_yandex_client_id, configured_yandex_redirect_uri,
    exchange_google_code, exchange_microsoft_code, exchange_yandex_code, generate_pkce,
    generate_state, google_authorize_url, microsoft_authorize_url, refresh_google_token,
    refresh_microsoft_token, refresh_yandex_token, yandex_authorize_url,
};

use crate::Result;
use crate::backend::{
    EwsBackend, GenericImapBackend, GmailBackend, JmapBackend, MailBackend, OutlookBackend,
    SendOutcome, YandexBackend,
};
use crate::model::{
    Account, AuthKind, BackendKind, FolderRole, NewAccount, Provider, Security, ServerConfig,
};
use crate::storage::Db;
use crate::storage::repo::AuxiliarySaveResult;
use base64::Engine as _;
use zeroize::Zeroizing;

fn sent_append_payload(raw: &[u8]) -> Result<String> {
    Ok(serde_json::to_string(&serde_json::json!({
        "raw": base64::engine::general_purpose::STANDARD.encode(raw)
    }))?)
}

fn sent_append_raw(payload: &str) -> Result<Vec<u8>> {
    let payload: serde_json::Value = serde_json::from_str(payload)?;
    let raw = payload["raw"].as_str().ok_or_else(|| {
        crate::Error::AccountConfig("append_sent outbox: нет MIME payload".into())
    })?;
    base64::engine::general_purpose::STANDARD
        .decode(raw)
        .map_err(|error| crate::Error::AccountConfig(format!("append_sent outbox: {error}")))
}

/// Результат короткой синхронизации Входящих. `new_messages` считает только
/// remote ID, которых не было в локальной БД до этого прохода; повторно
/// полученные EWS Modified-события поэтому не создают уведомления.
/// `new_message_ids` - локальные id этих же писем (только из папки Входящие),
/// отсортированные по дате по возрастанию: используются для карточки
/// уведомления, чтобы показывать именно новое письмо, а не самое свежее в БД.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxSyncResult {
    pub downloaded: usize,
    pub new_messages: usize,
    pub had_baseline: bool,
    pub new_message_ids: Vec<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SyncKind {
    Mail,
    Auxiliary,
}

#[cfg(test)]
mod sync_registry_tests {
    use super::*;

    fn gmail_account(id: i64) -> Account {
        Account {
            id,
            uuid: uuid::Uuid::new_v4().to_string(),
            email: "test@gmail.com".into(),
            display_name: "Gmail".into(),
            provider: Provider::Gmail,
            backend_kind: BackendKind::Imap,
            auth_kind: AuthKind::Oauth2,
            imap: None,
            smtp: None,
            ews_url: None,
            caldav_url: None,
            carddav_url: None,
            jmap_url: None,
            username: None,
            secret_ref: Some("unused-in-test".into()),
            include_in_unified: true,
            color: None,
            retention_days: 30,
            enabled: true,
        }
    }

    #[tokio::test]
    async fn serializes_same_account_and_scope_inside_core() {
        let registry = std::sync::Arc::new(SyncRegistry::default());
        let started = std::sync::Arc::new(tokio::sync::Notify::new());
        let release = std::sync::Arc::new(tokio::sync::Notify::new());
        let first_registry = registry.clone();
        let first_started = started.clone();
        let first_release = release.clone();
        let first = tokio::spawn(async move {
            first_registry
                .exclusive(7, SyncKind::Mail, async move {
                    first_started.notify_one();
                    first_release.notified().await;
                    Ok::<_, crate::Error>(())
                })
                .await
        });
        started.notified().await;

        assert!(
            registry
                .exclusive(7, SyncKind::Mail, async { Ok::<_, crate::Error>(()) })
                .await
                .is_err()
        );
        assert!(
            registry
                .exclusive(7, SyncKind::Auxiliary, async { Ok::<_, crate::Error>(()) })
                .await
                .is_err()
        );

        release.notify_one();
        first.await.expect("join first sync").expect("first sync");
        assert!(
            registry
                .exclusive(7, SyncKind::Mail, async { Ok::<_, crate::Error>(()) })
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn gmail_retry_after_survives_a_new_account_manager() {
        let root = std::env::temp_dir().join(format!(
            "truemail-gmail-retry-after-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("create temp data dir");
        let crypto = std::sync::Arc::new(crate::crypto::StorageCrypto::from_key(rand::random()));
        let database_key = crate::crypto::DatabaseKey::from_key(rand::random());
        let db = Db::open_with_database_key(&root, crypto, &database_key)
            .await
            .expect("open database");
        db.migrate().await.expect("migrate database");
        let account = gmail_account(42);
        let retry_at = chrono::Utc::now() + chrono::Duration::minutes(15);

        let first = AccountManager::new(db.clone());
        first
            .remember_gmail_rate_limit(
                &account,
                &crate::Error::RateLimited {
                    backend: "gmail-api".into(),
                    retry_at,
                    message: "test quota".into(),
                },
            )
            .await;
        drop(first);

        let restarted = AccountManager::new(db.clone());
        let error = restarted
            .ensure_gmail_mail_allowed(&account)
            .await
            .expect_err("persisted deadline must block HTTP after restart");
        match error {
            crate::Error::RateLimited {
                backend,
                retry_at: stored,
                message,
            } => {
                assert_eq!(backend, "gmail-api");
                assert_eq!(stored.timestamp_millis(), retry_at.timestamp_millis());
                assert!(message.contains("HTTP-запрос не отправлен"));
            }
            other => panic!("unexpected error: {other}"),
        }
        assert_eq!(
            restarted
                .process_mail_outbox(&account)
                .await
                .expect("empty outbox must not touch Gmail transport"),
            0
        );

        drop(restarted);
        db.close().await;
        drop(db);
        std::fs::remove_dir_all(root).expect("remove temp data dir");
    }

    /// Без secret_ref обе ветки auxiliary_credential падают на первом же шаге,
    /// ещё до обращения к системному keychain - этого достаточно, чтобы по
    /// тексту ошибки отличить маршрут OAuth2 (oauth_access_token) от
    /// Password/Ntlm, не завязываясь на keychain, которого на CI-раннере
    /// может не быть. Регрессия: раньше create_event/create_contact и
    /// подобные команды в commands.rs всегда звали oauth_access_token
    /// напрямую, и Exchange-аккаунт с Password получал невнятную ошибку про
    /// OAuth-токен вместо честной "нет ссылки на пароль".
    #[tokio::test]
    async fn auxiliary_credential_routes_by_auth_kind() {
        let root = std::env::temp_dir().join(format!(
            "truemail-auxiliary-credential-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("create temp data dir");
        let crypto = std::sync::Arc::new(crate::crypto::StorageCrypto::from_key(rand::random()));
        let database_key = crate::crypto::DatabaseKey::from_key(rand::random());
        let db = Db::open_with_database_key(&root, crypto, &database_key)
            .await
            .expect("open database");
        db.migrate().await.expect("migrate database");
        let manager = AccountManager::new(db.clone());

        let mut oauth_account = gmail_account(1);
        oauth_account.secret_ref = None;
        let error = manager
            .auxiliary_credential(&oauth_account)
            .await
            .expect_err("oauth account without secret_ref must fail before keychain access");
        assert!(
            matches!(&error, crate::Error::AccountConfig(message) if message.contains("OAuth-токен")),
            "unexpected error: {error}"
        );

        for auth_kind in [AuthKind::Password, AuthKind::Ntlm, AuthKind::AppPassword] {
            let mut password_account = gmail_account(2);
            password_account.provider = Provider::Exchange;
            password_account.auth_kind = auth_kind;
            password_account.secret_ref = None;
            let error = manager
                .auxiliary_credential(&password_account)
                .await
                .expect_err(
                    "password-family account without secret_ref must fail before keychain access",
                );
            assert!(
                matches!(&error, crate::Error::AccountConfig(message) if message.contains("пароль")),
                "unexpected error for {auth_kind:?}: {error}"
            );
        }

        db.close().await;
        drop(db);
        std::fs::remove_dir_all(root).expect("remove temp data dir");
    }
}

#[derive(Default)]
struct SyncRegistry {
    locks:
        tokio::sync::Mutex<std::collections::HashMap<i64, std::sync::Arc<tokio::sync::Semaphore>>>,
}

impl SyncRegistry {
    async fn exclusive<T>(
        &self,
        account_id: i64,
        _kind: SyncKind,
        operation: impl std::future::Future<Output = Result<T>>,
    ) -> Result<T> {
        let semaphore = self
            .locks
            .lock()
            .await
            .entry(account_id)
            .or_insert_with(|| std::sync::Arc::new(tokio::sync::Semaphore::new(1)))
            .clone();
        let _permit = semaphore.try_acquire_owned().map_err(|_| {
            crate::Error::Other(format!(
                "синхронизация аккаунта {account_id} уже выполняется"
            ))
        })?;
        operation.await
    }
}

pub struct AccountManager {
    db: Db,
    // Сериализует обновление OAuth-токена: параллельные mail/aux-sync иначе
    // одновременно видят "истёк" и рефрешат по нескольку раз за минуту.
    refresh_lock: tokio::sync::Mutex<()>,
    sync_registry: SyncRegistry,
    exchange_outbox_repaired: tokio::sync::Mutex<std::collections::HashSet<i64>>,
}

#[derive(Debug)]
pub struct ConnectedAccountSync {
    pub account: Account,
    pub mail_folders: usize,
    pub calendars: usize,
    pub events: usize,
    pub contacts: usize,
    pub warnings: Vec<String>,
}

impl AccountManager {
    pub fn new(db: Db) -> Self {
        Self {
            db,
            refresh_lock: tokio::sync::Mutex::new(()),
            sync_registry: SyncRegistry::default(),
            exchange_outbox_repaired: tokio::sync::Mutex::new(std::collections::HashSet::new()),
        }
    }

    fn gmail_rate_limit_key(account_id: i64) -> String {
        format!("gmail_api_retry_at:{account_id}")
    }

    /// secret_ref уже подключённого аккаунта с этим email, если он есть -
    /// снимок состояния ДО апсерта. save_account делает UPSERT по email, так
    /// что после сохранения старое значение уже не прочитать: смотреть
    /// нужно заранее, чтобы потом понять, какую запись keychain подчищать.
    async fn existing_secret_ref(&self, email: &str) -> Option<String> {
        match self.db.list_accounts().await {
            Ok(accounts) => accounts
                .into_iter()
                .find(|account| account.email.eq_ignore_ascii_case(email))
                .and_then(|account| account.secret_ref),
            Err(error) => {
                tracing::warn!(%error, "не удалось прочитать список аккаунтов перед подключением");
                None
            }
        }
    }

    /// Если email раньше был подключён другим способом (был пароль IMAP,
    /// стал OAuth и т.п.), secret_ref в БД сменился на новый, а старая
    /// запись в системном keychain так и осталась висеть с прежним секретом.
    /// Подчищаем её здесь. Вызывать строго ПОСЛЕ того, как новый секрет и
    /// аккаунт уже успешно сохранены: сбой этой очистки не должен ронять
    /// само подключение, а порядок исключает риск остаться совсем без
    /// секрета при сбое на середине.
    fn cleanup_stale_secret(previous: Option<String>, new_secret_ref: &str) {
        let Some(previous) = previous else {
            return;
        };
        if previous == new_secret_ref {
            return;
        }
        match keyring::Entry::new("truemail", &previous) {
            Ok(entry) => {
                if let Err(error) = entry.delete_credential() {
                    tracing::warn!(
                        secret_ref = %previous,
                        %error,
                        "не удалось удалить осиротевшую запись keychain (возможно, уже отсутствует)"
                    );
                }
            }
            Err(error) => {
                tracing::warn!(
                    secret_ref = %previous,
                    %error,
                    "не удалось открыть осиротевшую запись keychain для удаления"
                );
            }
        }
    }

    /// Не выпускать Gmail HTTP-запрос раньше серверного Retry-After даже после
    /// перезапуска приложения. Значение хранится в зашифрованной settings.
    async fn ensure_gmail_mail_allowed(&self, account: &Account) -> Result<()> {
        if account.provider != Provider::Gmail {
            return Ok(());
        }
        let key = Self::gmail_rate_limit_key(account.id);
        let Some(value) = self.db.setting(&key).await? else {
            return Ok(());
        };
        let Ok(retry_at) = chrono::DateTime::parse_from_rfc3339(&value) else {
            tracing::warn!(account = %crate::logging::mask_email(&account.email), value, "повреждён сохранённый Gmail Retry-After; значение проигнорировано");
            return Ok(());
        };
        let retry_at = retry_at.with_timezone(&chrono::Utc);
        let now = chrono::Utc::now();
        if retry_at <= now {
            return Ok(());
        }
        let seconds = (retry_at - now).num_seconds().max(1);
        Err(crate::Error::RateLimited {
            backend: "gmail-api".into(),
            retry_at,
            message: format!(
                "сохранённый Retry-After ещё действует ({seconds} с); HTTP-запрос не отправлен"
            ),
        })
    }

    async fn remember_gmail_rate_limit(&self, account: &Account, error: &crate::Error) {
        if account.provider != Provider::Gmail {
            return;
        }
        let crate::Error::RateLimited {
            backend, retry_at, ..
        } = error
        else {
            return;
        };
        if backend != "gmail-api" {
            return;
        }
        let key = Self::gmail_rate_limit_key(account.id);
        let existing = match self.db.setting(&key).await {
            Ok(value) => value,
            Err(read_error) => {
                tracing::warn!(account = %crate::logging::mask_email(&account.email), %read_error, "Gmail Retry-After не прочитан перед обновлением");
                None
            }
        };
        let existing_retry_at = existing
            .as_deref()
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&chrono::Utc));
        if existing_retry_at.is_some_and(|stored| stored >= *retry_at) {
            return;
        }
        let value = retry_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        match self.db.set_setting(&key, &value).await {
            Ok(()) => tracing::info!(
                account = %crate::logging::mask_email(&account.email),
                retry_at = %value,
                "Gmail Retry-After сохранён и переживёт перезапуск"
            ),
            Err(write_error) => tracing::warn!(
                account = %crate::logging::mask_email(&account.email),
                %write_error,
                "Gmail Retry-After не удалось сохранить"
            ),
        }
    }

    /// `parent_folder_id` - локальный id уже существующей папки, внутри
    /// которой создаётся новая (None - верхний уровень аккаунта). В отличие
    /// от rename/delete здесь нет защиты системных папок: создать подпапку
    /// внутри Входящих - обычная операция, ничего не переименовывает и не
    /// удаляет.
    pub async fn create_folder(
        &self,
        account_id: i64,
        parent_folder_id: Option<i64>,
        name: &str,
    ) -> Result<()> {
        let account = self
            .db
            .list_accounts()
            .await?
            .into_iter()
            .find(|account| account.id == account_id)
            .ok_or_else(|| crate::Error::AccountConfig("аккаунт не найден".into()))?;
        let parent_remote_path = match parent_folder_id {
            Some(id) => {
                let parent = self.db.folder(id).await?;
                if parent.account_id != account_id {
                    return Err(crate::Error::AccountConfig(
                        "родительская папка принадлежит другому аккаунту".into(),
                    ));
                }
                Some(parent.remote_path)
            }
            None => None,
        };
        let token = self.mail_credential(&account).await?;
        let backend = Self::mail_backend(&account)?;
        backend
            .create_folder(&account.email, &token, parent_remote_path.as_deref(), name)
            .await?;
        // Локально папка заводится обычным циклом синхронизации списка папок
        // (как в sync_mail_account_inner) - так UI сразу видит новую папку,
        // а не только remote_path, о котором знает только что созданный backend.
        if let Ok(folders) = backend.discover_folders(&account.email, &token).await {
            self.db
                .save_discovered_folders(account.id, &folders)
                .await?;
        }
        Ok(())
    }

    pub async fn rename_folder(&self, folder_id: i64, new_name: &str) -> Result<()> {
        let folder = self.db.folder(folder_id).await?;
        if folder.role.is_some() {
            return Err(crate::Error::AccountConfig(
                "системную папку нельзя переименовать".into(),
            ));
        }
        let account = self
            .db
            .list_accounts()
            .await?
            .into_iter()
            .find(|account| account.id == folder.account_id)
            .ok_or_else(|| crate::Error::AccountConfig("аккаунт папки не найден".into()))?;
        let token = self.mail_credential(&account).await?;
        let backend = Self::mail_backend(&account)?;
        let remote = backend
            .rename_folder(&account.email, &token, &folder.remote_path, new_name)
            .await?;
        self.db
            .rename_folder_local(folder.id, &remote, new_name.trim())
            .await
    }

    pub async fn delete_folder(&self, folder_id: i64) -> Result<()> {
        let folder = self.db.folder(folder_id).await?;
        if folder.role.is_some() {
            return Err(crate::Error::AccountConfig(
                "системную папку нельзя удалить".into(),
            ));
        }
        let account = self
            .db
            .list_accounts()
            .await?
            .into_iter()
            .find(|account| account.id == folder.account_id)
            .ok_or_else(|| crate::Error::AccountConfig("аккаунт папки не найден".into()))?;
        let token = self.mail_credential(&account).await?;
        let backend = Self::mail_backend(&account)?;
        backend
            .delete_folder(&account.email, &token, &folder.remote_path)
            .await?;
        self.db.delete_folder_local(folder.id).await
    }

    pub async fn list(&self) -> Result<Vec<Account>> {
        self.db.list_accounts().await
    }

    fn mail_backend(account: &Account) -> Result<Box<dyn MailBackend>> {
        if account.backend_kind == BackendKind::Jmap {
            return Ok(Box::new(JmapBackend {
                session_url: account.jmap_url.clone().ok_or_else(|| {
                    crate::Error::AccountConfig("для аккаунта не настроен JMAP Session URL".into())
                })?,
                username: account
                    .username
                    .clone()
                    .unwrap_or_else(|| account.email.clone()),
            }));
        }
        match account.provider {
            Provider::Yandex => Ok(Box::new(YandexBackend)),
            Provider::Gmail => Ok(Box::new(GmailBackend)),
            Provider::Outlook => Ok(Box::new(OutlookBackend)),
            Provider::Mailru | Provider::Icloud | Provider::Generic => {
                let imap = account.imap.clone().ok_or_else(|| {
                    crate::Error::AccountConfig("для аккаунта не настроен IMAP-сервер".into())
                })?;
                Ok(Box::new(GenericImapBackend {
                    username: account
                        .username
                        .clone()
                        .unwrap_or_else(|| account.email.clone()),
                    imap,
                    smtp: account.smtp.clone(),
                }))
            }
            Provider::Exchange => Ok(Box::new(EwsBackend {
                endpoint: account.ews_url.clone().ok_or_else(|| {
                    crate::Error::AccountConfig("для Exchange не настроен адрес EWS".into())
                })?,
                username: account
                    .username
                    .clone()
                    .unwrap_or_else(|| account.email.clone()),
            })),
        }
    }

    async fn mail_credential(&self, account: &Account) -> Result<Zeroizing<String>> {
        self.ensure_gmail_mail_allowed(account).await?;
        self.auxiliary_credential(account).await
    }

    /// Секрет для вспомогательных операций (запись события/контакта из
    /// commands.rs) и для Exchange EWS вне почтового rate-limit'а Gmail:
    /// для OAuth2 - access token с обновлением по refresh (см.
    /// oauth_access_token), для Password/Ntlm/AppPassword - обычный пароль
    /// из системного keychain. Раньше эти команды всегда звали
    /// oauth_access_token напрямую, из-за чего Exchange-аккаунт с паролем
    /// падал на serde_json::from_str ещё до выбора провайдера - секрет в
    /// keychain у него лежит строкой, а не JSON StoredOAuthCredential.
    pub async fn auxiliary_credential(&self, account: &Account) -> Result<Zeroizing<String>> {
        if account.auth_kind == AuthKind::Oauth2 {
            return self.oauth_access_token(account).await;
        }
        let secret_ref = account
            .secret_ref
            .as_deref()
            .ok_or_else(|| crate::Error::AccountConfig("нет ссылки на пароль аккаунта".into()))?;
        keyring::Entry::new("truemail", secret_ref)
            .map_err(|error| crate::Error::Keyring(error.to_string()))?
            .get_password()
            .map(Zeroizing::new)
            .map_err(|error| crate::Error::Keyring(error.to_string()))
    }

    /// Прочитать сохранённый OAuth access token из системного keychain.
    pub async fn oauth_access_token(&self, account: &Account) -> Result<Zeroizing<String>> {
        let secret_ref = account
            .secret_ref
            .as_deref()
            .ok_or_else(|| crate::Error::AccountConfig("нет ссылки на OAuth-токен".into()))?;
        let entry = keyring::Entry::new("truemail", secret_ref)
            .map_err(|e| crate::Error::Keyring(e.to_string()))?;
        let serialized = Zeroizing::new(
            entry
                .get_password()
                .map_err(|e| crate::Error::Keyring(e.to_string()))?,
        );
        let mut credential: StoredOAuthCredential = serde_json::from_str(&serialized)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        if credential
            .expires_at
            .is_some_and(|expires| expires <= now + 60)
        {
            // Под мьютексом перечитываем токен: пока ждали блокировку, другой
            // поток мог уже обновить его - тогда повторный refresh не нужен.
            let _guard = self.refresh_lock.lock().await;
            if let Ok(serialized) = entry.get_password() {
                let serialized = Zeroizing::new(serialized);
                if let Ok(fresh) = serde_json::from_str::<StoredOAuthCredential>(&serialized) {
                    credential = fresh;
                }
            }
            // Токен ещё живой (или бессрочный) - обновлять нечего.
            if credential
                .expires_at
                .is_none_or(|expires| expires > now + 60)
            {
                return Ok(Zeroizing::new(credential.access_token.clone()));
            }
            let refresh_token =
                Zeroizing::new(credential.refresh_token.clone().ok_or_else(|| {
                    crate::Error::AccountConfig(
                        "OAuth-токен истёк и не содержит refresh_token".into(),
                    )
                })?);
            let refreshed = match account.provider {
                Provider::Yandex => {
                    let client_id = configured_yandex_client_id().ok_or_else(|| {
                        crate::Error::AccountConfig(
                            "для обновления OAuth-токена не задан TRUEMAIL_YANDEX_CLIENT_ID".into(),
                        )
                    })?;
                    refresh_yandex_token(&client_id, &refresh_token).await?
                }
                Provider::Gmail => {
                    let client_id = configured_google_client_id().ok_or_else(|| {
                        crate::Error::AccountConfig(
                            "для обновления OAuth-токена не задан TRUEMAIL_GOOGLE_CLIENT_ID".into(),
                        )
                    })?;
                    let client_secret =
                        Zeroizing::new(configured_google_client_secret().ok_or_else(|| {
                            crate::Error::AccountConfig(
                                "для обновления OAuth-токена не задан TRUEMAIL_GOOGLE_CLIENT_SECRET"
                                    .into(),
                            )
                        })?);
                    refresh_google_token(&client_id, &client_secret, &refresh_token).await?
                }
                Provider::Outlook => {
                    let client_id = configured_microsoft_client_id().ok_or_else(|| {
                        crate::Error::AccountConfig(
                            "для обновления OAuth-токена не задан TRUEMAIL_MICROSOFT_CLIENT_ID"
                                .into(),
                        )
                    })?;
                    refresh_microsoft_token(
                        &client_id,
                        &configured_microsoft_tenant(),
                        &refresh_token,
                    )
                    .await?
                }
                _ => {
                    return Err(crate::Error::AccountConfig(
                        "обновление OAuth-токена для провайдера не настроено".into(),
                    ));
                }
            };
            let mut updated = StoredOAuthCredential::from_refresh(refreshed, &refresh_token);
            // Google при refresh обычно не возвращает scope - сохраняем прежний,
            // иначе информация о выданных разрешениях теряется.
            if updated.scope.is_none() {
                updated.scope = credential.scope.clone();
            }
            let serialized = Zeroizing::new(serde_json::to_string(&updated)?);
            entry
                .set_password(&serialized)
                .map_err(|e| crate::Error::Keyring(e.to_string()))?;
            tracing::info!(email = %crate::logging::mask_email(&account.email), provider = ?account.provider, scope = ?updated.scope, "OAuth-токен обновлён через refresh");
            credential = updated;
        }
        Ok(Zeroizing::new(credential.access_token.clone()))
    }

    /// Лёгкая проверка последних ID Gmail Входящих без загрузки писем.
    /// Сравнение с предыдущим снимком выполняет цикл уведомлений: локальная БД
    /// может быть ещё не заполнена во время стартовой синхронизации.
    pub async fn gmail_latest_message_ids(&self, account: &Account) -> Result<Vec<String>> {
        if account.provider != Provider::Gmail {
            return Ok(Vec::new());
        }
        self.ensure_gmail_mail_allowed(account).await?;
        let token = self.oauth_access_token(account).await?;
        let result = crate::backend::gmail_latest_ids(&token, 25).await;
        if let Err(error) = &result {
            self.remember_gmail_rate_limit(account, error).await;
        }
        result
    }

    /// Дозагрузить только последние входящие и отдельно посчитать действительно
    /// новые письма. Exchange использует этот результат для watchdog-уведомлений.
    pub async fn sync_mail_inbox_delta(&self, account: &Account) -> Result<InboxSyncResult> {
        let access_token = self.mail_credential(account).await?;
        let backend = Self::mail_backend(account)?;
        let cursors = self.db.folder_sync_cursors(account.id).await?;
        let discovery_result = backend
            .discover_inbox(&account.email, &access_token, &cursors)
            .await;
        if let Err(error) = &discovery_result {
            self.remember_gmail_rate_limit(account, error).await;
        }
        let discovery = discovery_result?;
        let had_baseline = discovery
            .folders
            .iter()
            .find(|folder| folder.role == Some(FolderRole::Inbox))
            .and_then(|folder| cursors.get(&folder.remote_path))
            .and_then(|cursor| cursor.sync_token.as_deref())
            .is_some_and(|value| !value.is_empty());
        let downloaded = discovery.messages.len();
        let mut remote_ids = discovery
            .messages
            .iter()
            .filter_map(|message| message.remote_id.clone())
            .collect::<Vec<_>>();
        remote_ids.sort();
        remote_ids.dedup();
        // До этого момента письма ещё не в БД - список остаётся полным набором
        // действительно новых remote_id, а не тем, что уже успело устареть.
        let unknown_remote_ids = self.db.unknown_remote_ids(account.id, &remote_ids).await?;
        let new_messages = unknown_remote_ids.len();
        self.db
            .save_discovered_folders(account.id, &discovery.folders)
            .await?;
        self.db
            .reconcile_imap_snapshot(account.id, &discovery.server_uids, &discovery.reset_folders)
            .await?;
        self.db
            .reconcile_discovered_folders(account.id, &discovery.folders)
            .await?;
        self.db
            .apply_imap_vanished(account.id, &discovery.deleted_uids)
            .await?;
        self.db
            .apply_imap_flag_updates(account.id, &discovery.flag_updates)
            .await?;
        self.db
            .reconcile_remote_projections(
                account.id,
                &discovery.messages,
                &discovery.changed_remote_ids,
                discovery.remote_snapshot.as_deref(),
            )
            .await?;
        self.db
            .save_discovered_messages(account.id, &discovery.messages)
            .await?;
        // Письма к этому моменту уже в БД и получили локальные id - можно
        // достать их для уведомления. Только Входящие (роль 'inbox'): другие
        // папки уведомлению не нужны.
        let new_message_ids = if unknown_remote_ids.is_empty() {
            Vec::new()
        } else {
            self.db
                .inbox_message_ids_by_remote_ids(account.id, &unknown_remote_ids)
                .await?
        };
        self.db
            .save_folder_sync_tokens(account.id, &discovery.folders)
            .await?;
        if let Err(error) = self.db.process_mail_rules().await {
            tracing::warn!(%error, "правила обработки будут повторены при следующей синхронизации");
        }
        Ok(InboxSyncResult {
            downloaded,
            new_messages,
            had_baseline,
            new_message_ids,
        })
    }

    /// Совместимый интерфейс для IMAP/Gmail: полный результат синхронизации
    /// Входящих, включая новые id для уведомлений.
    pub async fn sync_mail_inbox(&self, account: &Account) -> Result<InboxSyncResult> {
        self.sync_mail_inbox_delta(account).await
    }

    /// Гарантировать наличие сырого MIME письма локально. Если кэш был вычищен
    /// по глубине хранения, письмо докачивается с сервера и сохраняется, чтобы
    /// открыться мгновенно в этой сессии (prune действует только на старте).
    pub async fn ensure_message_raw(&self, message_id: i64) -> Result<()> {
        let Some((account_id, folder_path, uid, remote_id, has_raw)) =
            self.db.message_fetch_locator(message_id).await?
        else {
            return Ok(());
        };
        if has_raw {
            return Ok(());
        }
        let Some(account) = self
            .db
            .list_accounts()
            .await?
            .into_iter()
            .find(|item| item.id == account_id)
        else {
            return Ok(());
        };
        let access_token = self.mail_credential(&account).await?;
        let backend = Self::mail_backend(&account)?;
        let raw = backend
            .fetch_message_raw(
                &account.email,
                &access_token,
                &folder_path,
                uid as u32,
                remote_id.as_deref(),
            )
            .await?;
        self.db.store_fetched_raw(message_id, &raw).await?;
        tracing::info!(message_id, account = %crate::logging::mask_email(&account.email), "письмо докачано с сервера (вне кэша)");
        Ok(())
    }

    /// Очистить кэш всех аккаунтов по их глубине хранения. Вызывается ОДИН РАЗ
    /// при старте приложения: в течение сессии свежие письма не удаляются, а
    /// письма за рамками периода при открытии докачиваются с сервера.
    pub async fn prune_all_caches_on_start(&self) -> Result<()> {
        for account in self.db.list_accounts().await? {
            if account.retention_days <= 0 {
                continue;
            }
            match self
                .db
                .prune_cached_messages(account.id, account.retention_days)
                .await
            {
                Ok(pruned) if pruned > 0 => tracing::info!(
                    account = %crate::logging::mask_email(&account.email),
                    pruned,
                    retention_days = account.retention_days,
                    "кэш очищен по глубине хранения (старт)"
                ),
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(account = %crate::logging::mask_email(&account.email), %error, "автоочистка кэша не удалась")
                }
            }
        }
        Ok(())
    }

    /// Определить базовые адреса CalDAV/CardDAV для аккаунта: уже заданные
    /// на аккаунте (ручная настройка или прошлое обнаружение), фиксированные
    /// адреса Яндекса, либо RFC 6764 .well-known/{caldav,carddav} по домену
    /// почты. Найденные адреса сохраняются на аккаунте, чтобы не искать их
    /// заново при каждой синхронизации. SRV-записи (_caldavs._tcp) не
    /// проверяются - см. dav::discover_well_known.
    async fn resolve_dav_bases(
        &self,
        account: &Account,
    ) -> Result<(Option<String>, Option<String>)> {
        if account.provider == Provider::Yandex {
            let (cal, card) = dav::resolve_yandex_bases(
                account.caldav_url.as_deref(),
                account.carddav_url.as_deref(),
            );
            return Ok((Some(cal), Some(card)));
        }
        let mut caldav_url = account.caldav_url.clone();
        let mut carddav_url = account.carddav_url.clone();
        if let Some((_, domain)) = account.email.rsplit_once('@') {
            let origin = format!("https://{domain}");
            if caldav_url.is_none() {
                caldav_url = dav::discover_well_known(&origin, dav::WELL_KNOWN_CALDAV).await;
            }
            if carddav_url.is_none() {
                carddav_url = dav::discover_well_known(&origin, dav::WELL_KNOWN_CARDDAV).await;
            }
        }
        if caldav_url != account.caldav_url || carddav_url != account.carddav_url {
            self.db
                .set_dav_urls(account.id, caldav_url.as_deref(), carddav_url.as_deref())
                .await?;
        }
        Ok((caldav_url, carddav_url))
    }

    /// Обновить календарь и контакты по CalDAV/CardDAV, не запуская тяжёлую
    /// IMAP-синхронизацию. Работает для любого провайдера с известными или
    /// обнаруженными DAV-адресами (см. resolve_dav_bases) - раньше это было
    /// жёстко привязано к Яндексу.
    pub async fn sync_dav_auxiliary_account(
        &self,
        account: &Account,
    ) -> Result<AuxiliarySaveResult> {
        self.sync_registry
            .exclusive(
                account.id,
                SyncKind::Auxiliary,
                self.sync_dav_auxiliary_account_inner(account),
            )
            .await
    }

    async fn sync_dav_auxiliary_account_inner(
        &self,
        account: &Account,
    ) -> Result<AuxiliarySaveResult> {
        // auxiliary_credential (не oauth_access_token) - иначе аккаунт с
        // Password/AppPassword (iCloud, Mail.ru, generic) упадёт на разборе
        // JSON ещё до диспетчеризации по провайдеру.
        let secret = self.auxiliary_credential(account).await?;
        let auth = dav::DavAuth::new(
            dav::dav_auth_scheme(account.provider, account.auth_kind),
            account
                .username
                .clone()
                .unwrap_or_else(|| account.email.clone()),
            secret.as_str(),
        );
        let (caldav_base, carddav_base) = self.resolve_dav_bases(account).await?;
        let cursors = self.db.auxiliary_sync_cursors(account.id).await?;
        let dav = dav::sync_dav_account(
            &account.email,
            &auth,
            caldav_base.as_deref(),
            carddav_base.as_deref(),
            &cursors,
        )
        .await?;
        self.db.save_dav(account.id, &dav).await
    }

    /// Обновить Google Calendar, Contacts и Tasks отдельно от IMAP.
    pub async fn sync_google_auxiliary_account(
        &self,
        account: &Account,
    ) -> Result<AuxiliarySaveResult> {
        self.sync_registry
            .exclusive(
                account.id,
                SyncKind::Auxiliary,
                self.sync_google_auxiliary_account_inner(account),
            )
            .await
    }

    async fn sync_google_auxiliary_account_inner(
        &self,
        account: &Account,
    ) -> Result<AuxiliarySaveResult> {
        let access_token = self.oauth_access_token(account).await?;
        let cursors = self.db.auxiliary_sync_cursors(account.id).await?;
        let data = sync_google_services(&access_token, &cursors).await?;
        self.db.save_google_services(account.id, &data).await
    }

    /// Обновить календарь и контакты Exchange через EWS отдельно от почты.
    pub async fn sync_exchange_auxiliary_account(
        &self,
        account: &Account,
    ) -> Result<AuxiliarySaveResult> {
        self.sync_registry
            .exclusive(
                account.id,
                SyncKind::Auxiliary,
                self.sync_exchange_auxiliary_account_inner(account),
            )
            .await
    }

    async fn sync_exchange_auxiliary_account_inner(
        &self,
        account: &Account,
    ) -> Result<AuxiliarySaveResult> {
        let credential = self.mail_credential(account).await?;
        let endpoint = account.ews_url.clone().ok_or_else(|| {
            crate::Error::AccountConfig("для Exchange не настроен адрес EWS".into())
        })?;
        let username = account
            .username
            .clone()
            .unwrap_or_else(|| account.email.clone());
        let backend = EwsBackend { endpoint, username };
        let cursors = self.db.auxiliary_sync_cursors(account.id).await?;
        let data = backend.auxiliary(&credential, &cursors).await?;
        self.db
            .save_auxiliary_data(account.id, "exchange", &data)
            .await
    }

    /// Обновить дополнительные сервисы поддерживаемого провайдера.
    /// Почтовый цикл вызывает только почтовую синхронизацию, а этот метод —
    /// единственная точка входа для календарей, контактов и задач.
    pub async fn sync_auxiliary_account(&self, account: &Account) -> Result<AuxiliarySaveResult> {
        // JMAP-аккаунт живёт как Provider::Generic (см. add_jmap_password), но
        // у JMAP нет календаря/контактов через DAV - без этой проверки он
        // попал бы в общую DAV-ветку и на каждой синхронизации безрезультатно
        // дёргал бы .well-known на своём домене.
        if account.backend_kind == BackendKind::Jmap {
            return Ok(AuxiliarySaveResult::default());
        }
        match account.provider {
            Provider::Gmail => self.sync_google_auxiliary_account(account).await,
            Provider::Exchange => self.sync_exchange_auxiliary_account(account).await,
            Provider::Yandex
            | Provider::Icloud
            | Provider::Mailru
            | Provider::Outlook
            | Provider::Generic => self.sync_dav_auxiliary_account(account).await,
        }
    }

    /// Ответить на приглашение (RSVP): единая точка входа, дальше ветвление
    /// по провайдеру - тот же приём, что у write_event/delete_event в
    /// auxiliary.rs, только здесь нужен доступ к self (учётные данные,
    /// SMTP-транспорт), поэтому это метод AccountManager, а не свободная
    /// функция в auxiliary.rs.
    /// - Google: events.patch с обновлённым responseStatus (sendUpdates=all) -
    ///   сервер сам уведомляет организатора.
    /// - CalDAV (Яндекс и остальные DAV-провайдеры): сервер не рассылает
    ///   приглашения сам - PUT своей копии события с новым PARTSTAT плюс
    ///   письмо-ответ организатору в формате iMIP (METHOD:REPLY, RFC 5546).
    /// - Exchange: штатные SOAP-операции AcceptItem/DeclineItem/
    ///   TentativelyAcceptItem - сервер сам формирует и рассылает ответ.
    pub async fn respond_to_event(
        &self,
        account: &Account,
        calendar_source: &str,
        remote: RemoteObject<'_>,
        event: &crate::model::Event,
        response: crate::model::RsvpResponse,
    ) -> Result<()> {
        match account.provider {
            Provider::Gmail => {
                let token = self.oauth_access_token(account).await?;
                let attendees =
                    auxiliary::updated_attendees(&event.attendees, &account.email, response);
                let remote_url = remote.remote_url.ok_or_else(|| {
                    crate::Error::AccountConfig("у события нет серверного идентификатора".into())
                })?;
                auxiliary::respond_to_google_event(calendar_source, remote_url, &attendees, &token)
                    .await
            }
            Provider::Yandex
            | Provider::Icloud
            | Provider::Mailru
            | Provider::Outlook
            | Provider::Generic => {
                self.respond_to_dav_event(account, calendar_source, remote, event, response)
                    .await
            }
            Provider::Exchange => {
                let credential = self.mail_credential(account).await?;
                let endpoint = account.ews_url.clone().ok_or_else(|| {
                    crate::Error::AccountConfig("для Exchange не настроен адрес EWS".into())
                })?;
                let username = account
                    .username
                    .clone()
                    .unwrap_or_else(|| account.email.clone());
                let backend = EwsBackend { endpoint, username };
                let remote_url = remote.remote_url.ok_or_else(|| {
                    crate::Error::AccountConfig("у события нет серверного идентификатора".into())
                })?;
                let item_id = remote_url.strip_prefix("ews-event:").ok_or_else(|| {
                    crate::Error::AccountConfig(
                        "неизвестный серверный идентификатор события".into(),
                    )
                })?;
                backend
                    .respond_to_calendar_item(&credential, item_id, response)
                    .await
            }
        }
    }

    /// Ветка respond_to_event для CalDAV (Яндекс и остальные DAV-провайдеры):
    /// сервер не знает про iMIP, поэтому отвечающий сам обновляет свою копию
    /// события (PUT с новым PARTSTAT) и сам же отправляет организатору
    /// письмо-ответ. Оба шага делаем последовательно; если PUT прошёл, а
    /// письмо не ушло - вернём ошибку письма (само событие уже несёт
    /// правильный статус локально после следующей синхронизации, и
    /// update_own_partstat в commands.rs применит его немедленно только если
    /// respond_to_event вернул Ok, так что тут именно так: без письма
    /// организатор не узнает об ответе).
    /// EwsBackend по данным аккаунта. Вынесено, чтобы шесть операций записи
    /// не повторяли сборку endpoint/username каждая по-своему.
    fn ews_backend(&self, account: &Account) -> Result<EwsBackend> {
        let endpoint = account.ews_url.clone().ok_or_else(|| {
            crate::Error::AccountConfig("для Exchange не настроен адрес EWS".into())
        })?;
        let username = account
            .username
            .clone()
            .unwrap_or_else(|| account.email.clone());
        Ok(EwsBackend { endpoint, username })
    }

    /// Идентификатор элемента EWS из remote_url. Читающая сторона (ews.rs)
    /// сохраняет их с префиксом ews-event:/ews-contact:, поэтому чужой
    /// префикс - это признак, что объект пришёл от другого провайдера.
    fn ews_item_id<'a>(remote_url: &'a str, prefix: &str) -> Result<&'a str> {
        remote_url.strip_prefix(prefix).ok_or_else(|| {
            crate::Error::AccountConfig("неизвестный серверный идентификатор объекта".into())
        })
    }

    /// Создать или изменить событие. Для Exchange - через EWS, для остальных -
    /// прежним путём (Google REST или CalDAV).
    pub async fn write_event(
        &self,
        account: &Account,
        calendar_source: &str,
        remote: RemoteObject<'_>,
        input: &EventInput,
    ) -> Result<()> {
        let credential = self.auxiliary_credential(account).await?;
        if account.provider == Provider::Exchange {
            let backend = self.ews_backend(account)?;
            return match remote.remote_url {
                Some(url) => {
                    backend
                        .update_calendar_item(
                            &credential,
                            Self::ews_item_id(url, "ews-event:")?,
                            input,
                        )
                        .await
                }
                // ItemId созданного элемента намеренно отбрасываем: как и в
                // DAV-ветке, локальные записи получают серверные идентификаторы
                // ближайшим refresh_auxiliary, отдельного пути для них нет.
                None => backend
                    .create_calendar_item(&credential, input)
                    .await
                    .map(|_| ()),
            };
        }
        write_event(
            account.provider,
            account.auth_kind,
            &account.email,
            &credential,
            calendar_source,
            remote,
            input,
        )
        .await
    }

    /// Удалить событие.
    pub async fn delete_event(
        &self,
        account: &Account,
        calendar_source: &str,
        remote_url: &str,
        etag: Option<&str>,
    ) -> Result<()> {
        let credential = self.auxiliary_credential(account).await?;
        if account.provider == Provider::Exchange {
            return self
                .ews_backend(account)?
                .delete_calendar_item(&credential, Self::ews_item_id(remote_url, "ews-event:")?)
                .await;
        }
        delete_event(
            account.provider,
            account.auth_kind,
            &account.email,
            &credential,
            calendar_source,
            remote_url,
            etag,
        )
        .await
    }

    /// Создать или изменить контакт.
    pub async fn write_contact(
        &self,
        account: &Account,
        collection_url: Option<&str>,
        remote: RemoteObject<'_>,
        input: &ContactInput,
    ) -> Result<()> {
        let credential = self.auxiliary_credential(account).await?;
        if account.provider == Provider::Exchange {
            let backend = self.ews_backend(account)?;
            return match remote.remote_url {
                Some(url) => {
                    backend
                        .update_contact_item(
                            &credential,
                            Self::ews_item_id(url, "ews-contact:")?,
                            input,
                        )
                        .await
                }
                None => backend
                    .create_contact_item(&credential, input)
                    .await
                    .map(|_| ()),
            };
        }
        write_contact(
            account.provider,
            account.auth_kind,
            &account.email,
            &credential,
            collection_url,
            remote,
            input,
        )
        .await
    }

    /// Удалить контакт.
    pub async fn delete_contact(
        &self,
        account: &Account,
        remote_url: &str,
        etag: Option<&str>,
    ) -> Result<()> {
        let credential = self.auxiliary_credential(account).await?;
        if account.provider == Provider::Exchange {
            return self
                .ews_backend(account)?
                .delete_contact_item(&credential, Self::ews_item_id(remote_url, "ews-contact:")?)
                .await;
        }
        delete_contact(
            account.provider,
            account.auth_kind,
            &account.email,
            &credential,
            remote_url,
            etag,
        )
        .await
    }

    async fn respond_to_dav_event(
        &self,
        account: &Account,
        calendar_source: &str,
        remote: RemoteObject<'_>,
        event: &crate::model::Event,
        response: crate::model::RsvpResponse,
    ) -> Result<()> {
        let organizer = event.organizer.clone().ok_or_else(|| {
            crate::Error::AccountConfig(
                "у события не указан организатор - отправить ответ некому".into(),
            )
        })?;
        let uid = event
            .uid
            .clone()
            .or_else(|| remote.uid.map(str::to_owned))
            .ok_or_else(|| crate::Error::AccountConfig("у события нет UID".into()))?;
        let attendees = auxiliary::updated_attendees(&event.attendees, &account.email, response);
        // auxiliary_credential (не oauth_access_token) - Яндекс всегда Oauth2,
        // но остальные DAV-провайдеры (iCloud, Mail.ru, generic) обычно идут
        // по паролю/app-password, и oauth_access_token для них упал бы.
        let credential = self.auxiliary_credential(account).await?;
        let input = auxiliary::event_to_input(event, attendees.clone());
        auxiliary::write_event(
            account.provider,
            account.auth_kind,
            &account.email,
            &credential,
            calendar_source,
            remote,
            &input,
        )
        .await?;
        let own = attendees
            .iter()
            .find(|attendee| attendee.email.eq_ignore_ascii_case(&account.email))
            .ok_or_else(|| {
                crate::Error::AccountConfig(
                    "пользователь не найден среди участников события".into(),
                )
            })?;
        let ics = auxiliary::imip_reply_body(&uid, &organizer, event.sequence, own);
        let subject = format!("Re: {}", event.summary);
        let body_text = match response {
            crate::model::RsvpResponse::Accepted => "Приглашение принято.",
            crate::model::RsvpResponse::Declined => "Приглашение отклонено.",
            crate::model::RsvpResponse::Tentative => "Участие пока под вопросом.",
        }
        .to_owned();
        let message = crate::backend::OutgoingMessage {
            from: account.email.clone(),
            to: vec![organizer],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject,
            body_text,
            body_html: None,
            attachments: vec![crate::backend::OutgoingAttachment {
                filename: "reply.ics".into(),
                mime_type: "text/calendar; method=REPLY; charset=UTF-8".into(),
                data: ics.into_bytes(),
            }],
        };
        self.send_outgoing(account.id, message).await
    }

    /// Отправить письмо через транспорт выбранного аккаунта; поле From задаёт core.
    pub async fn send_outgoing(
        &self,
        account_id: i64,
        mut message: crate::backend::OutgoingMessage,
    ) -> Result<()> {
        let account = self
            .db
            .list_accounts()
            .await?
            .into_iter()
            .find(|account| account.id == account_id)
            .ok_or_else(|| crate::Error::AccountConfig("аккаунт отправителя не найден".into()))?;
        message.from = account.email.clone();
        let credential = self.mail_credential(&account).await?;
        let backend = Self::mail_backend(&account)?;
        let provider = backend.provider_id();
        if let SendOutcome::NeedsSentAppend(raw) = backend.send(message, &credential).await?
            && let Err(error) = backend.append_sent(&account.email, &credential, &raw).await
        {
            let payload = sent_append_payload(&raw)?;
            self.db
                .queue_sent_append(account.id, &payload, &error.to_string())
                .await?;
            tracing::warn!(
                account = %crate::logging::mask_email(&account.email),
                provider,
                %error,
                "SMTP доставил письмо; сохранение в Отправленные поставлено в отдельный retry"
            );
            return Ok(());
        }
        tracing::info!(
            account = %crate::logging::mask_email(&account.email),
            provider,
            "письмо отправлено, серверная копия сохранена"
        );
        Ok(())
    }

    /// Ждать серверное изменение через механизм выбранного транспорта.
    pub async fn wait_for_mail_change(&self, account: &Account) -> Result<()> {
        let credential = self.mail_credential(account).await?;
        Self::mail_backend(account)?
            .wait_for_change(&account.email, &credential)
            .await
    }

    /// Доставить накопленные локальные операции с ограниченным retry/backoff.
    pub async fn process_mail_outbox(&self, account: &Account) -> Result<usize> {
        if account.provider == Provider::Exchange
            && !self
                .exchange_outbox_repaired
                .lock()
                .await
                .contains(&account.id)
        {
            self.db
                .requeue_exchange_change_key_operations(account.id)
                .await?;
            self.exchange_outbox_repaired
                .lock()
                .await
                .insert(account.id);
        }
        let operations = self.db.claim_outbox_operations(account.id, 50).await?;
        if operations.is_empty() {
            return Ok(0);
        }
        // Не трогаем transport/OAuth/quota gate, когда отправлять нечего.
        let token = self.mail_credential(account).await?;
        let backend = Self::mail_backend(account)?;
        let mut completed = 0;
        for operation in operations {
            let applied = if operation.op_kind == "send" {
                match serde_json::from_str::<crate::backend::OutgoingMessage>(&operation.payload) {
                    Ok(message) => match backend.send(message, &token).await {
                        Ok(SendOutcome::SavedOnServer) => Ok(()),
                        Ok(SendOutcome::NeedsSentAppend(raw)) => {
                            match backend.append_sent(&account.email, &token, &raw).await {
                                Ok(()) => Ok(()),
                                Err(error) => {
                                    let payload = sent_append_payload(&raw)?;
                                    self.db
                                        .convert_outbox_to_sent_append(
                                            operation.id,
                                            &payload,
                                            &error.to_string(),
                                        )
                                        .await?;
                                    tracing::warn!(
                                        account = %crate::logging::mask_email(&account.email),
                                        operation = operation.id,
                                        %error,
                                        "SMTP доставил scheduled-письмо; retry продолжит только IMAP APPEND"
                                    );
                                    continue;
                                }
                            }
                        }
                        Err(error) => Err(error),
                    },
                    Err(error) => Err(crate::Error::Json(error)),
                }
            } else if operation.op_kind == "append_sent" {
                match sent_append_raw(&operation.payload) {
                    Ok(raw) => backend.append_sent(&account.email, &token, &raw).await,
                    Err(error) => Err(error),
                }
            } else {
                backend
                    .apply_operation(
                        &account.email,
                        &token,
                        &operation.op_kind,
                        &operation.payload,
                    )
                    .await
            };
            match applied {
                Ok(()) => {
                    self.db.complete_outbox_operation(&operation).await?;
                    completed += 1;
                }
                Err(error) => {
                    self.db
                        .fail_outbox_operation(operation.id, &error.to_string())
                        .await?;
                    tracing::warn!(
                        account = %crate::logging::mask_email(&account.email),
                        operation = operation.id,
                        attempts = operation.attempts + 1,
                        %error,
                        "операция outbox будет повторена"
                    );
                }
            }
        }
        Ok(completed)
    }

    /// Подключить обычный IMAP/SMTP-аккаунт по паролю приложения. Пароль
    /// проверяется на сервере и хранится только в системном keychain.
    pub async fn add_password_imap(
        &self,
        email: &str,
        display_name: &str,
        username: &str,
        password: &str,
        config: &ProviderConfig,
    ) -> Result<ConnectedAccountSync> {
        if password.is_empty() {
            return Err(crate::Error::AccountConfig("пароль не указан".into()));
        }
        let imap = config.imap.clone().ok_or_else(|| {
            crate::Error::AccountConfig("сервер IMAP не найден; укажите его вручную".into())
        })?;
        let backend = GenericImapBackend {
            username: username.to_owned(),
            imap: imap.clone(),
            smtp: config.smtp.clone(),
        };
        backend.validate(email, password).await?;

        let secret_ref = format!("mail-password:{}", email.to_lowercase());
        let previous_secret_ref = self.existing_secret_ref(email).await;
        let entry = keyring::Entry::new("truemail", &secret_ref)
            .map_err(|error| crate::Error::Keyring(error.to_string()))?;
        entry
            .set_password(password)
            .map_err(|error| crate::Error::Keyring(error.to_string()))?;
        let account = match self
            .db
            .save_account(&NewAccount {
                email: email.to_owned(),
                display_name: display_name.to_owned(),
                provider: config.provider,
                backend_kind: BackendKind::Imap,
                auth_kind: config.auth_kind,
                imap: Some(imap),
                smtp: config.smtp.clone(),
                ews_url: None,
                caldav_url: None,
                carddav_url: None,
                jmap_url: None,
                username: Some(username.to_owned()),
                secret_ref: secret_ref.clone(),
                color: Some("#3F7C85".into()),
            })
            .await
        {
            Ok(account) => account,
            Err(error) => {
                let _ = entry.delete_credential();
                return Err(error);
            }
        };
        Self::cleanup_stale_secret(previous_secret_ref, &secret_ref);
        Ok(ConnectedAccountSync {
            account,
            mail_folders: 0,
            calendars: 0,
            events: 0,
            contacts: 0,
            warnings: if config.smtp.is_none() {
                vec!["SMTP-сервер не найден: чтение работает, отправку нужно настроить".into()]
            } else {
                Vec::new()
            },
        })
    }

    /// Подключить on-premises Exchange через Autodiscover и EWS. WinHTTP
    /// выполняет Negotiate с откатом на NTLM; пароль остаётся в keychain.
    pub async fn add_exchange_ews(
        &self,
        email: &str,
        display_name: &str,
        username: &str,
        password: &str,
        server_hint: Option<&str>,
    ) -> Result<ConnectedAccountSync> {
        if password.is_empty() {
            return Err(crate::Error::AccountConfig("пароль не указан".into()));
        }
        let endpoint =
            crate::backend::discover_ews_url(email, username, password, server_hint).await?;
        let backend = EwsBackend {
            endpoint: endpoint.clone(),
            username: username.to_owned(),
        };
        backend.validate(email, password).await?;
        let secret_ref = format!("exchange-password:{}", email.to_lowercase());
        let previous_secret_ref = self.existing_secret_ref(email).await;
        let entry = keyring::Entry::new("truemail", &secret_ref)
            .map_err(|error| crate::Error::Keyring(error.to_string()))?;
        entry
            .set_password(password)
            .map_err(|error| crate::Error::Keyring(error.to_string()))?;
        let account = match self
            .db
            .save_account(&NewAccount {
                email: email.to_owned(),
                display_name: display_name.to_owned(),
                provider: Provider::Exchange,
                backend_kind: BackendKind::Ews,
                auth_kind: AuthKind::Ntlm,
                imap: None,
                smtp: None,
                ews_url: Some(endpoint),
                caldav_url: None,
                carddav_url: None,
                jmap_url: None,
                username: Some(username.to_owned()),
                secret_ref: secret_ref.clone(),
                color: Some("#0078D4".into()),
            })
            .await
        {
            Ok(account) => account,
            Err(error) => {
                let _ = entry.delete_credential();
                return Err(error);
            }
        };
        Self::cleanup_stale_secret(previous_secret_ref, &secret_ref);
        Ok(ConnectedAccountSync {
            account,
            mail_folders: 0,
            calendars: 0,
            events: 0,
            contacts: 0,
            warnings: Vec::new(),
        })
    }

    /// Подключить JMAP-сервер по отдельному паролю приложения.
    pub async fn add_jmap_password(
        &self,
        email: &str,
        display_name: &str,
        username: &str,
        password: &str,
        session_url: &str,
    ) -> Result<ConnectedAccountSync> {
        if password.is_empty() {
            return Err(crate::Error::AccountConfig("пароль не указан".into()));
        }
        let backend = JmapBackend {
            session_url: session_url.trim().to_owned(),
            username: username.to_owned(),
        };
        backend.validate(email, password).await?;
        let secret_ref = format!("jmap-password:{}", email.to_lowercase());
        let previous_secret_ref = self.existing_secret_ref(email).await;
        let entry = keyring::Entry::new("truemail", &secret_ref)
            .map_err(|error| crate::Error::Keyring(error.to_string()))?;
        entry
            .set_password(password)
            .map_err(|error| crate::Error::Keyring(error.to_string()))?;
        let account = match self
            .db
            .save_account(&NewAccount {
                email: email.to_owned(),
                display_name: display_name.to_owned(),
                provider: Provider::Generic,
                backend_kind: BackendKind::Jmap,
                auth_kind: AuthKind::AppPassword,
                imap: None,
                smtp: None,
                ews_url: None,
                caldav_url: None,
                carddav_url: None,
                jmap_url: Some(session_url.trim().to_owned()),
                username: Some(username.to_owned()),
                secret_ref: secret_ref.clone(),
                color: Some("#6B5DD3".into()),
            })
            .await
        {
            Ok(account) => account,
            Err(error) => {
                let _ = entry.delete_credential();
                return Err(error);
            }
        };
        Self::cleanup_stale_secret(previous_secret_ref, &secret_ref);
        Ok(ConnectedAccountSync {
            account,
            mail_folders: 0,
            calendars: 0,
            events: 0,
            contacts: 0,
            warnings: Vec::new(),
        })
    }

    /// Сохранить авторизованный аккаунт Яндекса. OAuth-токены никогда не попадают в SQLite.
    pub async fn add_yandex_oauth(
        &self,
        email: &str,
        display_name: &str,
        token: OAuthToken,
    ) -> Result<ConnectedAccountSync> {
        let access_token = Zeroizing::new(token.access_token.clone());
        let secret_ref = format!("yandex-oauth:{}", email.to_lowercase());
        let previous_secret_ref = self.existing_secret_ref(email).await;
        let entry = keyring::Entry::new("truemail", &secret_ref)
            .map_err(|e| crate::Error::Keyring(e.to_string()))?;
        let credential = StoredOAuthCredential::from(token);
        let serialized = Zeroizing::new(serde_json::to_string(&credential)?);
        entry
            .set_password(&serialized)
            .map_err(|e| crate::Error::Keyring(e.to_string()))?;

        let account = match self
            .db
            .save_account(&NewAccount {
                email: email.to_owned(),
                display_name: display_name.to_owned(),
                provider: Provider::Yandex,
                backend_kind: BackendKind::Imap,
                auth_kind: AuthKind::Oauth2,
                imap: Some(ServerConfig {
                    host: "imap.yandex.com".into(),
                    port: 993,
                    security: Security::Ssl,
                }),
                smtp: Some(ServerConfig {
                    host: "smtp.yandex.com".into(),
                    port: 465,
                    security: Security::Ssl,
                }),
                ews_url: None,
                caldav_url: None,
                carddav_url: None,
                jmap_url: None,
                username: Some(email.to_owned()),
                secret_ref: secret_ref.clone(),
                color: Some("#5B63D3".into()),
            })
            .await
        {
            Ok(account) => account,
            Err(error) => {
                let _ = entry.delete_credential();
                return Err(error);
            }
        };
        Self::cleanup_stale_secret(previous_secret_ref, &secret_ref);

        // Код уже обменян и одноразовый, поэтому токен сначала надёжно
        // сохраняется. Проверки доступа быстрые; их временный сбой становится
        // предупреждением и не заставляет пользователя получать новый код.
        let dav_auth =
            dav::DavAuth::new(dav::DavAuthScheme::BasicToken, email, access_token.as_str());
        let (mail_access, dav_access) = tokio::join!(
            YandexBackend.validate(email, &access_token),
            validate_dav(&dav_auth, YANDEX_CALDAV_BASE, YANDEX_CARDDAV_BASE)
        );
        let mut warnings = Vec::new();
        if let Err(error) = mail_access {
            warnings.push(format!("Проверка доступа к почте: {error}"));
        }
        if let Err(error) = dav_access {
            warnings.push(format!("Проверка календаря и контактов: {error}"));
        }

        Ok(ConnectedAccountSync {
            account,
            mail_folders: 0,
            calendars: 0,
            events: 0,
            contacts: 0,
            warnings,
        })
    }

    /// Сохранить Gmail-аккаунт после desktop OAuth PKCE. Токены остаются в keychain.
    pub async fn add_gmail_oauth(
        &self,
        email: &str,
        display_name: &str,
        token: OAuthToken,
    ) -> Result<ConnectedAccountSync> {
        tracing::info!(email = %crate::logging::mask_email(email), scope = ?token.scope, "Gmail OAuth: провайдер вернул scope");
        if let Some(granted) = token.scope.as_deref() {
            let granted: std::collections::HashSet<_> = granted.split_whitespace().collect();
            let missing: Vec<_> = GOOGLE_SCOPES
                .split_whitespace()
                .filter(|scope| !granted.contains(scope))
                .collect();
            if !missing.is_empty() {
                tracing::warn!(email = %crate::logging::mask_email(email), missing = ?missing, "Gmail OAuth: Google выдал не все запрошенные scope");
                return Err(crate::Error::AccountConfig(format!(
                    "Google не выдал все разрешения truemail. Повторите подключение и подтвердите доступ к Gmail, Календарю, Контактам и Задачам. Не выданы: {}",
                    missing.join(", ")
                )));
            }
        } else {
            tracing::warn!(
                email = %crate::logging::mask_email(email),
                "Gmail OAuth: провайдер не вернул поле scope, проверку разрешений пропускаем"
            );
        }
        let access_token = Zeroizing::new(token.access_token.clone());
        let secret_ref = format!("google-oauth:{}", email.to_lowercase());
        let previous_secret_ref = self.existing_secret_ref(email).await;
        let entry = keyring::Entry::new("truemail", &secret_ref)
            .map_err(|e| crate::Error::Keyring(e.to_string()))?;
        let credential = StoredOAuthCredential::from(token);
        let serialized = Zeroizing::new(serde_json::to_string(&credential)?);
        entry
            .set_password(&serialized)
            .map_err(|e| crate::Error::Keyring(e.to_string()))?;

        let account = match self
            .db
            .save_account(&NewAccount {
                email: email.to_owned(),
                display_name: display_name.to_owned(),
                provider: Provider::Gmail,
                backend_kind: BackendKind::Imap,
                auth_kind: AuthKind::Oauth2,
                imap: Some(ServerConfig {
                    host: "imap.gmail.com".into(),
                    port: 993,
                    security: Security::Ssl,
                }),
                smtp: Some(ServerConfig {
                    host: "smtp.gmail.com".into(),
                    port: 465,
                    security: Security::Ssl,
                }),
                ews_url: None,
                caldav_url: None,
                carddav_url: None,
                jmap_url: None,
                username: Some(email.to_owned()),
                secret_ref: secret_ref.clone(),
                color: Some("#4285F4".into()),
            })
            .await
        {
            Ok(account) => account,
            Err(error) => {
                let _ = entry.delete_credential();
                return Err(error);
            }
        };
        Self::cleanup_stale_secret(previous_secret_ref, &secret_ref);

        let mut warnings = Vec::new();
        if let Err(error) = GmailBackend.validate(email, &access_token).await {
            warnings.push(format!("Проверка доступа к Gmail: {error}"));
        }
        Ok(ConnectedAccountSync {
            account,
            mail_folders: 0,
            calendars: 0,
            events: 0,
            contacts: 0,
            warnings,
        })
    }

    /// Сохранить Outlook/Exchange Online после Microsoft desktop OAuth PKCE.
    pub async fn add_outlook_oauth(
        &self,
        email: &str,
        display_name: &str,
        token: OAuthToken,
    ) -> Result<ConnectedAccountSync> {
        if let Some(granted) = token.scope.as_deref() {
            let granted: std::collections::HashSet<_> = granted.split_whitespace().collect();
            let missing: Vec<_> = MICROSOFT_SCOPES
                .split_whitespace()
                .filter(|scope| !granted.contains(scope))
                .collect();
            if !missing.is_empty() {
                return Err(crate::Error::AccountConfig(format!(
                    "Microsoft не выдал все разрешения для почты. Не выданы: {}",
                    missing.join(", ")
                )));
            }
        }
        let access_token = Zeroizing::new(token.access_token.clone());
        let secret_ref = format!("microsoft-oauth:{}", email.to_lowercase());
        let previous_secret_ref = self.existing_secret_ref(email).await;
        let entry = keyring::Entry::new("truemail", &secret_ref)
            .map_err(|error| crate::Error::Keyring(error.to_string()))?;
        let credential = StoredOAuthCredential::from(token);
        let serialized = Zeroizing::new(serde_json::to_string(&credential)?);
        entry
            .set_password(&serialized)
            .map_err(|error| crate::Error::Keyring(error.to_string()))?;

        let account = match self
            .db
            .save_account(&NewAccount {
                email: email.to_owned(),
                display_name: display_name.to_owned(),
                provider: Provider::Outlook,
                backend_kind: BackendKind::Imap,
                auth_kind: AuthKind::Oauth2,
                imap: Some(ServerConfig {
                    host: "outlook.office365.com".into(),
                    port: 993,
                    security: Security::Ssl,
                }),
                smtp: Some(ServerConfig {
                    host: "smtp.office365.com".into(),
                    port: 587,
                    security: Security::Starttls,
                }),
                ews_url: None,
                caldav_url: None,
                carddav_url: None,
                jmap_url: None,
                username: Some(email.to_owned()),
                secret_ref: secret_ref.clone(),
                color: Some("#0078D4".into()),
            })
            .await
        {
            Ok(account) => account,
            Err(error) => {
                let _ = entry.delete_credential();
                return Err(error);
            }
        };
        Self::cleanup_stale_secret(previous_secret_ref, &secret_ref);

        let mut warnings = Vec::new();
        if let Err(error) = OutlookBackend.validate(email, &access_token).await {
            warnings.push(format!("Проверка доступа к Outlook: {error}"));
        }
        Ok(ConnectedAccountSync {
            account,
            mail_folders: 0,
            calendars: 0,
            events: 0,
            contacts: 0,
            warnings,
        })
    }

    /// Полная синхронизация уже сохранённого аккаунта; предназначена для фоновой задачи.
    pub async fn sync_mail_account(&self, account: &Account) -> Result<ConnectedAccountSync> {
        self.sync_registry
            .exclusive(
                account.id,
                SyncKind::Mail,
                self.sync_mail_account_inner(account),
            )
            .await
    }

    async fn sync_mail_account_inner(&self, account: &Account) -> Result<ConnectedAccountSync> {
        let access_token = self.mail_credential(account).await?;
        let backend = Self::mail_backend(account)?;
        let cursors = self.db.folder_sync_cursors(account.id).await?;
        let mut warnings = Vec::new();
        // Имена и счётчики папок появляются в UI сразу, пока тела писем и DAV
        // коллекции загружаются параллельно.
        if let Ok(folders) = backend
            .discover_folders(&account.email, &access_token)
            .await
            && let Err(error) = self.db.save_discovered_folders(account.id, &folders).await
        {
            warnings.push(format!("Папки почты не сохранились: {error}"));
        }
        let imap_result = backend
            .discover(
                &account.email,
                &access_token,
                &cursors,
                account.retention_days,
            )
            .await;
        let mail_folders = match imap_result {
            Ok(imap) => {
                let saved = match self
                    .db
                    .save_discovered_folders(account.id, &imap.folders)
                    .await
                {
                    Ok(()) => {
                        match self
                            .db
                            .reconcile_imap_snapshot(
                                account.id,
                                &imap.server_uids,
                                &imap.reset_folders,
                            )
                            .await
                        {
                            Ok(_) => {
                                if let Err(error) = self
                                    .db
                                    .reconcile_discovered_folders(account.id, &imap.folders)
                                    .await
                                {
                                    warnings.push(format!("Удалённые папки не очищены: {error}"));
                                }
                                if let Err(error) = self
                                    .db
                                    .apply_imap_vanished(account.id, &imap.deleted_uids)
                                    .await
                                {
                                    warnings.push(format!("Удаления IMAP не сохранились: {error}"));
                                }
                                if let Err(error) = self
                                    .db
                                    .apply_imap_flag_updates(account.id, &imap.flag_updates)
                                    .await
                                {
                                    warnings.push(format!("Флаги писем не сохранились: {error}"));
                                }
                                match self
                                    .db
                                    .reconcile_remote_projections(
                                        account.id,
                                        &imap.messages,
                                        &imap.changed_remote_ids,
                                        imap.remote_snapshot.as_deref(),
                                    )
                                    .await
                                {
                                    Ok(_) => {
                                        match self
                                            .db
                                            .save_discovered_messages(account.id, &imap.messages)
                                            .await
                                        {
                                            Ok(()) => {
                                                match self
                                                    .db
                                                    .save_folder_sync_tokens(
                                                        account.id,
                                                        &imap.folders,
                                                    )
                                                    .await
                                                {
                                                    Ok(()) => {
                                                        if let Err(error) =
                                                            self.db.process_mail_rules().await
                                                        {
                                                            tracing::warn!(%error, "правила обработки будут повторены при следующей синхронизации");
                                                        }
                                                        Ok(())
                                                    }
                                                    Err(error) => Err(error),
                                                }
                                            }
                                            Err(error) => Err(error),
                                        }
                                    }
                                    Err(error) => Err(error),
                                }
                            }
                            Err(error) => Err(error),
                        }
                    }
                    Err(error) => Err(error),
                };
                match saved {
                    Ok(()) => imap.folders.len(),
                    Err(error) => {
                        warnings.push(format!("Почта подключена, но не сохранилась: {error}"));
                        0
                    }
                }
            }
            Err(error) => {
                self.remember_gmail_rate_limit(account, &error).await;
                warnings.push(format!(
                    "Почта подключена, первая синхронизация отложена: {error}"
                ));
                0
            }
        };
        Ok(ConnectedAccountSync {
            account: account.clone(),
            mail_folders,
            calendars: 0,
            events: 0,
            contacts: 0,
            warnings,
        })
    }
}
