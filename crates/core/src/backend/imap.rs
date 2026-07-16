//! Синхронизация почты через IMAP с OAuth2.

use crate::model::{FolderRole, infer_folder_role};
use crate::{Error, Result};
use async_imap::{Authenticator, types::Name, types::NameAttribute};
use futures::TryStreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_rustls::{
    TlsConnector,
    rustls::{ClientConfig, RootCertStore, pki_types::ServerName},
};

#[derive(Debug, Clone)]
pub struct DiscoveredFolder {
    pub remote_path: String,
    pub display_name: String,
    pub role: Option<FolderRole>,
    pub unread_count: i64,
    pub total_count: i64,
    pub uidvalidity: Option<u32>,
    pub uidnext: Option<u32>,
    pub highestmodseq: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct FolderSyncCursor {
    pub uidvalidity: Option<u32>,
    pub last_uid: Option<u32>,
}

#[derive(Debug)]
pub struct DiscoveredMessage {
    pub folder_path: String,
    pub uid: u32,
    pub remote_id: Option<String>,
    pub size: Option<u32>,
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
    pub draft: bool,
    pub raw: Vec<u8>,
}

type OAuthSession = async_imap::Session<tokio_rustls::client::TlsStream<TcpStream>>;
const MESSAGE_FETCH_ITEMS: &str = "(UID BODY.PEEK[] FLAGS RFC822.SIZE)";

#[derive(Debug)]
pub struct ImapDiscovery {
    pub folders: Vec<DiscoveredFolder>,
    pub messages: Vec<DiscoveredMessage>,
    pub server_uids: Vec<(String, Vec<u32>)>,
    pub reset_folders: Vec<String>,
}

struct OAuth2<'a> {
    email: &'a str,
    access_token: &'a str,
}

impl Authenticator for OAuth2<'_> {
    type Response = Vec<u8>;

    fn process(&mut self, _challenge: &[u8]) -> Self::Response {
        format!(
            "user={}\u{1}auth=Bearer {}\u{1}\u{1}",
            self.email, self.access_token
        )
        .into_bytes()
    }
}

/// TLS-конфиг строим один раз и переиспользуем: системные корневые сертификаты
/// не меняются в рамках сессии, а грузились они при каждом IMAP-подключении
/// (десятки раз в минуту из-за IDLE/поллинга) - это было и дорого, и шумело в лог.
fn tls_client_config() -> Arc<ClientConfig> {
    static CONFIG: std::sync::OnceLock<Arc<ClientConfig>> = std::sync::OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let native = rustls_native_certs::load_native_certs();
            for error in native.errors {
                tracing::warn!(%error, "не удалось загрузить часть системных TLS-сертификатов");
            }
            let mut roots = RootCertStore::empty();
            let (added, ignored) = roots.add_parsable_certificates(native.certs);
            tracing::debug!(added, ignored, "загружены системные TLS-сертификаты");
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            Arc::new(
                ClientConfig::builder()
                    .with_root_certificates(roots)
                    .with_no_client_auth(),
            )
        })
        .clone()
}

async fn connect_oauth(host: &str, email: &str, access_token: &str) -> Result<OAuthSession> {
    let tcp = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        TcpStream::connect((host, 993)),
    )
    .await
    .map_err(|_| Error::Backend {
        backend: "imap".into(),
        message: "тайм-аут подключения".into(),
    })??;
    // TCP keepalive держит канал живым на уровне ОС, чтобы простаивающее IDLE
    // не закрывалось промежуточным NAT по таймауту неактивности.
    {
        let keepalive = socket2::TcpKeepalive::new()
            .with_time(std::time::Duration::from_secs(45))
            .with_interval(std::time::Duration::from_secs(15));
        if let Err(error) = socket2::SockRef::from(&tcp).set_tcp_keepalive(&keepalive) {
            tracing::debug!(%error, "не удалось включить TCP keepalive для IMAP");
        }
    }
    let config = tls_client_config();
    let server_name = ServerName::try_from(host.to_owned()).map_err(|e| Error::Backend {
        backend: "imap".into(),
        message: e.to_string(),
    })?;
    let tls = TlsConnector::from(config)
        .connect(server_name, tcp)
        .await
        .map_err(|e| Error::Backend {
            backend: "imap-tls".into(),
            message: e.to_string(),
        })?;
    let mut client = async_imap::Client::new(tls);
    client
        .read_response()
        .await
        .map_err(|e| Error::Backend {
            backend: "imap".into(),
            message: e.to_string(),
        })?
        .ok_or_else(|| Error::Backend {
            backend: "imap".into(),
            message: "сервер закрыл соединение".into(),
        })?;
    let auth = OAuth2 {
        email,
        access_token,
    };
    let session = client
        .authenticate("XOAUTH2", auth)
        .await
        .map_err(|(e, _)| Error::Backend {
            backend: "imap-auth".into(),
            message: e.to_string(),
        })?;
    Ok(session)
}

