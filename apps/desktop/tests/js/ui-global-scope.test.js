// Проверки контроля общей области имён у файлов интерфейса.
// Спецификация: specs/ui-global-scope-check.md.
// Запуск: node --test apps/desktop/tests/js/ui-global-scope.test.js (Node 22+).
'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {topLevelNames, findCollisions} = require('../../../../scripts/ui-global-scope.js');

test('S-001: столкновение имён между двумя файлами найдено', () => {
  const collisions = findCollisions([
    {path: 'a.js', text: 'const ruleLang = 1;\n'},
    {path: 'b.js', text: 'const ruleLang=()=>1;\n'},
  ]);
  assert.equal(collisions.length, 1);
  assert.equal(collisions[0].name, 'ruleLang');
  assert.equal(collisions[0].first, 'a.js');
  assert.equal(collisions[0].second, 'b.js');
});

test('S-002: разные имена столкновения не дают', () => {
  const collisions = findCollisions([
    {path: 'a.js', text: 'const ruleLocale = 1;\n'},
    {path: 'b.js', text: 'const ruleLang = 2;\n'},
  ]);
  assert.deepEqual(collisions, []);
});

test('S-003: объявления с отступом в общую область не попадают', () => {
  const names = topLevelNames('function outer(){\n  const inner = 1;\n}\n');
  assert.ok(names.has('outer'));
  assert.ok(!names.has('inner'));
});

test('S-004: учитываются все виды объявлений верхнего уровня', () => {
  const names = topLevelNames('const a=1;\nlet b=2;\nvar c=3;\nfunction d(){}\nclass E{}\n');
  assert.deepEqual([...names].sort(), ['a', 'b', 'c', 'd', 'E'].sort());
});

test('S-005: слово const внутри примечания объявлением не считается', () => {
  const names = topLevelNames('// const spoiler = 1;\n/* const other = 2; */\nconst real = 3;\n');
  assert.deepEqual([...names], ['real']);
});

test('S-005: слово const внутри строки объявлением не считается', () => {
  const names = topLevelNames('const text = "const spoiler = 1";\nconst mark = `const other`;\n');
  assert.deepEqual([...names].sort(), ['mark', 'text']);
});

test('S-006: адрес со сдвоенной косой чертой за примечание не принимается', () => {
  const names = topLevelNames('const url = "https://example.test";\nconst next = 1;\n');
  assert.deepEqual([...names].sort(), ['next', 'url']);
});

test('S-007: столкновение с третьим файлом называет первого владельца имени', () => {
  const collisions = findCollisions([
    {path: 'a.js', text: 'const shared = 1;\n'},
    {path: 'b.js', text: 'const other = 2;\n'},
    {path: 'c.js', text: 'const shared = 3;\n'},
  ]);
  assert.equal(collisions.length, 1);
  assert.equal(collisions[0].first, 'a.js');
  assert.equal(collisions[0].second, 'c.js');
});

test('S-008: повтор имени внутри одного файла столкновением не считается', () => {
  const collisions = findCollisions([{path: 'a.js', text: 'const a=1;\nconst a=2;\n'}]);
  assert.deepEqual(collisions, []);
});

test('S-009: пустой файл имён не даёт', () => {
  assert.deepEqual([...topLevelNames('')], []);
  assert.deepEqual([...topLevelNames(null)], []);
});

test('S-010: генератор верхнего уровня опознаётся по имени', () => {
  assert.ok(topLevelNames('function* walk(){}\n').has('walk'));
});
