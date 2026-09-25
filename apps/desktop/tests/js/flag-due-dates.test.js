// Проверки сроков у флажка письма и раздела "Дела": готовые сроки,
// согласованность сроков, группы и порядок списка дел, карточка напоминания,
// а также настоящие пути окна - раздел "Дела", подпись срока в строке списка
// писем и снятие флажка с писем, у которых заданы сроки.
// Пути окна проверяются на настоящей разметке из index.html: модули окна
// выполняются целиком, нажатия идут теми же обработчиками, которые вызовет
// браузер, а итог смотрится по тому, что уходит в мост к ядру. Чистыми
// остались только те проверки, где счёт времени не зависит от разметки.
// Спецификация: specs/flag-due-dates.md.
// Запуск: node --test apps/desktop/tests/js/flag-due-dates.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const {createUiWindow} = require('./ui-window.js');

// Модуль сроков флажка появляется вместе с реализацией. Пока его нет, каждая
// проверка падает на своём месте и своими словами, а не одной общей ошибкой
// загрузки файла.
function loadFlagModule() {
  try {
    return require('../../ui/modules/flag-due-dates.js');
  } catch (error) {
    if (error && error.code === 'MODULE_NOT_FOUND') return null;
    throw error;
  }
}

const flag = loadFlagModule();

function fn(name, whatBreaks) {
  if (flag && typeof flag[name] === 'function') return flag[name];
  return () => {
    throw new Error(`${name}: ${whatBreaks}`);
  };
}

// Имена границы выбраны автором кода: модуль отдаёт duePresets, validateTask,
// taskGroup, groupLabel, formatDue и taskErrorText, а время напоминания зовёт
// reminder_at - тем же именем, что и ядро. Ниже только приведение обращений.
function taskTimesError(times, now, lang) {
  const check = fn('validateTask', 'сроки принимаются без проверки: дело получит срок начала позже срока исполнения')(times, now);
  if (check.ok) return null;
  return fn('taskErrorText', 'причина отказа не называется: сроки пропадают молча')(check.reason, lang);
}

const NOW = Date.parse('2026-09-18T12:00:00Z');

function task(extra) {
  return Object.assign(
    {
      message_id: 1,
      subject: 'Договор',
      from_name: 'Начальник',
      from_addr: 'boss@example.test',
      account_email: 'me@example.test',
      folder_id: 10,
      state: 'active',
      due_at: null,
      start_at: null,
      reminder_at: null,
      completed_at: null,
      snoozed_until: null,
      takeaway_pending: false,
    },
    extra,
  );
}

// Окно считает сроки настоящим Date, поэтому проверкам путей момент задаётся
// подставным Date: иначе "сегодня в 21:30" зависело бы от часа запуска.
const WINDOW_NOW = new Date(2026, 8, 20, 10, 0, 0, 0);

function fixedDate() {
  return class FixedDate extends Date {
    constructor(...args) {
      if (args.length === 0) super(WINDOW_NOW.getTime());
      else super(...args);
    }

    static now() { return WINDOW_NOW.getTime(); }
  };
}

// Время задаётся календарными сутками компьютера: сроки дел живут в местном
// часовом поясе, а не во всемирном времени.
function localIso(day, hours, minutes) {
  return new Date(2026, 8, day, hours, minutes, 0, 0).toISOString();
}

function createWindow(options = {}) {
  const reloads = [];
  const ui = createUiWindow({
    answers: options.answers || {},
    globals: {Date: fixedDate(), reloadCoreData: async () => { reloads.push(1); }, ...(options.globals || {})},
  });
  ui.reloads = reloads;
  return ui;
}

// Список писем строится тем же путём, что и в окне: открыта папка входящих,
// письма лежат в общем перечне, а разметку собирает applyListOptions.
async function openList(messages, options = {}) {
  const ui = createWindow(options);
  await ui.clock.drain();
  ui.evaluate(`
    coreAccounts=[{id:1,email:'me@example.test',color:'#0058ff'}];
    setCoreFolders([{id:2,account_id:1,role:'inbox',display_name:'Входящие',remote_path:'INBOX'}]);
    currentFolderId=2;currentSmartIndex=null;
    messages=${JSON.stringify(messages)};
    applyListOptions(true,'Входящие');
  `);
  return ui;
}

