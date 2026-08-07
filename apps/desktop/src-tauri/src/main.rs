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

/// Файлы из аргументов командной строки: проводник передаёт пути через пункт
/// "Отправить". Флаги (--hidden от автозапуска) и несуществующие пути
/// отбрасываем, первым аргументом идёт сам исполняемый файл.
fn attachment_args<I: IntoIterator<Item = String>>(args: I) -> Vec<String> {
    args.into_iter()
        .skip(1)
        .filter(|arg| !arg.starts_with('-'))
        .filter(|arg| std::path::Path::new(arg).is_file())
        .collect()
}

/// Завести пункт "Отправить -> truemail" один раз за установку. Работа идёт в
/// отдельном потоке: ярлык создаёт powershell, и зависший на чужой машине
/// процесс не должен задерживать показ окна.
///
/// Установщик добавляет его только при обычной установке: при обновлении
/// (updater запускает NSIS с /UPDATE) ярлыка ни у кого нет, и иначе фича не
/// доехала бы до тех, кто уже пользуется программой. Маркер в каталоге данных
/// гарантирует единственную попытку - удалённый пользователем пункт обратно не
/// возвращается. В отладочной сборке не трогаем: ярлык указывал бы на
/// target/debug вместо установленной программы.
#[cfg(all(windows, not(debug_assertions)))]
fn ensure_sendto_shortcut(app: &tauri::AppHandle) {
    // Маркер лежит в стандартном каталоге данных, а не в выбранном
    // пользователем: иначе смена каталога выглядела бы как первый запуск и
    // возвращала удалённый пункт. Деинсталлятор его убирает вместе с ярлыком.
    let marker = default_data_dir().join("sendto-initialized");
    if marker.exists() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || match commands::set_sendto_shortcut(app, true) {
        Ok(()) => {
            if let Some(parent) = marker.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&marker, "1");
            tracing::info!("добавлен пункт \"Отправить -> truemail\"");
        }
        Err(error) => tracing::warn!(error = %error.message, "пункт \"Отправить\" не создан"),
    });
}

/// Открыть зашифрованное хранилище. Пока ключей нет (первый запуск), ядра тоже
/// нет: его создаст мастер настройки после сбора энтропии.
fn open_core() -> anyhow::Result<Option<Arc<Core>>> {
    if !truemail_core::crypto::keys_initialized()? {
        return Ok(None);
    }
    Ok(Some(Arc::new(tauri::async_runtime::block_on(
        Core::bootstrap(data_dir()),
    )?)))
}

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
        show_startup_error(&error);
    }
}

