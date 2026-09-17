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
  formatErrorDetails,
  shouldShowSyncToast,
  planToastQueue,
  beginToastAction,
  finishToastAction,
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
  // F11: вид остаётся unknown (S-011 - никакого разбора текста ради вида), но
  // раз собственный текст ошибки есть и не пуст, он и показывается - иначе
  // локальные причины (проверка полей, отказ второй попытки подключения,
  // отказ файловой операции) терялись бы за общим текстом.
  assert.equal(old.text,'invalid_grant rateLimitExceeded');
  assert.equal(old.message,'invalid_grant rateLimitExceeded');
  assert.equal(old.action,'diagnostics');
});

test('F11: unknown без текста показывает общий текст, unknown с текстом - свой',()=>{
  const withText=presentError({kind:'unknown',message:'укажите имя пользователя и IMAP-сервер'},{locale:'ru',translations});
  assert.equal(withText.text,'укажите имя пользователя и IMAP-сервер');
  const withoutText=presentError({kind:'unknown',message:''},{locale:'ru',translations});
  assert.equal(withoutText.text,translations.ru.errorUnknown);
  const noMessageAtAll=presentError({kind:'unknown'},{locale:'ru',translations});
  assert.equal(noMessageAtAll.text,translations.ru.errorUnknown);
  const blankMessage=presentError({kind:'unknown',message:'   '},{locale:'ru',translations});
  assert.equal(blankMessage.text,translations.ru.errorUnknown);
});

