//! JMAP Core + Mail transport (RFC 8620 / RFC 8621).

use super::{
    DiscoveredFolder, DiscoveredMessage, FolderSyncCursor, ImapDiscovery, MailBackend,
    OutgoingMessage,
};
use crate::model::FolderRole;
use crate::{Error, Result};
use async_trait::async_trait;
use futures::{StreamExt, TryStreamExt, stream};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use url::Url;

const CORE: &str = "urn:ietf:params:jmap:core";
const MAIL: &str = "urn:ietf:params:jmap:mail";
const SUBMISSION: &str = "urn:ietf:params:jmap:submission";
const MAX_EMAILS_PER_MAILBOX: usize = 500;

#[derive(Debug, Clone)]
pub struct JmapBackend {
    pub session_url: String,
    pub username: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Session {
    capabilities: HashMap<String, Value>,
    primary_accounts: HashMap<String, String>,
    api_url: String,
    download_url: String,
    upload_url: String,
    event_source_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Mailbox {
    id: String,
    name: String,
    role: Option<String>,
    #[serde(default)]
    total_emails: i64,
    #[serde(default)]
    unread_emails: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JmapCursor {
    query_state: String,
    email_state: String,
}

#[derive(Debug)]
struct QueryResult {
    ids: Vec<String>,
    query_state: String,
    total: usize,
    full: bool,
    removed: Vec<String>,
}

fn backend_error(scope: &str, message: impl std::fmt::Display) -> Error {
    Error::Backend {
        backend: format!("jmap-{scope}"),
        message: message.to_string(),
    }
}

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(90))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|error| backend_error("client", error))
}

fn validate_session_url(value: &str) -> Result<Url> {
    let url = Url::parse(value.trim()).map_err(|error| backend_error("session", error))?;
    let loopback = url
        .host_str()
        .is_some_and(|host| matches!(host, "127.0.0.1" | "localhost"));
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err(Error::AccountConfig(
            "JMAP Session URL должен использовать HTTPS".into(),
        ));
    }
    Ok(url)
}

impl JmapBackend {
    async fn session(&self, password: &str) -> Result<Session> {
        let url = validate_session_url(&self.session_url)?;
        let response = client()?
            .get(url)
            .basic_auth(&self.username, Some(password))
            .send()
            .await
            .map_err(|error| backend_error("session", error))?;
        response_json(response, "session").await
    }

    async fn api(&self, session: &Session, password: &str, calls: Vec<Value>) -> Result<Value> {
        let mut using = vec![CORE, MAIL];
        if session.capabilities.contains_key(SUBMISSION) {
            using.push(SUBMISSION);
        }
        let response = client()?
            .post(&session.api_url)
            .basic_auth(&self.username, Some(password))
            .json(&json!({
                "using": using,
                "methodCalls": calls,
            }))
            .send()
            .await
            .map_err(|error| backend_error("api", error))?;
        response_json(response, "api").await
    }

