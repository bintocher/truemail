// Мост между фронтендом и ядром truemail через Tauri invoke.

// Размер страницы писем на папку при полной перезагрузке данных. Список писем
// сверяется с этим числом, когда решает, исчезло письмо из папки или просто не
// попало в страницу, - поэтому значение общее, а не локальное.
window.corePageSize = 100;

(function () {
  const tauri = window.__TAURI__;
  if (!tauri || !tauri.core) {
    console.error("truemail: ядро Tauri не подключено");
    return;
  }
  const invoke = tauri.core.invoke;
  if (window.clearDemoData) window.clearDemoData();

  // Настройки приезжают уже после того, как окно показано: до этого поведение
  // кнопок, зависящих от них, неизвестно. Промис даёт таким обработчикам
  // дождаться настоящего значения вместо догадки.
  let settingsLoaded;
  window.tmSettingsReady = new Promise(resolve => { settingsLoaded = resolve; });
  window.markSettingsLoaded = () => settingsLoaded();

  // Единая точка доступа к ядру для остального фронтенда.
  window.tm = {
    bootstrapStatus: () => invoke("bootstrap_status"),
    initializeStorage: (dataDir, locale, entropy) => invoke("initialize_storage", { dataDir, locale, entropy }),
    exportKeyBackup: (path, password) => invoke("export_key_backup", { path, password }),
    restoreKeyBackup: (dataDir, backupPath, password) => invoke("restore_key_backup", { dataDir, backupPath, password }),
    chooseDataDir: (defaultPath) => tauri.dialog.open({ directory: true, multiple: false, defaultPath }),
    chooseDir: (defaultPath) => tauri.dialog.open({ directory: true, multiple: false, defaultPath }),
    saveFileDialog: (defaultPath) => tauri.dialog.save({ defaultPath }),
    chooseKeyBackup: (defaultPath) => tauri.dialog.open({ directory: false, multiple: false, defaultPath, filters: [{ name: "truemail key backup", extensions: ["tmkeys"] }] }),
    listAccounts: () => invoke("list_accounts"),
    renameAccount: (accountId, displayName) => invoke("rename_account", { accountId, displayName }),
    setAccountColor: (accountId, color) => invoke("set_account_color", { accountId, color }),
    setAccountRetention: (accountId, days) => invoke("set_account_retention", { accountId, days }),
    changeAccountPassword: (accountId, password) => invoke("change_account_password", { accountId, password }),
    listLabels: () => invoke("list_labels"),
    createLabel: (name, color) => invoke("create_label", { name, color }),
    updateLabel: (id, name, color) => invoke("update_label", { id, name, color }),
    deleteLabel: (id) => invoke("delete_label", { id }),
    toggleMessageLabel: (messageId, labelId, on) => invoke("toggle_message_label", { messageId, labelId, on }),
    messageLabelIds: (messageId) => invoke("message_label_ids", { messageId }),
    listFolders: (accountId) => invoke("list_folders", { accountId }),
    setFolderRole: (accountId, role, folderId) => invoke("set_folder_role", { accountId, role, folderId }),
    createFolder: (accountId, parentFolderId, name) => invoke("create_folder", { accountId, parentFolderId, name }),
    renameFolder: (folderId, newName) => invoke("rename_folder", { folderId, newName }),
    deleteFolder: (folderId) => invoke("delete_folder", { folderId }),
    listMessages: (folderId, limit) => invoke("list_messages", { folderId, limit }),
    listMessagesPage: (folderId, beforeDate, beforeId, limit = 100) => invoke("list_messages_page", { folderId, beforeDate, beforeId, limit }),
    listLabelMessagesPage: (label, beforeDate, beforeId, limit = 100) => invoke("list_label_messages_page", { label, beforeDate, beforeId, limit }),
    labelMessageCounts: () => invoke("label_message_counts"),
    fetchOlderMessages: (folderId, before, limit = 500) => invoke("fetch_older_messages", { folderId, before, limit }),
    uiLog: (message) => invoke("ui_log", { message }).catch(() => {}),
    getMessage: (messageId) => invoke("get_message", { messageId }),
    messageRaw: (messageId) => invoke("message_raw", { messageId }),
    exportMessageEml: (messageId, destPath) => invoke("export_message_eml", { messageId, destPath }),
    unsubscribeOneClick: (url) => invoke("unsubscribe_one_click", { url }),
    setAutostart: (enabled) => invoke("set_autostart", { enabled }),
    getAutostart: () => invoke("get_autostart"),
    getSendtoShortcut: () => invoke("get_sendto_shortcut"),
    setSendtoShortcut: (enabled) => invoke("set_sendto_shortcut", { enabled }),
    takePendingAttachments: () => invoke("take_pending_attachments"),
    readLocalFile: (path) => invoke("read_local_file", { path }),
    attachmentContent: (messageId, attachmentId) => invoke("attachment_content", { messageId, attachmentId }),
    saveAttachment: (messageId, attachmentId, destPath) => invoke("save_attachment", { messageId, attachmentId, destPath }),
    saveAllAttachments: (messageId, destDir) => invoke("save_all_attachments", { messageId, destDir }),
    listSmartFolders: () => invoke("list_smart_folders"),
    saveSmartFolders: (folders) => invoke("save_smart_folders", { folders }),
    listSmartFolderMessages: (smartFolderId, beforeDate = null, beforeId = null, limit = 500) => invoke("list_smart_folder_messages", { smartFolderId, beforeDate, beforeId, limit }),
    countSmartFolderMessages: (smartFolderIds) => invoke("count_smart_folder_messages", { smartFolderIds }),
    listUnifiedSources: () => invoke("list_unified_sources"),
    setUnifiedSource: (folderId, included) => invoke("set_unified_source", { folderId, included }),
    listMailRules: () => invoke("list_mail_rules"),
    saveMailRule: (rule, applyExisting) => invoke("save_mail_rule", { rule, applyExisting }),
    setMailRuleEnabled: (id, enabled) => invoke("set_mail_rule_enabled", { id, enabled }),
    deleteMailRule: (id) => invoke("delete_mail_rule", { id }),
    listContacts: (query) => invoke("list_contacts", { query }),
    search: (query) => invoke("search", { query }),
    listCalendarData: () => invoke("list_calendar_data"),
    setCalendarVisible: (calendarId, visible) => invoke("set_calendar_visible", { calendarId, visible }),
    createEvent: (accountId, calendarId, input) => invoke("create_event", { accountId, calendarId, input }),
    updateEvent: (eventId, input) => invoke("update_event", { eventId, input }),
    deleteEvent: (eventId) => invoke("delete_event", { eventId }),
    respondToEvent: (eventId, response) => invoke("respond_to_event", { eventId, response }),
    createContact: (accountId, input) => invoke("create_contact", { accountId, input }),
    updateContact: (contactId, input) => invoke("update_contact", { contactId, input }),
    deleteContact: (contactId) => invoke("delete_contact", { contactId }),
    storageStatus: () => invoke("storage_status"),
    moveStorage: (target) => invoke("move_storage", { target }),
    openDataDir: () => invoke("open_data_dir"),
    clearLocalData: (scope) => invoke("clear_local_data", { scope }),
    syncAccounts: () => invoke("sync_accounts"),
    syncAuxiliaryAccounts: () => invoke("sync_auxiliary_accounts"),
    startRealtime: () => invoke("start_realtime"),
    sendMessage: (request) => invoke("send_message", { request }),
    scheduleMessage: (request, sendAt) => invoke("schedule_message", { request, sendAt }),
    markSeen: (messageId, seen) => invoke("mark_seen", { messageId, seen }),
    markFlagged: (messageId, flagged) => invoke("mark_flagged", { messageId, flagged }),
    snoozeMessages: (messageIds, until) => invoke("snooze_messages", { messageIds, until }),
    unsnoozeMessages: (messageIds) => invoke("unsnooze_messages", { messageIds }),
    releaseDueSnoozes: () => invoke("release_due_snoozes"),
    listSignatures: (accountId) => invoke("list_signatures", { accountId }),
    saveSignature: (accountId, kind, bodyHtml, enabled) => invoke("save_signature", { accountId, kind, bodyHtml, enabled }),
    listMessageTemplates: (accountId) => invoke("list_message_templates", { accountId }),
    saveMessageTemplate: (template) => invoke("save_message_template", template),
    deleteMessageTemplate: (id, accountId) => invoke("delete_message_template", { id, accountId }),
    messageAction: (messageIds, action) => invoke("message_action", { messageIds, action }),
    moveMessagesToFolder: (messageIds, folderId) => invoke("move_messages_to_folder", { messageIds, folderId }),
    undoMessageAction: (operationIds) => invoke("undo_message_action", { operationIds }),
    getSetting: (key) => invoke("get_setting", { key }),
    setSetting: (key, value) => invoke("set_setting", { key, value }),
    listKeybindings: () => invoke("list_keybindings"),
    setKeybinding: (action, combo) => invoke("set_keybinding", { action, combo }),
    imageSenderTrusted: (sender) => invoke("image_sender_trusted", { sender }),
    setImageSenderTrusted: (sender, allow) => invoke("set_image_sender_trusted", { sender, allow }),
    allSettings: () => invoke("all_settings"),
    setNotifyPosition: (value) => invoke("set_notify_position", { value }),
    openExternal: (url) => invoke("open_external_url", { url }),
    beginAccountConnection: (email) => invoke("begin_account_connection", { email }),
    completePasswordImap: (config) => invoke("complete_password_imap", config),
    completeExchangeEws: (config) => invoke("complete_exchange_ews", config),
    completeJmap: (config) => invoke("complete_jmap", config),
    beginYandexOauth: (email) => invoke("begin_account_connection", { email }),
    completeYandexOauth: (state, code) => invoke("complete_yandex_oauth", { oauthState: state, code }),
    apiTools: () => invoke("api_tools"),
    externalApiStatus: () => invoke("external_api_status"),
    startExternalApi: (port) => invoke("start_external_api", { port }),
    stopExternalApi: () => invoke("stop_external_api"),
    listApiClients: () => invoke("list_api_clients"),
    createApiClient: (name, caps) => invoke("create_api_client", { name, caps }),
    revokeApiClient: (clientId) => invoke("revoke_api_client", { clientId }),
    listApiAudit: (limit = 50) => invoke("list_api_audit", { limit }),
    clearApiAudit: () => invoke("clear_api_audit"),
    checkForUpdate: () => invoke("check_for_update"),
    installUpdate: () => invoke("install_update"),
  };
  // Проверка обновлений идёт по кругу каждые 6 часов, и без этой памяти одно
  // и то же предложение всплывало бы снова и снова, пока пользователь не
  // обновится. Помним версию, а не факт показа: на следующую версию тост
  // появится опять. Память живёт в сессии - после перезапуска напомнить один
  // раз уместно.
  let offeredVersion = null;
  const offerUpdate = info => {
    if (!info?.available_version) return;
    // Кнопка в строке заголовка остаётся на виду, даже когда уведомление уже
    // пропало: иначе о новой версии узнать можно было только из настроек.
    window.showUpdateButton?.(info.available_version, info.downloaded);
    if (offeredVersion === info.available_version) return;
    offeredVersion = info.available_version;
    const message = wizardLocale === "en" ? `truemail ${info.available_version} is available` : `Доступен truemail ${info.available_version}`;
    showToast(message, L("Обновить", "Update"), async () => {
      const status = document.getElementById("updateStatus");
      if (status) status.textContent = L("Скачиваю и устанавливаю обновление…", "Downloading and installing the update…");
      // Через ту же точку, что и кнопка в строке заголовка: обе должны видеть
      // общее состояние установки.
      if (window.startUpdateInstall) await window.startUpdateInstall();
      else await window.tm.installUpdate();
    });
  };
  tauri.event?.listen("truemail-update-available", event => offerUpdate(event.payload)).catch(console.error);
  tauri.event?.listen("truemail-update-progress", event => {
    const status = document.getElementById("updateStatus"), progress = event.payload;
    if (!status || !progress) return;
    if (progress.event === "finished") status.textContent = L("Обновление скачано, запускаю установку…", "Update downloaded, starting installation…");
    else if (progress.total) {
      const percent = Math.min(100, Math.round(progress.downloaded / progress.total * 100));
      status.textContent = L(`Скачано ${percent}%`, `Downloaded ${percent}%`);
    }
  }).catch(console.error);
  tauri.event?.listen("truemail-global-shortcut", event => {
    const action = event.payload;
    if (action === "compose") document.getElementById("composeBtn")?.click();
    else if (action === "search") document.getElementById("searchBox")?.click();
  }).catch(console.error);

  // "Отправить -> truemail" в проводнике: пути файлов приходят аргументами
  // процесса и складываются в очередь ядра. Событие лишь сообщает, что очередь
  // пополнил второй экземпляр программы.
  tauri.event?.listen("truemail-attach-files", () => {
    window.consumePendingAttachments?.();
  }).catch(console.error);

  // Очередь файлов из аргументов запуска живёт в ядре, пока её не заберут:
  // пока мастер настройки не завершён или нет ни одного аккаунта, письмо
  // открывать некуда - файлы ждут в очереди, композер их не выдернет.
  window.consumePendingAttachments = function () {
    if (!window.tmComposerReady) return Promise.resolve();
    return window.tm.takePendingAttachments()
      .then(paths => { if (paths.length) window.composeWithFiles?.(paths); })
      .catch(console.error);
  };

  // Открытие письма по клику "Открыть" в своём уведомлении.
  tauri.event?.listen("truemail-open-message", async event => {
    const id = Number(event.payload);
    if (!Number.isFinite(id)) return;
    try { await window.reloadCoreData?.(); } catch (_) {}
    await window.openMessageById?.(id);
  }).catch(console.error);

  // Переход в календарь по клику "Открыть" в карточке изменения встречи.
  tauri.event?.listen("truemail-open-event", event => {
    const payload = event.payload || {};
    window.openCalendarEventById?.(payload.event_id ?? null, payload.start ?? null);
  }).catch(console.error);

  // Полная перезагрузка данных - дорогая операция: запрос страницы писем по
  // каждой папке каждого аккаунта плюс контакты, календари и умные папки. При
  // активной синхронизации событий truemail-data-changed приходят десятки, и без
  // ограничения снизу перезагрузки шли непрерывно. Держим минимальный интервал и
  // не трогаем данные, пока окно скрыто: за свёрнутым окном обновлять список
  // некому, а вернувшись, пользователь получит свежие данные сразу.
  const MIN_RELOAD_INTERVAL = 5000;
  let reloadTimer = null, lastReloadAt = 0, reloadPending = false, reloadRunning = false;
  async function runReload() {
    reloadTimer = null;
    // Пока окно скрыто, обновлять список некому - помечаем и вернёмся к этому,
    // когда окно снова покажут.
    if (document.hidden) { reloadPending = true; return; }
    // Перезагрузка идёт - вторую параллельно не пускаем: они завершались бы в
    // произвольном порядке, и более старый ответ мог затереть свежий список.
    if (reloadRunning) { reloadPending = true; return; }
    reloadRunning = true;
    reloadPending = false;
    try { await window.reloadCoreData?.(); }
    catch (e) { console.error(e); }
    finally {
      reloadRunning = false;
      lastReloadAt = Date.now();
      if (reloadPending) scheduleReload(0);
    }
  }
  // Именно throttle, а не debounce: уже назначенный запуск не переносим, иначе
  // непрерывный поток событий синхронизации откладывал бы обновление
  // бесконечно.
  function scheduleReload(delay = 250) {
    if (reloadTimer) return;
    const wait = Math.max(delay, MIN_RELOAD_INTERVAL - (Date.now() - lastReloadAt));
    reloadTimer = setTimeout(() => { runReload().catch(console.error); }, Math.max(0, wait));
  }
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) {
      // Окно скрыто: держать в памяти разметку списка и накопленные страницы
      // незачем. Освобождаем и помечаем, что при возврате данные надо перечитать.
      window.releaseHiddenMemory?.();
      reloadPending = true;
      return;
    }
    // Сначала рисуем из того, что осталось в памяти - список появляется сразу,
    // а не через паузу троттлинга перезагрузки.
    window.restoreAfterHidden?.();
    if (reloadPending) scheduleReload(0);
  });
  tauri.event?.listen("truemail-data-changed", () => scheduleReload()).catch(console.error);
  tauri.event?.listen("truemail-sync-state", event => window.handleSyncState?.(event.payload)).catch(console.error);
  tauri.event?.listen("truemail-storage-moved", async () => {
    await window.reloadCoreData?.();
    await window.tm.startRealtime();
  }).catch(console.error);

  async function loadCoreData(accounts) {
    const folders = await Promise.all(accounts.map(account => window.tm.listFolders(account.id)));
    const allFolders = folders.flat();
    const unifiedSources = await window.tm.listUnifiedSources();
    window.coreUnifiedSettings = Object.fromEntries(unifiedSources.map(source=>[source.folder_id,source.included?'1':'0']));
    const messageGroups = await Promise.all(allFolders.map(folder => window.tm.listMessagesPage(folder.id, null, null, window.corePageSize)));
    const [contacts, calendarData, smartFolders, storage] = await Promise.all([
      window.tm.listContacts(), window.tm.listCalendarData(), window.tm.listSmartFolders(), window.tm.storageStatus(),
    ]);
    window.renderCoreAccounts?.(accounts, folders, messageGroups.flat(), contacts, calendarData, smartFolders, storage);
  }
  // Перезагрузки выстраиваем в очередь: reloadCoreData зовут и обработчик
  // событий, и модули после действий пользователя. Параллельные проходы
  // завершались бы в произвольном порядке, и более старый ответ мог перерисовать
  // список поверх свежего.
  let coreReloadChain = null;
  window.reloadCoreData = () => {
    // Окно скрыто - данные читать некому: список всё равно очищен ради памяти, а
    // перезагрузка наполнила бы его заново. Прямые вызовы приходят и из таймеров
    // первых секунд после запуска, поэтому проверка тут, а не только в runReload.
    if (document.hidden) { reloadPending = true; return Promise.resolve(); }
    const run = async () => {
      const accounts = await window.tm.listAccounts();
      if (accounts.length) await loadCoreData(accounts);
    };
    const chained = coreReloadChain ? coreReloadChain.then(run, run) : run();
    coreReloadChain = chained;
    chained.catch(() => {}).then(() => { if (coreReloadChain === chained) coreReloadChain = null; });
    return chained;
  };

  // Первичная загрузка только реальных данных из ядра.
  (async () => {
    try {
      await window.localizationReady;
      const bootstrap = await window.tm.bootstrapStatus();
      window.tmStorageReady = bootstrap.ready;
      window.tmDefaultDataDir = bootstrap.data_dir;
      if (window.configureStorageWizard) window.configureStorageWizard(bootstrap);
      if (!bootstrap.ready) {
        if (window.showWizard) window.showWizard(1);
        return;
      }
      const accounts = await window.tm.listAccounts();
      // Все настройки разом. Перечислять ключи здесь нельзя: забытый ключ -
      // молча не восстановленная настройка (так терялись show_conversations,
      // preview_lines, contacts_view, notify_position).
      const settings = await window.tm.allSettings();
      await window.refreshKeybindings?.();
      const onboardingCompleted = settings.onboarding_completed;
      // Выставляем до загрузки данных: renderCoreAccounts по этому флагу решает,
      // можно ли открывать композер для файлов из меню "Отправить".
      window.tmOnboardingDone = onboardingCompleted === "true";
      const savedLocale = settings.locale;
      if (savedLocale && window.applyWizardLanguage) window.applyWizardLanguage(savedLocale, false);
      if (window.applyCoreSettings) window.applyCoreSettings(settings);
      window.markSettingsLoaded?.();
      await window.reloadMailRules?.();
      console.info("truemail: подключено к ядру, аккаунтов:", accounts.length);
      if (accounts.length === 0 && window.showEmptyMailbox) window.showEmptyMailbox();
      // Стартовая загрузка тоже наполняет список: пока она идёт, освобождение
      // памяти при скрытом окне должно её дождаться, иначе очистка сработает
      // впустую и список тут же наполнится заново.
      else {
        try { await loadCoreData(accounts); }
        finally {
          // Запуск свёрнутым (автозагрузка в трей) или сворачивание прямо во
          // время стартовой загрузки: разметка успела построиться в скрытом окне,
          // поэтому освобождаем сразу - обработчик сворачивания это пропустил.
          if (document.hidden) { window.releaseHiddenMemory?.(); reloadPending = true; }
        }
      }
      if (onboardingCompleted === "true") showView("mailView");
      else if (window.showWizard) window.showWizard(4);
      // Запуск из меню "Отправить": файлы ждали в ядре, пока грузился интерфейс.
      // Только на настроенной программе - в визарде композер открывать некуда,
      // поэтому очередь остаётся в ядре, а забирает её finishOnboarding.
      window.tmComposerReady = onboardingCompleted === "true" && accounts.length > 0;
      window.consumePendingAttachments();
      if (accounts.length) {
        const releaseSnoozed = async () => {
          const released = await window.tm.releaseDueSnoozes();
          if (released) scheduleReload(0);
        };
        releaseSnoozed().catch(console.error);
        setInterval(() => releaseSnoozed().catch(console.error), 30000);
        window.tm.startRealtime().catch(console.error);
        window.tm.syncAccounts().catch(console.error);
        // Фоновая синхронизация не блокирует запуск. Обновляем экран по мере
        // появления данных, не перезагружая весь WebView.
        [3000, 10000, 30000].forEach(delay => setTimeout(() => window.reloadCoreData().catch(console.error), delay));
        // DAV не имеет push-канала: обновляем календарь и контакты отдельно,
        // не перекачивая почту. Письма Yandex приходят через постоянный IMAP IDLE.
        // Gmail проверяет новые ID каждые 25 секунд, а этот проход подхватывает
        // изменения ярлыков/удаления, которые не создали новое входящее письмо.
        setInterval(() => {
          window.tm.syncAccounts().catch(console.error);
        }, 5 * 60 * 1000);
        document.addEventListener("visibilitychange", () => {
          if (document.visibilityState === "visible") {
            // Только фоновая синхронизация. Полную перезагрузку списка тут НЕ
            // делаем: она сбрасывала список на первую страницу и теряла и
            // догруженные письма, и позицию прокрутки при возврате в окно
            // (alt-tab). Новые письма подтянет realtime и периодический sync.
            window.tm.syncAuxiliaryAccounts().catch(console.error);
          }
        });
      }
    } catch (e) {
      console.error("truemail bridge:", e);
    } finally {
      // Промис готовности настроек обязан разрешиться на любом исходе: на его
      // ожидании висят обработчики (кнопка сворачивания), и незавершённый
      // промис оставил бы их неработающими до перезапуска. На первом запуске и
      // при сбое загрузки в силе документированные значения по умолчанию.
      window.markSettingsLoaded?.();
    }
  })();
})();

