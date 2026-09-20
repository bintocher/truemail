// Проверки окна выбора срока письма (openMessageTaskEditor). Проверяется
// настоящий путь: разметка окна берётся из index.html, модули выполняются
// целиком, окно срока открывается кнопкой срока в строке списка, а нажатия
// идут теми же обработчиками, которые вызовет браузер. Итог смотрим по тому,
// что уходит в мост к ядру.
// Раньше окно срока не проверялось вовсе: снятая привязка кнопки готового
// срока, потерянная проверка сроков перед сохранением или отметка о выполнении
// не по состоянию дела оставались незамеченными.
// Спецификация: specs/message-flag-due-dates.md.
// Запуск: node --test apps/desktop/tests/js/message-task-editor.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const {createUiWindow} = require('./ui-window.js');

// Готовые сроки окна считаются от текущего момента настоящим Date, поэтому
// момент задаётся окну подставным Date: иначе ожидание "завтра в 09:00"
// зависело бы от часа запуска проверки.
const NOW = new Date(2026, 8, 20, 10, 0, 0, 0);

function fixedDate() {
  return class FixedDate extends Date {
    constructor(...args) {
      if (args.length === 0) super(NOW.getTime());
      else super(...args);
    }

    static now() { return NOW.getTime(); }
  };
}

// Значение, которое окно обязано отдать ядру для готового срока "завтра в
// 09:00": оно собрано здесь из календарных суток, а не вызовом того же кода,
// который проверяется.
function localIso(year, month, day, hours, minutes) {
  return new Date(year, month, day, hours, minutes, 0, 0).toISOString();
}

function sampleMessages() {
  return [
    {id: 12, account_id: 1, folder_id: 2, subject: 'Отчёт', preview: 'срок', from: {name: 'Коллега', email: 'mate@example.test'},
      to: [], cc: [], flags: {seen: true, flagged: true}, labels: [], date: '2026-09-18T10:00:00Z', pinned_at: null,
      task_due_at: '2026-09-22T09:00:00Z', task_state: 'active'},
  ];
}

async function openTaskEditor(options = {}) {
  const reloads = [];
  const ui = createUiWindow({
    answers: {
      getMessageTask: () => ({message_id: 12, start_at: null, due_at: '2026-09-22T09:00:00Z', reminder_at: null,
        state: options.state || 'active'}),
      ...(options.answers || {}),
    },
    globals: {Date: fixedDate(), reloadCoreData: async () => { reloads.push(1); }},
  });
  await ui.clock.drain();
  ui.evaluate(`
    coreAccounts=[{id:1,email:'me@example.test',color:'#0058ff'}];
    setCoreFolders([{id:2,account_id:1,role:'inbox',display_name:'Входящие',remote_path:'INBOX'}]);
    currentFolderId=2;currentSmartIndex=null;
    messages=${JSON.stringify(sampleMessages())};
    applyListOptions(true,'Входящие');
  `);
  // Окно срока открывается тем же путём, что и в программе: кнопкой срока в
  // строке списка писем.
  ui.query('#msgs .msg[data-message-id="12"] .task-due').dispatch('click');
  await ui.clock.drain();
  assert.ok(ui.query('.task-modal'), 'окно срока не открылось: проверять нечего');
  return {ui, reloads};
}

const field = (ui, selector) => ui.query(`.task-modal ${selector}`);
// Сообщения окна смотрим там же, где их видит пользователь: карточкой в углу
// окна. Так проверка ловит и обрыв показа сообщения, а не только его сборку.
const noticeTexts = ui => ui.queryAll('.app-toast .app-toast-line').map(node => node.textContent.trim());