    fn mail_account<'a>(&self, session: &'a Session) -> Result<&'a str> {
        session
            .primary_accounts
            .get(MAIL)
            .map(String::as_str)
            .ok_or_else(|| backend_error("session", "сервер не объявил primary mail account"))
    }

    fn submission_account<'a>(&self, session: &'a Session) -> Result<&'a str> {
        session
            .primary_accounts
            .get(SUBMISSION)
            .or_else(|| session.primary_accounts.get(MAIL))
            .map(String::as_str)
            .ok_or_else(|| backend_error("submission", "нет primary submission account"))
    }

    async fn mailboxes(&self, session: &Session, password: &str) -> Result<Vec<Mailbox>> {
        let account_id = self.mail_account(session)?;
        let response = self
            .api(
                session,
                password,
                vec![json!(["Mailbox/get", {
                    "accountId": account_id,
                    "properties": ["id", "name", "role", "totalEmails", "unreadEmails"]
                }, "mailboxes"])],
            )
            .await?;
        serde_json::from_value(method_args(&response, "mailboxes")?["list"].clone())
            .map_err(|error| backend_error("mailboxes", error))
    }

    async fn query_mailbox(
        &self,
        session: &Session,
        password: &str,
        account_id: &str,
        mailbox_id: &str,
        cursor: Option<&JmapCursor>,
    ) -> Result<QueryResult> {
        if let Some(cursor) = cursor {
            let response = self
                .api(
                    session,
                    password,
                    vec![json!(["Email/queryChanges", {
                        "accountId": account_id,
                        "filter": {"inMailbox": mailbox_id},
                        "sort": [{"property": "receivedAt", "isAscending": false}],
                        "sinceQueryState": cursor.query_state,
                        "maxChanges": MAX_EMAILS_PER_MAILBOX
                    }, "query"])],
                )
                .await?;
            if let Ok(args) = method_args(&response, "query") {
                let ids = args["added"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|item| item["id"].as_str().map(str::to_owned))
                    .collect();
                let removed = string_array(&args["removed"]);
                return Ok(QueryResult {
                    ids,
                    query_state: required_string(args, "newQueryState")?,
                    total: 0,
                    full: false,
                    removed,
                });
            }
        }

        let response = self
            .api(
                session,
                password,
                vec![json!(["Email/query", {
                    "accountId": account_id,
                    "filter": {"inMailbox": mailbox_id},
                    "sort": [{"property": "receivedAt", "isAscending": false}],
                    "limit": MAX_EMAILS_PER_MAILBOX,
                    "calculateTotal": true
                }, "query"])],
            )
            .await?;
        let args = method_args(&response, "query")?;
        let ids = string_array(&args["ids"]);
        let total = args["total"].as_u64().unwrap_or(ids.len() as u64) as usize;
        Ok(QueryResult {
            full: ids.len() == total,
            total,
            ids,
            query_state: required_string(args, "queryState")?,
            removed: Vec::new(),
        })
    }

    async fn email_changes(
        &self,
        session: &Session,
        password: &str,
        account_id: &str,
        since_state: Option<&str>,
    ) -> Result<(HashSet<String>, Option<String>)> {
        let Some(since_state) = since_state else {
            return Ok((HashSet::new(), None));
        };
        let response = self
            .api(
                session,
                password,
                vec![json!(["Email/changes", {
                    "accountId": account_id,
                    "sinceState": since_state,
                    "maxChanges": 2000
                }, "changes"])],
            )
            .await?;
        let Ok(args) = method_args(&response, "changes") else {
            return Ok((HashSet::new(), None));
        };
        let ids = ["created", "updated", "destroyed"]
            .into_iter()
            .flat_map(|key| string_array(&args[key]))
            .collect();
        Ok((ids, args["newState"].as_str().map(str::to_owned)))
    }

    async fn get_emails(
        &self,
        session: &Session,
        password: &str,
        account_id: &str,
        ids: &[String],
    ) -> Result<(Vec<Value>, String)> {
        if ids.is_empty() {
            let response = self
                .api(
                    session,
                    password,
                    vec![json!(["Email/get", {
                        "accountId": account_id,
                        "ids": [],
                        "properties": ["id"]
                    }, "emails"])],
                )
                .await?;
            let args = method_args(&response, "emails")?;
            return Ok((Vec::new(), required_string(args, "state")?));
        }
        let mut values = Vec::new();
        let mut state = None;
        for chunk in ids.chunks(50) {
            let response = self
                .api(
                    session,
                    password,
                    vec![json!(["Email/get", {
                        "accountId": account_id,
                        "ids": chunk,
                        "properties": ["id", "blobId", "mailboxIds", "keywords", "size"]
                    }, "emails"])],
                )
                .await?;
            let args = method_args(&response, "emails")?;
            state = args["state"].as_str().map(str::to_owned).or(state);
            values.extend(args["list"].as_array().cloned().unwrap_or_default());
        }
        Ok((
            values,
            state.ok_or_else(|| backend_error("emails", "Email/get не вернул state"))?,
        ))
    }

    async fn download_raw(
        &self,
        session: &Session,
        password: &str,
        account_id: &str,
        blob_id: &str,
    ) -> Result<Vec<u8>> {
        let url = expand_download_url(&session.download_url, account_id, blob_id);
        let response = client()?
            .get(url)
            .basic_auth(&self.username, Some(password))
            .header(reqwest::header::ACCEPT, "message/rfc822")
            .send()
            .await
            .map_err(|error| backend_error("download", error))?;
        response_bytes(response, "download").await
    }

    async fn discover_scope(
        &self,
        password: &str,
        cursors: &HashMap<String, FolderSyncCursor>,
        inbox_only: bool,
    ) -> Result<ImapDiscovery> {
        let session = self.session(password).await?;
        let account_id = self.mail_account(&session)?.to_owned();
        let mailboxes = self.mailboxes(&session, password).await?;
        let selected: Vec<_> = mailboxes
            .iter()
            .filter(|mailbox| !inbox_only || mailbox.role.as_deref() == Some("inbox"))
            .collect();
        if selected.is_empty() {
            return Err(backend_error("mailboxes", "папка Входящие не найдена"));
        }

        let parsed_cursors: HashMap<_, _> = cursors
            .iter()
            .filter_map(|(id, cursor)| {
                serde_json::from_str::<JmapCursor>(cursor.sync_token.as_deref()?)
                    .ok()
                    .map(|parsed| (id.as_str(), parsed))
            })
            .collect();
        let previous_email_state = parsed_cursors
            .values()
            .next()
            .map(|cursor| cursor.email_state.as_str());
        let (mut changed_ids, changed_state) = self
            .email_changes(&session, password, &account_id, previous_email_state)
            .await?;

        let mut query_results = HashMap::new();
        let mut fetch_ids = HashSet::new();
        let mut all_full = !inbox_only;
        let mut snapshot = HashSet::new();
        let mut server_uids = Vec::new();
        for mailbox in &selected {
            let result = self
                .query_mailbox(
                    &session,
                    password,
                    &account_id,
                    &mailbox.id,
                    parsed_cursors.get(mailbox.id.as_str()),
                )
                .await?;
            fetch_ids.extend(result.ids.iter().cloned());
            changed_ids.extend(result.ids.iter().cloned());
            changed_ids.extend(result.removed.iter().cloned());
            if result.full {
                snapshot.extend(result.ids.iter().cloned());
                server_uids.push((
                    mailbox.id.clone(),
                    result.ids.iter().map(|id| stable_uid(id)).collect(),
                ));
            } else {
                all_full = false;
            }
            let _ = result.total;
            query_results.insert(mailbox.id.clone(), result);
        }
        fetch_ids.extend(changed_ids.iter().cloned());
        let fetch_ids: Vec<_> = fetch_ids.into_iter().collect();
        let (email_values, get_state) = self
            .get_emails(&session, password, &account_id, &fetch_ids)
            .await?;
        let email_state = changed_state.unwrap_or(get_state);

        let downloads = stream::iter(email_values.into_iter().map(|value| {
            let backend = self.clone();
            let session = session.clone();
            let password = password.to_owned();
            let account_id = account_id.clone();
            async move {
                let id = required_string(&value, "id")?;
                let blob_id = required_string(&value, "blobId")?;
                let raw = backend
                    .download_raw(&session, &password, &account_id, &blob_id)
                    .await?;
                Ok::<_, Error>((value, id, raw))
            }
        }))
        .buffer_unordered(8)
        .try_collect::<Vec<_>>()
        .await?;

        let mut messages = Vec::new();
        for (value, id, raw) in downloads {
            let mailbox_ids = value["mailboxIds"].as_object();
            let keywords = value["keywords"].as_object();
            for mailbox_id in mailbox_ids
                .into_iter()
                .flatten()
                .filter_map(|(id, enabled)| enabled.as_bool().unwrap_or(false).then_some(id))
            {
                messages.push(DiscoveredMessage {
                    folder_path: mailbox_id.to_owned(),
                    uid: stable_uid(&id),
                    remote_id: Some(id.clone()),
                    size: value["size"]
                        .as_u64()
                        .and_then(|size| u32::try_from(size).ok()),
                    seen: keyword(keywords, "$seen"),
                    flagged: keyword(keywords, "$flagged"),
                    answered: keyword(keywords, "$answered"),
                    draft: keyword(keywords, "$draft"),
                    raw: raw.clone(),
                });
            }
        }

        let folders = mailboxes
            .into_iter()
            .map(|mailbox| {
                let sync_token = query_results.get(mailbox.id.as_str()).map(|result| {
                    serde_json::to_string(&JmapCursor {
                        query_state: result.query_state.clone(),
                        email_state: email_state.clone(),
                    })
                    .expect("JMAP cursor JSON")
                });
                DiscoveredFolder {
                    remote_path: mailbox.id,
                    display_name: mailbox.name,
                    role: mailbox.role.as_deref().and_then(jmap_role),
                    unread_count: mailbox.unread_emails,
                    total_count: mailbox.total_emails,
                    uidvalidity: None,
                    uidnext: None,
                    highestmodseq: None,
                    sync_token,
                }
            })
            .collect();

        Ok(ImapDiscovery {
            folders,
            messages,
            server_uids,
            reset_folders: Vec::new(),
            remote_snapshot: all_full.then(|| snapshot.into_iter().collect()),
            changed_remote_ids: changed_ids.into_iter().collect(),
        })
    }

    async fn email_set(&self, password: &str, update: Value, destroy: Vec<String>) -> Result<()> {
        let session = self.session(password).await?;
        let account_id = self.mail_account(&session)?;
        let response = self
            .api(
                &session,
                password,
                vec![json!(["Email/set", {
                    "accountId": account_id,
                    "update": update,
                    "destroy": destroy
                }, "set"])],
            )
            .await?;
        let args = method_args(&response, "set")?;
        ensure_set_succeeded(args, "Email/set")
    }

    async fn send_message(&self, password: &str, message: OutgoingMessage) -> Result<()> {
        let session = self.session(password).await?;
        if !session.capabilities.contains_key(SUBMISSION) {
            return Err(backend_error(
                "submission",
                "сервер не объявил JMAP Submission capability",
            ));
        }
        let account_id = self.mail_account(&session)?.to_owned();
        let submission_account_id = self.submission_account(&session)?.to_owned();
        let raw = super::smtp::build_message(message)?.formatted();
        let upload_url = expand_upload_url(&session.upload_url, &account_id);
        let upload = client()?
            .post(upload_url)
            .basic_auth(&self.username, Some(password))
            .header(reqwest::header::CONTENT_TYPE, "message/rfc822")
            .body(raw)
            .send()
            .await
            .map_err(|error| backend_error("upload", error))?;
        let upload: Value = response_json(upload, "upload").await?;
        let blob_id = required_string(&upload, "blobId")?;
        let mailboxes = self.mailboxes(&session, password).await?;
        let drafts = mailboxes
            .iter()
            .find(|mailbox| mailbox.role.as_deref() == Some("drafts"))
            .or_else(|| mailboxes.first())
            .ok_or_else(|| backend_error("submission", "нет доступного mailbox для import"))?;
        let sent = mailboxes
            .iter()
            .find(|mailbox| mailbox.role.as_deref() == Some("sent"));

        let import = self
            .api(
                &session,
                password,
                vec![json!(["Email/import", {
                    "accountId": account_id,
                    "emails": {"draft": {
                        "blobId": blob_id,
                        "mailboxIds": {drafts.id.clone(): true},
                        "keywords": {"$draft": true}
                    }}
                }, "import"])],
            )
            .await?;
        let imported = method_args(&import, "import")?;
        ensure_set_succeeded(imported, "Email/import")?;
        let email_id = required_string(&imported["created"]["draft"], "id")?;

        let identities = self
            .api(
                &session,
                password,
                vec![json!(["Identity/get", {"accountId": submission_account_id}, "identities"])],
            )
            .await?;
        let identity_id = method_args(&identities, "identities")?["list"]
            .as_array()
            .and_then(|list| list.first())
            .and_then(|identity| identity["id"].as_str())
            .ok_or_else(|| backend_error("submission", "Identity/get не вернул identity"))?;
        let mut success_patch = serde_json::Map::new();
        success_patch.insert(
            format!("mailboxIds/{}", patch_segment(&drafts.id)),
            Value::Null,
        );
        success_patch.insert("keywords/$draft".into(), Value::Null);
        if let Some(sent) = sent {
            success_patch.insert(
                format!("mailboxIds/{}", patch_segment(&sent.id)),
                Value::Bool(true),
            );
        }
        let submission = self
            .api(
                &session,
                password,
                vec![json!(["EmailSubmission/set", {
                    "accountId": submission_account_id,
                    "create": {"send": {"emailId": email_id, "identityId": identity_id}},
                    "onSuccessUpdateEmail": {"#send": Value::Object(success_patch)}
                }, "submission"])],
            )
            .await?;
        ensure_set_succeeded(
            method_args(&submission, "submission")?,
            "EmailSubmission/set",
        )
    }
}

