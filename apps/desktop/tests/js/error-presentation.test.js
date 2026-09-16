// Проверки выбора текста, действия и очереди ошибок.
// Спецификация: specs/error-kinds-and-messages.md.
'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const {
  ERROR_KINDS,
  presentError,
  planToastQueue,
}=require('../../ui/modules/error-presentation.js');

const uiRoot=path.join(__dirname,'../../ui');
const translations={
  ru:JSON.parse(fs.readFileSync(path.join(uiRoot,'locales/ru.json'),'utf8')),
  en:JSON.parse(fs.readFileSync(path.join(uiRoot,'locales/en.json'),'utf8')),
};

// S-005, S-006: каждый известный вид имеет два перевода и одно действие.
test('S-005 S-006: известные виды выбирают локализованный текст и действие',()=>{
  Object.keys(ERROR_KINDS).filter(kind=>kind!=='unknown').forEach(kind=>{
    const ru=presentError({kind,message:'internal text'},{locale:'ru',translations,connected:true});
    const en=presentError({kind,message:'internal text'},{locale:'en',translations,connected:true});
    assert.notEqual(ru.text,'internal text',kind);
    assert.notEqual(en.text,'internal text',kind);
    assert.notEqual(ru.text,en.text,kind);
    assert.ok(ru.action,kind);
    assert.ok(ru.actionLabel,kind);
  });
});

test('S-006: вход выбирает действие по наличию сохраненного аккаунта',()=>{
  assert.equal(presentError({kind:'invalid_credentials',account_id:7},{translations,connected:true}).action,'reconnect');
  assert.equal(presentError({kind:'needs_reauth'},{translations,connected:false}).action,'check_and_retry');
  assert.equal(presentError({kind:'timeout'},{translations}).action,'retry');
  assert.equal(presentError({kind:'rate_limited',retry_at:'2026-09-16T20:00:00Z'},{translations}).action,'wait');
  assert.equal(presentError({kind:'account_config'},{translations}).action,'settings');
  assert.equal(presentError({kind:'crypto_error'},{translations}).action,'diagnostics');
});

test('S-007 S-011: неизвестный и старый ответ не классифицируются по тексту',()=>{
  const old=presentError({message:'invalid_grant rateLimitExceeded'},{locale:'ru',translations});
  const strange=presentError({kind:'future_kind',message:'password rejected'},{locale:'ru',translations});
  assert.equal(old.kind,'unknown');
  assert.equal(strange.kind,'unknown');
  assert.equal(old.text,translations.ru.errorUnknown);
  assert.equal(old.message,'invalid_grant rateLimitExceeded');
  assert.equal(old.action,'diagnostics');
});

test('S-016: только два вида требуют повторного входа',()=>{
  Object.keys(ERROR_KINDS).forEach(kind=>{
    assert.equal(
      presentError({kind},{translations}).requiresReauth,
      kind==='invalid_credentials'||kind==='needs_reauth',
      kind
    );
  });
});

function item(id,kind,text,accountId=1,action='retry',hasAction=false){
  return {id,kind,text,accountId,action,hasAction};
}

test('S-008: три одинаковых сообщения схлопываются и продлеваются',()=>{
  let result=planToastQueue([],item('a','timeout','Ошибка'),1000);
  result=planToastQueue(result.cards,item('b','timeout','Ошибка'),5000);
  result=planToastQueue(result.cards,item('c','timeout','Ошибка'),9000);
  assert.equal(result.cards.length,1);
  assert.equal(result.cards[0].repeatCount,3);
  assert.equal(result.cards[0].expiresAt,18000);
});

test('S-009: четыре разные ошибки оставляют три новые в исходном порядке',()=>{
  let cards=[];
  for(let index=1;index<=4;index++)cards=planToastQueue(cards,item(String(index),`kind-${index}`,`text-${index}`),index*1000).cards;
  assert.deepEqual(cards.map(card=>card.id),['2','3','4']);
});

test('S-008 S-009: вид, аккаунт, текст и действие входят в ключ повтора',()=>{
  let cards=planToastQueue([],item('a','timeout','same',1,'retry'),1000).cards;
  cards=planToastQueue(cards,item('b','network_unavailable','same',1,'retry'),2000).cards;
  cards=planToastQueue(cards,item('c','timeout','same',2,'retry'),3000).cards;
  cards=planToastQueue(cards,item('d','timeout','other',1,'retry'),4000).cards;
  cards=planToastQueue(cards,item('e','timeout','same',1,'diagnostics'),5000).cards;
  assert.equal(cards.length,3);
  assert.deepEqual(cards.map(card=>card.id),['c','d','e']);
});

test('S-015: карточка с действием не получает время автоматического скрытия',()=>{
  const result=planToastQueue([],item('a','timeout','Ошибка',1,'retry',true),1000);
  assert.equal(result.cards[0].expiresAt,null);
});
