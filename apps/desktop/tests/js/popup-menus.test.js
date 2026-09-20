// Проверки правила "одно вспомогательное меню одновременно".
// Спецификация: specs/single-popup-menu.md.
// Запуск: node --test apps/desktop/tests/js/popup-menus.test.js (Node 22+).
//
// Часть проверок идёт по настоящему пути: разметка берётся из index.html,
// модули окна выполняются целиком, меню открываются теми же событиями, которые
// пришлют браузеру правый клик и нажатие кнопки. Так ловятся дефекты связки
// "разметка - обработчик - план", которые чистый план не видит: план может
// говорить "закрой меню письма", а реестр узлов - не найти его узел.
// Остальные проверки остаются на чистом плане: они про арифметику решения, и
// через окно те же случаи заняли бы вчетверо больше времени.
'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {planPopupMenus, withDependentMenus} = require('../../ui/modules/popup-menus.js');
const {createUiWindow} = require('./ui-window.js');

const sampleMessages = () => ([
  {id: 11, account_id: 1, folder_id: 2, subject: 'Счёт', preview: 'оплата', from: {name: 'Босс', email: 'boss@example.test'},
    to: [], cc: [], flags: {seen: true, flagged: false}, labels: [], date: '2026-09-19T10:00:00Z', pinned_at: null, task_due_at: null},
]);

// Открытое письмо с вложением: без него меню вложения открыть нечем, а это
// единственное меню, живущее отдельным узлом, а не классом.
const fullMessage = () => ({
  meta: {id: 11, account_id: 1, folder_id: 2, subject: 'Счёт', from: {name: 'Босс', email: 'boss@example.test'},
    to: [], cc: [], date: '2026-09-19T10:00:00Z', flags: {seen: true}, labels: []},
  body_html: '', body_text: 'тело',
  attachments: [{id: 'a1', filename: 'act.pdf', mime_type: 'application/pdf', size: 1024}],
});

