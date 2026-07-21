//! Работающий транспортный слой Яндекс Почты через IMAP OAuth2.

mod ews;
mod gmail_api;
mod imap;
mod jmap;
mod smtp;

pub use ews::{EwsBackend, discover_ews_url};
pub use imap::{
    DiscoveredFlagUpdate, DiscoveredFolder, DiscoveredMessage, FolderSyncCursor, ImapDiscovery,
    apply_gmail_operation, apply_password_operation, apply_yandex_operation, discover_gmail,
    discover_gmail_folders, discover_gmail_inbox, discover_password, discover_password_folders,
    discover_password_inbox, discover_yandex, discover_yandex_folders, discover_yandex_inbox,
    validate_gmail, validate_password, validate_yandex, wait_for_gmail_change,
    wait_for_password_change, wait_for_yandex_change,
};
pub use jmap::{JmapBackend, probe_session_url as probe_jmap_session_url};
pub use smtp::{OutgoingAttachment, OutgoingMessage, send_gmail, send_password, send_yandex};

#[derive(Debug)]
pub enum SendOutcome {
    /// Provider API отправил письмо и сам сохранил серверную копию.
    SavedOnServer,
    /// SMTP доставил письмо; эти точные MIME-байты ещё нужно APPEND-ить в Sent.
    NeedsSentAppend(Vec<u8>),
}

/// ID последних писем Gmail Входящих - для быстрых уведомлений о новой почте.
pub async fn gmail_latest_ids(access_token: &str, limit: u32) -> Result<Vec<String>> {
    gmail_api::latest_message_ids(access_token, limit).await
}

/// Одношаговая отписка (RFC 8058): POST на List-Unsubscribe URL с телом
/// "List-Unsubscribe=One-Click". Возвращает HTTP-код ответа сервера.
///
/// URL приходит прямо из заголовка письма, то есть от произвольного
/// отправителя, а локальный HTTP API приложения слушает 127.0.0.1:34981 -
/// без проверок это SSRF: письмо может дёрнуть внутренний эндпоинт или
/// любую машину в приватной сети получателя.
pub async fn unsubscribe_one_click(url: &str) -> Result<u16> {
    let parsed = url::Url::parse(url).map_err(|error| crate::Error::Backend {
        backend: "unsubscribe".into(),
        message: format!("некорректный URL отписки: {error}"),
    })?;
    let checked = ensure_unsubscribe_target_is_public(&parsed).await?;
    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        // Иначе проверка хоста выше обходится тривиально: сервер письма
        // отвечает 302 на локальный адрес, и reqwest сам туда сходит.
        .redirect(reqwest::redirect::Policy::none());
    // Проверка адреса и установка соединения - два разных резолва, между
    // которыми DNS успевает ответить иначе (rebinding): проверили публичный
    // адрес, а подключились к 127.0.0.1. Прибиваем соединение к тому адресу,
    // который реально прошёл проверку. Для literal IP в URL это не нужно -
    // там резолва нет и подменять нечего.
    if let (Some(host), Some(addr)) = (parsed.host_str(), checked) {
        builder = builder.resolve(host, addr);
    }
    let client = builder.build().map_err(|error| crate::Error::Backend {
        backend: "unsubscribe".into(),
        message: error.to_string(),
    })?;
    let response = client
        .post(parsed)
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

/// Схема должна быть веб-ссылкой, а хост - не указывать на loopback,
/// link-local или приватную сеть. Хост в URL может быть как literal IP, так
/// и DNS-именем, поэтому имя резолвится и проверяются уже полученные
/// адреса; резолв идёт через tokio::net::lookup_host, а не через блокирующий
/// системный резолвер, чтобы не подвесить async runtime.
/// Возвращает проверенный адрес для DNS-имени (его и надо использовать при
/// соединении) либо None, если в URL был literal IP.
async fn ensure_unsubscribe_target_is_public(
    url: &url::Url,
) -> Result<Option<std::net::SocketAddr>> {
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(crate::Error::Backend {
            backend: "unsubscribe".into(),
            message: format!("схема \"{}\" недопустима для отписки", url.scheme()),
        });
    }
    let host = url.host_str().ok_or_else(|| crate::Error::Backend {
        backend: "unsubscribe".into(),
        message: "URL отписки без хоста".into(),
    })?;
    // Порт нужен и для lookup_host, и чтобы вернуть готовый SocketAddr для
    // привязки соединения.
    let port = url.port_or_known_default().unwrap_or(80);
    if let Ok(literal) = host.parse::<std::net::IpAddr>() {
        reject_if_disallowed(literal)?;
        return Ok(None);
    }
    let resolved = tokio::net::lookup_host((host, port))
        .await
        .map_err(|error| crate::Error::Backend {
            backend: "unsubscribe".into(),
            message: format!("не удалось разрешить хост отписки: {error}"),
        })?;
    // Проверяем все адреса, но соединяться будем строго по первому: если
    // хоть один из них запрещён, запрос не уходит вовсе.
    let mut first = None;
    for addr in resolved {
        reject_if_disallowed(addr.ip())?;
        if first.is_none() {
            first = Some(addr);
        }
    }
    first
        .ok_or_else(|| crate::Error::Backend {
            backend: "unsubscribe".into(),
            message: "хост отписки не резолвится ни в один адрес".into(),
        })
        .map(Some)
}

