//! Менеджер аккаунтов и автоконфигурация провайдеров.

mod autoconfig;
mod dav;
mod oauth;
pub use autoconfig::{ProviderConfig, autoconfig};
pub use dav::{DavSyncResult, sync_yandex_dav, validate_yandex_dav};
pub use oauth::{
    OAuthToken, PkcePair, StoredOAuthCredential, YANDEX_SCOPES, exchange_yandex_code,
    generate_pkce, generate_state, refresh_yandex_token, yandex_authorize_url,
};

use crate::Result;
use crate::backend::{MailBackend, YandexBackend};
use crate::model::{Account, AuthKind, BackendKind, NewAccount, Provider, Security, ServerConfig};
use crate::storage::Db;

pub struct AccountManager {
    db: Db,
}

#[derive(Debug)]
pub struct ConnectedYandex {
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

    pub async fn list(&self) -> Result<Vec<Account>> {
        self.db.list_accounts().await
    }

    /// Прочитать сохранённый OAuth access token из системного keychain.
    pub async fn yandex_access_token(&self, account: &Account) -> Result<String> {
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
            let client_id = std::env::var("TRUEMAIL_YANDEX_CLIENT_ID")
                .ok()
                .or_else(|| option_env!("TRUEMAIL_YANDEX_CLIENT_ID").map(str::to_owned))
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    crate::Error::AccountConfig(
                        "для обновления OAuth-токена не задан TRUEMAIL_YANDEX_CLIENT_ID".into(),
                    )
                })?;
            let refreshed = refresh_yandex_token(&client_id, &refresh_token).await?;
            let mut updated = StoredOAuthCredential::from(refreshed);
            if updated.refresh_token.is_none() {
                updated.refresh_token = Some(refresh_token);
            }
            entry
                .set_password(&serde_json::to_string(&updated)?)
                .map_err(|e| crate::Error::Keyring(e.to_string()))?;
            credential = updated;
        }
        Ok(credential.access_token)
    }

    /// Дозагрузить только последние входящие после события IMAP IDLE.
    pub async fn sync_yandex_inbox(&self, account: &Account) -> Result<usize> {
        let access_token = self.yandex_access_token(account).await?;
        let cursors = self.db.folder_sync_cursors(account.id).await?;
        let discovery = YandexBackend
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
        let access_token = self.yandex_access_token(account).await?;
        let dav = sync_yandex_dav(&account.email, &access_token).await?;
        self.db.save_yandex_dav(account.id, &dav).await
    }

    /// Доставить накопленные локальные операции с ограниченным retry/backoff.
    pub async fn process_yandex_outbox(&self, account: &Account) -> Result<usize> {
        let token = self.yandex_access_token(account).await?;
        let operations = self.db.claim_outbox_operations(account.id, 50).await?;
        let mut completed = 0;
        for operation in operations {
            let applied = if operation.op_kind == "send" {
                match serde_json::from_str::<crate::backend::OutgoingMessage>(&operation.payload) {
                    Ok(message) => YandexBackend.send(message, &token).await,
                    Err(error) => Err(crate::Error::Json(error)),
                }
            } else {
                YandexBackend
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
    ) -> Result<ConnectedYandex> {
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

        Ok(ConnectedYandex {
            account,
            mail_folders: 0,
            calendars: 0,
            events: 0,
            contacts: 0,
            warnings,
        })
    }

    /// Полная синхронизация уже сохранённого аккаунта; предназначена для фоновой задачи.
    pub async fn sync_yandex_account(&self, account: &Account) -> Result<ConnectedYandex> {
        let access_token = self.yandex_access_token(account).await?;
        let cursors = self.db.folder_sync_cursors(account.id).await?;
        let mut warnings = Vec::new();
        // Имена и счётчики папок появляются в UI сразу, пока тела писем и DAV
        // коллекции загружаются параллельно.
        if let Ok(folders) = YandexBackend
            .discover_folders(&account.email, &access_token)
            .await
            && let Err(error) = self.db.save_discovered_folders(account.id, &folders).await
        {
            warnings.push(format!("Папки почты не сохранились: {error}"));
        }
        let (imap_result, dav_result) = tokio::join!(
            YandexBackend.discover(&account.email, &access_token, &cursors),
            sync_yandex_dav(&account.email, &access_token)
        );
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
        let (calendars, events, contacts) = match dav_result {
            Ok(dav) => self
                .db
                .save_yandex_dav(account.id, &dav)
                .await
                .unwrap_or_else(|error| {
                    warnings.push(format!(
                        "Календарь и контакты подключены, но не сохранились: {error}"
                    ));
                    (0, 0, 0)
                }),
            Err(error) => {
                warnings.push(format!(
                    "Календарь и контакты: первая синхронизация отложена: {error}"
                ));
                (0, 0, 0)
            }
        };
        Ok(ConnectedYandex {
            account: account.clone(),
            mail_folders,
            calendars,
            events,
            contacts,
            warnings,
        })
    }
}
