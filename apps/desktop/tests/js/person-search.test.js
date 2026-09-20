// Проверки поиска людей по транслитерации и раскладке:
// ui/modules/person-search.js и настоящий путь подсказки адресата - ввод в
// поле "Кому", отрисовка списка вариантов и выбор одного из них на разметке
// index.html.
// Сам подбор - чистая функция, но до пользователя он доходит через обработчик
// ввода, кэш ключей и предел из реестра ядра: проверка одной функции не
// замечает, что подсказка перестала открываться.
// Спецификация: specs/person-search-translit.md.
// Запуск: node --test apps/desktop/tests/js/person-search.test.js (Node 22+).
'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {personSearchKeys, personSearchVariants, personMatches, suggestRecipients, createPersonSearchCache} = require('../../ui/modules/person-search.js');
const {startApp} = require('./ui-app.js');

// S-002: раскладко-независимый поиск работает в обе стороны - и для набранного
// не в той раскладке латинского текста, и для кириллического.
test('S-002: набранное не в той раскладке находит и кириллицу, и латиницу', () => {
  assert.equal(personMatches('Коннова', 'rjyyjdf'), true);
  assert.equal(personMatches('Konnova', 'rjyyjdf'), true);
  assert.equal(personMatches('фы', 'as'), true);
});

// S-003, S-004, S-007: таблица транслитерации - по каждой букве в обоих
// регистрах, и сравнение идёт в обе стороны. Неоднозначные сочетания (щ, ц, ю,
// я, ы, э) и мягкий знак на конце проверяются целыми словами: по одной букве
// склейка "shch" и "sh"+"ch" неотличима.
test('S-003, S-004: таблица транслитерации побуквенно и неоднозначные сочетания целыми словами', () => {
  const table = {
    'а': 'a', 'б': 'b', 'в': 'v', 'г': 'g', 'д': 'd', 'ж': 'zh', 'з': 'z', 'и': 'i',
    'й': 'y', 'к': 'k', 'л': 'l', 'м': 'm', 'н': 'n', 'о': 'o', 'п': 'p', 'р': 'r',
    'с': 's', 'т': 't', 'у': 'u', 'ф': 'f', 'х': 'kh', 'ц': 'ts', 'ч': 'ch', 'ш': 'sh',
    'щ': 'shch', 'ы': 'y', 'э': 'e', 'ю': 'yu', 'я': 'ya',
  };
  Object.entries(table).forEach(([cyrillic, latin]) => {
    assert.ok(personSearchKeys(cyrillic).includes(latin), `${cyrillic} -> ${latin}`);
    assert.ok(personSearchKeys(cyrillic.toUpperCase()).includes(latin), `${cyrillic.toUpperCase()} -> ${latin}`);
  });
  // Твёрдый и мягкий знаки дают пустую строку - видно только в составе слова.
  assert.ok(personSearchKeys('подъезд').includes('podezd'));
  assert.ok(personSearchKeys('коньки').includes('konki'));
  const pairs = [
    ['Щука', 'Shchuka'], ['Царёв', 'Tsaryov'], ['Юлия', 'Yuliya'], ['Яна', 'Yana'],
    ['Кызыл', 'Kyzyl'], ['Элина', 'Elina'], ['Рязань', 'Ryazan'], ['Жуков', 'Zhukov'],
    ['Коннова', 'Konnova'],
  ];
  pairs.forEach(([cyrillic, latin]) => {
    assert.equal(personMatches(cyrillic, latin), true, `${cyrillic} <- ${latin}`);
    assert.equal(personMatches(latin, cyrillic), true, `${latin} <- ${cyrillic}`);
  });
});

