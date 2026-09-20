// Проверки показа меток письма в самой строке списка и у заголовка списка.
// Проверяется настоящий путь: разметка окна берётся из index.html, модули
// интерфейса выполняются целиком, панель ящика строит renderCoreAccounts,
// список открывается нажатием на папку и на метку в панели - теми же
// обработчиками, которые вызовет браузер, - а итог смотрится по узлам строки.
// Разбор меток (цвета, сегменты, счётчик) проверяется отдельно в
// message-labels.test.js, но сам по себе он ничего не доказывает: снятое
// добавление точек, переименованный класс .label-dot или потерянная полоса
// оставляли все проверки разбора зелёными, а строка списка переставала
// показывать метки.
// Спецификация: specs/message-labels-visible.md.
// Запуск: node --test apps/desktop/tests/js/message-labels-row.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const {createUiWindow} = require('./ui-window.js');

const TAGS = [
  {id: 1, name: 'Важное', color: '#e5484d'},
  {id: 2, name: 'Счета', color: '#3e63dd'},
  {id: 3, name: 'Личное', color: '#30a46c'},
  {id: 4, name: 'Отпуск', color: '#f5a524'},
];
const NEUTRAL = require('../../ui/modules/message-labels.js').LABEL_NEUTRAL;

// Три письма одной папки: с четырьмя метками (полоса из трёх цветов и
// нейтрального остатка, три точки и счётчик), с одной меткой (сплошной цвет)
// и совсем без меток (ни полосы, ни точек, ни подсказки).
function sampleMessages() {
  const base = {account_id: 1, folder_id: 2, to: [], cc: [], flags: {seen: true, flagged: false}, pinned_at: null, task_due_at: null};
  return [
    {...base, id: 11, subject: 'Счёт за сентябрь', preview: 'оплата', from: {name: 'Босс', email: 'boss@example.test'},
      labels: ['Важное', 'Счета', 'Личное', 'Отпуск'], date: '2026-09-19T10:00:00Z'},
    {...base, id: 12, subject: 'Договор', preview: 'подпись', from: {name: 'Юрист', email: 'law@example.test'},
      labels: ['Счета'], date: '2026-09-18T10:00:00Z'},
    {...base, id: 13, subject: 'Рассылка', preview: 'новости', from: {name: 'Магазин', email: 'shop@example.test'},
      labels: [], date: '2026-09-17T10:00:00Z'},
  ];
}

// Окно с одним ящиком, одной папкой и перечнем меток из ядра: метки приходят
// ответом listLabels, как в настоящем окне, - подменять coreTags руками значит
// обойти путь, по которому они попадают в строку.
async function openWindow() {
  const ui = createUiWindow({
    answers: {
      listPinnedMessages: () => ({messages: [], ordinary: []}),
      listLabels: () => TAGS,
    },
    globals: {reloadCoreData: async () => {}, resetTagPaging: () => {}, loadNextTagPage: async () => {}},
  });
  await ui.clock.drain();
  ui.evaluate(`
    messages=${JSON.stringify(sampleMessages())};
    renderCoreAccounts([{id:1,email:'me@example.test',color:'#0058ff'}],[[{id:2,account_id:1,role:'inbox',display_name:'Входящие',remote_path:'INBOX'}]]);
  `);
  await ui.clock.drain();
  return ui;
}

// Папку и метку открываем нажатием в панели: именно эти обработчики решают,
// какая метка считается открытой, и именно от этого зависит вид строки.
async function openFolder(ui) {
  ui.evaluate(`messages=${JSON.stringify(sampleMessages())};`);
  ui.query('.nav .folder-row[data-folder-id="2"]').dispatch('click');
  await ui.clock.drain();
}

async function openTag(ui, name) {
  ui.evaluate(`messages=${JSON.stringify(sampleMessages())};`);
  const row = ui.queryAll('#tagsNav .tag-row').find(item => item.textContent.includes(name));
  assert.ok(row, `метки "${name}" нет в панели: открывать нечего`);
  row.dispatch('click');
  await ui.clock.drain();
}

const rowOf = (ui, id) => ui.query(`#msgs .msg[data-message-id="${id}"]`);
const dotColors = row => row.querySelectorAll('.label-dot').map(dot => dot.style.getPropertyValue('--dot-color'));

