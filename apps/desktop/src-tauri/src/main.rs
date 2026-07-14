//! truemail — десктоп-приложение (Tauri v2). Тонкий клиент над ядром.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use commands::AppState;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use truemail_core::Core;

pub(crate) fn default_data_dir() -> std::path::PathBuf {
    dirs_data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("truemail")
}

fn data_dir() -> std::path::PathBuf {
    truemail_core::crypto::load_data_dir()
        .ok()
        .flatten()
        .unwrap_or_else(default_data_dir)
}

fn dirs_data_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from)
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|h| std::path::PathBuf::from(h).join("Library/Application Support"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share"))
            })
    }
}

fn main() {
    if let Err(error) = run() {
        tracing::error!(%error, "truemail failed to start");
        let _ = rfd::MessageDialog::new()
            .set_title("truemail — ошибка запуска")
            .set_description(format!(
                "Приложение не удалось запустить.\n\n{error}\n\nДанные не были изменены."
            ))
            .set_level(rfd::MessageLevel::Error)
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }
}

fn run() -> anyhow::Result<()> {
    // Rustls 0.23 требует выбрать процессный provider до создания ClientConfig.
    // В Cargo features оставлен только aws-lc-rs, но явная установка сохраняет
    // однозначное поведение при добавлении новых TLS-зависимостей.
    tokio_rustls::rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("не удалось установить TLS crypto provider aws-lc-rs"))?;

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // На новой установке ядро создаст визард после сбора пользовательской
    // энтропии. На настроенной — открываем SQLCipher сразу.
    let rt = tokio::runtime::Runtime::new()?;
    let core = if truemail_core::crypto::keys_initialized()? {
        Some(Arc::new(rt.block_on(Core::bootstrap(data_dir()))?))
    } else {
        None
    };
    let state = AppState {
        core: tokio::sync::RwLock::new(core),
        oauth: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        syncing: Arc::new(tokio::sync::Mutex::new(std::collections::HashSet::new())),
        watching: Arc::new(tokio::sync::Mutex::new(std::collections::HashSet::new())),
        generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };

    let show_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyM);
    let compose_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyC);
    let search_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyF);
    let show_handler = show_shortcut;
    let compose_handler = compose_shortcut;
    let search_handler = search_shortcut;
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    let action = if shortcut == &show_handler {
                        "toggle"
                    } else if shortcut == &compose_handler {
                        "compose"
                    } else if shortcut == &search_handler {
                        "search"
                    } else {
                        return;
                    };
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.unminimize();
                        let _ = window.set_focus();
                    }
                    let _ = app.emit("truemail-global-shortcut", action);
                })
                .build(),
        )
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .setup(move |app| {
            app.global_shortcut().register(show_shortcut)?;
            app.global_shortcut().register(compose_shortcut)?;
            app.global_shortcut().register(search_shortcut)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap_status,
            commands::initialize_storage,
            commands::list_accounts,
            commands::list_folders,
            commands::set_folder_role,
            commands::list_messages,
            commands::list_messages_page,
            commands::get_message,
            commands::list_smart_folders,
            commands::list_contacts,
            commands::search,
            commands::list_calendar_data,
            commands::storage_status,
            commands::move_storage,
            commands::open_data_dir,
            commands::clear_local_data,
            commands::sync_accounts,
            commands::sync_auxiliary_accounts,
            commands::start_realtime,
            commands::send_message,
            commands::schedule_message,
            commands::mark_seen,
            commands::message_action,
            commands::undo_message_action,
            commands::get_setting,
            commands::set_setting,
            commands::begin_account_connection,
            commands::complete_yandex_oauth,
            commands::api_tools,
            commands::localization_catalog,
        ])
        .run(tauri::generate_context!())?;
    Ok(())
}
