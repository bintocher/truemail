// Проверки чистой логики apps/desktop/ui/modules/connect-progress.js.
// Запуск: node --test apps/desktop/tests/js/connect-progress.test.js (Node 20+).
'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const {CONNECT_STAGES,connectStageMessageKey,isConnectAttemptCurrent,connectErrorText}=require('../../ui/modules/connect-progress.js');

// S-003: только пять подтверждённых этапов, без выдуманного процента.
test('S-003: набор этапов - ровно пять подтверждённых состояний',()=>{
  assert.deepEqual(CONNECT_STAGES,['detecting','waiting_code','checking_server','saving','connected']);
});
test('S-003: у каждого этапа есть ключ локализации',()=>{
  for(const stage of CONNECT_STAGES)assert.equal(typeof connectStageMessageKey(stage),'string');
});
test('S-003: неизвестный этап не подставляет текст',()=>{
  assert.equal(connectStageMessageKey('percent-42'),null);
});

// S-016: поздний ответ прежней попытки не должен применяться.
test('S-016: номер попытки совпадает и экран открыт - применяем',()=>{
  assert.equal(isConnectAttemptCurrent(3,3,true),true);
});
test('S-016: номер попытки устарел - не применяем',()=>{
  assert.equal(isConnectAttemptCurrent(2,3,true),false);
});
test('S-016: номер совпадает, но экран уже закрыт - не применяем (S-011)',()=>{
  assert.equal(isConnectAttemptCurrent(3,3,false),false);
});

// S-006: текст истечения общего предела времени - отдельный, конкретный.
test('S-006: вид ошибки timeout - подставляется текст про подключение',()=>{
  assert.equal(connectErrorText('timeout','Сервер не ответил вовремя.','Подключение заняло слишком много времени.'),'Подключение заняло слишком много времени.');
});
test('прочие виды ошибок - общий текст без изменений',()=>{
  assert.equal(connectErrorText('invalid_credentials','Не удалось войти.','Подключение заняло слишком много времени.'),'Не удалось войти.');
});
