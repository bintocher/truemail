//! Синхронизация почты через IMAP с OAuth2.

use crate::model::{FolderRole, Security, infer_folder_role};
use crate::{Error, Result};
use async_imap::{Authenticator, types::Name, types::NameAttribute};
use futures::TryStreamExt;
use imap_proto::{AttributeValue, MailboxDatum, Response, ResponseCode, Status};
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
    pub sync_token: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct FolderSyncCursor {
    pub uidvalidity: Option<u32>,
    pub first_uid: Option<u32>,
    pub last_uid: Option<u32>,
    pub known_uids: Vec<u32>,
    pub highestmodseq: Option<u64>,
    pub sync_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DiscoveredFlagUpdate {
    pub folder_path: String,
    pub uid: u32,
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
    pub draft: bool,
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
    /// `false` означает лёгкую проекцию заголовков/preview: полный MIME будет
    /// лениво загружен при открытии письма.
    pub body_fetched: bool,
}

type OAuthSession = async_imap::Session<tokio_rustls::client::TlsStream<TcpStream>>;
const MESSAGE_FETCH_ITEMS: &str = "(UID BODY.PEEK[] FLAGS RFC822.SIZE)";

fn uid_set(uids: &[u32]) -> String {
    let mut sorted = uids.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut ranges = Vec::new();
    let mut index = 0;
    while index < sorted.len() {
        let start = sorted[index];
        let mut end = start;
        while index + 1 < sorted.len() && sorted[index + 1] == end.saturating_add(1) {
            index += 1;
            end = sorted[index];
        }
        if start == end {
            ranges.push(start.to_string());
        } else {
            ranges.push(format!("{start}:{end}"));
        }
        index += 1;
    }
    ranges.join(",")
}

async fn select_qresync(
    session: &mut OAuthSession,
    mailbox_name: &str,
    uidvalidity: u32,
    highestmodseq: u64,
    known_uids: &[u32],
) -> Result<(
    async_imap::types::Mailbox,
    Vec<u32>,
    Vec<DiscoveredFlagUpdate>,
)> {
    if mailbox_name.contains(['\r', '\n']) {
        return Err(Error::Backend {
            backend: "imap-qresync".into(),
            message: "недопустимое имя mailbox".into(),
        });
    }
    let quoted_mailbox = format!(
        "\"{}\"",
        mailbox_name.replace('\\', "\\\\").replace('"', "\\\"")
    );
    let known = uid_set(known_uids);
    let command =
        format!("SELECT {quoted_mailbox} (QRESYNC ({uidvalidity} {highestmodseq} {known}))");
    let request_id = session
        .run_command(command)
        .await
        .map_err(|error| Error::Backend {
            backend: "imap-qresync".into(),
            message: error.to_string(),
        })?;
    let mut mailbox = async_imap::types::Mailbox::default();
    let mut vanished = Vec::new();
    let mut flag_updates = Vec::new();
    loop {
        let response = session
            .read_response()
            .await
            .map_err(|error| Error::Backend {
                backend: "imap-qresync".into(),
                message: error.to_string(),
            })?
            .ok_or_else(|| Error::Backend {
                backend: "imap-qresync".into(),
                message: "сервер закрыл соединение во время SELECT QRESYNC".into(),
            })?;
        match response.parsed() {
            Response::MailboxData(MailboxDatum::Exists(value)) => mailbox.exists = *value,
            Response::MailboxData(MailboxDatum::Recent(value)) => mailbox.recent = *value,
            Response::Data {
                code: Some(code), ..
            } => match code {
                ResponseCode::UidValidity(value) => mailbox.uid_validity = Some(*value),
                ResponseCode::UidNext(value) => mailbox.uid_next = Some(*value),
                ResponseCode::HighestModSeq(value) => mailbox.highest_modseq = Some(*value),
                ResponseCode::Unseen(value) => mailbox.unseen = Some(*value),
                _ => {}
            },
            Response::Vanished { uids, .. } => {
                for range in uids {
                    vanished.extend(range.clone());
                }
            }
            Response::Fetch(_, attributes) => {
                let uid = attributes.iter().find_map(|attribute| match attribute {
                    AttributeValue::Uid(uid) => Some(*uid),
                    _ => None,
                });
                let flags = attributes.iter().find_map(|attribute| match attribute {
                    AttributeValue::Flags(flags) => Some(flags),
                    _ => None,
                });
                if let (Some(uid), Some(flags)) = (uid, flags) {
                    flag_updates.push(DiscoveredFlagUpdate {
                        folder_path: mailbox_name.to_owned(),
                        uid,
                        seen: flags.iter().any(|flag| flag.eq_ignore_ascii_case("\\Seen")),
                        flagged: flags
                            .iter()
                            .any(|flag| flag.eq_ignore_ascii_case("\\Flagged")),
                        answered: flags
                            .iter()
                            .any(|flag| flag.eq_ignore_ascii_case("\\Answered")),
                        draft: flags
                            .iter()
                            .any(|flag| flag.eq_ignore_ascii_case("\\Draft")),
                    });
                }
            }
            Response::Done {
                tag,
                status,
                information,
                ..
            } if tag == &request_id => {
                if *status == Status::Ok {
                    vanished.sort_unstable();
                    vanished.dedup();
                    return Ok((mailbox, vanished, flag_updates));
                }
                return Err(Error::Backend {
                    backend: "imap-qresync".into(),
                    message: information
                        .as_deref()
                        .unwrap_or("SELECT QRESYNC отклонён сервером")
                        .to_owned(),
                });
            }
            _ => {}
        }
    }
}

#[derive(Debug)]
pub struct ImapDiscovery {
    pub folders: Vec<DiscoveredFolder>,
    pub messages: Vec<DiscoveredMessage>,
    pub server_uids: Vec<(String, Vec<u32>)>,
    pub reset_folders: Vec<String>,
    /// Complete set of remote IDs, when the provider returned a full snapshot.
    pub remote_snapshot: Option<Vec<String>>,
    /// Remote IDs whose folder projections may have changed in this delta.
    pub changed_remote_ids: Vec<String>,
    /// Flag-only changes returned by IMAP CONDSTORE without downloading MIME bodies.
    pub flag_updates: Vec<DiscoveredFlagUpdate>,
    /// Exact expunged UIDs reported by QRESYNC VANISHED, scoped per mailbox.
    pub deleted_uids: Vec<(String, Vec<u32>)>,
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
    let client = connect_tls_client(host, 993, Security::Ssl).await?;
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

async fn connect_password(
    host: &str,
    port: u16,
    security: Security,
    username: &str,
    password: &str,
) -> Result<OAuthSession> {
    let client = connect_tls_client(host, port, security).await?;
    client
        .login(username, password)
        .await
        .map_err(|(error, _)| Error::Backend {
            backend: "imap-auth".into(),
            message: error.to_string(),
        })
}

async fn connect_tls_client(
    host: &str,
    port: u16,
    security: Security,
) -> Result<async_imap::Client<tokio_rustls::client::TlsStream<TcpStream>>> {
    if security == Security::None {
        return Err(Error::AccountConfig(
            "незашифрованный IMAP не поддерживается; выберите SSL/TLS или STARTTLS".into(),
        ));
    }
    let tcp = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        TcpStream::connect((host, port)),
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
    let tcp = if security == Security::Starttls {
        let mut client = async_imap::Client::new(tcp);
        client
            .read_response()
            .await
            .map_err(|error| Error::Backend {
                backend: "imap".into(),
                message: error.to_string(),
            })?
            .ok_or_else(|| Error::Backend {
                backend: "imap".into(),
                message: "сервер закрыл соединение".into(),
            })?;
        client
            .run_command_and_check_ok("STARTTLS", None)
            .await
            .map_err(|error| Error::Backend {
                backend: "imap-starttls".into(),
                message: error.to_string(),
            })?;
        client.into_inner()
    } else {
        tcp
    };
    let tls = TlsConnector::from(config)
        .connect(server_name, tcp)
        .await
        .map_err(|e| Error::Backend {
            backend: "imap-tls".into(),
            message: e.to_string(),
        })?;
    let mut client = async_imap::Client::new(tls);
    if security == Security::Ssl {
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
    }
    Ok(client)
}

fn sent_mailbox_candidate(remote_path: &str, special_use_sent: bool) -> bool {
    if special_use_sent {
        return true;
    }
    let display_name = decode_modified_utf7(remote_path).unwrap_or_else(|| remote_path.to_owned());
    infer_folder_role(remote_path, &display_name) == Some(FolderRole::Sent)
}

fn mime_message_id(raw: &[u8]) -> Option<String> {
    raw.split(|byte| *byte == b'\n')
        .map(|line| {
            std::str::from_utf8(line)
                .unwrap_or_default()
                .trim_end_matches('\r')
        })
        .take_while(|line| !line.is_empty())
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("Message-ID")
                .then(|| value.trim().to_owned())
        })
        .filter(|value| !value.is_empty() && !value.contains(['\r', '\n']))
}

