// Проверки кнопок строки списка писем: флажок, закрепление и срок дела.
// Проверяется настоящий путь: разметка окна берётся из index.html, модули
// интерфейса выполняются целиком, строки списка строит сам mail.js, а нажатия
// идут теми же обработчиками, которые вызовет браузер. Итог смотрим по тому,
// что уходит в мост к ядру.
// Раньше эти кнопки не проверялись вовсе: снятая привязка flag.onclick или
// pin.onclick оставляла все проверки проекта зелёными, а кнопка в окне
// переставала работать.
// Спецификации: specs/message-flag-due-dates.md, specs/pin-message.md.
// Запуск: node --test apps/desktop/tests/js/message-row-actions.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const {createUiWindow} = require('./ui-window.js');

// Письма списка: у первого меток и срока нет, у второго стоит флажок и
// просроченный срок - обе кнопки строки должны отличаться по виду.
function sampleMessages() {
  return [
    {id: 11, account_id: 1, folder_id: 2, subject: 'Счёт', preview: 'оплата', from: {name: 'Босс', email: 'boss@example.test'},
      to: [], cc: [], flags: {seen: true, flagged: false}, labels: [], date: '2026-09-19T10:00:00Z', pinned_at: null, task_due_at: null},
    {id: 12, account_id: 1, folder_id: 2, subject: 'Отчёт', preview: 'срок', from: {name: 'Коллега', email: 'mate@example.test'},
      to: [], cc: [], flags: {seen: true, flagged: true}, labels: [], date: '2026-09-18T10:00:00Z', pinned_at: null,
      task_due_at: '2026-09-19T12:00:00Z'},
  ];
}

async function openList(options = {}) {
  const ui = createUiWindow({
    answers: {getMessageTask: () => ({message_id: 12, start_at: null, due_at: '2026-09-19T12:00:00Z', reminder_at: null, state: 'active'}), ...(options.answers || {})},
    globals: {reloadCoreData: async () => {}, ...(options.globals || {})},
  });
  await ui.clock.drain();
  // Список строится тем же путём, что и в окне: открыта папка входящих, письма
  // лежат в общем перечне, а разметку собирает applyListOptions - её же зовут
  // обработчики кнопок после ответа ядра.
  ui.evaluate(`
    coreAccounts=[{id:1,email:'me@example.test',color:'#0058ff'}];
    setCoreFolders([{id:2,account_id:1,role:'inbox',display_name:'Входящие',remote_path:'INBOX'}]);
    currentFolderId=2;currentSmartIndex=null;
    messages=${JSON.stringify(sampleMessages())};
    applyListOptions(true,'Входящие');
  `);
  return ui;
}

const rowOf = (ui, id) => ui.query(`#msgs .msg[data-message-id="${id}"]`);

test('S-001: флажок в строке письма уходит в ядро тем письмом, по которому нажали', async () => {
  const ui = await openList();
  const row = rowOf(ui, 11);
  assert.ok(row, 'строка письма не построилась: проверять нечего');
  const flag = row.querySelector('.row-flag');
  assert.ok(flag, 'в строке списка нет кнопки флажка');
  assert.equal(flag.classes.has('on'), false, 'флажок нарисован поднятым у письма без флажка');

  flag.dispatch('click');
  await ui.clock.drain();
  // Перечень номеров рождается внутри подставного окна, поэтому переносим его
  // в обычный массив: иначе сравнение спорит о происхождении массива.
  assert.deepEqual(ui.callsOf('markFlagged').map(call => [[...call.args[0]], call.args[1], call.args[2]]), [[[11], true, 'user']],
    'нажатие на флажок строки не дошло до ядра: кнопка в окне ничего не делает');
  // Нажатие на кнопку строки не должно открывать письмо: обработчик строки
  // обязан остановиться на кнопке.
  assert.equal(ui.callsOf('getMessage').length, 0,
    'нажатие на флажок открыло письмо: событие кнопки дошло до обработчика строки');
  assert.equal(rowOf(ui, 11).querySelector('.row-flag').classes.has('on'), true,
    'вид флажка не обновился: пользователь не видит, что флажок поставлен');

  // Повторное нажатие снимает флажок - и ядру уходит снятие, а не повторная
  // установка.
  rowOf(ui, 11).querySelector('.row-flag').dispatch('click');
  await ui.clock.drain();
  const repeat = ui.callsOf('markFlagged').at(-1);
  assert.deepEqual([[...repeat.args[0]], repeat.args[1], repeat.args[2]], [[11], false, 'user'],
    'повторное нажатие не сняло флажок');
});

test('S-024: флажок при групповом выделении уходит по всем выбранным письмам', async () => {
  const ui = await openList();
  rowOf(ui, 11).dispatch('click', {ctrlKey: true});
  rowOf(ui, 12).dispatch('click', {ctrlKey: true});
  await ui.clock.drain();
  assert.equal(ui.evaluate('selectedMessageIds.size'), 2, 'выделить два письма не удалось: проверять групповое действие не на чем');

  rowOf(ui, 11).querySelector('.row-flag').dispatch('click');
  await ui.clock.drain();
  const call = ui.callsOf('markFlagged').at(-1);
  assert.ok(call, 'групповое нажатие на флажок не дошло до ядра');
  assert.deepEqual([...call.args[0]].sort((a, b) => a - b), [11, 12],
    'флажок поставлен только письму под курсором: остальные выбранные письма пропущены');
});

test('S-002: закрепление в строке письма уходит в ядро и не открывает письмо', async () => {
  const ui = await openList();
  const pin = rowOf(ui, 12).querySelector('.row-pin');
  assert.ok(pin, 'в строке списка нет кнопки закрепления');
  pin.dispatch('click');
  await ui.clock.drain();
  assert.deepEqual(ui.callsOf('setMessagesPinned').map(call => [[...call.args[0]], call.args[1]]), [[[12], true]],
    'нажатие на закрепление не дошло до ядра: кнопка в окне ничего не делает');
  assert.equal(ui.callsOf('getMessage').length, 0,
    'нажатие на закрепление открыло письмо: событие кнопки дошло до обработчика строки');
});

test('S-048: кнопка срока показывает просрочку и открывает окно срока', async () => {
  const ui = await openList();
  const due = rowOf(ui, 12).querySelector('.task-due');
  assert.ok(due, 'у письма со сроком нет кнопки срока в строке');
  assert.ok(due.classes.has('overdue'), 'прошедший срок не помечен просрочкой: пользователь не видит, что срок истёк');
  assert.ok(due.textContent.trim().length, 'кнопка срока пуста: показывать пользователю нечего');
  assert.equal(rowOf(ui, 11).querySelector('.task-due'), null, 'у письма без срока появилась кнопка срока');

  due.dispatch('click');
  await ui.clock.drain();
  assert.equal(ui.callsOf('getMessage').length, 0, 'нажатие на срок открыло письмо вместо окна срока');
  assert.equal(ui.callsOf('getMessageTask').length, 1, 'окно срока не запросило дело письма у ядра');
  assert.ok(ui.query('.task-modal'), 'окно срока не открылось');
});
