// Проверки показа меток письма в списке.
// Спецификация: specs/message-labels-visible.md.
// Запуск: node --test apps/desktop/tests/js/message-labels.test.js (Node 22+).

const test = require('node:test');
const assert = require('node:assert/strict');
const labels = require('../../ui/modules/message-labels.js');

const TAGS = [
  {id: 1, name: 'Важное', color: '#e5484d'},
  {id: 2, name: 'Счета', color: '#3e63dd'},
  {id: 3, name: 'Личное', color: '#30a46c'},
  {id: 4, name: 'Отпуск', color: '#f5a524'},
  {id: 5, name: 'Без цвета', color: null},
];

test('S-009: цвет метки берётся по имени', () => {
  assert.equal(labels.labelColor('Счета', TAGS), '#3e63dd');
});

test('S-009: неизвестная метка получает нейтральный цвет, а не пропадает', () => {
  assert.equal(labels.labelColor('Удалённая', TAGS), labels.LABEL_NEUTRAL);
  assert.equal(labels.labelColor('Счета', []), labels.LABEL_NEUTRAL);
});

test('S-009: пустой цвет в перечне заменяется нейтральным', () => {
  assert.equal(labels.labelColor('Без цвета', TAGS), labels.LABEL_NEUTRAL);
  assert.equal(labels.labelColor('Пробелы', [{id: 9, name: 'Пробелы', color: '   '}]), labels.LABEL_NEUTRAL);
});

test('S-010: в списке метки сама метка скрыта, остальные показаны', () => {
  assert.deepEqual(labels.shownLabels(['Важное', 'Счета'], 'Важное'), ['Счета']);
});

test('S-010: вне списка метки показываются все метки письма', () => {
  assert.deepEqual(labels.shownLabels(['Важное', 'Счета'], null), ['Важное', 'Счета']);
});

test('S-002: письмо без меток не даёт ни полосы, ни точек', () => {
  assert.deepEqual(labels.shownLabels(null, null), []);
  assert.equal(labels.stripeValue([], TAGS), '');
  assert.deepEqual(labels.dotsModel([], TAGS), {dots: [], more: 0});
});

test('S-002: письмо с единственной меткой в её же списке остаётся без полосы', () => {
  const shown = labels.shownLabels(['Важное'], 'Важное');
  assert.deepEqual(shown, []);
  assert.equal(labels.stripeValue(shown, TAGS), '');
});

test('S-001: одна метка - сплошной цвет без градиента', () => {
  assert.equal(labels.stripeValue(['Счета'], TAGS), '#3e63dd');
});

test('S-003: две метки делят полосу пополам в порядке меток письма', () => {
  assert.equal(
    labels.stripeValue(['Важное', 'Счета'], TAGS),
    'linear-gradient(to bottom,#e5484d 0.000% 50.000%,#3e63dd 50.000% 100.000%)',
  );
});

test('S-003: три метки делят полосу на равные трети', () => {
  const value = labels.stripeValue(['Важное', 'Счета', 'Личное'], TAGS);
  assert.equal(
    value,
    'linear-gradient(to bottom,#e5484d 0.000% 33.333%,#3e63dd 33.333% 66.667%,#30a46c 66.667% 100.000%)',
  );
});

test('S-004: от четырёх меток - три сегмента и один нейтральный', () => {
  const value = labels.stripeValue(['Важное', 'Счета', 'Личное', 'Отпуск', 'Пятая'], TAGS);
  const stops = value.slice('linear-gradient(to bottom,'.length, -1).split(',');
  assert.equal(stops.length, 4);
  assert.ok(stops[3].startsWith(labels.LABEL_NEUTRAL));
  assert.ok(value.includes('#30a46c'));
  assert.ok(!value.includes('#f5a524'));
});

test('S-005: точки повторяют цвета первых трёх меток', () => {
  assert.deepEqual(labels.dotsModel(['Важное', 'Счета'], TAGS), {
    dots: ['#e5484d', '#3e63dd'],
    more: 0,
  });
});

test('S-006: сверх трёх меток показывается счётчик остатка', () => {
  const model = labels.dotsModel(['Важное', 'Счета', 'Личное', 'Отпуск', 'Пятая'], TAGS);
  assert.equal(model.dots.length, 3);
  assert.equal(model.more, 2);
});

test('S-006: ровно три метки счётчика не дают', () => {
  assert.equal(labels.dotsModel(['Важное', 'Счета', 'Личное'], TAGS).more, 0);
});

test('пустые имена в перечне меток письма отбрасываются', () => {
  assert.deepEqual(labels.shownLabels(['Важное', '', null, undefined], null), ['Важное']);
});