async fn append_sent(mut session: OAuthSession, raw: &[u8]) -> Result<()> {
    let names: Vec<Name> = session
        .list(Some(""), Some("*"))
        .await
        .map_err(|error| Error::Backend {
            backend: "imap-sent-list".into(),
            message: error.to_string(),
        })?
        .try_collect()
        .await
        .map_err(|error| Error::Backend {
            backend: "imap-sent-list".into(),
            message: error.to_string(),
        })?;
    let sent = names
        .iter()
        .find(|name| {
            name.attributes()
                .iter()
                .any(|attribute| matches!(attribute, NameAttribute::Sent))
        })
        .or_else(|| {
            names
                .iter()
                .find(|name| sent_mailbox_candidate(name.name(), false))
        })
        .map(|name| name.name().to_owned())
        .ok_or_else(|| Error::Backend {
            backend: "imap-sent-append".into(),
            message: "сервер не объявил папку отправленных (\\Sent)".into(),
        })?;
    if let Some(message_id) = mime_message_id(raw) {
        session
            .select(&sent)
            .await
            .map_err(|error| Error::Backend {
                backend: "imap-sent-select".into(),
                message: format!("{sent}: {error}"),
            })?;
        let quoted = message_id.replace('\\', "\\\\").replace('"', "\\\"");
        let existing = session
            .uid_search(format!("HEADER Message-ID \"{quoted}\""))
            .await
            .map_err(|error| Error::Backend {
                backend: "imap-sent-search".into(),
                message: format!("{sent}: {error}"),
            })?;
        if !existing.is_empty() {
            tracing::info!(sent, message_id, "серверная Sent-копия уже существует");
            let _ = session.logout().await;
            return Ok(());
        }
    }
    session
        .append(&sent, Some("(\\Seen)"), None, raw)
        .await
        .map_err(|error| Error::Backend {
            backend: "imap-sent-append".into(),
            message: format!("{sent}: {error}"),
        })?;
    let _ = session.logout().await;
    Ok(())
}

