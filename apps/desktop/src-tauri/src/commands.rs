//! Tauri-команды: тонкая прослойка между фронтендом и ядром.
//! Фронтенд (ui/) вызывает их через `invoke(...)`.

use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    str::FromStr,
    sync::Arc,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
use tauri_plugin_shell::ShellExt;
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use truemail_core::Core;
use truemail_core::account::{
    ContactInput, EventInput, RemoteObject, configured_google_client_id,
    configured_google_client_secret, configured_microsoft_client_id, configured_microsoft_tenant,
    configured_yandex_client_id, configured_yandex_redirect_uri,
};
use truemail_core::api::{
    ApiAuditEntry, ApiClient, Capability, CreatedApiClient, McpTool, mcp_tools,
};
use truemail_core::model::{
    Account, AuthKind, BackendKind, Contact, Event, Folder, Keybinding, MailRule, MailRuleInput,
    MessageFull, MessageMeta, MessageTemplate, Provider, Security, ServerConfig, Signature,
    SmartFolder,
};
use truemail_core::storage::repo::CalendarSummary;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

/// Общее состояние приложения — ядро.
pub struct AppState {
    pub core: tokio::sync::RwLock<Option<Arc<Core>>>,
    pub oauth: tokio::sync::Mutex<HashMap<String, PendingOAuth>>,
    pub syncing: Arc<tokio::sync::Mutex<HashSet<i64>>>,
    // Отдельный флаг занятости для aux-синхронизации (календарь/контакты/задачи),
    // чтобы тяжёлый почтовый sync Gmail не блокировал обновление календаря и наоборот.
    pub syncing_aux: Arc<tokio::sync::Mutex<HashSet<i64>>>,
    pub watching: Arc<tokio::sync::Mutex<HashSet<i64>>>,
    pub generation: Arc<std::sync::atomic::AtomicU64>,
    pub api_server: Arc<tokio::sync::Mutex<Option<truemail_core::api::RunningApiServer>>>,
    pub shortcut_actions: Arc<std::sync::RwLock<HashMap<String, String>>>,
    // true, когда пользователь выбрал "Выход" из трея: закрытие окна тогда
    // действительно завершает приложение, а не сворачивает в трей.
    pub quitting: Arc<std::sync::atomic::AtomicBool>,
    // Гарантирует единственный фоновый цикл напоминаний о встречах.
    pub reminders_started: Arc<std::sync::atomic::AtomicBool>,
    // Куда прижимать окно уведомлений; кэш настройки notify_position,
    // чтобы позиционирование не лезло в БД (оно синхронное).
    pub notify_anchor: Arc<std::sync::Mutex<NotifyAnchor>>,
}

pub fn default_keybindings() -> Vec<Keybinding> {
    [
        ("toggle_window", "global", "Ctrl+Shift+M"),
        ("compose_global", "global", "Ctrl+Shift+C"),
        ("quick_search", "global", "Ctrl+Shift+F"),
        ("palette", "local", "Ctrl+K"),
        ("compose", "local", "C"),
        ("reply", "local", "R"),
        ("reply_all", "local", "A"),
        ("forward", "local", "F"),
        ("archive", "local", "E"),
        ("snooze", "local", "H"),
        ("next_message", "local", "J"),
        ("prev_message", "local", "K"),
        ("delete", "local", "Del"),
    ]
    .into_iter()
    .map(|(action, scope, combo)| Keybinding {
        action: action.into(),
        scope: scope.into(),
        combo: combo.into(),
    })
    .collect()
}

pub fn register_global_shortcuts(app: &AppHandle, bindings: &[Keybinding]) -> anyhow::Result<()> {
    let manager = app.global_shortcut();
    manager.unregister_all()?;
    let mut actions = HashMap::new();
    for binding in bindings.iter().filter(|binding| binding.scope == "global") {
        let emitted = match binding.action.as_str() {
            "toggle_window" => "toggle",
            "compose_global" => "compose",
            "quick_search" => "search",
            _ => continue,
        };
        let shortcut = Shortcut::from_str(&binding.combo)
            .map_err(|error| anyhow::anyhow!("{}: {error}", binding.combo))?;
        manager.register(shortcut)?;
        actions.insert(shortcut.to_string(), emitted.to_owned());
    }
    *app.state::<AppState>()
        .shortcut_actions
        .write()
        .map_err(|_| anyhow::anyhow!("блокировка горячих клавиш повреждена"))? = actions;
    Ok(())
}

/// Угол экрана для окна уведомлений.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NotifyAnchor {
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl NotifyAnchor {
    /// Привычное для платформы место: Windows/Linux - правый нижний угол,
    /// macOS - правый верхний (там уведомления системы живут именно там).
    pub fn platform_default() -> Self {
        if cfg!(target_os = "macos") {
            NotifyAnchor::TopRight
        } else {
            NotifyAnchor::BottomRight
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "top-left" => NotifyAnchor::TopLeft,
            "top-center" => NotifyAnchor::TopCenter,
            "top-right" => NotifyAnchor::TopRight,
            "bottom-left" => NotifyAnchor::BottomLeft,
            "bottom-center" => NotifyAnchor::BottomCenter,
            "bottom-right" => NotifyAnchor::BottomRight,
            _ => NotifyAnchor::platform_default(),
        }
    }

    fn is_top(self) -> bool {
        matches!(
            self,
            NotifyAnchor::TopLeft | NotifyAnchor::TopCenter | NotifyAnchor::TopRight
        )
    }
}

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct PendingOAuth {
    email: String,
    verifier: String,
    client_id: String,
}

#[derive(Serialize)]
pub struct PendingOAuthResponse {
    mode: String,
    state: Option<String>,
    connected: Option<ConnectedAccount>,
    password_config: Option<PasswordConnectionInfo>,
}

#[derive(Serialize)]
pub struct PasswordConnectionInfo {
    provider: Provider,
    backend_kind: BackendKind,
    username: String,
    imap: Option<ServerConfig>,
    smtp: Option<ServerConfig>,
    jmap_url: Option<String>,
    ews_url: Option<String>,
}

#[derive(Serialize)]
pub struct ConnectedAccount {
    account: Account,
    mail_folders: usize,
    calendars: usize,
    events: usize,
    contacts: usize,
    warnings: Vec<String>,
}

#[derive(Serialize)]
pub struct ApiError {
    pub(crate) message: String,
}

impl From<truemail_core::Error> for ApiError {
    fn from(e: truemail_core::Error) -> Self {
        ApiError {
            message: e.to_string(),
        }
    }
}

type CmdResult<T> = Result<T, ApiError>;

fn api_error(message: impl Into<String>) -> ApiError {
    ApiError {
        message: message.into(),
    }
}

const DEFAULT_UPDATE_ENDPOINT: &str = "https://chernov.gitverse.site/truemail/latest.json";

fn update_manifest_endpoint() -> CmdResult<url::Url> {
    let value = std::env::var("TRUEMAIL_UPDATE_ENDPOINT")
        .unwrap_or_else(|_| DEFAULT_UPDATE_ENDPOINT.to_owned());
    url::Url::parse(value.trim())
        .map_err(|error| api_error(format!("адрес манифеста обновлений: {error}")))
}

async fn available_update(app: &AppHandle) -> CmdResult<Option<Update>> {
    let endpoint = update_manifest_endpoint()?;
    app.updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|error| api_error(error.to_string()))?
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| api_error(error.to_string()))?
        .check()
        .await
        .map_err(|error| api_error(format!("проверка обновления: {error}")))
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    current_version: String,
    available_version: Option<String>,
    notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct UpdateProgress {
    event: &'static str,
    downloaded: u64,
    total: Option<u64>,
}

#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> CmdResult<UpdateInfo> {
    let update = available_update(&app).await?;
    Ok(UpdateInfo {
        current_version: app.package_info().version.to_string(),
        available_version: update.as_ref().map(|value| value.version.clone()),
        notes: update.and_then(|value| value.body),
    })
}

pub async fn announce_available_update(app: AppHandle) -> CmdResult<()> {
    let info = check_for_update(app.clone()).await?;
    if info.available_version.is_some() {
        let _ = app.emit("truemail-update-available", info);
    }
    Ok(())
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> CmdResult<()> {
    let Some(update) = available_update(&app).await? else {
        return Err(api_error("новая версия уже не найдена"));
    };
    let progress_app = app.clone();
    let finished_app = app.clone();
    let mut downloaded = 0_u64;
    update
        .download_and_install(
            move |chunk, total| {
                downloaded = downloaded.saturating_add(chunk as u64);
                let _ = progress_app.emit(
                    "truemail-update-progress",
                    UpdateProgress {
                        event: "progress",
                        downloaded,
                        total,
                    },
                );
            },
            move || {
                let _ = finished_app.emit(
                    "truemail-update-progress",
                    UpdateProgress {
                        event: "finished",
                        downloaded: 0,
                        total: None,
                    },
                );
            },
        )
        .await
        .map_err(|error| api_error(format!("установка обновления: {error}")))?;
    app.state::<AppState>()
        .quitting
        .store(true, std::sync::atomic::Ordering::SeqCst);
    app.restart();
}

#[derive(Serialize)]
pub struct BootstrapStatus {
    ready: bool,
    data_dir: String,
}

#[derive(Serialize)]
pub struct CalendarData {
    calendars: Vec<CalendarSummary>,
    events: Vec<Event>,
}

#[derive(Serialize)]
pub struct StorageStatus {
    data_dir: String,
    total_bytes: u64,
    database_bytes: u64,
    blob_bytes: u64,
}

#[derive(Debug, Deserialize)]
pub struct SendAttachmentRequest {
    filename: String,
    mime_type: String,
    data: Vec<u8>,
}

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    account_id: i64,
    to: Vec<String>,
    cc: Vec<String>,
    bcc: Vec<String>,
    subject: String,
    body_text: String,
    body_html: Option<String>,
    attachments: Vec<SendAttachmentRequest>,
}

async fn core(state: &State<'_, AppState>) -> CmdResult<Arc<Core>> {
    state.core.read().await.clone().ok_or_else(|| ApiError {
        message: "Хранилище ещё не создано. Завершите первоначальную настройку.".into(),
    })
}

/// Показать собственное уведомление (кросс-платформенное окно в стиле софта).
/// Данные уходят в webview-окно "notify", которое рисует карточку с кнопками.
fn push_notification(app: &AppHandle, payload: serde_json::Value) {
    position_notify_window(app);
    let _ = app.emit_to("notify", "notify-push", payload);
    if let Some(window) = app.get_webview_window("notify") {
        // Пока окно показано, курсор ему нужен - иначе кнопки карточки не нажать.
        let _ = window.set_ignore_cursor_events(false);
        let _ = window.show();
    }
}

/// Прижать окно уведомлений к выбранному пользователем углу основного монитора.
pub fn position_notify_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("notify") else {
        return;
    };
    // primary_monitor может не отдать монитор (RDP, смена конфигурации) -
    // тогда окно осталось бы в дефолтной позиции поверх главного.
    let monitor = window
        .primary_monitor()
        .ok()
        .flatten()
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| {
            window
                .available_monitors()
                .ok()
                .and_then(|list| list.into_iter().next())
        });
    let Some(monitor) = monitor else {
        return;
    };
    let anchor = app
        .state::<AppState>()
        .notify_anchor
        .lock()
        .map(|value| *value)
        .unwrap_or_else(|_| NotifyAnchor::platform_default());
    let screen = monitor.size();
    let origin = monitor.position();
    let Ok(size) = window.outer_size() else {
        return;
    };
    let margin = 16i32;
    // Запас под панель задач/док, чтобы карточка не пряталась под ней.
    let reserved = 48i32;
    let free_w = (screen.width as i32 - size.width as i32).max(0);
    let free_h = (screen.height as i32 - size.height as i32).max(0);
    let x = match anchor {
        NotifyAnchor::TopLeft | NotifyAnchor::BottomLeft => margin.min(free_w),
        NotifyAnchor::TopCenter | NotifyAnchor::BottomCenter => free_w / 2,
        NotifyAnchor::TopRight | NotifyAnchor::BottomRight => (free_w - margin).max(0),
    };
    let y = if anchor.is_top() {
        margin.min(free_h)
    } else {
        (free_h - margin - reserved).max(0)
    };
    let _ = window.set_position(tauri::PhysicalPosition::new(origin.x + x, origin.y + y));
}

