// Проверки чистой логики apps/desktop/ui/modules/folder-names.js.
// Запуск: node --test apps/desktop/tests/js/folder-names.test.js (Node 20+).
'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {folderPathLabel, compareFolderLabels, decodeFolderSegment} = require('../../ui/modules/folder-names.js');

const byId = list => new Map(list.map(f => [f.id, f]));
const folder = (over) => ({id: 1, remote_path: '', display_name: '', parent_id: null, role: null, account_id: 1, ...over});

// decodeFolderSegment: те же примеры, что и в Rust-тестах imap.rs:2032-2145.
test('decodeFolderSegment: INBOX без escape-последовательностей', () => {
  assert.equal(decodeFolderSegment('INBOX'), 'INBOX');
});
test('decodeFolderSegment: кириллическое имя декодируется', () => {
  assert.equal(decodeFolderSegment('&BB4EQgQ,BEAEMAQyBDsENQQ9BD0ESwQ1-'), 'Отправленные');
});
test('decodeFolderSegment: экранирование "&" через "&-"', () => {
  assert.equal(decodeFolderSegment('A&-B'), 'A&B');
});
test('decodeFolderSegment: повреждённая последовательность возвращается как есть', () => {
  assert.equal(decodeFolderSegment('&BB4-broken&'), '&BB4-broken&');
  assert.equal(decodeFolderSegment('&!!!-'), '&!!!-');
});
test('decodeFolderSegment: пустая и нестроковая величина', () => {
  assert.equal(decodeFolderSegment(''), '');
  assert.equal(decodeFolderSegment(undefined), '');
});

// S-001: подпись строки списка источников для IMAP, EWS, JMAP.
test('S-001: IMAP - подпись из декодированного remote_path без родителя', () => {
  const f = folder({id: 1, remote_path: '&BB4EQgQ,BEAEMAQyBDsENQQ9BD0ESwQ1-', display_name: 'Отправленные'});
  assert.equal(folderPathLabel(f, byId([f])), 'Отправленные');
});
test('S-001: EWS - подпись строится по display_name верхнего уровня', () => {
  const f = folder({id: 2, remote_path: 'AAMkADRkM2U1ZDAw', display_name: 'Архив проектов', parent_id: null});
  assert.equal(folderPathLabel(f, byId([f])), 'Архив проектов');
});
test('S-001: JMAP - подпись из display_name, remote_path - непрозрачный id', () => {
  const f = folder({id: 3, remote_path: 'mailbox-id-42', display_name: 'Спам'});
  assert.equal(folderPathLabel(f, byId([f])), 'Спам');
});

// S-002: вложенные папки различимы - по parent_id (EWS) и по разделителям remote_path (IMAP).
test('S-002: EWS - путь от корня по цепочке parent_id', () => {
  const root = folder({id: 1, display_name: 'Архив', parent_id: null});
  const child = folder({id: 2, display_name: 'Проекты', parent_id: 1});
  const grandchild = folder({id: 3, display_name: '2026', parent_id: 2});
  const map = byId([root, child, grandchild]);
  assert.equal(folderPathLabel(grandchild, map), 'Архив/Проекты/2026');
});
test('S-002: IMAP - путь из двух сегментов remote_path', () => {
  const f = folder({id: 1, remote_path: 'INBOX/Work', display_name: 'Work'});
  assert.equal(folderPathLabel(f, byId([f])), 'INBOX/Work');
});
test('S-002: IMAP - путь из трёх сегментов с закодированным кириллическим сегментом', () => {
  const f = folder({id: 1, remote_path: 'INBOX|&BB4EQgQ,BEAEMAQyBDsENQQ9BD0ESwQ1-|2026', display_name: '2026'});
  assert.equal(folderPathLabel(f, byId([f])), 'INBOX/Отправленные/2026');
});
test('S-002: отсутствующий родитель в списке - берётся только имя папки', () => {
  const f = folder({id: 2, display_name: 'Проекты', parent_id: 999});
  assert.equal(folderPathLabel(f, byId([f])), 'Проекты');
});
test('S-002: циклическая ссылка parent_id - берётся только имя папки', () => {
  const a = folder({id: 1, display_name: 'A', parent_id: 2});
  const b = folder({id: 2, display_name: 'B', parent_id: 1});
  const map = byId([a, b]);
  assert.equal(folderPathLabel(a, map), 'A');
  assert.equal(folderPathLabel(b, map), 'B');
});
test('S-002: две одноимённые папки в разных ветках дают разные подписи', () => {
  const rootA = folder({id: 1, display_name: 'Архив', parent_id: null});
  const rootB = folder({id: 2, display_name: 'Личное', parent_id: null});
  const childA = folder({id: 3, display_name: 'Проекты', parent_id: 1});
  const childB = folder({id: 4, display_name: 'Проекты', parent_id: 2});
  const map = byId([rootA, rootB, childA, childB]);
  assert.notEqual(folderPathLabel(childA, map), folderPathLabel(childB, map));
  assert.equal(folderPathLabel(childA, map), 'Архив/Проекты');
  assert.equal(folderPathLabel(childB, map), 'Личное/Проекты');
});

// S-003: сортировка по показанной подписи, поверх текущего порядка ролей.
test('S-003: порядок ролей приоритетнее алфавита подписи', () => {
  const inbox = {folder: folder({id: 1, role: 'inbox'}), label: 'Яндекс'};
  const archive = {folder: folder({id: 2, role: 'archive'}), label: 'Алфавит'};
  assert.ok(compareFolderLabels(inbox, archive) < 0);
});
test('S-003: внутри одной роли сортировка идёт по алфавиту подписи', () => {
  const items = [
    {folder: folder({id: 1, role: null}), label: 'Ящик Б'},
    {folder: folder({id: 2, role: null}), label: 'Ящик А'},
  ];
  const sorted = [...items].sort(compareFolderLabels);
  assert.deepEqual(sorted.map(i => i.label), ['Ящик А', 'Ящик Б']);
});
test('S-003: набор папок IMAP и EWS сортируется по видимому имени с учётом ролей', () => {
  const items = [
    {folder: folder({id: 1, role: 'trash'}), label: 'Корзина'},
    {folder: folder({id: 2, role: 'inbox'}), label: 'Входящие'},
    {folder: folder({id: 3, role: null}), label: 'Б-папка'},
    {folder: folder({id: 4, role: null}), label: 'А-папка'},
  ];
  const sorted = [...items].sort(compareFolderLabels).map(i => i.label);
  assert.deepEqual(sorted, ['Входящие', 'Корзина', 'А-папка', 'Б-папка']);
});

// S-004: пустые и битые значения не ломают строку.
test('S-004: пустой display_name - показывается remote_path', () => {
  const f = folder({id: 1, remote_path: 'raw-id-1', display_name: ''});
  assert.equal(folderPathLabel(f, byId([f])), 'raw-id-1');
});
test('S-004: пусты оба поля - строка пустая, не undefined', () => {
  const f = folder({id: 1, remote_path: '', display_name: ''});
  assert.equal(folderPathLabel(f, byId([f])), '');
});
test('S-004: null-папка даёт пустую строку', () => {
  assert.equal(folderPathLabel(null, new Map()), '');
});
