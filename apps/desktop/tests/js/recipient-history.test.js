// Проверки подсказки получателей по истории переписки: порядок ядра
// сохраняется, предел подсказок соблюдается, кандидат из истории помечен.
// Спецификация: specs/recipient-history.md.
// Запуск: node --test apps/desktop/tests/js/recipient-history.test.js (Node 22+).

const test = require('node:test');
const assert = require('node:assert/strict');
const personSearch = require('../../ui/modules/person-search.js');
const history = require('../../ui/modules/recipient-history.js');

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
  const matches = history.historySuggestions(candidates, 'петр', new Set(), null, personSearch);
  assert.equal(matches.length, history.HISTORY_SUGGESTION_LIMIT);
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

test('S-039: пометка "из переписки" стоит только у кандидата, которого нет в контактах', () => {
  assert.equal(history.historyCandidateBadge({source: 'history'}, 'ru'), 'из переписки');
  assert.equal(history.historyCandidateBadge({source: 'history'}, 'en'), 'from correspondence');
  assert.equal(history.historyCandidateBadge({source: 'both'}, 'ru'), '');
  assert.equal(history.historyCandidateBadge({source: 'contact'}, 'ru'), '');
});

test('S-042: раздел управления называет число сохранённых отметок, а не число писем', () => {
  const row = history.historyRowText({
    name: 'Иванов Иван',
    address: 'ivanov@example.test',
    saved_touches: 50,
    last_used_at: '2026-09-17T10:00:00+00:00',
  }, 'ru');
  assert.ok(row.includes('сохранённых отметок: 50'), row);
  assert.ok(!row.includes('писем: 50'), 'число писем могло быть больше числа отметок');
  // Скрытая пользователем запись и вытесненная пределом различаются прямо.
  assert.ok(history.historyRowText({address: 'a@example.test', hidden_by_user: true}, 'ru').includes('убрано вами'));
  assert.ok(history.historyRowText({address: 'a@example.test', evicted: true}, 'ru').includes('вытеснено пределом'));
});

test('S-036 - S-038: подпись кандидата показывает имя, а при его отсутствии сам адрес', () => {
  assert.equal(history.historyCandidateLabel({name: 'Иванов Иван', email: 'ivanov@example.test'}), 'Иванов Иван');
  assert.equal(history.historyCandidateLabel({name: 'ivanov@example.test', email: 'ivanov@example.test'}), 'ivanov@example.test');
  assert.equal(history.historyCandidateLabel({name: '', email: 'ivanov@example.test'}), 'ivanov@example.test');
});
