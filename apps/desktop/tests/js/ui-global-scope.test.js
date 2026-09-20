// Проверки контроля общей области имён у файлов интерфейса.
// Спецификация: specs/ui-global-scope-check.md.
// Запуск: node --test apps/desktop/tests/js/ui-global-scope.test.js (Node 22+).
'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {topLevelNames, findCollisions} = require('../../../../scripts/ui-global-scope.js');

// S-001, S-002, S-007: столкновение имён - это отказ половины программы без
// единого сообщения, поэтому проверка обязана назвать имя, владельца и файл,
// который перестанет выполняться. Пропуск столкновения и выдуманное
// столкновение одинаково вредны, поэтому оба случая идут одной таблицей.
test('S-001, S-002, S-007: столкновения имён между файлами найдены и названы', () => {
  const cases = [
    {
      name: 'одно имя в двух файлах',
      files: [
        {path: 'a.js', text: 'const ruleLang = 1;\n'},
        {path: 'b.js', text: 'const ruleLang=()=>1;\n'},
      ],
      expect: [{name: 'ruleLang', first: 'a.js', second: 'b.js'}],
      why: 'ровно так ruleLang и уронил разбор smart-rules.js целиком',
    },
    {
      name: 'разные имена',
      files: [
        {path: 'a.js', text: 'const ruleLocale = 1;\n'},
        {path: 'b.js', text: 'const ruleLang = 2;\n'},
      ],
      expect: [],
      why: 'выдуманное столкновение заставит переименовывать исправные имена',
    },
    {
      name: 'столкновение через файл',
      files: [
        {path: 'a.js', text: 'const shared = 1;\n'},
        {path: 'b.js', text: 'const other = 2;\n'},
        {path: 'c.js', text: 'const shared = 3;\n'},
      ],
      expect: [{name: 'shared', first: 'a.js', second: 'c.js'}],
      why: 'владельцем имени должен называться тот файл, что объявил его первым по порядку подключения',
    },
    {
      name: 'повтор имени внутри одного файла',
      files: [{path: 'a.js', text: 'const a=1;\nconst a=2;\n'}],
      expect: [],
      why: 'повтор внутри файла находит сам разбор файла, проверке тут добавить нечего',
    },
  ];
  for (const item of cases) {
    assert.deepEqual(findCollisions(item.files), item.expect, `${item.name}: ${item.why}`);
  }
});

// S-004, S-010: в общую область попадает объявление любого вида, а слово,
// которое лишь начинается с ключевого (constant, functionName), объявлением не
// является. Разбор построчный, поэтому оба края правила проверяются вместе.
test('S-004, S-010: распознаются все виды объявлений и только они', () => {
  const cases = [
    {source: 'const a=1;\n', expect: ['a'], why: 'const - самый частый вид объявления'},
    {source: 'let b=2;\n', expect: ['b'], why: 'let делит область так же, как const'},
    {source: 'var c=3;\n', expect: ['c'], why: 'var в старых файлах интерфейса ещё встречается'},
    {source: 'function d(){}\n', expect: ['d'], why: 'имя функции тоже занимает общую область'},
    {source: 'function* walk(){}\n', expect: ['walk'], why: 'генератор объявляет имя через звёздочку'},
    {source: 'function *walk2(){}\n', expect: ['walk2'], why: 'звёздочка бывает записана у имени'},
    {source: 'class E{}\n', expect: ['E'], why: 'class повторно объявить нельзя так же, как const'},
    {source: 'constant = 1;\n', expect: [], why: 'слово constant начинается с const, но объявлением не является'},
    {source: 'functionName();\n', expect: [], why: 'вызов functionName() имени не объявляет'},
    {source: 'className.toggle("on");\n', expect: [], why: 'обращение к className объявлением не является'},
  ];
  for (const item of cases) {
    assert.deepEqual([...topLevelNames(item.source)], item.expect, `${item.source.trim()}: ${item.why}`);
  }
});

// S-005, S-006: ключевое слово внутри примечания или строки объявлением не
// является, а адрес со сдвоенной косой чертой не является примечанием. Ошибка
// в любую сторону одинаково плоха: лишнее имя даёт выдуманное столкновение,
// потерянное - пропускает настоящее.
test('S-005, S-006: примечания и строки за объявления не принимаются', () => {
  const cases = [
    {
      source: '// const spoiler = 1;\nconst real = 3;\n',
      expect: ['real'],
      why: 'имя из однострочного примечания попало бы в общую область как чужое',
    },
    {
      source: '/* const other = 2; */\nconst real = 3;\n',
      expect: ['real'],
      why: 'то же в многострочном примечании',
    },
    {
      source: 'const text = "const spoiler = 1";\n',
      expect: ['text'],
      why: 'пример кода в строке объявлением не является',
    },
    {
      source: "const text = 'const spoiler = 1';\nconst mark = `const other`;\n",
      expect: ['text', 'mark'],
      why: 'одинарные и шаблонные кавычки прячут ключевое слово так же',
    },
    {
      source: 'const url = "https://example.test";\nconst next = 1;\n',
      expect: ['url', 'next'],
      why: 'сдвоенная косая в адресе - не начало примечания, иначе потерялась бы вся строка с объявлением',
    },
  ];
  for (const item of cases) {
    assert.deepEqual([...topLevelNames(item.source)].sort(), [...item.expect].sort(), item.why);
  }
});

test('S-003: объявления с отступом в общую область не попадают', () => {
  const names = topLevelNames('function outer(){\n  const inner = 1;\n}\n');
  assert.ok(names.has('outer'));
  assert.ok(!names.has('inner'), 'имя внутри функции столкнуться с чужим не может');
});

test('S-009: пустой файл имён не даёт', () => {
  assert.deepEqual([...topLevelNames('')], []);
  assert.deepEqual([...topLevelNames(null)], []);
});
