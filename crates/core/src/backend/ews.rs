//! Exchange Web Services transport for self-hosted Exchange Server.

use super::{
    DiscoveredFolder, DiscoveredMessage, FolderSyncCursor, ImapDiscovery, MailBackend,
    OutgoingMessage,
};
use crate::account::{
    AuxiliarySyncCursors, ContactInput, DavCalendar, DavCollection, DavContact, DavEvent,
    DavSyncResult, EventInput, SyncScope,
};
use crate::model::{Attendee, ContactPhone, FolderRole, infer_folder_role};
use crate::{Error, Result};
use async_trait::async_trait;
use base64::Engine as _;
use roxmltree::{Document, Node};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

const SOAP_NS: &str = "http://schemas.xmlsoap.org/soap/envelope/";
const MSG_NS: &str = "http://schemas.microsoft.com/exchange/services/2006/messages";
const TYPES_NS: &str = "http://schemas.microsoft.com/exchange/services/2006/types";

#[derive(Debug, Clone)]
pub struct EwsBackend {
    pub endpoint: String,
    pub username: String,
}

#[derive(Debug)]
struct EwsResponse {
    status: u16,
    body: String,
}

const MAIL_SYNC_TOKEN_PREFIX: &str = "ews-sync-v1:";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
struct MailSyncToken {
    hierarchy: Option<String>,
    items: Option<String>,
}

#[derive(Debug, Default)]
struct HierarchySync {
    sync_state: String,
    deleted_folder_ids: Vec<String>,
    initial: bool,
}

#[derive(Debug, Default)]
struct ItemSync {
    sync_state: String,
    changed_ids: Vec<String>,
    deleted_ids: Vec<String>,
    initial: bool,
}

fn encode_mail_sync_token(token: &MailSyncToken) -> Result<String> {
    let json = serde_json::to_vec(token)?;
    Ok(format!(
        "{MAIL_SYNC_TOKEN_PREFIX}{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)
    ))
}

fn decode_mail_sync_token(value: Option<&str>) -> MailSyncToken {
    let Some(value) = value.and_then(|value| value.strip_prefix(MAIL_SYNC_TOKEN_PREFIX)) else {
        return MailSyncToken::default();
    };
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .ok()
        .and_then(|json| serde_json::from_slice(&json).ok())
        .unwrap_or_default()
}

fn backend_error(kind: &str, message: impl ToString) -> Error {
    Error::Backend {
        backend: format!("ews-{kind}"),
        message: message.to_string(),
    }
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// XML-фрагмент t:Updates для UpdateItem операции 'flag': message:IsRead
/// шлём всегда, message:Flag (комплексное свойство с FlagStatus
/// Flagged/NotFlagged) - только если "flagged" явно есть в payload. Flag
/// доступен в EWS начиная с Exchange 2010 - тот же минимум, что и
/// RequestServerVersion=Exchange2010_SP2 в конверте envelope() выше, поэтому
/// используем его напрямую вместо обходных путей вроде Importance. Вынесено
/// в чистую функцию, чтобы протестировать маппинг без реального EWS-запроса.
fn flag_update_fields(seen: bool, flagged: Option<bool>) -> String {
    let mut updates = format!(
        r#"<t:SetItemField><t:FieldURI FieldURI="message:IsRead"/><t:Message><t:IsRead>{seen}</t:IsRead></t:Message></t:SetItemField>"#
    );
    if let Some(flagged) = flagged {
        let flag_status = if flagged { "Flagged" } else { "NotFlagged" };
        updates.push_str(&format!(
            r#"<t:SetItemField><t:FieldURI FieldURI="message:Flag"/><t:Message><t:Flag><t:FlagStatus>{flag_status}</t:FlagStatus></t:Flag></t:Message></t:SetItemField>"#
        ));
    }
    updates
}

/// Тело CreateFolder: ParentFolderId ссылается либо на конкретную папку по
/// Id, либо (без родителя) на distinguished-папку msgfolderroot - корень
/// пользовательских папок почты в Exchange. Вынесено в чистую функцию, чтобы
/// протестировать сборку XML без реального EWS-запроса.
fn create_folder_body(parent_path: Option<&str>, name: &str) -> String {
    let parent = match parent_path {
        Some(id) => format!(r#"<t:FolderId Id="{}"/>"#, escape(id)),
        None => r#"<t:DistinguishedFolderId Id="msgfolderroot"/>"#.to_owned(),
    };
    format!(
        r#"<m:CreateFolder><m:ParentFolderId>{parent}</m:ParentFolderId><m:Folders><t:Folder><t:DisplayName>{}</t:DisplayName></t:Folder></m:Folders></m:CreateFolder>"#,
        escape(name)
    )
}

fn envelope(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="{SOAP_NS}" xmlns:m="{MSG_NS}" xmlns:t="{TYPES_NS}">
 <s:Header><t:RequestServerVersion Version="Exchange2010_SP2"/></s:Header>
 <s:Body>{body}</s:Body>
</s:Envelope>"#
    )
}

fn node_text<'a>(node: Node<'a, 'a>, name: &str) -> Option<&'a str> {
    node.descendants()
        .find(|child| child.is_element() && child.tag_name().name() == name)
        .and_then(|child| child.text())
}

fn response_error(body: &str) -> Option<String> {
    let document = Document::parse(body).ok()?;
    let response = document
        .descendants()
        .find(|node| node.is_element() && node.attribute("ResponseClass") == Some("Error"))?;
    let code = node_text(response, "ResponseCode");
    let text = node_text(response, "MessageText");
    Some(match (code, text) {
        (Some(code), Some(text)) => format!("{code}: {text}"),
        (Some(code), None) => code.to_owned(),
        (None, Some(text)) => text.to_owned(),
        (None, None) => "Exchange вернул ошибку".to_owned(),
    })
}

fn is_invalid_sync_state(error: &Error) -> bool {
    error.to_string().contains("ErrorInvalidSyncStateData")
}

fn sync_state(xml: &str) -> Result<String> {
    let document = Document::parse(xml).map_err(|error| backend_error("sync-xml", error))?;
    document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "SyncState")
        .and_then(|node| node.text())
        .map(str::to_owned)
        .ok_or_else(|| backend_error("sync", "Exchange не вернул SyncState"))
}

fn sync_page_complete(xml: &str, name: &str) -> Result<bool> {
    let document = Document::parse(xml).map_err(|error| backend_error("sync-xml", error))?;
    Ok(document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == name)
        .and_then(|node| node.text())
        .is_none_or(|value| value.eq_ignore_ascii_case("true")))
}

fn change_ids(xml: &str, item_kind: &str) -> Result<(Vec<String>, Vec<String>)> {
    let document = Document::parse(xml).map_err(|error| backend_error("sync-xml", error))?;
    let mut changed = HashSet::new();
    let mut deleted = HashSet::new();
    for action in document.descendants().filter(|node| {
        node.is_element()
            && matches!(
                node.tag_name().name(),
                "Create" | "Update" | "Delete" | "ReadFlagChange"
            )
            && node
                .ancestors()
                .any(|parent| parent.is_element() && parent.tag_name().name() == "Changes")
    }) {
        let Some(id) = action
            .descendants()
            .find(|node| node.is_element() && node.tag_name().name() == item_kind)
            .and_then(|node| node.attribute("Id"))
        else {
            continue;
        };
        if action.tag_name().name() == "Delete" {
            deleted.insert(id.to_owned());
        } else {
            changed.insert(id.to_owned());
        }
    }
    let mut changed = changed.into_iter().collect::<Vec<_>>();
    let mut deleted = deleted.into_iter().collect::<Vec<_>>();
    changed.sort();
    deleted.sort();
    Ok((changed, deleted))
}

#[cfg(windows)]
async fn authenticated_post(
    url: &str,
    username: &str,
    password: &str,
    content_type: &str,
    soap_action: Option<&str>,
    body: &str,
) -> Result<EwsResponse> {
    use winhttp::{AuthScheme, AuthTarget, RedirectPolicy, Session, SessionConfig};

    let url = url::Url::parse(url).map_err(|error| backend_error("url", error))?;
    if url.scheme() != "https" {
        return Err(Error::AccountConfig(
            "Exchange endpoint должен использовать HTTPS".into(),
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| backend_error("url", "в адресе Exchange нет имени сервера"))?;
    let port = url.port_or_known_default().unwrap_or(443);
    let mut path = url.path().to_owned();
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }
    let session = Session::with_config_async(SessionConfig {
        user_agent: "truemail/0.1 Exchange EWS".into(),
        connect_timeout_ms: 10_000,
        send_timeout_ms: 30_000,
        receive_timeout_ms: 30_000,
    })
    .map_err(|error| backend_error("http", error))?;
    let connection = session
        .connect(host, port)
        .map_err(|error| backend_error("http", error))?;
    let mut last = None;
    for scheme in [AuthScheme::NEGOTIATE, AuthScheme::NTLM] {
        let mut builder = connection
            .request("POST", &path)
            .secure()
            .header("Content-Type", content_type)
            .header("Accept", "text/xml");
        if let Some(action) = soap_action {
            builder = builder.header("SOAPAction", action);
        }
        let request = builder
            .build()
            .map_err(|error| backend_error("http", error))?;
        request
            .set_redirect_policy_typed(RedirectPolicy::DisallowHttpsToHttp)
            .map_err(|error| backend_error("http", error))?;
        request
            .set_credentials_typed(AuthTarget::SERVER, scheme, username, password)
            .map_err(|error| backend_error("auth", error))?;
        let response = request
            .into_async()
            .map_err(|error| backend_error("http", error))?
            .send_with_body(body.as_bytes().to_vec())
            .await
            .map_err(|error| backend_error("http", error))?;
        let status = response
            .status_code()
            .map_err(|error| backend_error("http", error))?;
        let bytes = response
            .read_all()
            .await
            .map_err(|error| backend_error("http", error))?;
        let result = EwsResponse {
            status,
            body: String::from_utf8_lossy(&bytes).into_owned(),
        };
        if status != 401 {
            return Ok(result);
        }
        last = Some(result);
    }
    Ok(last.unwrap_or(EwsResponse {
        status: 401,
        body: String::new(),
    }))
}

#[cfg(not(windows))]
async fn authenticated_post(
    _url: &str,
    _username: &str,
    _password: &str,
    _content_type: &str,
    _soap_action: Option<&str>,
    _body: &str,
) -> Result<EwsResponse> {
    Err(Error::AccountConfig(
        "NTLM/Negotiate для self-hosted Exchange сейчас поддерживается в Windows-сборке".into(),
    ))
}

fn direct_ews_url(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if value.to_ascii_lowercase().contains("/ews/") {
        return Some(value.trim_end_matches('/').to_owned());
    }
    let base = if value.starts_with("https://") {
        value.trim_end_matches('/').to_owned()
    } else {
        format!("https://{}", value.trim_end_matches('/'))
    };
    Some(format!("{base}/EWS/Exchange.asmx"))
}

pub async fn discover_ews_url(
    email: &str,
    username: &str,
    password: &str,
    server_hint: Option<&str>,
) -> Result<String> {
    if let Some(hint) = server_hint.filter(|value| value.to_ascii_lowercase().contains("/ews/")) {
        return direct_ews_url(hint).ok_or_else(|| Error::AccountConfig("адрес EWS пуст".into()));
    }
    let domain = email.rsplit('@').next().unwrap_or_default();
    let mut endpoints = Vec::new();
    if let Some(hint) = server_hint.filter(|value| !value.trim().is_empty()) {
        let base = if hint.starts_with("https://") {
            hint.trim_end_matches('/').to_owned()
        } else {
            format!("https://{}", hint.trim_end_matches('/'))
        };
        endpoints.push(format!("{base}/autodiscover/autodiscover.xml"));
    }
    endpoints.push(format!(
        "https://autodiscover.{domain}/autodiscover/autodiscover.xml"
    ));
    endpoints.push(format!("https://{domain}/autodiscover/autodiscover.xml"));
    let request = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<Autodiscover xmlns="http://schemas.microsoft.com/exchange/autodiscover/outlook/requestschema/2006">
 <Request><EMailAddress>{}</EMailAddress><AcceptableResponseSchema>http://schemas.microsoft.com/exchange/autodiscover/outlook/responseschema/2006a</AcceptableResponseSchema></Request>
</Autodiscover>"#,
        escape(email)
    );
    let mut errors = Vec::new();
    for endpoint in endpoints {
        match authenticated_post(
            &endpoint,
            username,
            password,
            "text/xml; charset=utf-8",
            None,
            &request,
        )
        .await
        {
            Ok(response) if (200..300).contains(&response.status) => {
                if let Ok(document) = Document::parse(&response.body)
                    && let Some(url) = document
                        .descendants()
                        .find(|node| node.is_element() && node.tag_name().name() == "EwsUrl")
                        .and_then(|node| node.text())
                {
                    return Ok(url.to_owned());
                }
                errors.push(format!("{endpoint}: ответ не содержит EwsUrl"));
            }
            Ok(response) => errors.push(format!("{endpoint}: HTTP {}", response.status)),
            Err(error) => errors.push(format!("{endpoint}: {error}")),
        }
    }
    if let Some(hint) = server_hint.and_then(direct_ews_url) {
        return Ok(hint);
    }
    Err(backend_error("autodiscover", errors.join("; ")))
}

