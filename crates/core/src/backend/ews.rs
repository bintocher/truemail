//! Exchange Web Services transport for self-hosted Exchange Server.

use super::{
    DiscoveredFolder, DiscoveredMessage, FolderSyncCursor, ImapDiscovery, MailBackend,
    OutgoingMessage,
};
use crate::account::{
    AuxiliarySyncCursors, ContactInput, DavCalendar, DavCollection, DavContact, DavEvent,
    DavSyncResult, EventInput, SyncScope,
};
use crate::model::{Attendee, ContactAddress, ContactPhone, FolderRole, infer_folder_role};
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

/// Ответ участника EWS (ResponseType) -> PARTSTAT iCalendar. Организатора
/// помечаем ACCEPTED - он не участник в смысле кнопок ответа, а сам инициатор.
fn ews_response_to_partstat(response: &str) -> &'static str {
    match response {
        "Accept" => "ACCEPTED",
        "Decline" => "DECLINED",
        "Tentative" => "TENTATIVE",
        "Organizer" => "ACCEPTED",
        _ => "NEEDS-ACTION",
    }
}

/// Участники CalendarItem: обязательные и необязательные, со статусом ответа.
/// Без них UI не знает свой PARTSTAT и не показывает кнопки ответа на
/// приглашение (resolve_my_attendance по email аккаунта).
fn parse_ews_attendees<'a>(item: Node<'a, 'a>) -> Vec<Attendee> {
    let mut attendees = Vec::new();
    for (container_tag, role) in [
        ("RequiredAttendees", "REQ-PARTICIPANT"),
        ("OptionalAttendees", "OPT-PARTICIPANT"),
    ] {
        let Some(container) = item
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == container_tag)
        else {
            continue;
        };
        for attendee in container
            .children()
            .filter(|node| node.is_element() && node.tag_name().name() == "Attendee")
        {
            let Some(email) = node_text(attendee, "EmailAddress").map(str::to_owned) else {
                continue;
            };
            let partstat = attendee
                .children()
                .find(|node| node.is_element() && node.tag_name().name() == "ResponseType")
                .and_then(|node| node.text())
                .map(ews_response_to_partstat)
                .unwrap_or("NEEDS-ACTION");
            attendees.push(Attendee {
                email,
                name: node_text(attendee, "Name").map(str::to_owned),
                role: Some(role.to_owned()),
                partstat: Some(partstat.to_owned()),
                rsvp: true,
            });
        }
    }
    attendees
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
        let body = r#"<m:FindFolder Traversal="Deep"><m:FolderShape><t:BaseShape>Default</t:BaseShape><t:AdditionalProperties><t:FieldURI FieldURI="folder:ParentFolderId"/></t:AdditionalProperties></m:FolderShape><m:ParentFolderIds><t:DistinguishedFolderId Id="msgfolderroot"/></m:ParentFolderIds></m:FindFolder>"#;
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
    /// Повторяемость (RRULE) переносится в элемент Recurrence: поддержаны
    /// паттерны Daily/Weekly/AbsoluteMonthly/RelativeMonthly/AbsoluteYearly/
    /// RelativeYearly и все три вида диапазона (бесконечный, до даты, по числу
    /// повторений). Правило, которое схема EWS выразить не может, не отправляем
    /// вовсе: событие создаётся одиночным с предупреждением в лог, зато
    /// сохранение не падает - подробнее в recurrence_xml.
    pub async fn create_calendar_item(&self, password: &str, input: &EventInput) -> Result<String> {
        let body = create_calendar_item_body(input)?;
        let response = self.soap(password, "CreateItem", &body).await?;
        parse_created_item_id(&response)
    }

    /// Изменить событие: ChangeKey - свежий (та же оптимистичная блокировка,
    /// что и у respond_to_calendar_item/apply_operation), каждое поле - в
    /// своём SetItemField, поэтому порядок между ними не важен (важен только
    /// порядок полей внутри одного CalendarItem, а тут в каждом ровно одно).
    ///
    /// Заодно спрашиваем, повторяется ли событие сейчас на сервере. Это нужно
    /// для случая "пользователь снял повторение": отсутствие поля в Updates
    /// означает "не менять", то есть серия уцелела бы, а человек видел бы у
    /// себя одиночную встречу - расхождение, которое обнаружится только на
    /// следующей синхронизации. Стирание в EWS - отдельный DeleteItemField, и
    /// слать его вслепую нельзя: на неповторяющемся событии Exchange отвечает
    /// ошибкой. Поэтому сначала IsRecurring, и удаление уходит, только если
    /// повторение там действительно есть.
    pub async fn update_calendar_item(
        &self,
        password: &str,
        item_id: &str,
        input: &EventInput,
    ) -> Result<()> {
        let (change_key, was_recurring) = self.calendar_item_state(password, item_id).await?;
        let body = update_calendar_item_body(item_id, &change_key, input, was_recurring)?;
        self.soap(password, "UpdateItem", &body).await.map(|_| ())
    }

    /// ChangeKey события и признак того, что на сервере это серия. Один GetItem
    /// вместо прежнего item_change_key - лишнего обращения не появилось.
    async fn calendar_item_state(&self, password: &str, item_id: &str) -> Result<(String, bool)> {
        let body = format!(
            r#"<m:GetItem><m:ItemShape><t:BaseShape>IdOnly</t:BaseShape><t:AdditionalProperties><t:FieldURI FieldURI="calendar:IsRecurring"/></t:AdditionalProperties></m:ItemShape><m:ItemIds><t:ItemId Id="{}"/></m:ItemIds></m:GetItem>"#,
            escape(item_id)
        );
        let response = self.soap(password, "GetItem", &body).await?;
        let document = Document::parse(&response).map_err(|error| backend_error("xml", error))?;
        let change_key = document
            .descendants()
            .find(|node| node.is_element() && node.tag_name().name() == "ItemId")
            .and_then(|node| node.attribute("ChangeKey"))
            .map(str::to_owned)
            .ok_or_else(|| backend_error("item", "Exchange не вернул ChangeKey события"))?;
        // Свойства нет у события, созданного не как встреча - считаем такое
        // одиночным: лишний DeleteItemField хуже пропущенного, он сорвал бы
        // сохранение целиком.
        let was_recurring = document
            .descendants()
            .find(|node| node.is_element() && node.tag_name().name() == "IsRecurring")
            .and_then(|node| node.text())
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("true"));
        Ok((change_key, was_recurring))
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
    /// contact_item_updates.
    ///
    /// SetItemField задаёт значение, но убранную запись не стирает: если
    /// пользователь удалил телефон, соответствующий ключ просто не попадает в
    /// Updates, и на сервере остаётся прежнее значение. Стирание в EWS - это
    /// отдельный DeleteItemField с тем же IndexedFieldURI, а чтобы понять,
    /// какие ключи стирать, нужен прежний набор заполненных индексов. Он
    /// берётся из contact_remote_state: одним GetItem, который заодно отдаёт
    /// свежий ChangeKey (то есть лишнего обращения к серверу против прежней
    /// схемы с item_change_key не появилось).
    pub async fn update_contact_item(
        &self,
        password: &str,
        item_id: &str,
        input: &ContactInput,
    ) -> Result<()> {
        let previous = self.contact_remote_state(password, item_id).await?;
        let body = update_contact_item_body(item_id, &previous, input);
        self.soap(password, "UpdateItem", &body).await.map(|_| ())
    }

    /// Прежнее состояние контакта на сервере: ChangeKey плюс ключи уже
    /// заполненных индексированных свойств.
    ///
    /// Запрашиваем словари целиком плоскими FieldURI (contacts:EmailAddresses,
    /// contacts:PhoneNumbers), а не по одному IndexedFieldURI на каждый
    /// возможный ключ: во-первых, это один запрос вместо полутора десятков,
    /// во-вторых, запрос индексированного свойства, которого у элемента нет,
    /// Exchange считает ошибкой, так что перебор ключей пришлось бы ещё и
    /// обкладывать разбором частичных отказов. AllProperties тоже подошёл бы,
    /// но тянет тело письма и вложения контакта - лишний трафик на каждое
    /// сохранение.
    async fn contact_remote_state(
        &self,
        password: &str,
        item_id: &str,
    ) -> Result<ContactRemoteState> {
        let body = format!(
            r#"<m:GetItem><m:ItemShape><t:BaseShape>IdOnly</t:BaseShape><t:AdditionalProperties><t:FieldURI FieldURI="contacts:EmailAddresses"/><t:FieldURI FieldURI="contacts:PhoneNumbers"/><t:FieldURI FieldURI="contacts:PhysicalAddresses"/></t:AdditionalProperties></m:ItemShape><m:ItemIds><t:ItemId Id="{}"/></m:ItemIds></m:GetItem>"#,
            escape(item_id)
        );
        let response = self.soap(password, "GetItem", &body).await?;
        parse_contact_remote_state(&response)
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
        // ParentFolderId связывает папку с родителем. У папок верхнего уровня он
        // указывает на msgfolderroot, которого нет среди синхронизируемых папок,
        // поэтому parent_id при разрешении останется NULL - это и есть корень.
        let parent_remote_path = node
            .children()
            .find(|child| child.is_element() && child.tag_name().name() == "ParentFolderId")
            .and_then(|child| child.attribute("Id"))
            .map(str::to_owned);
        folders.push(DiscoveredFolder {
            remote_path: id.to_owned(),
            display_name: name.to_owned(),
            role: infer_folder_role(name, name),
            parent_remote_path,
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
        // Recurrence есть только у мастер-элемента серии
        // (CalendarItemType=RecurringMaster). CalendarView в calendar_events
        // разворачивает серию в отдельные Occurrence/Exception - у них
        // Recurrence не приходит, и правило останется None: каждое вхождение
        // уже самостоятельное событие со своим ItemId. recurrence_id при этом
        // намеренно не заполняем - мастера серии в базе нет, а непустой
        // recurrence_id пометил бы вхождение исключением несуществующей серии.
        let rrule = item
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "Recurrence")
            .and_then(recurrence_to_rrule);
        let raw = build_vevent(
            id,
            &summary,
            &dtstart,
            dtend.as_deref(),
            location.as_deref(),
            rrule.as_deref(),
        );
        events.push(DavEvent {
            remote_url: Some(format!("ews-event:{id}")),
            uid: id.to_owned(),
            summary,
            description,
            location,
            dtstart,
            dtend,
            rrule,
            recurrence_id: None,
            exdates: None,
            rdates: None,
            status,
            attendees: parse_ews_attendees(item),
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
    rrule: Option<&str>,
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
    // RRULE в теле VEVENT не экранируем: это структурированное значение, а не
    // текст, запятые и точки с запятой в нём значимы.
    if let Some(rrule) = rrule {
        lines.push(format!("RRULE:{rrule}"));
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
        let addresses = item
            .descendants()
            .filter(|node| node.is_element() && node.tag_name().name() == "Entry")
            .filter(|node| {
                node.parent()
                    .is_some_and(|parent| parent.tag_name().name() == "PhysicalAddresses")
            })
            .map(|node| {
                let part = |name: &str| {
                    node_text(node, name)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_owned)
                };
                ContactAddress {
                    kind: node.attribute("Key").map(address_kind),
                    street: part("Street"),
                    city: part("City"),
                    region: part("State"),
                    postal_code: part("PostalCode"),
                    country: part("CountryOrRegion"),
                }
            })
            .filter(|address| !address.is_empty())
            .collect::<Vec<_>>();
        let raw = build_vcard(
            id,
            &display_name,
            first_name.as_deref(),
            last_name.as_deref(),
            organization.as_deref(),
            &emails,
            &phones,
            &addresses,
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
            addresses,
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

/// Ключ словаря PhysicalAddresses (Home|Business|Other) в тип нашей модели.
fn address_kind(key: &str) -> String {
    match key {
        "Home" => "home",
        "Business" => "work",
        _ => "other",
    }
    .to_owned()
}

/// Обратное преобразование: тип адреса в FieldIndex EWS. Второго адреса того
/// же вида словарь не допускает - в отличие от телефонов, "2"-вариантов ключа
/// у PhysicalAddresses в схеме нет, поэтому повторный home/work на сервер не
/// уедет (см. used в contact_addresses_xml).
fn ews_address_key(kind: Option<&str>) -> &'static str {
    match kind {
        Some("home") => "Home",
        Some("work") => "Business",
        _ => "Other",
    }
}

fn build_vcard(
    uid: &str,
    display_name: &str,
    first_name: Option<&str>,
    last_name: Option<&str>,
    organization: Option<&str>,
    emails: &[String],
    phones: &[ContactPhone],
    addresses: &[ContactAddress],
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
    for address in addresses {
        let part = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .map(ical_escape)
                .unwrap_or_default()
        };
        lines.push(format!(
            "ADR;TYPE={}:;;{};{};{};{};{}",
            address.kind.as_deref().unwrap_or("other").to_uppercase(),
            part(&address.street),
            part(&address.city),
            part(&address.region),
            part(&address.postal_code),
            part(&address.country)
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

/// Дни недели: слева значение EWS (DayOfWeekType), справа код BYDAY из
/// RFC5545. Порядок - от воскресенья, как в обоих стандартах.
const RECURRENCE_WEEKDAYS: [(&str, &str); 7] = [
    ("Sunday", "SU"),
    ("Monday", "MO"),
    ("Tuesday", "TU"),
    ("Wednesday", "WE"),
    ("Thursday", "TH"),
    ("Friday", "FR"),
    ("Saturday", "SA"),
];

/// Месяцы EWS (MonthNamesType) по номеру: индекс 0 - январь.
const RECURRENCE_MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// DayOfWeekIndexType в относительных паттернах и его эквивалент в числовом
/// префиксе BYDAY: Last - это -1 ("последний в месяце"), а не 5.
const RECURRENCE_WEEK_INDEXES: [(&str, i32); 5] = [
    ("First", 1),
    ("Second", 2),
    ("Third", 3),
    ("Fourth", 4),
    ("Last", -1),
];

/// Интервал повторения: EWS всегда присылает Interval у Daily/Weekly/Monthly,
/// но подстраховываемся значением 1, чтобы кривой ответ не терял всё правило.
fn recurrence_interval<'a>(pattern: Node<'a, 'a>) -> u32 {
    node_text(pattern, "Interval")
        .and_then(|value| value.trim().parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1)
}

/// INTERVAL=1 - значение по умолчанию в RFC5545, в строку его не пишем: так
/// правило совпадает с тем, что присылают CalDAV/Google, и не сбивает
/// разворачивание серии в UI.
fn interval_part(interval: u32) -> String {
    if interval > 1 {
        format!(";INTERVAL={interval}")
    } else {
        String::new()
    }
}

/// DaysOfWeek в EWS - список через пробел, где кроме конкретных дней бывают
/// групповые значения Day/Weekday/WeekendDay. В RFC5545 групп нет, поэтому
/// раскрываем их в перечисление дней.
fn ews_days_to_ical(value: &str) -> Option<Vec<&'static str>> {
    let mut days: Vec<&'static str> = Vec::new();
    for token in value.split_whitespace() {
        let expanded: &[&'static str] = match token {
            "Day" => &["SU", "MO", "TU", "WE", "TH", "FR", "SA"],
            "Weekday" => &["MO", "TU", "WE", "TH", "FR"],
            "WeekendDay" => &["SA", "SU"],
            _ => {
                days.push(ews_single_day_to_ical(token)?);
                continue;
            }
        };
        days.extend_from_slice(expanded);
    }
    let mut unique: Vec<&'static str> = Vec::new();
    for day in days {
        if !unique.contains(&day) {
            unique.push(day);
        }
    }
    (!unique.is_empty()).then_some(unique)
}

/// Относительные паттерны (BYDAY вида "2MO") описывают ровно один день недели:
/// групповые значения и списки в такую запись не укладываются.
fn ews_single_day_to_ical(value: &str) -> Option<&'static str> {
    RECURRENCE_WEEKDAYS
        .iter()
        .find(|(ews, _)| *ews == value)
        .map(|(_, ical)| *ical)
}

fn ical_day_to_ews(value: &str) -> Option<&'static str> {
    RECURRENCE_WEEKDAYS
        .iter()
        .find(|(_, ical)| *ical == value)
        .map(|(ews, _)| *ews)
}

fn ews_month_to_number(value: &str) -> Option<u32> {
    RECURRENCE_MONTHS
        .iter()
        .position(|month| *month == value)
        .and_then(|index| u32::try_from(index + 1).ok())
}

fn month_number_to_ews(value: u32) -> Option<&'static str> {
    usize::try_from(value)
        .ok()
        .and_then(|value| value.checked_sub(1))
        .and_then(|index| RECURRENCE_MONTHS.get(index))
        .copied()
}

fn ews_week_index_to_position(value: &str) -> Option<i32> {
    RECURRENCE_WEEK_INDEXES
        .iter()
        .find(|(ews, _)| *ews == value)
        .map(|(_, position)| *position)
}

fn position_to_ews_week_index(value: i32) -> Option<&'static str> {
    RECURRENCE_WEEK_INDEXES
        .iter()
        .find(|(_, position)| *position == value)
        .map(|(ews, _)| *ews)
}

/// EndDate в EWS - дата без времени ("2026-12-31" либо со смещением
/// "2026-12-31+03:00"). В RFC5545 UNTIL должен быть того же типа, что DTSTART,
/// а DTSTART мы отдаём как момент в UTC (см. to_ical_datetime), поэтому берём
/// конец указанных суток.
fn ical_until_from_ews_date(value: &str) -> Option<String> {
    let date = ews_date_prefix(value)?;
    Some(format!("UNTIL={}T235959Z", date.replace('-', "")))
}

/// Первые 10 символов значения, если это дата вида YYYY-MM-DD.
fn ews_date_prefix(value: &str) -> Option<String> {
    let date = value.trim().get(..10)?;
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .ok()
        .map(|date| date.format("%Y-%m-%d").to_string())
}

/// Обратное преобразование: UNTIL=20261231T235959Z (или просто 20261231) в
/// дату EWS. Время из UNTIL отбрасываем - EndDate его не хранит, серия
/// заканчивается указанным днём включительно в обоих представлениях.
fn ews_date_from_ical_until(value: &str) -> Option<String> {
    let digits: String = value
        .trim()
        .chars()
        .take_while(|symbol| *symbol != 'T' && *symbol != 't')
        .filter(|symbol| symbol.is_ascii_digit())
        .collect();
    chrono::NaiveDate::parse_from_str(digits.get(..8)?, "%Y%m%d")
        .ok()
        .map(|date| date.format("%Y-%m-%d").to_string())
}

/// Собрать RRULE из элемента t:Recurrence ответа EWS.
///
/// Recurrence приходит только у мастер-элемента серии
/// (CalendarItemType=RecurringMaster). Если паттерн распознать не удалось,
/// возвращаем None: событие останется одиночным, но не потеряется целиком.
fn recurrence_to_rrule<'a>(recurrence: Node<'a, 'a>) -> Option<String> {
    let mut pattern: Option<String> = None;
    let mut bound: Option<String> = None;
    for child in recurrence.children().filter(|node| node.is_element()) {
        let name = child.tag_name().name();
        match name {
            "DailyRecurrence"
            | "WeeklyRecurrence"
            | "AbsoluteMonthlyRecurrence"
            | "RelativeMonthlyRecurrence"
            | "AbsoluteYearlyRecurrence"
            | "RelativeYearlyRecurrence" => {
                pattern = recurrence_pattern_to_rrule(child, name);
                if pattern.is_none() {
                    tracing::warn!(
                        pattern = name,
                        "EWS: паттерн повторения не разобран, событие показано одиночным"
                    );
                    return None;
                }
            }
            // NoEndRecurrence - бесконечная серия, в RRULE это просто
            // отсутствие UNTIL/COUNT.
            "NoEndRecurrence" => {}
            "EndDateRecurrence" => {
                bound = node_text(child, "EndDate").and_then(ical_until_from_ews_date);
            }
            "NumberedRecurrence" => {
                bound = node_text(child, "NumberOfOccurrences")
                    .and_then(|value| value.trim().parse::<u32>().ok())
                    .filter(|value| *value > 0)
                    .map(|value| format!("COUNT={value}"));
            }
            // Прочие дочерние элементы (например DailyRegeneration у задач)
            // игнорируем: паттерн так и останется None и правило не появится.
            _ => {}
        }
    }
    let pattern = pattern?;
    Some(match bound {
        Some(bound) => format!("{pattern};{bound}"),
        None => pattern,
    })
}