#[async_trait]
impl MailBackend for JmapBackend {
    fn provider_id(&self) -> &'static str {
        "jmap"
    }

    async fn validate(&self, _email: &str, credential: &str) -> Result<()> {
        let session = self.session(credential).await?;
        if !session.capabilities.contains_key(MAIL) {
            return Err(backend_error(
                "session",
                "сервер не объявил JMAP Mail capability",
            ));
        }
        self.mail_account(&session).map(|_| ())
    }

    async fn discover(
        &self,
        _email: &str,
        credential: &str,
        cursors: &HashMap<String, FolderSyncCursor>,
    ) -> Result<ImapDiscovery> {
        self.discover_scope(credential, cursors, false).await
    }

    async fn discover_folders(
        &self,
        _email: &str,
        credential: &str,
    ) -> Result<Vec<DiscoveredFolder>> {
        let session = self.session(credential).await?;
        Ok(self
            .mailboxes(&session, credential)
            .await?
            .into_iter()
            .map(|mailbox| DiscoveredFolder {
                remote_path: mailbox.id,
                display_name: mailbox.name,
                role: mailbox.role.as_deref().and_then(jmap_role),
                unread_count: mailbox.unread_emails,
                total_count: mailbox.total_emails,
                uidvalidity: None,
                uidnext: None,
                highestmodseq: None,
                sync_token: None,
            })
            .collect())
    }

    async fn discover_inbox(
        &self,
        _email: &str,
        credential: &str,
        cursors: &HashMap<String, FolderSyncCursor>,
    ) -> Result<ImapDiscovery> {
        self.discover_scope(credential, cursors, true).await
    }

    async fn apply_operation(
        &self,
        _email: &str,
        credential: &str,
        operation: &str,
        payload: &str,
    ) -> Result<()> {
        let payload: Value = serde_json::from_str(payload)?;
        let id = payload["remote_id"]
            .as_str()
            .ok_or_else(|| Error::AccountConfig("JMAP outbox: нет remote_id".into()))?;
        let mut patch = serde_json::Map::new();
        match operation {
            "flag" => {
                patch.insert(
                    "keywords/$seen".into(),
                    payload["seen"]
                        .as_bool()
                        .map(Value::Bool)
                        .unwrap_or(Value::Null),
                );
            }
            "move" => {
                let source = payload["folder_path"]
                    .as_str()
                    .ok_or_else(|| Error::AccountConfig("JMAP outbox: нет folder_path".into()))?;
                let target = payload["target_folder_path"].as_str().ok_or_else(|| {
                    Error::AccountConfig("JMAP outbox: нет target_folder_path".into())
                })?;
                patch.insert(format!("mailboxIds/{}", patch_segment(source)), Value::Null);
                patch.insert(
                    format!("mailboxIds/{}", patch_segment(target)),
                    Value::Bool(true),
                );
            }
            "delete" => {
                let session = self.session(credential).await?;
                let trash = self
                    .mailboxes(&session, credential)
                    .await?
                    .into_iter()
                    .find(|mailbox| mailbox.role.as_deref() == Some("trash"));
                let source = payload["folder_path"].as_str();
                if source == trash.as_ref().map(|mailbox| mailbox.id.as_str()) || trash.is_none() {
                    return self
                        .email_set(credential, json!({}), vec![id.to_owned()])
                        .await;
                }
                if let Some(source) = source {
                    patch.insert(format!("mailboxIds/{}", patch_segment(source)), Value::Null);
                }
                let trash = trash.expect("checked above");
                patch.insert(
                    format!("mailboxIds/{}", patch_segment(&trash.id)),
                    Value::Bool(true),
                );
            }
            other => {
                return Err(Error::AccountConfig(format!(
                    "JMAP outbox: неизвестная операция {other}"
                )));
            }
        }
        self.email_set(credential, json!({id: Value::Object(patch)}), Vec::new())
            .await
    }

    async fn rename_folder(
        &self,
        _email: &str,
        credential: &str,
        remote_path: &str,
        new_name: &str,
    ) -> Result<String> {
        let session = self.session(credential).await?;
        let account_id = self.mail_account(&session)?;
        let response = self
            .api(
                &session,
                credential,
                vec![json!(["Mailbox/set", {
                    "accountId": account_id,
                    "update": {remote_path: {"name": new_name}}
                }, "set"])],
            )
            .await?;
        ensure_set_succeeded(method_args(&response, "set")?, "Mailbox/set")?;
        Ok(remote_path.to_owned())
    }

    async fn delete_folder(&self, _email: &str, credential: &str, remote_path: &str) -> Result<()> {
        let session = self.session(credential).await?;
        let account_id = self.mail_account(&session)?;
        let response = self
            .api(
                &session,
                credential,
                vec![json!(["Mailbox/set", {
                    "accountId": account_id,
                    "destroy": [remote_path]
                }, "set"])],
            )
            .await?;
        ensure_set_succeeded(method_args(&response, "set")?, "Mailbox/set")
    }

    async fn wait_for_change(&self, _email: &str, credential: &str) -> Result<()> {
        let session = self.session(credential).await?;
        let Some(template) = session.event_source_url else {
            tokio::time::sleep(Duration::from_secs(30)).await;
            return Ok(());
        };
        let url = template
            .replace("{types}", "Email,Mailbox")
            .replace("{closeafter}", "state")
            .replace("{ping}", "30");
        let response = client()?
            .get(url)
            .basic_auth(&self.username, Some(credential))
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .send()
            .await
            .map_err(|error| backend_error("events", error))?;
        let _ = response_bytes(response, "events").await?;
        Ok(())
    }

    async fn send(&self, message: OutgoingMessage, credential: &str) -> Result<()> {
        self.send_message(credential, message).await
    }

    async fn fetch_message_raw(
        &self,
        _email: &str,
        credential: &str,
        _folder_path: &str,
        _uid: u32,
        remote_id: Option<&str>,
    ) -> Result<Vec<u8>> {
        let id = remote_id.ok_or_else(|| Error::AccountConfig("JMAP: нет remote_id".into()))?;
        let session = self.session(credential).await?;
        let account_id = self.mail_account(&session)?.to_owned();
        let (values, _) = self
            .get_emails(&session, credential, &account_id, &[id.to_owned()])
            .await?;
        let blob_id = values
            .first()
            .and_then(|value| value["blobId"].as_str())
            .ok_or_else(|| backend_error("download", "Email/get не вернул blobId"))?;
        self.download_raw(&session, credential, &account_id, blob_id)
            .await
    }
}

