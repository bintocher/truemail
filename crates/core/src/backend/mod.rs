//! Работающий транспортный слой Яндекс Почты через IMAP OAuth2.

mod imap;
mod smtp;

pub use imap::{
    DiscoveredFolder, DiscoveredMessage, FolderSyncCursor, ImapDiscovery, apply_gmail_operation,
    apply_yandex_operation, discover_gmail, discover_gmail_folders, discover_gmail_inbox,
    discover_yandex, discover_yandex_folders, discover_yandex_inbox, validate_gmail,
    validate_yandex, wait_for_gmail_change, wait_for_yandex_change,
};
pub use smtp::{OutgoingAttachment, OutgoingMessage, send_gmail, send_yandex};

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
    async fn wait_for_change(&self, email: &str, credential: &str) -> Result<()>;
    async fn send(&self, message: OutgoingMessage, credential: &str) -> Result<()>;
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

    async fn send(&self, message: OutgoingMessage, credential: &str) -> Result<()> {
        send_yandex(message, credential).await
    }
}

#[async_trait]
impl MailBackend for GmailBackend {
    fn provider_id(&self) -> &'static str {
        "gmail"
    }

    async fn validate(&self, email: &str, credential: &str) -> Result<()> {
        validate_gmail(email, credential).await
    }

    async fn discover(
        &self,
        email: &str,
        credential: &str,
        cursors: &HashMap<String, FolderSyncCursor>,
    ) -> Result<ImapDiscovery> {
        discover_gmail(email, credential, cursors).await
    }

    async fn discover_folders(
        &self,
        email: &str,
        credential: &str,
    ) -> Result<Vec<DiscoveredFolder>> {
        discover_gmail_folders(email, credential).await
    }

    async fn discover_inbox(
        &self,
        email: &str,
        credential: &str,
        cursors: &HashMap<String, FolderSyncCursor>,
    ) -> Result<ImapDiscovery> {
        discover_gmail_inbox(email, credential, cursors).await
    }

    async fn apply_operation(
        &self,
        email: &str,
        credential: &str,
        operation: &str,
        payload: &str,
    ) -> Result<()> {
        apply_gmail_operation(email, credential, operation, payload).await
    }

    async fn wait_for_change(&self, email: &str, credential: &str) -> Result<()> {
        wait_for_gmail_change(email, credential).await
    }

    async fn send(&self, message: OutgoingMessage, credential: &str) -> Result<()> {
        send_gmail(message, credential).await
    }
}
