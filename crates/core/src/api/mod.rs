//! Локальный REST/MCP API для агентов и скриптов.
//! Listener всегда привязан к loopback; каждый вызов проходит capability-проверку и аудит.

use crate::backend::{OutgoingAttachment, OutgoingMessage};
use crate::{Core, Error, Result};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine as _;
use rand::Rng as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Digest as _;
use std::sync::Arc;

type AuditRow = (
    i64,
    Option<i64>,
    Option<String>,
    String,
    Option<String>,
    String,
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Capability {
    Read,
    Search,
    Send,
    Labels,
    Calendar,
    Network,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiClient {
    pub id: i64,
    pub name: String,
    pub caps: Vec<Capability>,
    pub created_at: String,
    pub last_used: Option<String>,
}

impl ApiClient {
    pub fn require(&self, cap: Capability) -> Result<()> {
        if self.caps.contains(&cap) {
            Ok(())
        } else {
            Err(Error::Forbidden(format!("{cap:?}")))
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CreatedApiClient {
    pub client: ApiClient,
    /// Показывается один раз. В SQLite не хранится.
    pub token: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiAuditEntry {
    pub id: i64,
    pub client_id: Option<i64>,
    pub client_name: Option<String>,
    pub action: String,
    pub detail: Option<String>,
    pub at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpTool {
    pub name: &'static str,
    pub description: &'static str,
    pub required_cap: Capability,
}

pub fn mcp_tools() -> Vec<McpTool> {
    use Capability::*;
    vec![
        McpTool {
            name: "search",
            description: "Поиск по почте",
            required_cap: Search,
        },
        McpTool {
            name: "get_message",
            description: "Прочитать письмо",
            required_cap: Read,
        },
        McpTool {
            name: "list_folders",
            description: "Список папок аккаунта",
            required_cap: Read,
        },
        McpTool {
            name: "list_messages",
            description: "Список писем папки",
            required_cap: Read,
        },
        McpTool {
            name: "send",
            description: "Отправить письмо",
            required_cap: Send,
        },
        McpTool {
            name: "label",
            description: "Поставить или снять метку",
            required_cap: Labels,
        },
        McpTool {
            name: "list_events",
            description: "События календаря",
            required_cap: Calendar,
        },
        McpTool {
            name: "list_contacts",
            description: "Контакты",
            required_cap: Read,
        },
    ]
}

fn token_hash(token: &str) -> String {
    sha2::Sha256::digest(token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn parse_caps(serialized: &str) -> Result<Vec<Capability>> {
    Ok(serde_json::from_str(serialized)?)
}

pub async fn list_clients(core: &Core) -> Result<Vec<ApiClient>> {
    let rows: Vec<(i64, String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, name, caps, created_at, last_used FROM api_clients ORDER BY name, id",
    )
    .fetch_all(&core.db.pool)
    .await?;
    rows.into_iter()
        .map(|(id, name, caps, created_at, last_used)| {
            Ok(ApiClient {
                id,
                name,
                caps: parse_caps(&caps)?,
                created_at,
                last_used,
            })
        })
        .collect()
}

pub async fn create_client(
    core: &Core,
    name: &str,
    caps: Vec<Capability>,
) -> Result<CreatedApiClient> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::AccountConfig("имя API-клиента не указано".into()));
    }
    if caps.is_empty() {
        return Err(Error::AccountConfig(
            "API-клиенту не выдано ни одного права".into(),
        ));
    }
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let token = format!(
        "tm_{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    );
    bytes.fill(0);
    let token_ref = format!("api-token:{}", uuid::Uuid::new_v4());
    let entry = keyring::Entry::new("truemail", &token_ref)
        .map_err(|error| Error::Keyring(error.to_string()))?;
    entry
        .set_password(&token)
        .map_err(|error| Error::Keyring(error.to_string()))?;
    let inserted = sqlx::query(
        "INSERT INTO api_clients(name, token_ref, token_hash, caps) VALUES(?, ?, ?, ?)",
    )
    .bind(name)
    .bind(&token_ref)
    .bind(token_hash(&token))
    .bind(serde_json::to_string(&caps)?)
    .execute(&core.db.write_pool)
    .await;
    let id = match inserted {
        Ok(result) => result.last_insert_rowid(),
        Err(error) => {
            let _ = entry.delete_credential();
            return Err(error.into());
        }
    };
    let client = list_clients(core)
        .await?
        .into_iter()
        .find(|client| client.id == id)
        .ok_or_else(|| Error::Other("созданный API-клиент не найден".into()))?;
    Ok(CreatedApiClient { client, token })
}

pub async fn revoke_client(core: &Core, client_id: i64) -> Result<bool> {
    let row: Option<(String,)> = sqlx::query_as("SELECT token_ref FROM api_clients WHERE id=?")
        .bind(client_id)
        .fetch_optional(&core.db.pool)
        .await?;
    let Some((token_ref,)) = row else {
        return Ok(false);
    };
    sqlx::query("DELETE FROM api_clients WHERE id=?")
        .bind(client_id)
        .execute(&core.db.write_pool)
        .await?;
    if let Ok(entry) = keyring::Entry::new("truemail", &token_ref) {
        let _ = entry.delete_credential();
    }
    Ok(true)
}

pub async fn list_audit(core: &Core, limit: i64) -> Result<Vec<ApiAuditEntry>> {
    let rows: Vec<AuditRow> = sqlx::query_as(
        "SELECT a.id, a.client_id, c.name, a.action, a.detail, a.at
             FROM api_audit a LEFT JOIN api_clients c ON c.id=a.client_id
             ORDER BY a.id DESC LIMIT ?",
    )
    .bind(limit.clamp(1, 500))
    .fetch_all(&core.db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(id, client_id, client_name, action, detail, at)| ApiAuditEntry {
                id,
                client_id,
                client_name,
                action,
                detail,
                at,
            },
        )
        .collect())
}

pub async fn clear_audit(core: &Core) -> Result<u64> {
    Ok(sqlx::query("DELETE FROM api_audit")
        .execute(&core.db.write_pool)
        .await?
        .rows_affected())
}

async fn authenticate(core: &Core, token: &str) -> Result<ApiClient> {
    let row: Option<(i64, String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, name, caps, created_at, last_used FROM api_clients WHERE token_hash=?",
    )
    .bind(token_hash(token))
    .fetch_optional(&core.db.pool)
    .await?;
    let Some((id, name, caps, created_at, last_used)) = row else {
        return Err(Error::Forbidden("действительный API-токен".into()));
    };
    sqlx::query("UPDATE api_clients SET last_used=datetime('now') WHERE id=?")
        .bind(id)
        .execute(&core.db.write_pool)
        .await?;
    Ok(ApiClient {
        id,
        name,
        caps: parse_caps(&caps)?,
        created_at,
        last_used,
    })
}

async fn audit(core: &Core, client_id: Option<i64>, action: &str, detail: &Value) {
    let _ = sqlx::query("INSERT INTO api_audit(client_id, action, detail) VALUES(?, ?, ?)")
        .bind(client_id)
        .bind(action)
        .bind(detail.to_string())
        .execute(&core.db.write_pool)
        .await;
}

#[derive(Debug, Deserialize)]
struct ApiSendInput {
    account_id: i64,
    #[serde(default)]
    to: Vec<String>,
    #[serde(default)]
    cc: Vec<String>,
    #[serde(default)]
    bcc: Vec<String>,
    #[serde(default)]
    subject: String,
    #[serde(default)]
    body_text: String,
    body_html: Option<String>,
    #[serde(default)]
    attachments: Vec<OutgoingAttachment>,
}

async fn execute_tool(core: &Core, name: &str, arguments: Value) -> Result<Value> {
    match name {
        "search" => {
            let query = arguments
                .get("query")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::AccountConfig("query не указан".into()))?;
            let limit = arguments.get("limit").and_then(Value::as_i64).unwrap_or(50);
            let mut ids = Vec::new();
            for variant in crate::search::layout_variants(query) {
                if let Some(fts_query) = crate::search::prefix_query(&variant) {
                    for id in core.search.search(&fts_query, limit.clamp(1, 500)).await? {
                        if !ids.contains(&id) {
                            ids.push(id);
                        }
                    }
                }
            }
            Ok(serde_json::to_value(
                core.db.list_messages_by_ids(&ids).await?,
            )?)
        }
        "get_message" => {
            let id = required_i64(&arguments, "message_id")?;
            core.accounts.ensure_message_raw(id).await?;
            Ok(serde_json::to_value(core.db.get_message(id).await?)?)
        }
        "list_folders" => {
            let account_id = required_i64(&arguments, "account_id")?;
            Ok(serde_json::to_value(
                core.db.list_folders(account_id).await?,
            )?)
        }
        "list_messages" => {
            let folder_id = required_i64(&arguments, "folder_id")?;
            let limit = arguments
                .get("limit")
                .and_then(Value::as_i64)
                .unwrap_or(100);
            Ok(serde_json::to_value(
                core.db
                    .list_messages(folder_id, limit.clamp(1, 1000))
                    .await?,
            )?)
        }
        "send" => {
            let input: ApiSendInput = serde_json::from_value(arguments)?;
            core.accounts
                .send_outgoing(
                    input.account_id,
                    OutgoingMessage {
                        from: String::new(),
                        to: input.to,
                        cc: input.cc,
                        bcc: input.bcc,
                        subject: input.subject,
                        body_text: input.body_text,
                        body_html: input.body_html,
                        attachments: input.attachments,
                    },
                )
                .await?;
            Ok(json!({"sent": true}))
        }
        "label" => {
            let message_id = required_i64(&arguments, "message_id")?;
            let label_id = required_i64(&arguments, "label_id")?;
            let on = arguments.get("on").and_then(Value::as_bool).unwrap_or(true);
            core.db
                .toggle_message_label(message_id, label_id, on)
                .await?;
            Ok(json!({"updated": true}))
        }
        "list_events" => {
            let (calendars, events) = core.db.list_calendars_and_events().await?;
            Ok(json!({"calendars": calendars, "events": events}))
        }
        "list_contacts" => {
            let query = arguments.get("query").and_then(Value::as_str);
            Ok(serde_json::to_value(core.db.list_contacts(query).await?)?)
        }
        _ => Err(Error::AccountConfig(format!(
            "неизвестный инструмент: {name}"
        ))),
    }
}

fn required_i64(arguments: &Value, key: &str) -> Result<i64> {
    arguments
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| Error::AccountConfig(format!("{key} не указан")))
}

fn tool(name: &str) -> Option<McpTool> {
    mcp_tools().into_iter().find(|tool| tool.name == name)
}

async fn call_authorized(
    core: &Core,
    client: &ApiClient,
    name: &str,
    arguments: Value,
) -> Result<Value> {
    let definition = tool(name)
        .ok_or_else(|| Error::AccountConfig(format!("неизвестный инструмент: {name}")))?;
    if let Err(error) = client.require(definition.required_cap) {
        audit(
            core,
            Some(client.id),
            &format!("tool:{name}:denied"),
            &json!({"error": error.to_string()}),
        )
        .await;
        return Err(error);
    }
    let result = execute_tool(core, name, arguments).await;
    let (suffix, detail) = match &result {
        Ok(_) => ("ok", json!({"status": "ok"})),
        Err(error) => ("error", json!({"error": error.to_string()})),
    };
    audit(
        core,
        Some(client.id),
        &format!("tool:{name}:{suffix}"),
        &detail,
    )
    .await;
    result
}

#[derive(Clone)]
struct ApiState {
    core: Arc<Core>,
}

pub struct RunningApiServer {
    pub port: u16,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

impl RunningApiServer {
    pub fn stop(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

pub async fn start_server(core: Arc<Core>, port: u16) -> Result<RunningApiServer> {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    let actual_port = listener.local_addr()?.port();
    let state = ApiState { core };
    let router = Router::new()
        .route("/health", get(health))
        .route("/v1/tools", get(rest_tools))
        .route("/v1/tools/{name}", post(rest_call))
        .route("/mcp", post(mcp))
        .with_state(state);
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
        {
            tracing::error!(%error, "локальный API listener остановлен с ошибкой");
        }
    });
    Ok(RunningApiServer {
        port: actual_port,
        shutdown: Some(shutdown_tx),
    })
}

async fn health() -> Json<Value> {
    Json(json!({"status": "ok", "service": "truemail"}))
}

async fn rest_tools(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> std::result::Result<Json<Value>, HttpError> {
    let client = client_from_headers(&state.core, &headers).await?;
    let tools = mcp_tools()
        .into_iter()
        .filter(|tool| client.caps.contains(&tool.required_cap))
        .collect::<Vec<_>>();
    audit(
        &state.core,
        Some(client.id),
        "tools:list:ok",
        &json!({"count": tools.len()}),
    )
    .await;
    Ok(Json(json!({"tools": tools})))
}

async fn rest_call(
    State(state): State<ApiState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Json(arguments): Json<Value>,
) -> std::result::Result<Json<Value>, HttpError> {
    let client = client_from_headers(&state.core, &headers).await?;
    Ok(Json(
        call_authorized(&state.core, &client, &name, arguments).await?,
    ))
}

async fn mcp(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> std::result::Result<Json<Value>, HttpError> {
    let client = client_from_headers(&state.core, &headers).await?;
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let result = match method {
        "initialize" => json!({
            "protocolVersion": "2025-03-26",
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {"name": "truemail", "version": env!("CARGO_PKG_VERSION")}
        }),
        "notifications/initialized" => Value::Null,
        "tools/list" => {
            let tools = mcp_tools()
                .into_iter()
                .filter(|tool| client.caps.contains(&tool.required_cap))
                .map(|tool| json!({"name": tool.name, "description": tool.description, "inputSchema": tool_schema(tool.name)}))
                .collect::<Vec<_>>();
            json!({"tools": tools})
        }
        "tools/call" => {
            let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let output = call_authorized(&state.core, &client, name, arguments).await?;
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&output)?}], "structuredContent": output, "isError": false})
        }
        _ => {
            return Err(HttpError::rpc(
                StatusCode::BAD_REQUEST,
                "неизвестный MCP method",
            ));
        }
    };
    Ok(Json(json!({"jsonrpc": "2.0", "id": id, "result": result})))
}

fn tool_schema(name: &str) -> Value {
    match name {
        "search" => {
            json!({"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"integer"}},"required":["query"]})
        }
        "get_message" => {
            json!({"type":"object","properties":{"message_id":{"type":"integer"}},"required":["message_id"]})
        }
        "list_folders" => {
            json!({"type":"object","properties":{"account_id":{"type":"integer"}},"required":["account_id"]})
        }
        "list_messages" => {
            json!({"type":"object","properties":{"folder_id":{"type":"integer"},"limit":{"type":"integer"}},"required":["folder_id"]})
        }
        "send" => {
            json!({"type":"object","properties":{"account_id":{"type":"integer"},"to":{"type":"array","items":{"type":"string"}},"cc":{"type":"array","items":{"type":"string"}},"bcc":{"type":"array","items":{"type":"string"}},"subject":{"type":"string"},"body_text":{"type":"string"},"body_html":{"type":["string","null"]}},"required":["account_id","to"]})
        }
        "label" => {
            json!({"type":"object","properties":{"message_id":{"type":"integer"},"label_id":{"type":"integer"},"on":{"type":"boolean"}},"required":["message_id","label_id"]})
        }
        "list_contacts" => json!({"type":"object","properties":{"query":{"type":"string"}}}),
        _ => json!({"type":"object","properties":{}}),
    }
}

async fn client_from_headers(
    core: &Core,
    headers: &HeaderMap,
) -> std::result::Result<ApiClient, HttpError> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| HttpError::rpc(StatusCode::UNAUTHORIZED, "нужен Bearer token"))?;
    match authenticate(core, token).await {
        Ok(client) => Ok(client),
        Err(Error::Forbidden(_)) => Err(HttpError::rpc(
            StatusCode::UNAUTHORIZED,
            "API-токен недействителен или отозван",
        )),
        Err(error) => Err(HttpError::from(error)),
    }
}

struct HttpError {
    status: StatusCode,
    message: String,
}

impl HttpError {
    fn rpc(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

impl From<Error> for HttpError {
    fn from(error: Error) -> Self {
        let status = match error {
            Error::Forbidden(_) => StatusCode::FORBIDDEN,
            Error::AccountConfig(_) | Error::Json(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self {
            status,
            message: error.to_string(),
        }
    }
}

impl From<serde_json::Error> for HttpError {
    fn from(error: serde_json::Error) -> Self {
        HttpError::from(Error::Json(error))
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({"error": self.message}))).into_response()
    }
}