/// Подогнать высоту окна уведомлений под стек карточек: лишняя прозрачная
/// площадь всё равно ловит курсор и съедает клики по окнам под ней.
#[tauri::command]
pub fn notify_resize(app: AppHandle, height: f64) -> CmdResult<()> {
    if let Some(window) = app.get_webview_window("notify") {
        let height = height.clamp(1.0, 640.0);
        let _ = window.set_size(tauri::LogicalSize::new(380.0, height));
        position_notify_window(&app);
    }
    Ok(())
}

/// Сменить угол показа уведомлений: сохраняем в БД и в кэш состояния.
#[tauri::command]
pub async fn set_notify_position(
    app: AppHandle,
    state: State<'_, AppState>,
    value: String,
) -> CmdResult<()> {
    if let Ok(mut anchor) = state.notify_anchor.lock() {
        *anchor = NotifyAnchor::parse(&value);
    }
    core(&state)
        .await?
        .db
        .set_setting("notify_position", &value)
        .await?;
    position_notify_window(&app);
    Ok(())
}

/// Уведомление о новых письмах: отправитель, тема и превью последнего письма.
async fn notify_new_mail(
    app: &AppHandle,
    core: &Arc<Core>,
    account: &truemail_core::model::Account,
    count: usize,
) {
    let meta = core
        .db
        .latest_inbox_message(account.id)
        .await
        .ok()
        .flatten();
    let payload = match meta {
        Some((id, from, subject, preview)) => serde_json::json!({
            "kind": "mail",
            "title": if from.trim().is_empty() { account.email.clone() } else { from },
            "subject": if subject.trim().is_empty() { "(без темы)".to_owned() } else { subject },
            "preview": preview.trim(),
            "count": count,
            "account_id": account.id,
            "message_id": id,
        }),
        None => serde_json::json!({
            "kind": "mail",
            "title": account.email.clone(),
            "subject": match count { 1 => "1 новое письмо".to_owned(), n => format!("{n} новых писем") },
            "preview": "",
            "count": count,
            "account_id": account.id,
            "message_id": serde_json::Value::Null,
        }),
    };
    push_notification(app, payload);
}

/// Почти реалтайм-поллинг новых писем Gmail: лёгкая проверка ID Входящих,
/// уведомление и дозагрузка при появлении новых. Gmail API push требует
/// внешней Cloud Pub/Sub-инфраструктуры, которой у desktop-only клиента нет.
fn observe_gmail_message_ids(
    observed: &mut HashMap<i64, HashSet<String>>,
    account_id: i64,
    ids: Vec<String>,
) -> Option<Vec<String>> {
    use std::collections::hash_map::Entry;
    match observed.entry(account_id) {
        Entry::Vacant(entry) => {
            entry.insert(ids.into_iter().collect());
            None
        }
        Entry::Occupied(mut entry) => {
            let seen = entry.get_mut();
            let fresh = ids
                .iter()
                .filter(|id| !seen.contains(*id))
                .cloned()
                .collect();
            seen.extend(ids);
            Some(fresh)
        }
    }
}

async fn gmail_realtime_loop(
    core: Arc<Core>,
    app: AppHandle,
    syncing: Arc<tokio::sync::Mutex<HashSet<i64>>>,
) {
    let mut observed: HashMap<i64, HashSet<String>> = HashMap::new();
    let mut pending: HashMap<i64, HashSet<String>> = HashMap::new();
    loop {
        let accounts = match core.db.list_accounts().await {
            Ok(accounts) => accounts,
            Err(_) => {
                tokio::time::sleep(std::time::Duration::from_secs(25)).await;
                continue;
            }
        };
        for account in accounts {
            if account.provider != truemail_core::model::Provider::Gmail {
                continue;
            }
            let latest_ids = match core.accounts.gmail_latest_message_ids(&account).await {
                Ok(ids) => ids,
                Err(_) => continue,
            };
            // Первый снимок — только исходная точка. Наличие письма в Gmail, но
            // отсутствие его в ещё заполняющейся локальной БД не делает письмо новым.
            let Some(fresh) = observe_gmail_message_ids(&mut observed, account.id, latest_ids)
            else {
                tracing::debug!(account = %account.email, "Gmail realtime: исходный снимок сохранён");
                continue;
            };
            if !fresh.is_empty() {
                pending.entry(account.id).or_default().extend(fresh);
            }
            let pending_count = pending.get(&account.id).map(HashSet::len).unwrap_or(0);
            if pending_count == 0 {
                continue;
            }
            // Если стартовая или ручная синхронизация ещё идёт, сохраняем новые
            // ID в pending и ждём. Показывать последнее старое письмо из БД нельзя.
            let free = {
                let mut guard = syncing.lock().await;
                guard.insert(account.id)
            };
            if !free {
                continue;
            }
            let synced = core.accounts.sync_mail_inbox(&account).await;
            syncing.lock().await.remove(&account.id);
            match synced {
                Ok(_) => {
                    pending.remove(&account.id);
                    let _ = app.emit("truemail-data-changed", account.id);
                    notify_new_mail(&app, &core, &account, pending_count).await;
                }
                Err(error) => {
                    tracing::warn!(account = %account.email, %error, "Gmail realtime: не удалось загрузить новые письма");
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(25)).await;
    }
}

/// Найти начало ближайшего http/https URL в тексте.
fn find_url_start(text: &str) -> Option<usize> {
    match (text.find("http://"), text.find("https://")) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

/// Извлечь все ссылки (уникальные) из места и описания встречи - для
/// кликабельных кнопок "Присоединиться" в уведомлении.
fn extract_meeting_urls(location: &str, description: &str) -> Vec<String> {
    let mut urls: Vec<String> = Vec::new();
    for text in [location, description] {
        let mut search = text;
        while let Some(pos) = find_url_start(search) {
            let tail = &search[pos..];
            let end = tail
                .char_indices()
                .find(|(_, c)| {
                    c.is_whitespace() || matches!(c, '<' | '>' | '"' | ')' | ']' | '}' | ',')
                })
                .map(|(index, _)| index)
                .unwrap_or(tail.len());
            let url = tail[..end].trim_end_matches(['.', ';', ':']).to_owned();
            if url.len() > 8 && !urls.contains(&url) {
                urls.push(url);
            }
            search = &tail[end..];
        }
    }
    urls
}

/// Открыть ссылку в браузере по умолчанию: из уведомления, письма, отписки.
///
/// Webview сам ссылку не откроет: target="_blank" в нём означает попап, а Tauri
/// его блокирует, и клик молча ничего не делает. Наружу - только через опенер.
#[tauri::command]
pub fn open_external_url(app: AppHandle, url: String) -> CmdResult<()> {
    // Наружу отдаём только веб-ссылки: file:// и прочие схемы из письма
    // запускали бы произвольные обработчики в системе.
    let allowed = url.starts_with("https://") || url.starts_with("http://");
    if !allowed {
        return Err(ApiError {
            message: "ссылку такого вида открывать нельзя".into(),
        });
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|error| ApiError {
            message: format!("не удалось открыть ссылку: {error}"),
        })
}

/// Разобрать время начала события (ISO 8601, с таймзоной или без).
fn parse_event_start(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::TimeZone;
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(value) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    for format in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y%m%dT%H%M%S"] {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(value, format) {
            return chrono::Local
                .from_local_datetime(&naive)
                .single()
                .map(|dt| dt.with_timezone(&chrono::Utc));
        }
    }
    None
}

/// Фоновый цикл: уведомляет о встречах, начинающихся в ближайшие 10 минут.
async fn reminders_loop(core: Arc<Core>, app: AppHandle) {
    let mut notified: HashSet<String> = HashSet::new();
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        let events = match core.db.list_calendars_and_events().await {
            Ok((_, events)) => events,
            Err(_) => continue,
        };
        let now = chrono::Utc::now();
        for event in events {
            // Напоминания только если они заданы в самой встрече (alarms).
            if event.all_day || event.alarms.is_empty() {
                continue;
            }
            let Some(start) = parse_event_start(&event.dtstart) else {
                continue;
            };
            let minutes = (start - now).num_minutes();
            for alarm in &event.alarms {
                let trigger = alarm.trigger_minutes.max(0) as i64;
                // Момент напоминания настал: до начала осталось не больше trigger.
                if minutes > trigger || minutes < -1 {
                    continue;
                }
                let key = format!(
                    "{}|{}|{}",
                    event.uid.as_deref().unwrap_or(&event.summary),
                    event.dtstart,
                    trigger
                );
                if !notified.insert(key) {
                    continue;
                }
                let when = if minutes <= 0 {
                    "сейчас".to_owned()
                } else {
                    format!("через {minutes} мин")
                };
                let urls = extract_meeting_urls(
                    event.location.as_deref().unwrap_or(""),
                    event.description.as_deref().unwrap_or(""),
                );
                push_notification(
                    &app,
                    serde_json::json!({
                        "kind": "event",
                        "title": format!("Встреча {when}"),
                        "subject": event.summary.clone(),
                        "preview": event.location.clone().unwrap_or_default(),
                        "urls": urls,
                        "count": 1,
                        "account_id": serde_json::Value::Null,
                        "message_id": serde_json::Value::Null,
                    }),
                );
            }
        }
        // Не даём множеству бесконечно расти между перезапусками.
        if notified.len() > 1000 {
            notified.clear();
        }
    }
}

#[tauri::command]
pub async fn bootstrap_status(state: State<'_, AppState>) -> CmdResult<BootstrapStatus> {
    let ready = state.core.read().await.is_some();
    let data_dir = truemail_core::crypto::load_data_dir()?
        .unwrap_or_else(super::default_data_dir)
        .to_string_lossy()
        .into_owned();
    Ok(BootstrapStatus { ready, data_dir })
}

#[tauri::command]
pub async fn initialize_storage(
    state: State<'_, AppState>,
    data_dir: String,
    locale: String,
    mut entropy: Vec<u8>,
) -> CmdResult<()> {
    if state.core.read().await.is_some() {
        return Err(ApiError {
            message: "Хранилище этой установки уже создано".into(),
        });
    }
    let path = PathBuf::from(data_dir.trim());
    if !path.is_absolute() {
        return Err(ApiError {
            message: "Выберите полный путь к папке данных".into(),
        });
    }
    std::fs::create_dir_all(&path).map_err(|error| ApiError {
        message: format!("Не удалось создать папку данных: {error}"),
    })?;
    if path.join("truemail.db").exists() {
        return Err(ApiError {
            message: "В выбранной папке уже есть truemail.db, но ключи этой установки не найдены. Выберите другую папку.".into(),
        });
    }
    let probe = path.join(format!(".truemail-write-test-{}", std::process::id()));
    std::fs::write(&probe, b"truemail").map_err(|error| ApiError {
        message: format!("Нет доступа на запись в выбранную папку: {error}"),
    })?;
    let _ = std::fs::remove_file(probe);

    let initialization = async {
        truemail_core::crypto::initialize_keys_from_entropy(&entropy)?;
        truemail_core::crypto::store_data_dir(&path)?;
        let initialized = Arc::new(Core::bootstrap(path.clone()).await?);
        initialized.db.set_setting("locale", &locale).await?;
        initialized
            .db
            .set_setting("data_dir", &path.to_string_lossy())
            .await?;
        Ok::<Arc<Core>, truemail_core::Error>(initialized)
    }
    .await;
    entropy.fill(0);

    let initialized = match initialization {
        Ok(initialized) => initialized,
        Err(error) => {
            let _ = truemail_core::crypto::remove_installation_keys();
            for suffix in [
                "",
                "-wal",
                "-shm",
                ".encrypted.migrating",
                ".plaintext.migrating",
            ] {
                let _ = std::fs::remove_file(path.join(format!("truemail.db{suffix}")));
            }
            let blobs = path.join("blobs");
            if blobs
                .read_dir()
                .is_ok_and(|mut entries| entries.next().is_none())
            {
                let _ = std::fs::remove_dir(blobs);
            }
            return Err(error.into());
        }
    };
    *state.core.write().await = Some(initialized);
    Ok(())
}

#[tauri::command]
pub async fn export_key_backup(
    state: State<'_, AppState>,
    path: String,
    mut password: String,
) -> CmdResult<()> {
    let _ = core(&state).await?;
    let path = PathBuf::from(path.trim());
    if !path.is_absolute() {
        password.zeroize();
        return Err(ApiError {
            message: "Выберите полный путь для резервной копии".into(),
        });
    }
    let backup = tokio::task::spawn_blocking(move || {
        let result = truemail_core::crypto::export_key_backup(&password);
        password.zeroize();
        result
    })
    .await
    .map_err(|error| ApiError {
        message: format!("Создание резервной копии прервано: {error}"),
    })??;
    std::fs::write(&path, backup).map_err(|error| ApiError {
        message: format!("Не удалось записать резервную копию: {error}"),
    })?;
    Ok(())
}