// Окно с письмом в списке, меткой в панели, папкой, контактом и умными папками:
// все виды меню семейства открываются настоящими действиями пользователя.
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
    renderCoreAccounts(
      [{id:1,email:'me@example.test',display_name:'Я',color:'#0058ff',auth_kind:'password'}],
      [[{id:2,account_id:1,role:'inbox',display_name:'Входящие',remote_path:'INBOX'},{id:3,account_id:1,role:null,display_name:'Проекты',remote_path:'Projects'}]],
      [],
      [{id:7,display_name:'Иван Петров',emails:[{email:'ivan@example.test'}]}]
    );
    renderAccountSettings([{id:1,email:'me@example.test',display_name:'Я',auth_kind:'password'}],[[]],[]);
  `);
  await ui.clock.drain();
  return ui;
}

const openMenuIds = ui => ui.evaluate('openPopupMenuIds()').join(',');

// Действия пользователя, открывающие каждый вид меню семейства. Подменю меток
// сюда не входит: оно открывается только в паре с меню письма и проверяется
// отдельно.
function menuOpeners(ui) {
  return [
    ['message', 'правый клик по письму', async () => ui.query('#msgs .msg[data-message-id="11"]').dispatch('contextmenu', {clientX: 100, clientY: 100})],
    ['tag', 'правый клик по метке', async () => ui.query('#tagsNav .tag-row').dispatch('contextmenu', {clientX: 40, clientY: 40})],
    ['smart', 'правый клик по умной папке', async () => ui.query('[data-smart-index]').dispatch('contextmenu', {clientX: 40, clientY: 40})],
    ['folder', 'правый клик по папке', async () => ui.queryAll('.acc-sub .navitem')[1].dispatch('contextmenu', {clientX: 40, clientY: 40})],
    ['contact', 'правый клик по карточке контакта', async () => ui.query('.ccard[data-contact-id]').dispatch('contextmenu', {clientX: 40, clientY: 40})],
    ['attachment', 'правый клик по плашке вложения', async () => {
      ui.query('#msgs .msg[data-message-id="11"]').dispatch('click');
      await ui.clock.drain();
      ui.query('.att-chip').dispatch('contextmenu', {clientX: 40, clientY: 40});
    }],
    ['filter', 'нажатие кнопки фильтра', async () => ui.byId('filterBtn').dispatch('click')],
    ['sort', 'нажатие кнопки сортировки', async () => ui.byId('sortBtn').dispatch('click')],
    ['more', 'нажатие кнопки "ещё" в беседе', async () => ui.byId('threadMoreButton').dispatch('click')],
    ['icon', 'нажатие выбора значка умной папки', async () => ui.byId('smartIconButton').dispatch('click')],
    ['color', 'нажатие выбора цвета аккаунта', async () => ui.query('.account-card .color-current').dispatch('click')],
  ];
}

// S-001, S-002 по настоящему пути. Раньше это место проверял план в отрыве от
// окна, и смотрел он поле plan.open, которого приложение не читает вовсе:
// закрытие идёт по plan.close. Здесь итог смотрится там же, где его видит
// пользователь - в списке открытых меню окна.
test('S-001, S-002: открытие любого меню настоящим действием гасит все прочие', async () => {
  const ui = await openWindow();
  const openers = menuOpeners(ui);
  for (const [id, action, open] of openers) {
    await open();
    await ui.clock.drain();
    assert.equal(openMenuIds(ui), id, `${action}: в окне открыто не одно меню "${id}", а другой набор`);
  }
  // Второй проход по кругу: первое меню списка открывается поверх последнего -
  // порядок в реестре не должен решать, гасится меню или нет.
  await openers[0][2]();
  await ui.clock.drain();
  assert.equal(openMenuIds(ui), 'message', 'меню письма, открытое после меню цвета, не погасило его');
});

// S-002: признак открытости у меню свой у каждого вида - класс open, снятый
// класс hidden или само существование узла. Прежняя проверка перебирала
// идентификаторы в плане и о признаках не знала: меню, которое план велел
// закрыть, могло остаться на экране.
test('S-002: прежнее меню гаснет в разметке по своему признаку открытости', async () => {
  const ui = await openWindow();
  ui.byId('threadMoreButton').dispatch('click');
  await ui.clock.drain();
  assert.equal(ui.byId('threadMoreMenu').classes.has('open'), true, 'меню "ещё" не открылось: проверять закрытие не на чем');

  ui.byId('filterBtn').dispatch('click');
  await ui.clock.drain();
  assert.equal(ui.byId('threadMoreMenu').classes.has('open'), false, 'меню "ещё" осталось на экране с классом open');
  assert.equal(ui.byId('filterMenu').classes.has('hidden'), false, 'меню фильтра не открылось');

  ui.query('#msgs .msg[data-message-id="11"]').dispatch('click');
  await ui.clock.drain();
  ui.query('.att-chip').dispatch('contextmenu', {clientX: 40, clientY: 40});
  await ui.clock.drain();
  assert.equal(ui.byId('filterMenu').classes.has('hidden'), true, 'меню фильтра осталось на экране: класс hidden не вернулся');
  assert.equal(ui.queryAll('.att-menu:not(.flag-menu)').length, 1, 'меню вложения не открылось');

  ui.query('#msgs .msg[data-message-id="11"]').dispatch('contextmenu', {clientX: 100, clientY: 100});
  await ui.clock.drain();
  assert.equal(ui.queryAll('.att-menu:not(.flag-menu)').length, 0, 'узел меню вложения остался в дереве после открытия меню письма');
});

// S-014, S-019: клик мимо меню. Проверка идёт по настоящему пути, потому что
// решение принимает обработчик документа: он отличает клик внутри меню от
// клика мимо, и подменю меток должно уйти вместе с родителем.
test('S-014: клик мимо меню закрывает меню письма вместе с подменю меток', async () => {
  const ui = await openWindow();
  ui.query('#msgs .msg[data-message-id="11"]').dispatch('contextmenu', {clientX: 100, clientY: 100});
  await ui.clock.drain();
  ui.query('#ctxmenu [data-context-action="flag"]').dispatch('mouseenter');
  await ui.clock.drain();
  assert.equal(openMenuIds(ui), 'message,flag', 'подменю меток не открылось наведением: проверять закрытие пары не на чем');

  ui.query('#msgs').dispatch('click');
  await ui.clock.drain();
  assert.equal(openMenuIds(ui), '', 'клик мимо меню не закрыл открытые меню');
  assert.equal(ui.queryAll('.flag-menu').length, 0, 'подменю меток осталось висеть после клика мимо меню');
});

// Дальше - чистый план: арифметика решения без окна.

// S-002, S-003, S-008, S-016: открытие меню гасит любое другое, кроме
// разрешённой пары. Случаи собраны в таблицу - каждый закрывает свой ранее
// найденный дефект.
test('S-002: план закрывает прочие меню при открытии нового', () => {
  const cases = [
    ['меню папки поверх меню письма', ['message'], 'folder', ['message']],
    ['меню папки поверх меню умной папки', ['smart'], 'folder', ['smart']],
    ['сортировка поверх фильтра', ['filter'], 'sort', ['filter']],
    ['фильтр поверх сортировки', ['sort'], 'filter', ['sort']],
    ['меню папки гасит и подменю меток', ['message', 'flag'], 'folder', ['flag', 'message']],
    ['меню поверх нескольких открытых', ['filter', 'attachment', 'more', 'color'], 'sort', ['attachment', 'color', 'filter', 'more']],
  ];
  cases.forEach(([name, open, id, expected]) => {
    const plan = planPopupMenus(open, {type: 'open', id});
    assert.deepEqual(plan.close.sort(), expected, `случай "${name}": закрыт не тот набор меню`);
    assert.equal(plan.reposition, false, `случай "${name}": открытие нового меню принято за перестановку прежнего`);
  });
});

// S-004: повторный правый клик по тому же элементу переставляет меню к новой
// точке курсора, клик по соседнему элементу открывает меню заново.
test('S-004: перестановка меню только для повторного клика по тому же элементу', () => {
  const cases = [
    ['повторный клик по тому же письму', true, true, []],
    ['клик по соседнему письму', false, false, []],
  ];
  cases.forEach(([name, sameTarget, reposition, close]) => {
    const plan = planPopupMenus(['message'], {type: 'open', id: 'message', sameTarget});
    assert.equal(plan.reposition, reposition, `случай "${name}": признак перестановки меню выставлен неверно`);
    assert.deepEqual(plan.close, close, `случай "${name}": закрыто лишнее меню`);
  });
});

// S-006, S-007: подменю меток держится за родителя только при наведении.
test('S-006: при наведении меню письма остаётся открытым', () => {
  const plan = planPopupMenus(['message'], {type: 'open', id: 'flag', hover: true});
  assert.deepEqual(plan.open.sort(), ['flag', 'message']);
  assert.deepEqual(plan.close, []);
});
test('S-007: без наведения подменю остаётся единственным', () => {
  const plan = planPopupMenus(['message'], {type: 'open', id: 'flag', hover: false});
  assert.deepEqual(plan.close, ['message']);
});
test('S-006: наведение закрывает прочие меню, кроме родителя', () => {
  const plan = planPopupMenus(['message', 'filter'], {type: 'open', id: 'flag', hover: true});
  assert.deepEqual(plan.close, ['filter']);
});

// S-008, S-011: Escape закрывает всё открытое, каким бы ни был набор.
test('S-011: Escape закрывает все открытые меню', () => {
  const cases = [
    ['пара меню письма и подменю меток', ['message', 'flag'], ['flag', 'message']],
    ['меню всех прочих видов', ['tag', 'contact', 'attachment', 'more', 'icon', 'color'], ['attachment', 'color', 'contact', 'icon', 'more', 'tag']],
  ];
  cases.forEach(([name, open, expected]) => {
    const plan = planPopupMenus(open, {type: 'escape'});
    assert.deepEqual(plan.close.sort(), expected, `случай "${name}": Escape закрыл не все меню`);
  });
});

// S-008: закрытие меню письма тянет за собой подменю меток - по этому списку
// окно гасит зависимые меню, даже если о них не просили.
test('S-008: закрытие меню письма тянет за собой подменю', () => {
  assert.deepEqual(withDependentMenus(['message']).sort(), ['flag', 'message']);
  assert.deepEqual(withDependentMenus(['folder']), ['folder']);
});
