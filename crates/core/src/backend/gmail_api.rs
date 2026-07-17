use super::{DiscoveredFolder, DiscoveredMessage, FolderSyncCursor, ImapDiscovery};
use crate::model::FolderRole;
use crate::{Error, Result};
use base64::Engine as _;
use base64::alphabet::URL_SAFE;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use futures::{StreamExt, TryStreamExt, stream};
use reqwest::{Client, Method, Response, StatusCode, Url};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

const GMAIL_BASE: &str = "https://gmail.googleapis.com/gmail/v1/users/me";

// Gmail отдаёт поле raw как base64url, но для части писем - с padding '='.
// Строгий NO_PAD-декодер на них падает с "Invalid padding" и письмо теряется,
// поэтому padding делаем необязательным (Indifferent).
const GMAIL_RAW_B64: GeneralPurpose = GeneralPurpose::new(
    &URL_SAFE,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

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
#[serde(rename_all = "camelCase")]
struct MessageList {
    #[serde(default)]
    messages: Vec<MessageRef>,
    next_page_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct MessageRef {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Profile {
    history_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryList {
    #[serde(default)]
    history: Vec<HistoryRecord>,
    next_page_token: Option<String>,
    history_id: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryRecord {
    #[serde(default)]
    messages: Vec<MessageRef>,
    #[serde(default)]
    messages_added: Vec<HistoryMessage>,
    #[serde(default)]
    messages_deleted: Vec<HistoryMessage>,
    #[serde(default)]
    labels_added: Vec<HistoryMessage>,
    #[serde(default)]
    labels_removed: Vec<HistoryMessage>,
}

#[derive(Debug, Deserialize)]
struct HistoryMessage {
    message: MessageRef,
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
    // Gmail отклоняет POST без Content-Length (HTTP 411). Операции trash/modify
    // без тела (move в корзину и т.п.) иначе зацикливаются на ретраях.
    request = match body {
        Some(body) => request.json(&body),
        None => request.header(reqwest::header::CONTENT_LENGTH, 0),
    };
    let response = request.send().await.map_err(|error| Error::Backend {
        backend: "gmail-api".into(),
        message: error.to_string(),
    })?;
    checked(response, "gmail-api").await
}

async fn get_allow_not_found(client: &Client, url: Url, token: &str) -> Result<Option<Response>> {
    let response = client
        .get(url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|error| Error::Backend {
            backend: "gmail-api".into(),
            message: error.to_string(),
        })?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    checked(response, "gmail-api").await.map(Some)
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

/// Лёгкая проверка: ID последних писем во Входящих (без загрузки тел).
/// Используется для почти реалтайм-уведомлений о новых письмах Gmail.
pub async fn latest_message_ids(access_token: &str, limit: u32) -> Result<Vec<String>> {
    let client = client()?;
    let mut list_url = url(&["messages"])?;
    list_url
        .query_pairs_mut()
        .append_pair("maxResults", &limit.to_string())
        .append_pair("labelIds", "INBOX");
    let listed: MessageList = request(&client, Method::GET, list_url, access_token, None)
        .await?
        .json()
        .await
        .map_err(|error| Error::Backend {
            backend: "gmail-messages".into(),
            message: error.to_string(),
        })?;
    Ok(listed.messages.into_iter().map(|item| item.id).collect())
}

/// Докачать сырой MIME одного письма Gmail по его remote_id (format=raw),
/// когда локальный кэш вычищен по глубине хранения.
pub async fn fetch_message_raw(access_token: &str, remote_id: &str) -> Result<Vec<u8>> {
    let client = client()?;
    let mut message_url = url(&["messages", remote_id])?;
    message_url.query_pairs_mut().append_pair("format", "raw");
    let message: RawMessage = request(&client, Method::GET, message_url, access_token, None)
        .await?
        .json()
        .await
        .map_err(|error| Error::Backend {
            backend: "gmail-message".into(),
            message: error.to_string(),
        })?;
    GMAIL_RAW_B64
        .decode(message.raw.as_bytes())
        .map_err(|error| Error::Backend {
            backend: "gmail-message".into(),
            message: format!("raw не декодирован: {error}"),
        })
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
            sync_token: None,
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
            sync_token: None,
        });
    }
    Ok(folders)
}

async fn profile_history_id(client: &Client, access_token: &str) -> Result<String> {
    let profile: Profile = request(client, Method::GET, url(&["profile"])?, access_token, None)
        .await?
        .json()
        .await
        .map_err(|error| Error::Backend {
            backend: "gmail-profile".into(),
            message: error.to_string(),
        })?;
    Ok(profile.history_id)
}

async fn list_all_message_ids(client: &Client, access_token: &str) -> Result<Vec<String>> {
    let mut ids = Vec::new();
    let mut page_token = None;
    loop {
        let mut list_url = url(&["messages"])?;
        list_url
            .query_pairs_mut()
            .append_pair("maxResults", "500")
            .append_pair("includeSpamTrash", "true");
        if let Some(token) = page_token.as_deref() {
            list_url.query_pairs_mut().append_pair("pageToken", token);
        }
        let listed: MessageList = request(client, Method::GET, list_url, access_token, None)
            .await?
            .json()
            .await
            .map_err(|error| Error::Backend {
                backend: "gmail-messages".into(),
                message: error.to_string(),
            })?;
        ids.extend(listed.messages.into_iter().map(|message| message.id));
        page_token = listed.next_page_token;
        if page_token.is_none() {
            break;
        }
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
}

fn collect_history_ids(records: Vec<HistoryRecord>) -> HashSet<String> {
    let mut ids = HashSet::new();
    for record in records {
        ids.extend(record.messages.into_iter().map(|message| message.id));
        ids.extend(
            record
                .messages_added
                .into_iter()
                .map(|item| item.message.id),
        );
        ids.extend(
            record
                .messages_deleted
                .into_iter()
                .map(|item| item.message.id),
        );
        ids.extend(record.labels_added.into_iter().map(|item| item.message.id));
        ids.extend(
            record
                .labels_removed
                .into_iter()
                .map(|item| item.message.id),
        );
    }
    ids
}

/// `None` means that Gmail no longer accepts the stored history ID and the
/// caller must rebuild a full snapshot.
async fn history_delta(
    client: &Client,
    access_token: &str,
    start_history_id: &str,
) -> Result<Option<(Vec<String>, String)>> {
    let mut ids = HashSet::new();
    let mut page_token = None;
    let mut latest_history_id: String;
    loop {
        let mut history_url = url(&["history"])?;
        history_url
            .query_pairs_mut()
            .append_pair("startHistoryId", start_history_id)
            .append_pair("maxResults", "500");
        if let Some(token) = page_token.as_deref() {
            history_url
                .query_pairs_mut()
                .append_pair("pageToken", token);
        }
        let Some(response) = get_allow_not_found(client, history_url, access_token).await? else {
            return Ok(None);
        };
        let page: HistoryList = response.json().await.map_err(|error| Error::Backend {
            backend: "gmail-history".into(),
            message: error.to_string(),
        })?;
        ids.extend(collect_history_ids(page.history));
        latest_history_id = page.history_id;
        page_token = page.next_page_token;
        if page_token.is_none() {
            break;
        }
    }
    let mut ids: Vec<_> = ids.into_iter().collect();
    ids.sort();
    Ok(Some((ids, latest_history_id)))
}

async fn fetch_messages(
    client: &Client,
    access_token: &str,
    ids: Vec<String>,
) -> Result<(Vec<RawMessage>, HashSet<String>)> {
    let token = access_token.to_owned();
    let results: Vec<(String, Option<RawMessage>)> = stream::iter(ids)
        .map(|id| {
            let client = client.clone();
            let token = token.clone();
            async move {
                let mut message_url = url(&["messages", &id])?;
                message_url.query_pairs_mut().append_pair("format", "raw");
                let Some(response) = get_allow_not_found(&client, message_url, &token).await?
                else {
                    return Ok::<_, Error>((id, None));
                };
                let message = response.json().await.map_err(|error| Error::Backend {
                    backend: "gmail-message".into(),
                    message: error.to_string(),
                })?;
                Ok::<_, Error>((id, Some(message)))
            }
        })
        .buffer_unordered(8)
        .try_collect()
        .await?;
    let mut fetched = Vec::new();
    let mut not_found = HashSet::new();
    for (id, message) in results {
        match message {
            Some(message) => fetched.push(message),
            None => {
                not_found.insert(id);
            }
        }
    }
    Ok((fetched, not_found))
}

fn project_messages(
    fetched: Vec<RawMessage>,
    included: &HashSet<String>,
) -> Result<Vec<DiscoveredMessage>> {
    let mut messages = Vec::new();
    for message in fetched {
        let raw = GMAIL_RAW_B64
            .decode(message.raw.as_bytes())
            .map_err(|error| Error::Backend {
                backend: "gmail-message".into(),
                message: format!("{}: raw не декодирован: {error}", message.id),
            })?;
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
    Ok(messages)
}

pub async fn discover(
    access_token: &str,
    cursors: &HashMap<String, FolderSyncCursor>,
) -> Result<ImapDiscovery> {
    let client = client()?;
    let mut folders = discover_folders(access_token).await?;
    let included: HashSet<String> = folders
        .iter()
        .map(|folder| folder.remote_path.clone())
        .collect();
    let cursor = cursors
        .values()
        .find_map(|cursor| cursor.sync_token.as_deref());
    let (ids, sync_token, remote_snapshot, changed_remote_ids) = if let Some(cursor) = cursor {
        match history_delta(&client, access_token, cursor).await? {
            Some((ids, sync_token)) => (ids.clone(), sync_token, None, ids),
            None => {
                let sync_token = profile_history_id(&client, access_token).await?;
                let ids = list_all_message_ids(&client, access_token).await?;
                (ids.clone(), sync_token, Some(ids), Vec::new())
            }
        }
    } else {
        // Read the baseline before the snapshot. Changes racing with this full
        // load then remain visible to the next history request.
        let sync_token = profile_history_id(&client, access_token).await?;
        let ids = list_all_message_ids(&client, access_token).await?;
        (ids.clone(), sync_token, Some(ids), Vec::new())
    };
    let (fetched, not_found) = fetch_messages(&client, access_token, ids).await?;
    let remote_snapshot = remote_snapshot.map(|ids| {
        ids.into_iter()
            .filter(|id| !not_found.contains(id))
            .collect()
    });
    let messages = project_messages(fetched, &included)?;
    for folder in &mut folders {
        folder.sync_token = Some(sync_token.clone());
    }
    Ok(ImapDiscovery {
        folders,
        messages,
        server_uids: Vec::new(),
        reset_folders: Vec::new(),
        remote_snapshot,
        changed_remote_ids,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn history_message(id: &str) -> HistoryMessage {
        HistoryMessage {
            message: MessageRef { id: id.into() },
        }
    }

    #[test]
    fn history_collects_every_change_shape_without_duplicates() {
        let ids = collect_history_ids(vec![HistoryRecord {
            messages: vec![MessageRef {
                id: "direct".into(),
            }],
            messages_added: vec![history_message("added")],
            messages_deleted: vec![history_message("deleted")],
            labels_added: vec![history_message("labels"), history_message("direct")],
            labels_removed: vec![history_message("labels")],
        }]);
        assert_eq!(ids.len(), 4);
        assert!(ids.contains("direct"));
        assert!(ids.contains("added"));
        assert!(ids.contains("deleted"));
        assert!(ids.contains("labels"));
    }

    #[test]
    fn gmail_message_is_projected_to_labels_and_all_mail() {
        let included = HashSet::from(["INBOX".into(), "ALL".into(), "TRASH".into()]);
        let raw = b"Subject: test\r\n\r\nbody";
        let projected = project_messages(
            vec![RawMessage {
                id: "remote-1".into(),
                label_ids: vec!["INBOX".into(), "UNREAD".into()],
                raw: GMAIL_RAW_B64.encode(raw),
                size_estimate: raw.len() as u32,
            }],
            &included,
        )
        .expect("project message");
        assert_eq!(projected.len(), 2);
        assert_eq!(
            projected
                .iter()
                .map(|message| message.folder_path.as_str())
                .collect::<HashSet<_>>(),
            HashSet::from(["INBOX", "ALL"])
        );
        assert!(projected.iter().all(|message| !message.seen));
        assert!(
            projected
                .iter()
                .all(|message| message.remote_id.as_deref() == Some("remote-1"))
        );
    }

    #[test]
    fn malformed_raw_aborts_delta_instead_of_deleting_local_message() {
        let error = project_messages(
            vec![RawMessage {
                id: "remote-1".into(),
                label_ids: vec!["INBOX".into()],
                raw: "%%%".into(),
                size_estimate: 3,
            }],
            &HashSet::from(["INBOX".into()]),
        )
        .expect_err("invalid raw must fail the whole sync");
        assert!(error.to_string().contains("raw не декодирован"));
    }
}