function listMessage(extra) {
  return Object.assign({
    id: 1, account_id: 1, folder_id: 2, subject: 'Отчёт', preview: 'текст',
    from: {name: 'Коллега', email: 'mate@example.test'}, to: [], cc: [],
    flags: {seen: true, flagged: false}, labels: [], date: '2026-09-18T10:00:00Z',
    pinned_at: null, task_due_at: null, task_start_at: null, task_reminder_at: null,
  }, extra);
}

const rowOf = (ui, id) => ui.query(`#msgs .msg[data-message-id="${id}"]`);
// Сообщения смотрим там же, где их видит пользователь: карточкой в углу окна.
const noticeTexts = ui => ui.queryAll('#activityPanel .activity-entry-text').map(node => node.textContent.trim());

test('S-016: готовые сроки отсчитываются от текущего момента', () => {
  // Неверно посчитанный готовый срок молча ставит напоминание не на то время:
  // ошибку видно только тогда, когда напоминание не пришло.
  const presets = now => Object.fromEntries(
    fn('duePresets', 'готовых сроков нет: срок ставится только вручную')(now, 'ru')
      .map(preset => [preset.id, preset.value]));
  // Вторник, середина дня: ни одно значение не попадает на границу суток.
  const tuesday = new Date(2026, 8, 15, 12, 30, 45, 123);
  const value = presets(tuesday);
  assert.equal(value.hour.getTime() - tuesday.getTime(), 3600000,
    '"через час" считается не от текущего момента');
  assert.deepEqual(
    [value.today.getDate(), value.today.getHours(), value.today.getMinutes(), value.today.getSeconds()],
    [15, 23, 59, 59], '"сегодня" обязано означать конец текущих суток');
  assert.deepEqual([value.evening.getDate(), value.evening.getHours(), value.evening.getMinutes()],
    [15, 18, 0], '"сегодня вечером" обязано означать 18:00 текущих суток');
  assert.deepEqual([value.tomorrow.getDate(), value.tomorrow.getHours()], [16, 9],
    '"завтра в 09:00" уехало с завтрашнего утра');
  assert.deepEqual([value.monday.getDay(), value.monday.getDate(), value.monday.getHours()], [1, 21, 9],
    '"в понедельник в 09:00" указывает не на ближайший понедельник');
  assert.deepEqual([value.next_week.getMonth(), value.next_week.getDate(), value.next_week.getHours()], [8, 22, 9],
    '"через неделю" считается не семью сутками вперёд');
  assert.deepEqual([value.next_month.getMonth(), value.next_month.getDate(), value.next_month.getHours()], [9, 15, 9],
    '"через месяц" считается не месяцем вперёд');
  assert.equal(value.none, null, '"без срока" обязано оставаться пустым значением');
  // Час за полночь и месяц от длинного месяца - две границы, на которых
  // наивный сдвиг даты промахивается.
  const lateEvening = presets(new Date(2026, 8, 15, 23, 30, 0, 0));
  assert.deepEqual([lateEvening.hour.getDate(), lateEvening.hour.getHours()], [16, 0],
    '"через час" перед полуночью обязан уходить на следующие сутки');
  const marchEnd = presets(new Date(2026, 2, 31, 10, 0, 0, 0));
  assert.deepEqual([marchEnd.next_month.getMonth(), marchEnd.next_month.getDate()], [3, 30],
    'месяц от 31 марта обязан давать 30 апреля, а не 1 мая');
});