#[tauri::command]
pub async fn restore_key_backup(
    state: State<'_, AppState>,
    data_dir: String,
    backup_path: String,
    mut password: String,
) -> CmdResult<()> {
    if state.core.read().await.is_some() {
        password.zeroize();
        return Err(ApiError {
            message: "Хранилище уже открыто; существующие ключи не перезаписываются".into(),
        });
    }
    let data_dir = PathBuf::from(data_dir.trim());
    let backup_path = PathBuf::from(backup_path.trim());
    if !data_dir.is_absolute() || !backup_path.is_absolute() {
        password.zeroize();
        return Err(ApiError {
            message: "Выберите полные пути к архиву и резервной копии".into(),
        });
    }
    if !data_dir.join("truemail.db").is_file() {
        password.zeroize();
        return Err(ApiError {
            message: "В выбранной папке нет truemail.db".into(),
        });
    }
    let serialized = std::fs::read_to_string(&backup_path).map_err(|error| {
        password.zeroize();
        ApiError {
            message: format!("Не удалось прочитать резервную копию: {error}"),
        }
    })?;
    tokio::task::spawn_blocking(move || {
        let result = truemail_core::crypto::restore_key_backup(&serialized, &password);
        password.zeroize();
        result
    })
    .await
    .map_err(|error| ApiError {
        message: format!("Восстановление ключей прервано: {error}"),
    })??;

    let opened = async {
        truemail_core::crypto::store_data_dir(&data_dir)?;
        Core::bootstrap(data_dir.clone()).await
    }
    .await;
    let opened = match opened {
        Ok(core) => Arc::new(core),
        Err(error) => {
            let _ = truemail_core::crypto::remove_installation_keys();
            return Err(ApiError {
                message: format!(
                    "Ключи расшифрованы, но архив не открылся; восстановление отменено: {error}"
                ),
            });
        }
    };
    *state.core.write().await = Some(opened);
    Ok(())
}

#[tauri::command]
pub async fn list_accounts(state: State<'_, AppState>) -> CmdResult<Vec<Account>> {
    Ok(core(&state).await?.db.list_accounts().await?)
}

#[tauri::command]
pub async fn rename_account(
    state: State<'_, AppState>,
    account_id: i64,
    display_name: String,
) -> CmdResult<()> {
    Ok(core(&state)
        .await?
        .db
        .rename_account(account_id, &display_name)
        .await?)
}

#[derive(Serialize)]
pub struct LabelInfo {
    id: i64,
    name: String,
    color: Option<String>,
}

#[tauri::command]
pub async fn list_labels(state: State<'_, AppState>) -> CmdResult<Vec<LabelInfo>> {
    Ok(core(&state)
        .await?
        .db
        .list_labels()
        .await?
        .into_iter()
        .map(|(id, name, color)| LabelInfo { id, name, color })
        .collect())
}

#[tauri::command]
pub async fn create_label(
    state: State<'_, AppState>,
    name: String,
    color: String,
) -> CmdResult<i64> {
    Ok(core(&state).await?.db.create_label(&name, &color).await?)
}

#[tauri::command]
pub async fn update_label(
    state: State<'_, AppState>,
    id: i64,
    name: String,
    color: String,
) -> CmdResult<()> {
    Ok(core(&state)
        .await?
        .db
        .update_label(id, &name, &color)
        .await?)
}

#[tauri::command]
pub async fn delete_label(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    Ok(core(&state).await?.db.delete_label(id).await?)
}

#[tauri::command]
pub async fn toggle_message_label(
    state: State<'_, AppState>,
    message_id: i64,
    label_id: i64,
    on: bool,
) -> CmdResult<()> {
    Ok(core(&state)
        .await?
        .db
        .toggle_message_label(message_id, label_id, on)
        .await?)
}

#[tauri::command]
pub async fn message_label_ids(state: State<'_, AppState>, message_id: i64) -> CmdResult<Vec<i64>> {
    Ok(core(&state).await?.db.message_label_ids(message_id).await?)
}

/// Задать цвет аккаунта (аватары писем, сайдбар).
#[tauri::command]
pub async fn set_account_color(
    state: State<'_, AppState>,
    account_id: i64,
    color: String,
) -> CmdResult<()> {
    Ok(core(&state)
        .await?
        .db
        .set_account_color(account_id, &color)
        .await?)
}

/// Глубина локального кэша аккаунта в днях (0 - без ограничений).
#[tauri::command]
pub async fn set_account_retention(
    state: State<'_, AppState>,
    account_id: i64,
    days: i64,
) -> CmdResult<()> {
    Ok(core(&state)
        .await?
        .db
        .set_account_retention(account_id, days)
        .await?)
}

#[tauri::command]
pub async fn list_folders(state: State<'_, AppState>, account_id: i64) -> CmdResult<Vec<Folder>> {
    Ok(core(&state).await?.db.list_folders(account_id).await?)
}

#[tauri::command]
pub async fn set_folder_role(
    state: State<'_, AppState>,
    account_id: i64,
    role: String,
    folder_id: Option<i64>,
) -> CmdResult<()> {
    Ok(core(&state)
        .await?
        .db
        .set_folder_role(account_id, &role, folder_id)
        .await?)
}

#[tauri::command]
pub async fn rename_folder(
    state: State<'_, AppState>,
    folder_id: i64,
    new_name: String,
) -> CmdResult<()> {
    Ok(core(&state)
        .await?
        .accounts
        .rename_folder(folder_id, &new_name)
        .await?)
}

#[tauri::command]
pub async fn delete_folder(state: State<'_, AppState>, folder_id: i64) -> CmdResult<()> {
    Ok(core(&state)
        .await?
        .accounts
        .delete_folder(folder_id)
        .await?)
}

#[tauri::command]
pub async fn list_messages(
    state: State<'_, AppState>,
    folder_id: i64,
    limit: Option<i64>,
) -> CmdResult<Vec<MessageMeta>> {
    Ok(core(&state)
        .await?
        .db
        .list_messages(folder_id, limit.unwrap_or(200))
        .await?)
}

#[tauri::command]
pub async fn list_messages_page(
    state: State<'_, AppState>,
    folder_id: i64,
    before_date: Option<String>,
    before_id: Option<i64>,
    limit: Option<i64>,
) -> CmdResult<Vec<MessageMeta>> {
    Ok(core(&state)
        .await?
        .db
        .list_messages_page(
            folder_id,
            before_date.as_deref(),
            before_id,
            limit.unwrap_or(100),
        )
        .await?)
}

#[tauri::command]
pub async fn get_message(state: State<'_, AppState>, message_id: i64) -> CmdResult<MessageFull> {
    let core = core(&state).await?;
    // Если письмо вне кэша (raw вычищен по глубине хранения) - докачиваем с сервера.
    if let Err(error) = core.accounts.ensure_message_raw(message_id).await {
        tracing::warn!(message_id, %error, "докачка письма с сервера не удалась");
    }
    Ok(core.db.get_message(message_id).await?)
}

/// Сырой MIME-исходник письма - для окна "Исходный текст".
#[tauri::command]
pub async fn message_raw(state: State<'_, AppState>, message_id: i64) -> CmdResult<String> {
    let core = core(&state).await?;
    if let Err(error) = core.accounts.ensure_message_raw(message_id).await {
        tracing::warn!(message_id, %error, "докачка исходника письма не удалась");
    }
    Ok(core.db.message_raw(message_id).await?)
}

/// Экспортировать исходный RFC 5322/MIME без перекодирования.
#[tauri::command]
pub async fn export_message_eml(
    state: State<'_, AppState>,
    message_id: i64,
    dest_path: String,
) -> CmdResult<()> {
    let core = core(&state).await?;
    core.accounts.ensure_message_raw(message_id).await?;
    let raw = core.db.message_raw_bytes(message_id).await?;
    std::fs::write(&dest_path, raw).map_err(|error| ApiError {
        message: format!("не удалось сохранить .eml: {error}"),
    })?;
    Ok(())
}

/// Одношаговая отписка (RFC 8058) - POST на List-Unsubscribe URL.
#[tauri::command]
pub async fn unsubscribe_one_click(url: String) -> CmdResult<u16> {
    truemail_core::backend::unsubscribe_one_click(&url)
        .await
        .map_err(|error| ApiError {
            message: error.to_string(),
        })
}

/// Кнопка "Открыть" в уведомлении: показать главное окно и открыть письмо.
#[tauri::command]
pub fn notify_open(app: AppHandle, message_id: Option<i64>) -> CmdResult<()> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    if let Some(id) = message_id {
        let _ = app.emit("truemail-open-message", id);
    }
    Ok(())
}

/// Кнопка "Закрыть"/пустая очередь: спрятать окно уведомлений.
#[tauri::command]
pub fn notify_close(app: AppHandle, has_more: bool) -> CmdResult<()> {
    if !has_more {
        if let Some(window) = app.get_webview_window("notify") {
            let _ = window.hide();
            // Скрытое прозрачное окно поверх всех не должно ловить курсор.
            let _ = window.set_ignore_cursor_events(true);
        }
    }
    Ok(())
}

/// Включить/выключить запуск truemail при старте системы.
#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> CmdResult<()> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    result.map_err(|error| ApiError {
        message: format!("не удалось изменить автозапуск: {error}"),
    })
}

/// Текущее состояние автозапуска.
#[tauri::command]
pub fn get_autostart(app: AppHandle) -> CmdResult<bool> {
    use tauri_plugin_autostart::ManagerExt;
    Ok(app.autolaunch().is_enabled().unwrap_or(false))
}

#[derive(Serialize)]
pub struct AttachmentContent {
    filename: String,
    mime_type: Option<String>,
    base64: String,
}

/// Содержимое вложения в base64 - для предпросмотра (картинки, галерея).
#[tauri::command]
pub async fn attachment_content(
    state: State<'_, AppState>,
    message_id: i64,
    attachment_id: i64,
) -> CmdResult<AttachmentContent> {
    use base64::Engine as _;
    let (filename, mime_type, bytes) = core(&state)
        .await?
        .db
        .attachment_bytes(message_id, attachment_id)
        .await?;
    Ok(AttachmentContent {
        filename,
        mime_type,
        base64: base64::engine::general_purpose::STANDARD.encode(bytes),
    })
}

/// Сохранить одно вложение по абсолютному пути (путь выбирает пользователь в диалоге).
#[tauri::command]
pub async fn save_attachment(
    state: State<'_, AppState>,
    message_id: i64,
    attachment_id: i64,
    dest_path: String,
) -> CmdResult<()> {
    let (_, _, bytes) = core(&state)
        .await?
        .db
        .attachment_bytes(message_id, attachment_id)
        .await?;
    std::fs::write(&dest_path, bytes).map_err(|error| ApiError {
        message: format!("не удалось сохранить файл: {error}"),
    })?;
    Ok(())
}

/// Сохранить все вложения письма в выбранную папку. Возвращает список записанных имён.
#[tauri::command]
pub async fn save_all_attachments(
    state: State<'_, AppState>,
    message_id: i64,
    dest_dir: String,
) -> CmdResult<Vec<String>> {
    let core = core(&state).await?;
    let full = core.db.get_message(message_id).await?;
    let mut saved = Vec::new();
    for attachment in &full.attachments {
        let (filename, _, bytes) = core.db.attachment_bytes(message_id, attachment.id).await?;
        // Защита от коллизий имён: при повторе добавляем индекс.
        let mut target = std::path::Path::new(&dest_dir).join(&filename);
        let mut counter = 1;
        while target.exists() {
            let stem = std::path::Path::new(&filename)
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("attachment");
            let ext = std::path::Path::new(&filename)
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| format!(".{value}"))
                .unwrap_or_default();
            target = std::path::Path::new(&dest_dir).join(format!("{stem} ({counter}){ext}"));
            counter += 1;
        }
        std::fs::write(&target, bytes).map_err(|error| ApiError {
            message: format!("не удалось сохранить {filename}: {error}"),
        })?;
        saved.push(
            target
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or(&filename)
                .to_owned(),
        );
    }
    Ok(saved)
}

#[tauri::command]
pub async fn list_smart_folders(state: State<'_, AppState>) -> CmdResult<Vec<SmartFolder>> {
    Ok(core(&state).await?.db.list_smart_folders().await?)
}

#[tauri::command]
pub async fn save_smart_folders(
    state: State<'_, AppState>,
    folders: Vec<SmartFolder>,
) -> CmdResult<()> {
    Ok(core(&state).await?.db.save_smart_folders(&folders).await?)
}