// S-005: 'е' и 'ё' в начале слова и после гласной пишут и через 'ye'/'yo', и
// через 'e' - одно написание вместо перечня теряет половину людей.
test('S-005: буквы е и ё находятся по каждому допустимому написанию', () => {
  const cases = [
    ['Фёдор', ['Fyodor', 'Fedor']],
    ['Ёлкин', ['Yolkin', 'Yelkin']],
    ['Елена', ['Elena', 'Yelena']],
  ];
  cases.forEach(([name, queries]) => {
    queries.forEach(query => assert.equal(personMatches(name, query), true, `${name} <- ${query}`));
  });
  // Ветки транслитерации ограничены сверху: без предела слово из одних 'ё'
  // дало бы комбинаторный взрыв.
  assert.ok(personSearchKeys('ёёёёёёёёёё').length <= 8);
});

// S-007: сравнение не зависит от регистра, диакритики и формы записи Unicode.
test('S-007: регистр, диакритика и форма записи Unicode на совпадение не влияют', () => {
  assert.equal(personMatches('КОННОВА', 'коннова'), true);
  assert.equal(personMatches('Коннова', 'КОННОВА'), true);
  const umlaut = String.fromCharCode(0x00eb);
  assert.equal(personMatches('Noel', `no${umlaut}l`), true);
  assert.equal(personMatches(`No${umlaut.toUpperCase()}l`, 'noel'), true);
  // Одна и та же буква в составной (NFD) и слитной (NFC) записи.
  const composed = String.fromCharCode(0x00e9);
  const decomposed = 'e' + String.fromCharCode(0x0301);
  assert.equal(personMatches(`caf${composed}`, 'cafe'), true);
  assert.equal(personMatches(`caf${decomposed}`, `caf${composed}`), true);
});

// S-009: сравнение остаётся поиском подстроки - опечатки не исправляются.
test('S-009: подстрока находится, опечатка - нет', () => {
  assert.equal(personMatches('Валентина Коннова', 'коннова'), true);
  assert.equal(personMatches('Konnova', 'konn'), true);
  assert.equal(personMatches('Валентина Коннова', 'нет'), false);
  assert.equal(personMatches('Konnova', 'конова'), false);
});

// S-008: транслитерация включается с двух букв и не применяется к вводу
// адреса. Кириллический запрос с собачкой - единственный случай, где отказ от
// транслитерации виден: у латинского запроса транслитерировать нечего.
test('S-008: транслитерация - с двух букв и мимо ввода адреса', () => {
  assert.equal(personMatches('Konnova', 'к'), false);
  assert.equal(personMatches('Konnova', 'ко'), true);
  assert.equal(
    personMatches('konnova@example.test', 'коннова@'), false,
    'ввод адреса не должен подбираться по транслитерации: так адрес чужого человека попадал бы в подсказку',
  );
  assert.ok(!personSearchVariants('коннова@').includes('konnova@'));
  // Прямое совпадение по введённому адресу при этом работает.
  assert.equal(personMatches('konnova@example.com', 'konnova@'), true);
});

// S-014: пустой запрос подсказку не показывает.
test('S-014: пустой и пробельный запрос подсказку не открывают', () => {
  const addresses = [{name: 'Аня', email: 'a@example.com'}];
  assert.deepEqual(suggestRecipients(addresses, '', [], 8), []);
  assert.deepEqual(suggestRecipients(addresses, '   ', [], 8), []);
  assert.equal(personSearchKeys('').length, 0);
});

// S-012: ключи считаются один раз на набор данных.
test('S-012: построение ключей вызывается один раз на набор, invalidate сбрасывает кэш', () => {
  let calls = 0;
  const cache = createPersonSearchCache();
  const text = () => {
    calls += 1;
    return 'Валентина Коннова';
  };
  cache.get(1, text);
  cache.get(1, text);
  cache.get(1, text);
  assert.equal(calls, 1);
  cache.invalidate();
  cache.get(1, text);
  assert.equal(calls, 2);
  // Ключ кэша - id контакта: разные контакты не должны делить один набор ключей.
  const other = createPersonSearchCache();
  assert.notDeepEqual(other.get('a', 'Аня'), other.get('b', 'Боря'));
});

