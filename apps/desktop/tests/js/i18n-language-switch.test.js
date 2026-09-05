// Проверки перевода интерфейса без подмены пользовательских данных.
// Спецификация: specs/ui-language-switch.md.
// Запуск: node --test apps/desktop/tests/js/i18n-language-switch.test.js (Node 22+).
// Модуль i18n-onboarding.js работает с DOM и здесь не выполняется: проверяется
// его исходный текст и разметка, потому что требования S-001, S-002 и S-004 -
// это утверждения о способе перевода, а не о результате одного вызова.
'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');

const uiDir=path.join(__dirname,'..','..','ui');
const read=name=>fs.readFileSync(path.join(uiDir,name),'utf8');
const i18nSource=read('modules/i18n-onboarding.js');
const indexHtml=read('index.html');
const ru=JSON.parse(read('locales/ru.json'));
const en=JSON.parse(read('locales/en.json'));

// Фразы, которые до этого изменения переводил словарь совпадений.
const formerDictionaryKeys=[
  'navSmartFolders','navAccounts','navCalendar','navContacts',
  'actionReply','actionReplyAll','actionForward','actionArchive','actionDelete','send',
  'setGeneral','setToolbar','setUnified','setFolders','setCalendars','setStorage','setThemes','setPrivacy','setKeys',
];

// S-002: подмены по совпадению текста больше нет - ни словаря, ни обхода узлов.
test('S-002: в модуле нет словаря русских фраз и обхода текстовых узлов',()=>{
  assert.equal(i18nSource.includes('uiKeyByRussian'),false,'словарь остался в модуле');
  assert.equal(i18nSource.includes('createTreeWalker'),false,'обход текстовых узлов остался в модуле');
  assert.equal(i18nSource.includes('__truemailI18nKey'),false,'пометка узлов от словаря осталась');
});

// S-001: перевод идёт по разметке.
test('S-001: перевод выполняется по признакам разметки',()=>{
  for(const attribute of ['[data-i18n]','[data-i18n-placeholder]','[data-i18n-title]','[data-i18n-aria]']){
    assert.ok(i18nSource.includes(attribute),`перевод по ${attribute} пропал`);
  }
});

// S-004: каждая фраза бывшего словаря переводится разметкой.
test('S-004: фразы бывшего словаря покрыты разметкой перевода',()=>{
  for(const key of formerDictionaryKeys){
    assert.ok(ru[key],`нет русской подписи для ключа ${key}`);
    assert.ok(en[key],`нет английской подписи для ключа ${key}`);
    assert.ok(indexHtml.includes(`data-i18n="${key}"`),`ключ ${key} не привязан к разметке`);
  }
});

// S-005, S-007, S-008: динамика перерисовывается при смене языка.
test('S-005, S-007, S-008: смена языка перерисовывает дерево папок, карточки и метки',()=>{
  const dynamic=i18nSource.slice(i18nSource.indexOf('function relocalizeDynamic'));
  for(const call of ['relocalizeFolderTree','relocalizeAccountSettings','renderTagsNav()','renderTagSettings()']){
    assert.ok(dynamic.includes(call),`перерисовка ${call} не вызывается при смене языка`);
  }
});

// S-005, S-006: подписи дерева собирает folderTitle - системные папки по роли,
// остальные именем сервера.
test('S-006: папка без системной роли сохраняет имя сервера',()=>{
  const mailSource=read('modules/mail.js');
  const line=mailSource.split('\n').find(text=>text.startsWith('function folderTitle('));
  assert.ok(line,'функция подписи папки не найдена');
  assert.ok(line.includes('folder?.display_name'),'имя папки сервера не используется');
  assert.ok(line.includes('names[folder?.role]'),'подпись по роли не используется');
});
