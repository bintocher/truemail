//! Tauri-команды: тонкая прослойка между фронтендом и ядром.
//! Фронтенд (ui/) вызывает их через `invoke(...)`.

use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_shell::ShellExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use truemail_core::Core;
use truemail_core::account::{
    ContactInput, EventInput, RemoteObject, configured_google_client_id,
    configured_google_client_secret, configured_yandex_client_id,
};
use truemail_core::api::{McpTool, mcp_tools};
use truemail_core::model::{
    Account, Contact, Event, Folder, MessageFull, MessageMeta, SmartFolder,
};
use truemail_core::storage::repo::CalendarSummary;

/// Общее состояние приложения — ядро.
pub struct AppState {
    pub core: tokio::sync::RwLock<Option<Arc<Core>>>,
    pub oauth: tokio::sync::Mutex<HashMap<String, PendingOAuth>>,
    pub syncing: Arc<tokio::sync::Mutex<HashSet<i64>>>,
    pub watching: Arc<tokio::sync::Mutex<HashSet<i64>>>,
    pub generation: Arc<std::sync::atomic::AtomicU64>,
}

#[derive(Clone)]
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
    message: String,
}

impl From<truemail_core::Error> for ApiError {
    fn from(e: truemail_core::Error) -> Self {
        ApiError {
            message: e.to_string(),
        }
    }
}

type CmdResult<T> = Result<T, ApiError>;

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
    Ok(core(&state).await?.db.get_message(message_id).await?)
}