pub async fn probe_session_url(email: &str) -> Option<String> {
    let domain = email.rsplit('@').next()?.trim().to_ascii_lowercase();
    if domain.is_empty() {
        return None;
    }
    let url = format!("https://{domain}/.well-known/jmap");
    let probe = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::limited(3))
        .build()
        .ok()?;
    let response = probe.get(&url).send().await.ok()?;
    let status = response.status();
    let is_jmap_auth = response
        .headers()
        .get(reqwest::header::WWW_AUTHENTICATE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value.to_ascii_lowercase().contains("basic")
                || value.to_ascii_lowercase().contains("bearer")
        });
    (status.is_success() || (status == reqwest::StatusCode::UNAUTHORIZED && is_jmap_auth))
        .then_some(url)
}

async fn response_json<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
    scope: &str,
) -> Result<T> {
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|error| backend_error(scope, error))?;
    if !status.is_success() {
        return Err(backend_error(
            scope,
            format!("HTTP {status}: {}", String::from_utf8_lossy(&body)),
        ));
    }
    serde_json::from_slice(&body).map_err(|error| backend_error(scope, error))
}

async fn response_bytes(response: reqwest::Response, scope: &str) -> Result<Vec<u8>> {
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|error| backend_error(scope, error))?;
    if !status.is_success() {
        return Err(backend_error(
            scope,
            format!("HTTP {status}: {}", String::from_utf8_lossy(&body)),
        ));
    }
    Ok(body.to_vec())
}