pub(crate) async fn append_oauth_sent(
    host: &str,
    email: &str,
    access_token: &str,
    raw: &[u8],
) -> Result<()> {
    append_sent(connect_oauth(host, email, access_token).await?, raw).await
}

pub(crate) async fn append_password_sent(
    host: &str,
    port: u16,
    security: Security,
    username: &str,
    password: &str,
    raw: &[u8],
) -> Result<()> {
    append_sent(
        connect_password(host, port, security, username, password).await?,
        raw,
    )
    .await
}

pub(crate) async fn rename_oauth_folder(
    host: &str,
    email: &str,
    access_token: &str,
    remote_path: &str,
    new_name: &str,
) -> Result<String> {
    let session = connect_oauth(host, email, access_token).await?;
    rename_folder(session, remote_path, new_name).await
}

pub(crate) async fn rename_password_folder(
    host: &str,
    port: u16,
    security: Security,
    username: &str,
    password: &str,
    remote_path: &str,
    new_name: &str,
) -> Result<String> {
    let session = connect_password(host, port, security, username, password).await?;
    rename_folder(session, remote_path, new_name).await
}

async fn rename_folder(
    mut session: OAuthSession,
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
    let session = connect_oauth(host, email, access_token).await?;
    delete_folder(session, remote_path).await
}