test('S-019 - S-023: срок начала не позже исполнения, напоминание не в прошлом, пустой срок исполнения допустим', () => {
  // Отклонённое сохранение обязано назвать причину: молчаливый отказ выглядит
  // потерей введённых сроков.
  const validate = taskTimesError;
  assert.equal(validate({start_at: '2026-09-19T09:00:00Z', due_at: '2026-09-20T09:00:00Z', reminder_at: null}, NOW, 'ru'), null);
  assert.equal(validate({start_at: '2026-09-20T09:00:00Z', due_at: '2026-09-20T09:00:00Z', reminder_at: null}, NOW, 'ru'), null,
    'равные сроки начала и исполнения допустимы: запрещено только "позже"');
  const reversed = validate({start_at: '2026-09-21T09:00:00Z', due_at: '2026-09-20T09:00:00Z', reminder_at: null}, NOW, 'ru');
  assert.ok(reversed, 'срок начала позже срока исполнения принят');
  assert.ok(/срок/i.test(reversed), `отказ обязан объяснять причину: ${reversed}`);
  const past = validate({start_at: null, due_at: null, reminder_at: '2026-09-18T11:59:00Z'}, NOW, 'ru');
  assert.ok(past, 'время напоминания в прошлом принято: напоминание не пришло бы никогда');
  // Напоминание без срока исполнения и позже него - обычные случаи, а не ошибка.
  assert.equal(validate({start_at: null, due_at: null, reminder_at: '2026-09-19T09:00:00Z'}, NOW, 'ru'), null);
  assert.equal(validate({start_at: null, due_at: '2026-09-19T09:00:00Z', reminder_at: '2026-09-25T09:00:00Z'}, NOW, 'ru'), null);
});

// Границы групп проходят по суткам часового пояса компьютера, а не по суткам
// всемирного времени: иначе вечернее дело попадало бы в "Завтра" у всех, кто
// живёт восточнее Гринвича.
function localDue(dayShift, hours, minutes) {
  const value = new Date(NOW);
  value.setDate(value.getDate() + dayShift);
  value.setHours(hours, minutes, 0, 0);
  return value.toISOString();
}

test('S-046: список дел разбит на группы по границам суток', () => {
  const group = fn('taskGroup', 'список дел не разделяется на группы: просроченное смешано с сегодняшним');
  assert.equal(group(task({due_at: localDue(-1, 9, 0)}), NOW), 'overdue');
  assert.equal(group(task({due_at: localDue(0, 23, 59)}), NOW), 'today');
  assert.equal(group(task({due_at: localDue(1, 0, 1)}), NOW), 'tomorrow');
  // "На неделе" - ближайшие 7 суток после завтрашнего дня.
  assert.equal(group(task({due_at: localDue(3, 10, 0)}), NOW), 'week');
  assert.equal(group(task({due_at: localDue(42, 10, 0)}), NOW), 'later');
  assert.equal(group(task({due_at: null}), NOW), 'none');
  assert.equal(group(task({state: 'done', due_at: localDue(-1, 9, 0), completed_at: localDue(0, 8, 0)}), NOW), 'done',
    'выполненное дело уходит в свою группу, а не остаётся просроченным');
});

test('S-047: список дел выстроен полным порядком', () => {
  // Полный порядок: группа, срок по возрастанию, дата письма по убыванию,
  // номер письма по убыванию. Неполный порядок дал бы повтор и потерю строк
  // между страницами.
  const sorted = fn('sortTasks', 'порядок списка дел не задан: страницы будут повторять и терять строки')([
    task({message_id: 3, due_at: '2026-09-19T09:00:00Z', date: '2026-09-10T10:00:00Z'}),
    task({message_id: 1, due_at: '2026-09-17T09:00:00Z', date: '2026-09-10T10:00:00Z'}),
    task({message_id: 5, due_at: '2026-09-19T09:00:00Z', date: '2026-09-12T10:00:00Z'}),
    task({message_id: 4, due_at: '2026-09-19T09:00:00Z', date: '2026-09-10T10:00:00Z'}),
  ], NOW);
  assert.deepEqual(sorted.map(item => item.message_id), [1, 5, 4, 3]);
});

