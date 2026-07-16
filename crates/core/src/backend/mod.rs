//! Работающий транспортный слой Яндекс Почты через IMAP OAuth2.

mod gmail_api;
mod imap;
mod smtp;

pub use imap::{
    DiscoveredFolder, DiscoveredMessage, FolderSyncCursor, ImapDiscovery, apply_gmail_operation,
    apply_yandex_operation, discover_gmail, discover_gmail_folders, discover_gmail_inbox,
    discover_yandex, discover_yandex_folders, discover_yandex_inbox, validate_gmail,
    validate_yandex, wait_for_gmail_change, wait_for_yandex_change,
};
pub use smtp::{OutgoingAttachment, OutgoingMessage, send_gmail, send_yandex};

/// ID последних писем Gmail Входящих - для быстрых уведомлений о новой почте.
pub async fn gmail_latest_ids(access_token: &str, limit: u32) -> Result<Vec<String>> {
    gmail_api::latest_message_ids(access_token, limit).await
}

/// Одношаговая отписка (RFC 8058): POST на List-Unsubscribe URL с телом
/// "List-Unsubscribe=One-Click". Возвращает HTTP-код ответа сервера.
pub async fn unsubscribe_one_click(url: &str) -> Result<u16> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|error| crate::Error::Backend {
            backend: "unsubscribe".into(),
            message: error.to_string(),
        })?;
    let response = client
        .post(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body("List-Unsubscribe=One-Click")
        .send()
        .await
        .map_err(|error| crate::Error::Backend {
            backend: "unsubscribe".into(),
            message: error.to_string(),
        })?;
    Ok(response.status().as_u16())
}

use crate::Result;
use async_trait::async_trait;
use std::collections::HashMap;

/// Provider-neutral boundary used by account orchestration. A second provider
/// can implement this trait without teaching storage or the UI its protocol.
#[async_trait]
pub trait MailBackend: Send + Sync {
    fn provider_id(&self) -> &'static str;
    async fn validate(&self, email: &str, credential: &str) -> Result<()>;
    async fn discover(
        &self,
        email: &str,
        credential: &str,
        cursors: &HashMap<String, FolderSyncCursor>,
    ) -> Result<ImapDiscovery>;
    async fn discover_folders(
        &self,
        email: &str,
        credential: &str,
    ) -> Result<Vec<DiscoveredFolder>>;
    async fn discover_inbox(
        &self,
        email: &str,
        credential: &str,
        cursors: &HashMap<String, FolderSyncCursor>,
    ) -> Result<ImapDiscovery>;
    async fn apply_operation(
        &self,
        email: &str,
        credential: &str,
        operation: &str,
        payload: &str,
    ) -> Result<()>;
    async fn rename_folder(
        &self,
        email: &str,
        credential: &str,
        remote_path: &str,
        new_name: &str,
    ) -> Result<String>;
    async fn delete_folder(&self, email: &str, credential: &str, remote_path: &str) -> Result<()>;
    async fn wait_for_change(&self, email: &str, credential: &str) -> Result<()>;
    async fn send(&self, message: OutgoingMessage, credential: &str) -> Result<()>;
    /// Докачать сырой MIME письма с сервера (кэш вычищен по глубине хранения).
    async fn fetch_message_raw(
        &self,
        email: &str,
        credential: &str,
        folder_path: &str,
        uid: u32,
        remote_id: Option<&str>,
    ) -> Result<Vec<u8>>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct YandexBackend;

#[derive(Debug, Default, Clone, Copy)]
pub struct GmailBackend;

#[async_trait]
impl MailBackend for YandexBackend {
    fn provider_id(&self) -> &'static str {
        "yandex"
    }

    async fn validate(&self, email: &str, credential: &str) -> Result<()> {
        validate_yandex(email, credential).await
    }

    async fn discover(
        &self,
        email: &str,
        credential: &str,
        cursors: &HashMap<String, FolderSyncCursor>,
    ) -> Result<ImapDiscovery> {
        discover_yandex(email, credential, cursors).await
    }

    async fn discover_folders(
        &self,
        email: &str,
        credential: &str,
    ) -> Result<Vec<DiscoveredFolder>> {
        discover_yandex_folders(email, credential).await
    }