fn reject_if_disallowed(ip: std::net::IpAddr) -> Result<()> {
    if is_disallowed_unsubscribe_ip(ip) {
        return Err(crate::Error::Backend {
            backend: "unsubscribe".into(),
            message: format!("URL отписки указывает на недопустимый адрес {ip}"),
        });
    }
    Ok(())
}

/// Loopback, link-local, приватные диапазоны и unspecified-адрес запрещены
/// и для IPv4, и для IPv6 - в эти сети как раз и попадает локальный API
/// приложения и внутренняя сеть пользователя.
fn is_disallowed_unsubscribe_ip(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_multicast()
                || v4.is_broadcast()
        }
        IpAddr::V6(v6) => {
            // IPv4-mapped (::ffff:a.b.c.d) - проверяем как обычный IPv4.
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_disallowed_unsubscribe_ip(IpAddr::V4(mapped));
            }
            // fc00::/7 - Unique Local Address, IPv6-аналог приватных сетей;
            // is_unique_local() в std пока нестабилен, поэтому маска вручную.
            let unique_local = (v6.segments()[0] & 0xfe00) == 0xfc00;
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_unicast_link_local()
                || v6.is_multicast()
                || unique_local
        }
    }
}

use crate::Result;
use crate::model::{Security, ServerConfig};
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
        retention_days: i64,
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
    /// Создать папку. `parent_path` - remote_path родительской папки (None -
    /// создание на верхнем уровне). Возвращает remote_path новой папки.
    async fn create_folder(
        &self,
        email: &str,
        credential: &str,
        parent_path: Option<&str>,
        name: &str,
    ) -> Result<String>;
    async fn rename_folder(
        &self,
        email: &str,
        credential: &str,
        remote_path: &str,
        new_name: &str,
    ) -> Result<String>;
    async fn delete_folder(&self, email: &str, credential: &str, remote_path: &str) -> Result<()>;
    async fn wait_for_change(&self, email: &str, credential: &str) -> Result<()>;
    async fn send(&self, message: OutgoingMessage, credential: &str) -> Result<SendOutcome>;
    async fn append_sent(&self, email: &str, credential: &str, raw: &[u8]) -> Result<()> {
        let _ = (email, credential, raw);
        Err(crate::Error::AccountConfig(
            "транспорт не поддерживает отдельное сохранение в Отправленные".into(),
        ))
    }
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