impl EwsBackend {
    async fn soap(&self, password: &str, action: &str, body: &str) -> Result<String> {
        let soap_action =
            format!("http://schemas.microsoft.com/exchange/services/2006/messages/{action}");
        let response = authenticated_post(
            &self.endpoint,
            &self.username,
            password,
            "text/xml; charset=utf-8",
            Some(&soap_action),
            &envelope(body),
        )
        .await?;
        if !(200..300).contains(&response.status) {
            return Err(backend_error("http", format!("HTTP {}", response.status)));
        }
        if let Some(error) = response_error(&response.body) {
            return Err(backend_error("soap", error));
        }
        Ok(response.body)
    }

    async fn sync_folder_hierarchy_pages(
        &self,
        password: &str,
        initial_state: Option<&str>,
    ) -> Result<HierarchySync> {
        let initial = initial_state.is_none();
        let mut state = initial_state.map(str::to_owned);
        let mut deleted = HashSet::new();
        let mut pages = 0usize;
        loop {
            pages += 1;
            if pages > 10_000 {
                return Err(backend_error(
                    "sync",
                    "SyncFolderHierarchy превысил 10000 страниц",
                ));
            }
            let previous_state = state.clone();
            let state_xml = state
                .as_deref()
                .map(|value| format!("<m:SyncState>{}</m:SyncState>", escape(value)))
                .unwrap_or_default();
            let body = format!(
                r#"<m:SyncFolderHierarchy>
 <m:FolderShape><t:BaseShape>Default</t:BaseShape></m:FolderShape>
 <m:SyncFolderId><t:DistinguishedFolderId Id="msgfolderroot"/></m:SyncFolderId>
 {state_xml}
</m:SyncFolderHierarchy>"#
            );
            let response = self.soap(password, "SyncFolderHierarchy", &body).await?;
            state = Some(sync_state(&response)?);
            let (_, page_deleted) = change_ids(&response, "FolderId")?;
            deleted.extend(page_deleted);
            if sync_page_complete(&response, "IncludesLastFolderInRange")? {
                let mut deleted_folder_ids = deleted.into_iter().collect::<Vec<_>>();
                deleted_folder_ids.sort();
                return Ok(HierarchySync {
                    sync_state: state.unwrap_or_default(),
                    deleted_folder_ids,
                    initial,
                });
            }
            if state == previous_state {
                return Err(backend_error(
                    "sync",
                    "SyncFolderHierarchy не продвинул SyncState",
                ));
            }
        }
    }

    async fn sync_folder_hierarchy(
        &self,
        password: &str,
        state: Option<&str>,
    ) -> Result<HierarchySync> {
        match self.sync_folder_hierarchy_pages(password, state).await {
            Err(error) if state.is_some() && is_invalid_sync_state(&error) => {
                self.sync_folder_hierarchy_pages(password, None).await
            }
            result => result,
        }
    }

    async fn sync_folder_items_pages(
        &self,
        password: &str,
        folder_id_xml: &str,
        initial_state: Option<&str>,
    ) -> Result<ItemSync> {
        let initial = initial_state.is_none();
        let mut state = initial_state.map(str::to_owned);
        let mut changed = HashSet::new();
        let mut deleted = HashSet::new();
        let mut pages = 0usize;
        loop {
            pages += 1;
            if pages > 10_000 {
                return Err(backend_error(
                    "sync",
                    "SyncFolderItems превысил 10000 страниц",
                ));
            }
            let previous_state = state.clone();
            let state_xml = state
                .as_deref()
                .map(|value| format!("<m:SyncState>{}</m:SyncState>", escape(value)))
                .unwrap_or_default();
            let body = format!(
                r#"<m:SyncFolderItems>
 <m:ItemShape><t:BaseShape>IdOnly</t:BaseShape></m:ItemShape>
 <m:SyncFolderId>{folder_id_xml}</m:SyncFolderId>
 {state_xml}
 <m:MaxChangesReturned>512</m:MaxChangesReturned>
 <m:SyncScope>NormalItems</m:SyncScope>
</m:SyncFolderItems>"#
            );
            let response = self.soap(password, "SyncFolderItems", &body).await?;
            state = Some(sync_state(&response)?);
            let (page_changed, page_deleted) = change_ids(&response, "ItemId")?;
            changed.extend(page_changed);
            deleted.extend(page_deleted);
            if sync_page_complete(&response, "IncludesLastItemInRange")? {
                let mut changed_ids = changed.into_iter().collect::<Vec<_>>();
                let mut deleted_ids = deleted.into_iter().collect::<Vec<_>>();
                changed_ids.sort();
                deleted_ids.sort();
                return Ok(ItemSync {
                    sync_state: state.unwrap_or_default(),
                    changed_ids,
                    deleted_ids,
                    initial,
                });
            }
            if state == previous_state {
                return Err(backend_error(
                    "sync",
                    "SyncFolderItems не продвинул SyncState",
                ));
            }
        }
    }

    async fn sync_folder_items(
        &self,
        password: &str,
        folder_id_xml: &str,
        state: Option<&str>,
    ) -> Result<ItemSync> {
        match self
            .sync_folder_items_pages(password, folder_id_xml, state)
            .await
        {
            Err(error) if state.is_some() && is_invalid_sync_state(&error) => {
                self.sync_folder_items_pages(password, folder_id_xml, None)
                    .await
            }
            result => result,
        }
    }

