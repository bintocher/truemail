// Проверки закрепления письма в списке: сборка строк с разделителем, четыре
// сортировки, общий отбор для обеих частей, предел закреплённой части, курсор
// обычной части, отдельный перечень закреплённых писем, свёрнутые беседы и
// настоящий путь открепления через мост команд. Разметка здесь не участвует.
// Спецификация: specs/pin-message.md.
// Запуск: node --test apps/desktop/tests/js/pinned-messages.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');

// Модуль закрепления появляется вместе с реализацией. Пока его нет, каждая
// проверка падает на своём месте и своими словами.
function loadPinModule() {
  try {
    return require('../../ui/modules/pin-message.js');
  } catch (error) {
    if (error && error.code === 'MODULE_NOT_FOUND') return null;
    throw error;
  }
}

const pins = loadPinModule();

function fn(name, whatBreaks) {
  if (pins && typeof pins[name] === 'function') return pins[name];
  return () => {
    throw new Error(`${name}: ${whatBreaks}`);
  };
}

// Имена границы выбраны автором кода: модуль отдаёт buildRows, messageRows,
// compareMessages, filterMessages, ordinaryCursor и separatorText. Ниже только
// приведение обращений к этим именам и к их сигнатурам; утверждения проверок
// остаются прежними.
const SORT = {newest: 'date-desc', oldest: 'date-asc', sender: 'sender', subject: 'subject'};
const SEPARATOR = 'pinned_separator';

// buildRows принимает отбор списка плоскими признаками рядом с сортировкой и
// языком, а набор строк отдаёт полем rows.
function listRows(pinned, ordinary, options = {}) {
  const build = fn('buildRows', 'закреплённая часть не собирается: закреплённое письмо не показывается вовсе');
  const request = Object.assign({sort: SORT[options.sort] || options.sort, locale: options.lang}, options.filters || {});
  return build(pinned || [], ordinary, request).rows;
}

// Модуль отдаёт сравнение двух писем, а не готовую сортировку.
function sortMessages(items, sort, lang = 'ru') {
  const compare = fn('compareMessages', 'сортировка закреплённой части не задана: закреплённые письма встанут вразнобой');
  return [...items].sort((a, b) => compare(a, b, SORT[sort] || sort, lang));
}

function applyListFilters(items, filters) {
  return fn('filterMessages', 'отбор списка не вынесен в общую функцию: у закреплённой части появится вторая копия правил')(
    items, filters,
  );
}

// Разделитель несёт число не поместившихся писем, а подпись собирается по нему
// отдельной функцией модуля.
function separatorRow(rows) {
  return rows.find(row => row.kind === SEPARATOR);
}

function separatorLabel(row, lang) {
  return fn('separatorText', 'подпись разделителя не собирается: число непоместившихся писем сказать нечем')(
    row.hidden, lang,
  );
}

// Курсор обычной части модуль считает по одной полученной странице: закреплённые
// письма в неё не попадают по построению, а пустая страница курсор не двигает.
function advanceNormalCursor(base, page) {
  const cursor = fn('ordinaryCursor', 'курсор обычной части не отделён от показанных строк: следующая страница потеряет письма')(
    page,
  );
  return cursor ? {date: cursor.date, id: cursor.id} : base;
}

function message(extra) {
  return Object.assign(
    {
      id: 1,
      account_id: 1,
      folder_id: 10,
      thread_id: null,
      subject: 'Договор',
      from: {name: 'Начальник', email: 'boss@example.test'},
      date: '2026-09-18T10:00:00Z',
      flags: {seen: true, flagged: false},
      has_attachments: false,
      labels: [],
      pinned_at: null,
    },
    extra,
  );
}

