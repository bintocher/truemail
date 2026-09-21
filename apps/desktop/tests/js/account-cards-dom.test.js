// Проверки аккордеона карточек ящиков в настройках на настоящем пути:
// карточки строит сам интерфейс (renderAccountSettings в settings.js), нажатия
// идут теми же обработчиками, которые вызовет браузер, раскрытая карточка
// записывается в хранилище браузера и оттуда же восстанавливается.
// Решения о том, какую карточку раскрыть, проверяются отдельно
// (account-cards.test.js) - здесь проверяется, что окно действительно ими
// пользуется: снятая привязка обработчика шапки, потерянный вызов
// applyOpenAccountDom или забытая проверка кнопок действий чистым проверкам не
// видны, а в окне карточка перестаёт раскрываться или схлопывается на каждом
// нажатии кнопки внутри неё.
// Спецификация: specs/accounts-accordion-password.md.
// Запуск: node --test apps/desktop/tests/js/account-cards-dom.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const {createUiWindow} = require('./ui-window.js');

const ACCORDION_KEY = 'truemail-account-open';

const accounts = [
  {id: 1, email: 'one@example.test', display_name: 'Первый', auth_kind: 'password', retention_days: 0},
  {id: 2, email: 'two@example.test', display_name: 'Второй', auth_kind: 'app_password', retention_days: 30},
];
const foldersByAccount = [[{id: 21, account_id: 1, role: 'inbox', display_name: 'Входящие'}], []];

async function openSettings(options = {}) {
  const ui = createUiWindow({answers: {allSettings: () => ({}), listSignatures: () => [], ...(options.answers || {})}});
  await ui.clock.drain();
  // Запись хранилища ставится до первой отрисовки: так её и застаёт раздел
  // настроек при открытии.
  if (options.saved !== undefined) ui.storage.setItem(ACCORDION_KEY, options.saved);
  await render(ui, options.accounts || accounts);
  return ui;
}

async function render(ui, list) {
  ui.evaluate(`renderAccountSettings(${JSON.stringify(list)},${JSON.stringify(foldersByAccount)},[])`);
  await ui.clock.drain();
}

const card = (ui, id) => ui.query(`#set-accounts .account-card[data-account-id="${id}"]`);
const isOpen = (ui, id) => card(ui, id).classes.has('open');
const saved = ui => ui.storage.getItem(ACCORDION_KEY);

test('S-002, S-004: нажатие на шапку раскрывает одну карточку и запоминает её в хранилище браузера', async () => {
  const ui = await openSettings();
  assert.equal(ui.queryAll('#set-accounts .account-card').length, 2, 'карточки ящиков не построились');
  assert.equal(isOpen(ui, 1), false, 'карточка раскрыта сразу после отрисовки');
  const toggle = id => card(ui, id).querySelector('.account-toggle');
  assert.equal(toggle(1).getAttribute('aria-expanded'), 'false',
    'свёрнутая карточка объявлена раскрытой для программ экранного доступа');

  card(ui, 1).querySelector('.account-toggle').dispatch('click');
  await ui.clock.drain();
  assert.equal(isOpen(ui, 1), true, 'нажатие на кнопку раскрытия не раскрыло карточку');
  assert.equal(toggle(1).getAttribute('aria-expanded'), 'true',
    'раскрытая карточка объявлена свёрнутой для программ экранного доступа');
  assert.equal(saved(ui), '1', 'раскрытая карточка не записана в хранилище: при следующем открытии раздел будет свёрнут');

  // Свободная область шапки работает так же, как кнопка.
  card(ui, 2).querySelector('.ch').dispatch('click');
  await ui.clock.drain();
  assert.equal(isOpen(ui, 2), true, 'нажатие на свободную область шапки не раскрыло карточку');
  assert.equal(isOpen(ui, 1), false, 'раскрытыми остались две карточки сразу');
  assert.equal(saved(ui), '2', 'в хранилище осталась прежняя карточка');

  // Повторное нажатие сворачивает и убирает запись: раскрытых карточек нет.
  card(ui, 2).querySelector('.ch').dispatch('click');
  await ui.clock.drain();
  assert.equal(isOpen(ui, 2), false, 'повторное нажатие не свернуло карточку');
  assert.equal(saved(ui), null, 'в хранилище осталась запись о раскрытой карточке, хотя раскрытых нет');
});