pub(crate) async fn delete_password_folder(
    host: &str,
    port: u16,
    security: Security,
    username: &str,
    password: &str,
    remote_path: &str,
) -> Result<()> {
    let session = connect_password(host, port, security, username, password).await?;
    delete_folder(session, remote_path).await
}

async fn delete_folder(mut session: OAuthSession, remote_path: &str) -> Result<()> {
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
        tracing::warn!(
            remote_path,
            "imap-delete: папка не найдена на сервере (LIST пуст)"
        );
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
    validate_session(&mut session).await?;
    let _ = session.logout().await;
    Ok(())
}

pub async fn validate_password(
    host: &str,
    port: u16,
    security: Security,
    username: &str,
    password: &str,
) -> Result<()> {
    let mut session = connect_password(host, port, security, username, password).await?;
    validate_session(&mut session).await?;
    let _ = session.logout().await;
    Ok(())
}

async fn validate_session(session: &mut OAuthSession) -> Result<()> {
    session.noop().await.map_err(|e| Error::Backend {
        backend: "imap-auth".into(),
        message: e.to_string(),
    })?;
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
    let session = connect_oauth(host, email, access_token).await?;
    apply_operation(session, op_kind, payload).await
}

pub async fn apply_password_operation(
    host: &str,
    port: u16,
    security: Security,
    username: &str,
    password: &str,
    op_kind: &str,
    payload: &str,
) -> Result<()> {
    let session = connect_password(host, port, security, username, password).await?;
    apply_operation(session, op_kind, payload).await
}

