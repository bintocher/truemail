// Проверки подписи отправителя и получателя: модель выбора стороны строки
// (ui/modules/mail-addresses.js) и настоящий путь отрисовки - строка списка
// писем и строки "Кому"/"Копия" шапки письма, собранные кодом mail.js на
// разметке index.html.
// Суффикс "+N", текст "Без получателя", кружок с инициалом и подпись ящика
// рисует mail.js, а не модель: проверка, читающая одну модель, не замечает,
// что строка списка перестала показывать половину из этого.
// Спецификации: specs/sent-recipient-display.md, specs/message-mailbox-owner.md.
// Запуск: node --test apps/desktop/tests/js/mail-addresses.test.js (Node 22+).
'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {rowPresentation, addressLineModel, displayName, mailboxLabel, listMailboxLabel} = require('../../ui/modules/mail-addresses.js');
const {startApp} = require('./ui-app.js');

const rolesOf = map => new Map(Object.entries(map).map(([id, role]) => [Number(id), role]));
const addr = (name, email) => ({name, email});
const many = n => Array.from({length: n}, (_, index) => addr(`Имя${index}`, `a${index}@example.com`));
const accountsOf = (...emails) => emails.map((email, index) => ({id: index + 1, email}));

// --- Модель: сторона строки, подпись, свёртка ---

// S-001, S-006: сторона строки определяется ролью папки самого письма.
// flags.draft не участвует, у каждого письма беседы своя роль.
test('S-001, S-006: сторона строки - роль папки письма, и только она', () => {
  const message = {from: addr('Аня', 'anna@example.com'), to: [addr('Боря', 'boris@example.com')], cc: []};
  const sides = {sent: 'recipient', drafts: 'recipient', inbox: 'sender', archive: 'sender', trash: 'sender', spam: 'sender'};
  Object.entries(sides).forEach(([role, kind]) => {
    assert.equal(rowPresentation({...message, folder_id: 1}, rolesOf({1: role})).kind, kind, role);
  });
  // Папка письма не найдена и роль неизвестна - сторона отправителя.
  assert.equal(rowPresentation({...message, folder_id: 99}, rolesOf({1: 'sent'})).kind, 'sender');
  assert.equal(rowPresentation({...message, folder_id: 1}, new Map()).kind, 'sender');
  // Признак черновика у самого письма роль папки не подменяет.
  const draftInInbox = rowPresentation({...message, folder_id: 1, flags: {draft: true}}, rolesOf({1: 'inbox'}));
  assert.equal(draftInInbox.kind, 'sender');
  assert.equal(draftInInbox.text, 'Аня');
  // Письмо самому себе: в Входящих виден отправитель, в Отправленных - получатель.
  const same = addr('Аня', 'anna@example.com');
  const roles = rolesOf({1: 'inbox', 2: 'sent'});
  assert.equal(rowPresentation({folder_id: 1, from: same, to: [same], cc: []}, roles).kind, 'sender');
  assert.equal(rowPresentation({folder_id: 2, from: same, to: [same], cc: []}, roles).kind, 'recipient');
});

// S-002: пустой to и непустой cc - подпись по cc: письмо, отправленное только
// в копию, не должно выглядеть письмом без получателя.
test('S-002: пустой to заменяется списком cc, повторы не схлопываются', () => {
  const byCc = rowPresentation({folder_id: 1, from: addr('', ''), to: [], cc: [addr('Копия', 'cc@example.com')]}, rolesOf({1: 'sent'}));
  assert.equal(byCc.kind, 'recipient');
  assert.equal(byCc.text, 'Копия');
  assert.equal(byCc.extra, 0);
  // Два одинаковых адреса - это два получателя: удаление повторов показало бы
  // одного там, где письмо ушло двоим.
  const twice = rowPresentation({folder_id: 1, from: addr('', ''), to: [addr('Аня', 'a@example.com'), addr('Аня', 'a@example.com')], cc: []}, rolesOf({1: 'sent'}));
  assert.equal(twice.extra, 1);
});

