use super::{DiscoveredFolder, DiscoveredMessage, FolderSyncCursor, ImapDiscovery};
use crate::model::FolderRole;
use crate::{Error, Result};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use futures::{StreamExt, stream};
use reqwest::{Client, Method, Response, Url};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

const GMAIL_BASE: &str = "https://gmail.googleapis.com/gmail/v1/users/me";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LabelList {
    #[serde(default)]
    labels: Vec<Label>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Label {
    id: String,
    name: String,
    #[serde(default)]
    messages_total: i64,
    #[serde(default)]
    messages_unread: i64,
}

#[derive(Debug, Deserialize)]
struct MessageList {
    #[serde(default)]
    messages: Vec<MessageRef>,
}

#[derive(Debug, Deserialize)]
struct MessageRef {
    id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMessage {
    id: String,
    #[serde(default)]
    label_ids: Vec<String>,
    raw: String,
    #[serde(default)]
    size_estimate: u32,
}

fn client() -> Result<Client> {
    Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| Error::Backend {
            backend: "gmail-api".into(),
            message: error.to_string(),
        })
}

async fn checked(response: Response, backend: &str) -> Result<Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(Error::Backend {
        backend: backend.into(),
        message: format!("HTTP {status}: {body}"),
    })
}

async fn request(
    client: &Client,
    method: Method,
    url: Url,
    token: &str,
    body: Option<serde_json::Value>,
) -> Result<Response> {
    let mut request = client.request(method, url).bearer_auth(token);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await.map_err(|error| Error::Backend {
        backend: "gmail-api".into(),
        message: error.to_string(),
    })?;
    checked(response, "gmail-api").await
}

fn url(parts: &[&str]) -> Result<Url> {
    let mut result = Url::parse(GMAIL_BASE).map_err(|error| Error::Other(error.to_string()))?;
    {
        let mut segments = result
            .path_segments_mut()
            .map_err(|_| Error::Other("некорректный Gmail API URL".into()))?;
        segments.pop_if_empty();
        for part in parts {
            segments.push(part);
        }
    }
    Ok(result)
}

fn role(id: &str) -> Option<FolderRole> {
    match id {
        "INBOX" => Some(FolderRole::Inbox),
        "SENT" => Some(FolderRole::Sent),
        "DRAFT" => Some(FolderRole::Drafts),
        "SPAM" => Some(FolderRole::Spam),
        "TRASH" => Some(FolderRole::Trash),
        "ALL" => Some(FolderRole::Archive),
        _ => None,
    }
}

fn visible_label(label: &Label) -> bool {
    !matches!(
        label.id.as_str(),
        "UNREAD"
            | "STARRED"
            | "IMPORTANT"
            | "CHAT"
            | "CATEGORY_PERSONAL"
            | "CATEGORY_SOCIAL"
            | "CATEGORY_PROMOTIONS"
            | "CATEGORY_UPDATES"
            | "CATEGORY_FORUMS"
    )
}

fn stable_uid(remote_id: &str) -> u32 {
    let digest = Sha256::digest(remote_id.as_bytes());
    u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]).max(1)
}

pub async fn validate(access_token: &str) -> Result<()> {
    request(
        &client()?,
        Method::GET,
        url(&["profile"])?,
        access_token,
        None,
    )
    .await?;
    Ok(())
}

pub async fn discover_folders(access_token: &str) -> Result<Vec<DiscoveredFolder>> {
    let client = client()?;
    let labels: LabelList = request(&client, Method::GET, url(&["labels"])?, access_token, None)
        .await?
        .json()
        .await
        .map_err(|error| Error::Backend {
            backend: "gmail-labels".into(),
            message: error.to_string(),
        })?;
    let mut folders: Vec<_> = labels
        .labels
        .into_iter()
        .filter(visible_label)
        .map(|label| DiscoveredFolder {
            role: role(&label.id),
            remote_path: label.id,
            display_name: label.name,
            unread_count: label.messages_unread,
            total_count: label.messages_total,
            uidvalidity: None,
            uidnext: None,
            highestmodseq: None,
        })
        .collect();
    if !folders
        .iter()
        .any(|folder| folder.role == Some(FolderRole::Archive))
    {
        folders.push(DiscoveredFolder {
            remote_path: "ALL".into(),
            display_name: "All Mail".into(),
            role: Some(FolderRole::Archive),
            unread_count: 0,
            total_count: 0,
            uidvalidity: None,
            uidnext: None,
            highestmodseq: None,
        });
    }
    Ok(folders)
}