fn recurrence_pattern_to_rrule<'a>(pattern: Node<'a, 'a>, name: &str) -> Option<String> {
    let interval = interval_part(recurrence_interval(pattern));
    match name {
        "DailyRecurrence" => Some(format!("FREQ=DAILY{interval}")),
        "WeeklyRecurrence" => {
            let days = ews_days_to_ical(node_text(pattern, "DaysOfWeek")?)?;
            // FirstDayOfWeek в RRULE соответствует WKST, но в строку его не
            // пишем: значение по умолчанию (понедельник) на результат не
            // влияет, а лишний параметр ломает простые разворачиватели серий.
            Some(format!("FREQ=WEEKLY{interval};BYDAY={}", days.join(",")))
        }
        "AbsoluteMonthlyRecurrence" => {
            let day = node_text(pattern, "DayOfMonth")?
                .trim()
                .parse::<u32>()
                .ok()?;
            Some(format!("FREQ=MONTHLY{interval};BYMONTHDAY={day}"))
        }
        "RelativeMonthlyRecurrence" => {
            let day = ews_single_day_to_ical(node_text(pattern, "DaysOfWeek")?.trim())?;
            let position =
                ews_week_index_to_position(node_text(pattern, "DayOfWeekIndex")?.trim())?;
            Some(format!("FREQ=MONTHLY{interval};BYDAY={position}{day}"))
        }
        "AbsoluteYearlyRecurrence" => {
            // У годовых паттернов схема EWS не предусматривает Interval -
            // серия всегда ежегодная, поэтому INTERVAL не выводим.
            let day = node_text(pattern, "DayOfMonth")?
                .trim()
                .parse::<u32>()
                .ok()?;
            let month = ews_month_to_number(node_text(pattern, "Month")?.trim())?;
            Some(format!("FREQ=YEARLY;BYMONTHDAY={day};BYMONTH={month}"))
        }
        "RelativeYearlyRecurrence" => {
            let day = ews_single_day_to_ical(node_text(pattern, "DaysOfWeek")?.trim())?;
            let position =
                ews_week_index_to_position(node_text(pattern, "DayOfWeekIndex")?.trim())?;
            let month = ews_month_to_number(node_text(pattern, "Month")?.trim())?;
            Some(format!("FREQ=YEARLY;BYDAY={position}{day};BYMONTH={month}"))
        }
        _ => None,
    }
}

