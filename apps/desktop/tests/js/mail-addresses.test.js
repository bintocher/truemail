// Проверки чистой логики apps/desktop/ui/modules/mail-addresses.js.
// Запуск: node --test apps/desktop/tests/js/mail-addresses.test.js (Node 20+).
// Каталог вне apps/desktop/ui, поэтому в дистрибутив Tauri не попадает.
'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const {rowPresentation,addressLineModel,displayName}=require('../../ui/modules/mail-addresses.js');

const rolesOf=map=>new Map(Object.entries(map).map(([id,role])=>[Number(id),role]));
const addr=(name,email)=>({name,email});

// S-001: сторона строки определяется ролью папки письма, flags.draft не участвует.
test('S-001: роль sent/drafts дает получателя, остальные - отправителя',()=>{
  const message={folder_id:1,from:addr('Аня','anna@example.com'),to:[addr('Боря','boris@example.com')],cc:[]};
  assert.equal(rowPresentation({...message,folder_id:1},rolesOf({1:'sent'})).kind,'recipient');
  assert.equal(rowPresentation({...message,folder_id:2},rolesOf({2:'drafts'})).kind,'recipient');
  assert.equal(rowPresentation({...message,folder_id:3},rolesOf({3:'inbox'})).kind,'sender');
  assert.equal(rowPresentation({...message,folder_id:4},rolesOf({4:'archive'})).kind,'sender');
  assert.equal(rowPresentation({...message,folder_id:5},rolesOf({5:'trash'})).kind,'sender');
  assert.equal(rowPresentation({...message,folder_id:6},rolesOf({6:'spam'})).kind,'sender');
  assert.equal(rowPresentation({...message,folder_id:99},rolesOf({1:'sent'})).kind,'sender'); // папка не найдена
  assert.equal(rowPresentation({...message,folder_id:1},new Map()).kind,'sender'); // роль неизвестна
});
test('S-001: flags.draft=true в роли inbox все равно показывает отправителя',()=>{
  const message={folder_id:1,from:addr('Аня','anna@example.com'),to:[addr('Боря','boris@example.com')],cc:[],flags:{draft:true}};
  const result=rowPresentation(message,rolesOf({1:'inbox'}));
  assert.equal(result.kind,'sender');
  assert.equal(result.text,'Аня');
});

// S-002: формат "первый +N" по to, с запасным источником cc.
test('S-002: k=1 - подпись без счетчика',()=>{
  const message={folder_id:1,from:addr('','from@example.com'),to:[addr('Боря','boris@example.com')],cc:[]};
  const result=rowPresentation(message,rolesOf({1:'sent'}));
  assert.equal(result.text,'Боря');assert.equal(result.extra,0);
});
test('S-002: k=2,3,5 - первый и суффикс +N',()=>{
  const many=n=>Array.from({length:n},(_,i)=>addr(`Имя${i}`,`a${i}@example.com`));
  for(const k of [2,3,5]){
    const message={folder_id:1,from:addr('',''),to:many(k),cc:[]};
    const result=rowPresentation(message,rolesOf({1:'sent'}));
    assert.equal(result.text,'Имя0');assert.equal(result.extra,k-1);
  }
});
test('S-002: адрес без name показывается по email',()=>{
  const message={folder_id:1,from:addr('',''),to:[addr('','boris@example.com')],cc:[]};
  assert.equal(rowPresentation(message,rolesOf({1:'sent'})).text,'boris@example.com');
});
test('S-002: пустой первый элемент не дает пустую подпись',()=>{
  const message={folder_id:1,from:addr('',''),to:[addr('',''),addr('Аня','a@example.com'),addr('Боря','b@example.com')],cc:[]};
  const result=rowPresentation(message,rolesOf({1:'sent'}));
  assert.equal(result.text,'Аня');assert.equal(result.extra,1); // пустой элемент не считается
});
test('S-002: повторяющиеся адреса не схлопываются',()=>{
  const message={folder_id:1,from:addr('',''),to:[addr('Аня','a@example.com'),addr('Аня','a@example.com')],cc:[]};
  const result=rowPresentation(message,rolesOf({1:'sent'}));
  assert.equal(result.text,'Аня');assert.equal(result.extra,1);
});
test('S-002: пустой to и непустой cc - подпись строится по cc',()=>{
  const message={folder_id:1,from:addr('',''),to:[],cc:[addr('Копия','cc@example.com')]};
  const result=rowPresentation(message,rolesOf({1:'sent'}));
  assert.equal(result.kind,'recipient');assert.equal(result.text,'Копия');assert.equal(result.extra,0);
});