async fn apply_operation(mut session: OAuthSession, op_kind: &str, payload: &str) -> Result<()> {
    let payload: serde_json::Value = serde_json::from_str(payload)?;
    let folder = payload["folder_path"]
        .as_str()
        .ok_or_else(|| Error::AccountConfig("outbox: нет folder_path".into()))?;
    let uid = payload["uid"]
        .as_u64()
        .ok_or_else(|| Error::AccountConfig("outbox: нет uid".into()))?;
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
            sync_token: None,
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

pub async fn discover_password_folders(
    host: &str,
    port: u16,
    security: Security,
    username: &str,
    password: &str,
) -> Result<Vec<DiscoveredFolder>> {
    let mut session = connect_password(host, port, security, username, password).await?;
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
    retention_days: Option<i64>,
    condstore: bool,
    qresync: bool,
) -> Result<(
    Vec<DiscoveredMessage>,
    Vec<u32>,
    bool,
    bool,
    Vec<DiscoveredFlagUpdate>,
    Vec<u32>,
)> {
    let qresync_selection = if qresync {
        if let Some(cursor) = cursor
            && let (Some(uidvalidity), Some(highestmodseq)) =
                (cursor.uidvalidity, cursor.highestmodseq)
            && !cursor.known_uids.is_empty()
        {
            match select_qresync(
                session,
                &folder.remote_path,
                uidvalidity,
                highestmodseq,
                &cursor.known_uids,
            )
            .await
            {
                Ok(selection) => Some(selection),
                Err(error) => {
                    tracing::warn!(folder = %folder.remote_path, %error, "IMAP QRESYNC rejected; falling back to CONDSTORE");
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };
    let qresync_used = qresync_selection.is_some();
    let (mailbox, vanished_uids, mut flag_updates) = if let Some(selection) = qresync_selection {
        selection
    } else {
        let mailbox = if condstore {
            session.select_condstore(&folder.remote_path).await
        } else {
            session.select(&folder.remote_path).await
        }
        .map_err(|error| Error::Backend {
            backend: "imap-select".into(),
            message: format!("{}: {error}", folder.remote_path),
        })?;
        (mailbox, Vec::new(), Vec::new())
    };
    folder.uidvalidity = mailbox.uid_validity;
    folder.uidnext = mailbox.uid_next;
    folder.highestmodseq = mailbox.highest_modseq;
    let uidvalidity_changed = cursor.is_some_and(|cursor| {
        cursor.uidvalidity.is_some()
            && mailbox.uid_validity.is_some()
            && cursor.uidvalidity != mailbox.uid_validity
    });
    let now = chrono::Utc::now().timestamp();
    let last_full_reconcile = cursor
        .and_then(|value| value.sync_token.as_deref())
        .and_then(|value| value.strip_prefix("imap-reconcile:"))
        .and_then(|value| value.parse::<i64>().ok());
    let full_snapshot = cursor.is_none()
        || uidvalidity_changed
        || last_full_reconcile.is_none_or(|last| now.saturating_sub(last) >= 24 * 60 * 60);
    let mut uids: Vec<u32> = if full_snapshot {
        session
            .uid_search("ALL")
            .await
            .map_err(|error| Error::Backend {
                backend: "imap-search".into(),
                message: format!("{}: {error}", folder.remote_path),
            })?
            .into_iter()
            .collect()
    } else if let Some(last_uid) = cursor.and_then(|value| value.last_uid) {
        let first_new = last_uid.saturating_add(1);
        if mailbox
            .uid_next
            .is_some_and(|uid_next| uid_next <= first_new)
        {
            Vec::new()
        } else {
            session
                .uid_search(format!("UID {first_new}:*"))
                .await
                .map_err(|error| Error::Backend {
                    backend: "imap-search".into(),
                    message: format!("{}: {error}", folder.remote_path),
                })?
                .into_iter()
                .collect()
        }
    } else {
        Vec::new()
    };
    uids.sort_unstable();
    if full_snapshot {
        folder.sync_token = Some(format!("imap-reconcile:{now}"));
    } else {
        folder.sync_token = cursor.and_then(|value| value.sync_token.clone());
    }
    if condstore
        && !qresync_used
        && !uidvalidity_changed
        && let Some(previous_modseq) = cursor.and_then(|value| value.highestmodseq)
        && mailbox
            .highest_modseq
            .is_some_and(|current| current > previous_modseq)
    {
        let fetched = session
            .uid_fetch(
                "1:*",
                format!("(UID FLAGS MODSEQ) (CHANGEDSINCE {previous_modseq})"),
            )
            .await
            .map_err(|error| Error::Backend {
                backend: "imap-condstore".into(),
                message: format!("{}: {error}", folder.remote_path),
            })?
            .try_collect::<Vec<_>>()
            .await
            .map_err(|error| Error::Backend {
                backend: "imap-condstore".into(),
                message: format!("{}: {error}", folder.remote_path),
            })?;
        for fetch in fetched {
            let Some(uid) = fetch.uid else { continue };
            if cursor
                .and_then(|value| value.last_uid)
                .is_some_and(|last_uid| uid > last_uid)
            {
                continue;
            }
            let flags = fetch.flags().collect::<Vec<_>>();
            flag_updates.push(DiscoveredFlagUpdate {
                folder_path: folder.remote_path.clone(),
                uid,
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
            });
        }
    }
    let retained_uids = match retention_days {
        Some(days) if days > 0 => {
            let since = (chrono::Utc::now().date_naive() - chrono::Duration::days(days))
                .format("%d-%b-%Y")
                .to_string();
            let mut retained = session
                .uid_search(format!("SINCE {since}"))
                .await
                .map_err(|error| Error::Backend {
                    backend: "imap-search".into(),
                    message: format!("{}: {error}", folder.remote_path),
                })?
                .into_iter()
                .collect::<Vec<_>>();
            retained.sort_unstable();
            retained
        }
        Some(_) if full_snapshot => uids.clone(),
        Some(_) => Vec::new(),
        None => Vec::new(),
    };
    let selected = if !uidvalidity_changed
        && let Some(cursor) = cursor
        && let Some(last_uid) = cursor.last_uid
    {
        let mut selected = uids
            .iter()
            .copied()
            .filter(|uid| *uid > last_uid)
            .take(limit)
            .collect::<Vec<_>>();
        if retention_days.is_some()
            && let Some(first_uid) = cursor.first_uid
        {
            selected.extend(retained_uids.iter().copied().filter(|uid| *uid < first_uid));
        }
        selected.sort_unstable();
        selected.dedup();
        selected
    } else if retention_days.is_some() {
        retained_uids
    } else {
        uids[uids.len().saturating_sub(limit)..].to_vec()
    };
    if selected.is_empty() {
        return Ok((
            Vec::new(),
            uids,
            uidvalidity_changed,
            full_snapshot,
            flag_updates,
            vanished_uids,
        ));
    }
    let mut messages = Vec::with_capacity(selected.len());
    for chunk in selected.chunks(limit.max(1)) {
        let set = chunk
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
                body_fetched: true,
            });
        }
    }
    Ok((
        messages,
        uids,
        uidvalidity_changed,
        full_snapshot,
        flag_updates,
        vanished_uids,
    ))
}

/// Быстрая дозагрузка входящих после IMAP IDLE-события.
pub async fn discover_oauth_inbox(
    host: &str,
    email: &str,
    access_token: &str,
    cursors: &HashMap<String, FolderSyncCursor>,
) -> Result<ImapDiscovery> {
    let session = connect_oauth(host, email, access_token).await?;
    discover_inbox(session, cursors).await
}

pub async fn discover_password_inbox(
    host: &str,
    port: u16,
    security: Security,
    username: &str,
    password: &str,
    cursors: &HashMap<String, FolderSyncCursor>,
) -> Result<ImapDiscovery> {
    let session = connect_password(host, port, security, username, password).await?;
    discover_inbox(session, cursors).await
}

async fn discover_inbox(
    mut session: OAuthSession,
    cursors: &HashMap<String, FolderSyncCursor>,
) -> Result<ImapDiscovery> {
    let mut folders = list_oauth_folders(&mut session).await?;
    let capabilities = session
        .capabilities()
        .await
        .map_err(|error| Error::Backend {
            backend: "imap-capability".into(),
            message: error.to_string(),
        })?;
    let qresync = capabilities.has_str("QRESYNC");
    let condstore = capabilities.has_str("CONDSTORE") || qresync;
    let inbox = folders
        .iter_mut()
        .find(|folder| folder.role == Some(FolderRole::Inbox))
        .ok_or_else(|| Error::Backend {
            backend: "imap-list".into(),
            message: "папка INBOX не найдена".into(),
        })?;
    let path = inbox.remote_path.clone();
    let (messages, uids, reset, full_snapshot, flag_updates, vanished_uids) =
        fetch_incremental_messages(
            &mut session,
            inbox,
            cursors.get(&path),
            500,
            None,
            condstore,
            qresync,
        )
        .await?;
    let _ = session.logout().await;
    Ok(ImapDiscovery {
        folders,
        messages,
        server_uids: full_snapshot
            .then_some((path.clone(), uids))
            .into_iter()
            .collect(),
        reset_folders: reset.then_some(path.clone()).into_iter().collect(),
        remote_snapshot: None,
        changed_remote_ids: Vec::new(),
        flag_updates,
        deleted_uids: (!vanished_uids.is_empty())
            .then_some((path, vanished_uids))
            .into_iter()
            .collect(),
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
    let session = connect_oauth(host, email, access_token).await?;
    wait_for_change(session).await
}

pub async fn wait_for_password_change(
    host: &str,
    port: u16,
    security: Security,
    username: &str,
    password: &str,
) -> Result<()> {
    let session = connect_password(host, port, security, username, password).await?;
    wait_for_change(session).await
}

async fn wait_for_change(mut session: OAuthSession) -> Result<()> {
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
    retention_days: i64,
) -> Result<ImapDiscovery> {
    let session = connect_oauth(host, email, access_token).await?;
    discover_session(session, cursors, retention_days).await
}

pub async fn discover_password(
    host: &str,
    port: u16,
    security: Security,
    username: &str,
    password: &str,
    cursors: &HashMap<String, FolderSyncCursor>,
    retention_days: i64,
) -> Result<ImapDiscovery> {
    let session = connect_password(host, port, security, username, password).await?;
    discover_session(session, cursors, retention_days).await
}

async fn discover_session(
    mut session: OAuthSession,
    cursors: &HashMap<String, FolderSyncCursor>,
    retention_days: i64,
) -> Result<ImapDiscovery> {
    let mut folders = list_oauth_folders(&mut session).await?;
    let capabilities = session
        .capabilities()
        .await
        .map_err(|error| Error::Backend {
            backend: "imap-capability".into(),
            message: error.to_string(),
        })?;
    let qresync = capabilities.has_str("QRESYNC");
    let condstore = qresync || capabilities.has_str("CONDSTORE");
    tracing::debug!(qresync, condstore, "IMAP incremental capabilities");
    let mut messages = Vec::new();
    let mut server_uids = Vec::new();
    let mut reset_folders = Vec::new();
    let mut flag_updates = Vec::new();
    let mut deleted_uids = Vec::new();
    for folder in &mut folders {
        let path = folder.remote_path.clone();
        let folder_started = std::time::Instant::now();
        match fetch_incremental_messages(
            &mut session,
            folder,
            cursors.get(&path),
            500,
            Some(retention_days),
            condstore,
            qresync,
        )
        .await
        {
            Ok((
                mut folder_messages,
                uids,
                reset,
                full_snapshot,
                mut folder_flag_updates,
                vanished,
            )) => {
                tracing::info!(
                    collection = %path,
                    scope = if full_snapshot { "full-reconcile" } else { "delta" },
                    messages = folder_messages.len(),
                    flag_updates = folder_flag_updates.len(),
                    snapshot_uids = if full_snapshot { uids.len() } else { 0 },
                    network_ms = folder_started.elapsed().as_millis() as u64,
                    "IMAP collection delta fetched"
                );
                messages.append(&mut folder_messages);
                flag_updates.append(&mut folder_flag_updates);
                if !vanished.is_empty() {
                    deleted_uids.push((path.clone(), vanished));
                }
                if full_snapshot {
                    server_uids.push((path.clone(), uids));
                }
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
        remote_snapshot: None,
        changed_remote_ids: Vec::new(),
        flag_updates,
        deleted_uids,
    })
}

pub async fn discover_yandex(
    email: &str,
    access_token: &str,
    cursors: &HashMap<String, FolderSyncCursor>,
    retention_days: i64,
) -> Result<ImapDiscovery> {
    discover_oauth(
        "imap.yandex.com",
        email,
        access_token,
        cursors,
        retention_days,
    )
    .await
}

pub async fn discover_gmail(
    email: &str,
    access_token: &str,
    cursors: &HashMap<String, FolderSyncCursor>,
    retention_days: i64,
) -> Result<ImapDiscovery> {
    discover_oauth(
        "imap.gmail.com",
        email,
        access_token,
        cursors,
        retention_days,
    )
    .await
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
    let session = connect_oauth(host, email, access_token).await?;
    fetch_message_raw(session, folder_path, uid).await
}

pub async fn fetch_password_message_raw(
    host: &str,
    port: u16,
    security: Security,
    username: &str,
    password: &str,
    folder_path: &str,
    uid: u32,
) -> Result<Vec<u8>> {
    let session = connect_password(host, port, security, username, password).await?;
    fetch_message_raw(session, folder_path, uid).await
}

async fn fetch_message_raw(
    mut session: OAuthSession,
    folder_path: &str,
    uid: u32,
) -> Result<Vec<u8>> {
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
    use super::{
        MESSAGE_FETCH_ITEMS, decode_modified_utf7, encode_modified_utf7, mime_message_id,
        sent_mailbox_candidate, uid_set,
    };

    #[test]
    fn qresync_known_uids_are_compacted_without_inventing_gaps() {
        assert_eq!(uid_set(&[9, 2, 3, 4, 9, 12, 13]), "2:4,9,12:13");
    }

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

    #[test]
    fn recognizes_sent_mailbox_by_special_use_or_localized_name() {
        assert!(sent_mailbox_candidate("anything", true));
        assert!(sent_mailbox_candidate("Sent Items", false));
        assert!(sent_mailbox_candidate(
            "&BB4EQgQ,BEAEMAQyBDsENQQ9BD0ESwQ1-",
            false
        ));
        assert!(!sent_mailbox_candidate("INBOX", false));
    }

    #[test]
    fn extracts_message_id_for_idempotent_sent_append() {
        let raw =
            b"From: me@example.test\r\nMessage-ID: <stable@example.test>\r\nSubject: x\r\n\r\nbody";
        assert_eq!(
            mime_message_id(raw).as_deref(),
            Some("<stable@example.test>")
        );
        assert_eq!(mime_message_id(b"Subject: none\r\n\r\nbody"), None);
    }
}