    async fn messages_by_ids(
        &self,
        password: &str,
        folder_path: &str,
        ids: &[String],
    ) -> Result<Vec<DiscoveredMessage>> {
        let mut messages = Vec::new();
        for chunk in ids.chunks(50) {
            let item_ids = chunk
                .iter()
                .map(|id| format!(r#"<t:ItemId Id="{}"/>"#, escape(id)))
                .collect::<String>();
            let body = format!(
                r#"<m:GetItem><m:ItemShape><t:BaseShape>AllProperties</t:BaseShape><t:IncludeMimeContent>true</t:IncludeMimeContent></m:ItemShape><m:ItemIds>{item_ids}</m:ItemIds></m:GetItem>"#
            );
            let response = self.soap(password, "GetItem", &body).await?;
            messages.extend(parse_messages(&response, folder_path)?);
        }
        Ok(messages)
    }

    async fn recent_messages_in_folder(
        &self,
        password: &str,
        folder: &DiscoveredFolder,
        limit: usize,
        retention_days: Option<i64>,
    ) -> Result<Vec<DiscoveredMessage>> {
        let restriction = retention_days
            .filter(|days| *days > 0)
            .map(|days| {
                let since = chrono::Utc::now() - chrono::Duration::days(days);
                format!(
                    r#"<m:Restriction><t:IsGreaterThanOrEqualTo><t:FieldURI FieldURI="item:DateTimeReceived"/><t:FieldURIOrConstant><t:Constant Value="{}"/></t:FieldURIOrConstant></t:IsGreaterThanOrEqualTo></m:Restriction>"#,
                    since.format("%Y-%m-%dT%H:%M:%SZ")
                )
            })
            .unwrap_or_default();
        let body = format!(
            r#"<m:FindItem Traversal="Shallow"><m:ItemShape><t:BaseShape>IdOnly</t:BaseShape></m:ItemShape><m:IndexedPageItemView MaxEntriesReturned="{}" Offset="0" BasePoint="Beginning"/><m:SortOrder><t:FieldOrder Order="Descending"><t:FieldURI FieldURI="item:DateTimeReceived"/></t:FieldOrder></m:SortOrder>{restriction}<m:ParentFolderIds><t:FolderId Id="{}"/></m:ParentFolderIds></m:FindItem>"#,
            limit,
            escape(&folder.remote_path)
        );
        let response = self.soap(password, "FindItem", &body).await?;
        let ids = parse_item_ids(&response)?;
        self.messages_by_ids(password, &folder.remote_path, &ids)
            .await
    }

    async fn folders(&self, password: &str) -> Result<Vec<DiscoveredFolder>> {
        let body = r#"<m:FindFolder Traversal="Deep"><m:FolderShape><t:BaseShape>Default</t:BaseShape></m:FolderShape><m:ParentFolderIds><t:DistinguishedFolderId Id="msgfolderroot"/></m:ParentFolderIds></m:FindFolder>"#;
        let response = self.soap(password, "FindFolder", body).await?;
        parse_folders(&response)
    }

    async fn discover_incremental(
        &self,
        password: &str,
        cursors: &HashMap<String, FolderSyncCursor>,
        retention_days: i64,
    ) -> Result<ImapDiscovery> {
        let hierarchy_state = cursors
            .values()
            .map(|cursor| decode_mail_sync_token(cursor.sync_token.as_deref()))
            .find_map(|token| token.hierarchy);
        let hierarchy = self
            .sync_folder_hierarchy(password, hierarchy_state.as_deref())
            .await?;
        let mut folders = self.folders(password).await?;
        let current_folder_ids = folders
            .iter()
            .map(|folder| folder.remote_path.clone())
            .collect::<HashSet<_>>();
        let mut deleted_folder_ids = hierarchy
            .deleted_folder_ids
            .into_iter()
            .collect::<HashSet<_>>();
        if hierarchy.initial {
            deleted_folder_ids.extend(
                cursors
                    .keys()
                    .filter(|id| !current_folder_ids.contains(*id))
                    .cloned(),
            );
        }

        let mut messages = Vec::new();
        let mut server_uids = deleted_folder_ids
            .iter()
            .cloned()
            .map(|id| (id, Vec::new()))
            .collect::<Vec<_>>();
        let mut reset_folders = deleted_folder_ids.into_iter().collect::<Vec<_>>();
        let mut changed_remote_ids = HashSet::new();
        for folder in &mut folders {
            let folder_started = std::time::Instant::now();
            let previous = decode_mail_sync_token(
                cursors
                    .get(&folder.remote_path)
                    .and_then(|cursor| cursor.sync_token.as_deref()),
            );
            let folder_xml = format!(r#"<t:FolderId Id="{}"/>"#, escape(&folder.remote_path));
            let sync = match self
                .sync_folder_items(password, &folder_xml, previous.items.as_deref())
                .await
            {
                Ok(sync) => sync,
                Err(error) => {
                    tracing::warn!(folder = %folder.display_name, %error, "EWS: инкрементальная синхронизация папки пропущена");
                    continue;
                }
            };
            changed_remote_ids.extend(sync.deleted_ids.iter().cloned());
            let mut found = if sync.initial {
                self.recent_messages_in_folder(password, folder, 500, Some(retention_days))
                    .await?
            } else {
                self.messages_by_ids(password, &folder.remote_path, &sync.changed_ids)
                    .await?
            };
            let changed_ids_count = sync.changed_ids.len();
            let deleted_ids_count = sync.deleted_ids.len();
            let downloaded_count = found.len();
            let unchanged =
                changed_ids_count == 0 && deleted_ids_count == 0 && downloaded_count == 0;
            if unchanged {
                tracing::debug!(
                    provider = "exchange-ews",
                    collection = %folder.display_name,
                    scope = if sync.initial { "full-reconcile" } else { "delta" },
                    changed_ids = changed_ids_count,
                    deleted_ids = deleted_ids_count,
                    downloaded = downloaded_count,
                    network_ms = folder_started.elapsed().as_millis() as u64,
                    "EWS collection delta fetched"
                );
            } else {
                tracing::info!(
                    provider = "exchange-ews",
                    collection = %folder.display_name,
                    scope = if sync.initial { "full-reconcile" } else { "delta" },
                    changed_ids = changed_ids_count,
                    deleted_ids = deleted_ids_count,
                    downloaded = downloaded_count,
                    network_ms = folder_started.elapsed().as_millis() as u64,
                    "EWS collection delta fetched"
                );
            }
            changed_remote_ids.extend(found.iter().filter_map(|message| message.remote_id.clone()));
            messages.append(&mut found);
            if sync.initial {
                server_uids.push((
                    folder.remote_path.clone(),
                    sync.changed_ids.iter().map(|id| stable_uid(id)).collect(),
                ));
                reset_folders.push(folder.remote_path.clone());
            }
            folder.sync_token = Some(encode_mail_sync_token(&MailSyncToken {
                hierarchy: Some(hierarchy.sync_state.clone()),
                items: Some(sync.sync_state),
            })?);
        }
        server_uids.sort_by(|left, right| left.0.cmp(&right.0));
        reset_folders.sort();
        let mut changed_remote_ids = changed_remote_ids.into_iter().collect::<Vec<_>>();
        changed_remote_ids.sort();
        Ok(ImapDiscovery {
            folders,
            messages,
            server_uids,
            reset_folders,
            remote_snapshot: None,
            changed_remote_ids,
            flag_updates: Vec::new(),
            deleted_uids: Vec::new(),
        })
    }

    async fn discover_inbox_incremental(
        &self,
        password: &str,
        cursors: &HashMap<String, FolderSyncCursor>,
    ) -> Result<ImapDiscovery> {
        let mut folders = self.folders(password).await?;
        let inbox_index = folders
            .iter()
            .position(|folder| folder.role == Some(FolderRole::Inbox))
            .ok_or_else(|| backend_error("folders", "папка Входящие не найдена"))?;
        let folder = &mut folders[inbox_index];
        let previous = decode_mail_sync_token(
            cursors
                .get(&folder.remote_path)
                .and_then(|cursor| cursor.sync_token.as_deref()),
        );
        if previous.items.is_none() {
            let messages = self
                .recent_messages_in_folder(password, folder, 50, None)
                .await?;
            return Ok(ImapDiscovery {
                folders,
                messages,
                server_uids: Vec::new(),
                reset_folders: Vec::new(),
                remote_snapshot: None,
                changed_remote_ids: Vec::new(),
                flag_updates: Vec::new(),
                deleted_uids: Vec::new(),
            });
        }
        let folder_xml = format!(r#"<t:FolderId Id="{}"/>"#, escape(&folder.remote_path));
        let sync = self
            .sync_folder_items(password, &folder_xml, previous.items.as_deref())
            .await?;
        let messages = self
            .messages_by_ids(password, &folder.remote_path, &sync.changed_ids)
            .await?;
        let server_uids = if sync.initial {
            vec![(
                folder.remote_path.clone(),
                sync.changed_ids.iter().map(|id| stable_uid(id)).collect(),
            )]
        } else {
            Vec::new()
        };
        let reset_folders = sync
            .initial
            .then(|| folder.remote_path.clone())
            .into_iter()
            .collect();
        let mut changed_remote_ids = sync.changed_ids.clone();
        changed_remote_ids.extend(sync.deleted_ids);
        changed_remote_ids.sort();
        changed_remote_ids.dedup();
        folder.sync_token = Some(encode_mail_sync_token(&MailSyncToken {
            hierarchy: previous.hierarchy,
            items: Some(sync.sync_state),
        })?);
        Ok(ImapDiscovery {
            folders,
            messages,
            server_uids,
            reset_folders,
            remote_snapshot: None,
            changed_remote_ids,
            flag_updates: Vec::new(),
            deleted_uids: Vec::new(),
        })
    }

    async fn raw_item(&self, password: &str, item_id: &str) -> Result<Vec<u8>> {
        let body = format!(
            r#"<m:GetItem><m:ItemShape><t:BaseShape>IdOnly</t:BaseShape><t:IncludeMimeContent>true</t:IncludeMimeContent></m:ItemShape><m:ItemIds><t:ItemId Id="{}"/></m:ItemIds></m:GetItem>"#,
            escape(item_id)
        );
        let response = self.soap(password, "GetItem", &body).await?;
        let document = Document::parse(&response).map_err(|error| backend_error("xml", error))?;
        let encoded = document
            .descendants()
            .find(|node| node.is_element() && node.tag_name().name() == "MimeContent")
            .and_then(|node| node.text())
            .ok_or_else(|| backend_error("item", "Exchange не вернул MIME письма"))?;
        base64::engine::general_purpose::STANDARD
            .decode(encoded.trim())
            .map_err(|error| backend_error("mime", error))
    }

    async fn item_change_key(&self, password: &str, item_id: &str) -> Result<String> {
        let body = format!(
            r#"<m:GetItem><m:ItemShape><t:BaseShape>IdOnly</t:BaseShape></m:ItemShape><m:ItemIds><t:ItemId Id="{}"/></m:ItemIds></m:GetItem>"#,
            escape(item_id)
        );
        let response = self.soap(password, "GetItem", &body).await?;
        let document = Document::parse(&response).map_err(|error| backend_error("xml", error))?;
        document
            .descendants()
            .find(|node| node.is_element() && node.tag_name().name() == "ItemId")
            .and_then(|node| node.attribute("ChangeKey"))
            .map(str::to_owned)
            .ok_or_else(|| backend_error("item", "Exchange не вернул ChangeKey письма"))
    }

    /// Ответить на приглашение штатным способом EWS: AcceptItem/DeclineItem/
    /// TentativelyAcceptItem внутри CreateItem с MessageDisposition=
    /// SendAndSaveCopy - сервер сам формирует и рассылает ответ организатору
    /// (как для обычного письма в send(), только тип элемента другой), и
    /// сохраняет копию в Отправленных. ChangeKey нужен свежий - берём его тем
    /// же способом, что и для операции "flag" в apply_operation.
    pub async fn respond_to_calendar_item(
        &self,
        password: &str,
        item_id: &str,
        response: crate::model::RsvpResponse,
    ) -> Result<()> {
        let change_key = self.item_change_key(password, item_id).await?;
        let element = match response {
            crate::model::RsvpResponse::Accepted => "AcceptItem",
            crate::model::RsvpResponse::Declined => "DeclineItem",
            crate::model::RsvpResponse::Tentative => "TentativelyAcceptItem",
        };
        let body = format!(
            r#"<m:CreateItem MessageDisposition="SendAndSaveCopy"><m:Items><t:{element}><t:ReferenceItemId Id="{}" ChangeKey="{}"/></t:{element}></m:Items></m:CreateItem>"#,
            escape(item_id),
            escape(&change_key)
        );
        self.soap(password, "CreateItem", &body).await.map(|_| ())
    }

    /// Создать событие в календаре Exchange. SendMeetingInvitations решает,
    /// рассылать ли приглашения участникам: если их нет - SendToNone (обычная
    /// запись в календаре без встречи), если есть - SendToAllAndSaveCopy.
    /// Повторяемость (RRULE) не поддержана: EWS описывает её отдельным типом
    /// Recurrence с десятком вариантов паттерна (Daily/Weekly/.../Yearly, у
    /// каждого свой набор полей), адекватный маппинг из RFC5545 RRULE не влез
    /// в объём этой задачи - событие создаётся одиночным, без повторения.
    pub async fn create_calendar_item(&self, password: &str, input: &EventInput) -> Result<String> {
        let body = create_calendar_item_body(input)?;
        let response = self.soap(password, "CreateItem", &body).await?;
        parse_created_item_id(&response)
    }

    /// Изменить событие: ChangeKey - свежий (та же оптимистичная блокировка,
    /// что и у respond_to_calendar_item/apply_operation), каждое поле - в
    /// своём SetItemField, поэтому порядок между ними не важен (важен только
    /// порядок полей внутри одного CalendarItem, а тут в каждом ровно одно).
    pub async fn update_calendar_item(
        &self,
        password: &str,
        item_id: &str,
        input: &EventInput,
    ) -> Result<()> {
        let change_key = self.item_change_key(password, item_id).await?;
        let body = update_calendar_item_body(item_id, &change_key, input)?;
        self.soap(password, "UpdateItem", &body).await.map(|_| ())
    }

    /// Удалить событие. SendMeetingCancellations обязателен для CalendarItem
    /// (в отличие от обычного письма) - без него Exchange не разошлёт отмену
    /// участникам встречи.
    pub async fn delete_calendar_item(&self, password: &str, item_id: &str) -> Result<()> {
        let body = delete_calendar_item_body(item_id);
        self.soap(password, "DeleteItem", &body).await.map(|_| ())
    }

    /// Создать контакт в адресной книге Exchange.
    pub async fn create_contact_item(
        &self,
        password: &str,
        input: &ContactInput,
    ) -> Result<String> {
        let body = create_contact_item_body(input);
        let response = self.soap(password, "CreateItem", &body).await?;
        parse_created_item_id(&response)
    }

    /// Изменить контакт. EmailAddresses/PhoneNumbers - словарные свойства
    /// EWS, для них плоский FieldURI не годится, нужен IndexedFieldURI на
    /// конкретную запись (contacts:EmailAddress/EmailAddress1..3,
    /// contacts:PhoneNumber/MobilePhone|BusinessPhone|...) - см.
    /// contact_item_updates. Убранные из формы адрес/телефон эти
    /// SetItemField не удаляют - для этого нужен отдельный DeleteItemField
    /// по индексу, который здесь не реализован.
    pub async fn update_contact_item(
        &self,
        password: &str,
        item_id: &str,
        input: &ContactInput,
    ) -> Result<()> {
        let change_key = self.item_change_key(password, item_id).await?;
        let body = update_contact_item_body(item_id, &change_key, input);
        self.soap(password, "UpdateItem", &body).await.map(|_| ())
    }

    /// Удалить контакт.
    pub async fn delete_contact_item(&self, password: &str, item_id: &str) -> Result<()> {
        let body = delete_contact_item_body(item_id);
        self.soap(password, "DeleteItem", &body).await.map(|_| ())
    }

    /// Календарь и адресная книга Exchange для aux-синхронизации.
    ///
    /// Календарь и контакты необязательны: у ящика может не быть прав на них, а
    /// почта при этом должна продолжать работать. Поэтому сбой любой из коллекций
    /// не роняет всю синхронизацию - только помечает её недоступной, чтобы
    /// save_auxiliary_data не удалил локальные данные из-за временной ошибки.
    pub async fn auxiliary(
        &self,
        password: &str,
        cursors: &AuxiliarySyncCursors,
    ) -> Result<DavSyncResult> {
        let calendar_url = "ews-calendar:calendar";
        let calendar_state = cursors
            .calendars
            .get(calendar_url)
            .and_then(|cursor| cursor.sync_token.as_deref());
        let calendar_folder = r#"<t:DistinguishedFolderId Id="calendar"/>"#;
        let (calendars, calendar_available) = match self
            .sync_folder_items(password, calendar_folder, calendar_state)
            .await
        {
            Ok(sync) => {
                match async {
                    let events = if sync.initial {
                        self.calendar_events(password).await?
                    } else {
                        self.calendar_events_by_ids(password, &sync.changed_ids)
                            .await?
                    };
                    Ok::<_, Error>(vec![DavCalendar {
                        url: calendar_url.into(),
                        name: "Exchange".into(),
                        ctag: None,
                        sync_token: Some(sync.sync_state),
                        sync_scope: if sync.initial {
                            SyncScope::Full
                        } else {
                            SyncScope::Delta
                        },
                        deleted_event_urls: sync
                            .deleted_ids
                            .into_iter()
                            .map(|id| format!("ews-event:{id}"))
                            .collect(),
                        events,
                    }])
                }
                .await
                {
                    Ok(calendars) => (calendars, true),
                    Err(error) => {
                        tracing::warn!(%error, "EWS: календарь пропущен");
                        (Vec::new(), false)
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%error, "EWS: календарь пропущен");
                (Vec::new(), false)
            }
        };
        let contacts_url = "ews-contacts:contacts";
        let contacts_folder = r#"<t:DistinguishedFolderId Id="contacts"/>"#;
        let contacts_result = self
            .sync_folder_items(
                password,
                contacts_folder,
                cursors
                    .contact_collections
                    .get(contacts_url)
                    .and_then(|cursor| cursor.sync_token.as_deref()),
            )
            .await;
        let (
            contacts,
            contacts_available,
            contacts_scope,
            contact_collections,
            deleted_contact_urls,
        ) = match contacts_result {
            Ok(sync) => match self.contacts_by_ids(password, &sync.changed_ids).await {
                Ok(contacts) => (
                    contacts,
                    true,
                    if sync.initial {
                        SyncScope::Full
                    } else {
                        SyncScope::Delta
                    },
                    vec![DavCollection {
                        url: contacts_url.into(),
                        ctag: None,
                        sync_token: Some(sync.sync_state),
                    }],
                    sync.deleted_ids
                        .into_iter()
                        .map(|id| format!("ews-contact:{id}"))
                        .collect(),
                ),
                Err(error) => {
                    tracing::warn!(%error, "EWS: контакты пропущены");
                    (
                        Vec::new(),
                        false,
                        SyncScope::Unchanged,
                        Vec::new(),
                        Vec::new(),
                    )
                }
            },
            Err(error) => {
                tracing::warn!(%error, "EWS: контакты пропущены");
                (
                    Vec::new(),
                    false,
                    SyncScope::Unchanged,
                    Vec::new(),
                    Vec::new(),
                )
            }
        };
        Ok(DavSyncResult {
            calendars,
            calendars_available: calendar_available,
            contacts,
            contact_collections,
            contacts_available,
            contacts_scope,
            contacts_sync_token: None,
            deleted_contact_urls,
        })
    }

    async fn calendar_events(&self, password: &str) -> Result<Vec<DavEvent>> {
        // CalendarView разворачивает повторения в отдельные вхождения с
        // собственными ItemId, поэтому uid каждого вхождения уникален и
        // рекуррентные события не конфликтуют в базе.
        let now = chrono::Utc::now();
        let start = now - chrono::Duration::days(30);
        let end = now + chrono::Duration::days(365);
        let body = format!(
            r#"<m:FindItem Traversal="Shallow"><m:ItemShape><t:BaseShape>IdOnly</t:BaseShape></m:ItemShape><m:CalendarView StartDate="{}" EndDate="{}" MaxEntriesReturned="1000"/><m:ParentFolderIds><t:DistinguishedFolderId Id="calendar"/></m:ParentFolderIds></m:FindItem>"#,
            start.format("%Y-%m-%dT%H:%M:%SZ"),
            end.format("%Y-%m-%dT%H:%M:%SZ"),
        );
        let response = self.soap(password, "FindItem", &body).await?;
        let ids = parse_item_ids(&response)?;
        let mut events = Vec::new();
        for chunk in ids.chunks(50) {
            let item_ids = chunk
                .iter()
                .map(|id| format!(r#"<t:ItemId Id="{}"/>"#, escape(id)))
                .collect::<String>();
            let body = format!(
                r#"<m:GetItem><m:ItemShape><t:BaseShape>AllProperties</t:BaseShape></m:ItemShape><m:ItemIds>{item_ids}</m:ItemIds></m:GetItem>"#
            );
            let response = self.soap(password, "GetItem", &body).await?;
            events.extend(parse_calendar_items(&response)?);
        }
        Ok(events)
    }

    async fn calendar_events_by_ids(
        &self,
        password: &str,
        ids: &[String],
    ) -> Result<Vec<DavEvent>> {
        let mut events = Vec::new();
        for chunk in ids.chunks(50) {
            let item_ids = chunk
                .iter()
                .map(|id| format!(r#"<t:ItemId Id="{}"/>"#, escape(id)))
                .collect::<String>();
            let body = format!(
                r#"<m:GetItem><m:ItemShape><t:BaseShape>AllProperties</t:BaseShape></m:ItemShape><m:ItemIds>{item_ids}</m:ItemIds></m:GetItem>"#
            );
            let response = self.soap(password, "GetItem", &body).await?;
            events.extend(parse_calendar_items(&response)?);
        }
        Ok(events)
    }

    async fn contacts_by_ids(&self, password: &str, ids: &[String]) -> Result<Vec<DavContact>> {
        let mut contacts = Vec::new();
        for chunk in ids.chunks(50) {
            let item_ids = chunk
                .iter()
                .map(|id| format!(r#"<t:ItemId Id="{}"/>"#, escape(id)))
                .collect::<String>();
            let body = format!(
                r#"<m:GetItem><m:ItemShape><t:BaseShape>AllProperties</t:BaseShape></m:ItemShape><m:ItemIds>{item_ids}</m:ItemIds></m:GetItem>"#
            );
            let response = self.soap(password, "GetItem", &body).await?;
            contacts.extend(parse_contacts(&response)?);
        }
        Ok(contacts)
    }
}

fn parse_folders(xml: &str) -> Result<Vec<DiscoveredFolder>> {
    let document = Document::parse(xml).map_err(|error| backend_error("xml", error))?;
    let mut folders = Vec::new();
    for node in document.descendants().filter(|node| {
        node.is_element()
            && matches!(
                node.tag_name().name(),
                "Folder" | "CalendarFolder" | "ContactsFolder" | "TasksFolder"
            )
    }) {
        if node.tag_name().name() != "Folder" {
            continue;
        }
        let Some(id) = node
            .children()
            .find(|child| child.is_element() && child.tag_name().name() == "FolderId")
            .and_then(|child| child.attribute("Id"))
        else {
            continue;
        };
        let name = node_text(node, "DisplayName").unwrap_or(id);
        let total = node_text(node, "TotalCount")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let unread = node_text(node, "UnreadCount")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        folders.push(DiscoveredFolder {
            remote_path: id.to_owned(),
            display_name: name.to_owned(),
            role: infer_folder_role(name, name),
            unread_count: unread,
            total_count: total,
            uidvalidity: None,
            uidnext: None,
            highestmodseq: None,
            sync_token: None,
        });
    }
    if folders.is_empty() {
        return Err(backend_error(
            "folders",
            "Exchange не вернул почтовые папки",
        ));
    }
    Ok(folders)
}

fn parse_item_ids(xml: &str) -> Result<Vec<String>> {
    let document = Document::parse(xml).map_err(|error| backend_error("xml", error))?;
    Ok(document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "ItemId")
        .filter_map(|node| node.attribute("Id").map(str::to_owned))
        .collect())
}

fn stable_uid(id: &str) -> u32 {
    let digest = Sha256::digest(id.as_bytes());
    u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]).max(1)
}

fn parse_messages(xml: &str, folder_path: &str) -> Result<Vec<DiscoveredMessage>> {
    let document = Document::parse(xml).map_err(|error| backend_error("xml", error))?;
    let mut messages = Vec::new();
    for message in document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "Message")
    {
        let Some(id) = message
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "ItemId")
            .and_then(|node| node.attribute("Id"))
        else {
            continue;
        };
        let Some(encoded) = node_text(message, "MimeContent") else {
            continue;
        };
        let raw = base64::engine::general_purpose::STANDARD
            .decode(encoded.trim())
            .map_err(|error| backend_error("mime", error))?;
        messages.push(DiscoveredMessage {
            folder_path: folder_path.to_owned(),
            uid: stable_uid(id),
            remote_id: Some(id.to_owned()),
            size: u32::try_from(raw.len()).ok(),
            seen: node_text(message, "IsRead") == Some("true"),
            flagged: false,
            answered: false,
            draft: node_text(message, "IsDraft") == Some("true"),
            raw,
            body_fetched: true,
        });
    }
    Ok(messages)
}

/// EWS отдаёт даты в RFC3339 (2026-07-16T17:00:00Z). Календарь хранит их в
/// компактном iCalendar-виде: "20260716T170000Z" для событий со временем,
/// "20260716" для события на весь день - по длине save_auxiliary_data отличает
/// всесуточные события.
fn to_ical_datetime(iso: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(iso).ok().map(|dt| {
        dt.with_timezone(&chrono::Utc)
            .format("%Y%m%dT%H%M%SZ")
            .to_string()
    })
}

fn to_ical_date(iso: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(iso)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc).format("%Y%m%d").to_string())
}

fn ical_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace(',', "\\,")
        .replace(';', "\\;")
}

fn parse_calendar_items(xml: &str) -> Result<Vec<DavEvent>> {
    let document = Document::parse(xml).map_err(|error| backend_error("calendar-xml", error))?;
    let mut events = Vec::new();
    for item in document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "CalendarItem")
    {
        let Some(id) = item
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "ItemId")
            .and_then(|node| node.attribute("Id"))
        else {
            continue;
        };
        let all_day = node_text(item, "IsAllDayEvent") == Some("true");
        let Some(start_iso) = node_text(item, "Start") else {
            continue;
        };
        let dtstart = if all_day {
            to_ical_date(start_iso)
        } else {
            to_ical_datetime(start_iso)
        };
        let Some(dtstart) = dtstart else {
            continue;
        };
        let dtend = node_text(item, "End").and_then(|value| {
            if all_day {
                to_ical_date(value)
            } else {
                to_ical_datetime(value)
            }
        });
        let summary = node_text(item, "Subject")
            .unwrap_or("Без названия")
            .to_owned();
        let description = node_text(item, "Body").map(str::to_owned);
        let location = node_text(item, "Location").map(str::to_owned);
        let organizer = item
            .descendants()
            .find(|node| node.is_element() && node.tag_name().name() == "Organizer")
            .and_then(|node| node_text(node, "EmailAddress"))
            .map(str::to_owned);
        // IsCancelled - штатное поле CalendarItem, приходит уже при
        // BaseShape=AllProperties (см. calendar_events/calendar_events_by_ids
        // выше), доп. свойств запрашивать не нужно.
        let status = if node_text(item, "IsCancelled") == Some("true") {
            Some("CANCELLED".to_owned())
        } else {
            None
        };
        let raw = build_vevent(
            id,
            &summary,
            &dtstart,
            dtend.as_deref(),
            location.as_deref(),
        );
        events.push(DavEvent {
            remote_url: Some(format!("ews-event:{id}")),
            uid: id.to_owned(),
            summary,
            description,
            location,
            dtstart,
            dtend,
            rrule: None,
            recurrence_id: None,
            exdates: None,
            rdates: None,
            status,
            attendees: Vec::new(),
            alarms: Vec::new(),
            timezone: None,
            transp: None,
            class: None,
            categories: Vec::new(),
            url: None,
            organizer,
            sequence: 0,
            raw,
            etag: None,
        });
    }
    Ok(events)
}

fn build_vevent(
    uid: &str,
    summary: &str,
    dtstart: &str,
    dtend: Option<&str>,
    location: Option<&str>,
) -> String {
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_owned(),
        "VERSION:2.0".to_owned(),
        "PRODID:-//truemail//EWS//EN".to_owned(),
        "BEGIN:VEVENT".to_owned(),
        format!("UID:{}", ical_escape(uid)),
        format!("SUMMARY:{}", ical_escape(summary)),
        format!("DTSTART:{dtstart}"),
    ];
    if let Some(dtend) = dtend {
        lines.push(format!("DTEND:{dtend}"));
    }
    if let Some(location) = location {
        lines.push(format!("LOCATION:{}", ical_escape(location)));
    }
    lines.push("END:VEVENT".to_owned());
    lines.push("END:VCALENDAR".to_owned());
    lines.join("\r\n")
}

