// Проверки истории получателей: подсказка в композере и раздел управления
// историей. Отбор кандидатов проверяется на чистых функциях, а подсказка и
// раздел - настоящим путём: окно собирается из настоящего index.html, модули
// выполняются целиком, ввод и нажатия идут теми же обработчиками, которые
// вызовет браузер, а итог смотрится по разметке и по тому, что уходит в ядро.
// Спецификация: specs/recipient-history.md.
// Запуск: node --test apps/desktop/tests/js/recipient-history.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const personSearch = require('../../ui/modules/person-search.js');
const history = require('../../ui/modules/recipient-history.js');
const {limits, applyTestLimits} = require('./limits-fixture.js');
const {
  openWindow, openQueueSection, fillAccountSelect, switchLanguage,
  sectionRows, rowButton, limitPayload,
} = require('./queue-window.js');

function candidate(email, name, source) {
  return {email, name, source};
}

test('S-031, S-032: подсказка сохраняет порядок ядра и прекращает отбор на пределе', () => {
  // Ядро уже упорядочило кандидатов по частоте и давности переписки. Своего
  // порядка интерфейс не наводит: иначе выбор зависел бы от поверхности.
  const candidates = [
    candidate('petrov@example.test', 'Петров Пётр', 'history'),
    candidate('petrova@example.test', 'Петрова Анна', 'both'),
    ...Array.from({length: 10}, (_, index) =>
      candidate(`petr${index}@example.test`, `Петров однофамилец ${index}`, 'contact')),
  ];
  // Длина подсказки - настройка ядра. Число здесь нарочно не то, что у ядра
  // по умолчанию: прежде оно стояло копией в этом модуле.
  applyTestLimits({[limits.KEYS.recipientSuggestions]: 5});
  const matches = history.historySuggestions(candidates, 'петр', new Set(), null, personSearch);
  assert.equal(matches.length, 5);
  assert.equal(matches[0].email, 'petrov@example.test', 'первым остаётся кандидат ядра');
  assert.equal(matches[1].email, 'petrova@example.test');
  // Уже выбранный адрес в подсказку не возвращается.
  const used = new Set(['petrov@example.test']);
  const without = history.historySuggestions(candidates, 'петр', used, null, personSearch);
  assert.ok(!without.some(item => item.email === 'petrov@example.test'));
});

test('S-033, S-034: пустой ввод подсказок не даёт, а транслитерация и раскладка работают как у контактов', () => {
  const candidates = [candidate('ivanov@example.test', 'Иванов Иван', 'history')];
  assert.deepEqual(history.historySuggestions(candidates, '   ', new Set(), null, personSearch), []);
  assert.equal(history.historySuggestions(candidates, 'ivanov', new Set(), null, personSearch).length, 1);
  // Смена раскладки: "Иванов", набранное латиницей вслепую.
  assert.equal(history.historySuggestions(candidates, 'bdfyjd', new Set(), null, personSearch).length, 1);
});

// Кандидаты в том виде, в каком их отдаёт ядро композеру: запись только из
// переписки, запись из переписки и контактов сразу, контакт без имени и запись,
// имя которой совпадает с адресом.
function coreCandidates() {
  return [
    candidate('petrov@example.test', 'Петров Пётр', 'history'),
    candidate('petrova@example.test', 'Петрова Анна', 'both'),
    candidate('petr.plain@example.test', '', 'contact'),
    candidate('petr.same@example.test', 'petr.same@example.test', 'history'),
  ];
}

// Ввод в поле получателей идёт настоящим обработчиком: подсказку строит сам
// композер по ответу ядра.
async function suggestFor(ui, query) {
  const input = ui.byId('compTo');
  input.value = query;
  input.dispatch('input');
  await ui.clock.drain();
  return ui.queryAll('.recipient-suggestions .recipient-option').map(option => ({
    label: option.querySelector('span').textContent,
    line: option.querySelector('small').textContent,
  }));
}