test('S-046, S-048, S-049, S-052: раздел "Дела" строит строки по ответу ядра и не показывает уводимое письмо', async () => {
  // Настоящий путь раздела: нажатие на пункт "Дела" - чтение страниц у ядра -
  // строки списка. Здесь ловится и потеря подписи письма в строке, и
  // перепутанные группы, и уводимое письмо, которое в списках писем уже
  // исчезло, а в делах осталось бы висеть (признак ядра зовётся has_takeaway).
  const ui = createWindow({
    answers: {
      listMessageTasks: () => ({
        items: [
          {task: {message_id: 32, start_at: null, due_at: localIso(20, 18, 0), reminder_at: null, state: 'active'},
            account_id: 1, account_email: 'me@example.test', folder_id: 2, subject: '',
            sender_name: null, sender_address: null, message_date: '2026-09-19T10:00:00Z',
            snoozed_until: null, has_takeaway: false},
          {task: {message_id: 31, start_at: null, due_at: localIso(19, 9, 0), reminder_at: null, state: 'active'},
            account_id: 1, account_email: 'me@example.test', folder_id: 2, subject: 'Договор',
            sender_name: 'Начальник', sender_address: 'boss@example.test', message_date: '2026-09-18T10:00:00Z',
            snoozed_until: null, has_takeaway: false},
          {task: {message_id: 33, start_at: null, due_at: localIso(20, 19, 0), reminder_at: null, state: 'active'},
            account_id: 1, account_email: 'me@example.test', folder_id: 2, subject: 'Уводится',
            sender_name: 'Никто', sender_address: 'nobody@example.test', message_date: '2026-09-17T10:00:00Z',
            snoozed_until: null, has_takeaway: true},
        ],
        next_cursor: null,
      }),
    },
  });
  await ui.clock.drain();
  ui.evaluate(`coreAccounts=[{id:1,email:'me@example.test',color:'#0058ff'}];
    setCoreFolders([{id:2,account_id:1,role:'inbox',display_name:'Входящие',remote_path:'INBOX'}]);`);
  ui.byId('tasksNav').dispatch('click');
  await ui.clock.drain();

  assert.equal(ui.callsOf('listMessageTasks').length, 1, 'раздел "Дела" не запросил список у ядра');
  // Порядок строк с разделителями: просроченное выше сегодняшнего, каждая
  // группа подписана. Названия групп записаны здесь словами, а не собраны тем
  // же кодом, который их строит.
  const rows = ui.queryAll('#msgs .msg').map(row => (row.classes.has('task-separator')
    ? row.textContent.trim()
    : Number(row.dataset.messageId)));
  assert.deepEqual(rows, ['Просрочено', 31, 'Сегодня', 32],
    'раздел "Дела" собран неверно: либо пропала группировка, либо уводимое письмо осталось в списке');

  const overdue = rowOf(ui, 31).querySelector('.prev').textContent;
  for (const piece of ['Договор', 'Начальник', 'me@example.test']) {
    assert.ok(overdue.includes(piece), `в строке дела нет "${piece}": письмо в списке не опознать - ${overdue}`);
  }
  const empty = rowOf(ui, 32).querySelector('.prev').textContent;
  assert.ok(empty.includes('Без темы'), `пустая тема в строке дела не подписана: ${empty}`);
  assert.ok(empty.includes('Отправитель неизвестен'), `неизвестный отправитель не подписан: ${empty}`);
});

test('S-052, S-053: уводимое письмо в списке дел не показывается, отложенное показывается с временем возврата', () => {
  const visible = fn('visibleTasks', 'список дел не отбирает письма: уводимое письмо покажется там, где его уже нет');
  const rows = visible([
    task({message_id: 1}),
    task({message_id: 2, takeaway_pending: true}),
    task({message_id: 3, snoozed_until: '2026-09-20T09:00:00Z'}),
  ], NOW);
  assert.deepEqual(rows.map(item => item.message_id), [1, 3],
    'уводимое письмо обязано исчезнуть, а отложенное - остаться: в списках писем его не видно, а срок по нему горит');
  const text = fn('taskRowText', 'строка списка дел пуста')(
    task({message_id: 3, snoozed_until: '2026-09-20T09:00:00Z'}), 'ru');
  assert.ok(/отложено|возврат/i.test(text), `отложенное письмо не подписано временем возврата: ${text}`);
});

