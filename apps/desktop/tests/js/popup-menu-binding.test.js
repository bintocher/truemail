// Связка правила "одно меню одновременно" с настоящей разметкой окна.
// Спецификация: specs/single-popup-menu.md.
// Запуск: node --test apps/desktop/tests/js/popup-menu-binding.test.js (Node 22+).
//
// План меню (modules/popup-menus.js) знает только идентификаторы, а узлы по ним
// ищет реестр POPUP_MENU_REGISTRY в modules/calendar-contacts.js. Идентификаторы
// реестра не сверялись с index.html ничем: опечатка в идентификаторе узла или
// переименованный класс оставляли план зелёным, а в окне меню оставалось висеть
// поверх соседнего. Здесь всё идёт настоящим путём: разметка из index.html,
// модули окна целиком, события contextmenu и mouseenter как от браузера.

'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {createUiWindow} = require('./ui-window.js');

const sampleMessages = () => ([
  {id: 11, account_id: 1, folder_id: 2, subject: 'Счёт', preview: 'оплата', from: {name: 'Босс', email: 'boss@example.test'},
    to: [], cc: [], flags: {seen: true, flagged: false}, labels: [], date: '2026-09-19T10:00:00Z', pinned_at: null, task_due_at: null},
  {id: 12, account_id: 1, folder_id: 2, subject: 'Отчёт', preview: 'срок', from: {name: 'Коллега', email: 'mate@example.test'},
    to: [], cc: [], flags: {seen: true, flagged: false}, labels: [], date: '2026-09-18T10:00:00Z', pinned_at: null, task_due_at: null},
]);

const fullMessage = () => ({
  meta: {id: 11, account_id: 1, folder_id: 2, subject: 'Счёт', from: {name: 'Босс', email: 'boss@example.test'},
    to: [], cc: [], date: '2026-09-19T10:00:00Z', flags: {seen: true}, labels: []},
  body_html: '', body_text: 'тело',
  attachments: [{id: 'a1', filename: 'act.pdf', mime_type: 'application/pdf', size: 1024}],
});

async function openWindow() {
  const ui = createUiWindow({
    answers: {
      listLabels: () => [{id: 1, name: 'Работа', color: '#f00'}],
      messageLabelIds: () => [],
      getMessage: () => fullMessage(),
    },
    globals: {reloadCoreData: async () => {}},
  });
  await ui.clock.drain();
  ui.evaluate(`
    coreAccounts=[{id:1,email:'me@example.test',color:'#0058ff'}];
    setCoreFolders([{id:2,account_id:1,role:'inbox',display_name:'Входящие',remote_path:'INBOX'}]);
    currentFolderId=2;currentSmartIndex=null;
    messages=${JSON.stringify(sampleMessages())};
    applyListOptions(true,'Входящие');
    coreTags=[{id:5,name:'Важное',color:'#e5484d'}];
    renderTagsNav();
    renderAccountSettings([{id:1,email:'me@example.test',display_name:'Я',auth_kind:'password'}],[[]],[]);
  `);
  await ui.clock.drain();
  return ui;
}

const openMenuIds = ui => ui.evaluate('openPopupMenuIds()').join(',');
const rightClickMessage = (ui, id) => ui.query(`#msgs .msg[data-message-id="${id}"]`).dispatch('contextmenu', {clientX: 100, clientY: 100});

// Меню, чьи узлы лежат в index.html с самого начала: реестр обязан найти их
// сразу после загрузки окна. Меню вложения, подменю меток и палитра цвета
// создаются кодом по ходу работы - они проверяются ниже, после действия,
// которое их создаёт.
const STATIC_MENUS = ['message', 'smart', 'folder', 'tag', 'contact', 'more', 'filter', 'sort', 'icon'];
const DYNAMIC_MENUS = ['attachment', 'flag', 'color'];

