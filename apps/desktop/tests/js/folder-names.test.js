// Проверки читаемых подписей папок: построение пути и порядок списка
// (ui/modules/folder-names.js) и настоящий путь раздела "Источники писем" -
// строка списка, снятый флажок и возврат состояния при отказе ядра
// (settings.js).
// Декодер сегментов наружу не вызывается ни разу: он виден только через
// подпись пути, и проверять его отдельно от неё нечего.
// Спецификация: specs/folder-names-readable.md.
// Запуск: node --test apps/desktop/tests/js/folder-names.test.js (Node 22+).
'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {folderPathLabel, compareFolderLabels} = require('../../ui/modules/folder-names.js');
const {startApp} = require('./ui-window.js');

const byId = list => new Map(list.map(folder => [folder.id, folder]));
const folder = over => ({id: 1, remote_path: '', display_name: '', parent_id: null, role: null, account_id: 1, ...over});
// Имя "Отправленные" в кодировке modified UTF-7 (RFC 3501) - в таком виде его
// присылает сервер IMAP.
const SENT_UTF7 = '&BB4EQgQ,BEAEMAQyBDsENQQ9BD0ESwQ1-';

// S-001, S-004: сегмент пути IMAP показывается человеку раскодированным. Те же
// примеры, что и у декодера ядра (crates/core/src/backend/imap.rs).
test('S-001, S-004: сегменты пути IMAP раскодируются, повреждённые показываются как есть', () => {
  const cases = [
    ['INBOX/Work', 'INBOX/Work', 'имя без кодирования'],
    [`INBOX/${SENT_UTF7}`, 'INBOX/Отправленные', 'кириллическое имя'],
    ['INBOX/A&-B', 'INBOX/A&B', 'сам знак & экранирован как "&-"'],
    ['INBOX/&BB4-broken&', 'INBOX/&BB4-broken&', 'повреждённая последовательность'],
    ['INBOX/&!!!-', 'INBOX/&!!!-', 'непригодные данные вместо base64'],
  ];
  cases.forEach(([remotePath, expected, reason]) => {
    const item = folder({remote_path: remotePath, display_name: 'Работа'});
    assert.equal(folderPathLabel(item, byId([item])), expected, reason);
  });
});

// S-001: у папки верхнего уровня подпись - её собственное имя, каким бы ни был
// технический remote_path: у IMAP это путь, у EWS и JMAP - непрозрачный
// идентификатор, и показывать его человеку нельзя.
test('S-001: папка верхнего уровня подписана своим именем при любом виде remote_path', () => {
  const cases = [
    [SENT_UTF7, 'Отправленные'],
    ['AAMkADRkM2U1ZDAw', 'Архив проектов'],
    ['mailbox-id-42', 'Спам'],
  ];
  cases.forEach(([remotePath, name]) => {
    const item = folder({remote_path: remotePath, display_name: name});
    assert.equal(folderPathLabel(item, byId([item])), name, remotePath);
  });
});

// S-002: вложенные папки различимы - у Exchange по цепочке parent_id, у IMAP
// по разделителям пути. Две одноимённые папки в разных ветках должны иметь
// разные подписи, иначе в списке источников их не различить.
test('S-002: путь строится от корня и различает одноимённые папки разных веток', () => {
  const root = folder({id: 1, display_name: 'Архив'});
  const personal = folder({id: 2, display_name: 'Личное'});
  const child = folder({id: 3, display_name: 'Проекты', parent_id: 1});
  const twin = folder({id: 4, display_name: 'Проекты', parent_id: 2});
  const deep = folder({id: 5, display_name: '2026', parent_id: 3});
  const map = byId([root, personal, child, twin, deep]);
  assert.equal(folderPathLabel(deep, map), 'Архив/Проекты/2026');
  assert.equal(folderPathLabel(child, map), 'Архив/Проекты');
  assert.equal(folderPathLabel(twin, map), 'Личное/Проекты');
  assert.notEqual(folderPathLabel(child, map), folderPathLabel(twin, map));
  // Путь IMAP из трёх сегментов, разделитель '|', средний сегмент закодирован.
  const imap = folder({id: 6, remote_path: `INBOX|${SENT_UTF7}|2026`, display_name: '2026'});
  assert.equal(folderPathLabel(imap, byId([imap])), 'INBOX/Отправленные/2026');
});

test('S-002: разорванная и закольцованная цепочка родителей даёт имя самой папки', () => {
  const orphan = folder({id: 2, display_name: 'Проекты', parent_id: 999});
  assert.equal(folderPathLabel(orphan, byId([orphan])), 'Проекты');
  const first = folder({id: 1, display_name: 'A', parent_id: 2});
  const second = folder({id: 2, display_name: 'B', parent_id: 1});
  const looped = byId([first, second]);
  assert.equal(folderPathLabel(first, looped), 'A');
  assert.equal(folderPathLabel(second, looped), 'B');
});

// S-004: пустые и битые значения не должны ломать строку списка.
test('S-004: при пустом имени показывается remote_path, при пустом всём - пустая строка', () => {
  const cases = [
    [folder({remote_path: 'raw-id-1', display_name: ''}), 'raw-id-1', 'имени нет - виден технический путь'],
    [folder({remote_path: '', display_name: ''}), '', 'пусты оба поля'],
    [folder({remote_path: '   ', display_name: '  '}), '', 'в обоих полях пробелы'],
    [null, '', 'папки нет вовсе'],
  ];
  cases.forEach(([item, expected, reason]) => assert.equal(folderPathLabel(item, new Map()), expected, reason));
});