fn method_args<'a>(response: &'a Value, tag: &str) -> Result<&'a Value> {
    let methods = response["methodResponses"]
        .as_array()
        .ok_or_else(|| backend_error("api", "нет methodResponses"))?;
    let method = methods
        .iter()
        .find(|method| method[2].as_str() == Some(tag))
        .ok_or_else(|| backend_error("api", format!("нет ответа {tag}")))?;
    if method[0].as_str() == Some("error") {
        return Err(backend_error("api", method[1].to_string()));
    }
    Ok(&method[1])
}

fn required_string(value: &Value, key: &str) -> Result<String> {
    value[key]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| backend_error("response", format!("нет поля {key}")))
}

fn string_array(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item.as_str().map(str::to_owned))
        .collect()
}

fn keyword(keywords: Option<&serde_json::Map<String, Value>>, name: &str) -> bool {
    keywords
        .and_then(|values| values.get(name))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn jmap_role(role: &str) -> Option<FolderRole> {
    match role {
        "inbox" => Some(FolderRole::Inbox),
        "sent" => Some(FolderRole::Sent),
        "drafts" => Some(FolderRole::Drafts),
        "junk" => Some(FolderRole::Spam),
        "trash" => Some(FolderRole::Trash),
        "archive" => Some(FolderRole::Archive),
        _ => None,
    }
}

fn stable_uid(id: &str) -> u32 {
    let digest = Sha256::digest(id.as_bytes());
    u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]).max(1)
}