test('S-018, S-055, S-056: подпись срока в строке письма называет время, завтрашний день и дату', async () => {
  // Ожидания записаны здесь словами и цифрами, а не собраны тем же
  // форматированием, что и код: иначе проверка повторяла бы код и молчала бы
  // при любой его ошибке. Сегодняшний срок - только время, завтрашний -
  // подписан словом, дальний - с числом и месяцем: без этого подпись "09:00" у
  // письма со сроком через неделю читалась бы как "сегодня".
  const ui = await openList([
    listMessage({id: 41, subject: 'Сегодня', task_due_at: localIso(20, 21, 30)}),
    listMessage({id: 42, subject: 'Завтра', task_due_at: localIso(21, 9, 5)}),
    listMessage({id: 43, subject: 'Позже', task_due_at: localIso(30, 9, 0)}),
  ]);
  const dueText = id => rowOf(ui, id).querySelector('.task-due').textContent.trim();

  assert.equal(dueText(41), '21:30',
    'срок сегодняшнего дня показан не одним временем: в узкой строке списка лишняя дата съедает подпись');
  const tomorrow = dueText(42);
  assert.ok(tomorrow.includes('Завтра'), `завтрашний срок не подписан словом "Завтра": ${tomorrow}`);
  assert.ok(tomorrow.includes('09:05'), `завтрашний срок потерял время: ${tomorrow}`);
  const later = dueText(43);
  assert.ok(!later.includes('Завтра'), `дальний срок подписан как завтрашний: ${later}`);
  assert.ok(later.includes('30') && /[а-я]/i.test(later), `дальний срок показан без числа и месяца: ${later}`);
  assert.ok(later.includes('09:00'), `дальний срок потерял время: ${later}`);
});

test('S-063, S-064: карточка напоминания называет письмо и предлагает три действия', () => {
  const card = fn('reminderCard', 'карточка напоминания не собирается: напоминание о сроке не покажется')(
    task({due_at: '2026-09-19T09:00:00Z', reminder_at: '2026-09-18T12:00:00Z'}), 'ru');
  assert.ok(card.title.includes('Договор') || card.body.includes('Договор'), 'в карточке нет темы письма');
  assert.ok(`${card.title} ${card.body}`.includes('Начальник'), 'в карточке нет отправителя');
  assert.ok(`${card.title} ${card.body}`.includes('19'), 'в карточке нет срока исполнения');
  assert.deepEqual(card.actions.map(action => action.id), ['open', 'snooze', 'done']);
  const empty = fn('reminderCard', 'карточка напоминания не собирается')(
    task({subject: '', reminder_at: '2026-09-18T12:00:00Z'}), 'ru');
  assert.ok(`${empty.title} ${empty.body}`.includes('Без темы'), 'пустая тема в карточке не подписана');
});

