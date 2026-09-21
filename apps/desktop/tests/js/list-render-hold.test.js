// Проверки отсрочки перестроения окна списка писем: пока кнопка указателя
// прижата к списку, строки не пересоздаются - иначе замена узла между
// прижатием и отпусканием съедает нажатие, и письмо не открывается (issue #61).
// Правила решения проверяются таблицей, а вся связка целиком - на настоящем
// окне: удержание начинается событием pointerdown на списке, а заканчивается
// pointerup на документе. Снятая привязка pointerup оставляет список замершим
// навсегда, и раньше это проходило мимо всех проверок файла.
// Спецификация: specs/message-click-not-lost.md.
// Запуск: node --test apps/desktop/tests/js/list-render-hold.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const {planWindowRender, releaseWindowRender} = require('../../ui/modules/list-render-hold.js');
const {createUiWindow} = require('./ui-window.js');

test('S-002 - S-007: решение о перестроении по удержанию, признаку и отложенному запросу', () => {
  // Таблица держит все ветки правила разом: без удержания перестроение идёт
  // сразу и забирает накопленный признак, при удержании запрос только
  // копится, а принудительность не теряется ни в одной из веток.
  const cases = [
    {held: false, force: false, pending: null, render: {force: false}, pendingAfter: null,
      why: 'без удержания перестроение обязано идти сразу'},
    {held: false, force: true, pending: null, render: {force: true}, pendingAfter: null,
      why: 'признак принудительного перестроения потерян по дороге'},
    {held: false, force: false, pending: {force: true}, render: {force: true}, pendingAfter: null,
      why: 'накопленная принудительность пропала: отложенное перестроение выполнится обычным'},
    {held: true, force: false, pending: null, render: null, pendingAfter: {force: false},
      why: 'во время удержания список перестроился: нажатие по строке будет съедено'},
    {held: true, force: true, pending: null, render: null, pendingAfter: {force: true},
      why: 'принудительный запрос во время удержания перестроил список'},
    {held: true, force: false, pending: {force: true}, render: null, pendingAfter: {force: true},
      why: 'принудительность прежнего отложенного запроса потеряна'},
  ];
  cases.forEach(item => {
    const plan = planWindowRender(item.held, item.force, item.pending);
    assert.deepEqual(plan.render, item.render, item.why);
    assert.deepEqual(plan.pending, item.pendingAfter, `${item.why}: отложенное состояние собрано неверно`);
  });

  // Конец удержания: накопленное выполняется один раз, пустое не будит список.
  let pending = null;
  for (let step = 0; step < 5; step += 1) pending = planWindowRender(true, step === 2, pending).pending;
  const release = releaseWindowRender(pending);
  assert.deepEqual(release.render, {force: true},
    'пять отложенных запросов с одним принудительным дали не одно принудительное перестроение');
  assert.equal(release.pending, null, 'после конца удержания отложенный запрос остался: список перестроится ещё раз');
  assert.deepEqual(releaseWindowRender(null), {render: null, pending: null},
    'конец удержания без отложенных запросов перестроил список зря');
});

// Список писем в окне: строки строит сам интерфейс, поэтому пересоздание узла
// видно по тому, тот ли это объект, что был до перестроения.
async function openList() {
  const ui = createUiWindow({globals: {reloadCoreData: async () => {}}});
  await ui.clock.drain();
  ui.evaluate(`
    coreAccounts=[{id:1,email:'me@example.test',color:'#0058ff'}];
    setCoreFolders([{id:2,account_id:1,role:'inbox',display_name:'Входящие',remote_path:'INBOX'}]);
    currentFolderId=2;currentSmartIndex=null;
    messages=[{id:11,account_id:1,folder_id:2,subject:'Первое',preview:'п',from:{name:'Босс',email:'boss@example.test'},to:[],cc:[],flags:{seen:true,flagged:false},labels:[],date:'2026-09-19T10:00:00Z',pinned_at:null,task_due_at:null}];
    applyListOptions(true,'Входящие');
  `);
  await ui.clock.drain();
  return ui;
}

const rowOf = ui => ui.query('#msgs .msg[data-message-id="11"]');

// Новое письмо приходит из ядра при перезагрузке данных - то же самое делает
// applyListOptions после любого события в списке.
function addMessage(ui) {
  ui.evaluate(`
    messages=messages.concat([{id:12,account_id:1,folder_id:2,subject:'Второе',preview:'п',from:{name:'Кто',email:'k@example.test'},to:[],cc:[],flags:{seen:true,flagged:false},labels:[],date:'2026-09-20T10:00:00Z',pinned_at:null,task_due_at:null}]);
    applyListOptions(false,'Входящие');
  `);
}

test('S-002, S-003: пока кнопка прижата к списку, строки не пересоздаются, а после отпускания список догоняет', async () => {
  const ui = await openList();
  const before = rowOf(ui);
  assert.ok(before, 'список не построился: проверять удержание не на чем');

  ui.byId('msgs').dispatch('pointerdown', {button: 0});
  addMessage(ui);
  await ui.clock.drain();
  assert.equal(rowOf(ui), before,
    'строка пересоздана во время удержания: нажатие по ней будет съедено, письмо не откроется');
  assert.equal(ui.queryAll('#msgs .msg').length, 1,
    'список перестроился во время удержания: разметка сменилась под прижатой кнопкой');

  // Отпускание кнопки приходит на документ: кнопку могли отпустить, уведя
  // курсор за пределы списка, и события самого списка тогда не будет вовсе.
  ui.document.dispatch('pointerup');
  await ui.clock.advance(1);
  assert.equal(ui.queryAll('#msgs .msg').length, 2,
    'после отпускания кнопки список не догнал данные: он замер до следующего нажатия');
  assert.notEqual(rowOf(ui), before, 'строки не пересозданы после конца удержания: отложенное перестроение не выполнилось');
});

test('S-006: потеря ввода окном снимает удержание, иначе список замер бы навсегда', async () => {
  const ui = await openList();
  const before = rowOf(ui);
  ui.byId('msgs').dispatch('pointerdown', {button: 0});
  addMessage(ui);
  await ui.clock.drain();
  assert.equal(rowOf(ui), before, 'удержание не началось: проверять страховку не на чем');

  // Событие отпускания может не прийти вовсе - окно потеряло ввод с прижатой
  // кнопкой (переключение окна, скрытие). Без страховки список замер бы.
  ui.context.window.dispatchWindowEvent('blur');
  await ui.clock.drain();
  assert.equal(ui.queryAll('#msgs .msg').length, 2,
    'после потери ввода окном список остался замершим: отпускания кнопки он уже не дождётся');
});

test('S-002: нажатие правой кнопкой на списке удержания не начинает', async () => {
  const ui = await openList();
  // Удержание заводит только основная кнопка: правый клик открывает меню и
  // перестроению не мешает, а лишнее удержание останавливало бы список до
  // следующего отпускания.
  ui.byId('msgs').dispatch('pointerdown', {button: 2});
  addMessage(ui);
  await ui.clock.drain();
  assert.equal(ui.queryAll('#msgs .msg').length, 2,
    'нажатие правой кнопкой остановило перестроение списка');
});
