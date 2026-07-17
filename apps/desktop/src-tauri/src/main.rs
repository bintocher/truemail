//! truemail — десктоп-приложение (Tauri v2). Тонкий клиент над ядром.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use commands::AppState;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

/// Что сохраняем и восстанавливаем для окон: геометрию - да, видимость - нет.
/// StateFlags::all() записывал visible=false для свёрнутого в трей окна, и при
/// следующем запуске программа открывалась без окна, одной иконкой в трее.
const WINDOW_STATE_FLAGS: StateFlags = StateFlags::SIZE
    .union(StateFlags::POSITION)
    .union(StateFlags::MAXIMIZED)
    .union(StateFlags::FULLSCREEN);
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use truemail_core::Core;

/// Показать и сфокусировать главное окно (из трея/клика).
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

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

    // Логи пишем и в stdout (виден при запуске из терминала), и в файл в
    // data_dir/logs/ - у GUI-сборки на Windows stdout не отображается, поэтому
    // файл единственный способ увидеть диагностику. По умолчанию включаем debug
    // для ядра, чтобы IMAP-операции (удаление папок и т.п.) логировались из коробки.
    use tracing_subscriber::fmt::writer::MakeWriterExt;
    let log_dir = data_dir().join("logs");
    let _ = std::fs::create_dir_all(&log_dir);
    let file_appender = tracing_appender::rolling::daily(&log_dir, "truemail.log");
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,truemail_core=debug"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(std::io::stdout.and(file_appender))
        .init();
    tracing::info!(log_dir = %log_dir.display(), "логирование инициализировано");

    // На новой установке ядро создаст визард после сбора пользовательской
    // энтропии. На настроенной — открываем SQLCipher сразу.
    let rt = tokio::runtime::Runtime::new()?;
    let core = if truemail_core::crypto::keys_initialized()? {
        Some(Arc::new(rt.block_on(Core::bootstrap(data_dir()))?))
    } else {
        None
    };
    // Куда показывать уведомления - читаем до старта: позиционирование окна синхронное.
    let notify_anchor = core
        .as_ref()
        .and_then(|core| {
            rt.block_on(core.db.setting("notify_position"))
                .ok()
                .flatten()
        })
        .map(|value| commands::NotifyAnchor::parse(&value))
        .unwrap_or_else(commands::NotifyAnchor::platform_default);
    let state = AppState {
        core: tokio::sync::RwLock::new(core),
        notify_anchor: Arc::new(std::sync::Mutex::new(notify_anchor)),
        oauth: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        syncing: Arc::new(tokio::sync::Mutex::new(std::collections::HashSet::new())),
        syncing_aux: Arc::new(tokio::sync::Mutex::new(std::collections::HashSet::new())),
        watching: Arc::new(tokio::sync::Mutex::new(std::collections::HashSet::new())),
        quitting: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        reminders_started: Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        // Окно уведомлений живёт по своим правилам: позицию ему задаёт
        // notify_position, размер - высота карточек. Плагин иначе восстанавливал
        // его позицию, размер и видимость, показывая пустое окно поверх главного.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(WINDOW_STATE_FLAGS)
                .with_denylist(&["notify"])
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .manage(state)
        .setup(move |app| {
            app.global_shortcut().register(show_shortcut)?;
            app.global_shortcut().register(compose_shortcut)?;
            app.global_shortcut().register(search_shortcut)?;

            // Меню и иконка в системном трее. Приложение продолжает работать в
            // фоне (IMAP IDLE, синхронизация), даже когда окно скрыто.
            let open_item = MenuItem::with_id(app, "tray_open", "Открыть truemail", true, None::<&str>)?;
            let compose_item =
                MenuItem::with_id(app, "tray_compose", "Написать письмо", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "tray_quit", "Выход", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&open_item, &compose_item, &quit_item])?;
            let mut tray = TrayIconBuilder::with_id("main-tray")
                .tooltip("truemail")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "tray_open" => show_main_window(app),
                    "tray_compose" => {
                        show_main_window(app);
                        let _ = app.emit("truemail-global-shortcut", "compose");
                    }
                    "tray_quit" => {
                        let _ = app.save_window_state(WINDOW_STATE_FLAGS);
                        app.state::<AppState>().quitting.store(true, Ordering::SeqCst);
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon().cloned() {
                tray = tray.icon(icon);
            }
            tray.build(app)?;

            // Автозапуск с флагом --hidden: стартуем свёрнутыми в трей.
            if std::env::args().any(|arg| arg == "--hidden") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            // Скрытое окно собственных уведомлений (frameless, поверх всех окон,
            // без панели задач). Наполняется через событие "notify-push".
            let notify_window = tauri::WebviewWindowBuilder::new(
                app,
                "notify",
                tauri::WebviewUrl::App("notify.html".into()),
            )
            .title("truemail")
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .resizable(false)
            .inner_size(380.0, 120.0)
            .visible(false)
            .build();
            if let Ok(window) = notify_window {
                // Пока карточек нет, окно не должно ни висеть поверх главного,
                // ни ловить курсор: иначе оно съедает клики по нему.
                let _ = window.hide();
                let _ = window.set_ignore_cursor_events(true);
                commands::position_notify_window(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Закрытие окна не завершает приложение, а прячет его в трей.
            // Настоящий выход - только через пункт "Выход" в меню трея.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                if !app.state::<AppState>().quitting.load(Ordering::SeqCst) {
                    api.prevent_close();
                    let _ = app.save_window_state(WINDOW_STATE_FLAGS);
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap_status,
            commands::initialize_storage,
            commands::export_key_backup,
            commands::restore_key_backup,
            commands::list_accounts,
            commands::rename_account,
            commands::set_account_color,
            commands::set_account_retention,
            commands::list_labels,
            commands::create_label,
            commands::update_label,
            commands::delete_label,
            commands::toggle_message_label,
            commands::message_label_ids,
            commands::list_folders,
            commands::set_folder_role,
            commands::rename_folder,
            commands::delete_folder,
            commands::list_messages,
            commands::list_messages_page,
            commands::get_message,
            commands::message_raw,
            commands::unsubscribe_one_click,
            commands::attachment_content,
            commands::save_attachment,
            commands::save_all_attachments,
            commands::list_smart_folders,
            commands::list_mail_rules,
            commands::save_mail_rule,
            commands::set_mail_rule_enabled,
            commands::delete_mail_rule,
            commands::list_contacts,
            commands::search,
            commands::list_calendar_data,
            commands::create_event,
            commands::update_event,
            commands::delete_event,
            commands::create_contact,
            commands::update_contact,
            commands::delete_contact,
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
            commands::move_messages_to_folder,
            commands::undo_message_action,
            commands::get_setting,
            commands::set_setting,
            commands::all_settings,
            commands::begin_account_connection,
            commands::complete_password_imap,
            commands::complete_exchange_ews,
            commands::complete_yandex_oauth,
            commands::api_tools,
            commands::localization_catalog,
            commands::set_autostart,
            commands::get_autostart,
            commands::notify_open,
            commands::notify_close,
            commands::open_external_url,
            commands::notify_resize,
            commands::set_notify_position,
        ])
        .run(tauri::generate_context!())?;
    Ok(())
}