/// Единственный способ сообщить об ошибке запуска: у релизной сборки нет
/// консоли (windows_subsystem = "windows"), поэтому без диалога пользователь
/// увидел бы просто не запустившуюся программу.
fn show_startup_error(error: &dyn std::fmt::Display) {
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

/// Потолок кучи JavaScript в процессе отрисовки WebView2, МБ. Интерфейс держит
/// в памяти только метаданные писем и разметку списка - реальный рабочий набор
/// это десятки мегабайт. Без потолка Chromium расширяет кучу под доступную
/// память машины и не возвращает её системе: у пользователя процесс отрисовки
/// разрастался до 1.3 ГБ при 92 МБ в самом приложении. С потолком сборщик
/// мусора начинает работать заметно раньше и лишнее возвращается.
#[cfg(windows)]
const WEBVIEW_JS_HEAP_LIMIT_MB: u32 = 384;

/// Аргументы процессу WebView2 задаются переменной окружения до создания окна -
/// иного способа их передать WebView2 не предоставляет. Это не жёсткая гарантия:
/// переменную окружения WebView2 игнорирует для приложений, запущенных с
/// повышением прав, и ограничивает она кучу JavaScript, а не весь процесс
/// отрисовки целиком. Основную экономию дают освобождение памяти при скрытом
/// окне и снятые утечки, а этот потолок заставляет сборщик мусора работать
/// раньше и возвращать освобождённое системе.
#[cfg(windows)]
fn limit_webview_memory() {
    const VAR: &str = "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS";
    let existing = std::env::var(VAR).unwrap_or_default();
    // Свой --js-flags снаружи уважаем целиком: пользователь настроил движок сам.
    if existing.contains("--js-flags") {
        tracing::info!("лимит памяти WebView2 задан снаружи, не трогаем");
        return;
    }
    // Ограничиваем только кучу JavaScript. Флаги вроде renderer-process-limit
    // WebView2 официально не поддерживает, а у приложения есть второе окно
    // (уведомления) - навязывать им общий процесс отрисовки значит делать общий
    // отказ на ровном месте. Прочие аргументы из окружения сохраняем.
    let mut args = format!("--js-flags=--max-old-space-size={WEBVIEW_JS_HEAP_LIMIT_MB}");
    if !existing.trim().is_empty() {
        args = format!("{} {args}", existing.trim());
    }
    // SAFETY: на Windows std::env::set_var потокобезопасен - переменные хранит
    // сама операционная система. Вызов идёт первым делом в run(), до запуска
    // рантайма и до создания окна, поэтому конкурирующего чтения нет и на
    // остальных платформах (там функция не компилируется вовсе).
    unsafe {
        std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", &args);
    }
    tracing::info!(args, "лимит памяти WebView2 задан");
}

fn run() -> anyhow::Result<()> {
    // Первым делом, до рантайма и до создания окна: WebView2 читает аргументы из
    // окружения на старте, а менять переменные окружения безопаснее всего пока
    // не запущены другие потоки.
    #[cfg(windows)]
    limit_webview_memory();

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
            eprintln!(
                "не удалось создать файловый логгер в {}: {error}; логи будут только в stdout",
                log_dir.display()
            );
        }
    }
    tracing::info!(log_dir = %log_dir.display(), "логирование инициализировано");

    // Ядро открываем не здесь, а в setup - после того, как плагин
    // single-instance завершит лишний процесс. Иначе каждый повторный запуск
    // (клик по ярлыку, "Отправить" из проводника) открывал бы ту же
    // SQLCipher-базу и гонял миграции со сборкой мусора параллельно с
    // работающей программой.
    let state = AppState {
        core: tokio::sync::RwLock::new(None),
        notify_anchor: Arc::new(std::sync::Mutex::new(
            commands::NotifyAnchor::platform_default(),
        )),
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
        notified_calendar_changes: Arc::new(tokio::sync::Mutex::new(
            std::collections::HashSet::new(),
        )),
        pending_attachments: Arc::new(std::sync::Mutex::new(attachment_args(std::env::args()))),
        allowed_attachments: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
        pending_update: Arc::new(tokio::sync::Mutex::new(None)),
        installing_update: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };
    tauri::Builder::default()
        // Должен быть первым плагином: второй процесс передаёт аргументы уже
        // работающему экземпляру и сразу завершается.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            show_main_window(app);
            // "Отправить -> truemail" на уже запущенной программе: второй
            // процесс отдаёт пути файлов сюда и выходит.
            // Файлы кладём в ту же очередь, что и при холодном старте, а
            // событие только будит интерфейс: если он ещё не подписался
            // (второй экземпляр успел раньше), очередь заберётся при загрузке.
            let files = attachment_args(args);
            if !files.is_empty() {
                let state = app.state::<AppState>();
                if let Ok(mut pending) = state.pending_attachments.lock() {
                    pending.extend(files);
                }
                let _ = app.emit("truemail-attach-files", ());
            }
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
            // На новой установке ядро создаст визард после сбора пользовательской
            // энтропии. На настроенной - открываем SQLCipher сразу. Лишний
            // процесс сюда не доходит: плагин single-instance завершает его на
            // своей инициализации, до этого хука.
            //
            // Ошибку здесь нельзя отдать через `?`: Tauri паникует на неудачном
            // setup внутри цикла событий, и у пользователя без консоли не
            // осталось бы никакого сообщения. Показываем тот же диалог, что и
            // при других сбоях запуска, и выходим.
            let core = match open_core() {
                Ok(core) => core,
                Err(error) => {
                    show_startup_error(&error);
                    std::process::exit(1);
                }
            };
            let initial_keybindings = core
                .as_ref()
                .and_then(|core| tauri::async_runtime::block_on(core.db.list_keybindings()).ok())
                .unwrap_or_else(commands::default_keybindings);
            {
                let state = app.state::<AppState>();
                // Куда показывать уведомления - читаем до создания окна:
                // позиционирование синхронное.
                if let Some(core) = core.as_ref() {
                    if let Ok(Some(value)) =
                        tauri::async_runtime::block_on(core.db.setting("notify_position"))
                    {
                        if let Ok(mut anchor) = state.notify_anchor.lock() {
                            *anchor = commands::NotifyAnchor::parse(&value);
                        }
                    }
                }
                tauri::async_runtime::block_on(async {
                    *state.core.write().await = core;
                });
            }
            // Уборка скачанных пакетов обновления. После установки программа
            // перезапускается уже новой версией - её инсталлятор здесь и
            // удаляется, как и всё, что осталось от прошлых версий и
            // недокачанных попыток.
            commands::cleanup_update_files();

            commands::register_global_shortcuts(app.handle(), &initial_keybindings)?;
            #[cfg(all(windows, not(debug_assertions)))]
            ensure_sendto_shortcut(app.handle());

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

            // Главное окно создаётся скрытым (visible: false в tauri.conf.json):
            // открытие SQLCipher выше блокирует поток, и показанное до него окно
            // висело бы серым "не отвечает". Показываем, когда всё готово, кроме
            // автозапуска с --hidden - тот стартует сразу свёрнутым в трей.
            // После обновления окно показываем всегда: установщик передаёт
            // аргументы прежнего процесса, и --hidden от автозапуска прятал бы
            // обновлённую программу обратно в трей.
            let after_update = commands::take_show_window_after_update(app.handle());
            if after_update || !std::env::args().any(|arg| arg == "--hidden") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
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
            commands::list_label_messages_page,
            commands::label_message_counts,
            commands::fetch_older_messages,
            commands::ui_log,
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
            commands::count_smart_folder_messages,
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
            commands::get_sendto_shortcut,
            commands::set_sendto_shortcut,
            commands::take_pending_attachments,
            commands::read_local_file,
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
