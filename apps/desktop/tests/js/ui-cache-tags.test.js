// Проверки логики контроля меток версий файлов интерфейса.
// Спецификация: specs/ui-cache-tag-check.md.
// Запуск: node --test apps/desktop/tests/js/ui-cache-tags.test.js (Node 22+).
'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const {checkCacheTags}=require('../../../../scripts/ui-cache-tags.js');

// Модуль flag-due-dates.js подключают оба окна - так он и стоит в исходниках,
// и на нём же проверяется случай двух подключающих файлов.
const indexHtml=tag=>`<link rel="stylesheet" href="styles.css?v=${tag}">
<script src="modules/mail.js?v=${tag}"></script>
<script src="modules/flag-due-dates.js?v=${tag}"></script>
<script src="modules/i18n-onboarding.js?v=${tag}"></script>`;
const notifyHtml=tag=>`<script src="modules/flag-due-dates.js?v=${tag}"></script>
<script src="notify.js?v=${tag}"></script>`;
const i18nModule=tag=>`const ready=fetch(\`locales/\${locale}.json?v=${tag}\`);`;

// Читалка подключающих файлов: одна метка на сторону, этого хватает проверкам.
const hosts=(baseTag,headTag)=>(path,side)=>{
  const tag=side==='base'?baseTag:headTag;
  if(path==='apps/desktop/ui/index.html')return indexHtml(tag);
  if(path==='apps/desktop/ui/notify.html')return notifyHtml(tag);
  if(path==='apps/desktop/ui/modules/i18n-onboarding.js')return i18nModule(tag);
  return null;
};

// Читалка с разными метками у окон: так выглядит правка, где метку подняли
// только в главном окне.
const splitHosts=(indexBase,indexHead,notifyBase,notifyHead)=>(path,side)=>{
  if(path==='apps/desktop/ui/index.html')return indexHtml(side==='base'?indexBase:indexHead);
  if(path==='apps/desktop/ui/notify.html')return notifyHtml(side==='base'?notifyBase:notifyHead);
  return null;
};

// S-001, S-002: метка изменённого файла обязана отличаться от метки базы.
// Неподнятая и потерянная метка - один класс ошибки: после обновления окно
// продолжает выполнять старую копию файла, поэтому оба случая идут таблицей.
test('S-001, S-002: метка изменённого файла проверяется по трём состояниям',()=>{
  const changed=[{status:'M',path:'apps/desktop/ui/modules/mail.js'}];
  const lostTag=(path,side)=>path!=='apps/desktop/ui/index.html'?null
    :(side==='base'?'<script src="modules/mail.js?v=20260101-1"></script>'
      :'<script src="modules/mail.js"></script>');
  const cases=[
    {
      name:'метка поднята',
      read:hosts('20260101-1','20260905-1'),
      violations:0,
      why:'поднятая метка нарушением быть не может, иначе проверка запретит обычную правку',
    },
    {
      name:'метка осталась прежней',
      read:hosts('20260101-1','20260101-1'),
      violations:1,
      tag:'20260101-1',
      why:'старая метка оставляет пользователю старую копию файла - это и есть ловимый дефект',
    },
    {
      name:'метка исчезла',
      read:lostTag,
      violations:1,
      tag:'метка исчезла',
      why:'файл без метки перестаёт версионироваться вовсе, а выглядит как "не подключён"',
    },
  ];
  for(const item of cases){
    const violations=checkCacheTags(changed,item.read);
    assert.equal(violations.length,item.violations,`${item.name}: ${item.why}`);
    if(item.violations){
      assert.equal(violations[0].file,'apps/desktop/ui/modules/mail.js',item.name);
      assert.equal(violations[0].host,'apps/desktop/ui/index.html',item.name);
      assert.equal(violations[0].tag,item.tag,item.name);
    }
  }
});

// Файл, подключённый обоими окнами, проверяется по каждому подключению. Пока
// смотрели только первое найденное, поднятая метка в index.html закрывала
// собой неподнятую метку в notify.html, и окно уведомления оставалось на
// старой копии модуля.
test('S-002: метка проверяется в каждом окне, а не в первом найденном',()=>{
  const changed=[{status:'M',path:'apps/desktop/ui/modules/flag-due-dates.js'}];
  const cases=[
    {
      name:'подняли в обоих окнах',
      read:splitHosts('20260101-1','20260905-1','20260101-2','20260905-2'),
      hosts:[],
    },
    {
      name:'подняли только в главном окне',
      read:splitHosts('20260101-1','20260905-1','20260101-2','20260101-2'),
      hosts:['apps/desktop/ui/notify.html'],
    },
    {
      name:'подняли только в окне уведомления',
      read:splitHosts('20260101-1','20260101-1','20260101-2','20260905-2'),
      hosts:['apps/desktop/ui/index.html'],
    },
    {
      name:'не подняли нигде',
      read:splitHosts('20260101-1','20260101-1','20260101-2','20260101-2'),
      hosts:['apps/desktop/ui/index.html','apps/desktop/ui/notify.html'],
    },
  ];
  for(const item of cases){
    const violations=checkCacheTags(changed,item.read);
    assert.deepEqual(violations.map(violation=>violation.host),item.hosts,
      `${item.name}: метку файла надо поднимать в каждом окне, где он подключён`);
  }
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