test('S-036 - S-039: подсказка называет кандидата именем или адресом и помечает только запись из переписки', async () => {
  // Пометка "из переписки" отделяет запись, которой нет в контактах: без неё
  // человек не отличит случайного адресата от собственного контакта, а
  // поставленная всем подряд - вводит в заблуждение. Подпись смотрится там, где
  // её видит человек: в самой подсказке композера.
  const ui = openWindow({answers: {recipientCandidates: () => coreCandidates()}});
  fillAccountSelect(ui);
  await ui.evaluate('loadRecipientCandidates(3)');
  await ui.clock.drain();

  const options = await suggestFor(ui, 'петр');
  assert.equal(options.length, 4, 'подсказка не построилась: проверять нечего');
  const expected = [
    {label: 'Петров Пётр', line: 'petrov@example.test - из переписки', why: 'запись из переписки помечена и названа именем'},
    {label: 'Петрова Анна', line: 'petrova@example.test', why: 'запись, которая есть и в контактах, помечена как чужая'},
    {label: 'petr.plain@example.test', line: 'petr.plain@example.test', why: 'контакт без имени показан пустой строкой'},
    {label: 'petr.same@example.test', line: 'petr.same@example.test - из переписки', why: 'имя, равное адресу, показано дважды'},
  ];
  expected.forEach((item, index) => {
    assert.equal(options[index].label, item.label, item.why);
    assert.equal(options[index].line, item.line, item.why);
  });

  // Выбор варианта из подсказки уходит в поле получателей целиком: иначе в
  // письмо попадёт обрывок набранного.
  ui.queryAll('.recipient-suggestions .recipient-option')[0].dispatch('mousedown');
  await ui.clock.drain();
  assert.deepEqual(ui.evaluate('recipientFieldAddresses("compTo")').join(','), 'Петров Пётр <petrov@example.test>',
    'выбранный в подсказке адрес не попал в поле получателей вместе с именем');

  // Пометка переводится вместе с окном: иначе человек читает чужой язык в
  // своей подсказке.
  await switchLanguage(ui, 'en');
  const english = await suggestFor(ui, 'petrova');
  assert.equal(english[0].line, 'petrova@example.test', 'кандидат из контактов получил пометку');
  const englishHistory = await suggestFor(ui, 'petr.same');
  assert.equal(englishHistory[0].line, 'petr.same@example.test - from correspondence',
    'пометка не переведена вместе с окном');
});

test('S-042, S-046: раздел истории показывает сохранённые отметки и правит записи через ядро', async () => {
  // Отметок хранится не больше настроенного предела, а писем могло быть
  // больше: число писем в этой строке было бы неправдой.
  const entries = [
    {id: 4, account_id: 3, name: 'Иванов Иван', address: 'ivanov@example.test',
      saved_touches: 50, last_used_at: '2026-09-17T10:00:00+00:00'},
    {id: 5, account_id: 3, name: '', address: 'hidden@example.test', saved_touches: 3, hidden_by_user: true},
    {id: 6, account_id: 3, name: '', address: 'old@example.test', saved_touches: 1, evicted: true},
  ];
  const prompts = ['Иванов И. И.', 'ivanov@example.test'];
  const ui = openWindow({
    answers: {
      listRecipientHistory: () => entries,
      // Размер страницы истории - настройка ядра. Число нарочно не то, что у
      // ядра по умолчанию: прежде оно стояло копией в интерфейсе.
      limitSettings: () => limitPayload({limit_history_page: 7}),
    },
    globals: {prompt: () => prompts.shift() ?? null},
  });
  await ui.evaluate('reloadLimitSettings()');
  await openQueueSection(ui, 'historyAccount');

  const rows = sectionRows(ui, 'historyList');
  assert.equal(rows.length, 3, 'раздел не построил строки истории: проверять нечего');
  const first = rows[0].children[0].textContent;
  assert.ok(first.includes('сохранённых отметок: 50'), first);
  assert.ok(!first.includes('писем: 50'), 'число отметок выдано за число писем');
  assert.ok(first.includes('ivanov@example.test'), first);
  // Скрытая человеком запись и вытесненная пределом различаются прямо: иначе
  // человек решит, что программа сама забыла его адресата.
  assert.ok(rows[1].children[0].textContent.includes('убрано вами'), rows[1].children[0].textContent);
  assert.ok(rows[2].children[0].textContent.includes('вытеснено пределом'), rows[2].children[0].textContent);

  const page = ui.callsOf('listRecipientHistory').at(-1);
  assert.equal(page.args[1], 7, 'размер страницы истории взят не из ответа ядра');

  // Правка записи уходит в ядро целиком: перепутанные местами имя и адрес
  // переименовали бы запись в адрес и наоборот.
  rowButton(rows[0], 'Изменить').dispatch('click');
  await ui.clock.drain();
  assert.deepEqual(ui.callsOf('updateRecipientHistory').at(-1).args, [3, 4, 'Иванов И. И.', 'ivanov@example.test'],
    'правка записи истории ушла в ядро не тем составом');

  rowButton(sectionRows(ui, 'historyList')[1], 'Убрать').dispatch('click');
  await ui.clock.drain();
  assert.deepEqual(ui.callsOf('deleteRecipientHistoryEntry').at(-1).args, [3, 5],
    'убрана не та запись истории');
  assert.ok(ui.callsOf('listRecipientHistory').length >= 3,
    'раздел не перечитал историю после правки: человек видит прежний список');
});