/// XML t:Recurrence для CalendarItem по RRULE события.
///
/// Если правило нельзя выразить схемой EWS (BYSETPOS, FREQ=HOURLY и прочее),
/// возвращаем None и пишем предупреждение: событие уедет на сервер одиночным.
/// Отказывать в сохранении здесь нельзя - пользователь потеряет всю правку
/// из-за одной непереносимой детали правила, а отправлять приблизительный
/// паттерн ещё хуже: серия молча начнёт повторяться не тогда, когда задумано.
fn recurrence_xml(input: &EventInput) -> Option<String> {
    let rrule = input.rrule.as_deref()?.trim();
    if rrule.is_empty() {
        return None;
    }
    match build_recurrence_xml(rrule, &input.dtstart) {
        Some(xml) => Some(xml),
        None => {
            tracing::warn!(
                rrule = %rrule,
                "EWS: правило повторения не выражается схемой Recurrence, событие сохранено одиночным"
            );
            None
        }
    }
}

/// Пары ключ-значение RRULE. Ключи приводим к верхнему регистру: RFC5545
/// разрешает любой, а сравнивать удобнее с одним.
fn parse_rrule_parts(rrule: &str) -> Option<Vec<(String, String)>> {
    let value = rrule.trim();
    let value = value.strip_prefix("RRULE:").unwrap_or(value);
    let mut parts = Vec::new();
    for chunk in value.split(';').filter(|chunk| !chunk.trim().is_empty()) {
        let (key, raw) = chunk.split_once('=')?;
        parts.push((
            key.trim().to_uppercase(),
            raw.trim().to_uppercase().replace(' ', ""),
        ));
    }
    (!parts.is_empty()).then_some(parts)
}

/// Дата начала серии для RecurrenceRange: берём её из dtstart события.
/// StartDate обязателен во всех трёх вариантах диапазона.
fn recurrence_start_date(dtstart: &str) -> Option<String> {
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(dtstart) {
        return Some(
            parsed
                .with_timezone(&chrono::Utc)
                .format("%Y-%m-%d")
                .to_string(),
        );
    }
    ews_date_prefix(dtstart)
}

fn recurrence_start_naive_date(dtstart: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(&recurrence_start_date(dtstart)?, "%Y-%m-%d").ok()
}

/// BYDAY без числового префикса: "MO,WE" в "Monday Wednesday".
fn ical_days_to_ews(value: &str) -> Option<Vec<&'static str>> {
    let mut days = Vec::new();
    for token in value.split(',').filter(|token| !token.trim().is_empty()) {
        let day = ical_day_to_ews(token.trim())?;
        if !days.contains(&day) {
            days.push(day);
        }
    }
    (!days.is_empty()).then_some(days)
}

/// BYDAY с числовым префиксом и ровно одним днём: "2MO" в (Second, Monday).
/// Список дней с позицией (например "1MO,1TU") схема EWS не выражает.
fn ical_positional_day_to_ews(value: &str) -> Option<(&'static str, &'static str)> {
    let token = value.trim();
    // Только ASCII: иначе split_at ниже может разрезать многобайтовый символ.
    if token.contains(',') || !token.is_ascii() {
        return None;
    }
    let split = token.len().checked_sub(2)?;
    let (prefix, day) = token.split_at(split);
    let day = ical_day_to_ews(day)?;
    let position = prefix.parse::<i32>().ok()?;
    Some((position_to_ews_week_index(position)?, day))
}