#[tauri::command]
pub async fn list_smart_folder_messages(
    state: State<'_, AppState>,
    smart_folder_id: String,
    limit: usize,
) -> CmdResult<Vec<MessageMeta>> {
    Ok(core(&state)
        .await?
        .db
        .list_smart_folder_messages(&smart_folder_id, limit)
        .await?)
}

#[tauri::command]
pub async fn list_unified_sources(
    state: State<'_, AppState>,
) -> CmdResult<Vec<truemail_core::model::UnifiedSource>> {
    Ok(core(&state).await?.db.list_unified_sources().await?)
}

#[tauri::command]
pub async fn set_unified_source(
    state: State<'_, AppState>,
    folder_id: i64,
    included: bool,
) -> CmdResult<()> {
    Ok(core(&state)
        .await?
        .db
        .set_unified_source(folder_id, included)
        .await?)
}

#[tauri::command]
pub async fn list_mail_rules(state: State<'_, AppState>) -> CmdResult<Vec<MailRule>> {
    Ok(core(&state).await?.db.list_mail_rules().await?)
}

#[tauri::command]
pub async fn save_mail_rule(
    state: State<'_, AppState>,
    rule: MailRuleInput,
    apply_existing: bool,
) -> CmdResult<MailRule> {
    let core = core(&state).await?;
    let saved = core.db.save_mail_rule(&rule, apply_existing).await?;
    if saved.enabled {
        core.db.process_mail_rules().await?;
    }
    Ok(saved)
}