pub(crate) async fn rename_oauth_folder(
    host: &str,
    email: &str,
    access_token: &str,
    remote_path: &str,
    new_name: &str,
) -> Result<String> {
    let name = new_name.trim();
    if name.is_empty() || name.contains(['/', '|']) {
        return Err(Error::AccountConfig(
            "имя папки пустое или содержит разделитель".into(),
        ));
    }
    let prefix_len = remote_path.rfind(['/', '|']).map_or(0, |index| index + 1);
    let target = format!(
        "{}{}",
        &remote_path[..prefix_len],
        encode_modified_utf7(name)
    );
    let mut session = connect_oauth(host, email, access_token).await?;
    session
        .rename(remote_path, &target)
        .await
        .map_err(|error| Error::Backend {
            backend: "imap-rename".into(),
            message: error.to_string(),
        })?;
    let _ = session.logout().await;
    Ok(target)
}

pub(crate) async fn delete_oauth_folder(
    host: &str,
    email: &str,
    access_token: &str,
    remote_path: &str,
) -> Result<()> {
    let mut session = connect_oauth(host, email, access_token).await?;
    tracing::info!(remote_path, "imap-delete: запрос удаления папки");

    // Шаг 1: узнаём разделитель иерархии для этой папки (у Яндекса это '|').
    let self_entries: Vec<Name> = session
        .list(Some(""), Some(remote_path))
        .await
        .map_err(|error| Error::Backend {
            backend: "imap-delete-list".into(),
            message: error.to_string(),
        })?
        .try_collect()
        .await
        .map_err(|error| Error::Backend {
            backend: "imap-delete-list".into(),
            message: error.to_string(),
        })?;
    for entry in &self_entries {
        tracing::debug!(
            name = entry.name(),
            delimiter = entry.delimiter().unwrap_or("?"),
            attributes = ?entry.attributes(),
            "imap-delete: цель"
        );
    }
    if self_entries.is_empty() {
        tracing::warn!(remote_path, "imap-delete: папка не найдена на сервере (LIST пуст)");
    }
    let delimiter = self_entries
        .iter()
        .find_map(|entry| entry.delimiter())
        .unwrap_or("|")
        .to_string();

    // Шаг 2: ищем подпапки. Яндекс отказывает в DELETE, если у папки есть дети.
    let child_pattern = format!("{remote_path}{delimiter}*");
    let children: Vec<Name> = session
        .list(Some(""), Some(&child_pattern))
        .await
        .map_err(|error| Error::Backend {
            backend: "imap-delete-list".into(),
            message: error.to_string(),
        })?
        .try_collect()
        .await
        .map_err(|error| Error::Backend {
            backend: "imap-delete-list".into(),
            message: error.to_string(),
        })?;
    let child_names: Vec<&str> = children.iter().map(Name::name).collect();
    tracing::info!(
        remote_path,
        delimiter = delimiter.as_str(),
        child_count = children.len(),
        children = ?child_names,
        "imap-delete: проверка подпапок перед удалением"
    );
    if !children.is_empty() {
        let _ = session.logout().await;
        return Err(Error::AccountConfig(format!(
            "у папки есть вложенные подпапки ({}), сначала удалите или перенесите их: {}",
            children.len(),
            child_names.join(", ")
        )));
    }

    // Шаг 3: собственно удаление.
    let result = session.delete(remote_path).await;
    match &result {
        Ok(()) => tracing::info!(remote_path, "imap-delete: папка удалена"),
        Err(error) => tracing::error!(
            remote_path,
            error = %error,
            "imap-delete: сервер отклонил DELETE"
        ),
    }
    let _ = session.logout().await;
    result.map_err(|error| Error::Backend {
        backend: "imap-delete-folder".into(),
        message: error.to_string(),
    })?;
    Ok(())
}

