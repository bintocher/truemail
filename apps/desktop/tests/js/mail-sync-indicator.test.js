// Проверки чистой логики apps/desktop/ui/modules/mail-sync-indicator.js.
// Запуск: node --test apps/desktop/tests/js/mail-sync-indicator.test.js (Node 20+).
'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {
  isMailSyncScope,
  nextMailSyncTransient,
  resolveSyncIndicator,
  parseUtcSqlDate,
} = require('../../ui/modules/mail-sync-indicator.js');

// Границы задачи: только "all" и "mail" относятся к почте, "auxiliary"/"dav" - нет.
test('isMailSyncScope отличает почту от календаря/контактов', () => {
  assert.equal(isMailSyncScope('all'), true);
  assert.equal(isMailSyncScope('mail'), true);
  assert.equal(isMailSyncScope('auxiliary'), false);
  assert.equal(isMailSyncScope('dav'), false);
  assert.equal(isMailSyncScope(undefined), false);
});

// S-007: полный приоритет пяти состояний.
test('S-007: needs_reauth выше сохранённой ошибки', () => {
  const account = { needs_reauth: true, last_sync_error: 'сеть недоступна' };
  assert.equal(resolveSyncIndicator(account, 'syncing').kind, 'needs_reauth');
});
// S-009: карточка и признак берут понятный текст по виду ошибки - вид должен
// доходить и для needs_reauth, а не теряться.
test('needs_reauth несёт вид и текст последней ошибки для локализации', () => {
  const account = { needs_reauth: true, last_sync_error: 'неверный пароль', last_sync_error_kind: 'invalid_credentials' };
  const state = resolveSyncIndicator(account, undefined);
  assert.equal(state.kind, 'needs_reauth');
  assert.equal(state.errorKind, 'invalid_credentials');
  assert.equal(state.message, 'неверный пароль');
});
test('S-007: сохранённая ошибка выше "повторной попытки"', () => {
  const account = { needs_reauth: false, last_sync_error: 'сеть недоступна', last_sync_error_kind: 'network_unavailable' };
  const state = resolveSyncIndicator(account, 'retrying');
  assert.equal(state.kind, 'error');
  assert.equal(state.errorKind, 'network_unavailable');
});
test('S-007: "повторная попытка" выше "идёт синхронизация"', () => {
  const account = { needs_reauth: false, last_sync_error: null };
  assert.equal(resolveSyncIndicator(account, 'retrying').kind, 'retrying');
});
test('S-007: "идёт синхронизация" выше отсутствия индикатора', () => {
  const account = { needs_reauth: false, last_sync_error: null, last_sync_at: '2026-09-16 10:00:00' };
  assert.equal(resolveSyncIndicator(account, 'syncing').kind, 'syncing');
});
// S-006: последний проход успешен и активного прохода нет - индикатора нет.
test('S-006: успешный последний проход без активной синхронизации - нет индикатора', () => {
  const account = { needs_reauth: false, last_sync_error: null, last_sync_at: '2026-09-16 10:00:00' };
  assert.equal(resolveSyncIndicator(account, undefined).kind, 'ok');
});
test('аккаунт без данных даёт нейтральное состояние', () => {
  assert.equal(resolveSyncIndicator(null, undefined).kind, 'idle');
  assert.equal(resolveSyncIndicator({}, undefined).kind, 'idle');
});

// S-011: переходное событие не стирает сохранённую ошибку - проверяется тем,
// что nextMailSyncTransient и resolveSyncIndicator вместе не понижают
// приоритет ошибки при поступлении "syncing".
test('S-011: новое событие "syncing" не прячет сохранённую ошибку', () => {
  const account = { needs_reauth: false, last_sync_error: 'сервер недоступен', last_sync_error_kind: 'server_unavailable' };
  let transient = nextMailSyncTransient({}, { account_id: 1, scope: 'all', status: 'syncing' });
  assert.equal(resolveSyncIndicator(account, transient[1]).kind, 'error');
});

// Карта переходных статусов по аккаунтам.
test('nextMailSyncTransient запоминает syncing/retrying и снимает по ready/error', () => {
  let map = nextMailSyncTransient({}, { account_id: 1, scope: 'all', status: 'syncing' });
  assert.equal(map[1], 'syncing');
  map = nextMailSyncTransient(map, { account_id: 1, scope: 'mail', status: 'retrying' });
  assert.equal(map[1], 'retrying');
  map = nextMailSyncTransient(map, { account_id: 1, scope: 'all', status: 'ready' });
  assert.equal(map[1], undefined);
});
test('nextMailSyncTransient игнорирует области событий вне почты', () => {
  const map = nextMailSyncTransient({}, { account_id: 1, scope: 'auxiliary', status: 'syncing' });
  assert.deepEqual(map, {});
});
test('nextMailSyncTransient не изменяет переданную карту (иммутабельность)', () => {
  const before = { 1: 'syncing' };
  const after = nextMailSyncTransient(before, { account_id: 2, scope: 'mail', status: 'syncing' });
  assert.deepEqual(before, { 1: 'syncing' });
  assert.equal(after[1], 'syncing');
  assert.equal(after[2], 'syncing');
});

// Формат последнего успеха совпадает с datetime('now') SQLite - без 'Z'.
test('parseUtcSqlDate трактует строку без зоны как UTC', () => {
  const date = parseUtcSqlDate('2026-09-16 10:00:00');
  assert.equal(date.toISOString(), '2026-09-16T10:00:00.000Z');
});
test('parseUtcSqlDate пропускает уже размеченные строки как есть', () => {
  const date = parseUtcSqlDate('2026-09-16T10:00:00Z');
  assert.equal(date.toISOString(), '2026-09-16T10:00:00.000Z');
});
test('parseUtcSqlDate на пустом и испорченном значении даёт null', () => {
  assert.equal(parseUtcSqlDate(null), null);
  assert.equal(parseUtcSqlDate(''), null);
  assert.equal(parseUtcSqlDate('не дата'), null);
});