#[derive(Debug, Default, Clone, Copy)]
pub struct OutlookBackend;

#[derive(Debug, Clone)]
pub struct GenericImapBackend {
    pub username: String,
    pub imap: ServerConfig,
    pub smtp: Option<ServerConfig>,
}

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
        retention_days: i64,
    ) -> Result<ImapDiscovery> {
        discover_yandex(email, credential, cursors, retention_days).await
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

    async fn create_folder(
        &self,
        email: &str,
        credential: &str,
        parent_path: Option<&str>,
        name: &str,
    ) -> Result<String> {
        imap::create_oauth_folder("imap.yandex.ru", email, credential, parent_path, name).await
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

    async fn send(&self, message: OutgoingMessage, credential: &str) -> Result<SendOutcome> {
        let raw =
            smtp::send_oauth_with_raw(message, credential, "smtp.yandex.com", 465, Security::Ssl)
                .await?;
        Ok(SendOutcome::NeedsSentAppend(raw))
    }

    async fn append_sent(&self, email: &str, credential: &str, raw: &[u8]) -> Result<()> {
        imap::append_oauth_sent("imap.yandex.com", email, credential, raw).await
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
        _retention_days: i64,
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

    async fn create_folder(
        &self,
        email: &str,
        credential: &str,
        parent_path: Option<&str>,
        name: &str,
    ) -> Result<String> {
        let _ = email;
        gmail_api::create_label(credential, parent_path, name).await
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

    async fn send(&self, message: OutgoingMessage, credential: &str) -> Result<SendOutcome> {
        send_gmail(message, credential).await?;
        Ok(SendOutcome::SavedOnServer)
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

#[async_trait]
impl MailBackend for OutlookBackend {
    fn provider_id(&self) -> &'static str {
        "outlook"
    }

    async fn validate(&self, email: &str, credential: &str) -> Result<()> {
        imap::validate_oauth("outlook.office365.com", email, credential).await
    }

    async fn discover(
        &self,
        email: &str,
        credential: &str,
        cursors: &HashMap<String, FolderSyncCursor>,
        retention_days: i64,
    ) -> Result<ImapDiscovery> {
        imap::discover_oauth(
            "outlook.office365.com",
            email,
            credential,
            cursors,
            retention_days,
        )
        .await
    }

    async fn discover_folders(
        &self,
        email: &str,
        credential: &str,
    ) -> Result<Vec<DiscoveredFolder>> {
        imap::discover_oauth_folders("outlook.office365.com", email, credential).await
    }

    async fn discover_inbox(
        &self,
        email: &str,
        credential: &str,
        cursors: &HashMap<String, FolderSyncCursor>,
    ) -> Result<ImapDiscovery> {
        imap::discover_oauth_inbox("outlook.office365.com", email, credential, cursors).await
    }

    async fn apply_operation(
        &self,
        email: &str,
        credential: &str,
        operation: &str,
        payload: &str,
    ) -> Result<()> {
        imap::apply_oauth_operation(
            "outlook.office365.com",
            email,
            credential,
            operation,
            payload,
        )
        .await
    }

    async fn create_folder(
        &self,
        email: &str,
        credential: &str,
        parent_path: Option<&str>,
        name: &str,
    ) -> Result<String> {
        imap::create_oauth_folder(
            "outlook.office365.com",
            email,
            credential,
            parent_path,
            name,
        )
        .await
    }

    async fn rename_folder(
        &self,
        email: &str,
        credential: &str,
        remote_path: &str,
        new_name: &str,
    ) -> Result<String> {
        imap::rename_oauth_folder(
            "outlook.office365.com",
            email,
            credential,
            remote_path,
            new_name,
        )
        .await
    }

    async fn delete_folder(&self, email: &str, credential: &str, remote_path: &str) -> Result<()> {
        imap::delete_oauth_folder("outlook.office365.com", email, credential, remote_path).await
    }

    async fn wait_for_change(&self, email: &str, credential: &str) -> Result<()> {
        imap::wait_for_oauth_change("outlook.office365.com", email, credential).await
    }

    async fn send(&self, message: OutgoingMessage, credential: &str) -> Result<SendOutcome> {
        let raw = smtp::send_oauth_with_raw(
            message,
            credential,
            "smtp.office365.com",
            587,
            Security::Starttls,
        )
        .await?;
        Ok(SendOutcome::NeedsSentAppend(raw))
    }

    async fn append_sent(&self, email: &str, credential: &str, raw: &[u8]) -> Result<()> {
        imap::append_oauth_sent("outlook.office365.com", email, credential, raw).await
    }

    async fn fetch_message_raw(
        &self,
        email: &str,
        credential: &str,
        folder_path: &str,
        uid: u32,
        _remote_id: Option<&str>,
    ) -> Result<Vec<u8>> {
        imap::fetch_oauth_message_raw("outlook.office365.com", email, credential, folder_path, uid)
            .await
    }
}

#[async_trait]
impl MailBackend for GenericImapBackend {
    fn provider_id(&self) -> &'static str {
        "generic-imap"
    }

    async fn validate(&self, _email: &str, credential: &str) -> Result<()> {
        validate_password(
            &self.imap.host,
            self.imap.port,
            self.imap.security,
            &self.username,
            credential,
        )
        .await
    }

    async fn discover(
        &self,
        _email: &str,
        credential: &str,
        cursors: &HashMap<String, FolderSyncCursor>,
        retention_days: i64,
    ) -> Result<ImapDiscovery> {
        discover_password(
            &self.imap.host,
            self.imap.port,
            self.imap.security,
            &self.username,
            credential,
            cursors,
            retention_days,
        )
        .await
    }

    async fn discover_folders(
        &self,
        _email: &str,
        credential: &str,
    ) -> Result<Vec<DiscoveredFolder>> {
        discover_password_folders(
            &self.imap.host,
            self.imap.port,
            self.imap.security,
            &self.username,
            credential,
        )
        .await
    }

    async fn discover_inbox(
        &self,
        _email: &str,
        credential: &str,
        cursors: &HashMap<String, FolderSyncCursor>,
    ) -> Result<ImapDiscovery> {
        discover_password_inbox(
            &self.imap.host,
            self.imap.port,
            self.imap.security,
            &self.username,
            credential,
            cursors,
        )
        .await
    }

    async fn apply_operation(
        &self,
        _email: &str,
        credential: &str,
        operation: &str,
        payload: &str,
    ) -> Result<()> {
        apply_password_operation(
            &self.imap.host,
            self.imap.port,
            self.imap.security,
            &self.username,
            credential,
            operation,
            payload,
        )
        .await
    }

    async fn create_folder(
        &self,
        _email: &str,
        credential: &str,
        parent_path: Option<&str>,
        name: &str,
    ) -> Result<String> {
        imap::create_password_folder(
            &self.imap.host,
            self.imap.port,
            self.imap.security,
            &self.username,
            credential,
            parent_path,
            name,
        )
        .await
    }

    async fn rename_folder(
        &self,
        _email: &str,
        credential: &str,
        remote_path: &str,
        new_name: &str,
    ) -> Result<String> {
        imap::rename_password_folder(
            &self.imap.host,
            self.imap.port,
            self.imap.security,
            &self.username,
            credential,
            remote_path,
            new_name,
        )
        .await
    }

    async fn delete_folder(&self, _email: &str, credential: &str, remote_path: &str) -> Result<()> {
        imap::delete_password_folder(
            &self.imap.host,
            self.imap.port,
            self.imap.security,
            &self.username,
            credential,
            remote_path,
        )
        .await
    }

    async fn wait_for_change(&self, _email: &str, credential: &str) -> Result<()> {
        wait_for_password_change(
            &self.imap.host,
            self.imap.port,
            self.imap.security,
            &self.username,
            credential,
        )
        .await
    }

    async fn send(&self, message: OutgoingMessage, credential: &str) -> Result<SendOutcome> {
        let smtp = self.smtp.as_ref().ok_or_else(|| {
            crate::Error::AccountConfig("для аккаунта не настроен SMTP-сервер".into())
        })?;
        let raw = smtp::send_password_with_raw(
            message,
            &self.username,
            credential,
            &smtp.host,
            smtp.port,
            smtp.security,
        )
        .await?;
        Ok(SendOutcome::NeedsSentAppend(raw))
    }

    async fn append_sent(&self, _email: &str, credential: &str, raw: &[u8]) -> Result<()> {
        imap::append_password_sent(
            &self.imap.host,
            self.imap.port,
            self.imap.security,
            &self.username,
            credential,
            raw,
        )
        .await
    }

    async fn fetch_message_raw(
        &self,
        _email: &str,
        credential: &str,
        folder_path: &str,
        uid: u32,
        _remote_id: Option<&str>,
    ) -> Result<Vec<u8>> {
        imap::fetch_password_message_raw(
            &self.imap.host,
            self.imap.port,
            self.imap.security,
            &self.username,
            credential,
            folder_path,
            uid,
        )
        .await
    }
}

#[cfg(test)]
mod unsubscribe_ssrf_tests {
    use super::*;

    #[test]
    fn blocks_ipv4_loopback_and_private_ranges() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.5",
            "192.168.1.1",
            "169.254.1.1",
            "0.0.0.0",
        ] {
            assert!(
                is_disallowed_unsubscribe_ip(ip.parse().unwrap()),
                "{ip} должен быть запрещён"
            );
        }
    }

    #[test]
    fn blocks_ipv6_loopback_link_local_and_unique_local() {
        for ip in ["::1", "::", "fe80::1", "fc00::1", "fd12:3456::1"] {
            assert!(
                is_disallowed_unsubscribe_ip(ip.parse().unwrap()),
                "{ip} должен быть запрещён"
            );
        }
    }

    #[test]
    fn blocks_ipv4_mapped_private_address() {
        // ::ffff:127.0.0.1 - тот же loopback, только в IPv6-обёртке.
        assert!(is_disallowed_unsubscribe_ip(
            "::ffff:127.0.0.1".parse().unwrap()
        ));
    }

    #[test]
    fn allows_ordinary_public_addresses() {
        assert!(!is_disallowed_unsubscribe_ip(
            "93.184.216.34".parse().unwrap()
        ));
        assert!(!is_disallowed_unsubscribe_ip(
            "2606:2800:220:1:248:1893:25c8:1946".parse().unwrap()
        ));
    }

    #[tokio::test]
    async fn rejects_non_http_scheme() {
        let url = url::Url::parse("file:///etc/passwd").unwrap();
        let error = ensure_unsubscribe_target_is_public(&url).await.unwrap_err();
        assert!(error.to_string().contains("схема"));
    }

    #[tokio::test]
    async fn rejects_literal_loopback_host() {
        let url = url::Url::parse("http://127.0.0.1:34981/unsubscribe").unwrap();
        assert!(ensure_unsubscribe_target_is_public(&url).await.is_err());
    }

    #[tokio::test]
    async fn rejects_literal_private_host() {
        let url = url::Url::parse("https://192.168.0.1/unsubscribe").unwrap();
        assert!(ensure_unsubscribe_target_is_public(&url).await.is_err());
    }

    #[tokio::test]
    async fn allows_ordinary_https_host() {
        // example.com зарезервирован IANA специально для документации и
        // тестов - не резолвится в приватный адрес и не бьёт по живому сервису.
        let url = url::Url::parse("https://example.com/unsubscribe").unwrap();
        assert!(ensure_unsubscribe_target_is_public(&url).await.is_ok());
    }
}
