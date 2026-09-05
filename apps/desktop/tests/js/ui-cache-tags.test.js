// Проверки логики контроля меток версий файлов интерфейса.
// Спецификация: specs/ui-cache-tag-check.md.
// Запуск: node --test apps/desktop/tests/js/ui-cache-tags.test.js (Node 22+).
'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const {checkCacheTags}=require('../../../../scripts/ui-cache-tags.js');

const indexHtml=tag=>`<link rel="stylesheet" href="styles.css?v=${tag}">
<script src="modules/mail.js?v=${tag}"></script>
<script src="modules/i18n-onboarding.js?v=${tag}"></script>`;
const notifyHtml=tag=>`<script src="notify.js?v=${tag}"></script>`;
const i18nModule=tag=>`const ready=fetch(\`locales/\${locale}.json?v=${tag}\`);`;

// Читалка подключающих файлов: одна метка на сторону, этого хватает проверкам.
const hosts=(baseTag,headTag)=>(path,side)=>{
  const tag=side==='base'?baseTag:headTag;
  if(path==='apps/desktop/ui/index.html')return indexHtml(tag);
  if(path==='apps/desktop/ui/notify.html')return notifyHtml(tag);
  if(path==='apps/desktop/ui/modules/i18n-onboarding.js')return i18nModule(tag);
  return null;
};

// S-001: метка поднята - нарушений нет.
test('S-001: изменённый модуль с поднятой меткой проходит проверку',()=>{
  const changes=[{status:'M',path:'apps/desktop/ui/modules/mail.js'}];
  assert.deepEqual(checkCacheTags(changes,hosts('20260101-1','20260905-1')),[]);
});

// S-002: метка та же - проверка называет файл и подключающий файл.
test('S-002: изменённый модуль без поднятой метки даёт нарушение',()=>{
  const changes=[{status:'M',path:'apps/desktop/ui/modules/mail.js'}];
  const violations=checkCacheTags(changes,hosts('20260101-1','20260101-1'));
  assert.equal(violations.length,1);
  assert.equal(violations[0].file,'apps/desktop/ui/modules/mail.js');
  assert.equal(violations[0].host,'apps/desktop/ui/index.html');
});

// S-003: локализации versioned строкой запроса внутри модуля.
test('S-003: изменённый файл локализации требует новой метки локализаций',()=>{
  const changes=[{status:'M',path:'apps/desktop/ui/locales/ru.json'}];
  assert.equal(checkCacheTags(changes,hosts('20260101-1','20260101-1')).length,1);
  assert.deepEqual(checkCacheTags(changes,hosts('20260101-1','20260905-1')),[]);
});

// S-004: изменения вне интерфейса проверку не касаются.
test('S-004: файлы вне apps/desktop/ui пропускаются',()=>{
  const changes=[{status:'M',path:'crates/core/src/backend/imap.rs'},{status:'M',path:'README.md'}];
  assert.deepEqual(checkCacheTags(changes,hosts('20260101-1','20260101-1')),[]);
});

// S-005: файл без подключения по адресу метки не имеет.
test('S-005: файл интерфейса без подключения по адресу пропускается',()=>{
  const changes=[{status:'M',path:'apps/desktop/ui/assets/logo.svg'}];
  assert.deepEqual(checkCacheTags(changes,hosts('20260101-1','20260101-1')),[]);
});

// S-006: удалённый файл нарушением не считается.
test('S-006: удалённый файл пропускается',()=>{
  const changes=[{status:'D',path:'apps/desktop/ui/modules/mail.js'}];
  assert.deepEqual(checkCacheTags(changes,hosts('20260101-1','20260101-1')),[]);
});

// S-008: перечисляются все нарушения, а не только первое.
test('S-008: в отчёт попадают все файлы с неподнятыми метками',()=>{
  const changes=[
    {status:'M',path:'apps/desktop/ui/modules/mail.js'},
    {status:'M',path:'apps/desktop/ui/styles.css'},
    {status:'M',path:'apps/desktop/ui/notify.js'},
  ];
  const violations=checkCacheTags(changes,hosts('20260101-1','20260101-1'));
  assert.equal(violations.length,3);
  assert.equal(violations[2].host,'apps/desktop/ui/notify.html');
});
