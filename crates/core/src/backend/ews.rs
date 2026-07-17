//! Exchange Web Services transport for self-hosted Exchange Server.

use super::{
    DiscoveredFolder, DiscoveredMessage, FolderSyncCursor, ImapDiscovery, MailBackend,
    OutgoingMessage,
};
use crate::model::{FolderRole, infer_folder_role};
use crate::{Error, Result};
use async_trait::async_trait;
use base64::Engine as _;
use roxmltree::{Document, Node};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

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
    Some(
        node_text(response, "MessageText")
            .or_else(|| node_text(response, "ResponseCode"))
            .unwrap_or("Exchange вернул ошибку")
            .to_owned(),
    )
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

    async fn folders(&self, password: &str) -> Result<Vec<DiscoveredFolder>> {
        let body = r#"<m:FindFolder Traversal="Deep"><m:FolderShape><t:BaseShape>Default</t:BaseShape></m:FolderShape><m:ParentFolderIds><t:DistinguishedFolderId Id="msgfolderroot"/></m:ParentFolderIds></m:FindFolder>"#;
        let response = self.soap(password, "FindFolder", body).await?;
        parse_folders(&response)
    }

    async fn messages_in_folder(
        &self,
        password: &str,
        folder: &DiscoveredFolder,
        limit: usize,
    ) -> Result<(Vec<DiscoveredMessage>, Vec<u32>)> {
        let body = format!(
            r#"<m:FindItem Traversal="Shallow"><m:ItemShape><t:BaseShape>IdOnly</t:BaseShape></m:ItemShape><m:IndexedPageItemView MaxEntriesReturned="{}" Offset="0" BasePoint="Beginning"/><m:SortOrder><t:FieldOrder Order="Descending"><t:FieldURI FieldURI="item:DateTimeReceived"/></t:FieldOrder></m:SortOrder><m:ParentFolderIds><t:FolderId Id="{}"/></m:ParentFolderIds></m:FindItem>"#,
            limit,
            escape(&folder.remote_path)
        );
        let response = self.soap(password, "FindItem", &body).await?;
        let ids = parse_item_ids(&response)?;
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
            messages.extend(parse_messages(&response, &folder.remote_path)?);
        }
        let uids = messages.iter().map(|message| message.uid).collect();
        Ok((messages, uids))
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
        });
    }
    Ok(messages)
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
        _cursors: &HashMap<String, FolderSyncCursor>,
    ) -> Result<ImapDiscovery> {
        let folders = self.folders(credential).await?;
        let mut messages = Vec::new();
        let mut server_uids = Vec::new();
        for folder in &folders {
            match self.messages_in_folder(credential, folder, 500).await {
                Ok((mut found, uids)) => {
                    messages.append(&mut found);
                    server_uids.push((folder.remote_path.clone(), uids));
                }
                Err(error) => {
                    tracing::warn!(folder = %folder.display_name, %error, "EWS: папка пропущена")
                }
            }
        }
        Ok(ImapDiscovery {
            folders,
            messages,
            server_uids,
            reset_folders: Vec::new(),
            remote_snapshot: None,
            changed_remote_ids: Vec::new(),
        })
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
        _cursors: &HashMap<String, FolderSyncCursor>,
    ) -> Result<ImapDiscovery> {
        let folders = self.folders(credential).await?;
        let inbox = folders
            .iter()
            .find(|folder| folder.role == Some(FolderRole::Inbox))
            .ok_or_else(|| backend_error("folders", "папка Входящие не найдена"))?;
        let (messages, uids) = self.messages_in_folder(credential, inbox, 500).await?;
        let inbox_path = inbox.remote_path.clone();
        Ok(ImapDiscovery {
            folders,
            messages,
            server_uids: vec![(inbox_path, uids)],
            reset_folders: Vec::new(),
            remote_snapshot: None,
            changed_remote_ids: Vec::new(),
        })
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
        let (action, body) = match operation {
            "flag" => (
                "UpdateItem",
                format!(
                    r#"<m:UpdateItem ConflictResolution="AutoResolve" MessageDisposition="SaveOnly"><m:ItemChanges><t:ItemChange><t:ItemId Id="{}"/><t:Updates><t:SetItemField><t:FieldURI FieldURI="message:IsRead"/><t:Message><t:IsRead>{}</t:IsRead></t:Message></t:SetItemField></t:Updates></t:ItemChange></m:ItemChanges></m:UpdateItem>"#,
                    escape(item_id),
                    payload["seen"].as_bool().unwrap_or(false)
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

    async fn send(&self, message: OutgoingMessage, credential: &str) -> Result<()> {
        let mime = super::smtp::build_message(message)?.formatted();
        let encoded = base64::engine::general_purpose::STANDARD.encode(mime);
        let item = format!(
            "<t:Message><t:MimeContent CharacterSet=\"UTF-8\">{encoded}</t:MimeContent></t:Message>"
        );
        let body = format!(
            r#"<m:CreateItem MessageDisposition="SendAndSaveCopy"><m:SavedItemFolderId><t:DistinguishedFolderId Id="sentitems"/></m:SavedItemFolderId><m:Items>{item}</m:Items></m:CreateItem>"#
        );
        self.soap(credential, "CreateItem", &body).await.map(|_| ())
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
    fn stable_ews_uid_is_nonzero_and_repeatable() {
        assert_ne!(stable_uid("AAMk-long-item-id"), 0);
        assert_eq!(
            stable_uid("AAMk-long-item-id"),
            stable_uid("AAMk-long-item-id")
        );
    }
}