fn url_component(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

fn expand_download_url(template: &str, account_id: &str, blob_id: &str) -> String {
    template
        .replace("{accountId}", &url_component(account_id))
        .replace("{blobId}", &url_component(blob_id))
        .replace("{name}", "message.eml")
        .replace("{type}", "message%2Frfc822")
}

fn expand_upload_url(template: &str, account_id: &str) -> String {
    template.replace("{accountId}", &url_component(account_id))
}

fn patch_segment(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn ensure_set_succeeded(args: &Value, method: &str) -> Result<()> {
    for key in ["notCreated", "notUpdated", "notDestroyed"] {
        if args[key]
            .as_object()
            .is_some_and(|errors| !errors.is_empty())
        {
            return Err(backend_error(method, args[key].to_string()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Json, Router,
        body::Body,
        extract::State,
        http::{Response, StatusCode},
        routing::{get, post},
    };

    #[test]
    fn expands_jmap_download_template_safely() {
        assert_eq!(
            expand_download_url(
                "https://mail.example/download/{accountId}/{blobId}/{name}?accept={type}",
                "a/1",
                "b+2"
            ),
            "https://mail.example/download/a%2F1/b%2B2/message.eml?accept=message%2Frfc822"
        );
    }

    #[test]
    fn maps_jmap_roles_and_builds_stable_uids() {
        assert_eq!(jmap_role("junk"), Some(FolderRole::Spam));
        assert_eq!(jmap_role("sent"), Some(FolderRole::Sent));
        assert_eq!(stable_uid("M123"), stable_uid("M123"));
        assert_ne!(stable_uid("M123"), 0);
    }

    #[test]
    fn rejects_plaintext_remote_session_urls() {
        assert!(validate_session_url("http://mail.example/.well-known/jmap").is_err());
        assert!(validate_session_url("http://127.0.0.1:8080/jmap").is_ok());
    }

    async fn mock_session(State(base): State<String>) -> Json<Value> {
        Json(json!({
            "capabilities": {CORE: {}, MAIL: {}, SUBMISSION: {}},
            "accounts": {"a1": {"name": "Test", "isPersonal": true, "isReadOnly": false, "accountCapabilities": {MAIL: {}}}},
            "primaryAccounts": {MAIL: "a1", SUBMISSION: "a1"},
            "username": "user@example.test",
            "apiUrl": format!("{base}/api"),
            "downloadUrl": format!("{base}/download/{{accountId}}/{{blobId}}/{{name}}?accept={{type}}"),
            "uploadUrl": format!("{base}/upload/{{accountId}}"),
            "state": "session-1"
        }))
    }

    async fn mock_api(Json(request): Json<Value>) -> Json<Value> {
        let call = &request["methodCalls"][0];
        let name = call[0].as_str().unwrap_or_default();
        let tag = call[2].as_str().unwrap_or_default();
        let args = match name {
            "Mailbox/get" => json!({
                "accountId": "a1",
                "state": "m1",
                "list": [
                    {"id":"inbox-id","name":"Inbox","role":"inbox","totalEmails":1,"unreadEmails":1},
                    {"id":"archive-id","name":"Archive","role":"archive","totalEmails":0,"unreadEmails":0}
                ],
                "notFound": []
            }),
            "Email/query" => json!({
                "accountId": "a1", "queryState": "q1", "canCalculateChanges": true,
                "position": 0, "ids": ["email-1"], "total": 1
            }),
            "Email/get" => json!({
                "accountId": "a1", "state": "e1",
                "list": [{
                    "id":"email-1", "blobId":"blob-1", "mailboxIds":{"inbox-id":true},
                    "keywords":{"$seen":false,"$flagged":true}, "size":74
                }],
                "notFound": []
            }),
            "Email/import" => json!({
                "accountId":"a1", "oldState":"e1", "newState":"e2",
                "created":{"draft":{"id":"email-sent","blobId":"blob-upload","threadId":"thread-1","size":100}}
            }),
            "Identity/get" => json!({
                "accountId":"a1", "state":"i1", "list":[{"id":"identity-1","name":"User","email":"user@example.test"}], "notFound":[]
            }),
            "EmailSubmission/set" => json!({
                "accountId":"a1", "oldState":"s1", "newState":"s2",
                "created":{"send":{"id":"submission-1"}}
            }),
            "Email/set" => json!({
                "accountId":"a1", "oldState":"e1", "newState":"e2",
                "updated":{"email-1":null}, "notUpdated":{}, "notDestroyed":{}
            }),
            _ => json!({"type":"unknownMethod","description":name}),
        };
        let response_name = if matches!(
            name,
            "Mailbox/get"
                | "Email/query"
                | "Email/get"
                | "Email/import"
                | "Identity/get"
                | "EmailSubmission/set"
                | "Email/set"
        ) {
            name
        } else {
            "error"
        };
        Json(json!({
            "methodResponses": [[response_name, args, tag]],
            "sessionState": "session-1"
        }))
    }

    async fn mock_download() -> Response<Body> {
        Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "message/rfc822")
            .body(Body::from(b"From: sender@example.test\r\nTo: user@example.test\r\nSubject: JMAP\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=outer\r\n\r\n--outer\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p><b>Hello</b> from JMAP</p>\r\n--outer\r\nContent-Type: text/plain; name=notes.txt\r\nContent-Disposition: attachment; filename=notes.txt\r\nContent-Transfer-Encoding: base64\r\n\r\nbm90ZXM=\r\n--outer--\r\n".to_vec()))
            .unwrap()
    }

    async fn mock_upload() -> Json<Value> {
        Json(json!({
            "accountId":"a1", "blobId":"blob-upload", "type":"message/rfc822", "size":100
        }))
    }

    #[tokio::test]
    async fn discovers_mailbox_and_raw_message_through_jmap() {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new()
            .route("/.well-known/jmap", get(mock_session))
            .route("/api", post(mock_api))
            .route("/download/a1/blob-1/message.eml", get(mock_download))
            .route("/upload/a1", post(mock_upload))
            .with_state(base.clone());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let backend = JmapBackend {
            session_url: format!("{base}/.well-known/jmap"),
            username: "user@example.test".into(),
        };

        let discovered = backend
            .discover("user@example.test", "app-password", &HashMap::new())
            .await
            .unwrap();

        server.abort();
        assert_eq!(discovered.folders.len(), 2);
        assert!(
            discovered
                .folders
                .iter()
                .any(|folder| folder.role == Some(FolderRole::Inbox))
        );
        assert!(
            discovered
                .folders
                .iter()
                .any(|folder| folder.role == Some(FolderRole::Archive))
        );
        assert!(discovered.folders[0].sync_token.is_some());
        assert_eq!(discovered.messages.len(), 1);
        assert_eq!(discovered.messages[0].remote_id.as_deref(), Some("email-1"));
        assert!(discovered.messages[0].flagged);
        assert!(
            discovered.messages[0]
                .raw
                .windows(b"<b>Hello</b>".len())
                .any(|window| window == b"<b>Hello</b>")
        );
        assert_eq!(discovered.remote_snapshot, Some(vec!["email-1".into()]));
    }

    #[tokio::test]
    async fn uploads_imports_and_submits_outgoing_message() {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new()
            .route("/.well-known/jmap", get(mock_session))
            .route("/api", post(mock_api))
            .route("/upload/a1", post(mock_upload))
            .with_state(base.clone());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let backend = JmapBackend {
            session_url: format!("{base}/.well-known/jmap"),
            username: "user@example.test".into(),
        };
        let message = OutgoingMessage {
            from: "user@example.test".into(),
            to: vec!["recipient@example.test".into()],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "JMAP send".into(),
            body_text: "Hello".into(),
            body_html: None,
            attachments: Vec::new(),
        };

        backend.send(message, "app-password").await.unwrap();

        server.abort();
    }

    #[tokio::test]
    async fn end_to_end_sync_read_send_move_and_undo() {
        use crate::crypto::{DatabaseKey, StorageCrypto};
        use crate::model::{AuthKind, BackendKind, NewAccount, Provider};
        use crate::storage::Db;
        use std::sync::Arc;

        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new()
            .route("/.well-known/jmap", get(mock_session))
            .route("/api", post(mock_api))
            .route("/download/a1/blob-1/message.eml", get(mock_download))
            .route("/upload/a1", post(mock_upload))
            .with_state(base.clone());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let backend = JmapBackend {
            session_url: format!("{base}/.well-known/jmap"),
            username: "user@example.test".into(),
        };
        let root = std::env::temp_dir().join(format!("truemail-jmap-e2e-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let db = Db::open_with_database_key(
            &root,
            Arc::new(StorageCrypto::from_key([7; 32])),
            &DatabaseKey::from_key([9; 32]),
        )
        .await
        .unwrap();
        db.migrate().await.unwrap();
        let account = db
            .save_account(&NewAccount {
                email: "user@example.test".into(),
                display_name: "JMAP E2E".into(),
                provider: Provider::Generic,
                backend_kind: BackendKind::Jmap,
                auth_kind: AuthKind::AppPassword,
                imap: None,
                smtp: None,
                ews_url: None,
                jmap_url: Some(format!("{base}/.well-known/jmap")),
                username: Some("user@example.test".into()),
                secret_ref: "test-keychain-ref".into(),
                color: None,
            })
            .await
            .unwrap();

        let discovery = backend
            .discover("user@example.test", "app-password", &HashMap::new())
            .await
            .unwrap();
        db.save_discovered_folders(account.id, &discovery.folders)
            .await
            .unwrap();
        db.save_discovered_messages(account.id, &discovery.messages)
            .await
            .unwrap();
        db.save_folder_sync_tokens(account.id, &discovery.folders)
            .await
            .unwrap();
        let (message_id,): (i64,) = sqlx::query_as("SELECT id FROM messages WHERE account_id=?")
            .bind(account.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        let message = db.get_message(message_id).await.unwrap();
        assert!(
            message
                .body_html
                .as_deref()
                .unwrap()
                .contains("<b>Hello</b>")
        );
        assert_eq!(message.attachments.len(), 1);
        assert_eq!(message.attachments[0].filename, "notes.txt");

        backend
            .send(
                OutgoingMessage {
                    from: "user@example.test".into(),
                    to: vec!["recipient@example.test".into()],
                    cc: Vec::new(),
                    bcc: Vec::new(),
                    subject: "JMAP E2E send".into(),
                    body_text: "Hello".into(),
                    body_html: Some("<b>Hello</b>".into()),
                    attachments: vec![crate::backend::OutgoingAttachment {
                        filename: "answer.txt".into(),
                        mime_type: "text/plain".into(),
                        data: b"answer".to_vec(),
                    }],
                },
                "app-password",
            )
            .await
            .unwrap();

        let (archive_id,): (i64,) =
            sqlx::query_as("SELECT id FROM folders WHERE account_id=? AND role='archive'")
                .bind(account.id)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        let pending = db
            .queue_message_move(&[message_id], archive_id)
            .await
            .unwrap();
        assert_eq!(
            db.cancel_outbox_operations(&pending.operation_ids)
                .await
                .unwrap(),
            1
        );
        let queued = db
            .queue_message_move(&[message_id], archive_id)
            .await
            .unwrap();
        sqlx::query("UPDATE outbox_ops SET next_attempt_at=datetime('now') WHERE id=?")
            .bind(queued.operation_ids[0])
            .execute(&db.write_pool)
            .await
            .unwrap();
        let operation = db
            .claim_outbox_operations(account.id, 1)
            .await
            .unwrap()
            .remove(0);
        backend
            .apply_operation(
                "user@example.test",
                "app-password",
                &operation.op_kind,
                &operation.payload,
            )
            .await
            .unwrap();
        db.complete_outbox_operation(&operation).await.unwrap();
        let (remaining,): (i64,) = sqlx::query_as("SELECT count(*) FROM messages WHERE id=?")
            .bind(message_id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(remaining, 0);

        db.close().await;
        server.abort();
        std::fs::remove_dir_all(root).unwrap();
    }
}
