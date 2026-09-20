// Проверка того, что поля ввода во всех разделах настроек стоят в колонке
// одной ширины. Раньше правая колонка строки подбиралась по содержимому:
// поле под числом "24" выходило вдвое уже поля под "3650", единица измерения
// вставала то рядом, то под полем, а поле адреса ящика было вдвое шире
// соседних. Раздел пределов из-за этого выглядел рваным.
// Задача: issue #105.
// Запуск: node --test apps/desktop/tests/js/settings-field-width.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const uiRoot = path.join(__dirname, '../../ui');
// Комментарии снимаем сразу: иначе они прилипают к следующему селектору.
const css = fs.readFileSync(path.join(uiRoot, 'styles.css'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '');
const html = fs.readFileSync(path.join(uiRoot, 'index.html'), 'utf8');

// Разбор грубый и того довольно: нужны объявления ширины, а не полная таблица
// стилей.
function rules() {
  const found = [];
  const re = /([^{}]+)\{([^{}]*)\}/g;
  let match;
  while ((match = re.exec(css))) found.push({selector: match[1].trim(), body: match[2]});
  return found;
}

// Содержимое правой колонки строки настроек: <div class="fc"> целиком. Ячейки
// вида "fc-inline" из других окон сюда не попадают - колонка настроек всегда
// объявлена отдельным классом.
function settingsFieldCells() {
  const cells = [];
  const re = /<div class="fc(\s[^"]*)?">/g;
  let match;
  while ((match = re.exec(html))) {
    let depth = 1;
    const tag = /<(\/?)div\b/g;
    tag.lastIndex = re.lastIndex;
    let inner;
    while ((inner = tag.exec(html))) {
      depth += inner[1] ? -1 : 1;
      if (depth === 0) break;
    }
    cells.push({
      extra: (match[1] || '').trim(),
      markup: html.slice(re.lastIndex, inner ? inner.index : re.lastIndex),
    });
  }
  return cells;
}

test('колонка полей настроек одной ширины во всех разделах', () => {
  // Узкий экран разворачивает колонку во всю строку своим правилом, поэтому
  // смотрим на все объявления колонки, а не на первое встреченное.
  const columns = rules().filter(rule => rule.selector === '.frow .fc');
  assert.ok(columns.length, 'правило правой колонки строки настроек потерялось');
  assert.ok(
    columns.some(rule => /width\s*:\s*var\(--field-col\)/.test(rule.body)),
    'колонка снова подбирает ширину по содержимому, и поля разъезжаются по строкам',
  );
  assert.match(css, /--field-col\s*:\s*\d+px/, 'ширина колонки нигде не задана');

  const fields = rules().filter(rule => rule.selector.includes('.frow .fc>.inp'));
  assert.ok(fields.length, 'поля в колонке не растягиваются на её ширину');
  assert.ok(
    fields.some(rule => /width\s*:\s*100%/.test(rule.body)),
    'поле занимает не всю колонку',
  );
});

test('внутри колонки настроек нет полей со своей шириной', () => {
  const offenders = [];
  // Ширина, назначенная поверх колонки правилом с большей силой: именно так
  // разнобой и заводился раньше.
  for (const rule of rules()) {
    if (!rule.selector.includes('.frow .fc')) continue;
    if (rule.selector.includes('keybind-cell')) continue;
    if (/width\s*:\s*\d+(px|em|rem|ch)/.test(rule.body)) offenders.push(`стиль ${rule.selector}`);
  }
  // Ширина прямо в разметке: атрибут style или size.
  for (const cell of settingsFieldCells()) {
    const re = /<(input|select)\b([^>]*)>/g;
    let match;
    while ((match = re.exec(cell.markup))) {
      const attrs = match[2].trim();
      if (/style="[^"]*width/.test(attrs) || /\bsize="/.test(attrs)) {
        offenders.push(`разметка ${match[1]}: ${attrs}`);
      }
    }
  }
  assert.deepEqual(offenders, [], `поля настроек получают разную ширину:\n${offenders.join('\n')}`);
});

test('поле сочетания клавиш оставляет кнопку снятия в той же строке', () => {
  const rule = rules().find(rule => rule.selector.includes('.keybind-cell>.inp'));
  assert.ok(rule, 'поле сочетания растянется на всю колонку и уведёт кнопку снятия вниз');
  assert.match(rule.body, /flex\s*:\s*1 1 0/, 'поле сочетания не делит колонку с кнопкой снятия');
});