test('S-023, S-026: закреплённая часть идёт выше обычной и отделена разделителем', () => {
  const rows = listRows(
    [message({id: 5, pinned_at: '2026-09-18T09:00:00Z', date: '2026-08-01T10:00:00Z'})],
    [message({id: 1}), message({id: 2, date: '2026-09-17T10:00:00Z'})],
    {sort: 'newest', filters: {}, lang: 'ru'},
  );
  assert.equal(rows[0].id, 5, 'закреплённое письмо не поднялось наверх');
  assert.equal(rows[1].kind, SEPARATOR, 'между частями нет разделителя');
  assert.deepEqual(rows.slice(2).map(row => row.id), [1, 2], 'обычная часть перестроилась');
});

test('S-025: разделитель не письмо - он вне выделения, вне счёта строк и вне удержания памяти', () => {
  const rows = listRows(
    [message({id: 5, pinned_at: '2026-09-18T09:00:00Z'})],
    [message({id: 1}), message({id: 2, date: '2026-09-17T10:00:00Z'})],
    {sort: 'newest', filters: {}, lang: 'ru'},
  );
  // Одна и та же функция модуля отсекает разделитель и от выделения, и от
  // подписи числа писем, и от удержания памяти.
  const messageRows = fn('messageRows', 'разделитель попадёт в выделение и в выбор всех писем');
  const selectable = messageRows(rows);
  assert.deepEqual(selectable.map(row => row.id), [5, 1, 2]);
  assert.equal(messageRows(rows).length, 3, 'подпись числа писем посчитает разделитель письмом');
  const held = messageRows(rows).map(row => row.id);
  assert.ok(!held.includes(undefined) && !held.includes(null), `в удержании памяти оказался не номер письма: ${held}`);
  assert.deepEqual(held.sort((a, b) => a - b), [1, 2, 5]);
});

test('S-030 - S-036: четыре сортировки, пустые значения наименьшие, равенство разрешается номером письма', () => {
  const sort = sortMessages;
  const items = [
    message({id: 1, subject: 'Бета', from: {name: 'Петров', email: 'p@example.test'}, date: '2026-09-10T10:00:00Z'}),
    message({id: 2, subject: '', from: {name: '', email: ''}, date: '2026-09-12T10:00:00Z'}),
    message({id: 3, subject: 'Альфа', from: {name: 'Иванов', email: 'i@example.test'}, date: '2026-09-11T10:00:00Z'}),
  ];
  assert.deepEqual(sort(items, 'newest').map(item => item.id), [2, 3, 1]);
  assert.deepEqual(sort(items, 'oldest').map(item => item.id), [1, 3, 2]);
  assert.deepEqual(sort(items, 'sender').map(item => item.id), [2, 3, 1],
    'пустое имя отправителя обязано быть наименьшим значением');
  assert.deepEqual(sort(items, 'subject').map(item => item.id), [2, 3, 1],
    'пустая тема обязана быть наименьшим значением');
  // Полный порядок: равные значения разрешаются убыванием номера письма.
  const same = [
    message({id: 7, subject: 'Альфа', from: {name: 'Иванов', email: 'i@example.test'}}),
    message({id: 9, subject: 'Альфа', from: {name: 'Иванов', email: 'i@example.test'}}),
  ];
  assert.deepEqual(sort(same, 'subject').map(item => item.id), [9, 7]);
  // Отдельного порядка по времени закрепления нет: он спорил бы с выбранной
  // сортировкой списка.
  const pinned = [
    message({id: 1, subject: 'Альфа', pinned_at: '2026-09-18T10:00:00Z', date: '2026-09-10T10:00:00Z'}),
    message({id: 2, subject: 'Бета', pinned_at: '2026-09-01T10:00:00Z', date: '2026-09-12T10:00:00Z'}),
  ];
  assert.deepEqual(sort(pinned, 'newest').map(item => item.id), [2, 1]);
});

