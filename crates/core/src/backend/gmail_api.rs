use super::{DiscoveredFolder, DiscoveredMessage, FolderSyncCursor, ImapDiscovery};
use crate::model::FolderRole;
use crate::{Error, Result};
use base64::Engine as _;
use base64::alphabet::URL_SAFE;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use futures::{StreamExt, TryStreamExt, stream};
use reqwest::{Client, Method, Response, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const GMAIL_BASE: &str = "https://gmail.googleapis.com/gmail/v1/users/me";
// Cold start must establish historyId quickly enough for the realtime loop.
// Backfill therefore advances by one small, persisted page per sync instead
// of attempting to download the whole mailbox before saving the cursor.
// Backfill тянет только metadata писем (raw качается лениво при открытии),
// поэтому страницу можно укрупнить: клиентский токен-бакет держит суммарный
// поток под лимитом Gmail, а крупная страница убирает обвязку labels/history на
// каждое письмо при холодной загрузке архива.
const BACKFILL_PAGE_SIZE: u32 = 25;
const MESSAGE_GET_CONCURRENCY: usize = 2;
const MESSAGE_GETS_PER_QUOTA_WINDOW: usize = 200;
const QUOTA_WINDOW: Duration = Duration::from_secs(60);
const MAX_READ_RETRIES: u32 = 5;

static GMAIL_REQUEST_GATE: OnceLock<tokio::sync::Mutex<GmailRequestGate>> = OnceLock::new();

#[derive(Debug, Default)]
struct GmailRequestGate {
    not_before: Option<Instant>,
    retry_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl GmailRequestGate {
    fn active_block(&mut self, now: Instant) -> Option<(Duration, chrono::DateTime<chrono::Utc>)> {
        let not_before = self.not_before?;
        if now >= not_before {
            self.not_before = None;
            self.retry_at = None;
            return None;
        }
        Some((
            not_before.duration_since(now),
            self.retry_at.unwrap_or_else(chrono::Utc::now),
        ))
    }

    fn block_for(
        &mut self,
        now: Instant,
        now_utc: chrono::DateTime<chrono::Utc>,
        delay: Duration,
    ) -> chrono::DateTime<chrono::Utc> {
        let not_before = now.checked_add(delay).unwrap_or(now);
        let delay_ms = delay.as_millis().min(i64::MAX as u128) as i64;
        let retry_at = now_utc + chrono::Duration::milliseconds(delay_ms);
        if self.not_before.is_none_or(|current| not_before > current) {
            self.not_before = Some(not_before);
            self.retry_at = Some(retry_at);
        }
        self.retry_at.unwrap_or(retry_at)
    }
}

fn gmail_request_gate() -> &'static tokio::sync::Mutex<GmailRequestGate> {
    GMAIL_REQUEST_GATE.get_or_init(|| tokio::sync::Mutex::new(GmailRequestGate::default()))
}

// Клиентский ограничитель частоты под лимит Gmail: 250 quota units на
// пользователя в секунду. Держим равномерный поток с запасом (пополнение ниже
// потолка), чтобы залпы get/list/history не выбивали длинный серверный
// Retry-After. Токен-бакет глобальный: у типичного пользователя один-два Gmail,
// а суммарный поток так гарантированно не превысит per-user лимит.
const QUOTA_UNITS_PER_SEC: f64 = 200.0;
const QUOTA_BUCKET_CAPACITY: f64 = 250.0;

static GMAIL_QUOTA_BUCKET: OnceLock<tokio::sync::Mutex<QuotaBucket>> = OnceLock::new();

struct QuotaBucket {
    tokens: f64,
    last_refill: Instant,
}

impl QuotaBucket {
    fn take(&mut self, cost: f64, now: Instant) -> Option<Duration> {
        let elapsed = now.saturating_duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * QUOTA_UNITS_PER_SEC).min(QUOTA_BUCKET_CAPACITY);
        self.last_refill = now;
        if self.tokens >= cost {
            self.tokens -= cost;
            None
        } else {
            Some(Duration::from_secs_f64((cost - self.tokens) / QUOTA_UNITS_PER_SEC))
        }
    }
}

