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
    AuxiliarySyncCursors, CollectionCursor, DavCalendar, DavCollection, DavContact, DavEvent,
    DavSyncResult, SyncScope, sync_yandex_dav, validate_yandex_dav,
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
    YandexBackend,
};
use crate::model::{Account, AuthKind, BackendKind, NewAccount, Provider, Security, ServerConfig};
use crate::storage::Db;
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SyncKind {
    Mail,
    Auxiliary,
}

#[cfg(test)]
mod sync_registry_tests {
    use super::*;

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
                .is_ok()
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
}

#[derive(Default)]
struct SyncRegistry {
    locks: tokio::sync::Mutex<
        std::collections::HashMap<(i64, SyncKind), std::sync::Arc<tokio::sync::Semaphore>>,
    >,
}

impl SyncRegistry {
    async fn exclusive<T>(
        &self,
        account_id: i64,
        kind: SyncKind,
        operation: impl std::future::Future<Output = Result<T>>,
    ) -> Result<T> {
        let semaphore = self
            .locks
            .lock()
            .await
            .entry((account_id, kind))
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
        }
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
            tracing::info!(email = %account.email, provider = ?account.provider, scope = ?updated.scope, "OAuth-токен обновлён через refresh");
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
        let token = self.oauth_access_token(account).await?;
        crate::backend::gmail_latest_ids(&token, 25).await
    }

    /// Дозагрузить только последние входящие после события IMAP IDLE.
    pub async fn sync_mail_inbox(&self, account: &Account) -> Result<usize> {
        let access_token = self.mail_credential(account).await?;
        let backend = Self::mail_backend(account)?;
        let cursors = self.db.folder_sync_cursors(account.id).await?;
        let discovery = backend
            .discover_inbox(&account.email, &access_token, &cursors)
            .await?;
        self.db
            .save_discovered_folders(account.id, &discovery.folders)
            .await?;
        self.db
            .reconcile_imap_snapshot(account.id, &discovery.server_uids, &discovery.reset_folders)
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
        self.db
            .save_folder_sync_tokens(account.id, &discovery.folders)
            .await?;
        if let Err(error) = self.db.process_mail_rules().await {
            tracing::warn!(%error, "правила обработки будут повторены при следующей синхронизации");
        }
        Ok(discovery.messages.len())
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
        tracing::info!(message_id, account = %account.email, "письмо докачано с сервера (вне кэша)");
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
                    account = %account.email,
                    pruned,
                    retention_days = account.retention_days,
                    "кэш очищен по глубине хранения (старт)"
                ),
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(account = %account.email, %error, "автоочистка кэша не удалась")
                }
            }
        }
        Ok(())
    }

    /// Обновить календарь и контакты, не запуская тяжёлую IMAP-синхронизацию.
    pub async fn sync_yandex_dav_account(
        &self,
        account: &Account,
    ) -> Result<(usize, usize, usize)> {
        self.sync_registry
            .exclusive(
                account.id,
                SyncKind::Auxiliary,
                self.sync_yandex_dav_account_inner(account),
            )
            .await
    }

    async fn sync_yandex_dav_account_inner(
        &self,
        account: &Account,
    ) -> Result<(usize, usize, usize)> {
        let access_token = self.oauth_access_token(account).await?;
        let cursors = self.db.auxiliary_sync_cursors(account.id).await?;
        let dav = sync_yandex_dav(&account.email, &access_token, &cursors).await?;
        self.db.save_yandex_dav(account.id, &dav).await
    }

    /// Обновить Google Calendar, Contacts и Tasks отдельно от IMAP.
    pub async fn sync_google_auxiliary_account(
        &self,
        account: &Account,
    ) -> Result<(usize, usize, usize)> {
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
    ) -> Result<(usize, usize, usize)> {
        let access_token = self.oauth_access_token(account).await?;
        let cursors = self.db.auxiliary_sync_cursors(account.id).await?;
        let data = sync_google_services(&access_token, &cursors).await?;
        self.db.save_google_services(account.id, &data).await
    }

    /// Обновить календарь и контакты Exchange через EWS отдельно от почты.
    pub async fn sync_exchange_auxiliary_account(
        &self,
        account: &Account,
    ) -> Result<(usize, usize, usize)> {
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
    ) -> Result<(usize, usize, usize)> {
        let credential = self.mail_credential(account).await?;
        let endpoint = account.ews_url.clone().ok_or_else(|| {
            crate::Error::AccountConfig("для Exchange не настроен адрес EWS".into())
        })?;
        let username = account
            .username
            .clone()
            .unwrap_or_else(|| account.email.clone());
        let backend = EwsBackend { endpoint, username };
        let data = backend.auxiliary(&credential).await?;
        self.db
            .save_auxiliary_data(account.id, "exchange", &data)
            .await
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
        Self::mail_backend(&account)?
            .send(message, &credential)
            .await
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
        let token = self.mail_credential(account).await?;
        let backend = Self::mail_backend(account)?;
        let operations = self.db.claim_outbox_operations(account.id, 50).await?;
        let mut completed = 0;
        for operation in operations {
            let applied = if operation.op_kind == "send" {
                match serde_json::from_str::<crate::backend::OutgoingMessage>(&operation.payload) {
                    Ok(message) => backend.send(message, &token).await,
                    Err(error) => Err(crate::Error::Json(error)),
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
                        account = %account.email,
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

        // Код уже обменян и одноразовый, поэтому токен сначала надёжно
        // сохраняется. Проверки доступа быстрые; их временный сбой становится
        // предупреждением и не заставляет пользователя получать новый код.
        let (mail_access, dav_access) = tokio::join!(
            YandexBackend.validate(email, &access_token),
            validate_yandex_dav(email, &access_token)
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
        tracing::info!(email, scope = ?token.scope, "Gmail OAuth: провайдер вернул scope");
        if let Some(granted) = token.scope.as_deref() {
            let granted: std::collections::HashSet<_> = granted.split_whitespace().collect();
            let missing: Vec<_> = GOOGLE_SCOPES
                .split_whitespace()
                .filter(|scope| !granted.contains(scope))
                .collect();
            if !missing.is_empty() {
                tracing::warn!(email, missing = ?missing, "Gmail OAuth: Google выдал не все запрошенные scope");
                return Err(crate::Error::AccountConfig(format!(
                    "Google не выдал все разрешения truemail. Повторите подключение и подтвердите доступ к Gmail, Календарю, Контактам и Задачам. Не выданы: {}",
                    missing.join(", ")
                )));
            }
        } else {
            tracing::warn!(
                email,
                "Gmail OAuth: провайдер не вернул поле scope, проверку разрешений пропускаем"
            );
        }
        let access_token = Zeroizing::new(token.access_token.clone());
        let secret_ref = format!("google-oauth:{}", email.to_lowercase());
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
            .discover(&account.email, &access_token, &cursors)
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
                warnings.push(format!(
                    "Почта подключена, первая синхронизация отложена: {error}"
                ));
                0
            }
        };
        let auxiliary_cursors = self.db.auxiliary_sync_cursors(account.id).await?;
        let dav_result = match account.provider {
            Provider::Yandex => Some((
                "caldav",
                sync_yandex_dav(&account.email, &access_token, &auxiliary_cursors).await,
            )),
            Provider::Gmail => Some((
                "google",
                sync_google_services(&access_token, &auxiliary_cursors).await,
            )),
            _ => None,
        };
        let (calendars, events, contacts) = match dav_result {
            None => (0, 0, 0),
            Some((source_kind, Ok(dav))) => self
                .db
                .save_auxiliary_data(account.id, source_kind, &dav)
                .await
                .unwrap_or_else(|error| {
                    warnings.push(format!(
                        "Календарь и контакты подключены, но не сохранились: {error}"
                    ));
                    (0, 0, 0)
                }),
            Some((_, Err(error))) => {
                warnings.push(format!(
                    "Календарь и контакты: первая синхронизация отложена: {error}"
                ));
                (0, 0, 0)
            }
        };
        Ok(ConnectedAccountSync {
            account: account.clone(),
            mail_folders,
            calendars,
            events,
            contacts,
            warnings,
        })
    }
}
