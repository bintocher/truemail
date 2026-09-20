// Проверки выбора показываемого состояния синхронизации почты:
// apps/desktop/ui/modules/mail-sync-indicator.js. Здесь проверяется только
// решение "что показывать" - сам значок в шапке ящика, его скрытие и клик
// проверяются на настоящем окне в mail-sync-badge.test.js.
// Спецификация: specs/mail-sync-visible-state.md.
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

// Границы задачи: значок почты отвечает за "all" (цикл sync_accounts и первая
// синхронизация) и "mail" (наблюдатель за новыми письмами). У календаря,
// задач и контактов своя строка #calSyncInfo, и их события не должны крутить
// значок почты - иначе он мигает при каждом проходе календаря.
test('область события: почтовые - all и mail, остальные значка почты не касаются', () => {
  const cases = [
    {scope: 'all', mail: true, why: 'цикл синхронизации почты принят за чужую область'},
    {scope: 'mail', mail: true, why: 'наблюдатель за новыми письмами принят за чужую область'},
    {scope: 'auxiliary', mail: false, why: 'события задач и контактов приняты за почтовые'},
    {scope: 'dav', mail: false, why: 'события календаря приняты за почтовые'},
    {scope: undefined, mail: false, why: 'событие без области принято за почтовое'},
  ];
  cases.forEach(item => {
    assert.equal(isMailSyncScope(item.scope), item.mail, `${item.scope}: ${item.why}`);
    // Карта переходных состояний обязана держать ту же границу: иначе
    // отбраковка областей есть, но ею никто не пользуется.
    const map = nextMailSyncTransient({}, {account_id: 1, scope: item.scope, status: 'syncing'});
    assert.equal(map[1] === 'syncing', item.mail,
      `${item.scope}: карта переходных состояний не держит ту же границу областей, что isMailSyncScope`);
  });
});

// S-006, S-011: значок идёт вслед за событиями синхронизации одного ящика.
// Последовательность важна целиком: "идёт синхронизация" сменяется
// "восстановлением соединения" при обрыве и возвращается обратно при новой
// попытке, а "ready" снимает переходное состояние и оставляет видимым то, что
// записано в базе. Отдельные проверки карты и приоритета этого не ловили:
// событие retrying, показанное как обычная синхронизация, оставалось незаметным.
test('S-006, S-011: показываемое состояние идёт вслед за событиями syncing, retrying и ready', () => {
  const account = {needs_reauth: false, last_sync_error: null, last_sync_at: '2026-09-16 10:00:00'};
  const steps = [
    {status: 'syncing', scope: 'all', transient: 'syncing', kind: 'syncing', why: 'начало прохода не показано как синхронизация'},
    {status: 'retrying', scope: 'mail', transient: 'retrying', kind: 'retrying', why: 'обрыв связи показан как обычная синхронизация: восстановление соединения неразличимо'},
    {status: 'syncing', scope: 'mail', transient: 'syncing', kind: 'syncing', why: 'возврат к обычной синхронизации после повторной попытки не виден'},
    {status: 'ready', scope: 'all', transient: undefined, kind: 'ok', why: 'успешное завершение не сняло переходное состояние: значок останется навсегда'},
  ];
  let map = {};
  steps.forEach(step => {
    map = nextMailSyncTransient(map, {account_id: 1, scope: step.scope, status: step.status});
    assert.equal(map[1], step.transient, `${step.status}: переходное состояние ящика записано неверно (${step.why})`);
    assert.equal(resolveSyncIndicator(account, map[1]).kind, step.kind, `${step.status}: ${step.why}`);
  });
});

// S-007: "нужен повторный вход" перекрывает всё остальное - пока ящик не
// переподключили, проход всё равно не состоится.
test('S-007: needs_reauth выше сохранённой ошибки', () => {
  const account = {needs_reauth: true, last_sync_error: 'сеть недоступна'};
  assert.equal(resolveSyncIndicator(account, 'syncing').kind, 'needs_reauth');
});