#[tauri::command]
pub async fn list_smart_folders(state: State<'_, AppState>) -> CmdResult<Vec<SmartFolder>> {
    Ok(core(&state).await?.db.list_smart_folders().await?)
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
    let remote_url = row.1.as_deref().ok_or_else(|| ApiError {
        message: "У контакта нет серверного идентификатора".into(),
    })?;
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
    let checkpoint_core = core(&state).await?;
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&checkpoint_core.db.pool)
        .await
        .map_err(truemail_core::Error::from)?;
    let old_core = state.core.write().await.take().ok_or_else(|| ApiError {
        message: "Хранилище ещё не создано".into(),
    })?;
    old_core.db.pool.close().await;
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
            sqlx::query("DELETE FROM messages WHERE folder_id IN (SELECT id FROM folders WHERE role IN ('trash','spam'))").execute(&core.db.pool).await.map_err(truemail_core::Error::from)?;
        }
        "all" => {
            let mut tx = core
                .db
                .pool
                .begin()
                .await
                .map_err(truemail_core::Error::from)?;
            sqlx::query("DELETE FROM outbox_ops")
                .execute(&mut *tx)
                .await
                .map_err(truemail_core::Error::from)?;
            sqlx::query("DELETE FROM messages")
                .execute(&mut *tx)
                .await
                .map_err(truemail_core::Error::from)?;
            sqlx::query("DELETE FROM messages_fts")
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
            sqlx::query("DELETE FROM attachments WHERE message_id IN (SELECT id FROM messages WHERE date < datetime('now','-1 year'))").execute(&core.db.pool).await.map_err(truemail_core::Error::from)?;
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
        if !matches!(
            account.provider,
            truemail_core::model::Provider::Yandex | truemail_core::model::Provider::Gmail
        ) {
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
            let state = match sync_core.accounts.sync_mail_account(&account).await {
                Ok(result) => {
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
            truemail_core::model::Provider::Yandex | truemail_core::model::Provider::Gmail
        ) {
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
            serde_json::json!({"account_id": account.id, "scope": "auxiliary", "status": "syncing"}),
        );
        tokio::spawn(async move {
            let sync_result = match account.provider {
                truemail_core::model::Provider::Yandex => {
                    sync_core.accounts.sync_yandex_dav_account(&account).await
                }
                truemail_core::model::Provider::Gmail => {
                    sync_core
                        .accounts
                        .sync_google_auxiliary_account(&account)
                        .await
                }
                _ => unreachable!(),
            };
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
    for account in core.db.list_accounts().await? {
        if !matches!(
            account.provider,
            truemail_core::model::Provider::Yandex | truemail_core::model::Provider::Gmail
        ) {
            continue;
        }
        let mut watching = state.watching.lock().await;
        if !watching.insert(account.id) {
            continue;
        }
        drop(watching);

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
                let token = match watch_core.accounts.oauth_access_token(&watch_account).await {
                    Ok(token) => token,
                    Err(error) => {
                        tracing::error!(account = %watch_account.email, %error, "не удалось прочитать OAuth-токен для IMAP IDLE");
                        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                        continue;
                    }
                };
                let wait = match watch_account.provider {
                    truemail_core::model::Provider::Yandex => {
                        truemail_core::backend::wait_for_yandex_change(&watch_account.email, &token)
                            .await
                    }
                    truemail_core::model::Provider::Gmail => {
                        truemail_core::backend::wait_for_gmail_change(&watch_account.email, &token)
                            .await
                    }
                    _ => unreachable!(),
                };
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
                            Ok(messages) => tracing::info!(
                                account = %watch_account.email,
                                messages,
                                "IMAP IDLE: входящие обновлены"
                            ),
                            Err(error) => tracing::error!(
                                account = %watch_account.email,
                                %error,
                                "IMAP IDLE: не удалось дозагрузить входящие"
                            ),
                        }
                        watch_syncing.lock().await.remove(&watch_account.id);
                        let _ = watch_app.emit("truemail-sync-state", serde_json::json!({"account_id": watch_account.id, "scope": "mail", "status": "ready"}));
                        let _ = watch_app.emit("truemail-data-changed", watch_account.id);
                    }
                    Err(error) => {
                        tracing::warn!(account = %watch_account.email, %error, "IMAP IDLE-соединение будет восстановлено");
                        let _ = watch_app.emit("truemail-sync-state", serde_json::json!({"account_id": watch_account.id, "scope": "mail", "status": "retrying"}));
                        tokio::time::sleep(retry_delay).await;
                        retry_delay = (retry_delay * 2).min(std::time::Duration::from_secs(60));
                    }
                }
            }
        });

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
    if !matches!(
        account.provider,
        truemail_core::model::Provider::Yandex | truemail_core::model::Provider::Gmail
    ) {
        return Err(ApiError {
            message: "Отправка для этого провайдера ещё не настроена".into(),
        });
    }
    let token = core.accounts.oauth_access_token(&account).await?;
    let outgoing = outgoing_message(&account, request);
    match account.provider {
        truemail_core::model::Provider::Yandex => {
            truemail_core::backend::send_yandex(outgoing, &token).await?
        }
        truemail_core::model::Provider::Gmail => {
            truemail_core::backend::send_gmail(outgoing, &token).await?
        }
        _ => unreachable!(),
    }
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
    if !matches!(
        account.provider,
        truemail_core::model::Provider::Yandex | truemail_core::model::Provider::Gmail
    ) {
        return Err(ApiError {
            message: "Отложенная отправка для этого провайдера не настроена".into(),
        });
    }
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

