// Проверки значка состояния синхронизации в шапке ящика боковой панели.
// Проверяется настоящий путь целиком: разметка окна берётся из index.html,
// модули интерфейса выполняются целиком, панель ящиков строит
// renderCoreAccounts, а события синхронизации идут через window.handleSyncState
// - тот же обработчик, которому bridge.js отдаёт truemail-sync-state.
// Выбор состояния проверяется отдельно в mail-sync-indicator.test.js, но сам по
// себе он ничего не доказывает: переименованный класс .acc-sync, снятый вызов
// refreshAccountSyncIndicator или незакрывающийся значок оставляли все проверки
// зелёными, а в окне значок либо не появлялся, либо висел навсегда.
// Спецификация: specs/mail-sync-visible-state.md.
// Запуск: node --test apps/desktop/tests/js/mail-sync-badge.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const {createUiWindow} = require('./ui-window.js');

// Ящик с успешным последним проходом: заметного состояния у него нет, значок
// скрыт - от него и пляшут все переходы.
function account(id, extra = {}) {
  return {
    id,
    email: `box${id}@example.test`,
    color: '#0058ff',
    needs_reauth: false,
    last_sync_error: null,
    last_sync_error_kind: null,
    last_sync_at: '2026-09-16 10:00:00',
    ...extra,
  };
}

async function openWindow(accounts = [account(1), account(2)]) {
  const ui = createUiWindow({
    answers: {listPinnedMessages: () => ({messages: [], ordinary: []}), listLabels: () => []},
    globals: {reloadCoreData: async () => {}},
  });
  await ui.clock.drain();
  ui.evaluate(`renderCoreAccounts(${JSON.stringify(accounts)},[[],[]]);`);
  await ui.clock.drain();
  return ui;
}

const badgeOf = (ui, id) => ui.query(`.acc-h[data-account-id="${id}"] .acc-sync`);

// Событие синхронизации приходит тем же путём, что от ядра: обработчик окна
// сам решает, какому ящику обновить значок.
async function syncEvent(ui, state) {
  ui.evaluate(`handleSyncState(${JSON.stringify(state)});`);
  await ui.clock.drain();
}

test('S-006, S-011: значок ящика идёт вслед за событиями синхронизации и закрывается по готовности', async () => {
  const ui = await openWindow();
  const first = badgeOf(ui, 1);
  assert.ok(first, 'в шапке ящика нет места под значок синхронизации');
  assert.equal(first.hidden, true, 'у ящика с успешным последним проходом значок показан: он мозолит глаза без повода');

  await syncEvent(ui, {account_id: 1, scope: 'all', status: 'syncing'});
  assert.equal(badgeOf(ui, 1).hidden, false,
    'начавшаяся синхронизация не показала значок: событие не доходит до шапки ящика');
  assert.ok(badgeOf(ui, 1).classes.has('acc-sync-syncing'), 'значок не помечен видом "идёт синхронизация": вид состояния не различить');
  assert.ok(badgeOf(ui, 1).title.trim().length, 'у значка синхронизации нет подсказки: наведя курсор, человек ничего не узнает');
  assert.equal(badgeOf(ui, 2).hidden, true,
    'событие одного ящика зажгло значок другого: в панели показано состояние не того ящика');

  const syncingTitle = badgeOf(ui, 1).title;
  await syncEvent(ui, {account_id: 1, scope: 'mail', status: 'retrying'});
  assert.ok(badgeOf(ui, 1).classes.has('acc-sync-retrying'),
    'обрыв связи показан как обычная синхронизация: восстановление соединения в панели неразличимо');
  assert.notEqual(badgeOf(ui, 1).title, syncingTitle, 'подсказка значка не изменилась при переходе к восстановлению связи');

  // Область календаря и контактов у значка почты своей строки не трогает: иначе
  // проход по календарю зажигал бы почтовый значок на пустом месте.
  await syncEvent(ui, {account_id: 1, scope: 'dav', status: 'ready'});
  assert.ok(badgeOf(ui, 1).classes.has('acc-sync-retrying'),
    'событие календаря погасило значок почты: состояния разных областей перепутаны');

  await syncEvent(ui, {account_id: 1, scope: 'all', status: 'ready'});
  assert.equal(badgeOf(ui, 1).hidden, true,
    'после успешного завершения значок остался виден: ящик синхронизирован, а панель показывает работу');
  assert.equal(badgeOf(ui, 1).className, 'acc-sync',
    'у скрытого значка остался вид прежнего состояния: следующее состояние нарисуется поверх прошлого');
});