// S-009: и заголовок значка, и карточка ящика берут понятный текст по виду
// ошибки через общую таблицу error-kinds-and-messages.md - вид обязан доходить
// и для needs_reauth, а не теряться по дороге.
test('needs_reauth несёт вид и текст последней ошибки для локализации', () => {
  const account = {needs_reauth: true, last_sync_error: 'неверный пароль', last_sync_error_kind: 'invalid_credentials'};
  const state = resolveSyncIndicator(account, undefined);
  assert.equal(state.kind, 'needs_reauth');
  assert.equal(state.errorKind, 'invalid_credentials');
  assert.equal(state.message, 'неверный пароль');
});

// S-007: сохранённая ошибка важнее переходной попытки: пока попытка идёт,
// человек всё ещё должен видеть, чем закончился прошлый проход.
test('S-007: сохранённая ошибка выше "повторной попытки"', () => {
  const account = {needs_reauth: false, last_sync_error: 'сеть недоступна', last_sync_error_kind: 'network_unavailable'};
  const state = resolveSyncIndicator(account, 'retrying');
  assert.equal(state.kind, 'error');
  assert.equal(state.errorKind, 'network_unavailable');
});

// S-011: пришедшее событие "syncing" не должно прятать сохранённую ошибку -
// иначе новый проход стирает единственный след прошлой беды с глаз.
test('S-011: новое событие "syncing" не прячет сохранённую ошибку', () => {
  const account = {needs_reauth: false, last_sync_error: 'сервер недоступен', last_sync_error_kind: 'server_unavailable'};
  const transient = nextMailSyncTransient({}, {account_id: 1, scope: 'all', status: 'syncing'});
  assert.equal(resolveSyncIndicator(account, transient[1]).kind, 'error');
});

// S-006: спокойные состояния значка не получают. Границы здесь - ящик без
// единого прохода и ящик с успешным последним проходом: и то и другое не
// повод мозолить глаза значком.
test('S-006: спокойные состояния значка не дают', () => {
  const cases = [
    {account: {needs_reauth: false, last_sync_error: null, last_sync_at: '2026-09-16 10:00:00'}, kind: 'ok', why: 'успешный последний проход без активной синхронизации помечен как заметное состояние'},
    {account: {}, kind: 'idle', why: 'ящик без единого прохода получил заметное состояние'},
    {account: null, kind: 'idle', why: 'данных об ящике ещё нет, а состояние уже не нейтральное'},
  ];
  cases.forEach(item => {
    assert.equal(resolveSyncIndicator(item.account, undefined).kind, item.kind, item.why);
  });
});

// SQLite datetime('now') пишет last_sync_at как "YYYY-MM-DD HH:MM:SS" в UTC без
// указания зоны: без явного 'Z' конструктор Date считает такую строку местным
// временем и сдвигает показанный час на разницу поясов.
test('parseUtcSqlDate читает время последнего прохода как UTC', () => {
  const cases = [
    {value: '2026-09-16 10:00:00', expect: '2026-09-16T10:00:00.000Z', why: 'строка SQLite без зоны разобрана как местное время: час показа уедет'},
    {value: '2026-09-16T10:00:00Z', expect: '2026-09-16T10:00:00.000Z', why: 'уже размеченная строка испорчена повторной разметкой'},
    {value: null, expect: null, why: 'отсутствие значения не дало null'},
    {value: '', expect: null, why: 'пустая строка не дала null'},
    {value: 'не дата', expect: null, why: 'испорченное значение не дало null: в окно уйдёт Invalid Date'},
  ];
  cases.forEach(item => {
    const date = parseUtcSqlDate(item.value);
    assert.equal(date === null ? null : date.toISOString(), item.expect, `${JSON.stringify(item.value)}: ${item.why}`);
  });
});