#[tauri::command]
pub async fn set_mail_rule_enabled(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> CmdResult<()> {
    let core = core(&state).await?;
    core.db.set_mail_rule_enabled(&id, enabled).await?;
    if enabled {
        core.db.process_mail_rules().await?;
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_mail_rule(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    Ok(core(&state).await?.db.delete_mail_rule(&id).await?)
}

#[tauri::command]
pub async fn list_contacts(
    state: State<'_, AppState>,
    query: Option<String>,
) -> CmdResult<Vec<Contact>> {
    Ok(core(&state)
        .await?
        .db
        .list_contacts(query.as_deref())
        .await?)
}

#[tauri::command]
pub async fn search(state: State<'_, AppState>, query: String) -> CmdResult<Vec<MessageMeta>> {
    let core = core(&state).await?;
    let mut from_filter = None;
    let mut attachments_only = false;
    let mut words = Vec::new();
    for token in query.split_whitespace() {
        let lower = token.to_lowercase();
        if let Some(value) = lower
            .strip_prefix("from:")
            .filter(|value| !value.is_empty())
        {
            from_filter = Some(value.to_owned());
        } else if matches!(
            token.to_ascii_lowercase().as_str(),
            "has:attachment" | "has:attachments"
        ) {
            attachments_only = true;
        } else {
            words.push(token);
        }
    }
    let text_query = words.join(" ");
    let mut ids = Vec::new();
    let variants = truemail_core::search::layout_variants(&text_query);
    for variant in &variants {
        if let Some(fts_query) = truemail_core::search::prefix_query(variant) {
            for id in core.search.search(&fts_query, 50).await? {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
    }
    // Коррекцию опечатки включаем только когда точный префикс во всех
    // раскладках ничего не нашёл, иначе она слишком расширяет нормальный поиск.
    if ids.is_empty() {
        for variant in &variants {
            for fts_query in truemail_core::search::typo_prefix_queries(variant)
                .into_iter()
                .skip(1)
            {
                for id in core.search.search(&fts_query, 50).await? {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
            }
        }
    }
    let mut messages = if text_query.trim().is_empty() {
        core.db.list_recent_messages(100).await?
    } else {
        core.db.list_messages_by_ids(&ids).await?
    };
    messages.retain(|message| {
        (!attachments_only || message.has_attachments)
            && from_filter.as_ref().is_none_or(|filter| {
                format!(
                    "{} {}",
                    message.from.name.as_deref().unwrap_or_default(),
                    message.from.email
                )
                .to_lowercase()
                .contains(filter)
            })
    });
    Ok(messages)
}

#[tauri::command]
pub async fn list_calendar_data(state: State<'_, AppState>) -> CmdResult<CalendarData> {
    let (calendars, events) = core(&state).await?.db.list_calendars_and_events().await?;
    Ok(CalendarData { calendars, events })
}

#[tauri::command]
pub async fn set_calendar_visible(
    state: State<'_, AppState>,
    calendar_id: i64,
    visible: bool,
) -> CmdResult<()> {
    Ok(core(&state)
        .await?
        .db
        .set_calendar_visible(calendar_id, visible)
        .await?)
}

async fn account_by_id(core: &Core, account_id: i64) -> CmdResult<Account> {
    core.db
        .list_accounts()
        .await?
        .into_iter()
        .find(|account| account.id == account_id)
        .ok_or_else(|| ApiError {
            message: "Аккаунт не найден".into(),
        })
}

async fn refresh_auxiliary(core: &Core, account: &Account) -> CmdResult<()> {
    match account.provider {
        truemail_core::model::Provider::Yandex => {
            core.accounts.sync_yandex_dav_account(account).await?;
        }
        truemail_core::model::Provider::Gmail => {
            core.accounts.sync_google_auxiliary_account(account).await?;
        }
        truemail_core::model::Provider::Exchange => {
            core.accounts
                .sync_exchange_auxiliary_account(account)
                .await?;
        }
        _ => {
            return Err(ApiError {
                message: "Календарь и контакты этого провайдера пока не поддерживаются".into(),
            });
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn create_event(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: i64,
    calendar_id: i64,
    input: EventInput,
) -> CmdResult<()> {
    let core = core(&state).await?;
    let account = account_by_id(&core, account_id).await?;
    let calendar: (i64, String) =
        sqlx::query_as("SELECT account_id, url FROM calendars WHERE id=?")
            .bind(calendar_id)
            .fetch_one(&core.db.pool)
            .await
            .map_err(truemail_core::Error::from)?;
    if calendar.0 != account_id {
        return Err(ApiError {
            message: "Календарь принадлежит другому аккаунту".into(),
        });
    }
    let token = core.accounts.oauth_access_token(&account).await?;
    truemail_core::account::write_event(
        account.provider,
        &account.email,
        &token,
        &calendar.1,
        RemoteObject {
            uid: None,
            remote_url: None,
            etag: None,
        },
        &input,
    )
    .await?;
    refresh_auxiliary(&core, &account).await?;
    let _ = app.emit("truemail-data-changed", account_id);
    Ok(())
}

#[tauri::command]
pub async fn update_event(
    app: AppHandle,
    state: State<'_, AppState>,
    event_id: i64,
    input: EventInput,
) -> CmdResult<()> {
    let core = core(&state).await?;
    let row: (i64, String, Option<String>, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT c.account_id, c.url, e.uid, e.remote_url, e.etag
         FROM events e JOIN calendars c ON c.id=e.calendar_id WHERE e.id=?",
    )
    .bind(event_id)
    .fetch_one(&core.db.pool)
    .await
    .map_err(truemail_core::Error::from)?;
    let account = account_by_id(&core, row.0).await?;
    let token = core.accounts.oauth_access_token(&account).await?;
    truemail_core::account::write_event(
        account.provider,
        &account.email,
        &token,
        &row.1,
        RemoteObject {
            uid: row.2.as_deref(),
            remote_url: row.3.as_deref(),
            etag: row.4.as_deref(),
        },
        &input,
    )
    .await?;
    refresh_auxiliary(&core, &account).await?;
    let _ = app.emit("truemail-data-changed", account.id);
    Ok(())
}

#[tauri::command]
pub async fn delete_event(
    app: AppHandle,
    state: State<'_, AppState>,
    event_id: i64,
) -> CmdResult<()> {
    let core = core(&state).await?;
    let row: (i64, String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT c.account_id, c.url, e.remote_url, e.etag
         FROM events e JOIN calendars c ON c.id=e.calendar_id WHERE e.id=?",
    )
    .bind(event_id)
    .fetch_one(&core.db.pool)
    .await
    .map_err(truemail_core::Error::from)?;
    let account = account_by_id(&core, row.0).await?;
    let remote_url = row.2.as_deref().ok_or_else(|| ApiError {
        message: "У события нет серверного идентификатора".into(),
    })?;
    let token = core.accounts.oauth_access_token(&account).await?;
    truemail_core::account::delete_event(
        account.provider,
        &account.email,
        &token,
        &row.1,
        remote_url,
        row.3.as_deref(),
    )
    .await?;
    refresh_auxiliary(&core, &account).await?;
    let _ = app.emit("truemail-data-changed", account.id);
    Ok(())
}

#[tauri::command]
pub async fn create_contact(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: i64,
    input: ContactInput,
) -> CmdResult<()> {
    let core = core(&state).await?;
    let account = account_by_id(&core, account_id).await?;
    if !matches!(account.provider, Provider::Gmail | Provider::Yandex) {
        core.db.save_local_contact(account_id, None, &input).await?;
        let _ = app.emit("truemail-data-changed", account_id);
        return Ok(());
    }
    let stored_collection: Option<(String,)> = sqlx::query_as(
        "SELECT url FROM auxiliary_collections WHERE account_id=? AND kind='carddav' LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(&core.db.pool)
    .await
    .map_err(truemail_core::Error::from)?;
    let collection = if let Some(row) = stored_collection {
        Some(row.0)
    } else {
        // Совместимость с базой, которая ещё не успела пройти новую DAV-синхронизацию.
        let remote_sample: Option<(String,)> = sqlx::query_as(
            "SELECT remote_url FROM contacts WHERE account_id=? AND remote_url LIKE 'http%' LIMIT 1",
        )
        .bind(account_id)
        .fetch_optional(&core.db.pool)
        .await
        .map_err(truemail_core::Error::from)?;
        remote_sample
            .as_ref()
            .and_then(|row| url::Url::parse(&row.0).ok())
            .and_then(|url| url.join(".").ok())
            .map(String::from)
    };
    let token = core.accounts.oauth_access_token(&account).await?;
    truemail_core::account::write_contact(
        account.provider,
        &account.email,
        &token,
        collection.as_deref(),
        RemoteObject {
            uid: None,
            remote_url: None,
            etag: None,
        },
        &input,
    )
    .await?;
    refresh_auxiliary(&core, &account).await?;
    let _ = app.emit("truemail-data-changed", account_id);
    Ok(())
}

#[tauri::command]
pub async fn update_contact(
    app: AppHandle,
    state: State<'_, AppState>,
    contact_id: i64,
    input: ContactInput,
) -> CmdResult<()> {
    let core = core(&state).await?;
    let row: (i64, Option<String>, Option<String>, Option<String>) =
        sqlx::query_as("SELECT account_id, uid, remote_url, etag FROM contacts WHERE id=?")
            .bind(contact_id)
            .fetch_one(&core.db.pool)
            .await
            .map_err(truemail_core::Error::from)?;
    let account = account_by_id(&core, row.0).await?;
    if row.2.is_none() {
        core.db
            .save_local_contact(account.id, Some(contact_id), &input)
            .await?;
        let _ = app.emit("truemail-data-changed", account.id);
        return Ok(());
    }
    let token = core.accounts.oauth_access_token(&account).await?;
    truemail_core::account::write_contact(
        account.provider,
        &account.email,
        &token,
        None,
        RemoteObject {
            uid: row.1.as_deref(),
            remote_url: row.2.as_deref(),
            etag: row.3.as_deref(),
        },
        &input,
    )
    .await?;
    refresh_auxiliary(&core, &account).await?;
    let _ = app.emit("truemail-data-changed", account.id);
    Ok(())
}

#[tauri::command]
pub async fn delete_contact(
    app: AppHandle,
    state: State<'_, AppState>,
    contact_id: i64,
) -> CmdResult<()> {
    let core = core(&state).await?;
    let row: (i64, Option<String>, Option<String>) =
        sqlx::query_as("SELECT account_id, remote_url, etag FROM contacts WHERE id=?")
            .bind(contact_id)
            .fetch_one(&core.db.pool)
            .await
            .map_err(truemail_core::Error::from)?;
    let account = account_by_id(&core, row.0).await?;
    let Some(remote_url) = row.1.as_deref() else {
        core.db.hide_local_contact(contact_id).await?;
        let _ = app.emit("truemail-data-changed", account.id);
        return Ok(());
    };
    let token = core.accounts.oauth_access_token(&account).await?;
    truemail_core::account::delete_contact(
        account.provider,
        &account.email,
        &token,
        remote_url,
        row.2.as_deref(),
    )
    .await?;
    refresh_auxiliary(&core, &account).await?;
    let _ = app.emit("truemail-data-changed", account.id);
    Ok(())
}

fn dir_size(path: &std::path::Path) -> u64 {
    std::fs::read_dir(path)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                dir_size(&path)
            } else {
                entry.metadata().map(|m| m.len()).unwrap_or(0)
            }
        })
        .sum()
}

#[tauri::command]
pub async fn storage_status(state: State<'_, AppState>) -> CmdResult<StorageStatus> {
    let _ = core(&state).await?;
    let data_dir = truemail_core::crypto::load_data_dir()?.unwrap_or_else(super::default_data_dir);
    let database_bytes = ["truemail.db", "truemail.db-wal", "truemail.db-shm"]
        .iter()
        .map(|name| {
            std::fs::metadata(data_dir.join(name))
                .map(|m| m.len())
                .unwrap_or(0)
        })
        .sum();
    let blob_bytes = dir_size(&data_dir.join("blobs"));
    Ok(StorageStatus {
        data_dir: data_dir.to_string_lossy().into_owned(),
        total_bytes: database_bytes + blob_bytes,
        database_bytes,
        blob_bytes,
    })
}

fn copy_dir(source: &std::path::Path, target: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(from, to)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn move_storage(
    app: AppHandle,
    state: State<'_, AppState>,
    target: String,
) -> CmdResult<()> {
    let source = truemail_core::crypto::load_data_dir()?.unwrap_or_else(super::default_data_dir);
    let target = PathBuf::from(target.trim());
    if !target.is_absolute() || target == source {
        return Err(ApiError {
            message: "Выберите другую полную папку".into(),
        });
    }
    std::fs::create_dir_all(&target).map_err(|e| ApiError {
        message: format!("Не удалось создать папку: {e}"),
    })?;
    if target.join("truemail.db").exists() || target.join("blobs").exists() {
        return Err(ApiError {
            message: "В выбранной папке уже есть данные truemail".into(),
        });
    }
    // Не даём фоновой синхронизации начать запись между checkpoint и
    // переключением Core. Активную синхронизацию пользователь может повторить
    // после её завершения, без риска получить неполную копию.
    let sync_guard = state.syncing.lock().await;
    if !sync_guard.is_empty() {
        return Err(ApiError {
            message: "Дождитесь окончания текущей синхронизации и повторите перенос".into(),
        });
    }
    let old_core = state.core.write().await.take().ok_or_else(|| ApiError {
        message: "Хранилище ещё не создано".into(),
    })?;
    // Порядок важен. wal_checkpoint(TRUNCATE) требует, чтобы читателей не было:
    // сначала закрываем пул чтения, только потом сливаем WAL в основной файл.
    old_core.db.pool.close().await;
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&old_core.db.write_pool)
        .await
        .map_err(truemail_core::Error::from)?;
    // Файл БД копируется дальше: writer тоже должен отпустить его, иначе на
    // Windows копирование упрётся в удерживаемый соединением файл.
    old_core.db.write_pool.close().await;
    let copy_result = std::fs::copy(source.join("truemail.db"), target.join("truemail.db"))
        .map(|_| ())
        .map_err(|e| ApiError {
            message: format!("Не удалось скопировать базу: {e}"),
        })
        .and_then(|_| {
            if source.join("blobs").exists() {
                copy_dir(&source.join("blobs"), &target.join("blobs")).map_err(|e| ApiError {
                    message: format!("Не удалось скопировать вложения: {e}"),
                })
            } else {
                Ok(())
            }
        });
    if let Err(error) = copy_result {
        let restored = Core::bootstrap(source.clone())
            .await
            .map_err(|restore| ApiError {
                message: format!(
                    "{}; исходное хранилище не открылось: {restore}",
                    error.message
                ),
            })?;
        *state.core.write().await = Some(Arc::new(restored));
        return Err(error);
    }
    let replacement = match Core::bootstrap(target.clone()).await {
        Ok(core) => Arc::new(core),
        Err(error) => {
            // Источник не удалялся: восстанавливаем рабочее ядро на прежнем
            // пути и возвращаем понятную ошибку вместо полурабочего состояния.
            let restored = Core::bootstrap(source.clone()).await.map_err(|restore| ApiError {
                message: format!(
                    "Копия не открылась: {error}. Исходное хранилище также не удалось открыть: {restore}"
                ),
            })?;
            *state.core.write().await = Some(Arc::new(restored));
            return Err(ApiError {
                message: format!("Копия не прошла проверку: {error}"),
            });
        }
    };
    if let Err(error) = truemail_core::crypto::store_data_dir(&target) {
        let restored = Core::bootstrap(source.clone())
            .await
            .map_err(|restore| ApiError {
                message: format!(
                    "Не удалось сохранить новый путь: {error}; восстановление: {restore}"
                ),
            })?;
        *state.core.write().await = Some(Arc::new(restored));
        return Err(error.into());
    }
    *state.core.write().await = Some(replacement);
    state
        .generation
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    state.watching.lock().await.clear();
    drop(sync_guard);
    let _ = app.emit(
        "truemail-storage-moved",
        target.to_string_lossy().into_owned(),
    );
    Ok(())
}

#[tauri::command]
pub async fn open_data_dir(state: State<'_, AppState>) -> CmdResult<()> {
    let _ = core(&state).await?;
    let path = truemail_core::crypto::load_data_dir()?.unwrap_or_else(super::default_data_dir);
    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer.exe")
        .arg(&path)
        .spawn()
        .map_err(|e| ApiError {
            message: format!("Не удалось открыть проводник: {e}"),
        })?;
    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg(&path)
        .spawn()
        .map_err(|e| ApiError {
            message: format!("Не удалось открыть Finder: {e}"),
        })?;
    #[cfg(all(unix, not(target_os = "macos")))]
    std::process::Command::new("xdg-open")
        .arg(&path)
        .spawn()
        .map_err(|e| ApiError {
            message: format!("Не удалось открыть папку: {e}"),
        })?;
    Ok(())
}

#[tauri::command]
pub async fn clear_local_data(state: State<'_, AppState>, scope: String) -> CmdResult<()> {
    let core = core(&state).await?;
    match scope.as_str() {
        "trash_spam" => {
            sqlx::query("DELETE FROM messages WHERE folder_id IN (SELECT id FROM folders WHERE role IN ('trash','spam'))").execute(&core.db.write_pool).await.map_err(truemail_core::Error::from)?;
        }
        "all" => {
            let mut tx = core.db.begin_write().await?;
            sqlx::query("DELETE FROM outbox_ops")
                .execute(&mut *tx)
                .await
                .map_err(truemail_core::Error::from)?;
            sqlx::query("DELETE FROM messages")
                .execute(&mut *tx)
                .await
                .map_err(truemail_core::Error::from)?;
            sqlx::query("DELETE FROM attachments")
                .execute(&mut *tx)
                .await
                .map_err(truemail_core::Error::from)?;
            sqlx::query("DELETE FROM contacts")
                .execute(&mut *tx)
                .await
                .map_err(truemail_core::Error::from)?;
            sqlx::query("DELETE FROM calendars")
                .execute(&mut *tx)
                .await
                .map_err(truemail_core::Error::from)?;
            sqlx::query("DELETE FROM folders")
                .execute(&mut *tx)
                .await
                .map_err(truemail_core::Error::from)?;
            tx.commit().await.map_err(truemail_core::Error::from)?;
            core.db.blobs.clear()?;
        }
        "old_attachments" => {
            sqlx::query("DELETE FROM attachments WHERE message_id IN (SELECT id FROM messages WHERE date < datetime('now','-1 year'))").execute(&core.db.write_pool).await.map_err(truemail_core::Error::from)?;
        }
        _ => {
            return Err(ApiError {
                message: "Неизвестный режим очистки".into(),
            });
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn sync_accounts(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let core = core(&state).await?;
    for account in core.db.list_accounts().await? {
        if !account.enabled {
            continue;
        }
        let mut syncing = state.syncing.lock().await;
        if !syncing.insert(account.id) {
            continue;
        }
        drop(syncing);
        let sync_core = core.clone();
        let sync_set = state.syncing.clone();
        let sync_app = app.clone();
        let _ = app.emit(
            "truemail-sync-state",
            serde_json::json!({"account_id": account.id, "scope": "all", "status": "syncing"}),
        );
        tokio::spawn(async move {
            tracing::info!(account = %account.email, provider = ?account.provider, "mail-sync начат");
            if account.provider == truemail_core::model::Provider::Exchange {
                match sync_core.accounts.sync_mail_inbox(&account).await {
                    Ok(messages) => {
                        tracing::info!(account = %account.email, messages, "Exchange: свежие входящие загружены");
                        let _ = sync_app.emit("truemail-data-changed", account.id);
                    }
                    Err(error) => {
                        tracing::warn!(account = %account.email, %error, "Exchange: быстрые входящие не загрузились");
                    }
                }
            }
            let state = match sync_core.accounts.sync_mail_account(&account).await {
                Ok(result) => {
                    tracing::info!(account = %account.email, "mail-sync завершён");
                    serde_json::json!({"account_id": account.id, "scope": "all", "status": "ready", "warnings": result.warnings})
                }
                Err(error) => {
                    tracing::error!(account = %account.email, %error, "фоновая синхронизация не удалась");
                    serde_json::json!({"account_id": account.id, "scope": "all", "status": "error", "error": error.to_string()})
                }
            };
            sync_set.lock().await.remove(&account.id);
            let _ = sync_app.emit("truemail-sync-state", state);
            let _ = sync_app.emit("truemail-data-changed", account.id);
        });
    }
    Ok(())
}

/// Периодически обновляет календари, контакты и задачи отдельно от почты.
#[tauri::command]
pub async fn sync_auxiliary_accounts(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let core = core(&state).await?;
    for account in core.db.list_accounts().await? {
        if !matches!(
            account.provider,
            truemail_core::model::Provider::Yandex
                | truemail_core::model::Provider::Gmail
                | truemail_core::model::Provider::Exchange
        ) || !account.enabled
        {
            continue;
        }
        let mut syncing = state.syncing_aux.lock().await;
        if !syncing.insert(account.id) {
            continue;
        }
        drop(syncing);
        let sync_core = core.clone();
        let sync_set = state.syncing_aux.clone();
        let sync_app = app.clone();
        let _ = app.emit(
            "truemail-sync-state",
            serde_json::json!({"account_id": account.id, "scope": "auxiliary", "status": "syncing"}),
        );
        tokio::spawn(async move {
            tracing::info!(account = %account.email, provider = ?account.provider, "aux-sync начат");
            let sync_result = sync_core.accounts.sync_auxiliary_account(&account).await;
            let state = match sync_result {
                Ok((calendars, events, contacts)) => {
                    tracing::info!(account = %account.email, calendars, events, contacts, "календари, задачи и контакты обновлены");
                    serde_json::json!({"account_id": account.id, "scope": "auxiliary", "status": "ready", "calendars": calendars, "events": events, "contacts": contacts})
                }
                Err(error) => {
                    tracing::error!(account = %account.email, %error, "синхронизация календаря, задач и контактов не удалась");
                    serde_json::json!({"account_id": account.id, "scope": "auxiliary", "status": "error", "error": error.to_string()})
                }
            };
            sync_set.lock().await.remove(&account.id);
            let _ = sync_app.emit("truemail-sync-state", state);
            let _ = sync_app.emit("truemail-data-changed", account.id);
        });
    }
    Ok(())
}

/// Запускает по одному постоянному IMAP IDLE-наблюдателю на аккаунт.
#[tauri::command]
pub async fn start_realtime(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let core = core(&state).await?;
    // Единый фоновый цикл напоминаний о встречах (не зависит от аккаунтов почты).
    if !state
        .reminders_started
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        let reminder_core = core.clone();
        let reminder_app = app.clone();
        tokio::spawn(async move { reminders_loop(reminder_core, reminder_app).await });
        // Очистка кэша по глубине хранения - один раз при старте сессии.
        let prune_core = core.clone();
        tokio::spawn(async move {
            let _ = prune_core.accounts.prune_all_caches_on_start().await;
        });
        // Почти реалтайм-уведомления о новых письмах Gmail без внешнего
        // Cloud Pub/Sub-сервера: лёгкий polling ID Входящих каждые ~25 секунд.
        let gmail_core = core.clone();
        let gmail_app = app.clone();
        let gmail_syncing = state.syncing.clone();
        tokio::spawn(
            async move { gmail_realtime_loop(gmail_core, gmail_app, gmail_syncing).await },
        );
    }
    for account in core.db.list_accounts().await? {
        if !account.enabled {
            continue;
        }
        let mut watching = state.watching.lock().await;
        if !watching.insert(account.id) {
            continue;
        }
        drop(watching);

        // Gmail работает через отдельный лёгкий REST polling. Для остальных
        // транспорт сам выбирает IDLE, EWS/JMAP polling или иной механизм ожидания.
        if !matches!(
            account.provider,
            truemail_core::model::Provider::Gmail | truemail_core::model::Provider::Exchange
        ) {
            let watch_core = core.clone();
            let watch_syncing = state.syncing.clone();
            let watch_app = app.clone();
            let watch_account = account.clone();
            let watch_generation = state.generation.clone();
            let generation = watch_generation.load(std::sync::atomic::Ordering::SeqCst);
            tokio::spawn(async move {
                let mut retry_delay = std::time::Duration::from_secs(2);
                loop {
                    if watch_generation.load(std::sync::atomic::Ordering::SeqCst) != generation {
                        break;
                    }
                    let wait = watch_core
                        .accounts
                        .wait_for_mail_change(&watch_account)
                        .await;
                    match wait {
                        Ok(()) => {
                            let _ = watch_app.emit("truemail-sync-state", serde_json::json!({"account_id": watch_account.id, "scope": "mail", "status": "syncing"}));
                            retry_delay = std::time::Duration::from_secs(2);
                            loop {
                                let mut syncing = watch_syncing.lock().await;
                                if syncing.insert(watch_account.id) {
                                    break;
                                }
                                drop(syncing);
                                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                            }
                            match watch_core.accounts.sync_mail_inbox(&watch_account).await {
                                // Плановая переустановка IDLE без новых писем (messages=0)
                                // происходит каждые ~90с - это debug, чтобы не шуметь.
                                // Реальные новые письма логируем на info.
                                Ok(messages) if messages > 0 => {
                                    tracing::info!(
                                        account = %watch_account.email,
                                        messages,
                                        "почтовый транспорт: входящие обновлены"
                                    );
                                    notify_new_mail(
                                        &watch_app,
                                        &watch_core,
                                        &watch_account,
                                        messages,
                                    )
                                    .await;
                                }
                                Ok(messages) => tracing::debug!(
                                    account = %watch_account.email,
                                    messages,
                                    "наблюдение переустановлено, новых писем нет"
                                ),
                                Err(error) => tracing::error!(
                                    account = %watch_account.email,
                                    %error,
                                    "не удалось дозагрузить входящие"
                                ),
                            }
                            watch_syncing.lock().await.remove(&watch_account.id);
                            let _ = watch_app.emit("truemail-sync-state", serde_json::json!({"account_id": watch_account.id, "scope": "mail", "status": "ready"}));
                            let _ = watch_app.emit("truemail-data-changed", watch_account.id);
                        }
                        Err(error) => {
                            // Разрыв простаивающего IDLE сервером/NAT (10054, close_notify,
                            // unexpected eof, connection reset) - ожидаемое поведение, а не
                            // сбой: логируем на debug, чтобы не пугать в логе. Остальные
                            // ошибки (авторизация, TLS-хендшейк и т.п.) остаются на warn.
                            let text = error.to_string();
                            let routine = text.contains("10054")
                                || text.contains("close_notify")
                                || text.contains("unexpected eof")
                                || text.contains("reset")
                                || text.contains("принудительно разорвал");
                            if routine {
                                tracing::debug!(account = %watch_account.email, %error, "наблюдение за почтой переустанавливается");
                            } else {
                                tracing::warn!(account = %watch_account.email, %error, "наблюдение за почтой будет восстановлено");
                            }
                            let _ = watch_app.emit("truemail-sync-state", serde_json::json!({"account_id": watch_account.id, "scope": "mail", "status": "retrying"}));
                            tokio::time::sleep(retry_delay).await;
                            retry_delay = (retry_delay * 2).min(std::time::Duration::from_secs(60));
                        }
                    }
                }
            });
        }

        let outbox_core = core.clone();
        let outbox_account = account.clone();
        let outbox_app = app.clone();
        let outbox_generation = state.generation.clone();
        let generation = outbox_generation.load(std::sync::atomic::Ordering::SeqCst);
        tokio::spawn(async move {
            loop {
                if outbox_generation.load(std::sync::atomic::Ordering::SeqCst) != generation {
                    break;
                }
                match outbox_core
                    .accounts
                    .process_mail_outbox(&outbox_account)
                    .await
                {
                    Ok(count) if count > 0 => {
                        let _ = outbox_app.emit("truemail-data-changed", outbox_account.id);
                    }
                    Ok(_) => {}
                    Err(error) => tracing::warn!(
                        account = %outbox_account.email,
                        %error,
                        "outbox временно недоступен"
                    ),
                }
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            }
        });
    }
    Ok(())
}

#[tauri::command]
pub async fn send_message(
    app: AppHandle,
    state: State<'_, AppState>,
    request: SendMessageRequest,
) -> CmdResult<()> {
    let core = core(&state).await?;
    let account = core
        .db
        .list_accounts()
        .await?
        .into_iter()
        .find(|account| account.id == request.account_id)
        .ok_or_else(|| ApiError {
            message: "Аккаунт отправителя не найден".into(),
        })?;
    let outgoing = outgoing_message(&account, request);
    core.accounts.send_outgoing(account.id, outgoing).await?;
    let _ = app.emit("truemail-data-changed", account.id);
    Ok(())
}

fn outgoing_message(
    account: &Account,
    request: SendMessageRequest,
) -> truemail_core::backend::OutgoingMessage {
    truemail_core::backend::OutgoingMessage {
        from: account.email.clone(),
        to: request.to,
        cc: request.cc,
        bcc: request.bcc,
        subject: request.subject,
        body_text: request.body_text,
        body_html: request.body_html,
        attachments: request
            .attachments
            .into_iter()
            .map(|item| truemail_core::backend::OutgoingAttachment {
                filename: item.filename,
                mime_type: item.mime_type,
                data: item.data,
            })
            .collect(),
    }
}

#[tauri::command]
pub async fn schedule_message(
    state: State<'_, AppState>,
    request: SendMessageRequest,
    send_at: String,
) -> CmdResult<i64> {
    let core = core(&state).await?;
    let account = core
        .db
        .list_accounts()
        .await?
        .into_iter()
        .find(|account| account.id == request.account_id)
        .ok_or_else(|| ApiError {
            message: "Аккаунт отправителя не найден".into(),
        })?;
    let send_at = chrono::DateTime::parse_from_rfc3339(&send_at).map_err(|_| ApiError {
        message: "Некорректная дата отложенной отправки".into(),
    })?;
    if send_at <= chrono::Utc::now() + chrono::Duration::seconds(5) {
        return Err(ApiError {
            message: "Выберите время в будущем".into(),
        });
    }
    let outgoing = outgoing_message(&account, request);
    let payload = serde_json::to_string(&outgoing).map_err(truemail_core::Error::from)?;
    Ok(core
        .db
        .queue_scheduled_send(
            account.id,
            &payload,
            &send_at
                .with_timezone(&chrono::Utc)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
        )
        .await?)
}

#[tauri::command]
pub async fn mark_seen(state: State<'_, AppState>, message_id: i64, seen: bool) -> CmdResult<()> {
    Ok(core(&state).await?.db.mark_seen(message_id, seen).await?)
}

#[tauri::command]
pub async fn snooze_messages(
    state: State<'_, AppState>,
    message_ids: Vec<i64>,
    until: String,
) -> CmdResult<usize> {
    if message_ids.is_empty() {
        return Err(ApiError {
            message: "Не выбрано ни одного письма".into(),
        });
    }
    let until = chrono::DateTime::parse_from_rfc3339(&until).map_err(|error| ApiError {
        message: format!("неверная дата пробуждения: {error}"),
    })?;
    if until <= chrono::Utc::now() {
        return Err(ApiError {
            message: "время пробуждения должно быть в будущем".into(),
        });
    }
    let until = until
        .with_timezone(&chrono::Utc)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    Ok(core(&state)
        .await?
        .db
        .set_messages_snoozed(&message_ids, Some(&until))
        .await?)
}

#[tauri::command]
pub async fn unsnooze_messages(
    state: State<'_, AppState>,
    message_ids: Vec<i64>,
) -> CmdResult<usize> {
    Ok(core(&state)
        .await?
        .db
        .set_messages_snoozed(&message_ids, None)
        .await?)
}

#[tauri::command]
pub async fn release_due_snoozes(state: State<'_, AppState>) -> CmdResult<usize> {
    Ok(core(&state).await?.db.release_due_snoozes().await?)
}

#[tauri::command]
pub async fn list_signatures(
    state: State<'_, AppState>,
    account_id: i64,
) -> CmdResult<Vec<Signature>> {
    Ok(core(&state).await?.db.list_signatures(account_id).await?)
}

#[tauri::command]
pub async fn save_signature(
    state: State<'_, AppState>,
    account_id: i64,
    kind: String,
    body_html: String,
    enabled: bool,
) -> CmdResult<()> {
    Ok(core(&state)
        .await?
        .db
        .upsert_signature(account_id, &kind, &body_html, enabled)
        .await?)
}

#[tauri::command]
pub async fn list_message_templates(
    state: State<'_, AppState>,
    account_id: i64,
) -> CmdResult<Vec<MessageTemplate>> {
    Ok(core(&state)
        .await?
        .db
        .list_message_templates(account_id)
        .await?)
}

#[tauri::command]
pub async fn save_message_template(
    state: State<'_, AppState>,
    id: Option<i64>,
    account_id: i64,
    name: String,
    subject: String,
    body_html: String,
) -> CmdResult<i64> {
    Ok(core(&state)
        .await?
        .db
        .save_message_template(id, account_id, &name, &subject, &body_html)
        .await?)
}

#[tauri::command]
pub async fn delete_message_template(
    state: State<'_, AppState>,
    id: i64,
    account_id: i64,
) -> CmdResult<bool> {
    Ok(core(&state)
        .await?
        .db
        .delete_message_template(id, account_id)
        .await?)
}

#[tauri::command]
pub async fn message_action(
    state: State<'_, AppState>,
    message_ids: Vec<i64>,
    action: String,
) -> CmdResult<truemail_core::storage::repo::QueuedAction> {
    if message_ids.is_empty() {
        return Err(ApiError {
            message: "Не выбрано ни одного письма".into(),
        });
    }
    let role = match action.as_str() {
        "archive" => "archive",
        "trash" => "trash",
        "spam" => "spam",
        _ => {
            return Err(ApiError {
                message: "Неизвестное действие с письмом".into(),
            });
        }
    };
    Ok(core(&state)
        .await?
        .db
        .queue_message_action(&message_ids, role)
        .await?)
}

#[tauri::command]
pub async fn move_messages_to_folder(
    state: State<'_, AppState>,
    message_ids: Vec<i64>,
    folder_id: i64,
) -> CmdResult<truemail_core::storage::repo::QueuedAction> {
    if message_ids.is_empty() {
        return Err(ApiError {
            message: "Не выбрано ни одного письма".into(),
        });
    }
    Ok(core(&state)
        .await?
        .db
        .queue_message_move(&message_ids, folder_id)
        .await?)
}

#[tauri::command]
pub async fn undo_message_action(
    state: State<'_, AppState>,
    operation_ids: Vec<i64>,
) -> CmdResult<usize> {
    Ok(core(&state)
        .await?
        .db
        .cancel_outbox_operations(&operation_ids)
        .await?)
}

#[tauri::command]
pub async fn get_setting(state: State<'_, AppState>, key: String) -> CmdResult<Option<String>> {
    Ok(core(&state).await?.db.setting(&key).await?)
}

/// Все настройки разом: UI восстанавливает из них состояние при старте.
#[tauri::command]
pub async fn all_settings(state: State<'_, AppState>) -> CmdResult<HashMap<String, String>> {
    Ok(core(&state).await?.db.all_settings().await?)
}

#[tauri::command]
pub async fn set_setting(state: State<'_, AppState>, key: String, value: String) -> CmdResult<()> {
    Ok(core(&state).await?.db.set_setting(&key, &value).await?)
}

#[tauri::command]
pub async fn list_keybindings(state: State<'_, AppState>) -> CmdResult<Vec<Keybinding>> {
    Ok(core(&state).await?.db.list_keybindings().await?)
}

#[tauri::command]
pub async fn set_keybinding(
    app: AppHandle,
    state: State<'_, AppState>,
    action: String,
    combo: String,
) -> CmdResult<()> {
    let combo = combo.trim();
    if combo.is_empty() {
        return Err(ApiError {
            message: "сочетание клавиш не может быть пустым".into(),
        });
    }
    let core = core(&state).await?;
    let previous = core.db.list_keybindings().await?;
    let mut updated = previous.clone();
    let binding = updated
        .iter_mut()
        .find(|binding| binding.action == action)
        .ok_or_else(|| ApiError {
            message: "неизвестное действие клавиатуры".into(),
        })?;
    if binding.scope == "global" {
        Shortcut::from_str(combo).map_err(|error| ApiError {
            message: format!("неверное сочетание клавиш: {error}"),
        })?;
    }
    binding.combo = combo.to_owned();
    let mut seen = HashSet::new();
    if updated
        .iter()
        .any(|binding| !seen.insert(binding.combo.to_ascii_lowercase()))
    {
        return Err(ApiError {
            message: "это сочетание уже назначено другому действию".into(),
        });
    }
    if let Err(error) = register_global_shortcuts(&app, &updated) {
        let _ = register_global_shortcuts(&app, &previous);
        return Err(ApiError {
            message: format!("не удалось зарегистрировать сочетание: {error}"),
        });
    }
    if let Err(error) = core.db.set_keybinding(&action, combo).await {
        let _ = register_global_shortcuts(&app, &previous);
        return Err(error.into());
    }
    Ok(())
}

#[tauri::command]
pub async fn image_sender_trusted(state: State<'_, AppState>, sender: String) -> CmdResult<bool> {
    Ok(core(&state).await?.db.image_sender_trusted(&sender).await?)
}

#[tauri::command]
pub async fn set_image_sender_trusted(
    state: State<'_, AppState>,
    sender: String,
    allow: bool,
) -> CmdResult<()> {
    Ok(core(&state)
        .await?
        .db
        .set_image_sender_trusted(&sender, allow)
        .await?)
}

fn yandex_client_id() -> CmdResult<String> {
    configured_yandex_client_id().ok_or_else(|| ApiError {
        message: "Подключение к Яндексу пока не настроено в этой сборке truemail.".into(),
    })
}

fn google_client_credentials() -> CmdResult<(String, String)> {
    let client_id = configured_google_client_id().ok_or_else(|| ApiError {
        message: "Gmail OAuth не настроен в этой сборке: не задан TRUEMAIL_GOOGLE_CLIENT_ID."
            .into(),
    })?;
    let client_secret = configured_google_client_secret().ok_or_else(|| ApiError {
        message: "Gmail OAuth не настроен в этой сборке: не задан TRUEMAIL_GOOGLE_CLIENT_SECRET."
            .into(),
    })?;
    Ok((client_id, client_secret))
}

fn microsoft_client_id() -> CmdResult<String> {
    configured_microsoft_client_id().ok_or_else(|| ApiError {
        message: "Outlook OAuth не настроен в этой сборке: не задан TRUEMAIL_MICROSOFT_CLIENT_ID."
            .into(),
    })
}

async fn receive_oauth_callback(
    listener: tokio::net::TcpListener,
    expected_state: &str,
    provider: &str,
) -> CmdResult<String> {
    tokio::time::timeout(std::time::Duration::from_secs(300), async {
        loop {
            let (mut stream, _) = listener.accept().await.map_err(|error| ApiError {
                message: format!("не удалось принять OAuth callback: {error}"),
            })?;
            let mut request = vec![0_u8; 16 * 1024];
            let size = stream.read(&mut request).await.map_err(|error| ApiError {
                message: format!("не удалось прочитать OAuth callback: {error}"),
            })?;
            let request = String::from_utf8_lossy(&request[..size]);
            let target = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            let parsed = url::Url::parse(&format!("http://127.0.0.1{target}"));
            let params = parsed
                .ok()
                .map(|url| url.query_pairs().into_owned().collect::<HashMap<_, _>>())
                .unwrap_or_default();
            let valid_state = params.get("state").is_some_and(|state| state == expected_state);
            let code = params.get("code").cloned();
            let error = params.get("error").cloned();
            let success = valid_state && code.is_some();
            let (status, title, body) = if success {
                (
                    "200 OK",
                    format!("{provider} подключён"),
                    "Авторизация завершена. Можно закрыть эту вкладку и вернуться в truemail.",
                )
            } else {
                (
                    "400 Bad Request",
                    format!("Не удалось подключить {provider}"),
                    "Вернитесь в truemail и повторите подключение.",
                )
            };
            let html = format!(
                "<!doctype html><meta charset=utf-8><title>{title}</title><style>body{{font:16px system-ui;max-width:620px;margin:12vh auto;padding:32px;color:#171923}}h1{{font-size:28px}}</style><h1>{title}</h1><p>{body}</p>"
            );
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
                html.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.shutdown().await;
            if !valid_state {
                return Err(ApiError {
                    message: format!("{provider} OAuth вернул неверный state; подключение отменено"),
                });
            }
            if let Some(error) = error {
                return Err(ApiError {
                    message: format!("{provider} OAuth: {error}"),
                });
            }
            if let Some(code) = code {
                return Ok(code);
            }
        }
    })
    .await
    .map_err(|_| ApiError {
        message: format!("Время ожидания входа в {provider} истекло. Нажмите «Подключить» ещё раз."),
    })?
}

fn connected_response(connected: truemail_core::account::ConnectedAccountSync) -> ConnectedAccount {
    ConnectedAccount {
        account: connected.account,
        mail_folders: connected.mail_folders,
        calendars: connected.calendars,
        events: connected.events,
        contacts: connected.contacts,
        warnings: connected.warnings,
    }
}

async fn spawn_initial_mail_sync(
    app: &AppHandle,
    state: &State<'_, AppState>,
    core: Arc<Core>,
    account: Account,
) {
    let mut syncing = state.syncing.lock().await;
    if !syncing.insert(account.id) {
        return;
    }
    drop(syncing);
    let sync_set = state.syncing.clone();
    let aux_sync_set = state.syncing_aux.clone();
    let sync_app = app.clone();
    tokio::spawn(async move {
        match core.accounts.sync_mail_account(&account).await {
            Ok(result) => {
                tracing::info!(account = %account.email, folders = result.mail_folders, "первая синхронизация почты завершена")
            }
            Err(error) => {
                tracing::error!(account = %account.email, %error, "первая синхронизация почты не удалась")
            }
        }
        sync_set.lock().await.remove(&account.id);
        let _ = sync_app.emit("truemail-data-changed", account.id);
        if matches!(
            account.provider,
            Provider::Yandex | Provider::Gmail | Provider::Exchange
        ) {
            let mut syncing_aux = aux_sync_set.lock().await;
            if !syncing_aux.insert(account.id) {
                return;
            }
            drop(syncing_aux);
            match core.accounts.sync_auxiliary_account(&account).await {
                Ok((calendars, events, contacts)) => tracing::info!(
                    account = %account.email,
                    calendars,
                    events,
                    contacts,
                    "первая синхронизация календарей и контактов завершена"
                ),
                Err(error) => tracing::error!(
                    account = %account.email,
                    %error,
                    "первая синхронизация календарей и контактов не удалась"
                ),
            }
            aux_sync_set.lock().await.remove(&account.id);
            let _ = sync_app.emit("truemail-data-changed", account.id);
        }
    });
}

fn open_in_yandex_browser(app: &AppHandle, url: &str) -> CmdResult<()> {
    #[cfg(target_os = "windows")]
    {
        let mut candidates = Vec::new();
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            candidates
                .push(PathBuf::from(local).join("Yandex/YandexBrowser/Application/browser.exe"));
        }
        if let Some(program_files) = std::env::var_os("ProgramFiles") {
            candidates.push(
                PathBuf::from(program_files).join("Yandex/YandexBrowser/Application/browser.exe"),
            );
        }
        if let Some(browser) = candidates.into_iter().find(|path| path.is_file()) {
            app.shell()
                .command(browser)
                .arg(url)
                .spawn()
                .map_err(|e| ApiError {
                    message: format!("не удалось открыть Яндекс Браузер: {e}"),
                })?;
            return Ok(());
        }
        // Яндекс Браузер предпочтителен по требованию продукта, но его
        // отсутствие не должно блокировать подключение аккаунта.
        #[allow(deprecated)]
        app.shell().open(url, None).map_err(|error| ApiError {
            message: format!("не удалось открыть системный браузер: {error}"),
        })?;
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        #[allow(deprecated)]
        app.shell().open(url, None).map_err(|e| ApiError {
            message: format!("не удалось открыть браузер: {e}"),
        })?;
        Ok(())
    }
}

#[tauri::command]
pub async fn begin_account_connection(
    app: AppHandle,
    state: State<'_, AppState>,
    email: String,
) -> CmdResult<PendingOAuthResponse> {
    let core = core(&state).await?;
    let email = email.trim().to_lowercase();
    let config = truemail_core::account::discover_provider(&email).await;
    let pkce = truemail_core::account::generate_pkce();
    let oauth_state = truemail_core::account::generate_state();
    match config.provider {
        truemail_core::model::Provider::Yandex => {
            let client_id = yandex_client_id()?;
            // Redirect URI должен быть зарегистрирован в OAuth-приложении
            // Яндекса с точным scheme/host/port/path.
            let redirect_uri = configured_yandex_redirect_uri();
            let redirect = url::Url::parse(&redirect_uri).map_err(|error| ApiError {
                message: format!("неверный TRUEMAIL_YANDEX_REDIRECT_URI: {error}"),
            })?;
            if redirect.scheme() != "http"
                || !matches!(redirect.host_str(), Some("127.0.0.1" | "localhost"))
            {
                return Err(ApiError {
                    message: "Яндекс OAuth callback должен быть локальным http://127.0.0.1 адресом"
                        .into(),
                });
            }
            let port = redirect.port().ok_or_else(|| ApiError {
                message: "в TRUEMAIL_YANDEX_REDIRECT_URI должен быть указан порт".into(),
            })?;
            let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
                .await
                .map_err(|error| ApiError {
                    message: format!(
                        "не удалось открыть Яндекс OAuth callback на порту {port}: {error}"
                    ),
                })?;
            let url = truemail_core::account::yandex_authorize_url(
                &client_id,
                &email,
                &oauth_state,
                &pkce.challenge,
                &redirect_uri,
            )?;
            open_in_yandex_browser(&app, &url)?;
            let code =
                Zeroizing::new(receive_oauth_callback(listener, &oauth_state, "Яндекс").await?);
            let token = truemail_core::account::exchange_yandex_code(
                &client_id,
                &code,
                &pkce.verifier,
                &redirect_uri,
            )
            .await?;
            let display_name = email.split('@').next().unwrap_or(&email).to_owned();
            let connected = core
                .accounts
                .add_yandex_oauth(&email, &display_name, token)
                .await?;
            let account = connected.account.clone();
            let response = connected_response(connected);
            spawn_initial_mail_sync(&app, &state, core, account).await;
            Ok(PendingOAuthResponse {
                mode: "connected".into(),
                state: None,
                connected: Some(response),
                password_config: None,
            })
        }
        truemail_core::model::Provider::Gmail => {
            let (client_id, client_secret) = google_client_credentials()?;
            let client_secret = Zeroizing::new(client_secret);
            let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
                .await
                .map_err(|error| ApiError {
                    message: format!("не удалось открыть локальный OAuth callback: {error}"),
                })?;
            let port = listener
                .local_addr()
                .map_err(|error| ApiError {
                    message: format!("не удалось определить OAuth callback: {error}"),
                })?
                .port();
            let redirect_uri = format!("http://127.0.0.1:{port}/oauth/google/callback");
            let url = truemail_core::account::google_authorize_url(
                &client_id,
                &email,
                &oauth_state,
                &pkce.challenge,
                &redirect_uri,
            )?;
            open_in_yandex_browser(&app, &url)?;
            let code =
                Zeroizing::new(receive_oauth_callback(listener, &oauth_state, "Google").await?);
            let token = truemail_core::account::exchange_google_code(
                &client_id,
                &client_secret,
                &code,
                &pkce.verifier,
                &redirect_uri,
            )
            .await?;
            let display_name = email.split('@').next().unwrap_or(&email).to_owned();
            let connected = core
                .accounts
                .add_gmail_oauth(&email, &display_name, token)
                .await?;
            let account = connected.account.clone();
            let response = connected_response(connected);
            spawn_initial_mail_sync(&app, &state, core, account).await;
            Ok(PendingOAuthResponse {
                mode: "connected".into(),
                state: None,
                connected: Some(response),
                password_config: None,
            })
        }
        truemail_core::model::Provider::Outlook => {
            let client_id = microsoft_client_id()?;
            let tenant = configured_microsoft_tenant();
            let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
                .await
                .map_err(|error| ApiError {
                    message: format!("не удалось открыть локальный OAuth callback: {error}"),
                })?;
            let port = listener
                .local_addr()
                .map_err(|error| ApiError {
                    message: format!("не удалось определить OAuth callback: {error}"),
                })?
                .port();
            let redirect_uri = format!("http://127.0.0.1:{port}/oauth/microsoft/callback");
            let url = truemail_core::account::microsoft_authorize_url(
                &client_id,
                &tenant,
                &email,
                &oauth_state,
                &pkce.challenge,
                &redirect_uri,
            )?;
            open_in_yandex_browser(&app, &url)?;
            let code =
                Zeroizing::new(receive_oauth_callback(listener, &oauth_state, "Microsoft").await?);
            let token = truemail_core::account::exchange_microsoft_code(
                &client_id,
                &tenant,
                &code,
                &pkce.verifier,
                &redirect_uri,
            )
            .await?;
            let display_name = email.split('@').next().unwrap_or(&email).to_owned();
            let connected = core
                .accounts
                .add_outlook_oauth(&email, &display_name, token)
                .await?;
            let account = connected.account.clone();
            let response = connected_response(connected);
            spawn_initial_mail_sync(&app, &state, core, account).await;
            Ok(PendingOAuthResponse {
                mode: "connected".into(),
                state: None,
                connected: Some(response),
                password_config: None,
            })
        }
        Provider::Mailru | Provider::Icloud | Provider::Generic => {
            let domain = email.rsplit('@').next().unwrap_or_default();
            Ok(PendingOAuthResponse {
                mode: "password".into(),
                state: None,
                connected: None,
                password_config: Some(PasswordConnectionInfo {
                    provider: config.provider,
                    backend_kind: config.backend_kind,
                    username: email.clone(),
                    imap: if config.backend_kind == BackendKind::Jmap {
                        None
                    } else {
                        Some(config.imap.unwrap_or(ServerConfig {
                            host: format!("imap.{domain}"),
                            port: 993,
                            security: Security::Ssl,
                        }))
                    },
                    smtp: if config.backend_kind == BackendKind::Jmap {
                        None
                    } else {
                        config.smtp.or_else(|| {
                            (!domain.is_empty()).then(|| ServerConfig {
                                host: format!("smtp.{domain}"),
                                port: 465,
                                security: Security::Ssl,
                            })
                        })
                    },
                    jmap_url: config.jmap_url,
                    ews_url: None,
                }),
            })
        }
        Provider::Exchange => Ok(PendingOAuthResponse {
            mode: "password".into(),
            state: None,
            connected: None,
            // Autodiscover уточнит адрес EWS с учётными данными; из discover
            // приходит только предполагаемый URL как подсказка для поля.
            password_config: Some(PasswordConnectionInfo {
                provider: Provider::Exchange,
                backend_kind: BackendKind::Ews,
                username: email.clone(),
                imap: None,
                smtp: None,
                jmap_url: None,
                ews_url: config.ews_url,
            }),
        }),
    }
}

fn parse_security(value: &str) -> CmdResult<Security> {
    match value {
        "ssl" => Ok(Security::Ssl),
        "starttls" => Ok(Security::Starttls),
        _ => Err(ApiError {
            message: "выберите SSL/TLS или STARTTLS".into(),
        }),
    }
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn complete_password_imap(
    app: AppHandle,
    state: State<'_, AppState>,
    email: String,
    username: String,
    password: String,
    provider: Provider,
    imap_host: String,
    imap_port: u16,
    imap_security: String,
    smtp_host: String,
    smtp_port: u16,
    smtp_security: String,
) -> CmdResult<ConnectedAccount> {
    let email = email.trim().to_lowercase();
    let username = username.trim();
    if username.is_empty() || imap_host.trim().is_empty() {
        return Err(ApiError {
            message: "укажите имя пользователя и IMAP-сервер".into(),
        });
    }
    if !matches!(
        provider,
        Provider::Mailru | Provider::Icloud | Provider::Generic
    ) {
        return Err(ApiError {
            message: "этот способ входа не подходит выбранному провайдеру".into(),
        });
    }
    let config = truemail_core::account::ProviderConfig {
        provider,
        backend_kind: BackendKind::Imap,
        auth_kind: if provider == Provider::Generic {
            AuthKind::Password
        } else {
            AuthKind::AppPassword
        },
        imap: Some(ServerConfig {
            host: imap_host.trim().to_owned(),
            port: imap_port,
            security: parse_security(&imap_security)?,
        }),
        smtp: (!smtp_host.trim().is_empty())
            .then(|| {
                Ok::<_, ApiError>(ServerConfig {
                    host: smtp_host.trim().to_owned(),
                    port: smtp_port,
                    security: parse_security(&smtp_security)?,
                })
            })
            .transpose()?,
        ews_url: None,
        jmap_url: None,
    };
    let core = core(&state).await?;
    let display_name = email.split('@').next().unwrap_or(&email).to_owned();
    let connected = core
        .accounts
        .add_password_imap(&email, &display_name, username, &password, &config)
        .await?;
    let account = connected.account.clone();
    let response = connected_response(connected);
    spawn_initial_mail_sync(&app, &state, core, account).await;
    Ok(response)
}

#[tauri::command]
pub async fn complete_exchange_ews(
    app: AppHandle,
    state: State<'_, AppState>,
    email: String,
    username: String,
    password: String,
    server_hint: String,
) -> CmdResult<ConnectedAccount> {
    let email = email.trim().to_lowercase();
    let username = username.trim();
    if username.is_empty() {
        return Err(ApiError {
            message: "укажите DOMAIN\\user, UPN или адрес пользователя Exchange".into(),
        });
    }
    let core = core(&state).await?;
    let display_name = email.split('@').next().unwrap_or(&email).to_owned();
    let connected = core
        .accounts
        .add_exchange_ews(
            &email,
            &display_name,
            username,
            &password,
            (!server_hint.trim().is_empty()).then_some(server_hint.trim()),
        )
        .await?;
    let account = connected.account.clone();
    let response = connected_response(connected);
    spawn_initial_mail_sync(&app, &state, core, account).await;
    Ok(response)
}

#[tauri::command]
pub async fn complete_jmap(
    app: AppHandle,
    state: State<'_, AppState>,
    email: String,
    username: String,
    password: String,
    session_url: String,
) -> CmdResult<ConnectedAccount> {
    let email = email.trim().to_lowercase();
    let username = username.trim();
    if username.is_empty() || session_url.trim().is_empty() {
        return Err(ApiError {
            message: "укажите имя пользователя и JMAP Session URL".into(),
        });
    }
    let core = core(&state).await?;
    let display_name = email.split('@').next().unwrap_or(&email).to_owned();
    let connected = core
        .accounts
        .add_jmap_password(
            &email,
            &display_name,
            username,
            &password,
            session_url.trim(),
        )
        .await?;
    let account = connected.account.clone();
    let response = connected_response(connected);
    spawn_initial_mail_sync(&app, &state, core, account).await;
    Ok(response)
}

#[tauri::command]
pub async fn complete_yandex_oauth(
    app: AppHandle,
    state: State<'_, AppState>,
    oauth_state: String,
    code: String,
) -> CmdResult<ConnectedAccount> {
    let code = Zeroizing::new(code);
    let core = core(&state).await?;
    let pending = state
        .oauth
        .lock()
        .await
        .get(&oauth_state)
        .cloned()
        .ok_or_else(|| ApiError {
            message: "OAuth-сессия не найдена или устарела".into(),
        })?;
    let token = truemail_core::account::exchange_yandex_code(
        &pending.client_id,
        &code,
        &pending.verifier,
        "https://oauth.yandex.ru/verification_code",
    )
    .await?;
    state.oauth.lock().await.remove(&oauth_state);
    let email = pending.email.trim().to_lowercase();
    let display_name = email.split('@').next().unwrap_or(&email).to_owned();
    let connected = core
        .accounts
        .add_yandex_oauth(&email, &display_name, token)
        .await?;
    let account = connected.account.clone();
    let response = connected_response(connected);
    spawn_initial_mail_sync(&app, &state, core, account).await;
    Ok(response)
}

/// Список инструментов внешнего API (для справки/настроек).
#[tauri::command]
pub fn api_tools() -> Vec<McpTool> {
    mcp_tools()
}

#[derive(Debug, Clone, Serialize)]
pub struct ExternalApiStatus {
    pub running: bool,
    pub port: Option<u16>,
    pub url: Option<String>,
}

#[tauri::command]
pub async fn external_api_status(state: State<'_, AppState>) -> CmdResult<ExternalApiStatus> {
    let server = state.api_server.lock().await;
    let port = server.as_ref().map(|server| server.port);
    Ok(ExternalApiStatus {
        running: port.is_some(),
        port,
        url: port.map(|port| format!("http://127.0.0.1:{port}")),
    })
}

#[tauri::command]
pub async fn start_external_api(
    state: State<'_, AppState>,
    port: Option<u16>,
) -> CmdResult<ExternalApiStatus> {
    let core = core(&state).await?;
    let mut server = state.api_server.lock().await;
    if let Some(running) = server.as_ref() {
        return Ok(ExternalApiStatus {
            running: true,
            port: Some(running.port),
            url: Some(format!("http://127.0.0.1:{}", running.port)),
        });
    }
    let requested_port = port.unwrap_or(34981);
    let running = truemail_core::api::start_server(core.clone(), requested_port).await?;
    let actual_port = running.port;
    *server = Some(running);
    core.db.set_setting("external_api_enabled", "1").await?;
    core.db
        .set_setting("external_api_port", &requested_port.to_string())
        .await?;
    Ok(ExternalApiStatus {
        running: true,
        port: Some(actual_port),
        url: Some(format!("http://127.0.0.1:{actual_port}")),
    })
}

#[tauri::command]
pub async fn stop_external_api(state: State<'_, AppState>) -> CmdResult<ExternalApiStatus> {
    let core = core(&state).await?;
    if let Some(server) = state.api_server.lock().await.take() {
        server.stop();
    }
    core.db.set_setting("external_api_enabled", "0").await?;
    Ok(ExternalApiStatus {
        running: false,
        port: None,
        url: None,
    })
}

#[tauri::command]
pub async fn list_api_clients(state: State<'_, AppState>) -> CmdResult<Vec<ApiClient>> {
    let core = core(&state).await?;
    Ok(truemail_core::api::list_clients(core.as_ref()).await?)
}

#[tauri::command]
pub async fn create_api_client(
    state: State<'_, AppState>,
    name: String,
    caps: Vec<Capability>,
) -> CmdResult<CreatedApiClient> {
    let core = core(&state).await?;
    Ok(truemail_core::api::create_client(core.as_ref(), &name, caps).await?)
}

#[tauri::command]
pub async fn revoke_api_client(state: State<'_, AppState>, client_id: i64) -> CmdResult<bool> {
    let core = core(&state).await?;
    Ok(truemail_core::api::revoke_client(core.as_ref(), client_id).await?)
}

#[tauri::command]
pub async fn list_api_audit(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> CmdResult<Vec<ApiAuditEntry>> {
    let core = core(&state).await?;
    Ok(truemail_core::api::list_audit(core.as_ref(), limit.unwrap_or(50)).await?)
}

#[tauri::command]
pub async fn clear_api_audit(state: State<'_, AppState>) -> CmdResult<u64> {
    let core = core(&state).await?;
    Ok(truemail_core::api::clear_audit(core.as_ref()).await?)
}

#[tauri::command]
pub fn localization_catalog(locale: String) -> HashMap<String, String> {
    truemail_core::i18n::I18n::new(&locale).catalog()
}

#[cfg(test)]
mod update_tests {
    use super::*;

    #[test]
    fn gmail_realtime_uses_first_snapshot_as_baseline() {
        let mut observed = HashMap::new();
        assert_eq!(
            observe_gmail_message_ids(&mut observed, 7, vec!["old".into()]),
            None
        );
        assert_eq!(
            observe_gmail_message_ids(&mut observed, 7, vec!["new".into(), "old".into()]),
            Some(vec!["new".into()])
        );
        assert_eq!(
            observe_gmail_message_ids(&mut observed, 7, vec!["new".into(), "old".into()]),
            Some(Vec::new())
        );
    }

    #[test]
    fn update_manifest_is_public_and_uses_https() {
        let endpoint = url::Url::parse(DEFAULT_UPDATE_ENDPOINT).unwrap();
        assert_eq!(endpoint.scheme(), "https");
        assert_eq!(endpoint.host_str(), Some("chernov.gitverse.site"));
        assert!(endpoint.path().ends_with("/latest.json"));
    }
}