test('S-006: нажатие на кнопку действия внутри карточки не трогает аккордеон', async () => {
  const ui = await openSettings();
  card(ui, 1).querySelector('.account-toggle').dispatch('click');
  await ui.clock.drain();
  assert.equal(isOpen(ui, 1), true, 'карточка не раскрылась: проверять сворачивание нечем');

  // Кнопки стоят в шапке карточки, и их нажатие всплывает до того же
  // обработчика. Без проверки на .account-actions каждое действие схлопывало
  // бы карточку под курсором.
  card(ui, 1).querySelector('.account-folders').dispatch('click');
  await ui.clock.drain();
  assert.equal(isOpen(ui, 1), true, 'кнопка "Папки" свернула карточку');
  assert.equal(saved(ui), '1', 'кнопка действия переписала запись о раскрытой карточке');
  // Само действие при этом выполняется: иначе проверка стерегла бы мёртвую кнопку.
  assert.equal(ui.byId('set-folders').classes.has('active'), true,
    'кнопка "Папки" не открыла раздел папок: нажатие до действия не дошло');

  card(ui, 1).querySelector('.account-password').dispatch('click');
  await ui.clock.drain();
  assert.equal(isOpen(ui, 1), true, 'кнопка смены пароля свернула карточку');
  assert.equal(ui.byId('pwOverlay').classes.has('open'), true,
    'кнопка смены пароля не открыла окно смены пароля');

  // Свёрнутая карточка от кнопки действия не раскрывается.
  card(ui, 2).querySelector('.account-rename').dispatch('click');
  await ui.clock.drain();
  assert.equal(isOpen(ui, 2), false, 'кнопка действия раскрыла соседнюю карточку');
  assert.equal(isOpen(ui, 1), true, 'кнопка действия соседней карточки свернула раскрытую');
});

test('S-001, S-003: сохранённая карточка раскрывается при открытии раздела, чужая - нет', async () => {
  const ui = await openSettings({saved: '2'});
  assert.equal(isOpen(ui, 2), true, 'сохранённая карточка не раскрылась при открытии раздела');
  assert.equal(isOpen(ui, 1), false, 'вместе с сохранённой раскрылась и другая карточка');

  // Запись про ящик, которого уже нет, раскрывать нечего и хранить незачем.
  const stale = await openSettings({saved: '99'});
  assert.deepEqual(stale.queryAll('#set-accounts .account-card.open'), [],
    'раскрыта карточка по записи о несуществующем ящике');
  assert.equal(saved(stale), null, 'устаревшая запись осталась в хранилище');

  const broken = await openSettings({saved: 'not-a-number'});
  assert.deepEqual(broken.queryAll('#set-accounts .account-card.open'), [],
    'испорченная запись хранилища раскрыла карточку');
});

test('S-004: фоновая перерисовка раздела не сворачивает раскрытую карточку', async () => {
  const ui = await openSettings();
  card(ui, 2).querySelector('.ch').dispatch('click');
  await ui.clock.drain();
  assert.equal(isOpen(ui, 2), true, 'карточка не раскрылась: проверять перерисовку нечем');

  // Раздел настроек перерисовывается на каждой перезагрузке данных из ядра.
  // Состояние аккордеона живёт вне разметки, иначе оно терялось бы вместе со
  // старыми карточками, а раздел схлопывался бы сам собой.
  await render(ui, accounts);
  assert.equal(isOpen(ui, 2), true, 'перерисовка раздела свернула раскрытую карточку');
  assert.equal(saved(ui), '2', 'перерисовка стёрла запись о раскрытой карточке');

  // Ящик удалили: раскрывать больше нечего, запись убирается.
  await render(ui, [accounts[0]]);
  assert.equal(card(ui, 2), null, 'удалённый ящик остался в разделе');
  assert.equal(isOpen(ui, 1), false, 'после удаления раскрытого ящика раскрылась чужая карточка');
  assert.equal(saved(ui), null, 'после удаления раскрытого ящика запись осталась в хранилище');
});