test('S-013: подсказка не показывает один адрес дважды', () => {
  const addresses = [{name: 'Аня', email: 'a@example.com'}, {name: 'Аня Д.', email: 'a@example.com'}];
  assert.equal(suggestRecipients(addresses, 'аня', [], 8).length, 1);
});

// --- Настоящий путь: подсказка адресата в поле "Кому" ---

const contact = (name, email) => ({display_name: name, emails: [{email}]});
const CONTACTS = [
  contact('Валентина Коннова', 'konnova@example.test'),
  contact('Фёдор Жуков', 'zhukov@example.test'),
  contact('Щукин Пётр', 'shchukin@example.test'),
];

function composerApp(options = {}) {
  return startApp({
    accounts: [{id: 1, email: 'me@example.test'}],
    contacts: options.contacts || CONTACTS,
    limits: {limit_recipient_suggestions: options.suggestions ?? 8},
  });
}

// Поле, меню подсказки и пункты - настоящая разметка композера.
function recipientField(app, id = 'compTo') {
  const input = app.document.getElementById(id);
  const menu = input.parent.querySelector('.recipient-suggestions');
  return {
    input,
    menu,
    type(value) {
      input.value = value;
      input.dispatch('input', {});
      return menu.querySelectorAll('.recipient-option');
    },
    labels() {
      return menu.querySelectorAll('.recipient-option span').map(node => node.textContent);
    },
    chips() {
      return input.parent.querySelectorAll('.rcpt-chip-t').map(node => node.textContent);
    },
  };
}

test('S-002, S-013: ввод латиницей и не в той раскладке открывает подсказку с нужным человеком', async () => {
  const app = composerApp();
  await app.ready();
  const field = recipientField(app);
  ['konnova', 'rjyyjdf', 'коннова'].forEach(query => {
    field.type(query);
    assert.ok(app.sandbox.document.querySelector('.recipient-suggestions').classes.has('open'), `подсказка открыта по запросу "${query}"`);
    assert.deepEqual(field.labels(), ['Валентина Коннова'], `подсказка по запросу "${query}"`);
  });
  // Адрес человека показан рядом с именем - по одному имени адресата не выбрать.
  assert.match(field.menu.querySelector('.recipient-option').textContent, /konnova@example\.test/);
});

test('S-013: выбор пункта подсказки кладёт адрес в поле и больше его не предлагает', async () => {
  const app = composerApp();
  await app.ready();
  const field = recipientField(app);
  field.type('shchukin');
  field.menu.querySelector('.recipient-option').dispatch('mousedown', {});
  assert.deepEqual(field.chips(), ['Щукин Пётр'], 'выбранный адресат показан плашкой');
  assert.equal(field.input.value, '', 'набранное заменено плашкой, а не осталось в поле');
  assert.equal(app.get("recipientModel.compTo[0].email"), 'shchukin@example.test');
  // Уже выбранный адрес в подсказке не повторяется: иначе его можно добавить дважды.
  assert.deepEqual(field.type('шукин').map(option => option.textContent), []);
  assert.equal(field.menu.classes.has('open'), false);
});

test('S-013: длина подсказки берётся из реестра пределов ядра', async () => {
  const contacts = Array.from({length: 9}, (_, index) => contact(`Коннов ${index}`, `k${index}@example.test`));
  const app = composerApp({contacts, suggestions: 3});
  await app.ready();
  const field = recipientField(app);
  assert.equal(field.type('konnov').length, 3, 'показано столько, сколько разрешает реестр');
  // Число не зашито в интерфейсе: другой предел из реестра меняет длину списка.
  app.applyLimits({limit_recipient_suggestions: 5});
  assert.equal(field.type('konnov').length, 5);
});

test('S-014: пустое поле подсказку закрывает', async () => {
  const app = composerApp();
  await app.ready();
  const field = recipientField(app);
  field.type('konnova');
  assert.equal(field.menu.classes.has('open'), true);
  field.type('   ');
  assert.equal(field.menu.classes.has('open'), false, 'пробельный ввод подсказку не открывает');
  assert.deepEqual(field.labels(), []);
});