test('S-001, S-005, S-008: строка письма показывает метки полосой, точками, счётчиком и подсказкой', async () => {
  const ui = await openWindow();
  await openFolder(ui);

  const many = rowOf(ui, 11);
  assert.ok(many, 'строка письма не построилась: проверять нечего');
  assert.ok(many.classes.has('has-labels'), 'строка письма с метками не помечена has-labels: полосе у края строки негде появиться');
  assert.equal(many.style.getPropertyValue('--label-stripe'),
    `linear-gradient(to bottom,#e5484d 0.000% 25.000%,#3e63dd 25.000% 50.000%,#30a46c 50.000% 75.000%,${NEUTRAL} 75.000% 100.000%)`,
    'полоса строки не получила цвета меток письма: разбор меток в строку не доехал');
  assert.deepEqual(dotColors(many), ['#e5484d', '#3e63dd', '#30a46c'],
    'в строке нет точек с цветами первых трёх меток: число меток по строке не сосчитать');
  const more = many.querySelector('.label-more');
  assert.ok(more, 'счётчик остальных меток в строке не нарисован: четвёртая метка письма исчезла бесследно');
  assert.equal(more.textContent, '+1', 'счётчик остальных меток показывает не то число');
  assert.equal(many.title, 'Метки: Важное, Счета, Личное, Отпуск',
    'подсказка строки не перечисляет все метки письма: полных имён в окне больше негде увидеть');

  // Одна метка - сплошной цвет: полоса у края строки остаётся цветом метки, а
  // не превращается в градиент из одного цвета.
  const single = rowOf(ui, 12);
  assert.ok(single.classes.has('has-labels'), 'строка письма с единственной меткой не помечена has-labels');
  assert.equal(single.style.getPropertyValue('--label-stripe'), '#3e63dd', 'одна метка не дала сплошного цвета полосы');
  assert.deepEqual(dotColors(single), ['#3e63dd'], 'у письма с одной меткой нет её точки');
  assert.equal(single.querySelector('.label-more'), null, 'у письма с одной меткой появился счётчик остальных');

  // S-002: письмо без меток не должно отличаться от прежнего вида строки -
  // ни полосы, ни точек, ни подсказки про метки.
  const plain = rowOf(ui, 13);
  assert.equal(plain.classes.has('has-labels'), false, 'письмо без меток помечено has-labels: у строки появится полоса');
  assert.equal(plain.style.getPropertyValue('--label-stripe'), '', 'письмо без меток получило цвет полосы');
  assert.equal(plain.querySelectorAll('.label-dot').length, 0, 'у письма без меток нарисованы точки');
  assert.equal(plain.title, '', 'у письма без меток появилась подсказка про метки');
});

test('S-010, S-011: в списке метки её цвет уходит к заголовку, а из строк пропадает', async () => {
  const ui = await openWindow();
  await openFolder(ui);
  const head = ui.byId('headTagDot');
  assert.ok(head, 'у заголовка списка нет точки открытой метки');
  assert.ok(head.classes.has('hidden'), 'в папке у заголовка показана точка метки: показывать нечего');

  await openTag(ui, 'Важное');
  assert.equal(head.classes.has('hidden'), false,
    'в списке метки точка у заголовка не показана: цвет открытой метки негде увидеть - из строк он убран');
  assert.equal(head.style.getPropertyValue('--dot-color'), '#e5484d', 'точка у заголовка нарисована не цветом открытой метки');
  assert.equal(head.title, 'Важное', 'подсказка точки у заголовка не называет открытую метку');

  // S-010: сама открытая метка из строк убрана - её цвет одинаков у всех строк
  // списка и только мешает различать письма.
  const row = rowOf(ui, 11);
  assert.deepEqual(dotColors(row), ['#3e63dd', '#30a46c', '#f5a524'],
    'открытая метка осталась точкой в строке: её цвет повторяется у каждого письма списка');
  assert.equal(row.style.getPropertyValue('--label-stripe'),
    'linear-gradient(to bottom,#3e63dd 0.000% 33.333%,#30a46c 33.333% 66.667%,#f5a524 66.667% 100.000%)',
    'полоса строки всё ещё содержит цвет открытой метки');
  assert.equal(row.querySelector('.label-more'), null,
    'после скрытия открытой метки остался счётчик остальных: считаются метки, которых в строке нет');
  // Подсказка перечисляет метки письма целиком, включая открытую: это
  // единственное место, где видны полные имена.
  assert.equal(row.title, 'Метки: Важное, Счета, Личное, Отпуск', 'подсказка строки потеряла метки письма');

  // Письмо, у которого открытая метка единственная, остаётся без полосы и без
  // точек - иначе весь список выглядел бы одинаково размеченным.
  await openTag(ui, 'Счета');
  const single = rowOf(ui, 12);
  assert.ok(single, 'письмо с открытой меткой не попало в список этой метки: проверять нечего');
  assert.equal(single.classes.has('has-labels'), false,
    'в списке своей единственной метки письмо всё равно получило полосу: цвет одинаков у всех строк');
  assert.equal(single.querySelectorAll('.label-dot').length, 0, 'открытая метка осталась единственной точкой строки');
  assert.equal(ui.byId('headTagDot').style.getPropertyValue('--dot-color'), '#3e63dd',
    'точка у заголовка не сменила цвет при переходе в другой список метки');

  // Возврат в папку снимает точку заголовка: метка больше не открыта.
  await openFolder(ui);
  assert.ok(ui.byId('headTagDot').classes.has('hidden'),
    'после возврата в папку точка у заголовка осталась: показан цвет метки, которая не открыта');
});
