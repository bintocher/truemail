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

// Объявления правила по именам: привязка к точному написанию значения роняла
// проверку на равнозначной записи, поэтому смотрим на смысл, а не на текст.
function declarations(body) {
  const found = new Map();
  for (const part of body.split(';')) {
    const colon = part.indexOf(':');
    if (colon < 0) continue;
    found.set(part.slice(0, colon).trim().toLowerCase(), part.slice(colon + 1).trim().toLowerCase());
  }
  return found;
}

// Растяжимость и исходный размер поля. Записи "flex:1", "flex:1 1 0",
// "flex:1 1 0%" и "flex-grow:1;flex-basis:0" означают одно и то же, а "none" -
// это "0 0 auto".
function flexOf(decls) {
  let grow = null;
  let basis = null;
  const short = decls.get('flex');
  if (short === 'none') { grow = 0; basis = 'auto'; }
  else if (short === 'auto') { grow = 1; basis = 'auto'; }
  else if (short) {
    const parts = short.split(/\s+/);
    grow = Number(parts[0]);
    if (parts.length === 1) basis = '0%';
    else if (parts.length === 2) basis = /^[\d.]+$/.test(parts[1]) ? '0%' : parts[1];
    else basis = parts[2];
  }
  if (decls.has('flex-grow')) grow = Number(decls.get('flex-grow'));
  if (decls.has('flex-basis')) basis = decls.get('flex-basis');
  return {grow, basis};
}

// Размер, назначенный самому полю. "100%" - это ширина колонки, а не своя;
// "auto" и "0" ширины не задают. Всё остальное с единицей измерения или в
// процентах - собственная ширина, из-за которой поля и разъезжались.
function ownWidth(value) {
  if (value === undefined || value === null) return null;
  const text = String(value).trim();
  if (!text || text === 'auto' || text === '100%' || text === 'inherit' || text === 'initial') return null;
  if (/^0(\.0+)?(px|em|rem|ch|%)?$/.test(text)) return null;
  if (text.startsWith('var(')) return null;
  return /^[\d.]+(px|em|rem|ch|vw|vh|%)$/.test(text) ? text : null;
}

// Правила, которые целят в само поле внутри колонки настроек, а не в колонку.
// Ширина колонки задаётся ей самой и законна; своя ширина поля - нет.
function fieldRules() {
  return rules().filter(rule => /\.fc\b/.test(rule.selector)
    && /\.fc[^,]*?[>\s]\s*\.(inp|sel)\b/.test(rule.selector));
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

// S-005: поле не задаёт себе ширину само. Ширину поле получает от колонки,
// поэтому нарушением считается любой собственный размер: и в пикселях, и в
// процентах, и исходным размером flex-basis - именно процентами и базисом
// разнобой и возвращается. Ограничители min-width и max-width ширины не
// задают: min-width:0 и max-width:100% стоят у правильного правила и
// нарушением быть не могут.
test('внутри колонки настроек нет полей со своей шириной', () => {
  const targeted = fieldRules();
  assert.ok(targeted.length >= 3,
    `правила полей колонки перестали разбираться: найдено ${targeted.length}`);
  const offenders = [];
  for (const rule of targeted) {
    const decls = declarations(rule.body);
    const width = ownWidth(decls.get('width'));
    if (width) offenders.push(`стиль ${rule.selector}: width ${width}`);
    const basis = ownWidth(flexOf(decls).basis);
    if (basis) offenders.push(`стиль ${rule.selector}: исходный размер flex ${basis}`);
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

// S-003, S-007: в колонке с парой "поле и кнопка" поле обязано делиться
// местом. Проверяется смысл записи, а не её текст: "flex:1", "flex:1 1 0" и
// "flex-grow:1;flex-basis:0" - одно и то же, и равнозначная запись не должна
// ронять проверку там, где вёрстка исправна.
test('поле рядом с кнопкой не уводит её на другую строку', () => {
  const bySelector = part => rules().find(rule => rule.selector.includes(part));

  const keybind = bySelector('.keybind-cell>.inp');
  assert.ok(keybind, 'поле сочетания растянется на всю колонку и уведёт кнопку снятия вниз');
  const keybindFlex = flexOf(declarations(keybind.body));
  assert.ok(keybindFlex.grow >= 1,
    `поле сочетания не забирает остаток колонки (растяжимость ${keybindFlex.grow}), и кнопка снятия уезжает вниз`);
  assert.ok(keybindFlex.basis !== '100%',
    'поле сочетания начинается с полной колонки, поэтому кнопке снятия места уже не остаётся');

  // Ряд "поле и кнопка" и ряд из двух таких пар (следующее и предыдущее
  // письмо) размечены классом fc-inline. Без своего правила каждое поле
  // занимает колонку целиком, и ряд рассыпается на четыре строки.
  const inline = bySelector('.fc-inline>.inp');
  assert.ok(inline, 'парный ряд рассыплется: поле займёт всю колонку');
  const inlineDecls = declarations(inline.body);
  const inlineFlex = flexOf(inlineDecls);
  assert.equal(inlineFlex.grow, 0,
    'поле парного ряда снова тянется на всю колонку, и сосед уходит на следующую строку');
  assert.ok((inlineDecls.get('width') || 'auto') !== '100%',
    'поле парного ряда занимает колонку целиком шириной, а не растяжимостью - ряд всё равно рассыплется');

  // Колонка с двумя и более полями обязана нести один из этих двух классов,
  // иначе исключение её не застанет.
  const offenders = [];
  for (const cell of settingsFieldCells()) {
    const fields = [...cell.markup.matchAll(/<(input|select)\b/g)].length;
    if (fields < 2) continue;
    if (/keybind-cell|fc-inline/.test(cell.extra)) continue;
    offenders.push(cell.markup.slice(0, 80));
  }
  assert.deepEqual(offenders, [], `колонка с несколькими полями без своего правила:\n${offenders.join('\n')}`);
});
