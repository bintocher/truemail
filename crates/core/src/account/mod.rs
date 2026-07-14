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
pub use dav::{DavSyncResult, sync_yandex_dav, validate_yandex_dav};
pub use google_services::sync_google_services;
pub use oauth::{
    GOOGLE_SCOPES, OAuthToken, PkcePair, StoredOAuthCredential, YANDEX_SCOPES,
    configured_google_client_id, configured_google_client_secret, configured_yandex_client_id,
    exchange_google_code, exchange_yandex_code, generate_pkce, generate_state,
    google_authorize_url, refresh_google_token, refresh_yandex_token, yandex_authorize_url,
};

use crate::Result;
use crate::backend::{GmailBackend, MailBackend, YandexBackend};
use crate::model::{Account, AuthKind, BackendKind, NewAccount, Provider, Security, ServerConfig};
use crate::storage::Db;

pub struct AccountManager {
    db: Db,
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
        Self { db }
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
        let token = self.oauth_access_token(&account).await?;
        let backend = Self::mail_backend(account.provider)?;
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
        let token = self.oauth_access_token(&account).await?;
        let backend = Self::mail_backend(account.provider)?;
        backend
            .delete_folder(&account.email, &token, &folder.remote_path)
            .await?;
        self.db.delete_folder_local(folder.id).await
    }

    pub async fn list(&self) -> Result<Vec<Account>> {
        self.db.list_accounts().await
    }

    fn mail_backend(provider: Provider) -> Result<Box<dyn MailBackend>> {
        match provider {
            Provider::Yandex => Ok(Box::new(YandexBackend)),
            Provider::Gmail => Ok(Box::new(GmailBackend)),
            _ => Err(crate::Error::AccountConfig(
                "почтовый OAuth-транспорт для провайдера не настроен".into(),
            )),
        }
    }

    /// Прочитать сохранённый OAuth access token из системного keychain.
    pub async fn oauth_access_token(&self, account: &Account) -> Result<String> {
        let secret_ref = account
            .secret_ref
            .as_deref()
            .ok_or_else(|| crate::Error::AccountConfig("нет ссылки на OAuth-токен".into()))?;
        let entry = keyring::Entry::new("truemail", secret_ref)
            .map_err(|e| crate::Error::Keyring(e.to_string()))?;
        let serialized = entry
            .get_password()
            .map_err(|e| crate::Error::Keyring(e.to_string()))?;
        let mut credential: StoredOAuthCredential = serde_json::from_str(&serialized)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        if credential
            .expires_at
            .is_some_and(|expires| expires <= now + 60)
        {
            let refresh_token = credential.refresh_token.clone().ok_or_else(|| {
                crate::Error::AccountConfig("OAuth-токен истёк и не содержит refresh_token".into())
            })?;
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
                    let client_secret = configured_google_client_secret().ok_or_else(|| {
                        crate::Error::AccountConfig(
                            "для обновления OAuth-токена не задан TRUEMAIL_GOOGLE_CLIENT_SECRET"
                                .into(),
                        )
                    })?;
                    refresh_google_token(&client_id, &client_secret, &refresh_token).await?
                }
                _ => {
                    return Err(crate::Error::AccountConfig(
                        "обновление OAuth-токена для провайдера не настроено".into(),
                    ));
                }
            };
            let updated = StoredOAuthCredential::from_refresh(refreshed, refresh_token);
            entry
                .set_password(&serde_json::to_string(&updated)?)
                .map_err(|e| crate::Error::Keyring(e.to_string()))?;
            credential = updated;
        }
        Ok(credential.access_token)
    }

    /// Дозагрузить только последние входящие после события IMAP IDLE.
    pub async fn sync_mail_inbox(&self, account: &Account) -> Result<usize> {
        let access_token = self.oauth_access_token(account).await?;
        let backend = Self::mail_backend(account.provider)?;
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
            .save_discovered_messages(account.id, &discovery.messages)
            .await?;
        Ok(discovery.messages.len())
    }

    /// Обновить календарь и контакты, не запуская тяжёлую IMAP-синхронизацию.
    pub async fn sync_yandex_dav_account(
        &self,
        account: &Account,
    ) -> Result<(usize, usize, usize)> {
        let access_token = self.oauth_access_token(account).await?;
        let dav = sync_yandex_dav(&account.email, &access_token).await?;
        self.db.save_yandex_dav(account.id, &dav).await
    }

    /// Обновить Google Calendar, Contacts и Tasks отдельно от IMAP.
    pub async fn sync_google_auxiliary_account(
        &self,
        account: &Account,
    ) -> Result<(usize, usize, usize)> {
        let access_token = self.oauth_access_token(account).await?;
        let data = sync_google_services(&access_token).await?;
        self.db.save_google_services(account.id, &data).await
    }

    /// Доставить накопленные локальные операции с ограниченным retry/backoff.
    pub async fn process_mail_outbox(&self, account: &Account) -> Result<usize> {
        let token = self.oauth_access_token(account).await?;
        let backend = Self::mail_backend(account.provider)?;
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

    /// Сохранить авторизованный аккаунт Яндекса. OAuth-токены никогда не попадают в SQLite.
    pub async fn add_yandex_oauth(
        &self,
        email: &str,
        display_name: &str,
        token: OAuthToken,
    ) -> Result<ConnectedAccountSync> {
        let access_token = token.access_token.clone();
        let secret_ref = format!("yandex-oauth:{}", email.to_lowercase());
        let entry = keyring::Entry::new("truemail", &secret_ref)
            .map_err(|e| crate::Error::Keyring(e.to_string()))?;
        let credential = StoredOAuthCredential::from(token);
        let serialized = serde_json::to_string(&credential)?;
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
        if let Some(granted) = token.scope.as_deref() {
            let granted: std::collections::HashSet<_> = granted.split_whitespace().collect();
            let missing: Vec<_> = GOOGLE_SCOPES
                .split_whitespace()
                .filter(|scope| !granted.contains(scope))
                .collect();
            if !missing.is_empty() {
                return Err(crate::Error::AccountConfig(format!(
                    "Google не выдал все разрешения truemail. Повторите подключение и подтвердите доступ к Gmail, Календарю, Контактам и Задачам. Не выданы: {}",
                    missing.join(", ")
                )));
            }
        }
        let access_token = token.access_token.clone();
        let secret_ref = format!("google-oauth:{}", email.to_lowercase());
        let entry = keyring::Entry::new("truemail", &secret_ref)
            .map_err(|e| crate::Error::Keyring(e.to_string()))?;
        let credential = StoredOAuthCredential::from(token);
        entry
            .set_password(&serde_json::to_string(&credential)?)
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

    /// Полная синхронизация уже сохранённого аккаунта; предназначена для фоновой задачи.
    pub async fn sync_mail_account(&self, account: &Account) -> Result<ConnectedAccountSync> {
        let access_token = self.oauth_access_token(account).await?;
        let backend = Self::mail_backend(account.provider)?;
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
                                self.db
                                    .save_discovered_messages(account.id, &imap.messages)
                                    .await
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
        let dav_result = match account.provider {
            Provider::Yandex => Some((
                "caldav",
                sync_yandex_dav(&account.email, &access_token).await,
            )),
            Provider::Gmail => Some(("google", sync_google_services(&access_token).await)),
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
