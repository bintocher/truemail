// Проверки чистой логики apps/desktop/ui/modules/wizard-exit.js.
// Запуск: node --test apps/desktop/tests/js/wizard-exit.test.js (Node 20+).
// Каталог вне apps/desktop/ui, поэтому в дистрибутив Tauri не попадает.
'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const {wizardExitAllowed,wizardEscapeAction}=require('../../ui/modules/wizard-exit.js');

// S-001, S-007: выход разрешён только по сохранённому признаку 'true'.
test('S-001: сохранённая настройка завершена - выход разрешён',()=>{
  assert.equal(wizardExitAllowed('true'),true);
});
// S-002: пустое, ложное или нечитаемое значение - выхода нет.
test('S-002: настройка не завершена - выхода нет',()=>{
  assert.equal(wizardExitAllowed('false'),false);
  assert.equal(wizardExitAllowed(''),false);
  assert.equal(wizardExitAllowed(null),false);
  assert.equal(wizardExitAllowed(undefined),false);
});
// S-007: расхождение значения JavaScript и сохранённой настройки - решает
// только сохранённое значение, переданное вызывающим кодом.
test('S-007: решение принимается по переданному сохранённому значению, а не по строке "1" или true (boolean)',()=>{
  assert.equal(wizardExitAllowed('1'),false);
  assert.equal(wizardExitAllowed(true),false);
});

// S-006, S-011: порядок обработки Escape - меню, затем оверлеи, затем мастер.
test('S-011: ничего не открыто - действия нет',()=>{
  assert.equal(wizardEscapeAction({}),'none');
});
test('S-004: только мастер открыт и выход разрешён - закрывается мастер',()=>{
  assert.equal(wizardEscapeAction({wizardOpen:true,wizardExitAvailable:true}),'wizard');
});
test('S-005: мастер открыт, но выход не разрешён (обязательный) - действия нет',()=>{
  assert.equal(wizardEscapeAction({wizardOpen:true,wizardExitAvailable:false}),'none');
});
test('S-006: открыто меню поверх мастера - первым закрывается меню',()=>{
  assert.equal(wizardEscapeAction({popupMenuOpen:true,overlayOpen:true,wizardOpen:true,wizardExitAvailable:true}),'popup');
});
test('S-006: открыто модальное окно поверх мастера - первым закрывается окно',()=>{
  assert.equal(wizardEscapeAction({overlayOpen:true,wizardOpen:true,wizardExitAvailable:true}),'overlay');
});
test('S-006: второе нажатие после закрытия окна - очередь доходит до мастера',()=>{
  // Первое нажатие: overlayOpen=true -> 'overlay'. Второе нажатие - оверлей уже
  // закрыт вызывающим кодом, поэтому overlayOpen=false.
  assert.equal(wizardEscapeAction({overlayOpen:false,wizardOpen:true,wizardExitAvailable:true}),'wizard');
});
test('меню имеет приоритет даже без открытого мастера',()=>{
  assert.equal(wizardEscapeAction({popupMenuOpen:true,wizardOpen:false,wizardExitAvailable:false}),'popup');
});