pub async fn discover(
    access_token: &str,
    _cursors: &HashMap<String, FolderSyncCursor>,
) -> Result<ImapDiscovery> {
    let client = client()?;
    let folders = discover_folders(access_token).await?;
    let included: HashSet<String> = folders
        .iter()
        .map(|folder| folder.remote_path.clone())
        .collect();
    let mut list_url = url(&["messages"])?;
    list_url.query_pairs_mut().append_pair("maxResults", "500");
    let listed: MessageList = request(&client, Method::GET, list_url, access_token, None)
        .await?
        .json()
        .await
        .map_err(|error| Error::Backend {
            backend: "gmail-messages".into(),
            message: error.to_string(),
        })?;
    let token = access_token.to_owned();
    let fetched: Vec<RawMessage> = stream::iter(listed.messages)
        .map(|message| {
            let client = client.clone();
            let token = token.clone();
            async move {
                let mut message_url = url(&["messages", &message.id])?;
                message_url.query_pairs_mut().append_pair("format", "raw");
                request(&client, Method::GET, message_url, &token, None)
                    .await?
                    .json()
                    .await
                    .map_err(|error| Error::Backend {
                        backend: "gmail-message".into(),
                        message: error.to_string(),
                    })
            }
        })
        .buffer_unordered(8)
        .filter_map(|result| async move {
            match result {
                Ok(message) => Some(message),
                Err(error) => {
                    tracing::warn!(%error, "письмо Gmail API пропущено");
                    None
                }
            }
        })
        .collect()
        .await;
    let mut messages = Vec::new();
    for message in fetched {
        let raw = match URL_SAFE_NO_PAD.decode(message.raw.as_bytes()) {
            Ok(raw) => raw,
            Err(error) => {
                tracing::warn!(message_id = %message.id, %error, "Gmail raw не декодирован");
                continue;
            }
        };
        let labels: HashSet<_> = message.label_ids.iter().cloned().collect();
        let mut destinations: Vec<String> = labels
            .iter()
            .filter(|label| included.contains(*label))
            .cloned()
            .collect();
        if included.contains("ALL") && !labels.contains("SPAM") && !labels.contains("TRASH") {
            destinations.push("ALL".into());
        }
        destinations.sort();
        destinations.dedup();
        for folder_path in destinations {
            messages.push(DiscoveredMessage {
                folder_path,
                uid: stable_uid(&message.id),
                remote_id: Some(message.id.clone()),
                size: Some(message.size_estimate),
                seen: !labels.contains("UNREAD"),
                flagged: labels.contains("STARRED"),
                answered: false,
                draft: labels.contains("DRAFT"),
                raw: raw.clone(),
            });
        }
    }
    Ok(ImapDiscovery {
        folders,
        messages,
        server_uids: Vec::new(),
        reset_folders: Vec::new(),
    })
}

pub async fn apply_operation(access_token: &str, op_kind: &str, payload: &str) -> Result<()> {
    let payload: serde_json::Value = serde_json::from_str(payload)?;
    let id = payload["remote_id"]
        .as_str()
        .ok_or_else(|| Error::AccountConfig("у письма Gmail нет remote_id".into()))?;
    let client = client()?;
    match op_kind {
        "flag" => {
            let seen = payload["seen"].as_bool().unwrap_or(true);
            let body = if seen {
                json!({"removeLabelIds":["UNREAD"]})
            } else {
                json!({"addLabelIds":["UNREAD"]})
            };
            request(
                &client,
                Method::POST,
                url(&["messages", id, "modify"])?,
                access_token,
                Some(body),
            )
            .await?;
        }
        "move" => {
            let source = payload["folder_path"].as_str().unwrap_or("INBOX");
            let target = payload["target_folder_path"].as_str().unwrap_or("ALL");
            if target == "TRASH" {
                request(
                    &client,
                    Method::POST,
                    url(&["messages", id, "trash"])?,
                    access_token,
                    None,
                )
                .await?;
            } else {
                let add: Vec<&str> = if target == "ALL" {
                    vec![]
                } else {
                    vec![target]
                };
                let remove: Vec<&str> = if source == "ALL" {
                    vec!["INBOX"]
                } else {
                    vec![source]
                };
                request(
                    &client,
                    Method::POST,
                    url(&["messages", id, "modify"])?,
                    access_token,
                    Some(json!({"addLabelIds":add,"removeLabelIds":remove})),
                )
                .await?;
            }
        }
        "delete" => {
            request(
                &client,
                Method::DELETE,
                url(&["messages", id])?,
                access_token,
                None,
            )
            .await?;
        }
        other => {
            return Err(Error::AccountConfig(format!(
                "Gmail API: неизвестная операция {other}"
            )));
        }
    }
    Ok(())
}

pub async fn rename_label(access_token: &str, label_id: &str, new_name: &str) -> Result<String> {
    request(
        &client()?,
        Method::PATCH,
        url(&["labels", label_id])?,
        access_token,
        Some(json!({"name":new_name.trim()})),
    )
    .await?;
    Ok(label_id.to_owned())
}

pub async fn delete_label(access_token: &str, label_id: &str) -> Result<()> {
    request(
        &client()?,
        Method::DELETE,
        url(&["labels", label_id])?,
        access_token,
        None,
    )
    .await?;
    Ok(())
}