/// Быстрая проверка токена без скачивания почты.
pub async fn validate_oauth(host: &str, email: &str, access_token: &str) -> Result<()> {
    let mut session = connect_oauth(host, email, access_token).await?;
    session.noop().await.map_err(|e| Error::Backend {
        backend: "imap-auth".into(),
        message: e.to_string(),
    })?;
    let _ = session.logout().await;
    Ok(())
}

pub async fn validate_yandex(email: &str, access_token: &str) -> Result<()> {
    validate_oauth("imap.yandex.com", email, access_token).await
}

pub async fn validate_gmail(email: &str, access_token: &str) -> Result<()> {
    validate_oauth("imap.gmail.com", email, access_token).await
}

/// Применить одну локально поставленную в очередь операцию к IMAP-серверу.
/// Payload содержит полный устойчивый адрес письма: mailbox + UID.
pub async fn apply_oauth_operation(
    host: &str,
    email: &str,
    access_token: &str,
    op_kind: &str,
    payload: &str,
) -> Result<()> {
    let payload: serde_json::Value = serde_json::from_str(payload)?;
    let folder = payload["folder_path"]
        .as_str()
        .ok_or_else(|| Error::AccountConfig("outbox: нет folder_path".into()))?;
    let uid = payload["uid"]
        .as_u64()
        .ok_or_else(|| Error::AccountConfig("outbox: нет uid".into()))?;
    let mut session = connect_oauth(host, email, access_token).await?;
    session
        .select(folder)
        .await
        .map_err(|error| Error::Backend {
            backend: "imap-outbox".into(),
            message: format!("{folder}: {error}"),
        })?;
    match op_kind {
        "flag" => {
            let seen = payload["seen"]
                .as_bool()
                .ok_or_else(|| Error::AccountConfig("outbox: нет seen".into()))?;
            let command = if seen {
                "+FLAGS.SILENT (\\Seen)"
            } else {
                "-FLAGS.SILENT (\\Seen)"
            };
            session
                .uid_store(uid.to_string(), command)
                .await
                .map_err(imap_outbox_error)?
                .try_collect::<Vec<_>>()
                .await
                .map_err(imap_outbox_error)?;
        }
        "move" => {
            let target = payload["target_folder_path"]
                .as_str()
                .ok_or_else(|| Error::AccountConfig("outbox: нет target_folder_path".into()))?;
            let capabilities = session.capabilities().await.map_err(imap_outbox_error)?;
            if capabilities.has_str("MOVE") {
                session
                    .uid_mv(uid.to_string(), target)
                    .await
                    .map_err(imap_outbox_error)?;
            } else {
                session
                    .uid_copy(uid.to_string(), target)
                    .await
                    .map_err(imap_outbox_error)?;
                mark_deleted(&mut session, uid).await?;
            }
        }
        "delete" => mark_deleted(&mut session, uid).await?,
        other => {
            return Err(Error::AccountConfig(format!(
                "outbox: неизвестная операция {other}"
            )));
        }
    }
    let _ = session.logout().await;
    Ok(())
}

pub async fn apply_yandex_operation(
    email: &str,
    access_token: &str,
    op_kind: &str,
    payload: &str,
) -> Result<()> {
    apply_oauth_operation("imap.yandex.com", email, access_token, op_kind, payload).await
}

pub async fn apply_gmail_operation(
    email: &str,
    access_token: &str,
    op_kind: &str,
    payload: &str,
) -> Result<()> {
    apply_oauth_operation("imap.gmail.com", email, access_token, op_kind, payload).await
}

fn imap_outbox_error(error: async_imap::error::Error) -> Error {
    Error::Backend {
        backend: "imap-outbox".into(),
        message: error.to_string(),
    }
}