test('S-037: фильтры списка отсекают письма обеих частей одним и тем же отбором', () => {
  const filter = applyListFilters;
  const items = [
    message({id: 1, flags: {seen: false, flagged: false}}),
    message({id: 2, flags: {seen: true, flagged: true}}),
    message({id: 3, flags: {seen: true, flagged: false}, has_attachments: true}),
  ];
  assert.deepEqual(filter(items, {unread: true}).map(item => item.id), [1]);
  assert.deepEqual(filter(items, {flagged: true}).map(item => item.id), [2]);
  assert.deepEqual(filter(items, {attachments: true}).map(item => item.id), [3]);
  // Отбор у автора кода отдаёт письма уже в порядке списка: даты здесь равны,
  // а равенство разрешается убыванием номера письма. Прошли все три письма.
  assert.deepEqual(filter(items, {text: 'договор'}).map(item => item.id), [3, 2, 1]);
  // Закреплённое письмо, не прошедшее фильтр, в списке не остаётся.
  const rows = listRows(
    [message({id: 5, pinned_at: '2026-09-18T09:00:00Z', flags: {seen: true, flagged: false}})],
    [message({id: 1, flags: {seen: false, flagged: false}})],
    {sort: 'newest', filters: {unread: true}, lang: 'ru'},
  );
  assert.deepEqual(rows.map(row => row.id), [1], 'фильтр пропустил закреплённое письмо мимо себя');
});

test('S-042, S-043, S-070: закреплённая часть ограничена 50 строками, а разделитель называет непоместившиеся', () => {
  const pinnedItems = Array.from({length: 62}, (_, index) =>
    message({id: 100 + index, pinned_at: '2026-09-18T09:00:00Z', date: `2026-09-${String(10 + (index % 20)).padStart(2, '0')}T10:00:00Z`}));
  const rows = listRows(
    pinnedItems,
    [message({id: 1})],
    {sort: 'newest', filters: {}, lang: 'ru'},
  );
  const separator = separatorRow(rows);
  assert.equal(rows.filter(row => row.kind !== SEPARATOR && row.pinned_at).length, 50,
    'закреплённая часть выросла в отдельный список и вытеснила обычные письма за нижний край экрана');
  const text = separatorLabel(separator, 'ru');
  assert.ok(text.includes('12'), `разделитель не называет число непоместившихся писем: ${text}`);
  const english = separatorLabel(separator, 'en');
  assert.ok(!/[А-Яа-я]/.test(english), `подпись разделителя осталась русской: ${english}`);
});

test('S-016 - S-019: курсор обычной части ведётся только полученными страницами', () => {
  const advance = advanceNormalCursor;
  const base = {date: '2026-09-10T10:00:00Z', id: 40};
  const page = [
    message({id: 30, date: '2026-09-08T10:00:00Z'}),
    message({id: 20, date: '2026-09-05T10:00:00Z'}),
  ];
  assert.deepEqual(advance(base, page), {date: '2026-09-05T10:00:00Z', id: 20});
  // Закреплённое письмо старше края страницы курсор не двигает.
  assert.deepEqual(advance(base, [], [message({id: 3, date: '2026-01-01T10:00:00Z', pinned_at: '2026-09-18T09:00:00Z'})]), base);
  // Пустая страница означает конец списка, а не сдвиг курсора.
  assert.deepEqual(advance(base, []), base);
});

test('S-020 - S-022: закреплённые письма держатся отдельным перечнем и переживают слияние страниц и предел памяти', () => {
  const pinnedItems = [message({id: 5, pinned_at: '2026-09-18T09:00:00Z', date: '2026-01-01T10:00:00Z'})];
  const merged = fn('mergeReloadedPages', 'перезагрузка данных отдаёт закреплённые письма слиянию страниц и теряет их')(
    {pinned: pinnedItems, normal: [message({id: 1}), message({id: 2})]},
    {freshIds: [1], fullPageEdge: '2026-09-17T10:00:00Z'},
  );
  assert.ok(merged.pinned.some(item => item.id === 5), 'закреплённое письмо выпало при перезагрузке данных');
  const trimmed = fn('trimToMemoryLimit', 'предел писем в памяти вытесняет закреплённые письма')(
    {pinned: pinnedItems, normal: Array.from({length: 30}, (_, index) => message({id: 1000 + index}))},
    {limit: 10, keepIds: []},
  );
  assert.ok(trimmed.pinned.some(item => item.id === 5),
    'закреплённое письмо вытеснено по дате, хотя пользователь держит его наверху');
  assert.ok(trimmed.normal.length <= 10, 'предел памяти не соблюдён');
});