fn parse_contacts(xml: &str) -> Result<Vec<DavContact>> {
    let document = Document::parse(xml).map_err(|error| backend_error("contacts-xml", error))?;
    let mut contacts = Vec::new();
    for item in document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "Contact")
    {
        let Some(id) = item
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "ItemId")
            .and_then(|node| node.attribute("Id"))
        else {
            continue;
        };
        let first_name = node_text(item, "GivenName").map(str::to_owned);
        let last_name = node_text(item, "Surname").map(str::to_owned);
        let organization = node_text(item, "CompanyName").map(str::to_owned);
        let display_name = node_text(item, "DisplayName")
            .map(str::to_owned)
            .or_else(|| match (&first_name, &last_name) {
                (Some(first), Some(last)) => Some(format!("{first} {last}")),
                (Some(name), None) | (None, Some(name)) => Some(name.clone()),
                _ => None,
            })
            .unwrap_or_else(|| "Без имени".to_owned());
        let emails = item
            .descendants()
            .filter(|node| node.is_element() && node.tag_name().name() == "Entry")
            .filter(|node| {
                node.parent()
                    .is_some_and(|parent| parent.tag_name().name() == "EmailAddresses")
            })
            .filter_map(|node| node.text())
            .map(|value| {
                value
                    .strip_prefix("SMTP:")
                    .or_else(|| value.strip_prefix("smtp:"))
                    .unwrap_or(value)
                    .to_owned()
            })
            .collect::<Vec<_>>();
        let phones = item
            .descendants()
            .filter(|node| node.is_element() && node.tag_name().name() == "Entry")
            .filter(|node| {
                node.parent()
                    .is_some_and(|parent| parent.tag_name().name() == "PhoneNumbers")
            })
            .filter_map(|node| {
                let value = node.text()?.trim();
                if value.is_empty() {
                    return None;
                }
                Some(ContactPhone::from_remote(
                    value,
                    node.attribute("Key").map(phone_kind),
                ))
            })
            .collect::<Vec<_>>();
        let raw = build_vcard(
            id,
            &display_name,
            first_name.as_deref(),
            last_name.as_deref(),
            organization.as_deref(),
            &emails,
            &phones,
        );
        contacts.push(DavContact {
            remote_url: Some(format!("ews-contact:{id}")),
            uid: id.to_owned(),
            display_name,
            first_name,
            last_name,
            organization,
            emails,
            phones,
            raw,
            etag: None,
        });
    }
    Ok(contacts)
}