async fn mark_deleted(session: &mut OAuthSession, uid: u64) -> Result<()> {
    session
        .uid_store(uid.to_string(), "+FLAGS.SILENT (\\Deleted)")
        .await
        .map_err(imap_outbox_error)?
        .try_collect::<Vec<_>>()
        .await
        .map_err(imap_outbox_error)?;
    session
        .uid_expunge(uid.to_string())
        .await
        .map_err(imap_outbox_error)?
        .try_collect::<Vec<_>>()
        .await
        .map_err(imap_outbox_error)?;
    Ok(())
}

async fn list_oauth_folders(session: &mut OAuthSession) -> Result<Vec<DiscoveredFolder>> {
    let names = session
        .list(Some(""), Some("*"))
        .await
        .map_err(|e| Error::Backend {
            backend: "imap-list".into(),
            message: e.to_string(),
        })?
        .map_ok(|name| {
            let role = if name.name().eq_ignore_ascii_case("INBOX") {
                Some(FolderRole::Inbox)
            } else if name
                .attributes()
                .iter()
                .any(|a| matches!(a, NameAttribute::Sent))
            {
                Some(FolderRole::Sent)
            } else if name
                .attributes()
                .iter()
                .any(|a| matches!(a, NameAttribute::Drafts))
            {
                Some(FolderRole::Drafts)
            } else if name
                .attributes()
                .iter()
                .any(|a| matches!(a, NameAttribute::Junk))
            {
                Some(FolderRole::Spam)
            } else if name
                .attributes()
                .iter()
                .any(|a| matches!(a, NameAttribute::Trash))
            {
                Some(FolderRole::Trash)
            } else if name
                .attributes()
                .iter()
                .any(|a| matches!(a, NameAttribute::Archive | NameAttribute::All))
            {
                Some(FolderRole::Archive)
            } else {
                None
            };
            (name.name().to_owned(), role)
        })
        .try_collect::<Vec<_>>()
        .await
        .map_err(|e| Error::Backend {
            backend: "imap-list".into(),
            message: e.to_string(),
        })?;

    let mut folders = Vec::with_capacity(names.len());
    for (remote_path, role) in names {
        let status = session
            .status(&remote_path, "(MESSAGES UNSEEN)")
            .await
            .map_err(|e| Error::Backend {
                backend: "imap-status".into(),
                message: format!("{remote_path}: {e}"),
            })?;
        let encoded_name = remote_path
            .rsplit(['/', '|'])
            .next()
            .unwrap_or(&remote_path)
            .to_owned();
        let display_name = decode_modified_utf7(&encoded_name).unwrap_or(encoded_name);
        let role = role.or_else(|| infer_folder_role(&remote_path, &display_name));
        folders.push(DiscoveredFolder {
            remote_path,
            display_name,
            role,
            unread_count: status.unseen.unwrap_or(0) as i64,
            total_count: status.exists as i64,
            uidvalidity: None,
            uidnext: None,
            highestmodseq: None,
        });
    }
    Ok(folders)
}

/// Быстро получить папки и счётчики. Используется до тяжёлой загрузки писем.
pub async fn discover_oauth_folders(
    host: &str,
    email: &str,
    access_token: &str,
) -> Result<Vec<DiscoveredFolder>> {
    let mut session = connect_oauth(host, email, access_token).await?;
    let folders = list_oauth_folders(&mut session).await?;
    let _ = session.logout().await;
    Ok(folders)
}

pub async fn discover_yandex_folders(
    email: &str,
    access_token: &str,
) -> Result<Vec<DiscoveredFolder>> {
    discover_oauth_folders("imap.yandex.com", email, access_token).await
}

pub async fn discover_gmail_folders(
    email: &str,
    access_token: &str,
) -> Result<Vec<DiscoveredFolder>> {
    discover_oauth_folders("imap.gmail.com", email, access_token).await
}