    async fn discover_inbox(
        &self,
        email: &str,
        credential: &str,
        cursors: &HashMap<String, FolderSyncCursor>,
    ) -> Result<ImapDiscovery> {
        discover_yandex_inbox(email, credential, cursors).await
    }

    async fn apply_operation(
        &self,
        email: &str,
        credential: &str,
        operation: &str,
        payload: &str,
    ) -> Result<()> {
        apply_yandex_operation(email, credential, operation, payload).await
    }

    async fn wait_for_change(&self, email: &str, credential: &str) -> Result<()> {
        wait_for_yandex_change(email, credential).await
    }

    async fn rename_folder(
        &self,
        email: &str,
        credential: &str,
        remote_path: &str,
        new_name: &str,
    ) -> Result<String> {
        imap::rename_oauth_folder("imap.yandex.ru", email, credential, remote_path, new_name).await
    }

    async fn delete_folder(&self, email: &str, credential: &str, remote_path: &str) -> Result<()> {
        imap::delete_oauth_folder("imap.yandex.ru", email, credential, remote_path).await
    }

    async fn send(&self, message: OutgoingMessage, credential: &str) -> Result<()> {
        send_yandex(message, credential).await
    }

    async fn fetch_message_raw(
        &self,
        email: &str,
        credential: &str,
        folder_path: &str,
        uid: u32,
        _remote_id: Option<&str>,
    ) -> Result<Vec<u8>> {
        imap::fetch_oauth_message_raw("imap.yandex.com", email, credential, folder_path, uid).await
    }
}

#[async_trait]
impl MailBackend for GmailBackend {
    fn provider_id(&self) -> &'static str {
        "gmail"
    }

    async fn validate(&self, email: &str, credential: &str) -> Result<()> {
        let _ = email;
        gmail_api::validate(credential).await
    }

    async fn discover(
        &self,
        email: &str,
        credential: &str,
        cursors: &HashMap<String, FolderSyncCursor>,
    ) -> Result<ImapDiscovery> {
        let _ = email;
        gmail_api::discover(credential, cursors).await
    }

    async fn discover_folders(
        &self,
        email: &str,
        credential: &str,
    ) -> Result<Vec<DiscoveredFolder>> {
        let _ = email;
        gmail_api::discover_folders(credential).await
    }

    async fn discover_inbox(
        &self,
        email: &str,
        credential: &str,
        cursors: &HashMap<String, FolderSyncCursor>,
    ) -> Result<ImapDiscovery> {
        let _ = email;
        gmail_api::discover(credential, cursors).await
    }

    async fn apply_operation(
        &self,
        email: &str,
        credential: &str,
        operation: &str,
        payload: &str,
    ) -> Result<()> {
        let _ = email;
        gmail_api::apply_operation(credential, operation, payload).await
    }

    async fn wait_for_change(&self, email: &str, credential: &str) -> Result<()> {
        let _ = (email, credential);
        // Gmail push требует серверного Cloud Pub/Sub webhook. Для desktop-only
        // клиента используем короткий REST polling, не зависящий от IMAP:993.
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        Ok(())
    }

    async fn rename_folder(
        &self,
        email: &str,
        credential: &str,
        remote_path: &str,
        new_name: &str,
    ) -> Result<String> {
        let _ = email;
        gmail_api::rename_label(credential, remote_path, new_name).await
    }

    async fn delete_folder(&self, email: &str, credential: &str, remote_path: &str) -> Result<()> {
        let _ = email;
        gmail_api::delete_label(credential, remote_path).await
    }

    async fn send(&self, message: OutgoingMessage, credential: &str) -> Result<()> {
        send_gmail(message, credential).await
    }

    async fn fetch_message_raw(
        &self,
        email: &str,
        credential: &str,
        _folder_path: &str,
        _uid: u32,
        remote_id: Option<&str>,
    ) -> Result<Vec<u8>> {
        let _ = email;
        let id = remote_id.ok_or_else(|| crate::Error::Backend {
            backend: "gmail-message".into(),
            message: "нет remote_id для докачки письма".into(),
        })?;
        gmail_api::fetch_message_raw(credential, id).await
    }
}
