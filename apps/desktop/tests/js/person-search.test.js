// Проверки чистой логики apps/desktop/ui/modules/person-search.js.
// Запуск: node --test apps/desktop/tests/js/person-search.test.js (Node 20+).
'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {personSearchKeys, personSearchVariants, personMatches, suggestRecipients, createPersonSearchCache} = require('../../ui/modules/person-search.js');

// S-002: прежние способы поиска (подстрока, смена раскладки) сохраняются.
test('S-002: rjyyjdf находит и Коннова, и Konnova', () => {
  assert.equal(personMatches('Коннова', 'rjyyjdf'), true);
  assert.equal(personMatches('Konnova', 'rjyyjdf'), true);
});
test('S-002: as находит фы', () => {
  assert.equal(personMatches('фы', 'as'), true);
});
test('S-002: прямая подстрока продолжает работать', () => {
  assert.equal(personMatches('Валентина Коннова', 'коннова'), true);
  assert.equal(personMatches('Валентина Коннова', 'нет'), false);
});

// S-003: таблица транслитерации по каждой букве в обоих регистрах.
test('S-003: таблица транслитерации по каждой букве', () => {
  const table = {
    'а': 'a', 'б': 'b', 'в': 'v', 'г': 'g', 'д': 'd', 'ж': 'zh', 'з': 'z', 'и': 'i',
    'й': 'y', 'к': 'k', 'л': 'l', 'м': 'm', 'н': 'n', 'о': 'o', 'п': 'p', 'р': 'r',
    'с': 's', 'т': 't', 'у': 'u', 'ф': 'f', 'х': 'kh', 'ц': 'ts', 'ч': 'ch', 'ш': 'sh',
    'щ': 'shch', 'ы': 'y', 'э': 'e', 'ю': 'yu', 'я': 'ya',
  };
  for (const [cyr, lat] of Object.entries(table)) {
    assert.ok(personSearchKeys(cyr).includes(lat), `${cyr} -> ${lat}`);
    assert.ok(personSearchKeys(cyr.toUpperCase()).includes(lat), `${cyr.toUpperCase()} -> ${lat}`);
  }
  // Твёрдый и мягкий знаки дают пустую строку - проверяем в составе слова.
  assert.ok(personSearchKeys('подъезд').includes('podezd'));
  assert.ok(personSearchKeys('коньки').includes('konki'));
});

// S-004: неоднозначные сочетания считаются равными.
test('S-004: неоднозначные пары считаются равными', () => {
  const pairs = [
    ['Щука', 'Shchuka'], ['Царёв', 'Tsaryov'], ['Юлия', 'Yuliya'],
    ['Яна', 'Yana'], ['Кызыл', 'Kyzyl'], ['Элина', 'Elina'],
  ];
  for (const [cyr, lat] of pairs) {
    assert.equal(personMatches(cyr, lat), true, `${cyr} <- ${lat}`);
    assert.equal(personMatches(lat, cyr), true, `${lat} <- ${cyr}`);
  }
});

// S-005: варианты для е и ё.
test('S-005: Фёдор находится по Fyodor и Fedor', () => {
  assert.equal(personMatches('Фёдор', 'Fyodor'), true);
  assert.equal(personMatches('Фёдор', 'Fedor'), true);
});
test('S-005: Ёлкин находится по Yolkin и Yelkin', () => {
  assert.equal(personMatches('Ёлкин', 'Yolkin'), true);
  assert.equal(personMatches('Ёлкин', 'Yelkin'), true);
});
test('S-005: Елена находится по Elena и Yelena', () => {
  assert.equal(personMatches('Елена', 'Elena'), true);
  assert.equal(personMatches('Елена', 'Yelena'), true);
});
test('S-005: число ключей одного текста не превышает восьми', () => {
  assert.ok(personSearchKeys('ёёёёёёёёёё').length <= 8);
});

// S-006: сравнение работает в обе стороны.
test('S-006: коннова/konnova, жуков/Zhukov, shchuka/Щука - в обе стороны', () => {
  assert.equal(personMatches('Valentina Konnova', 'коннова'), true);
  assert.equal(personMatches('Валентина Коннова', 'konnova'), true);
  assert.equal(personMatches('Zhukov', 'жуков'), true);
  assert.equal(personMatches('Щука', 'shchuka'), true);
});