test('S-016, S-020: готовый срок в окне уходит в ядро выбранным значением', async () => {
  const {ui, reloads} = await openTaskEditor();
  // Кнопки готовых сроков строятся по duePresets, и "без срока" среди них
  // остаётся: значение null обязано доходить до поля пустым, а не текущей датой.
  const tomorrow = field(ui, '[data-due="tomorrow"]');
  assert.ok(tomorrow, 'в окне нет кнопки готового срока "завтра": срок придётся набирать вручную');
  tomorrow.dispatch('click');
  assert.ok(field(ui, '.task-due-input').value, 'нажатие на готовый срок не заполнило поле срока исполнения');

  field(ui, '.task-save').dispatch('click');
  await ui.clock.drain();
  const saved = ui.callsOf('saveMessageTask');
  assert.equal(saved.length, 1, 'сохранение срока не дошло до ядра: кнопка в окне ничего не делает');
  assert.equal(saved[0].args[0].message_id, 12, 'срок сохранён не тому письму, по которому открыли окно');
  assert.equal(saved[0].args[0].due_at, localIso(2026, 8, 21, 9, 0),
    'в ядро ушёл не выбранный готовый срок: напоминание придёт не в то время');
  assert.equal(saved[0].args[0].start_at, null, 'пустое поле срока начала ушло в ядро заполненным');
  assert.equal(ui.query('.task-modal'), null, 'окно срока осталось открытым после сохранения');
  assert.equal(reloads.length, 1, 'список не перечитан после сохранения: строка письма покажет прежний срок');
});

test('S-019, S-021, S-022: неверные сроки называются пользователю и в ядро не уходят', async () => {
  const {ui} = await openTaskEditor();
  // Срок начала позже срока исполнения: молчаливый отказ выглядел бы потерей
  // набранных сроков, а сохранение завело бы дело с невыполнимыми сроками.
  field(ui, '.task-start').value = '2026-09-25T09:00';
  field(ui, '.task-due-input').value = '2026-09-21T09:00';
  field(ui, '.task-save').dispatch('click');
  await ui.clock.drain();
  assert.equal(ui.callsOf('saveMessageTask').length, 0,
    'срок начала позже срока исполнения ушёл в ядро: проверки перед сохранением нет');
  const notices = noticeTexts(ui);
  assert.equal(notices.length, 1, 'отказ прошёл молча: пользователь не узнает, почему срок не сохранился');
  assert.ok(/срок/i.test(notices[0]), `отказ не называет причину: ${notices[0]}`);
  assert.ok(ui.query('.task-modal'), 'окно закрылось при отказе: набранные сроки потеряны');
  assert.equal(field(ui, '.task-start').value, '2026-09-25T09:00', 'набранный срок начала стёрт при отказе');

  // Исправление сроков тем же окном обязано проходить: отказ не должен
  // запирать окно навсегда.
  field(ui, '.task-start').value = '2026-09-21T08:00';
  field(ui, '.task-save').dispatch('click');
  await ui.clock.drain();
  assert.equal(ui.callsOf('saveMessageTask').length, 1, 'исправленные сроки так и не сохранились');
  assert.equal(ui.callsOf('saveMessageTask')[0].args[0].start_at, localIso(2026, 8, 21, 8, 0),
    'сохранён не исправленный срок начала');
});

test('S-026, S-027: отметка о выполнении идёт своей командой, а выполненное дело возвращается в работу', async () => {
  const {ui, reloads} = await openTaskEditor();
  const complete = field(ui, '.task-complete');
  assert.equal(complete.textContent.trim(), 'Выполнено', 'кнопка дела в работе подписана не по состоянию дела');
  complete.dispatch('click');
  await ui.clock.drain();
  assert.equal(ui.callsOf('completeMessageTask').length, 1,
    'отметка о выполнении не дошла до ядра: кнопка в окне ничего не делает');
  assert.equal(ui.callsOf('saveMessageTask').length, 0,
    'отметка о выполнении прошла сохранением сроков: выполненное дело осталось бы в работе');
  assert.equal(ui.query('.task-modal'), null, 'окно осталось открытым после отметки о выполнении');
  assert.equal(reloads.length, 1, 'список не перечитан: письмо останется в делах в работе');

  // Выполненное дело открывается той же кнопкой, но возвращается в работу
  // другой командой: одна команда на оба состояния переключала бы дело не туда.
  const done = await openTaskEditor({state: 'done'});
  const reopen = field(done.ui, '.task-complete');
  assert.equal(reopen.textContent.trim(), 'Вернуть в работу', 'у выполненного дела кнопка подписана как у дела в работе');
  reopen.dispatch('click');
  await done.ui.clock.drain();
  assert.equal(done.ui.callsOf('reopenMessageTask').length, 1,
    'выполненное дело не вернулось в работу: в ядро ушла не та команда');
  assert.equal(done.ui.callsOf('completeMessageTask').length, 0,
    'выполненное дело отмечено выполненным ещё раз');
});