test('S-001, S-024, S-025, S-102: снятие флажка с писем со сроками спрашивает по всему выделению, отказ ничего не меняет', async () => {
  // Настоящий путь: кнопка флажка в строке списка - подтверждение - мост.
  // Сроки есть у обоих выделенных писем, причём у второго задано только
  // напоминание: вопрос, построенный по одному письму или только по сроку
  // исполнения, стёр бы чужие напоминания молча.
  const ui = await openList([
    listMessage({id: 51, subject: 'Счёт', flags: {seen: true, flagged: true}, task_due_at: localIso(21, 9, 0)}),
    listMessage({id: 52, subject: 'Отчёт', flags: {seen: true, flagged: true}, task_reminder_at: localIso(22, 9, 0)}),
    listMessage({id: 53, subject: 'Письмо', flags: {seen: true, flagged: false}, task_due_at: localIso(23, 9, 0)}),
  ]);
  rowOf(ui, 51).dispatch('click', {ctrlKey: true});
  rowOf(ui, 52).dispatch('click', {ctrlKey: true});
  await ui.clock.drain();
  assert.equal(ui.evaluate('selectedMessageIds.size'), 2, 'выделить два письма не удалось: проверять групповое снятие не на чем');

  rowOf(ui, 51).querySelector('.row-flag').dispatch('click');
  await ui.clock.drain();
  const cancel = ui.query('.overlay.open .confirm-cancel');
  assert.ok(cancel, 'снятие флажка у писем со сроками прошло без вопроса: напоминания удалены незаметно');
  const question = cancel.closest('.modal').querySelector('.mb').textContent;
  assert.ok(question.includes('2'), `вопрос не называет числа писем со сроками: ${question}`);
  assert.equal(rowOf(ui, 51).querySelector('.row-flag').classes.has('on'), true,
    'флажок снят с виду ещё до ответа на вопрос: отказ оставит строку с неверным значком');

  cancel.dispatch('click');
  await ui.clock.drain();
  assert.equal(ui.callsOf('markFlagged').length, 0, 'отказ от подтверждения всё равно снял флажок');
  assert.equal(rowOf(ui, 51).querySelector('.row-flag').classes.has('on'), true, 'после отказа флажок пропал из строки');
  assert.equal(ui.reloads.length, 0, 'после отказа список перечитан: строка мигает без причины');

  // Согласие снимает флажок у всех выделенных писем одной командой: команда на
  // письмо оставила бы половину выделения с флажком при обрыве связи.
  rowOf(ui, 51).querySelector('.row-flag').dispatch('click');
  await ui.clock.drain();
  ui.query('.overlay.open .mf .btn.primary').dispatch('click');
  await ui.clock.drain();
  const call = ui.callsOf('markFlagged');
  assert.equal(call.length, 1, 'снятие флажка ушло в ядро не одной командой на всё выделение');
  assert.deepEqual([[...call[0].args[0]].sort((a, b) => a - b), call[0].args[1], call[0].args[2]], [[51, 52], false, 'user'],
    'в ядро ушло не снятие флажка по обоим выделенным письмам');
  assert.equal(rowOf(ui, 51).querySelector('.row-flag').classes.has('on'), false, 'после согласия флажок остался в строке');

  // Установка флажка вопросов не задаёт ни при каких сроках: ничего не
  // удаляется.
  ui.evaluate('clearMessageSelection()');
  rowOf(ui, 53).querySelector('.row-flag').dispatch('click');
  await ui.clock.drain();
  assert.equal(ui.query('.overlay.open .confirm-cancel'), null, 'установка флажка спросила про удаление сроков');
  assert.deepEqual([...ui.callsOf('markFlagged').at(-1).args[0]], [53], 'установка флажка не дошла до ядра');
});

test('S-001, S-005: отказ ядра возвращает флажок строке и называет причину', async () => {
  // Значок строки меняется до ответа ядра, иначе нажатие выглядит
  // непроизошедшим. Без отката неудача ядра оставляет в списке флажок,
  // которого у письма нет: письмо числится помеченным до перезагрузки.
  const ui = await openList([
    listMessage({id: 61, subject: 'Счёт', flags: {seen: true, flagged: false}}),
  ], {answers: {markFlagged: () => { throw new Error('ядро недоступно'); }}});

  rowOf(ui, 61).querySelector('.row-flag').dispatch('click');
  await ui.clock.drain();
  assert.equal(ui.callsOf('markFlagged').length, 1, 'нажатие на флажок не дошло до ядра');
  assert.equal(rowOf(ui, 61).querySelector('.row-flag').classes.has('on'), false,
    'после отказа ядра флажок остался поднятым: письмо числится помеченным, а метки у него нет');
  assert.equal(ui.evaluate('messages.find(item=>item.id===61).flags.flagged'), false,
    'письмо в памяти окна осталось с флажком: следующее нажатие снимет несуществующий флажок');
  assert.ok(noticeTexts(ui).length, 'отказ ядра прошёл молча: пользователь считает флажок поставленным');
});