fn gmail_quota_bucket() -> &'static tokio::sync::Mutex<QuotaBucket> {
    GMAIL_QUOTA_BUCKET.get_or_init(|| {
        tokio::sync::Mutex::new(QuotaBucket {
            tokens: QUOTA_BUCKET_CAPACITY,
            last_refill: Instant::now(),
        })
    })
}

// Стоимость метода Gmail в quota units по официальному прайслисту.
fn request_quota_cost(url: &Url) -> f64 {
    let path = url.path();
    if path.contains("/history") {
        2.0
    } else if path.ends_with("/profile") || path.contains("/labels") {
        1.0
    } else if path.ends_with("/send") {
        100.0
    } else {
        // messages.list, messages.get, modify/trash и прочее - 5 units.
        5.0
    }
}

// Дождаться, пока в бакете накопится cost units, затем списать их. Ожидание
// идёт вне gate-мьютекса, поэтому не блокирует учёт серверного Retry-After.
async fn gmail_acquire_quota(cost: f64) {
    loop {
        let wait = {
            let mut bucket = gmail_quota_bucket().lock().await;
            bucket.take(cost, Instant::now())
        };
        match wait {
            None => return,
            Some(delay) => tokio::time::sleep(delay).await,
        }
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct GmailSyncCursor {
    v: u8,
    history_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    backfill_page_token: Option<String>,
    #[serde(default)]
    backfill_complete: bool,
}

impl GmailSyncCursor {
    fn bootstrap(history_id: String) -> Self {
        Self {
            v: 1,
            history_id,
            backfill_page_token: None,
            backfill_complete: false,
        }
    }
}

fn decode_sync_cursor(value: Option<&str>) -> Option<GmailSyncCursor> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    match serde_json::from_str::<GmailSyncCursor>(value) {
        Ok(cursor) if cursor.v == 1 && !cursor.history_id.is_empty() => Some(cursor),
        // A malformed structured cursor must be rebuilt. Treating JSON as a
        // legacy historyId would force a pointless history request and 404.
        _ if value.starts_with('{') => None,
        // Before v1 the folder token contained the raw Gmail historyId. Such
        // accounts have already completed their old full bootstrap.
        _ => Some(GmailSyncCursor {
            v: 1,
            history_id: value.to_owned(),
            backfill_page_token: None,
            backfill_complete: true,
        }),
    }
}

fn encode_sync_cursor(cursor: &GmailSyncCursor) -> Result<String> {
    serde_json::to_string(cursor).map_err(Into::into)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMessage {
    id: String,
    #[serde(default)]
    label_ids: Vec<String>,
    #[serde(default)]
    raw: Option<String>,
    #[serde(default)]
    size_estimate: u32,
    #[serde(default)]
    snippet: String,
    #[serde(default)]
    payload: GmailPayload,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct GmailPayload {
    #[serde(default)]
    headers: Vec<GmailHeader>,
}

#[derive(Debug, Clone, Deserialize)]
struct GmailHeader {
    name: String,
    value: String,
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

fn quota_limited_response(status: StatusCode, body: &str) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS
        || (status == StatusCode::FORBIDDEN
            && (body.contains("rateLimitExceeded")
                || body.contains("userRateLimitExceeded")
                || body.contains("RESOURCE_EXHAUSTED")))
}

fn retryable_response(status: StatusCode, body: &str) -> bool {
    quota_limited_response(status, body)
        || status == StatusCode::REQUEST_TIMEOUT
        || status.is_server_error()
}

fn retry_timestamp_delay(value: &str) -> Option<Duration> {
    let retry_at = chrono::DateTime::parse_from_rfc3339(value)
        .or_else(|_| chrono::DateTime::parse_from_rfc2822(value))
        .ok()?;
    let millis = retry_at
        .signed_duration_since(chrono::Utc::now())
        .num_milliseconds();
    Some(Duration::from_millis(millis.max(0) as u64))
}

fn server_retry_delay(retry_after: Option<&str>, body: &str) -> Option<Duration> {
    if let Some(value) = retry_after {
        if let Ok(seconds) = value.trim().parse::<u64>() {
            return Some(Duration::from_secs(seconds));
        }
        if let Some(delay) = retry_timestamp_delay(value.trim()) {
            return Some(delay);
        }
    }

    let marker = "Retry after ";
    let tail = body.split_once(marker)?.1;
    let timestamp = tail
        .split(|character: char| character.is_whitespace() || character == '"' || character == '\\')
        .next()?;
    retry_timestamp_delay(timestamp)
}

fn bounded_backoff(attempt: u32) -> Duration {
    let exponential = 1_u64 << attempt.min(6);
    let jitter_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_millis() as u64;
    Duration::from_millis(exponential * 1_000 + jitter_ms)
}

fn retry_delay(attempt: u32, retry_after: Option<&str>, body: &str) -> Duration {
    let backoff = bounded_backoff(attempt);
    server_retry_delay(retry_after, body)
        .map(|server| server.max(backoff))
        .unwrap_or(backoff)
}

fn throttled_error(
    remaining: Duration,
    retry_at: chrono::DateTime<chrono::Utc>,
    cause: Option<StatusCode>,
) -> Error {
    let seconds = remaining
        .as_secs()
        .saturating_add(u64::from(remaining.subsec_nanos() > 0));
    let cause = cause
        .map(|status| format!(" после HTTP {status}"))
        .unwrap_or_default();
    Error::RateLimited {
        backend: "gmail-api".into(),
        retry_at,
        message: format!(
            "Gmail API локально приостановлен{cause} до {} (ещё {seconds} с); HTTP-запрос не отправлен",
            retry_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        ),
    }
}

async fn execute_request(
    client: &Client,
    method: Method,
    url: Url,
    token: &str,
    body: Option<serde_json::Value>,
    allow_not_found: bool,
) -> Result<Option<Response>> {
    let may_retry = method == Method::GET;
    let cost = request_quota_cost(&url);
    let mut attempt = 0;
    loop {
        // Клиентский throttle: не отправляем запрос, пока в бакете не наберётся
        // его стоимость. Ждём ДО взятия gate, чтобы sleep не сериализовал учёт
        // серверного Retry-After. Каждый повтор (retry) тратит кванты заново -
        // это новый HTTP-запрос.
        gmail_acquire_quota(cost).await;
        // Keep request initiation process-wide serial. Most importantly, a
        // second startup/realtime task waiting here observes a quota deadline
        // set by the first 429 and never sends another HTTP request.
        let mut request_gate = gmail_request_gate().lock().await;
        if let Some((remaining, retry_at)) = request_gate.active_block(Instant::now()) {
            return Err(throttled_error(remaining, retry_at, None));
        }
        let mut request = client
            .request(method.clone(), url.clone())
            .bearer_auth(token);
        request = match body.as_ref() {
            Some(body) => request.json(body),
            None => request.header(reqwest::header::CONTENT_LENGTH, 0),
        };
        let response = request.send().await.map_err(|error| Error::Backend {
            backend: "gmail-api".into(),
            message: error.to_string(),
        })?;
        if allow_not_found && response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if response.status().is_success() {
            return Ok(Some(response));
        }

        let status = response.status();
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let response_body = response.text().await.unwrap_or_default();

        if quota_limited_response(status, &response_body) {
            let delay = retry_delay(0, retry_after.as_deref(), &response_body);
            let retry_at = request_gate.block_for(Instant::now(), chrono::Utc::now(), delay);
            tracing::warn!(
                %status,
                retry_ms = delay.as_millis(),
                retry_at = %retry_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                "Gmail API quota gate установлен; новые HTTP-запросы временно запрещены"
            );
            return Err(throttled_error(delay, retry_at, Some(status)));
        }

        if !may_retry || !retryable_response(status, &response_body) || attempt >= MAX_READ_RETRIES
        {
            return Err(Error::Backend {
                backend: "gmail-api".into(),
                message: format!("HTTP {status}: {response_body}"),
            });
        }

        // 408/5xx keep a short bounded retry. Release the process-wide gate
        // while waiting; unlike a quota deadline these errors do not prohibit
        // another independent Gmail request.
        let delay = bounded_backoff(attempt);
        tracing::warn!(
            %status,
            attempt = attempt + 1,
            retry_ms = delay.as_millis(),
            "Gmail API временно ограничил чтение; повторяем с backoff"
        );
        drop(request_gate);
        tokio::time::sleep(delay).await;
        attempt += 1;
    }
}

async fn request(
    client: &Client,
    method: Method,
    url: Url,
    token: &str,
    body: Option<serde_json::Value>,
) -> Result<Response> {
    execute_request(client, method, url, token, body, false)
        .await?
        .ok_or_else(|| Error::Other("Gmail API неожиданно вернул пустой ответ".into()))
}

async fn get_allow_not_found(client: &Client, url: Url, token: &str) -> Result<Option<Response>> {
    execute_request(client, Method::GET, url, token, None, true).await
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
    let raw = message.raw.ok_or_else(|| Error::Backend {
        backend: "gmail-message".into(),
        message: "Gmail API не вернул raw MIME".into(),
    })?;
    GMAIL_RAW_B64
        .decode(raw.as_bytes())
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

async fn list_backfill_page(
    client: &Client,
    access_token: &str,
    page_token: Option<&str>,
) -> Result<(Vec<String>, Option<String>)> {
    let mut list_url = url(&["messages"])?;
    list_url
        .query_pairs_mut()
        .append_pair("maxResults", &BACKFILL_PAGE_SIZE.to_string())
        .append_pair("includeSpamTrash", "true");
    if let Some(page_token) = page_token {
        list_url
            .query_pairs_mut()
            .append_pair("pageToken", page_token);
    }
    let listed: MessageList = request(client, Method::GET, list_url, access_token, None)
        .await?
        .json()
        .await
        .map_err(|error| Error::Backend {
            backend: "gmail-messages".into(),
            message: error.to_string(),
        })?;
    let next_page_token = listed.next_page_token;
    let mut ids: Vec<_> = listed
        .messages
        .into_iter()
        .map(|message| message.id)
        .collect();
    ids.sort();
    ids.dedup();
    Ok((ids, next_page_token))
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
    let mut results = Vec::with_capacity(ids.len());
    for (window, chunk) in ids.chunks(MESSAGE_GETS_PER_QUOTA_WINDOW).enumerate() {
        if window > 0 {
            tracing::info!(
                remaining = ids
                    .len()
                    .saturating_sub(window * MESSAGE_GETS_PER_QUOTA_WINDOW),
                wait_seconds = QUOTA_WINDOW.as_secs(),
                "Gmail API: продолжаем большую дельту после quota-safe паузы"
            );
            tokio::time::sleep(QUOTA_WINDOW).await;
        }
        let fetched: Vec<(String, Option<RawMessage>)> = stream::iter(chunk.iter().cloned())
            .map(|id| {
                let client = client.clone();
                let token = token.clone();
                async move {
                    let mut message_url = url(&["messages", &id])?;
                    // Для списка и уведомления нужны заголовки, flags и preview,
                    // а не многомегабайтные вложения. Полный raw MIME лениво
                    // загружается fetch_message_raw при открытии письма.
                    message_url
                        .query_pairs_mut()
                        .append_pair("format", "metadata");
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
            .buffer_unordered(MESSAGE_GET_CONCURRENCY)
            .try_collect()
            .await?;
        results.extend(fetched);
    }
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

fn metadata_projection_raw(message: &RawMessage) -> Vec<u8> {
    const LIST_HEADERS: &[&str] = &[
        "From",
        "To",
        "Cc",
        "Bcc",
        "Subject",
        "Date",
        "Message-ID",
        "In-Reply-To",
        "References",
        "Authentication-Results",
        "Received-SPF",
        "List-Unsubscribe",
        "List-Unsubscribe-Post",
    ];
    let mut raw = String::new();
    for header in &message.payload.headers {
        if LIST_HEADERS
            .iter()
            .any(|name| header.name.eq_ignore_ascii_case(name))
        {
            let value = header.value.replace(['\r', '\n'], " ");
            raw.push_str(&header.name);
            raw.push_str(": ");
            raw.push_str(&value);
            raw.push_str("\r\n");
        }
    }
    raw.push_str("MIME-Version: 1.0\r\n");
    raw.push_str("Content-Type: text/plain; charset=utf-8\r\n");
    raw.push_str("X-Truemail-Body-Pending: true\r\n\r\n");
    raw.push_str(&message.snippet);
    raw.into_bytes()
}

fn project_messages(
    fetched: Vec<RawMessage>,
    included: &HashSet<String>,
) -> Result<Vec<DiscoveredMessage>> {
    let mut messages = Vec::new();
    for message in fetched {
        let (raw, body_fetched) = match message.raw.as_deref() {
            Some(encoded) => (
                GMAIL_RAW_B64
                    .decode(encoded.as_bytes())
                    .map_err(|error| Error::Backend {
                        backend: "gmail-message".into(),
                        message: format!("{}: raw не декодирован: {error}", message.id),
                    })?,
                true,
            ),
            None => (metadata_projection_raw(&message), false),
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
                body_fetched,
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
    let stored_cursor = cursors
        .values()
        .find_map(|cursor| decode_sync_cursor(cursor.sync_token.as_deref()));

    // Always drain realtime history first. Backfill is independent and adds at
    // most one small page, whose nextPageToken is persisted in the same cursor.
    let (mut ids, mut cursor) = if let Some(mut cursor) = stored_cursor {
        match history_delta(&client, access_token, &cursor.history_id).await? {
            Some((ids, latest_history_id)) => {
                cursor.history_id = latest_history_id;
                (ids, cursor)
            }
            None => {
                let history_id = profile_history_id(&client, access_token).await?;
                tracing::warn!(
                    "Gmail historyId устарел; backfill перезапущен небольшими страницами"
                );
                (Vec::new(), GmailSyncCursor::bootstrap(history_id))
            }
        }
    } else {
        // Capture history before the first page. Mail arriving during the
        // page load is then observed by history.list on the next sync.
        let history_id = profile_history_id(&client, access_token).await?;
        (Vec::new(), GmailSyncCursor::bootstrap(history_id))
    };

    let history_count = ids.len();
    let mut backfill_count = 0;
    if !cursor.backfill_complete {
        let (backfill_ids, next_page_token) =
            list_backfill_page(&client, access_token, cursor.backfill_page_token.as_deref())
                .await?;
        backfill_count = backfill_ids.len();
        ids.extend(backfill_ids);
        cursor.backfill_page_token = next_page_token;
        cursor.backfill_complete = cursor.backfill_page_token.is_none();
    }
    ids.sort();
    ids.dedup();

    let changed_remote_ids = ids.clone();
    let (fetched, _not_found) = fetch_messages(&client, access_token, ids).await?;
    let messages = project_messages(fetched, &included)?;
    let sync_token = encode_sync_cursor(&cursor)?;
    for folder in &mut folders {
        folder.sync_token = Some(sync_token.clone());
    }
    tracing::info!(
        history = history_count,
        backfill = backfill_count,
        backfill_complete = cursor.backfill_complete,
        "Gmail incremental batch prepared"
    );
    Ok(ImapDiscovery {
        folders,
        messages,
        server_uids: Vec::new(),
        reset_folders: Vec::new(),
        // A single backfill page is never a global mailbox snapshot.
        remote_snapshot: None,
        changed_remote_ids,
        flag_updates: Vec::new(),
        deleted_uids: Vec::new(),
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
                raw: Some(GMAIL_RAW_B64.encode(raw)),
                size_estimate: raw.len() as u32,
                snippet: String::new(),
                payload: GmailPayload::default(),
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
        assert!(projected.iter().all(|message| message.body_fetched));
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
                raw: Some("%%%".into()),
                size_estimate: 3,
                snippet: String::new(),
                payload: GmailPayload::default(),
            }],
            &HashSet::from(["INBOX".into()]),
        )
        .expect_err("invalid raw must fail the whole sync");
        assert!(error.to_string().contains("raw не декодирован"));
    }

    #[test]
    fn gmail_metadata_projection_defers_full_body_download() {
        let projected = project_messages(
            vec![RawMessage {
                id: "remote-metadata".into(),
                label_ids: vec!["INBOX".into()],
                raw: None,
                size_estimate: 50_000_000,
                snippet: "small preview".into(),
                payload: GmailPayload {
                    headers: vec![
                        GmailHeader {
                            name: "From".into(),
                            value: "sender@example.test".into(),
                        },
                        GmailHeader {
                            name: "Subject".into(),
                            value: "Large attachment".into(),
                        },
                    ],
                },
            }],
            &HashSet::from(["INBOX".into()]),
        )
        .expect("project metadata");
        assert_eq!(projected.len(), 1);
        assert!(!projected[0].body_fetched);
        let raw = String::from_utf8_lossy(&projected[0].raw);
        assert!(raw.contains("Subject: Large attachment"));
        assert!(raw.ends_with("small preview"));
    }

    #[test]
    fn gmail_rate_limit_responses_are_retryable() {
        assert!(retryable_response(StatusCode::TOO_MANY_REQUESTS, ""));
        assert!(retryable_response(
            StatusCode::FORBIDDEN,
            r#"{"reason":"userRateLimitExceeded"}"#
        ));
        assert!(!retryable_response(StatusCode::UNAUTHORIZED, ""));
        assert!(!retryable_response(
            StatusCode::FORBIDDEN,
            r#"{"reason":"domainPolicy"}"#
        ));
    }

    #[test]
    fn retry_after_seconds_take_precedence_over_short_backoff() {
        assert_eq!(
            server_retry_delay(Some("37"), "ignored"),
            Some(Duration::from_secs(37))
        );
    }

    #[test]
    fn retry_after_timestamp_is_extracted_from_gmail_error_body() {
        let retry_at = chrono::Utc::now() + chrono::Duration::seconds(20);
        let body = format!(
            r#"{{"message":"User-rate limit exceeded. Retry after {}"}}"#,
            retry_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        );
        let delay = server_retry_delay(None, &body).expect("retry timestamp");
        assert!(delay >= Duration::from_secs(18));
        assert!(delay <= Duration::from_secs(20));
    }

    #[test]
    fn structured_cursor_round_trips_backfill_progress() {
        let cursor = GmailSyncCursor {
            v: 1,
            history_id: "history-42".into(),
            backfill_page_token: Some("page-7".into()),
            backfill_complete: false,
        };
        let encoded = encode_sync_cursor(&cursor).expect("encode cursor");
        assert_eq!(decode_sync_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn legacy_history_id_is_migrated_as_completed_backfill() {
        assert_eq!(
            decode_sync_cursor(Some("987654321")),
            Some(GmailSyncCursor {
                v: 1,
                history_id: "987654321".into(),
                backfill_page_token: None,
                backfill_complete: true,
            })
        );
    }

    #[test]
    fn malformed_structured_cursor_requires_fresh_bootstrap() {
        assert_eq!(decode_sync_cursor(Some(r#"{"v":1}"#)), None);
        assert_eq!(decode_sync_cursor(Some("")), None);
    }

    #[test]
    fn request_gate_blocks_until_shared_deadline_then_clears() {
        let now = Instant::now();
        let now_utc = chrono::Utc::now();
        let mut gate = GmailRequestGate::default();
        let retry_at = gate.block_for(now, now_utc, Duration::from_secs(37));
        assert_eq!(retry_at, now_utc + chrono::Duration::seconds(37));

        let (remaining, stored_retry_at) = gate
            .active_block(now + Duration::from_secs(7))
            .expect("gate must remain active");
        assert_eq!(remaining, Duration::from_secs(30));
        assert_eq!(stored_retry_at, retry_at);

        assert!(gate.active_block(now + Duration::from_secs(37)).is_none());
        assert!(gate.not_before.is_none());
        assert!(gate.retry_at.is_none());
    }

    #[test]
    fn local_throttle_error_is_explicit_that_no_http_was_sent() {
        let error = throttled_error(
            Duration::from_millis(1_001),
            chrono::Utc::now(),
            Some(StatusCode::TOO_MANY_REQUESTS),
        )
        .to_string();
        assert!(error.contains("HTTP 429"));
        assert!(error.contains("ещё 2 с"));
        assert!(error.contains("HTTP-запрос не отправлен"));
    }
}