// S-003: порядок списка источников - сначала роли в рабочем порядке, потом
// папки без роли по алфавиту. Проверяются все шесть ролей: перестановка
// соседних (отправленные с черновиками, архив со спамом) иначе проходит мимо.
// Подписи нарочно начинаются с разных букв одной азбуки - тогда ожидаемый
// порядок один и тот же при любой сборке Node, с полной таблицей сравнения и
// без неё.
test('S-003: порядок ролей идёт впереди алфавита, внутри роли - по подписи', () => {
  const items = [
    {folder: folder({id: 1, role: null}), label: 'Вторая'},
    {folder: folder({id: 2, role: 'trash'}), label: 'Корзина'},
    {folder: folder({id: 3, role: 'drafts'}), label: 'Черновики'},
    {folder: folder({id: 4, role: null}), label: 'Альфа'},
    {folder: folder({id: 5, role: 'spam'}), label: 'Спам'},
    {folder: folder({id: 6, role: 'inbox'}), label: 'Входящие'},
    {folder: folder({id: 7, role: 'archive'}), label: 'Архив'},
    {folder: folder({id: 8, role: 'sent'}), label: 'Отправленные'},
    {folder: folder({id: 9, role: null}), label: 'Берег'},
  ];
  assert.deepEqual(
    [...items].sort(compareFolderLabels).map(item => item.label),
    ['Входящие', 'Отправленные', 'Черновики', 'Архив', 'Спам', 'Корзина', 'Альфа', 'Берег', 'Вторая'],
  );
});

// --- Настоящий путь: список источников писем ---

const ACCOUNT = {id: 1, email: 'me@example.test'};
const FOLDERS = [
  {id: 1, account_id: 1, role: 'trash', display_name: 'Корзина', remote_path: 'Trash', parent_id: null},
  {id: 2, account_id: 1, role: null, display_name: 'Работа', remote_path: `INBOX|${SENT_UTF7}|Работа`, parent_id: null},
  {id: 3, account_id: 1, role: 'inbox', display_name: 'Входящие', remote_path: 'INBOX', parent_id: null},
];

async function sourcesApp(bridge = {}) {
  const app = startApp({accounts: [ACCOUNT], folders: FOLDERS, bridge});
  await app.ready();
  await app.setLanguage('ru');
  app.sandbox.__folders = FOLDERS;
  app.run('renderAccountSettings(coreAccounts, [__folders], [])');
  return app;
}

const sourceRows = app => app.document.querySelectorAll('.unified-source');
const sourceRow = (app, folderId) => sourceRows(app).find(row => row.querySelector('input').dataset.sourceFolder === String(folderId));

test('S-001, S-002, S-003: строка источника показывает читаемый путь и роль, список идёт в порядке ролей', async () => {
  const app = await sourcesApp();
  assert.deepEqual(
    sourceRows(app).map(row => row.querySelector('.source-path').textContent),
    ['Входящие', 'Корзина', 'INBOX/Отправленные/Работа'],
    'технический путь IMAP показан человеку раскодированным, порядок - по ролям',
  );
  assert.deepEqual(
    sourceRows(app).map(row => row.querySelector('.source-role').textContent),
    ['Входящие', 'Корзина', 'Без роли'],
  );
});

test('снятый флажок источника доходит до ядра и остаётся снятым', async () => {
  const app = await sourcesApp({setUnifiedSource: () => null});
  const row = sourceRow(app, 2);
  const box = row.querySelector('input');
  assert.equal(box.checked, true, 'источник включён по умолчанию');
  box.checked = false;
  box.dispatch('change', {target: box});
  await app.settle(10);
  assert.deepEqual(
    app.calls.filter(call => call.command === 'setUnifiedSource').map(call => call.args),
    [[2, false]],
    'снятие ушло в ядро одной командой с номером папки',
  );
  assert.equal(box.checked, false);
  assert.equal(app.document.querySelector('.source-count').textContent, '2 из 3 включено');
});

test('ядро отказало - флажок возвращается в прежнее состояние и об отказе сказано', async () => {
  const app = await sourcesApp({
    setUnifiedSource: () => {
      throw {kind: 'storage_error', message: 'база занята'};
    },
  });
  const box = sourceRow(app, 2).querySelector('input');
  box.checked = false;
  box.dispatch('change', {target: box});
  await app.settle(10);
  assert.equal(box.checked, true, 'состояние вернулось: источник на самом деле не выключен');
  const toasts = app.document.querySelectorAll('#activityPanel .activity-entry-text').map(node => node.textContent);
  assert.equal(toasts.length, 1);
  assert.match(toasts[0], /хранилищ/i, 'отказ объяснён человеку');
});

test('кнопка "Только стандартные" оставляет источниками папки с ролями', async () => {
  const app = await sourcesApp({setUnifiedSource: () => null});
  app.document.querySelector('[data-source-mode="standard"]').onclick();
  await app.settle(10);
  const sent = app.calls.filter(call => call.command === 'setUnifiedSource').map(call => call.args);
  assert.deepEqual(
    sent.sort((left, right) => left[0] - right[0]),
    [[1, true], [2, false], [3, true]],
    'папка без роли выключена, папки с ролями оставлены',
  );
  assert.equal(sourceRow(app, 2).querySelector('input').checked, false);
  assert.equal(app.document.querySelector('.source-count').textContent, '2 из 3 включено');
});
