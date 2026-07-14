// Мост между фронтендом и ядром truemail через Tauri invoke.

(function () {
  const tauri = window.__TAURI__;
  if (!tauri || !tauri.core) {
    console.error("truemail: ядро Tauri не подключено");
    return;
  }
  const invoke = tauri.core.invoke;
  if (window.clearDemoData) window.clearDemoData();

  // Единая точка доступа к ядру для остального фронтенда.
  window.tm = {
    bootstrapStatus: () => invoke("bootstrap_status"),
    initializeStorage: (dataDir, locale, entropy) => invoke("initialize_storage", { dataDir, locale, entropy }),
    chooseDataDir: (defaultPath) => tauri.dialog.open({ directory: true, multiple: false, defaultPath }),
    listAccounts: () => invoke("list_accounts"),
    listFolders: (accountId) => invoke("list_folders", { accountId }),
    setFolderRole: (accountId, role, folderId) => invoke("set_folder_role", { accountId, role, folderId }),
    listMessages: (folderId, limit) => invoke("list_messages", { folderId, limit }),
    listMessagesPage: (folderId, beforeDate, beforeId, limit = 100) => invoke("list_messages_page", { folderId, beforeDate, beforeId, limit }),
    getMessage: (messageId) => invoke("get_message", { messageId }),
    listSmartFolders: () => invoke("list_smart_folders"),
    listContacts: (query) => invoke("list_contacts", { query }),
    search: (query) => invoke("search", { query }),
    listCalendarData: () => invoke("list_calendar_data"),
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
    messageAction: (messageIds, action) => invoke("message_action", { messageIds, action }),
    undoMessageAction: (operationIds) => invoke("undo_message_action", { operationIds }),
    getSetting: (key) => invoke("get_setting", { key }),
    setSetting: (key, value) => invoke("set_setting", { key, value }),
    beginYandexOauth: (email) => invoke("begin_account_connection", { email }),
    completeYandexOauth: (state, code) => invoke("complete_yandex_oauth", { oauthState: state, code }),
    apiTools: () => invoke("api_tools"),
    localizationCatalog: (locale) => invoke("localization_catalog", { locale }),
  };
  tauri.event?.listen("truemail-global-shortcut", event => {
    const action = event.payload;
    if (action === "compose") document.getElementById("composeBtn")?.click();
    else if (action === "search") document.getElementById("searchBox")?.click();
  }).catch(console.error);

  let reloadTimer = null;
  function scheduleReload(delay = 250) {
    clearTimeout(reloadTimer);
    reloadTimer = setTimeout(() => window.reloadCoreData?.().catch(console.error), delay);
  }
  tauri.event?.listen("truemail-data-changed", () => scheduleReload()).catch(console.error);
  tauri.event?.listen("truemail-sync-state", event => window.handleSyncState?.(event.payload)).catch(console.error);
  tauri.event?.listen("truemail-storage-moved", async () => {
    await window.reloadCoreData?.();
    await window.tm.startRealtime();
  }).catch(console.error);

  async function loadCoreData(accounts) {
    const folders = await Promise.all(accounts.map(account => window.tm.listFolders(account.id)));
    const allFolders = folders.flat();
    const unifiedValues = await Promise.all(allFolders.map(folder => window.tm.getSetting(`unified_${folder.id}`)));
    window.coreUnifiedSettings = Object.fromEntries(allFolders.map((folder,index)=>[folder.id,unifiedValues[index]]));
    const messageGroups = await Promise.all(allFolders.map(folder => window.tm.listMessagesPage(folder.id, null, null, 100)));
    const [contacts, calendarData, smartFolders, storage] = await Promise.all([
      window.tm.listContacts(), window.tm.listCalendarData(), window.tm.listSmartFolders(), window.tm.storageStatus(),
    ]);
    window.renderCoreAccounts?.(accounts, folders, messageGroups.flat(), contacts, calendarData, smartFolders, storage);
  }
  window.reloadCoreData = async () => {
    const accounts = await window.tm.listAccounts();
    if (accounts.length) await loadCoreData(accounts);
  };

  // Первичная загрузка только реальных данных из ядра.
  (async () => {
    try {
      const bootstrap = await window.tm.bootstrapStatus();
      window.tmStorageReady = bootstrap.ready;
      window.tmDefaultDataDir = bootstrap.data_dir;
      if (window.configureStorageWizard) window.configureStorageWizard(bootstrap);
      if (!bootstrap.ready) {
        if (window.showWizard) window.showWizard(1);
        return;
      }
      const accounts = await window.tm.listAccounts();
      const onboardingCompleted = await window.tm.getSetting("onboarding_completed");
      const settingKeys = ["locale", "theme", "density", "accent", "expert_mode", "sidebar_width", "ui_scale", "toolbar_layout", "smart_folders_ui", "composer_draft", "search_history"];
      const settingValues = await Promise.all(settingKeys.map(key => window.tm.getSetting(key)));
      const settings = Object.fromEntries(settingKeys.map((key, index) => [key, settingValues[index]]));
      const savedLocale = settings.locale;
      if (savedLocale && window.applyWizardLanguage) window.applyWizardLanguage(savedLocale, false);
      if (savedLocale && window.applyUiCatalog) window.applyUiCatalog(await window.tm.localizationCatalog(savedLocale));
      if (window.applyCoreSettings) window.applyCoreSettings(settings);
      console.info("truemail: подключено к ядру, аккаунтов:", accounts.length);
      if (accounts.length === 0 && window.showEmptyMailbox) window.showEmptyMailbox();
      else await loadCoreData(accounts);
      if (onboardingCompleted === "true") showView("mailView");
      else if (window.showWizard) window.showWizard(4);
      if (accounts.length) {
        window.tm.startRealtime().catch(console.error);
        window.tm.syncAccounts().catch(console.error);
        // Фоновая синхронизация не блокирует запуск. Обновляем экран по мере
        // появления данных, не перезагружая весь WebView.
        [3000, 10000, 30000].forEach(delay => setTimeout(() => window.reloadCoreData().catch(console.error), delay));
        // DAV не имеет push-канала: обновляем календарь и контакты отдельно,
        // не перекачивая почту. Письма приходят через постоянный IMAP IDLE.
        setInterval(() => {
          window.tm.syncAccounts().catch(console.error);
          window.tm.syncAuxiliaryAccounts().catch(console.error);
        }, 5 * 60 * 1000);
        document.addEventListener("visibilitychange", () => {
          if (document.visibilityState === "visible") {
            window.tm.syncAuxiliaryAccounts().catch(console.error);
            scheduleReload(100);
          }
        });
      }
    } catch (e) {
      console.error("truemail bridge:", e);
    }
  })();
})();