test('S-020: каждый идентификатор семейства находит своё меню в разметке окна', async () => {
  const ui = await openWindow();
  // Реестр и перечень идентификаторов ведутся в разных файлах: меню, забытое в
  // одном из них, не закроется никогда.
  // Значения приходят из другого realm - переносим их в обычные массивы,
  // иначе сравнение спорит о происхождении массива, а не о составе.
  const registryIds = [...ui.evaluate('Object.keys(POPUP_MENU_REGISTRY)')].sort();
  assert.deepEqual(
    registryIds,
    [...ui.evaluate('POPUP_MENU_IDS')].sort(),
    'реестр узлов и перечень идентификаторов меню разошлись: меню без пары не участвует в правиле одного меню');
  assert.deepEqual(
    registryIds,
    [...STATIC_MENUS, ...DYNAMIC_MENUS].sort(),
    'состав семейства меню изменился: новое меню не описано в этой проверке и его связка с разметкой не проверяется');

  STATIC_MENUS.forEach(id => {
    const nodes = ui.evaluate(`popupMenuNodes(${JSON.stringify(id)}).length`);
    assert.ok(nodes > 0, `меню "${id}": реестр не нашёл узел в разметке окна - закрывать это меню будет нечем`);
    assert.equal(ui.evaluate(`popupMenuIsOpen(${JSON.stringify(id)})`), false,
      `меню "${id}": сразу после загрузки окна считается открытым - признак открытости в реестре не тот`);
  });

  // Узлы меню вложения и палитры цвета появляются только после действия
  // пользователя: селектор реестра должен совпасть с тем, что создаёт код окна.
  ui.query('#msgs .msg[data-message-id="11"]').dispatch('click');
  await ui.clock.drain();
  ui.query('.att-chip').dispatch('contextmenu', {clientX: 40, clientY: 40});
  await ui.clock.drain();
  assert.equal(openMenuIds(ui), 'attachment', 'меню вложения создано, но реестр его не видит: селектор .att-menu разошёлся с разметкой меню');

  ui.query('.account-card .color-current').dispatch('click');
  await ui.clock.drain();
  assert.equal(openMenuIds(ui), 'color', 'палитра цвета аккаунта открыта, но реестр её не видит: селектор .color-grid разошёлся с разметкой');

  rightClickMessage(ui, 11);
  await ui.clock.drain();
  ui.query('#ctxmenu [data-context-action="flag"]').dispatch('mouseenter');
  await ui.clock.drain();
  assert.equal(openMenuIds(ui), 'message,flag', 'подменю меток создано, но реестр его не видит: селектор .flag-menu разошёлся с разметкой');
});

test('S-002: правый клик по метке закрывает меню письма', async () => {
  const ui = await openWindow();
  rightClickMessage(ui, 11);
  await ui.clock.drain();
  assert.equal(ui.byId('ctxmenu').classes.has('open'), true, 'правый клик по письму не открыл меню письма: проверять нечего');

  ui.query('#tagsNav .tag-row').dispatch('contextmenu', {clientX: 40, clientY: 40});
  await ui.clock.drain();
  assert.equal(ui.byId('ctxtag').classes.has('open'), true, 'правый клик по метке не открыл меню метки');
  // Оба меню - братья в одном списке узлов, и на экране они не перекрываются:
  // меню письма, оставшееся открытым, висит рядом с меню метки поверх окна.
  assert.equal(ui.byId('ctxmenu').classes.has('open'), false, 'меню письма осталось открытым рядом с меню метки');
  assert.equal(openMenuIds(ui), 'tag', 'в окне открыто не только меню метки');
});

test('S-004, S-008: повторный правый клик по тому же письму сохраняет подменю меток, а выбор пункта гасит оба меню', async () => {
  const ui = await openWindow();
  rightClickMessage(ui, 11);
  await ui.clock.drain();
  ui.query('#ctxmenu [data-context-action="flag"]').dispatch('mouseenter');
  await ui.clock.drain();
  assert.equal(ui.queryAll('.flag-menu').length, 1, 'наведение на пункт меток не открыло подменю: проверять его судьбу не на чем');

  // Повторный правый клик по тому же письму переставляет меню письма к новой
  // точке курсора. Подменю меток при этом не открывалось заново - гасить его
  // нельзя: иначе метки, выбранные наведением, пропадают из-под указателя.
  rightClickMessage(ui, 11);
  await ui.clock.drain();
  assert.equal(ui.queryAll('.flag-menu').length, 1,
    'повторный правый клик по тому же письму снёс подменю меток: перестановка родителя не должна гасить пару');
  assert.equal(openMenuIds(ui), 'message,flag', 'после перестановки меню письма пара с подменю меток распалась');

  // Правый клик по соседнему письму - это уже другое меню, и подменю прежнего
  // письма показывало бы метки не того письма.
  ui.query('#msgs .msg[data-message-id="12"]').dispatch('contextmenu', {clientX: 120, clientY: 120});
  await ui.clock.drain();
  assert.equal(ui.queryAll('.flag-menu').length, 0, 'подменю меток прежнего письма осталось открытым при переходе к другому письму');

  // Выбор пункта меню письма закрывает и само меню, и подменю: зависимое меню
  // уходит вместе с родителем, о нём никто не просит отдельно.
  rightClickMessage(ui, 11);
  await ui.clock.drain();
  ui.query('#ctxmenu [data-context-action="flag"]').dispatch('mouseenter');
  await ui.clock.drain();
  assert.equal(ui.queryAll('.flag-menu').length, 1, 'подменю меток не открылось перед проверкой выбора пункта');
  ui.query('#ctxmenu [data-context-action="pin-message"]').dispatch('click');
  await ui.clock.drain();
  assert.equal(ui.byId('ctxmenu').classes.has('open'), false, 'выбранный пункт не закрыл меню письма');
  assert.equal(ui.queryAll('.flag-menu').length, 0, 'подменю меток осталось висеть после выбора пункта меню письма');
});