async fn fetch_incremental_messages(
    session: &mut OAuthSession,
    folder: &mut DiscoveredFolder,
    cursor: Option<&FolderSyncCursor>,
    limit: usize,
) -> Result<(Vec<DiscoveredMessage>, Vec<u32>, bool)> {
    if folder.total_count == 0 {
        return Ok((Vec::new(), Vec::new(), false));
    }
    let mailbox = session
        .select(&folder.remote_path)
        .await
        .map_err(|error| Error::Backend {
            backend: "imap-select".into(),
            message: format!("{}: {error}", folder.remote_path),
        })?;
    folder.uidvalidity = mailbox.uid_validity;
    folder.uidnext = mailbox.uid_next;
    folder.highestmodseq = mailbox.highest_modseq;
    let mut uids: Vec<_> = session
        .uid_search("ALL")
        .await
        .map_err(|error| Error::Backend {
            backend: "imap-search".into(),
            message: format!("{}: {error}", folder.remote_path),
        })?
        .into_iter()
        .collect();
    uids.sort_unstable();
    let uidvalidity_changed = cursor.is_some_and(|cursor| {
        cursor.uidvalidity.is_some()
            && mailbox.uid_validity.is_some()
            && cursor.uidvalidity != mailbox.uid_validity
    });
    let selected =
        if !uidvalidity_changed && let Some(last_uid) = cursor.and_then(|cursor| cursor.last_uid) {
            uids.iter()
                .copied()
                .filter(|uid| *uid > last_uid)
                .take(limit)
                .collect::<Vec<_>>()
        } else {
            uids[uids.len().saturating_sub(limit)..].to_vec()
        };
    if selected.is_empty() {
        return Ok((Vec::new(), uids, uidvalidity_changed));
    }
    let set = selected
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let fetched = session
        // BODY.PEEK[] принципиален: обычный RFC822/BODY[] устанавливает
        // серверный флаг \\Seen и портит состояние ящика при синхронизации.
        .uid_fetch(set, MESSAGE_FETCH_ITEMS)
        .await
        .map_err(|e| Error::Backend {
            backend: "imap-fetch".into(),
            message: e.to_string(),
        })?
        .try_collect::<Vec<_>>()
        .await
        .map_err(|e| Error::Backend {
            backend: "imap-fetch".into(),
            message: e.to_string(),
        })?;
    let mut messages = Vec::with_capacity(fetched.len());
    for fetch in fetched {
        let Some(uid) = fetch.uid else { continue };
        let Some(raw) = fetch.body() else { continue };
        let flags: Vec<_> = fetch.flags().collect();
        messages.push(DiscoveredMessage {
            folder_path: folder.remote_path.clone(),
            uid,
            remote_id: None,
            size: fetch.size,
            seen: flags
                .iter()
                .any(|flag| matches!(flag, async_imap::types::Flag::Seen)),
            flagged: flags
                .iter()
                .any(|flag| matches!(flag, async_imap::types::Flag::Flagged)),
            answered: flags
                .iter()
                .any(|flag| matches!(flag, async_imap::types::Flag::Answered)),
            draft: flags
                .iter()
                .any(|flag| matches!(flag, async_imap::types::Flag::Draft)),
            raw: raw.to_vec(),
        });
    }
    Ok((messages, uids, uidvalidity_changed))
}

/// Быстрая дозагрузка входящих после IMAP IDLE-события.
pub async fn discover_oauth_inbox(
    host: &str,
    email: &str,
    access_token: &str,
    cursors: &HashMap<String, FolderSyncCursor>,
) -> Result<ImapDiscovery> {
    let mut session = connect_oauth(host, email, access_token).await?;
    let mut folders = list_oauth_folders(&mut session).await?;
    let inbox = folders
        .iter_mut()
        .find(|folder| folder.role == Some(FolderRole::Inbox))
        .ok_or_else(|| Error::Backend {
            backend: "imap-list".into(),
            message: "папка INBOX не найдена".into(),
        })?;
    let path = inbox.remote_path.clone();
    let (messages, uids, reset) =
        fetch_incremental_messages(&mut session, inbox, cursors.get(&path), 500).await?;
    let _ = session.logout().await;
    Ok(ImapDiscovery {
        folders,
        messages,
        server_uids: vec![(path.clone(), uids)],
        reset_folders: reset.then_some(path).into_iter().collect(),
    })
}

pub async fn discover_yandex_inbox(
    email: &str,
    access_token: &str,
    cursors: &HashMap<String, FolderSyncCursor>,
) -> Result<ImapDiscovery> {
    discover_oauth_inbox("imap.yandex.com", email, access_token, cursors).await
}

