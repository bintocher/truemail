// Проверки разбора описания выпуска для окна "что нового".
// Задача: issue #99.
// Запуск: node --test apps/desktop/tests/js/release-notes.test.js (Node 22+).
'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {parseReleaseNotes} = require('../../ui/modules/release-notes.js');

// Настоящее описание выпуска 0.3.1 в том виде, в каком оно приходит из
// манифеста обновления: заголовки разделов, пункты списка и перенос строки
// внутри одного пункта.
const REAL_NOTES = `### Исправлено

- Сроки дел и закрепления писем больше не пропадают сами. Раньше их стирала
обычная проверка почты.
- Выполненные дела больше не возвращаются в работу сами.

### Безопасность

- Значок быстрого действия принимается только из готового набора.
`;

test('описание выпуска разбирается на разделы и пункты', () => {
  const nodes = parseReleaseNotes(REAL_NOTES);
  assert.deepEqual(
    nodes.map(node => node.kind),
    ['heading', 'item', 'item', 'heading', 'item'],
  );
  assert.equal(nodes[0].text, 'Исправлено');
  assert.equal(nodes[3].text, 'Безопасность');
});

test('перенос строки внутри пункта не рвёт его на два', () => {
  const nodes = parseReleaseNotes(REAL_NOTES);
  assert.ok(nodes[1].text.endsWith('обычная проверка почты.'));
  assert.ok(!nodes[1].text.includes('\n'));
});

test('уровень заголовка сохраняется', () => {
  const nodes = parseReleaseNotes('# Выпуск\n\n## Исправлено\n\n#### Мелочи\n');
  assert.deepEqual(nodes.map(node => node.level), [1, 2, 4]);
});

test('нумерованный список остаётся пунктами с номерами', () => {
  const nodes = parseReleaseNotes('1. Первое\n2) Второе\n');
  assert.deepEqual(nodes.map(node => node.kind), ['item', 'item']);
  assert.equal(nodes[0].text, '1. Первое');
  assert.equal(nodes[1].text, '2. Второе');
});

test('звёздочки и подчёркивания выделения снимаются, а не показываются', () => {
  const nodes = parseReleaseNotes('- **Важное** и *ещё* и `код` и __сильное__\n');
  assert.equal(nodes[0].text, 'Важное и ещё и код и сильное');
});

test('звёздочка внутри слова и одиночная остаются на месте', () => {
  const nodes = parseReleaseNotes('Формула 2*3 и звёздочка * сама по себе\n');
  assert.equal(nodes[0].text, 'Формула 2*3 и звёздочка * сама по себе');
});

test('пустое и отсутствующее описание не дают узлов', () => {
  assert.deepEqual(parseReleaseNotes(''), []);
  assert.deepEqual(parseReleaseNotes(null), []);
  assert.deepEqual(parseReleaseNotes('   \n\n  \n'), []);
});

test('абзацы разделяются пустой строкой, а не склеиваются', () => {
  const nodes = parseReleaseNotes('Первый абзац\nего продолжение\n\nВторой абзац\n');
  assert.deepEqual(nodes.map(node => node.kind), ['text', 'text']);
  assert.equal(nodes[0].text, 'Первый абзац его продолжение');
  assert.equal(nodes[1].text, 'Второй абзац');
});

test('решётка без текста заголовком не считается', () => {
  assert.deepEqual(parseReleaseNotes('###\n'), []);
});