// S-003: пробельные имя и адрес отображаемым получателем не считаются - без
// trim строка получила бы подпись из пробелов вместо текста "Без получателя".
test('S-003: адреса из одних пробелов дают заглушку без получателя', () => {
  const blank = rowPresentation({folder_id: 1, from: addr('', ''), to: [addr('  ', '')], cc: [addr('', ' ')]}, rolesOf({1: 'drafts'}));
  assert.deepEqual(blank, {kind: 'empty', text: '', extra: 0, initial: '?'});
  // Пустой элемент внутри списка не занимает место первого и не идёт в счёт.
  const withBlankFirst = rowPresentation({folder_id: 1, from: addr('', ''), to: [addr('', ''), addr('Аня', 'a@example.com'), addr('Боря', 'b@example.com')], cc: []}, rolesOf({1: 'sent'}));
  assert.equal(withBlankFirst.text, 'Аня');
  assert.equal(withBlankFirst.extra, 1);
});

// S-004, S-008: подпись адреса - имя после trim, иначе адрес; полный вид для
// строки шапки и подсказки - "Имя (email)".
test('S-004, S-008: подпись адреса и полный вид "Имя (email)"', () => {
  const cases = [
    [addr('Аня', 'a@example.com'), 'Аня'],
    [addr('', 'a@example.com'), 'a@example.com'],
    [addr('   ', 'a@example.com'), 'a@example.com'],
    [addr('', ''), ''],
    [undefined, ''],
  ];
  cases.forEach(([value, expected]) => assert.equal(displayName(value), expected, JSON.stringify(value)));
  // В строке шапки показывается тот же полный вид, что и в подсказке: иначе
  // однофамильцев в свёрнутой строке не различить.
  const model = addressLineModel([addr('', 'a@example.com'), addr('Аня', 'a@example.com'), addr('Аня', 'b@example.com')], true, 2);
  assert.deepEqual(model.shown.map(item => item.text), ['a@example.com', 'Аня (a@example.com)', 'Аня (b@example.com)']);
  assert.deepEqual(model.shown.map(item => item.title), ['a@example.com', 'Аня (a@example.com)', 'Аня (b@example.com)']);
  assert.equal(addressLineModel([addr('Аня', '')], false, 2).shown[0].text, 'Аня');
});

// S-005: инициал берётся из той же подписи, что и текст строки.
test('S-005: инициал - первая кодовая точка подписи, у пустой подписи "?"', () => {
  assert.equal(rowPresentation({folder_id: 1, from: addr('😀 Аня', 'a@example.com')}, rolesOf({1: 'inbox'})).initial, '😀');
  assert.equal(rowPresentation({folder_id: 1, from: addr('', '')}, rolesOf({1: 'inbox'})).initial, '?');
  assert.equal(rowPresentation({folder_id: 1, from: addr('аня', 'a@example.com')}, rolesOf({1: 'inbox'})).initial, 'А');
});

// S-009: сколько адресов показано и сколько скрыто при свёртке.
test('S-009: свёртка адресной строки по числу адресов и границе maxShown', () => {
  [[1, 1, 0], [2, 2, 0], [3, 2, 1], [7, 2, 5]].forEach(([count, shown, hidden]) => {
    const model = addressLineModel(many(count), false, 2);
    assert.equal(model.shown.length, shown, `${count} адресов - показано`);
    assert.equal(model.hidden, hidden, `${count} адресов - скрыто`);
  });
  // Граница свёртки - это параметр, а не жёсткая двойка внутри модели.
  const wider = addressLineModel(many(5), false, 3);
  assert.equal(wider.shown.length, 3);
  assert.equal(wider.hidden, 2);
  // Раскрытое состояние показывает все адреса и счётчика не даёт.
  const expanded = addressLineModel(many(7), true, 2);
  assert.equal(expanded.shown.length, 7);
  assert.equal(expanded.hidden, 0);
});