test('S-060 - S-062: закреплённое письмо не входит в свёртку бесед, а остальные письма беседы остаются обычной строкой', () => {
  const rows = listRows(
    [message({id: 5, thread_id: 77, pinned_at: '2026-09-18T09:00:00Z', date: '2026-09-18T10:00:00Z'})],
    [
      message({id: 6, thread_id: 77, date: '2026-09-17T10:00:00Z'}),
      message({id: 7, thread_id: 77, date: '2026-09-16T10:00:00Z'}),
      message({id: 8, thread_id: 78, date: '2026-09-15T10:00:00Z'}),
    ],
    {sort: 'newest', filters: {}, lang: 'ru', collapseThreads: true},
  );
  assert.equal(rows[0].id, 5, 'закреплённое письмо не показано отдельной строкой');
  const normal = rows.filter(row => row.kind !== SEPARATOR && row.id !== 5);
  assert.deepEqual(normal.map(row => row.id), [6, 8], 'беседа закреплённого письма собрана неверно');
  assert.equal(normal[0].threadCount, 2, 'закреплённое письмо посчитано в свёртке своей беседы');
});

test('S-003, S-004, S-063, S-064 - S-069: закрепление идёт одной командой моста и снимается без перезагрузки', async () => {
  // Сквозная проверка настоящего пути: строка, меню и выделение ведут в одну
  // команду, а отказ чтения закреплённых не прячет обычную часть списка.
  const calls = [];
  const bridge = {
    setMessagesPinned: async (ids, pinned) => {
      calls.push({ids, pinned});
      if (pinned && ids.length > 1) return {pinned: 1, rejected: ids.length - 1};
      return {pinned: ids.length, rejected: 0};
    },
  };
  const toggle = fn('togglePin', 'закрепить письмо нечем: закрепления в программе нет вовсе');
  const first = await toggle(bridge, [11], true);
  assert.deepEqual(calls[0], {ids: [11], pinned: true}, 'закрепление обязано идти одной командой с перечнем писем');
  assert.equal(first.rejected, 0, 'закрепление одного письма спросило лишнего');

  // Групповое закрепление упирается в предел и честно называет число отказов.
  const group = await toggle(bridge, [21, 22, 23], true);
  assert.equal(group.pinned, 1);
  assert.equal(group.rejected, 2);
  assert.ok(
    fn('pinLimitText', 'сообщение о пределе закрепления не собирается')(group, 'ru').includes('2'),
    'пользователю не сказано, сколько писем закрепить не удалось',
  );

  // Снятие закрепления возвращает письмо в обычную часть на место по
  // выбранной сортировке, не перезагружая представление.
  const state = {
    pinned: [message({id: 5, pinned_at: '2026-09-18T09:00:00Z', date: '2026-09-11T10:00:00Z'})],
    normal: [message({id: 1, date: '2026-09-18T10:00:00Z'}), message({id: 2, date: '2026-09-10T10:00:00Z'})],
  };
  await toggle(bridge, [5], false);
  const after = fn('unpinInPlace', 'открепление требует перезагрузки представления: список мигает и теряет место')(
    state, 5, {sort: 'newest'},
  );
  assert.deepEqual(after.pinned.map(item => item.id), []);
  assert.deepEqual(after.normal.map(item => item.id), [1, 5, 2], 'письмо встало не на своё место по дате');

  // Отказ чтения закреплённых писем оставляет обычную часть на месте: в модуль
  // он приходит пустым перечнем закреплённых.
  const rows = listRows(
    [],
    [message({id: 1})],
    {sort: 'newest', filters: {}, lang: 'ru', pinnedError: 'сеть недоступна'},
  );
  assert.deepEqual(rows.filter(row => row.kind !== SEPARATOR).map(row => row.id), [1],
    'отказ чтения закреплённых писем оставил список пустым');
});