fn phone_kind(key: &str) -> String {
    match key {
        "MobilePhone" => "mobile",
        "BusinessPhone" | "BusinessPhone2" => "work",
        "HomePhone" | "HomePhone2" => "home",
        "BusinessFax" | "HomeFax" => "fax",
        _ => "other",
    }
    .to_owned()
}

fn build_vcard(
    uid: &str,
    display_name: &str,
    first_name: Option<&str>,
    last_name: Option<&str>,
    organization: Option<&str>,
    emails: &[String],
    phones: &[ContactPhone],
) -> String {
    let mut lines = vec![
        "BEGIN:VCARD".to_owned(),
        "VERSION:3.0".to_owned(),
        format!("UID:{}", ical_escape(uid)),
        format!("FN:{}", ical_escape(display_name)),
        format!(
            "N:{};{};;;",
            ical_escape(last_name.unwrap_or("")),
            ical_escape(first_name.unwrap_or(""))
        ),
    ];
    if let Some(organization) = organization {
        lines.push(format!("ORG:{}", ical_escape(organization)));
    }
    for email in emails {
        lines.push(format!("EMAIL:{}", ical_escape(email)));
    }
    for phone in phones {
        let number = ical_escape(phone.number.trim());
        let value = match phone
            .extension
            .as_deref()
            .filter(|extension| !extension.trim().is_empty())
        {
            Some(extension) => format!("{number};ext={}", ical_escape(extension.trim())),
            None => number,
        };
        lines.push(format!(
            "TEL;TYPE={}:{}",
            phone.kind.as_deref().unwrap_or("other").to_uppercase(),
            value
        ));
    }
    lines.push("END:VCARD".to_owned());
    lines.join("\r\n")
}

/// SendMeetingInvitations/SendMeetingInvitationsOrCancellations: рассылаем
/// приглашения только если у события реально есть участники - иначе это
/// просто личная запись в календаре, а не встреча.
fn calendar_send_disposition(input: &EventInput) -> &'static str {
    if input.attendees.is_empty() {
        "SendToNone"
    } else {
        "SendToAllAndSaveCopy"
    }
}

/// EventInput.dtstart/dtend - RFC3339 (тот же формат, что понимают
/// write_dav_event и google_event_body в account/auxiliary.rs). Для
/// всесуточного события EWS хочет начало суток UTC; для обычного - точное
/// время, приведённое к UTC в формате "2026-07-16T17:00:00Z" (том же, что
/// разбирает to_ical_datetime при чтении).
fn ews_event_datetime(value: &str, all_day: bool) -> Result<String> {
    if all_day {
        let date = value.get(..10).filter(|date| date.len() == 10);
        return date.map(|date| format!("{date}T00:00:00Z")).ok_or_else(|| {
            backend_error("event-datetime", "некорректная дата всесуточного события")
        });
    }
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|dt| {
            dt.with_timezone(&chrono::Utc)
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string()
        })
        .map_err(|error| {
            backend_error(
                "event-datetime",
                format!("некорректная дата события: {error}"),
            )
        })
}

fn attendees_xml(attendees: &[&Attendee]) -> String {
    attendees
        .iter()
        .map(|attendee| {
            let name = attendee
                .name
                .as_deref()
                .map(|name| format!("<t:Name>{}</t:Name>", escape(name)))
                .unwrap_or_default();
            format!(
                "<t:Attendee><t:Mailbox>{name}<t:EmailAddress>{}</t:EmailAddress></t:Mailbox></t:Attendee>",
                escape(&attendee.email)
            )
        })
        .collect()
}

/// Поля CalendarItem в порядке, требуемом схемой EWS (CalendarItemType
/// наследует ItemType): Subject/Body/ReminderIsSet/ReminderMinutesBeforeStart
/// - из общей части ItemType, дальше Start/End/IsAllDayEvent/Location/
/// RequiredAttendees/OptionalAttendees - уже из calendar-специфичной части.
/// Для CreateItem все поля идут одним блоком <t:CalendarItem> и порядок
/// критичен (нарушение схемы = ErrorSchemaValidation); для UpdateItem тот же
/// список оборачивается по одному полю на SetItemField, там порядок между
/// полями уже не важен.
fn calendar_item_fields(input: &EventInput) -> Result<Vec<(&'static str, String)>> {
    let mut fields = vec![
        (
            "item:Subject",
            format!("<t:Subject>{}</t:Subject>", escape(&input.summary)),
        ),
        (
            "item:Body",
            format!(
                r#"<t:Body BodyType="Text">{}</t:Body>"#,
                escape(input.description.as_deref().unwrap_or(""))
            ),
        ),
    ];
    match input.alarms.first() {
        Some(alarm) => {
            fields.push((
                "item:ReminderIsSet",
                "<t:ReminderIsSet>true</t:ReminderIsSet>".to_owned(),
            ));
            fields.push((
                "item:ReminderMinutesBeforeStart",
                format!(
                    "<t:ReminderMinutesBeforeStart>{}</t:ReminderMinutesBeforeStart>",
                    alarm.trigger_minutes.max(0)
                ),
            ));
        }
        None => fields.push((
            "item:ReminderIsSet",
            "<t:ReminderIsSet>false</t:ReminderIsSet>".to_owned(),
        )),
    }
    fields.push((
        "calendar:Start",
        format!(
            "<t:Start>{}</t:Start>",
            ews_event_datetime(&input.dtstart, input.all_day)?
        ),
    ));
    let end = input.dtend.as_deref().unwrap_or(&input.dtstart);
    fields.push((
        "calendar:End",
        format!("<t:End>{}</t:End>", ews_event_datetime(end, input.all_day)?),
    ));
    fields.push((
        "calendar:IsAllDayEvent",
        format!("<t:IsAllDayEvent>{}</t:IsAllDayEvent>", input.all_day),
    ));
    if let Some(location) = input.location.as_deref().filter(|value| !value.is_empty()) {
        fields.push((
            "calendar:Location",
            format!("<t:Location>{}</t:Location>", escape(location)),
        ));
    }
    let (optional, required): (Vec<_>, Vec<_>) = input
        .attendees
        .iter()
        .partition(|attendee| attendee.role.as_deref() == Some("OPT-PARTICIPANT"));
    if !required.is_empty() {
        fields.push((
            "calendar:RequiredAttendees",
            format!(
                "<t:RequiredAttendees>{}</t:RequiredAttendees>",
                attendees_xml(&required)
            ),
        ));
    }
    if !optional.is_empty() {
        fields.push((
            "calendar:OptionalAttendees",
            format!(
                "<t:OptionalAttendees>{}</t:OptionalAttendees>",
                attendees_xml(&optional)
            ),
        ));
    }
    Ok(fields)
}