test('S-009: показывать нечего - строки нет вовсе', () => {
  assert.equal(addressLineModel([], false, 2), null);
  assert.equal(addressLineModel(undefined, false, 2), null);
  assert.equal(addressLineModel([addr('', ''), addr('  ', '')], false, 2), null);
});

// --- Подпись ящика письма (message-mailbox-owner.md) ---

test('S-001, S-002, S-006: подпись ящика есть только при нескольких ящиках', () => {
  const accounts = accountsOf('one@example.com', 'two@example.com');
  assert.equal(mailboxLabel(1, accounts, 'ящик удалён'), 'one@example.com');
  assert.equal(mailboxLabel(2, accounts, 'ящик удалён'), 'two@example.com');
  // Один ящик - путать нечего, подписи нет.
  assert.equal(mailboxLabel(1, accountsOf('one@example.com'), 'ящик удалён'), null);
  assert.equal(mailboxLabel(1, [], 'ящик удалён'), null);
  assert.equal(mailboxLabel(1, null, 'ящик удалён'), null);
  // Аккаунта письма больше нет либо адрес пуст - подпись удалённого ящика.
  assert.equal(mailboxLabel(99, accounts, 'ящик удалён'), 'ящик удалён');
  assert.equal(mailboxLabel(1, [{id: 1, email: '   '}, {id: 2, email: 'two@example.com'}], 'ящик удалён'), 'ящик удалён');
});

test('S-004, S-005: в папке одного ящика подпись ящика в списке не нужна', () => {
  const accounts = accountsOf('one@example.com', 'two@example.com');
  assert.equal(listMailboxLabel(1, accounts, true, 'ящик удалён'), 'one@example.com');
  assert.equal(listMailboxLabel(1, accounts, false, 'ящик удалён'), null);
  assert.equal(listMailboxLabel(1, accountsOf('one@example.com'), true, 'ящик удалён'), null);
});

// --- Настоящий путь: строка списка писем ---

const FOLDERS = [
  {id: 1, role: 'sent', display_name: 'Отправленные', account_id: 1},
  {id: 2, role: 'inbox', display_name: 'Входящие', account_id: 1},
];
const ACCOUNTS = [{id: 1, email: 'one@example.test'}, {id: 2, email: 'two@example.test'}];

function listApp(options = {}) {
  return startApp({accounts: options.accounts || ACCOUNTS, folders: FOLDERS, currentFolderId: options.currentFolderId ?? null});
}

const message = over => ({
  id: 10, account_id: 1, folder_id: 1, subject: 'Тема', preview: 'Превью',
  from: addr('Я', 'me@example.test'), to: [], cc: [], flags: {seen: true}, labels: [], ...over,
});

// Строка списка собирается кодом mail.js: подпись, счётчик остальных
// получателей, кружок с инициалом и подпись ящика - разные узлы, и пропажа
// любого из них модели не видна.
test('S-002, S-005: строка списка показывает первого получателя, счётчик остальных и инициал', () => {
  const app = listApp();
  [[1, null, 'Имя0', 'И'], [2, '+1', 'Имя0', 'И'], [3, '+2', 'Имя0', 'И'], [5, '+4', 'Имя0', 'И']].forEach(([count, extra, text, initial]) => {
    const row = app.sandbox.createMessageRow(message({to: many(count)}), 0);
    assert.equal(row.querySelector('.from').textContent, text, `${count} получателей - подпись`);
    assert.equal(row.querySelector('.from-extra')?.textContent ?? null, extra, `${count} получателей - счётчик`);
    assert.equal(row.querySelector('.ava').textContent, initial, `${count} получателей - инициал`);
  });
});

test('S-003: письмо без получателей показывает "Без получателя", а не отправителя', () => {
  const app = listApp();
  // from заполнен: откат к отправителю снова показывал бы в Отправленных себя.
  const row = app.sandbox.createMessageRow(message({from: addr('Аня', 'anna@example.test'), to: [], cc: []}), 0);
  assert.equal(row.querySelector('.from').textContent, 'Без получателя');
  assert.equal(row.querySelector('.ava').textContent, '?');
  assert.equal(row.querySelector('.from-extra'), null, 'счётчика остальных у заглушки нет');
});

