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
use tauri_plugin_global_shortcut::ShortcutState;
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
    // файл единственный способ увидеть диагностику. В отладочной сборке
    // по умолчанию включаем debug для ядра, чтобы IMAP-операции (удаление
    // папок и т.п.) логировались из коробки. В релизе дефолт - info: debug
    // для всего ядра в проде слишком шумный и может писать лишние детали.
    // RUST_LOG пользователя в любом случае имеет приоритет над этим дефолтом.
    use tracing_appender::rolling::{Builder, Rotation};
    use tracing_subscriber::fmt::writer::MakeWriterExt;
    let log_dir = data_dir().join("logs");
    let _ = std::fs::create_dir_all(&log_dir);
    // Ротация суточная, но без ограничения файлы копились бесконечно (у
    // пользователя за 6 дней набежало 4+ МБ). Храним последнюю неделю.
    let file_appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("truemail.log")
        .max_log_files(7)
        .build(&log_dir);
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        if cfg!(debug_assertions) {
            tracing_subscriber::EnvFilter::new("info,truemail_core=debug")
        } else {
            tracing_subscriber::EnvFilter::new("info")
        }
    });
    match file_appender {
        Ok(file_appender) => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_ansi(false)
                .with_writer(std::io::stdout.and(file_appender))
                .init();
        }
        Err(error) => {
            // Логирование не критично для работы приложения: если файловый
            // appender не поднялся (например, нет прав на директорию),
            // не роняем запуск, а продолжаем писать хотя бы в stdout.
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_ansi(false)
                .init();
            eprintln!("не удалось создать файловый логгер в {}: {error}; логи будут только в stdout", log_dir.display());
        }
    }
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
    let initial_keybindings = core
        .as_ref()
        .and_then(|core| rt.block_on(core.db.list_keybindings()).ok())
        .unwrap_or_else(commands::default_keybindings);
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
        api_server: Arc::new(tokio::sync::Mutex::new(None)),
        shortcut_actions: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        notified_messages: Arc::new(tokio::sync::Mutex::new(std::collections::HashSet::new())),
        notified_calendar_changes: Arc::new(tokio::sync::Mutex::new(std::collections::HashSet::new())),
    };
    tauri::Builder::default()
        // Должен быть первым плагином: второй процесс передаёт аргументы уже
        // работающему экземпляру и сразу завершается.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    let action = app
                        .state::<AppState>()
                        .shortcut_actions
                        .read()
                        .ok()
                        .and_then(|actions| actions.get(&shortcut.to_string()).cloned());
                    let Some(action) = action else { return };
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
        .plugin(tauri_plugin_updater::Builder::new().build())
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
            commands::register_global_shortcuts(app.handle(), &initial_keybindings)?;

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

            // Не задерживаем старт и не пугаем сетевой ошибкой: при появлении
            // подписанного релиза UI сам предложит установить новую версию.
            // Проверяем через 8 с после запуска и далее периодически, чтобы
            // обновление находилось само и без перезапуска приложения.
            let update_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(8)).await;
                loop {
                    if let Err(error) =
                        commands::announce_available_update(update_app.clone()).await
                    {
                        tracing::debug!(error = %error.message, "автопроверка обновлений пропущена");
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(6 * 3600)).await;
                }
            });
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
            commands::create_folder,
            commands::rename_folder,
            commands::delete_folder,
            commands::list_messages,
            commands::list_messages_page,
            commands::get_message,
            commands::message_raw,
            commands::export_message_eml,
            commands::unsubscribe_one_click,
            commands::attachment_content,
            commands::save_attachment,
            commands::save_all_attachments,
            commands::list_smart_folders,
            commands::save_smart_folders,
            commands::list_smart_folder_messages,
            commands::list_unified_sources,
            commands::set_unified_source,
            commands::list_mail_rules,
            commands::save_mail_rule,
            commands::set_mail_rule_enabled,
            commands::delete_mail_rule,
            commands::list_contacts,
            commands::search,
            commands::list_calendar_data,
            commands::set_calendar_visible,
            commands::create_event,
            commands::update_event,
            commands::delete_event,
            commands::respond_to_event,
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
            commands::mark_flagged,
            commands::snooze_messages,
            commands::unsnooze_messages,
            commands::release_due_snoozes,
            commands::list_signatures,
            commands::save_signature,
            commands::list_message_templates,
            commands::save_message_template,
            commands::delete_message_template,
            commands::message_action,
            commands::move_messages_to_folder,
            commands::undo_message_action,
            commands::get_setting,
            commands::set_setting,
            commands::list_keybindings,
            commands::set_keybinding,
            commands::image_sender_trusted,
            commands::set_image_sender_trusted,
            commands::all_settings,
            commands::begin_account_connection,
            commands::complete_password_imap,
            commands::complete_exchange_ews,
            commands::complete_jmap,
            commands::complete_yandex_oauth,
            commands::api_tools,
            commands::external_api_status,
            commands::start_external_api,
            commands::stop_external_api,
            commands::list_api_clients,
            commands::create_api_client,
            commands::revoke_api_client,
            commands::list_api_audit,
            commands::clear_api_audit,
            commands::check_for_update,
            commands::install_update,
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

#[cfg(test)]
mod command_contract_tests {
    const MAIN: &str = include_str!("main.rs");
    const COMMANDS: &str = include_str!("commands.rs");
    const BRIDGE: &str = include_str!("../../ui/bridge.js");

    #[test]
    fn critical_user_flows_are_exposed_by_tauri_and_the_ui_bridge() {
        for command in [
            "get_message",
            "send_message",
            "create_event",
            "update_event",
            "delete_event",
            "respond_to_event",
            "create_contact",
            "update_contact",
            "delete_contact",
            "message_action",
            "move_messages_to_folder",
            "undo_message_action",
            "check_for_update",
            "install_update",
        ] {
            assert!(
                MAIN.contains(&format!("commands::{command}")),
                "{command} is missing from generate_handler"
            );
            assert!(
                COMMANDS.contains(&format!("fn {command}(")),
                "{command} implementation is missing"
            );
            assert!(
                BRIDGE.contains(&format!("invoke(\"{command}\"")),
                "{command} is missing from bridge.js"
            );
        }
    }

    #[test]
    fn single_instance_is_registered_before_every_other_plugin() {
        let builder = MAIN
            .find("tauri::Builder::default()")
            .expect("Tauri builder missing");
        let single = MAIN
            .find(".plugin(tauri_plugin_single_instance::init")
            .expect("single-instance plugin missing");
        let first_plugin = MAIN[builder..]
            .find(".plugin(")
            .map(|offset| builder + offset)
            .expect("plugin registration missing");
        assert_eq!(single, first_plugin);
    }
}