// S-003: нет ни одного отображаемого адреса ни в to, ни в cc.
test('S-003: to=[] и cc пуст - заглушка',()=>{
  const result=rowPresentation({folder_id:1,from:addr('',''),to:[],cc:[]},rolesOf({1:'sent'}));
  assert.deepEqual(result,{kind:'empty',text:'',extra:0,initial:'?'});
});
test('S-003: to отсутствует',()=>{
  const result=rowPresentation({folder_id:1,from:addr('','')},rolesOf({1:'drafts'}));
  assert.equal(result.kind,'empty');
});
test('S-003: to из одного пустого адреса',()=>{
  const result=rowPresentation({folder_id:1,from:addr('',''),to:[addr('','')],cc:[]},rolesOf({1:'sent'}));
  assert.equal(result.kind,'empty');
});
test('S-003: to и cc оба пусты (с непустыми элементами без полей)',()=>{
  const result=rowPresentation({folder_id:1,from:addr('',''),to:[addr('  ','')],cc:[addr('',' ')]},rolesOf({1:'drafts'}));
  assert.equal(result.kind,'empty');
});
// Отката к отправителю быть не должно: заглушка выигрывает даже когда from
// заполнен - иначе в Отправленных снова видно себя.
test('S-003: непустой from не подменяет заглушку',()=>{
  const result=rowPresentation({folder_id:1,from:addr('Аня','anna@example.com'),to:[],cc:[]},rolesOf({1:'sent'}));
  assert.deepEqual(result,{kind:'empty',text:'',extra:0,initial:'?'});
});

// S-004: обычная строка - подпись from, нормализация имени из пробелов.
test('S-004: отправитель с именем',()=>{
  const result=rowPresentation({folder_id:1,from:addr('Аня','a@example.com')},rolesOf({1:'inbox'}));
  assert.equal(result.kind,'sender');assert.equal(result.text,'Аня');
});
test('S-004: отправитель только с email',()=>{
  const result=rowPresentation({folder_id:1,from:addr('','a@example.com')},rolesOf({1:'inbox'}));
  assert.equal(result.text,'a@example.com');
});
test('S-004: имя из одних пробелов показывается по email',()=>{
  const result=rowPresentation({folder_id:1,from:addr('   ','a@example.com')},rolesOf({1:'inbox'}));
  assert.equal(result.text,'a@example.com');
});
test('S-004: пустые поля - пустая подпись',()=>{
  const result=rowPresentation({folder_id:1,from:addr('','')},rolesOf({1:'inbox'}));
  assert.equal(result.kind,'sender');assert.equal(result.text,'');
});
test('S-004: письмо самому себе - inbox показывает отправителя, sent - получателя',()=>{
  const same=addr('Аня','anna@example.com');
  const inboxResult=rowPresentation({folder_id:1,from:same,to:[same],cc:[]},rolesOf({1:'inbox'}));
  const sentResult=rowPresentation({folder_id:2,from:same,to:[same],cc:[]},rolesOf({2:'sent'}));
  assert.equal(inboxResult.kind,'sender');assert.equal(sentResult.kind,'recipient');
  assert.equal(inboxResult.text,'Аня');assert.equal(sentResult.text,'Аня');
});

// S-005: инициал соответствует стороне подписи.
test('S-005: инициал берется из подписи, эмодзи не разрезается',()=>{
  const result=rowPresentation({folder_id:1,from:addr('😀 Аня','a@example.com')},rolesOf({1:'inbox'}));
  assert.equal(result.initial,'😀');
});
test('S-005: пустая подпись дает инициал ?',()=>{
  const result=rowPresentation({folder_id:1,from:addr('','')},rolesOf({1:'inbox'}));
  assert.equal(result.initial,'?');
});
test('S-005: суффикс +N не влияет на инициал',()=>{
  const message={folder_id:1,from:addr('',''),to:[addr('Аня','a@example.com'),addr('Боря','b@example.com'),addr('Вера','v@example.com')],cc:[]};
  const result=rowPresentation(message,rolesOf({1:'sent'}));
  assert.equal(result.initial,'А');assert.equal(result.extra,2);
});