fn calendar_item_xml(input: &EventInput) -> Result<String> {
    let body = calendar_item_fields(input)?
        .into_iter()
        .map(|(_, xml)| xml)
        .collect::<String>();
    Ok(format!("<t:CalendarItem>{body}</t:CalendarItem>"))
}

fn create_calendar_item_body(input: &EventInput) -> Result<String> {
    let disposition = calendar_send_disposition(input);
    let item = calendar_item_xml(input)?;
    Ok(format!(
        r#"<m:CreateItem SendMeetingInvitations="{disposition}"><m:SavedItemFolderId><t:DistinguishedFolderId Id="calendar"/></m:SavedItemFolderId><m:Items>{item}</m:Items></m:CreateItem>"#
    ))
}

fn update_calendar_item_body(
    item_id: &str,
    change_key: &str,
    input: &EventInput,
) -> Result<String> {
    let disposition = calendar_send_disposition(input);
    let updates = calendar_item_fields(input)?
        .into_iter()
        .map(|(field_uri, xml)| {
            format!(
                r#"<t:SetItemField><t:FieldURI FieldURI="{field_uri}"/><t:CalendarItem>{xml}</t:CalendarItem></t:SetItemField>"#
            )
        })
        .collect::<String>();
    Ok(format!(
        r#"<m:UpdateItem ConflictResolution="AutoResolve" SendMeetingInvitationsOrCancellations="{disposition}"><m:ItemChanges><t:ItemChange><t:ItemId Id="{}" ChangeKey="{}"/><t:Updates>{updates}</t:Updates></t:ItemChange></m:ItemChanges></m:UpdateItem>"#,
        escape(item_id),
        escape(change_key)
    ))
}

/// SendMeetingCancellations обязателен для CalendarItem - без него Exchange
/// не разошлёт отмену участникам встречи (в отличие от удаления обычного
/// письма, см. "delete" в apply_operation выше).
fn delete_calendar_item_body(item_id: &str) -> String {
    format!(
        r#"<m:DeleteItem DeleteType="MoveToDeletedItems" SendMeetingCancellations="SendToAllAndSaveCopy"><m:ItemIds><t:ItemId Id="{}"/></m:ItemIds></m:DeleteItem>"#,
        escape(item_id)
    )
}

fn parse_created_item_id(xml: &str) -> Result<String> {
    let document = Document::parse(xml).map_err(|error| backend_error("xml", error))?;
    document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "ItemId")
        .and_then(|node| node.attribute("Id"))
        .map(str::to_owned)
        .ok_or_else(|| backend_error("item", "Exchange не вернул идентификатор нового элемента"))
}

fn parse_created_folder_id(xml: &str) -> Result<String> {
    let document = Document::parse(xml).map_err(|error| backend_error("xml", error))?;
    document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "FolderId")
        .and_then(|node| node.attribute("Id"))
        .map(str::to_owned)
        .ok_or_else(|| backend_error("folder", "Exchange не вернул идентификатор новой папки"))
}

fn contact_emails_xml(emails: &[String]) -> String {
    emails
        .iter()
        .filter(|email| !email.trim().is_empty())
        .take(3)
        .enumerate()
        .map(|(index, email)| {
            format!(
                r#"<t:Entry Key="EmailAddress{}">{}</t:Entry>"#,
                index + 1,
                escape(email.trim())
            )
        })
        .collect()
}

/// EWS хранит телефоны контакта как словарь с фиксированным набором ключей
/// (MobilePhone, BusinessPhone/BusinessPhone2, HomePhone/HomePhone2,
/// BusinessFax, OtherTelephone) - произвольный "kind" сюда не положить.
/// Второй телефон того же вида (work/home) уходит под "2"-вариант ключа -
/// его же понимает phone_kind() при чтении; третий и далее телефон того же
/// вида на сервер не попадает (used не находит свободного ключа).
fn ews_phone_key<'a>(kind: Option<&str>, used: &mut HashSet<&'a str>) -> Option<&'a str> {
    let candidates: &[&'a str] = match kind {
        Some("mobile") => &["MobilePhone"],
        Some("work") => &["BusinessPhone", "BusinessPhone2"],
        Some("home") => &["HomePhone", "HomePhone2"],
        Some("fax") => &["BusinessFax"],
        _ => &["OtherTelephone"],
    };
    candidates.iter().copied().find(|key| used.insert(key))
}

fn contact_phones_xml(phones: &[ContactPhone]) -> String {
    let mut used = HashSet::new();
    phones
        .iter()
        .filter(|phone| !phone.number.trim().is_empty())
        .filter_map(|phone| {
            let key = ews_phone_key(phone.kind.as_deref(), &mut used)?;
            Some(format!(
                r#"<t:Entry Key="{key}">{}</t:Entry>"#,
                escape(&phone.remote_value())
            ))
        })
        .collect()
}

/// Поля Contact в порядке схемы EWS (ContactItemType): DisplayName/GivenName/
/// CompanyName идут в начале, EmailAddresses/PhoneNumbers - следом, а
/// Surname в самой схеме стоит куда дальше (после SpouseName) - неочевидно,
/// но так задокументировано у Microsoft, и здесь сохранено намеренно, а не
/// по интуитивному "имя, фамилия".
fn contact_item_xml(input: &ContactInput) -> String {
    let mut body = format!(
        "<t:DisplayName>{}</t:DisplayName>",
        escape(&input.display_name)
    );
    if let Some(first) = input
        .first_name
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        body.push_str(&format!("<t:GivenName>{}</t:GivenName>", escape(first)));
    }
    if let Some(company) = input
        .organization
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        body.push_str(&format!(
            "<t:CompanyName>{}</t:CompanyName>",
            escape(company)
        ));
    }
    if !input.emails.is_empty() {
        body.push_str(&format!(
            "<t:EmailAddresses>{}</t:EmailAddresses>",
            contact_emails_xml(&input.emails)
        ));
    }
    if !input.phones.is_empty() {
        body.push_str(&format!(
            "<t:PhoneNumbers>{}</t:PhoneNumbers>",
            contact_phones_xml(&input.phones)
        ));
    }
    if let Some(last) = input.last_name.as_deref().filter(|value| !value.is_empty()) {
        body.push_str(&format!("<t:Surname>{}</t:Surname>", escape(last)));
    }
    format!("<t:Contact>{body}</t:Contact>")
}

fn create_contact_item_body(input: &ContactInput) -> String {
    let item = contact_item_xml(input);
    format!(
        r#"<m:CreateItem><m:SavedItemFolderId><t:DistinguishedFolderId Id="contacts"/></m:SavedItemFolderId><m:Items>{item}</m:Items></m:CreateItem>"#
    )
}

fn update_contact_item_body(item_id: &str, change_key: &str, input: &ContactInput) -> String {
    let updates = contact_item_updates(input);
    format!(
        r#"<m:UpdateItem ConflictResolution="AutoResolve"><m:ItemChanges><t:ItemChange><t:ItemId Id="{}" ChangeKey="{}"/><t:Updates>{updates}</t:Updates></t:ItemChange></m:ItemChanges></m:UpdateItem>"#,
        escape(item_id),
        escape(change_key)
    )
}

fn delete_contact_item_body(item_id: &str) -> String {
    format!(
        r#"<m:DeleteItem DeleteType="MoveToDeletedItems"><m:ItemIds><t:ItemId Id="{}"/></m:ItemIds></m:DeleteItem>"#,
        escape(item_id)
    )
}

/// SetItemField по каждому простому полю; EmailAddresses/PhoneNumbers -
/// словарные свойства, для них нужен IndexedFieldURI на конкретную запись
/// (см. документацию EWS "Setting an indexed property"), а не общий FieldURI.
fn contact_item_updates(input: &ContactInput) -> String {
    let mut updates = format!(
        r#"<t:SetItemField><t:FieldURI FieldURI="contacts:DisplayName"/><t:Contact><t:DisplayName>{}</t:DisplayName></t:Contact></t:SetItemField>"#,
        escape(&input.display_name)
    );
    if let Some(first) = input
        .first_name
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        updates.push_str(&format!(
            r#"<t:SetItemField><t:FieldURI FieldURI="contacts:GivenName"/><t:Contact><t:GivenName>{}</t:GivenName></t:Contact></t:SetItemField>"#,
            escape(first)
        ));
    }
    if let Some(last) = input.last_name.as_deref().filter(|value| !value.is_empty()) {
        updates.push_str(&format!(
            r#"<t:SetItemField><t:FieldURI FieldURI="contacts:Surname"/><t:Contact><t:Surname>{}</t:Surname></t:Contact></t:SetItemField>"#,
            escape(last)
        ));
    }
    if let Some(company) = input
        .organization
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        updates.push_str(&format!(
            r#"<t:SetItemField><t:FieldURI FieldURI="contacts:CompanyName"/><t:Contact><t:CompanyName>{}</t:CompanyName></t:Contact></t:SetItemField>"#,
            escape(company)
        ));
    }
    for (index, email) in input
        .emails
        .iter()
        .filter(|value| !value.trim().is_empty())
        .take(3)
        .enumerate()
    {
        let key = format!("EmailAddress{}", index + 1);
        updates.push_str(&format!(
            r#"<t:SetItemField><t:IndexedFieldURI FieldURI="contacts:EmailAddress" FieldIndex="{key}"/><t:Contact><t:EmailAddresses><t:Entry Key="{key}">{}</t:Entry></t:EmailAddresses></t:Contact></t:SetItemField>"#,
            escape(email.trim())
        ));
    }
    let mut used = HashSet::new();
    for phone in input
        .phones
        .iter()
        .filter(|phone| !phone.number.trim().is_empty())
    {
        let Some(key) = ews_phone_key(phone.kind.as_deref(), &mut used) else {
            continue;
        };
        updates.push_str(&format!(
            r#"<t:SetItemField><t:IndexedFieldURI FieldURI="contacts:PhoneNumber" FieldIndex="{key}"/><t:Contact><t:PhoneNumbers><t:Entry Key="{key}">{}</t:Entry></t:PhoneNumbers></t:Contact></t:SetItemField>"#,
            escape(&phone.remote_value())
        ));
    }
    updates
}

