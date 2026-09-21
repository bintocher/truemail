// Проверки перевода интерфейса без подмены пользовательских данных.
// Смена языка выполняется по-настоящему: разметка берётся из index.html,
// словари - из ui/locales, модули окна выполняются целиком, а язык меняет тот
// же вызов, который стоит за переключателем языка. Раньше эти проверки искали
// подстроки в исходном тексте модуля, и сломанный перевод (пропавший ключ,
// неперерисованный раздел, переведённое имя папки пользователя) проходил их
// насквозь.
// Спецификация: specs/ui-language-switch.md.
// Запуск: node --test apps/desktop/tests/js/i18n-language-switch.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {createUiWindow} = require('./ui-window.js');

const uiDir = path.join(__dirname, '..', '..', 'ui');
const locale = name => JSON.parse(fs.readFileSync(path.join(uiDir, 'locales', `${name}.json`), 'utf8'));
const ru = locale('ru');
const en = locale('en');

// Ключи, которые до отказа от словаря русских фраз переводились совпадением
// текста: каждый обязан переводиться разметкой и менять текст в окне.
const FORMER_DICTIONARY_KEYS = [
  'navSmartFolders', 'navAccounts', 'navCalendar', 'navContacts',
  'actionReply', 'actionReplyAll', 'actionForward', 'actionArchive', 'actionDelete', 'send',
  'setGeneral', 'setToolbar', 'setUnified', 'setFolders', 'setCalendars', 'setStorage', 'setThemes', 'setPrivacy', 'setKeys',
];

// Окно с загруженными словарями: язык ставится тем же вызовом, что и из
// переключателя языка в настройках.
async function openWindow() {
  const ui = createUiWindow();
  await ui.clock.drain();
  await ui.evaluate('window.localizationReady');
  ui.evaluate('applyWizardLanguage("ru",false);');
  await ui.clock.drain();
  return ui;
}

async function switchTo(ui, language) {
  ui.evaluate(`applyWizardLanguage(${JSON.stringify(language)},false);`);
  await ui.clock.drain();
}

test('S-001, S-004: смена языка переводит подписи разметки во всех видах признаков и возвращает их обратно', async () => {
  const ui = await openWindow();
  const node = key => ui.query(`[data-i18n="${key}"]`);
  FORMER_DICTIONARY_KEYS.forEach(key => {
    assert.ok(ru[key] && en[key], `для ключа ${key} нет подписи в словаре одного из языков`);
    assert.ok(node(key), `ключ ${key} не привязан к разметке: подпись останется непереведённой`);
    assert.equal(node(key).textContent, ru[key], `русская подпись ${key} в окне не совпала со словарём`);
  });

  await switchTo(ui, 'en');
  assert.equal(ui.document.documentElement.lang, 'en', 'язык страницы не сменился: форматы дат и чисел останутся прежними');
  FORMER_DICTIONARY_KEYS.forEach(key => {
    assert.equal(node(key).textContent, en[key], `подпись ${key} не перевелась на английский`);
  });

  // Подсказки, подписи полей и подписи для программ экранного доступа
  // переводятся своими признаками: пропавшая ветка оставила бы их на прежнем
  // языке, а увидеть это в тексте модуля нельзя.
  const filter = ui.query('[data-i18n-placeholder="filterPlaceholder"]');
  assert.ok(filter, 'в разметке нет поля с переводимой подсказкой ввода');
  assert.equal(filter.placeholder, en.filterPlaceholder, 'подсказка поля ввода не перевелась');
  const titled = ui.query('[data-i18n-title]');
  assert.equal(titled.title, en[titled.dataset.i18nTitle], 'всплывающая подсказка не перевелась');
  const aria = ui.query('[data-i18n-aria]');
  assert.equal(aria.getAttribute('aria-label'), en[aria.dataset.i18nAria],
    'подпись для программ экранного доступа не перевелась');

  await switchTo(ui, 'ru');
  FORMER_DICTIONARY_KEYS.forEach(key => {
    assert.equal(node(key).textContent, ru[key], `возврат на русский не вернул подпись ${key}`);
  });
});

test('S-002, S-006: переводятся подписи программы, а имена пользователя остаются как есть', async () => {
  const ui = await openWindow();
  // Метка с именем "Настройки" и папка сервера с именем "Календарь" - это
  // данные пользователя. Прежний перевод по совпадению текста подменял их
  // подписями интерфейса, и метка в панели превращалась в Settings.
  // Метки ставим после отрисовки ящиков: она перечитывает их у ядра.
  ui.evaluate(`
    renderCoreAccounts([{id:1,email:'me@example.test'}],[[
      {id:2,account_id:1,role:'inbox',display_name:'INBOX',remote_path:'INBOX',unread_count:0,total_count:0},
      {id:3,account_id:1,role:null,display_name:'Календарь',remote_path:'Календарь',unread_count:0,total_count:0}]]);
  `);
  await ui.clock.drain();
  ui.evaluate(`coreTags=[{id:1,name:'Настройки',color:'#ff0000'}];renderTagsNav();`);
  await ui.clock.drain();
  const folderNames = () => ui.queryAll('.folder-row[data-folder-id] .folder-name').map(item => item.textContent);
  const tagNames = () => ui.queryAll('#tagsNav .tag-name').map(item => item.textContent);
  assert.deepEqual(folderNames(), ['Входящие', 'Календарь'], 'дерево папок построено не так, как ожидает проверка');
  assert.deepEqual(tagNames(), ['Настройки'], 'метка пользователя не показана в панели');

  await switchTo(ui, 'en');
  assert.deepEqual(folderNames(), ['Inbox', 'Календарь'],
    'системная папка не перевелась по роли либо переведено имя папки пользователя');
  assert.deepEqual(tagNames(), ['Настройки'],
    'имя метки пользователя переведено: перевод снова идёт по совпадению текста');
});

test('S-005, S-007, S-008: смена языка перерисовывает разделы, собранные в коде, и запоминается в ядре', async () => {
  const ui = await openWindow();
  ui.evaluate(`
    currentFolderId=2;
    renderCoreAccounts([{id:1,email:'me@example.test'}],[[{id:2,account_id:1,role:'inbox',display_name:'INBOX',remote_path:'INBOX',unread_count:3,total_count:5}]]);
  `);
  await ui.clock.drain();
  const heading = () => ui.query('.listhead h2').textContent;
  assert.equal(heading(), 'Входящие', 'заголовок открытой папки собран не по роли');

  await switchTo(ui, 'en');
  assert.equal(heading(), 'Inbox',
    'заголовок открытой папки остался на прежнем языке: разделы, собранные в коде, не перерисовываются');

  // Выбор языка уходит в ядро только после готовности хранилища: иначе
  // язык сбрасывался бы при следующем запуске.
  assert.equal(ui.callsOf('setSetting').length, 0, 'язык записан в ядро до готовности хранилища');
  ui.evaluate('window.tmStorageReady=true;applyWizardLanguage("en",true);');
  await ui.clock.drain();
  const saved = ui.callsOf('setSetting').at(-1);
  assert.ok(saved, 'выбор языка не ушёл в ядро: при следующем запуске язык будет прежним');
  assert.deepEqual([saved.args[0], saved.args[1]], ['locale', 'en'], 'в ядро ушёл не выбранный язык');
});