fn build_recurrence_xml(rrule: &str, dtstart: &str) -> Option<String> {
    let parts = parse_rrule_parts(rrule)?;
    let start_date = recurrence_start_date(dtstart)?;
    let start = recurrence_start_naive_date(dtstart)?;
    let mut freq: Option<String> = None;
    let mut interval = 1u32;
    let mut by_day: Option<String> = None;
    let mut by_month_day: Option<String> = None;
    let mut by_month: Option<String> = None;
    let mut count: Option<u32> = None;
    let mut until: Option<String> = None;
    let mut week_start: Option<String> = None;
    for (key, value) in parts {
        match key.as_str() {
            "FREQ" => freq = Some(value),
            "INTERVAL" => interval = value.parse::<u32>().ok().filter(|value| *value > 0)?,
            "BYDAY" => by_day = Some(value),
            "BYMONTHDAY" => by_month_day = Some(value),
            "BYMONTH" => by_month = Some(value),
            "COUNT" => count = Some(value.parse::<u32>().ok().filter(|value| *value > 0)?),
            "UNTIL" => until = Some(value),
            "WKST" => week_start = Some(value),
            // BYSETPOS, BYWEEKNO, BYYEARDAY, BYHOUR и прочее схема EWS не
            // выражает вовсе - отказываемся от всего правила целиком.
            _ => return None,
        }
    }
    // COUNT и UNTIL по RFC5545 взаимоисключающи, а в EWS это вообще разные
    // типы диапазона - такое правило считаем непереносимым.
    if count.is_some() && until.is_some() {
        return None;
    }
    let pattern = match freq.as_deref()? {
        "DAILY" => {
            if by_day.is_some() || by_month_day.is_some() || by_month.is_some() {
                return None;
            }
            format!("<t:DailyRecurrence><t:Interval>{interval}</t:Interval></t:DailyRecurrence>")
        }
        "WEEKLY" => {
            if by_month_day.is_some() || by_month.is_some() {
                return None;
            }
            let days = match by_day.as_deref() {
                Some(value) => ical_days_to_ews(value)?,
                // BYDAY в еженедельном правиле необязателен - тогда день
                // берётся из DTSTART. EWS же требует DaysOfWeek явно.
                None => vec![weekday_to_ews(start)],
            };
            let first = match week_start.as_deref() {
                Some(value) => ical_day_to_ews(value)?,
                None => "Monday",
            };
            format!(
                "<t:WeeklyRecurrence><t:Interval>{interval}</t:Interval><t:DaysOfWeek>{}</t:DaysOfWeek><t:FirstDayOfWeek>{first}</t:FirstDayOfWeek></t:WeeklyRecurrence>",
                days.join(" ")
            )
        }
        "MONTHLY" => {
            if by_month.is_some() {
                return None;
            }
            match (by_day.as_deref(), by_month_day.as_deref()) {
                (Some(_), Some(_)) => return None,
                (Some(day), None) => {
                    let (index, day) = ical_positional_day_to_ews(day)?;
                    format!(
                        "<t:RelativeMonthlyRecurrence><t:Interval>{interval}</t:Interval><t:DaysOfWeek>{day}</t:DaysOfWeek><t:DayOfWeekIndex>{index}</t:DayOfWeekIndex></t:RelativeMonthlyRecurrence>"
                    )
                }
                (None, day) => {
                    let day = match day {
                        Some(day) => parse_month_day(day)?,
                        None => chrono::Datelike::day(&start),
                    };
                    format!(
                        "<t:AbsoluteMonthlyRecurrence><t:Interval>{interval}</t:Interval><t:DayOfMonth>{day}</t:DayOfMonth></t:AbsoluteMonthlyRecurrence>"
                    )
                }
            }
        }
        "YEARLY" => {
            // AbsoluteYearlyRecurrencePatternType/RelativeYearlyRecurrencePatternType
            // не имеют Interval: "раз в N лет" схемой не выражается, и отправить
            // такое правило как ежегодное было бы искажением.
            if interval != 1 {
                return None;
            }
            let month = match by_month.as_deref() {
                Some(value) => month_number_to_ews(value.parse::<u32>().ok()?)?,
                None => month_number_to_ews(chrono::Datelike::month(&start))?,
            };
            match (by_day.as_deref(), by_month_day.as_deref()) {
                (Some(_), Some(_)) => return None,
                (Some(day), None) => {
                    let (index, day) = ical_positional_day_to_ews(day)?;
                    format!(
                        "<t:RelativeYearlyRecurrence><t:DaysOfWeek>{day}</t:DaysOfWeek><t:DayOfWeekIndex>{index}</t:DayOfWeekIndex><t:Month>{month}</t:Month></t:RelativeYearlyRecurrence>"
                    )
                }
                (None, day) => {
                    let day = match day {
                        Some(day) => parse_month_day(day)?,
                        None => chrono::Datelike::day(&start),
                    };
                    format!(
                        "<t:AbsoluteYearlyRecurrence><t:DayOfMonth>{day}</t:DayOfMonth><t:Month>{month}</t:Month></t:AbsoluteYearlyRecurrence>"
                    )
                }
            }
        }
        // FREQ=SECONDLY/MINUTELY/HOURLY у EWS аналога нет.
        _ => return None,
    };
    let range = match (count, until) {
        (Some(count), _) => format!(
            "<t:NumberedRecurrence><t:StartDate>{start_date}</t:StartDate><t:NumberOfOccurrences>{count}</t:NumberOfOccurrences></t:NumberedRecurrence>"
        ),
        (None, Some(until)) => {
            let end = ews_date_from_ical_until(&until)?;
            format!(
                "<t:EndDateRecurrence><t:StartDate>{start_date}</t:StartDate><t:EndDate>{end}</t:EndDate></t:EndDateRecurrence>"
            )
        }
        (None, None) => format!(
            "<t:NoEndRecurrence><t:StartDate>{start_date}</t:StartDate></t:NoEndRecurrence>"
        ),
    };
    Some(format!("<t:Recurrence>{pattern}{range}</t:Recurrence>"))
}

/// BYMONTHDAY: EWS хранит номер дня 1..31 и отрицательных значений ("-1" -
/// последний день месяца) не поддерживает.
fn parse_month_day(value: &str) -> Option<u32> {
    if value.contains(',') {
        return None;
    }
    value
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|day| (1..=31).contains(day))
}