pub async fn discover_gmail_inbox(
    email: &str,
    access_token: &str,
    cursors: &HashMap<String, FolderSyncCursor>,
) -> Result<ImapDiscovery> {
    discover_oauth_inbox("imap.gmail.com", email, access_token, cursors).await
}

/// Держать отдельное IDLE-соединение до первого изменения INBOX.
pub async fn wait_for_oauth_change(host: &str, email: &str, access_token: &str) -> Result<()> {
    let mut session = connect_oauth(host, email, access_token).await?;
    let capabilities = session
        .capabilities()
        .await
        .map_err(|error| Error::Backend {
            backend: "imap-capability".into(),
            message: error.to_string(),
        })?;
    if !capabilities.has_str("IDLE") {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        return Ok(());
    }
    session
        .select("INBOX")
        .await
        .map_err(|error| Error::Backend {
            backend: "imap-select".into(),
            message: error.to_string(),
        })?;
    let mut idle = session.idle();
    idle.init().await.map_err(|error| Error::Backend {
        backend: "imap-idle".into(),
        message: error.to_string(),
    })?;
    let outcome = {
        let (wait, interrupt) = idle.wait();
        // Плановая переустановка IDLE раз в ~90 секунд. Яндекс/промежуточный узел
        // рвут простаивающее соединение уже через ~2 минуты (os error 10054 /
        // peer closed without close_notify). Переустанавливая IDLE проактивно и
        // чаще, чем сервер закрывает, мы держим канал активным и делаем цикл
        // штатным, а не гонкой "ждём, пока оборвут".
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(90), wait).await;
        drop(interrupt);
        outcome
    };
    let _ = idle.done().await;
    match outcome {
        // Таймаут: событий не было, тихо переустанавливаем IDLE в watcher.
        Err(_elapsed) => Ok(()),
        Ok(response) => response.map(|_| ()).map_err(|error| Error::Backend {
            backend: "imap-idle".into(),
            message: error.to_string(),
        }),
    }
}

pub async fn wait_for_yandex_change(email: &str, access_token: &str) -> Result<()> {
    wait_for_oauth_change("imap.yandex.com", email, access_token).await
}

pub async fn wait_for_gmail_change(email: &str, access_token: &str) -> Result<()> {
    wait_for_oauth_change("imap.gmail.com", email, access_token).await
}

pub async fn discover_oauth(
    host: &str,
    email: &str,
    access_token: &str,
    cursors: &HashMap<String, FolderSyncCursor>,
) -> Result<ImapDiscovery> {
    let mut session = connect_oauth(host, email, access_token).await?;
    let mut folders = list_oauth_folders(&mut session).await?;
    let mut messages = Vec::new();
    let mut server_uids = Vec::new();
    let mut reset_folders = Vec::new();
    for folder in &mut folders {
        let path = folder.remote_path.clone();
        match fetch_incremental_messages(&mut session, folder, cursors.get(&path), 500).await {
            Ok((mut folder_messages, uids, reset)) => {
                messages.append(&mut folder_messages);
                server_uids.push((path.clone(), uids));
                if reset {
                    reset_folders.push(path);
                }
            }
            Err(error) => {
                tracing::warn!(folder = %folder.remote_path, %error, "IMAP: папка пропущена");
            }
        }
    }
    let _ = session.logout().await;
    Ok(ImapDiscovery {
        folders,
        messages,
        server_uids,
        reset_folders,
    })
}

pub async fn discover_yandex(
    email: &str,
    access_token: &str,
    cursors: &HashMap<String, FolderSyncCursor>,
) -> Result<ImapDiscovery> {
    discover_oauth("imap.yandex.com", email, access_token, cursors).await
}

pub async fn discover_gmail(
    email: &str,
    access_token: &str,
    cursors: &HashMap<String, FolderSyncCursor>,
) -> Result<ImapDiscovery> {
    discover_oauth("imap.gmail.com", email, access_token, cursors).await
}