test('F11: у известного вида текст всегда по таблице, а не собственный',()=>{
  const known=presentError({kind:'timeout',message:'какой-то внутренний текст'},{locale:'ru',translations});
  assert.equal(known.text,translations.ru.errorTimeout);
  assert.notEqual(known.text,'какой-то внутренний текст');
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

test('S-017: основной текст называет нужный аккаунт',()=>{
  const account={id:2,email:'work@example.com',display_name:'Работа'};
  const shown=presentError(
    {kind:'server_unavailable',account_id:2,message:'HTTP 500'},
    {locale:'ru',translations,account},
  );
  assert.match(shown.text,/Работа \(work@example\.com\)/);
  assert.doesNotMatch(shown.text,/HTTP 500/);
  const unnamed=presentError(
    {kind:'timeout',account_id:3,message:'timeout'},
    {locale:'ru',translations,account:{id:3,email:'home@example.com',display_name:''}},
  );
  assert.match(unnamed.text,/^home@example\.com:/);
});

test('S-018: предупреждение использует общую таблицу, транспорт остаётся в подробностях',()=>{
  const warning={kind:'server_unavailable',message:'транспорт (ews-http): HTTP 500'};
  const shown=presentError(warning,{locale:'ru',translations});
  assert.equal(shown.text,translations.ru.errorServerUnavailable);
  assert.match(shown.details,/HTTP 500/);
});

test('S-019: подробности содержат сервер, код, местное время и транспорт',()=>{
  const details=formatErrorDetails({
    kind:'server_unavailable',
    message:'transport failed',
    server:'mail.example.com:443',
    response_code:503,
    attempted_at:'2026-09-17T10:20:30Z',
  },{locale:'ru',translations});
  assert.match(details,/Сервер: mail\.example\.com:443/);
  assert.match(details,/Код ответа: 503/);
  assert.match(details,/Время попытки:/);
  assert.match(details,/Причина транспорта: transport failed/);
  assert.doesNotMatch(details,/2026-09-17T10:20:30Z/);
});

test('S-020: действие меняет ту же карточку на ожидание и исход',()=>{
  const original=[{id:'a',text:'Ошибка',hasAction:true,expiresAt:null}];
  const pending=beginToastAction(original,'a','Выполняется...');
  assert.equal(pending[0].id,'a');
  assert.equal(pending[0].actionState,'pending');
  assert.equal(pending[0].actionStatus,'Выполняется...');
  const success=finishToastAction(pending,'a',{ok:true,text:'Действие выполнено.'},1000);
  assert.equal(success[0].id,'a');
  assert.equal(success[0].text,'Действие выполнено.');
  assert.equal(success[0].hasAction,false);
  const failed=finishToastAction(pending,'a',{ok:false,item:{text:'Новая причина',hasAction:true}},1000);
  assert.equal(failed[0].id,'a');
  assert.equal(failed[0].text,'Новая причина');
  assert.equal(failed[0].actionState,'failed');
});

test('S-021: первый и очередной проход имеют разные формулировки',()=>{
  const first=presentError({kind:'timeout',message:'raw',sync_phase:'initial'},{locale:'ru',translations});
  const regular=presentError({kind:'timeout',message:'raw',sync_phase:'regular'},{locale:'ru',translations});
  assert.match(first.text,/Первая синхронизация отложена/);
  assert.match(regular.text,/Очередная синхронизация отложена/);
  assert.doesNotMatch(regular.text,/Первая/);
});

test('S-022: временный сбой до исчерпания повторов не всплывает',()=>{
  for(const kind of ['network_unavailable','timeout','server_unavailable','rate_limited']){
    assert.equal(shouldShowSyncToast({kind,retries_exhausted:false}),false,kind);
  }
});

test('S-023: один вид объединяет аккаунты в одной карточке',()=>{
  const first={...item('a','server_unavailable','one',1),groupByKind:true,baseText:'Сервер недоступен',accounts:[{id:1,email:'one@example.com'}],locale:'ru',translations};
  const second={...item('b','server_unavailable','two',2),groupByKind:true,baseText:'Сервер недоступен',accounts:[{id:2,email:'two@example.com'}],locale:'ru',translations};
  let cards=planToastQueue([],first,1000).cards;
  cards=planToastQueue(cards,second,2000).cards;
  assert.equal(cards.length,1);
  assert.deepEqual(cards[0].accountIds,[1,2]);
  assert.match(cards[0].text,/one@example\.com/);
  assert.match(cards[0].text,/two@example\.com/);
});

test('G3: повторный сбой того же аккаунта не дублирует обработчик действия',()=>{
  const callbackA1=()=>{};
  const first={...item('a','server_unavailable','one',1),groupByKind:true,baseText:'Сервер недоступен',accounts:[{id:1,email:'one@example.com'}],locale:'ru',translations,callback:callbackA1};
  const callbackA2=()=>{};
  const repeat={...item('b','server_unavailable','one-again',1),groupByKind:true,baseText:'Сервер недоступен',accounts:[{id:1,email:'one@example.com'}],locale:'ru',translations,callback:callbackA2};
  let cards=planToastQueue([],first,1000).cards;
  cards=planToastQueue(cards,repeat,2000).cards;
  assert.equal(cards.length,1);
  // Аккаунт уже учтён в карточке - второй обработчик того же аккаунта не
  // добавляется, иначе одно нажатие запускало бы действие дважды.
  assert.deepEqual(cards[0].callbacks,[callbackA1]);
});

test('G4: объединение карточки очищает подробности от первого аккаунта',()=>{
  const first={...item('a','server_unavailable','one',1),groupByKind:true,baseText:'Сервер недоступен',accounts:[{id:1,email:'one@example.com'}],locale:'ru',translations,details:'Код ответа: 503'};
  const second={...item('b','server_unavailable','two',2),groupByKind:true,baseText:'Сервер недоступен',accounts:[{id:2,email:'two@example.com'}],locale:'ru',translations,details:'Код ответа: 500'};
  let cards=planToastQueue([],first,1000).cards;
  assert.equal(cards[0].details,'Код ответа: 503');
  cards=planToastQueue(cards,second,2000).cards;
  // Подробности были только от первого аккаунта - при объединении общие
  // подробности не показываем, а не оставляем чужие для второго аккаунта.
  assert.equal(cards[0].details,'');
});

test('S-024: сразу всплывают только виды, требующие решения человека',()=>{
  for(const kind of ['invalid_credentials','needs_reauth','forbidden','certificate_error','account_config']){
    assert.equal(shouldShowSyncToast({kind,retries_exhausted:false}),true,kind);
  }
  assert.equal(shouldShowSyncToast({kind:'storage_error',retries_exhausted:true}),false);
  assert.equal(shouldShowSyncToast({kind:'server_unavailable',retries_exhausted:true}),true);
});