fn weekday_to_ews(date: chrono::NaiveDate) -> &'static str {
    usize::try_from(chrono::Datelike::weekday(&date).num_days_from_sunday())
        .ok()
        .and_then(|index| RECURRENCE_WEEKDAYS.get(index))
        .map(|(ews, _)| *ews)
        .unwrap_or("Monday")
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
/// наследует ItemType). Сначала общая часть ItemType: Subject, Body,
/// ReminderIsSet, ReminderMinutesBeforeStart. Дальше calendar-специфичная:
/// Start, End, IsAllDayEvent, Location, RequiredAttendees, OptionalAttendees.
/// Для CreateItem все поля идут одним блоком `t:CalendarItem` и порядок
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
    // В схеме CalendarItemType Recurrence идёт после списков участников (перед
    // FirstOccurrence/ModifiedOccurrences, которые мы не отправляем), поэтому
    // поле добавляется последним. Если правило не выразимо в EWS, поля просто
    // не будет и Exchange создаст одиночное событие - см. recurrence_xml.
    //
    // Обратная сторона для UpdateItem: снятое пользователем повторение здесь
    // превращается в отсутствие поля, то есть серия на сервере уцелеет. Стереть
    // её - это отдельный DeleteItemField, который на неповторяющемся событии
    // Exchange может отклонить, поэтому такой запрос не шлём.
    if let Some(recurrence) = recurrence_xml(input) {
        fields.push(("calendar:Recurrence", recurrence));
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
    was_recurring: bool,
) -> Result<String> {
    let disposition = calendar_send_disposition(input);
    let fields = calendar_item_fields(input)?;
    // Именно пустое правило, а не отсутствие поля в Updates: поля не будет и
    // тогда, когда правило есть, но схема EWS его не выражает (BYSETPOS и
    // прочее из recurrence_xml). Спутать эти случаи - значит стереть на сервере
    // серию, которую пользователь не трогал.
    let dropped_by_user = input
        .rrule
        .as_deref()
        .map(str::trim)
        .is_none_or(str::is_empty);
    let mut updates = fields
        .into_iter()
        .map(|(field_uri, xml)| {
            format!(
                r#"<t:SetItemField><t:FieldURI FieldURI="{field_uri}"/><t:CalendarItem>{xml}</t:CalendarItem></t:SetItemField>"#
            )
        })
        .collect::<String>();
    // Повторение было, а нового правила нет - значит его сняли, и серию надо
    // стереть явно. Условие про was_recurring обязательно: DeleteItemField по
    // calendar:Recurrence на одиночном событии Exchange считает ошибкой и
    // отклоняет весь UpdateItem, то есть обычное редактирование встречи
    // перестало бы сохраняться.
    if was_recurring && dropped_by_user {
        updates
            .push_str(r#"<t:DeleteItemField><t:FieldURI FieldURI="calendar:Recurrence"/></t:DeleteItemField>"#);
    }
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

/// Подполя адреса в порядке схемы EWS (PhysicalAddressType): Street, City,
/// State, CountryOrRegion, PostalCode. Порядок здесь не косметика - Exchange
/// отвергает Entry с переставленными элементами.
const ADDRESS_PARTS: [&str; 5] = ["Street", "City", "State", "CountryOrRegion", "PostalCode"];

fn address_part<'a>(address: &'a ContactAddress, part: &str) -> Option<&'a str> {
    let value = match part {
        "Street" => &address.street,
        "City" => &address.city,
        "State" => &address.region,
        "CountryOrRegion" => &address.country,
        _ => &address.postal_code,
    };
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn contact_addresses_xml(addresses: &[ContactAddress]) -> String {
    let mut used = HashSet::new();
    addresses
        .iter()
        .filter(|address| !address.is_empty())
        .filter_map(|address| {
            let key = ews_address_key(address.kind.as_deref());
            if !used.insert(key) {
                return None;
            }
            let body = ADDRESS_PARTS
                .iter()
                .filter_map(|part| {
                    address_part(address, part)
                        .map(|value| format!("<t:{part}>{}</t:{part}>", escape(value)))
                })
                .collect::<String>();
            Some(format!(r#"<t:Entry Key="{key}">{body}</t:Entry>"#))
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
    // PhysicalAddresses в ContactItemType стоят между EmailAddresses и
    // PhoneNumbers - порядок полей внутри t:Contact схема проверяет.
    let addresses = contact_addresses_xml(&input.addresses);
    if !addresses.is_empty() {
        body.push_str(&format!(
            "<t:PhysicalAddresses>{addresses}</t:PhysicalAddresses>"
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

/// Снимок индексированных свойств контакта на сервере перед обновлением:
/// ChangeKey для оптимистичной блокировки и ключи (Entry Key) уже заполненных
/// записей словарей. Нужен ровно для одного - вычислить, какие ключи исчезли
/// в новых данных и должны уйти в DeleteItemField.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct ContactRemoteState {
    change_key: String,
    email_keys: Vec<String>,
    phone_keys: Vec<String>,
    /// Заполненные подполя почтовых адресов парами (FieldIndex, элемент):
    /// ("Home", "Street"). Индексированное свойство здесь - каждое подполе
    /// по отдельности (contacts:PhysicalAddress:Street), так что и удалять
    /// приходится их поштучно.
    address_fields: Vec<(String, String)>,
}

/// Ключи словарной записи EWS отдаёт атрибутом Key у t:Entry. Пустые Entry
/// (сервер иногда возвращает их для очищенных полей) считаем незаполненными -
/// иначе на них выписался бы бессмысленный DeleteItemField.
fn contact_entry_keys(item: Node<'_, '_>, dictionary: &str) -> Vec<String> {
    item.descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "Entry")
        .filter(|node| {
            node.parent()
                .is_some_and(|parent| parent.tag_name().name() == dictionary)
        })
        .filter(|node| node.text().is_some_and(|value| !value.trim().is_empty()))
        .filter_map(|node| node.attribute("Key").map(str::to_owned))
        .collect()
}

/// В отличие от почт и телефонов, Entry словаря PhysicalAddresses не содержит
/// текста - только вложенные Street/City/State/CountryOrRegion/PostalCode.
/// Поэтому contact_entry_keys здесь не годится: он отбросил бы такие Entry как
/// пустые, и убранный адрес остался бы на сервере навсегда.
fn contact_address_fields(item: Node<'_, '_>) -> Vec<(String, String)> {
    item.descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "Entry")
        .filter(|node| {
            node.parent()
                .is_some_and(|parent| parent.tag_name().name() == "PhysicalAddresses")
        })
        .filter_map(|node| node.attribute("Key").map(|key| (key, node)))
        .flat_map(|(key, node)| {
            node.children()
                .filter(|child| child.is_element())
                .filter(|child| child.text().is_some_and(|value| !value.trim().is_empty()))
                .filter(|child| ADDRESS_PARTS.contains(&child.tag_name().name()))
                .map(|child| (key.to_owned(), child.tag_name().name().to_owned()))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn parse_contact_remote_state(xml: &str) -> Result<ContactRemoteState> {
    let document = Document::parse(xml).map_err(|error| backend_error("contacts-xml", error))?;
    let item = document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "Contact")
        .ok_or_else(|| backend_error("contact", "Exchange не вернул контакт по идентификатору"))?;
    let change_key = item
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "ItemId")
        .and_then(|node| node.attribute("ChangeKey"))
        .ok_or_else(|| backend_error("contact", "Exchange не вернул ChangeKey контакта"))?
        .to_owned();
    Ok(ContactRemoteState {
        change_key,
        email_keys: contact_entry_keys(item, "EmailAddresses"),
        phone_keys: contact_entry_keys(item, "PhoneNumbers"),
        address_fields: contact_address_fields(item),
    })
}

fn update_contact_item_body(
    item_id: &str,
    previous: &ContactRemoteState,
    input: &ContactInput,
) -> String {
    let updates = contact_item_updates(input, previous);
    format!(
        r#"<m:UpdateItem ConflictResolution="AutoResolve"><m:ItemChanges><t:ItemChange><t:ItemId Id="{}" ChangeKey="{}"/><t:Updates>{updates}</t:Updates></t:ItemChange></m:ItemChanges></m:UpdateItem>"#,
        escape(item_id),
        escape(&previous.change_key)
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
///
/// Хвостом идут DeleteItemField по тем ключам, которые были заполнены на
/// сервере (previous) и не попали в новый набор - именно они стирают убранные
/// телефоны, адреса электронной почты и почтовые адреса.
/// Схема EWS (NonEmptyArrayOfItemChangeDescriptionsType)
/// это choice с unbounded, порядок между Set и Delete ей безразличен, важен
/// лишь порядок полей внутри одного t:Contact - а тут в каждом SetItemField
/// ровно одно поле, ровно как в update_calendar_item. Delete держим в конце
/// осознанно: так Exchange сначала запишет новые значения ключей, а потом
/// уберёт лишние, и промежуточного состояния "всё стёрли, ничего не записали"
/// не возникает даже при частичном применении.
fn contact_item_updates(input: &ContactInput, previous: &ContactRemoteState) -> String {
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
    let mut email_keys = Vec::new();
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
        email_keys.push(key);
    }
    let mut used = HashSet::new();
    let mut phone_keys = Vec::new();
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
        phone_keys.push(key.to_owned());
    }
    // Каждое подполе адреса - собственное индексированное свойство, поэтому на
    // один адрес приходится до пяти SetItemField.
    let mut used_addresses = HashSet::new();
    let mut address_fields = Vec::new();
    for address in input.addresses.iter().filter(|value| !value.is_empty()) {
        let key = ews_address_key(address.kind.as_deref());
        if !used_addresses.insert(key) {
            continue;
        }
        for part in ADDRESS_PARTS {
            let Some(value) = address_part(address, part) else {
                continue;
            };
            updates.push_str(&format!(
                r#"<t:SetItemField><t:IndexedFieldURI FieldURI="contacts:PhysicalAddress:{part}" FieldIndex="{key}"/><t:Contact><t:PhysicalAddresses><t:Entry Key="{key}"><t:{part}>{}</t:{part}></t:Entry></t:PhysicalAddresses></t:Contact></t:SetItemField>"#,
                escape(value)
            ));
            address_fields.push((key.to_owned(), part.to_owned()));
        }
    }
    updates.push_str(&contact_deleted_index_fields(
        "contacts:EmailAddress",
        &previous.email_keys,
        &email_keys,
    ));
    // Ключи телефонов сравниваем как есть, включая те, что наш редактор
    // выставить не умеет (AssistantPhone, Callback, CarPhone и прочие из
    // словаря EWS): читаются они в UI как обычные телефоны (см. phone_kind),
    // значит пользователь их видит, и убранная им запись должна исчезнуть, а
    // не остаться на сервере под ключом, которого нет в ews_phone_key.
    updates.push_str(&contact_deleted_index_fields(
        "contacts:PhoneNumber",
        &previous.phone_keys,
        &phone_keys,
    ));
    updates.push_str(&contact_deleted_address_fields(
        &previous.address_fields,
        &address_fields,
    ));
    updates
}

/// DeleteItemField по подполям адресов, которые были заполнены на сервере и
/// пропали из новых данных. Отдельно от contact_deleted_index_fields: у
/// адресов FieldURI зависит от подполя, а ключ (FieldIndex) - от вида адреса.
fn contact_deleted_address_fields(
    previous: &[(String, String)],
    current: &[(String, String)],
) -> String {
    let current: HashSet<(&str, &str)> = current
        .iter()
        .map(|(key, part)| (key.as_str(), part.as_str()))
        .collect();
    let mut emitted = HashSet::new();
    previous
        .iter()
        .filter(|(key, part)| !current.contains(&(key.as_str(), part.as_str())))
        .filter(|(key, part)| emitted.insert((key.as_str(), part.as_str())))
        .map(|(key, part)| {
            format!(
                r#"<t:DeleteItemField><t:IndexedFieldURI FieldURI="contacts:PhysicalAddress:{}" FieldIndex="{}"/></t:DeleteItemField>"#,
                escape(part),
                escape(key)
            )
        })
        .collect()
}

/// DeleteItemField по каждому ключу, который был на сервере, но пропал из
/// новых данных. Дубликаты в previous (сервер такого не присылает, но парсер
/// их не запрещает) схлопываем: два DeleteItemField на один индекс Exchange
/// отвергает как ErrorIncorrectUpdatePropertyCount.
fn contact_deleted_index_fields(
    field_uri: &str,
    previous: &[String],
    current: &[String],
) -> String {
    let current: HashSet<&str> = current.iter().map(String::as_str).collect();
    let mut emitted = HashSet::new();
    previous
        .iter()
        .filter(|key| !current.contains(key.as_str()))
        .filter(|key| emitted.insert(key.as_str()))
        .map(|key| {
            format!(
                r#"<t:DeleteItemField><t:IndexedFieldURI FieldURI="{field_uri}" FieldIndex="{}"/></t:DeleteItemField>"#,
                escape(key)
            )
        })
        .collect()
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
        let body = update_calendar_item_body("item-1", "change-1", &sample_event_input(), false)
            .expect("build body");
        assert!(body.contains(r#"<t:ItemId Id="item-1" ChangeKey="change-1"/>"#));
        assert!(body.contains(r#"SendMeetingInvitationsOrCancellations="SendToNone""#));
        assert!(body.contains(r#"<t:FieldURI FieldURI="item:Subject"/>"#));
        assert!(body.contains(r#"<t:FieldURI FieldURI="calendar:Start"/>"#));
    }

    /// Разобрать фрагмент t:Recurrence в RRULE. Префикс t: в тестовом XML не
    /// объявлен, поэтому оборачиваем фрагмент в корень с namespace.
    fn rrule_from_recurrence(fragment: &str) -> Option<String> {
        let xml = format!(r#"<root xmlns:t="ews-types">{fragment}</root>"#);
        let document = Document::parse(&xml).expect("parse recurrence xml");
        let node = document
            .descendants()
            .find(|node| node.is_element() && node.tag_name().name() == "Recurrence")
            .expect("recurrence node");
        recurrence_to_rrule(node)
    }

    fn recurrence_of(rrule: &str) -> String {
        build_recurrence_xml(rrule, "2026-07-20T10:00:00+03:00").expect("build recurrence xml")
    }

    #[test]
    fn parses_every_supported_recurrence_pattern_into_rrule() {
        let cases = [
            (
                "<t:DailyRecurrence><t:Interval>3</t:Interval></t:DailyRecurrence>",
                "FREQ=DAILY;INTERVAL=3",
            ),
            (
                "<t:WeeklyRecurrence><t:Interval>2</t:Interval><t:DaysOfWeek>Monday Tuesday</t:DaysOfWeek><t:FirstDayOfWeek>Monday</t:FirstDayOfWeek></t:WeeklyRecurrence>",
                "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,TU",
            ),
            (
                "<t:AbsoluteMonthlyRecurrence><t:Interval>1</t:Interval><t:DayOfMonth>15</t:DayOfMonth></t:AbsoluteMonthlyRecurrence>",
                "FREQ=MONTHLY;BYMONTHDAY=15",
            ),
            (
                "<t:RelativeMonthlyRecurrence><t:Interval>2</t:Interval><t:DaysOfWeek>Monday</t:DaysOfWeek><t:DayOfWeekIndex>Second</t:DayOfWeekIndex></t:RelativeMonthlyRecurrence>",
                "FREQ=MONTHLY;INTERVAL=2;BYDAY=2MO",
            ),
            (
                "<t:AbsoluteYearlyRecurrence><t:DayOfMonth>9</t:DayOfMonth><t:Month>May</t:Month></t:AbsoluteYearlyRecurrence>",
                "FREQ=YEARLY;BYMONTHDAY=9;BYMONTH=5",
            ),
            (
                "<t:RelativeYearlyRecurrence><t:DaysOfWeek>Friday</t:DaysOfWeek><t:DayOfWeekIndex>Last</t:DayOfWeekIndex><t:Month>November</t:Month></t:RelativeYearlyRecurrence>",
                "FREQ=YEARLY;BYDAY=-1FR;BYMONTH=11",
            ),
        ];
        for (pattern, expected) in cases {
            let fragment = format!(
                "<t:Recurrence>{pattern}<t:NoEndRecurrence><t:StartDate>2026-07-20</t:StartDate></t:NoEndRecurrence></t:Recurrence>"
            );
            assert_eq!(rrule_from_recurrence(&fragment).as_deref(), Some(expected));
        }
    }

    #[test]
    fn parses_recurrence_bounds_into_until_and_count() {
        let until = rrule_from_recurrence(
            "<t:Recurrence><t:DailyRecurrence><t:Interval>1</t:Interval></t:DailyRecurrence><t:EndDateRecurrence><t:StartDate>2026-07-20</t:StartDate><t:EndDate>2026-12-31+03:00</t:EndDate></t:EndDateRecurrence></t:Recurrence>",
        );
        assert_eq!(until.as_deref(), Some("FREQ=DAILY;UNTIL=20261231T235959Z"));
        let count = rrule_from_recurrence(
            "<t:Recurrence><t:DailyRecurrence><t:Interval>1</t:Interval></t:DailyRecurrence><t:NumberedRecurrence><t:StartDate>2026-07-20</t:StartDate><t:NumberOfOccurrences>5</t:NumberOfOccurrences></t:NumberedRecurrence></t:Recurrence>",
        );
        assert_eq!(count.as_deref(), Some("FREQ=DAILY;COUNT=5"));
    }

    #[test]
    fn unknown_recurrence_pattern_yields_no_rrule() {
        // Паттерн задач (Regeneration) календарным событием не выражается -
        // правило не появится, но и разбор элемента не сломается.
        let rrule = rrule_from_recurrence(
            "<t:Recurrence><t:DailyRegeneration><t:Interval>1</t:Interval></t:DailyRegeneration><t:NoEndRecurrence><t:StartDate>2026-07-20</t:StartDate></t:NoEndRecurrence></t:Recurrence>",
        );
        assert_eq!(rrule, None);
    }

    #[test]
    fn calendar_item_carries_recurring_master_rule_into_vevent() {
        let xml = r#"<Envelope><CalendarItem><ItemId Id="series-1"/><Subject>Планёрка</Subject><Start>2026-07-20T07:00:00Z</Start><End>2026-07-20T08:00:00Z</End><IsAllDayEvent>false</IsAllDayEvent><CalendarItemType>RecurringMaster</CalendarItemType><Recurrence><WeeklyRecurrence><Interval>1</Interval><DaysOfWeek>Monday</DaysOfWeek><FirstDayOfWeek>Monday</FirstDayOfWeek></WeeklyRecurrence><NoEndRecurrence><StartDate>2026-07-20</StartDate></NoEndRecurrence></Recurrence></CalendarItem></Envelope>"#;
        let events = parse_calendar_items(xml).expect("calendar response");
        assert_eq!(events[0].rrule.as_deref(), Some("FREQ=WEEKLY;BYDAY=MO"));
        assert!(events[0].raw.contains("RRULE:FREQ=WEEKLY;BYDAY=MO"));
        // Отдельное вхождение серии повторения не несёт и исключением серии не
        // становится - у него собственный ItemId и собственная дата.
        let occurrence = r#"<Envelope><CalendarItem><ItemId Id="occurrence-1"/><Subject>Планёрка</Subject><Start>2026-07-27T07:00:00Z</Start><End>2026-07-27T08:00:00Z</End><IsAllDayEvent>false</IsAllDayEvent><CalendarItemType>Occurrence</CalendarItemType></CalendarItem></Envelope>"#;
        let events = parse_calendar_items(occurrence).expect("calendar response");
        assert_eq!(events[0].rrule, None);
        assert_eq!(events[0].recurrence_id, None);
    }

    #[test]
    fn calendar_item_parses_attendees_with_response_status() {
        let xml = r#"<Envelope><CalendarItem><ItemId Id="evt-1"/><Subject>Встреча</Subject><Start>2026-07-20T07:00:00Z</Start><End>2026-07-20T08:00:00Z</End><IsAllDayEvent>false</IsAllDayEvent><Organizer><Mailbox><EmailAddress>boss@example.test</EmailAddress></Mailbox></Organizer><RequiredAttendees><Attendee><Mailbox><Name>Иван</Name><EmailAddress>ivan@example.test</EmailAddress></Mailbox><ResponseType>Accept</ResponseType></Attendee></RequiredAttendees><OptionalAttendees><Attendee><Mailbox><EmailAddress>opt@example.test</EmailAddress></Mailbox><ResponseType>NoResponseReceived</ResponseType></Attendee></OptionalAttendees></CalendarItem></Envelope>"#;
        let events = parse_calendar_items(xml).expect("calendar response");
        let attendees = &events[0].attendees;
        assert_eq!(attendees.len(), 2);
        let ivan = attendees
            .iter()
            .find(|a| a.email == "ivan@example.test")
            .expect("required attendee");
        assert_eq!(ivan.partstat.as_deref(), Some("ACCEPTED"));
        assert_eq!(ivan.role.as_deref(), Some("REQ-PARTICIPANT"));
        assert_eq!(ivan.name.as_deref(), Some("Иван"));
        let opt = attendees
            .iter()
            .find(|a| a.email == "opt@example.test")
            .expect("optional attendee");
        assert_eq!(opt.partstat.as_deref(), Some("NEEDS-ACTION"));
        assert_eq!(opt.role.as_deref(), Some("OPT-PARTICIPANT"));
    }

    #[test]
    fn builds_every_supported_recurrence_pattern_from_rrule() {
        assert!(
            recurrence_of("FREQ=DAILY;INTERVAL=3")
                .contains("<t:DailyRecurrence><t:Interval>3</t:Interval></t:DailyRecurrence>")
        );
        assert!(recurrence_of("FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,TU").contains(
            "<t:WeeklyRecurrence><t:Interval>2</t:Interval><t:DaysOfWeek>Monday Tuesday</t:DaysOfWeek><t:FirstDayOfWeek>Monday</t:FirstDayOfWeek></t:WeeklyRecurrence>"
        ));
        assert!(recurrence_of("FREQ=MONTHLY;BYMONTHDAY=15").contains(
            "<t:AbsoluteMonthlyRecurrence><t:Interval>1</t:Interval><t:DayOfMonth>15</t:DayOfMonth></t:AbsoluteMonthlyRecurrence>"
        ));
        assert!(recurrence_of("FREQ=MONTHLY;INTERVAL=2;BYDAY=2MO").contains(
            "<t:RelativeMonthlyRecurrence><t:Interval>2</t:Interval><t:DaysOfWeek>Monday</t:DaysOfWeek><t:DayOfWeekIndex>Second</t:DayOfWeekIndex></t:RelativeMonthlyRecurrence>"
        ));
        assert!(recurrence_of("FREQ=YEARLY;BYMONTHDAY=9;BYMONTH=5").contains(
            "<t:AbsoluteYearlyRecurrence><t:DayOfMonth>9</t:DayOfMonth><t:Month>May</t:Month></t:AbsoluteYearlyRecurrence>"
        ));
        assert!(recurrence_of("FREQ=YEARLY;BYDAY=-1FR;BYMONTH=11").contains(
            "<t:RelativeYearlyRecurrence><t:DaysOfWeek>Friday</t:DaysOfWeek><t:DayOfWeekIndex>Last</t:DayOfWeekIndex><t:Month>November</t:Month></t:RelativeYearlyRecurrence>"
        ));
        // Диапазон всегда идёт после паттерна и всегда несёт StartDate.
        assert!(recurrence_of("FREQ=DAILY").contains(
            "</t:DailyRecurrence><t:NoEndRecurrence><t:StartDate>2026-07-20</t:StartDate></t:NoEndRecurrence>"
        ));
        assert!(recurrence_of("FREQ=DAILY;UNTIL=20261231T235959Z").contains(
            "<t:EndDateRecurrence><t:StartDate>2026-07-20</t:StartDate><t:EndDate>2026-12-31</t:EndDate></t:EndDateRecurrence>"
        ));
        assert!(recurrence_of("FREQ=DAILY;COUNT=5").contains(
            "<t:NumberedRecurrence><t:StartDate>2026-07-20</t:StartDate><t:NumberOfOccurrences>5</t:NumberOfOccurrences></t:NumberedRecurrence>"
        ));
    }

    #[test]
    fn recurrence_survives_round_trip_through_ews_xml() {
        let rules = [
            "FREQ=DAILY;INTERVAL=3",
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,TU",
            "FREQ=WEEKLY;BYDAY=MO",
            "FREQ=MONTHLY;BYMONTHDAY=15",
            "FREQ=MONTHLY;INTERVAL=2;BYDAY=2MO",
            "FREQ=YEARLY;BYMONTHDAY=9;BYMONTH=5",
            "FREQ=YEARLY;BYDAY=-1FR;BYMONTH=11",
            "FREQ=DAILY;UNTIL=20261231T235959Z",
            "FREQ=DAILY;COUNT=5",
        ];
        for rule in rules {
            assert_eq!(
                rrule_from_recurrence(&recurrence_of(rule)).as_deref(),
                Some(rule),
                "round-trip {rule}"
            );
        }
    }

    #[test]
    fn weekly_rule_without_byday_takes_weekday_from_dtstart() {
        // 2026-07-20 - понедельник; DaysOfWeek в EWS обязателен, поэтому день
        // недели выводим из даты начала.
        assert!(recurrence_of("FREQ=WEEKLY").contains("<t:DaysOfWeek>Monday</t:DaysOfWeek>"));
    }

    #[test]
    fn create_calendar_item_body_places_recurrence_after_attendees() {
        let mut input = sample_event_input();
        input.rrule = Some("FREQ=WEEKLY;BYDAY=MO".into());
        input.attendees = vec![Attendee {
            email: "req@example.test".into(),
            name: None,
            role: Some("REQ-PARTICIPANT".into()),
            partstat: None,
            rsvp: true,
        }];
        let body = create_calendar_item_body(&input).expect("build body");
        let attendees = body.find("</t:RequiredAttendees>").expect("attendees");
        let recurrence = body.find("<t:Recurrence>").expect("recurrence");
        assert!(attendees < recurrence);
        assert!(body.contains("<t:DaysOfWeek>Monday</t:DaysOfWeek>"));
        // Recurrence - последнее поле CalendarItem перед закрывающим тегом.
        assert!(body.contains("</t:Recurrence></t:CalendarItem>"));
    }

    #[test]
    fn update_calendar_item_body_sends_recurrence_as_its_own_field() {
        let mut input = sample_event_input();
        input.rrule = Some("FREQ=DAILY;COUNT=5".into());
        let body = update_calendar_item_body("item-1", "change-1", &input, false)
            .expect("build update body");
        assert!(body.contains(r#"<t:FieldURI FieldURI="calendar:Recurrence"/>"#));
        assert!(body.contains("<t:NumberOfOccurrences>5</t:NumberOfOccurrences>"));
        assert!(
            !body.contains("DeleteItemField"),
            "правило есть - стирать нечего"
        );
    }

    #[test]
    fn update_calendar_item_body_clears_recurrence_when_user_dropped_it() {
        let mut input = sample_event_input();
        input.rrule = None;
        // На сервере серия, в правке повторения нет - значит его сняли.
        let body = update_calendar_item_body("item-1", "change-1", &input, true)
            .expect("build update body");
        assert!(body.contains(
            r#"<t:DeleteItemField><t:FieldURI FieldURI="calendar:Recurrence"/></t:DeleteItemField>"#
        ));

        // То же событие, но на сервере оно и так одиночное: DeleteItemField по
        // calendar:Recurrence Exchange отклонил бы вместе со всем UpdateItem.
        let body = update_calendar_item_body("item-1", "change-1", &input, false)
            .expect("build update body");
        assert!(!body.contains("DeleteItemField"));
    }

    #[test]
    fn update_calendar_item_body_keeps_series_when_rule_is_not_expressible() {
        let mut input = sample_event_input();
        // BYSETPOS схема EWS не выражает: правило не уедет, но и стирать
        // существующую на сервере серию нельзя - пользователь её не снимал.
        input.rrule = Some("FREQ=MONTHLY;BYDAY=MO;BYSETPOS=-1".into());
        let body = update_calendar_item_body("item-1", "change-1", &input, true)
            .expect("build update body");
        assert!(!body.contains("DeleteItemField"));
    }

    #[test]
    fn unsupported_rrule_creates_single_event_without_recurrence() {
        // BYSETPOS, почасовые правила, COUNT вместе с UNTIL, "раз в N лет" и
        // BYDAY без позиции в месячном правиле схема EWS не выражает: событие
        // должно уехать одиночным, а не сорвать сохранение.
        let unsupported = [
            "FREQ=MONTHLY;BYDAY=MO;BYSETPOS=2",
            "FREQ=HOURLY;INTERVAL=6",
            "FREQ=DAILY;COUNT=5;UNTIL=20261231T235959Z",
            "FREQ=YEARLY;INTERVAL=2;BYMONTHDAY=9;BYMONTH=5",
            "FREQ=MONTHLY;BYDAY=MO",
            "FREQ=MONTHLY;BYMONTHDAY=-1",
            "FREQ=WEEKLY;BYDAY=1MO",
            "не-правило",
        ];
        for rule in unsupported {
            let mut input = sample_event_input();
            input.rrule = Some(rule.to_owned());
            let body = create_calendar_item_body(&input).expect("build body");
            assert!(!body.contains("<t:Recurrence>"), "правило {rule}");
            assert!(body.contains("<t:Subject>"), "правило {rule}");
        }
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
            addresses: vec![ContactAddress {
                kind: Some("home".into()),
                street: Some("ул. Ленина, 1".into()),
                city: Some("Москва".into()),
                region: None,
                postal_code: Some("101000".into()),
                country: Some("Россия".into()),
            }],
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

    /// Прежнее состояние контакта на сервере под сегодняшний sample_contact_input:
    /// те же три почтовых адреса и два телефона, что построит contact_item_updates.
    fn sample_contact_state() -> ContactRemoteState {
        ContactRemoteState {
            change_key: "change-1".into(),
            email_keys: vec![
                "EmailAddress1".into(),
                "EmailAddress2".into(),
                "EmailAddress3".into(),
            ],
            phone_keys: vec!["BusinessPhone".into(), "BusinessPhone2".into()],
            address_fields: vec![
                ("Home".into(), "Street".into()),
                ("Home".into(), "City".into()),
                ("Home".into(), "CountryOrRegion".into()),
                ("Home".into(), "PostalCode".into()),
            ],
        }
    }

    #[test]
    fn update_contact_item_body_uses_indexed_field_uri_for_emails_and_phones() {
        let body = update_contact_item_body(
            "contact-1",
            &sample_contact_state(),
            &sample_contact_input(),
        );
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
    fn update_contact_item_body_deletes_indexes_dropped_from_input() {
        // На сервере было три телефона (мобильный и два рабочих) и два адреса,
        // в новых данных остался один мобильный и один адрес.
        let previous = ContactRemoteState {
            change_key: "change-1".into(),
            email_keys: vec!["EmailAddress1".into(), "EmailAddress2".into()],
            phone_keys: vec![
                "MobilePhone".into(),
                "BusinessPhone".into(),
                "BusinessPhone2".into(),
            ],
            address_fields: Vec::new(),
        };
        let input = ContactInput {
            display_name: "Иванов".into(),
            first_name: None,
            last_name: None,
            organization: None,
            emails: vec!["a@example.test".into()],
            phones: vec![ContactPhone {
                number: "+79990000000".into(),
                kind: Some("mobile".into()),
                extension: None,
            }],
            addresses: Vec::new(),
        };
        let body = update_contact_item_body("contact-1", &previous, &input);
        assert!(body.contains(
            r#"<t:DeleteItemField><t:IndexedFieldURI FieldURI="contacts:PhoneNumber" FieldIndex="BusinessPhone"/></t:DeleteItemField>"#
        ));
        assert!(body.contains(
            r#"<t:DeleteItemField><t:IndexedFieldURI FieldURI="contacts:PhoneNumber" FieldIndex="BusinessPhone2"/></t:DeleteItemField>"#
        ));
        assert!(body.contains(
            r#"<t:DeleteItemField><t:IndexedFieldURI FieldURI="contacts:EmailAddress" FieldIndex="EmailAddress2"/></t:DeleteItemField>"#
        ));
        // Оставшиеся индексы по-прежнему записываются, а не стираются.
        assert!(body.contains(
            r#"<t:IndexedFieldURI FieldURI="contacts:PhoneNumber" FieldIndex="MobilePhone"/>"#
        ));
        assert!(!body.contains(r#"FieldIndex="MobilePhone"/></t:DeleteItemField>"#));
        assert!(!body.contains(r#"FieldIndex="EmailAddress1"/></t:DeleteItemField>"#));
        // Delete идут после Set - см. contact_item_updates.
        assert!(body.find("<t:SetItemField>").unwrap() < body.find("<t:DeleteItemField>").unwrap());
    }

    #[test]
    fn update_contact_item_body_without_changes_has_no_delete_fields() {
        let body = update_contact_item_body(
            "contact-1",
            &sample_contact_state(),
            &sample_contact_input(),
        );
        assert!(!body.contains("DeleteItemField"));
    }

    #[test]
    fn update_contact_item_body_deletes_server_only_phone_keys() {
        // Ключи, которые наш редактор не выставляет (AssistantPhone), тоже
        // должны стираться: в UI они видны как обычные телефоны.
        let previous = ContactRemoteState {
            change_key: "change-1".into(),
            email_keys: Vec::new(),
            phone_keys: vec!["AssistantPhone".into(), "AssistantPhone".into()],
            address_fields: Vec::new(),
        };
        let input = ContactInput {
            display_name: "Иванов".into(),
            first_name: None,
            last_name: None,
            organization: None,
            emails: Vec::new(),
            phones: Vec::new(),
            addresses: Vec::new(),
        };
        let body = update_contact_item_body("contact-1", &previous, &input);
        assert_eq!(body.matches("<t:DeleteItemField>").count(), 1);
        assert!(body.contains(r#"FieldIndex="AssistantPhone"/></t:DeleteItemField>"#));
    }

    #[test]
    fn parses_contact_remote_state_keys_and_change_key() {
        let xml = r#"<Envelope><Contact><ItemId Id="contact-1" ChangeKey="ck-7"/><EmailAddresses><Entry Key="EmailAddress1">a@example.test</Entry><Entry Key="EmailAddress2"></Entry></EmailAddresses><PhoneNumbers><Entry Key="MobilePhone">+79990000000</Entry><Entry Key="BusinessPhone">  </Entry></PhoneNumbers></Contact></Envelope>"#;
        let state = parse_contact_remote_state(xml).expect("contact state");
        assert_eq!(state.change_key, "ck-7");
        // Пустые Entry не считаем заполненными - стирать там нечего.
        assert_eq!(state.email_keys, ["EmailAddress1"]);
        assert_eq!(state.phone_keys, ["MobilePhone"]);
    }

    #[test]
    fn contact_item_xml_puts_physical_addresses_between_emails_and_phones() {
        let xml = contact_item_xml(&sample_contact_input());
        assert!(xml.contains(r#"<t:Entry Key="Home"><t:Street>ул. Ленина, 1</t:Street><t:City>Москва</t:City><t:CountryOrRegion>Россия</t:CountryOrRegion><t:PostalCode>101000</t:PostalCode></t:Entry>"#));
        // Незаполненный регион не превращается в пустой элемент.
        assert!(!xml.contains("<t:State>"));
        let emails_at = xml.find("EmailAddresses").unwrap();
        let addresses_at = xml.find("PhysicalAddresses").unwrap();
        let phones_at = xml.find("PhoneNumbers").unwrap();
        assert!(emails_at < addresses_at);
        assert!(addresses_at < phones_at);
    }

    #[test]
    fn contact_addresses_xml_keeps_only_first_address_of_each_kind() {
        let addresses = vec![
            ContactAddress {
                kind: Some("work".into()),
                city: Some("Москва".into()),
                ..ContactAddress::default()
            },
            ContactAddress {
                kind: Some("work".into()),
                city: Some("Казань".into()),
                ..ContactAddress::default()
            },
            ContactAddress::default(),
        ];
        let xml = contact_addresses_xml(&addresses);
        assert_eq!(xml.matches("<t:Entry").count(), 1);
        assert!(xml.contains(r#"<t:Entry Key="Business"><t:City>Москва</t:City></t:Entry>"#));
    }

    #[test]
    fn update_contact_item_body_sets_each_address_part_separately() {
        let body = update_contact_item_body(
            "contact-1",
            &sample_contact_state(),
            &sample_contact_input(),
        );
        assert!(body.contains(
            r#"<t:IndexedFieldURI FieldURI="contacts:PhysicalAddress:Street" FieldIndex="Home"/>"#
        ));
        assert!(
            body.contains(r#"<t:Entry Key="Home"><t:PostalCode>101000</t:PostalCode></t:Entry>"#)
        );
        // Состояние на сервере совпадает с новыми данными - удалять нечего.
        assert!(!body.contains("DeleteItemField"));
    }

    #[test]
    fn update_contact_item_body_deletes_address_parts_dropped_from_input() {
        // На сервере был домашний адрес целиком, в новых данных остался только
        // город, а рабочий адрес исчез вовсе.
        let previous = ContactRemoteState {
            change_key: "change-1".into(),
            email_keys: Vec::new(),
            phone_keys: Vec::new(),
            address_fields: vec![
                ("Home".into(), "Street".into()),
                ("Home".into(), "City".into()),
                ("Home".into(), "PostalCode".into()),
                ("Business".into(), "Street".into()),
            ],
        };
        let input = ContactInput {
            display_name: "Иванов".into(),
            first_name: None,
            last_name: None,
            organization: None,
            emails: Vec::new(),
            phones: Vec::new(),
            addresses: vec![ContactAddress {
                kind: Some("home".into()),
                city: Some("Москва".into()),
                ..ContactAddress::default()
            }],
        };
        let body = update_contact_item_body("contact-1", &previous, &input);
        assert!(body.contains(
            r#"<t:DeleteItemField><t:IndexedFieldURI FieldURI="contacts:PhysicalAddress:Street" FieldIndex="Home"/></t:DeleteItemField>"#
        ));
        assert!(body.contains(
            r#"<t:DeleteItemField><t:IndexedFieldURI FieldURI="contacts:PhysicalAddress:PostalCode" FieldIndex="Home"/></t:DeleteItemField>"#
        ));
        assert!(body.contains(
            r#"<t:DeleteItemField><t:IndexedFieldURI FieldURI="contacts:PhysicalAddress:Street" FieldIndex="Business"/></t:DeleteItemField>"#
        ));
        // Оставшийся город записывается, а не стирается.
        assert!(body.contains(
            r#"<t:IndexedFieldURI FieldURI="contacts:PhysicalAddress:City" FieldIndex="Home"/>"#
        ));
        assert!(!body.contains(
            r#"FieldURI="contacts:PhysicalAddress:City" FieldIndex="Home"/></t:DeleteItemField>"#
        ));
    }

    #[test]
    fn parses_contact_remote_state_address_parts_from_nested_entries() {
        // Entry адреса не содержит текста - только вложенные элементы, поэтому
        // читается отдельно от почт и телефонов (см. contact_address_fields).
        let xml = r#"<Envelope><Contact><ItemId Id="contact-1" ChangeKey="ck-7"/><PhysicalAddresses><Entry Key="Home"><Street>ул. Ленина, 1</Street><City>Москва</City><State>  </State></Entry><Entry Key="Business"/></PhysicalAddresses></Contact></Envelope>"#;
        let state = parse_contact_remote_state(xml).expect("contact state");
        assert_eq!(
            state.address_fields,
            [
                ("Home".to_owned(), "Street".to_owned()),
                ("Home".to_owned(), "City".to_owned()),
            ]
        );
    }

    #[test]
    fn contact_remote_state_requires_contact_and_change_key() {
        assert!(parse_contact_remote_state("<Envelope/>").is_err());
        assert!(
            parse_contact_remote_state(
                r#"<Envelope><Contact><ItemId Id="c"/></Contact></Envelope>"#
            )
            .is_err()
        );
        assert!(parse_contact_remote_state("<broken").is_err());
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