// S-006: роль каждого письма беседы применяется к его собственному folder_id.
test('S-006: представитель и дочернее письмо форматируются независимо по своей роли',()=>{
  const roles=rolesOf({1:'inbox',2:'sent'});
  const inboxChild=rowPresentation({folder_id:1,from:addr('Аня','a@example.com'),to:[addr('Я','me@example.com')],cc:[]},roles);
  const sentChild=rowPresentation({folder_id:2,from:addr('Я','me@example.com'),to:[addr('Боря','b@example.com')],cc:[]},roles);
  assert.equal(inboxChild.kind,'sender');assert.equal(inboxChild.text,'Аня');
  assert.equal(sentChild.kind,'recipient');assert.equal(sentChild.text,'Боря');
});

// S-008: форматтер адресной строки шапки - подпись и полная подсказка.
test('S-008: displayName и подсказка сохраняют порядок и не удаляют дубли',()=>{
  const list=[addr('','a@example.com'),addr('Аня','a@example.com'),addr('Аня','b@example.com')];
  const model=addressLineModel(list,true,2);
  assert.deepEqual(model.shown.map(item=>item.text),['a@example.com','Аня','Аня']);
  assert.deepEqual(model.shown.map(item=>item.title),['a@example.com','Аня (a@example.com)','Аня (b@example.com)']);
});
test('displayName: подпись адреса, name после trim, иначе email, иначе пусто',()=>{
  assert.equal(displayName(addr('Аня','a@example.com')),'Аня');
  assert.equal(displayName(addr('','a@example.com')),'a@example.com');
  assert.equal(displayName(addr('   ','a@example.com')),'a@example.com');
  assert.equal(displayName(addr('','')),'');
  assert.equal(displayName(undefined),'');
});

// S-009: модель свертки строк "Кому"/"Копия" - сколько показано, сколько скрыто.
test('S-009: 1 и 2 адреса - показаны все, счетчика нет',()=>{
  assert.equal(addressLineModel([addr('Аня','a@example.com')],false,2).hidden,0);
  assert.equal(addressLineModel([addr('Аня','a@example.com'),addr('Боря','b@example.com')],false,2).hidden,0);
});
test('S-009: 3 и 7 адресов в свернутом виде - первые два и счетчик скрытых',()=>{
  const many=n=>Array.from({length:n},(_,i)=>addr(`Имя${i}`,`a${i}@example.com`));
  const three=addressLineModel(many(3),false,2);
  assert.equal(three.shown.length,2);assert.equal(three.hidden,1);
  const seven=addressLineModel(many(7),false,2);
  assert.equal(seven.shown.length,2);assert.equal(seven.hidden,5);
});
test('S-009: maxShown задает границу свертки, а не жесткая двойка',()=>{
  const many=Array.from({length:5},(_,i)=>addr(`Имя${i}`,`a${i}@example.com`));
  const model=addressLineModel(many,false,3);
  assert.equal(model.shown.length,3);assert.equal(model.hidden,2);
});
test('S-009: раскрытое состояние показывает все адреса без счетчика',()=>{
  const many=Array.from({length:7},(_,i)=>addr(`Имя${i}`,`a${i}@example.com`));
  const model=addressLineModel(many,true,2);
  assert.equal(model.shown.length,7);assert.equal(model.hidden,0);
});
test('S-009: адрес с именем из пробелов показывается по email в модели свертки',()=>{
  const model=addressLineModel([addr('   ','a@example.com')],false,2);
  assert.equal(model.shown[0].text,'a@example.com');
});
test('S-009: пустой вход дает null',()=>{
  assert.equal(addressLineModel([],false,2),null);
  assert.equal(addressLineModel(undefined,false,2),null);
  assert.equal(addressLineModel([addr('',''),addr('  ','')],false,2),null);
});