#[tauri::command]
pub async fn set_setting(state: State<'_, AppState>, key: String, value: String) -> CmdResult<()> {
    Ok(core(&state).await?.db.set_setting(&key, &value).await?)
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

async fn receive_google_callback(
    listener: tokio::net::TcpListener,
    expected_state: &str,
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
                    "Gmail подключён",
                    "Авторизация завершена. Можно закрыть эту вкладку и вернуться в truemail.",
                )
            } else {
                (
                    "400 Bad Request",
                    "Не удалось подключить Gmail",
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
                    message: "Google OAuth вернул неверный state; подключение отменено".into(),
                });
            }
            if let Some(error) = error {
                return Err(ApiError {
                    message: format!("Google OAuth: {error}"),
                });
            }
            if let Some(code) = code {
                return Ok(code);
            }
        }
    })
    .await
    .map_err(|_| ApiError {
        message: "Время ожидания входа в Google истекло. Нажмите «Подключить» ещё раз.".into(),
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
    let sync_app = app.clone();
    tokio::spawn(async move {
        match core.accounts.sync_mail_account(&account).await {
            Ok(result) => {
                tracing::info!(account = %account.email, folders = result.mail_folders, calendars = result.calendars, events = result.events, contacts = result.contacts, "фоновая синхронизация завершена")
            }
            Err(error) => {
                tracing::error!(account = %account.email, %error, "фоновая синхронизация не удалась")
            }
        }
        sync_set.lock().await.remove(&account.id);
        let _ = sync_app.emit("truemail-data-changed", account.id);
    });
}

fn unsupported_provider_message(config: &truemail_core::account::ProviderConfig) -> String {
    let provider = format!("{:?}", config.provider);
    let imap = config
        .imap
        .as_ref()
        .map(|server| format!("{}:{}", server.host, server.port))
        .unwrap_or_else(|| "не найден".into());
    let smtp = config
        .smtp
        .as_ref()
        .map(|server| format!("{}:{}", server.host, server.port))
        .unwrap_or_else(|| "не найден".into());
    format!(
        "Определён провайдер {provider}, но OAuth для него в truemail пока не реализован. IMAP: {imap}; SMTP: {smtp}. Для Mail.ru и iCloud нужен отдельный пароль приложения; обычный пароль аккаунта использовать не следует."
    )
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
            let url = truemail_core::account::yandex_authorize_url(
                &client_id,
                &email,
                &oauth_state,
                &pkce.challenge,
            )?;
            open_in_yandex_browser(&app, &url)?;
            let mut oauth = state.oauth.lock().await;
            oauth.clear();
            oauth.insert(
                oauth_state.clone(),
                PendingOAuth {
                    email,
                    verifier: pkce.verifier,
                    client_id,
                },
            );
            Ok(PendingOAuthResponse {
                mode: "verification_code".into(),
                state: Some(oauth_state),
                connected: None,
            })
        }
        truemail_core::model::Provider::Gmail => {
            let (client_id, client_secret) = google_client_credentials()?;
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
            let code = receive_google_callback(listener, &oauth_state).await?;
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
            })
        }
        _ => Err(ApiError {
            message: unsupported_provider_message(&config),
        }),
    }
}

#[tauri::command]
pub async fn complete_yandex_oauth(
    app: AppHandle,
    state: State<'_, AppState>,
    oauth_state: String,
    code: String,
) -> CmdResult<ConnectedAccount> {
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
    let token =
        truemail_core::account::exchange_yandex_code(&pending.client_id, &code, &pending.verifier)
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

#[tauri::command]
pub fn localization_catalog(locale: String) -> HashMap<String, String> {
    const KEYS: &[&str] = &[
        "nav-smart-folders",
        "nav-accounts",
        "nav-calendar",
        "nav-contacts",
        "nav-all-inbox",
        "nav-all-important",
        "nav-all-sent",
        "nav-all-drafts",
        "nav-today",
        "nav-unread",
        "nav-with-attachments",
        "nav-waiting-reply",
        "folder-inbox",
        "folder-sent",
        "folder-drafts",
        "folder-archive",
        "folder-spam",
        "folder-trash",
        "action-reply",
        "action-reply-all",
        "action-forward",
        "action-archive",
        "action-delete",
        "action-compose",
        "action-send",
        "palette-title",
        "palette-placeholder",
        "settings",
        "settings-general",
        "settings-expert-mode",
        "settings-toolbar",
        "settings-accounts",
        "settings-smart",
        "settings-unified",
        "settings-folders",
        "settings-calendars",
        "settings-storage",
        "settings-themes",
        "settings-privacy",
        "settings-keys",
        "privacy-external-images",
        "storage-data-folder",
        "storage-encrypted",
        "wizard-back",
        "wizard-next",
        "wizard-skip",
        "wizard-connect",
        "wizard-confirm",
        "wizard-open-mail",
    ];
    truemail_core::i18n::I18n::new(&locale).catalog(KEYS)
}