/* Диагностика интерфейса. WebView при исчерпании памяти закрывает страницу
   молча: в журнале ядра не остаётся ни строки, и после падения не по чем
   искать причину. Поэтому пишем сами - ошибки страницы сразу, а замер памяти
   регулярно, чтобы последняя строка перед обрывом показывала, до чего дошло. */
(() => {
  const log = message => window.tm?.uiLog?.(message);
  const short = value => String(value ?? "").slice(0, 300);
  window.addEventListener("error", event => {
    log(`ошибка интерфейса: ${short(event.message)} (${short(event.filename)}:${event.lineno||0})`);
  });
  window.addEventListener("unhandledrejection", event => {
    log(`необработанный отказ: ${short(event.reason?.message || event.reason)}`);
  });
  const MEGABYTE = 1048576;
  let lastReported = 0;
  const measure = (reason) => {
    const memory = performance?.memory;
    const rows = document.querySelectorAll(".msg").length;
    if (!memory) { log(`память интерфейса (${reason}): строк списка ${rows}, замер кучи недоступен`); return; }
    const used = Math.round(memory.usedJSHeapSize / MEGABYTE);
    const limit = Math.round(memory.jsHeapSizeLimit / MEGABYTE);
    // Пишем каждый десятый замер (раз в 10 минут) и всякий раз, когда куча
    // подросла на 64 МБ с прошлой записи или подошла к потолку - у падения
    // из-за памяти в журнале останется нарастающий след, а не ровная строка.
    const grew = used - lastReported >= 64;
    const nearLimit = limit > 0 && used > limit * 0.8;
    if (reason !== "периодический" || grew || nearLimit || lastReported === 0) {
      lastReported = used;
      log(`память интерфейса (${reason}): куча ${used} из ${limit} МБ, строк списка ${rows}${nearLimit ? " - близко к потолку" : ""}`);
    }
  };
  let ticks = 0;
  setInterval(() => { ticks += 1; measure(ticks % 10 === 0 ? "каждые 10 минут" : "периодический"); }, 60000);
  setTimeout(() => measure("старт"), 5000);
})();