// S-007: нормализация независима от регистра и диакритики.
test('S-007: регистр не влияет на совпадение', () => {
  assert.equal(personMatches('КОННОВА', 'коннова'), true);
  assert.equal(personMatches('Коннова', 'КОННОВА'), true);
});
test('S-007: диакритика снимается - umlaut e против e', () => {
  const umlautNoel = 'no' + String.fromCharCode(0x00eb) + 'l'; // noel с umlaut над e
  assert.equal(personMatches('Noel', umlautNoel), true);
  assert.equal(personMatches('No' + String.fromCharCode(0x00eb).toUpperCase() + 'l', 'noel'), true);
});
test('S-007: составные символы Unicode (NFC и NFD одной буквы совпадают)', () => {
  const composed = String.fromCharCode(0x00e9); // e-acute, форма NFC
  assert.equal(personMatches('caf' + composed, 'cafe'), true);
});
test('S-007: Рязань/Ryazan', () => {
  assert.equal(personMatches('Рязань', 'Ryazan'), true);
  assert.equal(personMatches('Ryazan', 'рязань'), true);
});

// S-008: транслитерация включается с двух символов, @ отключает транслитерацию.
test('S-008: одиночная буква не транслитерируется, две буквы - да', () => {
  assert.equal(personMatches('Konnova', 'к'), false);
  assert.equal(personMatches('Konnova', 'ко'), true);
});
test('S-008: запрос с @ не транслитерируется', () => {
  const variants = personSearchVariants('konnova@');
  assert.ok(!variants.some(v => v.includes('к')));
  // прямое совпадение по-прежнему работает
  assert.equal(personMatches('konnova@example.com', 'konnova@'), true);
});

// S-009: сравнение остаётся поиском подстроки, опечатки не исправляются.
test('S-009: опечатка не находит', () => {
  assert.equal(personMatches('Konnova', 'конова'), false);
});
test('S-009: положительный случай подстроки', () => {
  assert.equal(personMatches('Konnova', 'konn'), true);
});

// S-010: набор полей не расширяется - адрес не превращается в кириллицу.
test('S-010: email не транслитерируется в кириллицу', () => {
  const keys = personSearchKeys('boris@example.com');
  assert.ok(keys.every(key => /^[a-z0-9@._+-]*$/.test(key)));
});

// S-012: ключи считаются один раз на набор данных, кэш сбрасывается по invalidate().
test('S-012: построение ключей вызывается один раз на набор', () => {
  let calls = 0;
  const cache = createPersonSearchCache();
  const text = () => { calls++; return 'Валентина Коннова'; };
  cache.get(1, text); cache.get(1, text); cache.get(1, text);
  assert.equal(calls, 1);
  cache.invalidate();
  cache.get(1, text);
  assert.equal(calls, 2);
});
test('S-012: разные id кэшируются независимо', () => {
  const cache = createPersonSearchCache();
  const keysA = cache.get('a', 'Аня');
  const keysB = cache.get('b', 'Боря');
  assert.notDeepEqual(keysA, keysB);
});

// S-013: отбор подсказки - лимит, дубли, уже выбранные адреса.
test('S-013: suggestRecipients ограничивает результат лимитом', () => {
  const addresses = Array.from({length: 10}, (_, i) => ({name: `Имя${i}`, email: `a${i}@example.com`}));
  const result = suggestRecipients(addresses, 'имя', [], 8);
  assert.equal(result.length, 8);
});
test('S-013: suggestRecipients исключает уже выбранные адреса', () => {
  const addresses = [{name: 'Аня', email: 'a@example.com'}, {name: 'Боря', email: 'b@example.com'}];
  const result = suggestRecipients(addresses, 'а', ['a@example.com'], 8);
  assert.ok(!result.some(a => a.email === 'a@example.com'));
});
test('S-013: suggestRecipients не даёт дублей по email', () => {
  const addresses = [{name: 'Аня', email: 'a@example.com'}, {name: 'Аня Д.', email: 'a@example.com'}];
  const result = suggestRecipients(addresses, 'аня', [], 8);
  assert.equal(result.length, 1);
});

// S-014: пустой запрос ничего не меняет.
test('S-014: пустой и пробельный запрос', () => {
  assert.equal(personSearchVariants('').length, 0);
  assert.equal(personSearchVariants('   ').length, 0);
  assert.equal(personSearchKeys('').length, 0);
});
test('S-014: suggestRecipients на пустой запрос не показывает подсказку', () => {
  const addresses = [{name: 'Аня', email: 'a@example.com'}];
  assert.deepEqual(suggestRecipients(addresses, '', [], 8), []);
  assert.deepEqual(suggestRecipients(addresses, '   ', [], 8), []);
});