/// Докачать сырой MIME одного письма по UID из конкретной папки. Нужно, когда
/// локальный кэш вычищен по глубине хранения, а пользователь открыл старое письмо.
pub async fn fetch_oauth_message_raw(
    host: &str,
    email: &str,
    access_token: &str,
    folder_path: &str,
    uid: u32,
) -> Result<Vec<u8>> {
    let mut session = connect_oauth(host, email, access_token).await?;
    session
        .select(folder_path)
        .await
        .map_err(|error| Error::Backend {
            backend: "imap-select".into(),
            message: format!("{folder_path}: {error}"),
        })?;
    let fetched = session
        .uid_fetch(uid.to_string(), "(UID BODY.PEEK[])")
        .await
        .map_err(|error| Error::Backend {
            backend: "imap-fetch".into(),
            message: error.to_string(),
        })?
        .try_collect::<Vec<_>>()
        .await
        .map_err(|error| Error::Backend {
            backend: "imap-fetch".into(),
            message: error.to_string(),
        })?;
    let raw = fetched
        .iter()
        .find(|fetch| fetch.uid == Some(uid))
        .and_then(|fetch| fetch.body())
        .map(<[u8]>::to_vec);
    let _ = session.logout().await;
    raw.ok_or_else(|| Error::Backend {
        backend: "imap-fetch".into(),
        message: format!("письмо uid={uid} не найдено на сервере"),
    })
}

/// IMAP использует modified UTF-7 для имён папок (RFC 3501, раздел 5.1.3).
/// Декодер оставлен локальным, чтобы не тащить устаревшую кодировочную библиотеку.
fn decode_modified_utf7(value: &str) -> Option<String> {
    use base64::Engine;
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start + 1..];
        let end = rest.find('-')?;
        let encoded = &rest[..end];
        if encoded.is_empty() {
            out.push('&');
        } else {
            let standard = encoded.replace(',', "/");
            let padded = format!("{standard}{}", "=".repeat((4 - standard.len() % 4) % 4));
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(padded)
                .ok()?;
            if bytes.len() % 2 != 0 {
                return None;
            }
            let units = bytes
                .chunks_exact(2)
                .map(|pair| u16::from_be_bytes([pair[0], pair[1]]));
            out.extend(
                char::decode_utf16(units).map(|ch| ch.unwrap_or(char::REPLACEMENT_CHARACTER)),
            );
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

fn encode_modified_utf7(value: &str) -> String {
    use base64::Engine;
    let mut output = String::new();
    let mut encoded = Vec::new();
    let flush = |output: &mut String, encoded: &mut Vec<u8>| {
        if encoded.is_empty() {
            return;
        }
        let value = base64::engine::general_purpose::STANDARD_NO_PAD
            .encode(&*encoded)
            .replace('/', ",");
        output.push('&');
        output.push_str(&value);
        output.push('-');
        encoded.clear();
    };
    for ch in value.chars() {
        if (' '..='~').contains(&ch) && ch != '&' {
            flush(&mut output, &mut encoded);
            output.push(ch);
        } else if ch == '&' {
            flush(&mut output, &mut encoded);
            output.push_str("&-");
        } else {
            for unit in ch.encode_utf16(&mut [0_u16; 2]).iter() {
                encoded.extend_from_slice(&unit.to_be_bytes());
            }
        }
    }
    flush(&mut output, &mut encoded);
    output
}

#[cfg(test)]
mod utf7_tests {
    use super::{MESSAGE_FETCH_ITEMS, decode_modified_utf7, encode_modified_utf7};

    #[test]
    fn message_fetch_never_marks_mail_as_seen() {
        assert!(MESSAGE_FETCH_ITEMS.contains("BODY.PEEK[]"));
        assert!(!MESSAGE_FETCH_ITEMS.contains(" RFC822 "));
    }

    #[test]
    fn decodes_imap_folder_names() {
        assert_eq!(decode_modified_utf7("INBOX").as_deref(), Some("INBOX"));
        assert_eq!(
            decode_modified_utf7("&BB4EQgQ,BEAEMAQyBDsENQQ9BD0ESwQ1-").as_deref(),
            Some("Отправленные")
        );
        assert_eq!(decode_modified_utf7("A&-B").as_deref(), Some("A&B"));
    }

    #[test]
    fn encodes_imap_folder_names() {
        let encoded = encode_modified_utf7("Архив & old");
        assert_eq!(
            decode_modified_utf7(&encoded).as_deref(),
            Some("Архив & old")
        );
    }
}