#[async_trait]
impl MailBackend for EwsBackend {
    fn provider_id(&self) -> &'static str {
        "exchange-ews"
    }

    async fn validate(&self, _email: &str, credential: &str) -> Result<()> {
        let body = r#"<m:GetFolder><m:FolderShape><t:BaseShape>IdOnly</t:BaseShape></m:FolderShape><m:FolderIds><t:DistinguishedFolderId Id="inbox"/></m:FolderIds></m:GetFolder>"#;
        self.soap(credential, "GetFolder", body).await.map(|_| ())
    }

    async fn discover(
        &self,
        _email: &str,
        credential: &str,
        cursors: &HashMap<String, FolderSyncCursor>,
        retention_days: i64,
    ) -> Result<ImapDiscovery> {
        self.discover_incremental(credential, cursors, retention_days)
            .await
    }

    async fn discover_folders(
        &self,
        _email: &str,
        credential: &str,
    ) -> Result<Vec<DiscoveredFolder>> {
        self.folders(credential).await
    }

    async fn discover_inbox(
        &self,
        _email: &str,
        credential: &str,
        cursors: &HashMap<String, FolderSyncCursor>,
    ) -> Result<ImapDiscovery> {
        self.discover_inbox_incremental(credential, cursors).await
    }

    async fn apply_operation(
        &self,
        _email: &str,
        credential: &str,
        operation: &str,
        payload: &str,
    ) -> Result<()> {
        let payload: serde_json::Value = serde_json::from_str(payload)?;
        let item_id = payload["remote_id"]
            .as_str()
            .ok_or_else(|| Error::AccountConfig("EWS outbox: нет remote_id".into()))?;
        let change_key = if operation == "flag" {
            Some(self.item_change_key(credential, item_id).await?)
        } else {
            None
        };
        let (action, body) = match operation {
            // message:IsRead всегда шлём. message:Flag (FlagStatus Flagged/NotFlagged)
            // - комплексное свойство, доступное EWS начиная с Exchange 2010 (тот же
            // минимум, что и RequestServerVersion=Exchange2010_SP2 в конверте soap()
            // ниже), поэтому используем его напрямую вместо обходных путей вроде
            // Importance. Ключ "flagged" опционален - его нет в операциях, вставленных
            // в outbox до появления звёздочки, тогда update по Flag не шлём вовсе.
            "flag" => (
                "UpdateItem",
                format!(
                    r#"<m:UpdateItem ConflictResolution="AutoResolve" MessageDisposition="SaveOnly"><m:ItemChanges><t:ItemChange><t:ItemId Id="{}" ChangeKey="{}"/><t:Updates>{}</t:Updates></t:ItemChange></m:ItemChanges></m:UpdateItem>"#,
                    escape(item_id),
                    escape(change_key.as_deref().unwrap_or_default()),
                    flag_update_fields(
                        payload["seen"].as_bool().unwrap_or(false),
                        payload["flagged"].as_bool()
                    )
                ),
            ),
            "move" => (
                "MoveItem",
                format!(
                    r#"<m:MoveItem><m:ToFolderId><t:FolderId Id="{}"/></m:ToFolderId><m:ItemIds><t:ItemId Id="{}"/></m:ItemIds></m:MoveItem>"#,
                    escape(payload["target_folder_path"].as_str().ok_or_else(|| {
                        Error::AccountConfig("EWS outbox: нет папки назначения".into())
                    })?),
                    escape(item_id)
                ),
            ),
            "delete" => (
                "DeleteItem",
                format!(
                    r#"<m:DeleteItem DeleteType="MoveToDeletedItems"><m:ItemIds><t:ItemId Id="{}"/></m:ItemIds></m:DeleteItem>"#,
                    escape(item_id)
                ),
            ),
            other => {
                return Err(Error::AccountConfig(format!(
                    "EWS outbox: неизвестная операция {other}"
                )));
            }
        };
        self.soap(credential, action, &body).await.map(|_| ())
    }

    async fn create_folder(
        &self,
        _email: &str,
        credential: &str,
        parent_path: Option<&str>,
        name: &str,
    ) -> Result<String> {
        let body = create_folder_body(parent_path, name.trim());
        let response = self.soap(credential, "CreateFolder", &body).await?;
        parse_created_folder_id(&response)
    }

    async fn rename_folder(
        &self,
        _email: &str,
        credential: &str,
        remote_path: &str,
        new_name: &str,
    ) -> Result<String> {
        let body = format!(
            r#"<m:UpdateFolder><m:FolderChanges><t:FolderChange><t:FolderId Id="{}"/><t:Updates><t:SetFolderField><t:FieldURI FieldURI="folder:DisplayName"/><t:Folder><t:DisplayName>{}</t:DisplayName></t:Folder></t:SetFolderField></t:Updates></t:FolderChange></m:FolderChanges></m:UpdateFolder>"#,
            escape(remote_path),
            escape(new_name)
        );
        self.soap(credential, "UpdateFolder", &body).await?;
        Ok(remote_path.to_owned())
    }

    async fn delete_folder(&self, _email: &str, credential: &str, remote_path: &str) -> Result<()> {
        let body = format!(
            r#"<m:DeleteFolder DeleteType="MoveToDeletedItems"><m:FolderIds><t:FolderId Id="{}"/></m:FolderIds></m:DeleteFolder>"#,
            escape(remote_path)
        );
        self.soap(credential, "DeleteFolder", &body)
            .await
            .map(|_| ())
    }

    async fn wait_for_change(&self, _email: &str, _credential: &str) -> Result<()> {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        Ok(())
    }

    async fn send(&self, message: OutgoingMessage, credential: &str) -> Result<super::SendOutcome> {
        let mime = super::smtp::build_message(message)?.formatted();
        let encoded = base64::engine::general_purpose::STANDARD.encode(mime);
        let item = format!(
            "<t:Message><t:MimeContent CharacterSet=\"UTF-8\">{encoded}</t:MimeContent></t:Message>"
        );
        let body = format!(
            r#"<m:CreateItem MessageDisposition="SendAndSaveCopy"><m:SavedItemFolderId><t:DistinguishedFolderId Id="sentitems"/></m:SavedItemFolderId><m:Items>{item}</m:Items></m:CreateItem>"#
        );
        self.soap(credential, "CreateItem", &body).await?;
        Ok(super::SendOutcome::SavedOnServer)
    }

    async fn fetch_message_raw(
        &self,
        _email: &str,
        credential: &str,
        _folder_path: &str,
        _uid: u32,
        remote_id: Option<&str>,
    ) -> Result<Vec<u8>> {
        self.raw_item(
            credential,
            remote_id.ok_or_else(|| Error::AccountConfig("EWS: нет remote_id письма".into()))?,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_autodiscover_ews_url() {
        let xml = r#"<Autodiscover><Response><Account><Protocol><EwsUrl>https://mail.example.test/EWS/Exchange.asmx</EwsUrl></Protocol></Account></Response></Autodiscover>"#;
        let document = Document::parse(xml).unwrap();
        let value = document
            .descendants()
            .find(|node| node.tag_name().name() == "EwsUrl")
            .and_then(|node| node.text());
        assert_eq!(value, Some("https://mail.example.test/EWS/Exchange.asmx"));
    }

    #[test]
    fn create_folder_body_targets_parent_by_id_when_given() {
        let body = create_folder_body(Some("parent-folder-id"), "Заметки");
        assert!(body.contains(r#"<t:FolderId Id="parent-folder-id"/>"#));
        assert!(body.contains("<t:DisplayName>Заметки</t:DisplayName>"));
        assert!(!body.contains("msgfolderroot"));
    }

    #[test]
    fn create_folder_body_falls_back_to_msgfolderroot_without_parent() {
        let body = create_folder_body(None, "Top level");
        assert!(body.contains(r#"<t:DistinguishedFolderId Id="msgfolderroot"/>"#));
        assert!(body.contains("<t:DisplayName>Top level</t:DisplayName>"));
    }

    #[test]
    fn create_folder_body_escapes_special_characters_in_name() {
        let body = create_folder_body(None, "A & B <test>");
        assert!(body.contains("<t:DisplayName>A &amp; B &lt;test&gt;</t:DisplayName>"));
    }

    #[test]
    fn stable_ews_uid_is_nonzero_and_repeatable() {
        assert_ne!(stable_uid("AAMk-long-item-id"), 0);
        assert_eq!(
            stable_uid("AAMk-long-item-id"),
            stable_uid("AAMk-long-item-id")
        );
    }

    #[test]
    fn parses_ews_calendar_item() {
        let xml = r#"<Envelope><CalendarItem><ItemId Id="event-1"/><Subject>Встреча</Subject><Body>Описание</Body><Start>2026-07-17T09:00:00Z</Start><End>2026-07-17T10:30:00Z</End><IsAllDayEvent>false</IsAllDayEvent><Location>Переговорная</Location><Organizer><Mailbox><EmailAddress>owner@example.test</EmailAddress></Mailbox></Organizer></CalendarItem></Envelope>"#;
        let events = parse_calendar_items(xml).expect("calendar response");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].uid, "event-1");
        assert_eq!(events[0].summary, "Встреча");
        assert_eq!(events[0].dtstart, "20260717T090000Z");
        assert_eq!(events[0].dtend.as_deref(), Some("20260717T103000Z"));
        assert_eq!(events[0].location.as_deref(), Some("Переговорная"));
        assert_eq!(events[0].organizer.as_deref(), Some("owner@example.test"));
    }

    #[test]
    fn parses_ews_contact_with_all_addresses_and_phones() {
        let xml = r#"<Envelope><Contact><ItemId Id="contact-1"/><DisplayName>Иван Петров</DisplayName><GivenName>Иван</GivenName><Surname>Петров</Surname><CompanyName>Пример</CompanyName><EmailAddresses><Entry Key="EmailAddress1">SMTP:ivan@example.test</Entry><Entry Key="EmailAddress2">other@example.test</Entry></EmailAddresses><PhoneNumbers><Entry Key="MobilePhone">+79990000000</Entry><Entry Key="BusinessPhone">+74950000000;ext=123</Entry></PhoneNumbers></Contact></Envelope>"#;
        let contacts = parse_contacts(xml).expect("contacts response");
        assert_eq!(contacts.len(), 1);
        assert_eq!(
            contacts[0].emails,
            ["ivan@example.test", "other@example.test"]
        );
        assert_eq!(contacts[0].phones.len(), 2);
        assert_eq!(contacts[0].phones[0].kind.as_deref(), Some("mobile"));
        assert_eq!(contacts[0].phones[1].extension.as_deref(), Some("123"));
        assert!(contacts[0].raw.contains("ORG:Пример"));
        assert!(
            contacts[0]
                .raw
                .contains("TEL;TYPE=WORK:+74950000000;ext=123")
        );
    }

    #[test]
    fn malformed_auxiliary_xml_is_an_error() {
        assert!(parse_calendar_items("<broken").is_err());
        assert!(parse_contacts("<broken").is_err());
    }

    #[test]
    fn mail_sync_token_round_trips_both_ews_states() {
        let token = MailSyncToken {
            hierarchy: Some("hierarchy+/=".into()),
            items: Some("items+/=".into()),
        };
        let encoded = encode_mail_sync_token(&token).expect("encode EWS sync token");
        assert!(encoded.starts_with(MAIL_SYNC_TOKEN_PREFIX));
        assert_eq!(decode_mail_sync_token(Some(&encoded)), token);
        assert_eq!(
            decode_mail_sync_token(Some("legacy-or-corrupted")),
            MailSyncToken::default()
        );
    }

    #[test]
    fn parses_all_sync_folder_item_change_kinds() {
        let xml = r#"<SyncFolderItemsResponseMessage ResponseClass="Success">
 <SyncState>next-state</SyncState>
 <IncludesLastItemInRange>false</IncludesLastItemInRange>
 <Changes>
  <Create><Message><ItemId Id="created"/></Message></Create>
  <Update><Message><ItemId Id="updated"/></Message></Update>
  <Delete><ItemId Id="deleted"/></Delete>
  <ReadFlagChange><ItemId Id="read-flag"/><IsRead>true</IsRead></ReadFlagChange>
 </Changes>
</SyncFolderItemsResponseMessage>"#;
        assert_eq!(sync_state(xml).unwrap(), "next-state");
        assert!(!sync_page_complete(xml, "IncludesLastItemInRange").unwrap());
        let (changed, deleted) = change_ids(xml, "ItemId").expect("parse item changes");
        assert_eq!(changed, ["created", "read-flag", "updated"]);
        assert_eq!(deleted, ["deleted"]);
    }

    #[test]
    fn parses_sync_folder_hierarchy_deletion_and_completion() {
        let xml = r#"<SyncFolderHierarchyResponseMessage ResponseClass="Success">
 <SyncState>hierarchy-state</SyncState>
 <IncludesLastFolderInRange>true</IncludesLastFolderInRange>
 <Changes><Delete><FolderId Id="removed-folder"/></Delete></Changes>
</SyncFolderHierarchyResponseMessage>"#;
        assert!(sync_page_complete(xml, "IncludesLastFolderInRange").unwrap());
        let (_, deleted) = change_ids(xml, "FolderId").expect("parse hierarchy changes");
        assert_eq!(deleted, ["removed-folder"]);
    }

    #[test]
    fn preserves_invalid_sync_state_response_code() {
        let xml = r#"<SyncFolderItemsResponseMessage ResponseClass="Error">
 <MessageText>The synchronization state data is invalid.</MessageText>
 <ResponseCode>ErrorInvalidSyncStateData</ResponseCode>
</SyncFolderItemsResponseMessage>"#;
        let error = response_error(xml).expect("response error");
        assert!(error.contains("ErrorInvalidSyncStateData"));
        assert!(error.contains("synchronization state data is invalid"));
    }

    fn sample_event_input() -> EventInput {
        EventInput {
            summary: "Планёрка <team>".into(),
            description: Some("Обсудить & согласовать".into()),
            location: Some("Переговорная \"Б\"".into()),
            dtstart: "2026-07-20T10:00:00+03:00".into(),
            dtend: Some("2026-07-20T11:00:00+03:00".into()),
            all_day: false,
            attendees: Vec::new(),
            alarms: Vec::new(),
            ..EventInput::default()
        }
    }

    #[test]
    fn ews_event_datetime_converts_offset_to_utc() {
        assert_eq!(
            ews_event_datetime("2026-07-20T10:00:00+03:00", false).unwrap(),
            "2026-07-20T07:00:00Z"
        );
        assert_eq!(
            ews_event_datetime("2026-07-20T00:00:00Z", true).unwrap(),
            "2026-07-20T00:00:00Z"
        );
        assert!(ews_event_datetime("not-a-date", false).is_err());
    }

    #[test]
    fn create_calendar_item_body_escapes_fields_and_sends_to_none_without_attendees() {
        let body = create_calendar_item_body(&sample_event_input()).expect("build body");
        assert!(body.contains(r#"SendMeetingInvitations="SendToNone""#));
        assert!(body.contains("<t:Subject>Планёрка &lt;team&gt;</t:Subject>"));
        assert!(body.contains("Обсудить &amp; согласовать"));
        assert!(body.contains("Переговорная &quot;Б&quot;"));
        assert!(body.contains(r#"<t:DistinguishedFolderId Id="calendar"/>"#));
        assert!(body.contains("<t:Start>2026-07-20T07:00:00Z</t:Start>"));
        assert!(body.contains("<t:End>2026-07-20T08:00:00Z</t:End>"));
        assert!(!body.contains("RequiredAttendees"));
    }

    #[test]
    fn create_calendar_item_body_splits_required_and_optional_attendees() {
        let mut input = sample_event_input();
        input.attendees = vec![
            Attendee {
                email: "req@example.test".into(),
                name: Some("Обязательный".into()),
                role: Some("REQ-PARTICIPANT".into()),
                partstat: None,
                rsvp: true,
            },
            Attendee {
                email: "opt@example.test".into(),
                name: None,
                role: Some("OPT-PARTICIPANT".into()),
                partstat: None,
                rsvp: true,
            },
        ];
        let body = create_calendar_item_body(&input).expect("build body");
        assert!(body.contains(r#"SendMeetingInvitations="SendToAllAndSaveCopy""#));
        assert!(body.contains("<t:RequiredAttendees>"));
        assert!(body.contains("req@example.test"));
        assert!(body.contains("<t:OptionalAttendees>"));
        assert!(body.contains("opt@example.test"));
        // Обязательный участник не должен попасть в OptionalAttendees и наоборот.
        let required_start = body.find("<t:RequiredAttendees>").unwrap();
        let required_end = body.find("</t:RequiredAttendees>").unwrap();
        assert!(!body[required_start..required_end].contains("opt@example.test"));
    }

    #[test]
    fn update_calendar_item_body_carries_change_key_and_disposition() {
        let body = update_calendar_item_body("item-1", "change-1", &sample_event_input())
            .expect("build body");
        assert!(body.contains(r#"<t:ItemId Id="item-1" ChangeKey="change-1"/>"#));
        assert!(body.contains(r#"SendMeetingInvitationsOrCancellations="SendToNone""#));
        assert!(body.contains(r#"<t:FieldURI FieldURI="item:Subject"/>"#));
        assert!(body.contains(r#"<t:FieldURI FieldURI="calendar:Start"/>"#));
    }

    #[test]
    fn delete_calendar_item_body_always_sends_meeting_cancellations() {
        let body = delete_calendar_item_body("item \"1\"");
        assert!(body.contains(r#"SendMeetingCancellations="SendToAllAndSaveCopy""#));
        assert!(body.contains(r#"DeleteType="MoveToDeletedItems""#));
        assert!(body.contains("item &quot;1&quot;"));
    }

    fn sample_contact_input() -> ContactInput {
        ContactInput {
            display_name: "Иванов & Ко".into(),
            first_name: Some("Иван".into()),
            last_name: Some("Иванов".into()),
            organization: Some("ООО \"Пример\"".into()),
            emails: vec![
                "a@example.test".into(),
                "b@example.test".into(),
                "c@example.test".into(),
                "d@example.test".into(),
            ],
            phones: vec![
                ContactPhone {
                    number: "+79990000001".into(),
                    kind: Some("work".into()),
                    extension: None,
                },
                ContactPhone {
                    number: "+79990000002".into(),
                    kind: Some("work".into()),
                    extension: None,
                },
            ],
        }
    }

    #[test]
    fn contact_item_xml_orders_fields_and_limits_emails_to_three() {
        let xml = contact_item_xml(&sample_contact_input());
        assert!(xml.contains("<t:DisplayName>Иванов &amp; Ко</t:DisplayName>"));
        assert!(xml.contains("<t:CompanyName>ООО &quot;Пример&quot;</t:CompanyName>"));
        assert!(xml.contains(r#"<t:Entry Key="EmailAddress1">a@example.test</t:Entry>"#));
        assert!(xml.contains(r#"<t:Entry Key="EmailAddress3">c@example.test</t:Entry>"#));
        assert!(!xml.contains("d@example.test"));
        // DisplayName должен идти раньше EmailAddresses, а Surname - позже него
        // (порядок ContactItemType в схеме EWS, см. contact_item_xml).
        let display_at = xml.find("DisplayName").unwrap();
        let emails_at = xml.find("EmailAddresses").unwrap();
        let surname_at = xml.find("Surname").unwrap();
        assert!(display_at < emails_at);
        assert!(emails_at < surname_at);
    }

    #[test]
    fn contact_phones_xml_uses_second_key_for_repeated_kind() {
        let xml = contact_phones_xml(&sample_contact_input().phones);
        assert!(xml.contains(r#"<t:Entry Key="BusinessPhone">+79990000001</t:Entry>"#));
        assert!(xml.contains(r#"<t:Entry Key="BusinessPhone2">+79990000002</t:Entry>"#));
    }

    #[test]
    fn create_contact_item_body_targets_contacts_folder() {
        let body = create_contact_item_body(&sample_contact_input());
        assert!(body.contains(r#"<t:DistinguishedFolderId Id="contacts"/>"#));
        assert!(body.contains("<t:Contact>"));
    }

    #[test]
    fn update_contact_item_body_uses_indexed_field_uri_for_emails_and_phones() {
        let body = update_contact_item_body("contact-1", "change-1", &sample_contact_input());
        assert!(body.contains(r#"<t:ItemId Id="contact-1" ChangeKey="change-1"/>"#));
        assert!(body.contains(
            r#"<t:IndexedFieldURI FieldURI="contacts:EmailAddress" FieldIndex="EmailAddress1"/>"#
        ));
        assert!(body.contains(
            r#"<t:IndexedFieldURI FieldURI="contacts:PhoneNumber" FieldIndex="BusinessPhone"/>"#
        ));
        assert!(body.contains(
            r#"<t:IndexedFieldURI FieldURI="contacts:PhoneNumber" FieldIndex="BusinessPhone2"/>"#
        ));
    }

    #[test]
    fn delete_contact_item_body_has_no_meeting_attributes() {
        let body = delete_contact_item_body("contact\"1");
        assert!(!body.contains("SendMeetingCancellations"));
        assert!(body.contains("contact&quot;1"));
    }

    #[test]
    fn parses_created_item_and_folder_ids() {
        let item_xml = r#"<CreateItemResponse><Items><CalendarItem><ItemId Id="new-item" ChangeKey="ck"/></CalendarItem></Items></CreateItemResponse>"#;
        assert_eq!(parse_created_item_id(item_xml).unwrap(), "new-item");
        let folder_xml = r#"<CreateFolderResponse><Folders><Folder><FolderId Id="new-folder"/></Folder></Folders></CreateFolderResponse>"#;
        assert_eq!(parse_created_folder_id(folder_xml).unwrap(), "new-folder");
    }
}