test('S-010: значок "нужен повторный вход" открывает переподключение того же ящика', async () => {
  const ui = await openWindow([account(1, {needs_reauth: true, last_sync_error: 'неверный пароль', last_sync_error_kind: 'invalid_credentials'}), account(2)]);
  const badge = badgeOf(ui, 1);
  assert.equal(badge.hidden, false, 'ящик просит повторный вход, а значка в панели нет: беду никто не заметит');
  assert.ok(badge.classes.has('acc-sync-needs_reauth'), 'значок не помечен видом "нужен повторный вход"');
  assert.ok(badge.title.trim().length, 'у значка нет подсказки: что делать с ящиком, непонятно');
  assert.equal(badge.getAttribute('role'), 'button', 'значок не объявлен кнопкой: программы экранного доступа не назовут его нажимаемым');
  assert.equal(badge.tabIndex, 0, 'к значку не попасть с клавиатуры, хотя он нажимается');

  // Значок живёт внутри шапки ящика, а её собственный обработчик сворачивает и
  // разворачивает список папок: нажатие на значок не должно попутно менять это.
  const wasOpen = ui.query('.acc-h[data-account-id="1"]').classes.has('open');
  badge.dispatch('click');
  await ui.clock.drain();
  assert.ok(ui.query('.settings').classes.has('account-wizard-mode'),
    'нажатие на значок не открыло переподключение ящика: значок показывает беду и ничего с ней не делает');
  assert.equal(ui.byId('accountEmail').value, 'box1@example.test',
    'переподключение открылось без адреса ящика: человеку предложено добавить ящик заново вместо починки прежнего');
  assert.equal(ui.query('.acc-h[data-account-id="1"]').classes.has('open'), wasOpen,
    'нажатие на значок заодно свернуло или развернуло ящик в панели: событие дошло до обработчика шапки');
});

test('S-007: сменившееся состояние ящика меняет и значок, и его нажатие', async () => {
  const ui = await openWindow([account(1, {needs_reauth: true, last_sync_error: 'неверный пароль', last_sync_error_kind: 'invalid_credentials'}), account(2)]);
  assert.equal(badgeOf(ui, 1).getAttribute('role'), 'button', 'значок повторного входа не стал кнопкой: проверять нечего');
  const reauthTitle = badgeOf(ui, 1).title;

  // Повторный вход больше не нужен, но последний проход оборвался: значок
  // обязан сменить вид и подсказку - вид ошибки берётся из общей таблицы
  // понятных текстов, а не из сырого сообщения ядра.
  ui.evaluate(`renderCoreAccounts(${JSON.stringify([account(1, {last_sync_error: 'сеть недоступна', last_sync_error_kind: 'network_unavailable'}), account(2)])},[[],[]]);`);
  await ui.clock.drain();
  assert.ok(badgeOf(ui, 1).classes.has('acc-sync-error'), 'сохранённая ошибка прохода не показана видом "ошибка"');
  assert.ok(badgeOf(ui, 1).title.trim().length, 'у значка ошибки нет подсказки: что стряслось с ящиком, не узнать');
  assert.notEqual(badgeOf(ui, 1).title, reauthTitle, 'подсказка ошибки совпала с подсказкой повторного входа: вид беды не различить');
  assert.equal(badgeOf(ui, 1).getAttribute('role'), null, 'значок ошибки остался объявлен кнопкой переподключения');

  // Ящик переподключили: данные перезагрузились, и панель строится заново теми
  // же узлами. Старое нажатие обязано уйти вместе с состоянием, иначе значок
  // обычной синхронизации продолжит открывать мастер подключения.
  ui.evaluate(`renderCoreAccounts(${JSON.stringify([account(1), account(2)])},[[],[]]);`);
  await ui.clock.drain();
  await syncEvent(ui, {account_id: 1, scope: 'all', status: 'syncing'});
  const badge = badgeOf(ui, 1);
  assert.ok(badge.classes.has('acc-sync-syncing'), 'после починки значок всё ещё показывает повторный вход');
  assert.equal(badge.getAttribute('role'), null, 'значок обычной синхронизации остался объявлен кнопкой');

  badge.dispatch('click');
  await ui.clock.drain();
  assert.equal(ui.query('.settings').classes.has('account-wizard-mode'), false,
    'нажатие на значок идущей синхронизации открыло переподключение: обработчик прошлого состояния не снят');
});