test('S-004: строка входящего письма показывает отправителя', () => {
  const app = listApp();
  const row = app.sandbox.createMessageRow(message({folder_id: 2, from: addr('Аня', 'anna@example.test'), to: [addr('Я', 'me@example.test')]}), 0);
  assert.equal(row.querySelector('.from').textContent, 'Аня');
  assert.equal(row.querySelector('.ava').textContent, 'А');
});

test('S-004, S-005: подпись ящика в строке есть только в объединённом списке', () => {
  const unified = listApp().sandbox.createMessageRow(message({to: [addr('Аня', 'a@example.test')]}), 0);
  const box = unified.querySelector('.mbox');
  assert.ok(box, 'в объединённом списке у строки есть подпись ящика');
  assert.equal(box.textContent, 'one@example.test');
  assert.equal(box.title, 'one@example.test', 'подпись целиком доступна подсказкой');
  // В папке одного ящика все письма одного ящика, и подпись была бы шумом.
  const inFolder = listApp({currentFolderId: 1}).sandbox.createMessageRow(message({to: [addr('Аня', 'a@example.test')]}), 0);
  assert.equal(inFolder.querySelector('.mbox'), null);
  // Один подключённый ящик - подписи нет и в объединённом списке.
  const single = listApp({accounts: [{id: 1, email: 'one@example.test'}]}).sandbox.createMessageRow(message({to: [addr('Аня', 'a@example.test')]}), 0);
  assert.equal(single.querySelector('.mbox'), null);
});

// --- Настоящий путь: строки "Кому" и "Копия" шапки письма ---

// Раскрытие живёт в замыкании render(expanded) внутри mail.js: модель в обоих
// состояниях одна и та же, и проверка одной модели не видит, что кнопка "+N"
// перестала раскрывать строку.
test('S-008, S-009: кнопка "+N" раскрывает строку шапки, "Свернуть" возвращает её', () => {
  const app = listApp();
  const line = app.sandbox.buildAddressLine('mail-toline', 'Кому: ', many(5));
  assert.equal(line.querySelector('.mail-address-label').textContent, 'Кому: ');
  assert.deepEqual(
    line.querySelectorAll('.mail-address-item').map(item => item.textContent),
    ['Имя0 (a0@example.com)', 'Имя1 (a1@example.com)'],
  );
  const more = line.querySelector('.mail-address-toggle');
  assert.equal(more.textContent, '+3');
  more.onclick();
  assert.equal(line.querySelectorAll('.mail-address-item').length, 5, 'раскрытая строка показывает всех');
  assert.ok(line.classes.has('expanded'));
  const less = line.querySelector('.mail-address-toggle');
  assert.equal(less.textContent, 'Свернуть');
  less.onclick();
  assert.equal(line.querySelectorAll('.mail-address-item').length, 2, 'свёрнутая строка снова показывает двоих');
  assert.equal(line.querySelector('.mail-address-toggle').textContent, '+3');
});

test('S-008: у каждого адреса строки шапки есть подсказка с полным видом', () => {
  const app = listApp();
  const line = app.sandbox.buildAddressLine('mail-ccline', 'Копия: ', [addr('Аня', 'a@example.com'), addr('', 'b@example.com')]);
  const items = line.querySelectorAll('.mail-address-item');
  assert.deepEqual(items.map(item => item.title), ['Аня (a@example.com)', 'b@example.com']);
  // Двух адресов хватает, чтобы показать их без свёртки: кнопки нет.
  assert.equal(line.querySelector('.mail-address-toggle'), null);
});

test('S-008: показывать нечего - строки шапки нет вовсе, отступ не остаётся', () => {
  const app = listApp();
  assert.equal(app.sandbox.buildAddressLine('mail-ccline', 'Копия: ', []), null);
  assert.equal(app.sandbox.buildAddressLine('mail-ccline', 'Копия: ', [addr('  ', ' ')]), null);
});
