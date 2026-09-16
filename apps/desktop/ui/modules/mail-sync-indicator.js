// truemail UI module: mail-sync-indicator.js
// Чистые функции без DOM и Tauri API: выбор показываемого признака состояния
// синхронизации почты аккаунта по приоритету и учёт переходных статусов
// событий truemail-sync-state. Подключается в index.html как обычный скрипт,
// перед mail.js и settings.js, и отдаёт функции через глобальный объект.
// См. specs/mail-sync-visible-state.md.
'use strict';

// Область события относится к почте (границы задачи): "all" - цикл
// sync_accounts и первая синхронизация после подключения, "mail" -
// наблюдатель за новыми письмами (кроме Gmail). "auxiliary"/"dav" - календарь,
// контакты и задачи - у них своя строка #calSyncInfo, вне этой задачи.
function isMailSyncScope(scope) {
  return scope === 'all' || scope === 'mail';
}

// Переходные статусы по аккаунтам (S-011): "syncing"/"retrying" запоминаются
// по account_id из событий truemail-sync-state, а "ready"/"error" снимают
// переходное состояние - дальше видно то, что записано в базе данных, и
// сохранённая ошибка не стирается переходным статусом благодаря порядку
// проверок в resolveSyncIndicator. current - предыдущая карта (не изменяется),
// event - {account_id, scope, status} из bridge.js. Возвращает новую карту.
function nextMailSyncTransient(current, event) {
  const map = { ...(current || {}) };
  if (!event || !isMailSyncScope(event.scope) || event.account_id == null) return map;
  if (event.status === 'syncing' || event.status === 'retrying') {
    map[event.account_id] = event.status;
  } else {
    delete map[event.account_id];
  }
  return map;
}

// Полный приоритет отображаемых состояний (S-006, S-007): "нужен повторный
// вход" выше сохранённой ошибки, сохранённая ошибка выше "повторной попытки",
// "повторная попытка" выше "идёт синхронизация", а она выше отсутствия
// индикатора при успешном последнем проходе. account - поля из list_accounts
// (needs_reauth, last_sync_error, last_sync_error_kind, last_sync_at);
// transientStatus - значение из карты nextMailSyncTransient для этого
// аккаунта, либо undefined/null, когда активного прохода нет.
function resolveSyncIndicator(account, transientStatus) {
  if (!account) return { kind: 'idle' };
  if (account.needs_reauth) {
    // Вид ошибки (invalid_credentials/needs_reauth) нужен и здесь: и заголовок
    // признака в боковой панели, и текст в карточке берут локализованное
    // сообщение по нему через ту же таблицу error-kinds-and-messages.md.
    return {
      kind: 'needs_reauth',
      errorKind: account.last_sync_error_kind || null,
      message: account.last_sync_error || null,
    };
  }
  if (account.last_sync_error) {
    return {
      kind: 'error',
      errorKind: account.last_sync_error_kind || null,
      message: account.last_sync_error,
    };
  }
  if (transientStatus === 'retrying') return { kind: 'retrying' };
  if (transientStatus === 'syncing') return { kind: 'syncing' };
  if (account.last_sync_at) return { kind: 'ok', lastSyncAt: account.last_sync_at };
  return { kind: 'idle' };
}

// SQLite datetime('now') (last_sync_at, как и созданные так же created_at/
// updated_at) отдаёт "YYYY-MM-DD HH:MM:SS" в UTC без указания зоны - без
// явного 'Z' конструктор Date разбирает такую строку как локальное время и
// сдвигает час показа. Строки, уже содержащие T или Z, возвращаются как есть.
function parseUtcSqlDate(value) {
  if (!value) return null;
  const normalized = /[TZ]/.test(value) ? value : `${value.replace(' ', 'T')}Z`;
  const date = new Date(normalized);
  return Number.isNaN(date.getTime()) ? null : date;
}

const mailSyncIndicator = {
  isMailSyncScope,
  nextMailSyncTransient,
  resolveSyncIndicator,
  parseUtcSqlDate,
};
if (typeof window !== 'undefined') window.mailSyncIndicator = mailSyncIndicator;
if (typeof module !== 'undefined' && module.exports) module.exports = mailSyncIndicator;
