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
    listLabels: () => invoke("list_labels"),
    createLabel: (name, color) => invoke("create_label", { name, color }),
    updateLabel: (id, name, color) => invoke("update_label", { id, name, color }),
    deleteLabel: (id) => invoke("delete_label", { id }),
    toggleMessageLabel: (messageId, labelId, on) => invoke("toggle_message_label", { messageId, labelId, on }),
    messageLabelIds: (messageId) => invoke("message_label_ids", { messageId }),
    listFolders: (accountId) => invoke("list_folders", { accountId }),
    setFolderRole: (accountId, role, folderId) => invoke("set_folder_role", { accountId, role, folderId }),
    renameFolder: (folderId, newName) => invoke("rename_folder", { folderId, newName }),
    deleteFolder: (folderId) => invoke("delete_folder", { folderId }),
    listMessages: (folderId, limit) => invoke("list_messages", { folderId, limit }),
    listMessagesPage: (folderId, beforeDate, beforeId, limit = 100) => invoke("list_messages_page", { folderId, beforeDate, beforeId, limit }),
    getMessage: (messageId) => invoke("get_message", { messageId }),
    messageRaw: (messageId) => invoke("message_raw", { messageId }),
    exportMessageEml: (messageId, destPath) => invoke("export_message_eml", { messageId, destPath }),
    unsubscribeOneClick: (url) => invoke("unsubscribe_one_click", { url }),
    setAutostart: (enabled) => invoke("set_autostart", { enabled }),
    getAutostart: () => invoke("get_autostart"),
    attachmentContent: (messageId, attachmentId) => invoke("attachment_content", { messageId, attachmentId }),
    saveAttachment: (messageId, attachmentId, destPath) => invoke("save_attachment", { messageId, attachmentId, destPath }),
    saveAllAttachments: (messageId, destDir) => invoke("save_all_attachments", { messageId, destDir }),
    listSmartFolders: () => invoke("list_smart_folders"),
    saveSmartFolders: (folders) => invoke("save_smart_folders", { folders }),
    listSmartFolderMessages: (smartFolderId, limit = 5000) => invoke("list_smart_folder_messages", { smartFolderId, limit }),
    listUnifiedSources: () => invoke("list_unified_sources"),
    setUnifiedSource: (folderId, included) => invoke("set_unified_source", { folderId, included }),
    listMailRules: () => invoke("list_mail_rules"),
    saveMailRule: (rule, applyExisting) => invoke("save_mail_rule", { rule, applyExisting }),
    setMailRuleEnabled: (id, enabled) => invoke("set_mail_rule_enabled", { id, enabled }),
    deleteMailRule: (id) => invoke("delete_mail_rule", { id }),
    listContacts: (query) => invoke("list_contacts", { query }),
    search: (query) => invoke("search", { query }),
    listCalendarData: () => invoke("list_calendar_data"),
    createEvent: (accountId, calendarId, input) => invoke("create_event", { accountId, calendarId, input }),
    updateEvent: (eventId, input) => invoke("update_event", { eventId, input }),
    deleteEvent: (eventId) => invoke("delete_event", { eventId }),
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
    allSettings: () => invoke("all_settings"),
    setNotifyPosition: (value) => invoke("set_notify_position", { value }),
    openExternal: (url) => invoke("open_external_url", { url }),
    beginAccountConnection: (email) => invoke("begin_account_connection", { email }),
    completePasswordImap: (config) => invoke("complete_password_imap", config),
    completeExchangeEws: (config) => invoke("complete_exchange_ews", config),
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

  // Открытие письма по клику "Открыть" в своём уведомлении.
  tauri.event?.listen("truemail-open-message", async event => {
    const id = Number(event.payload);
    if (!Number.isFinite(id)) return;
    await window.reloadCoreData?.().catch(() => {});
    window.openMessageById?.(id);
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
    const unifiedSources = await window.tm.listUnifiedSources();
    window.coreUnifiedSettings = Object.fromEntries(unifiedSources.map(source=>[source.folder_id,source.included?'1':'0']));
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
      // Все настройки разом. Перечислять ключи здесь нельзя: забытый ключ -
      // молча не восстановленная настройка (так терялись show_conversations,
      // preview_lines, contacts_view, notify_position).
      const settings = await window.tm.allSettings();
      const onboardingCompleted = settings.onboarding_completed;
      const savedLocale = settings.locale;
      if (savedLocale && window.applyWizardLanguage) window.applyWizardLanguage(savedLocale, false);
      if (savedLocale && window.applyUiCatalog) window.applyUiCatalog(await window.tm.localizationCatalog(savedLocale));
      if (window.applyCoreSettings) window.applyCoreSettings(settings);
      await window.reloadMailRules?.();
      console.info("truemail: подключено к ядру, аккаунтов:", accounts.length);
      if (accounts.length === 0 && window.showEmptyMailbox) window.showEmptyMailbox();
      else await loadCoreData(accounts);
      if (onboardingCompleted === "true") showView("mailView");
      else if (window.showWizard) window.showWizard(4);
      if (accounts.length) {
        const releaseSnoozed = async () => {
          const released = await window.tm.releaseDueSnoozes();
          if (released) scheduleReload(0);
        };
        releaseSnoozed().catch(console.error);
        setInterval(() => releaseSnoozed().catch(console.error), 30000);
        window.tm.startRealtime().catch(console.error);
        window.tm.syncAccounts().catch(console.error);
        // Календарь/контакты/задачи тянем сразу при старте, а не только в 5-минутном
        // интервале ниже - иначе они не появляются до нескольких минут после запуска.
        window.tm.syncAuxiliaryAccounts().catch(console.error);
        // Фоновая синхронизация не блокирует запуск. Обновляем экран по мере
        // появления данных, не перезагружая весь WebView.
        [3000, 10000, 30000].forEach(delay => setTimeout(() => window.reloadCoreData().catch(console.error), delay));
        // DAV не имеет push-канала: обновляем календарь и контакты отдельно,
        // не перекачивая почту. Письма Yandex приходят через постоянный IMAP IDLE,
        // Gmail подтягивается этим 5-минутным sync (IMAP 993 у Gmail часто закрыт).
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
