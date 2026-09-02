// Проверки чистой логики apps/desktop/ui/modules/message-view.js.
// Запуск: node --test apps/desktop/tests/js/message-view.test.js (Node 20+).
// Каталог вне apps/desktop/ui, поэтому в дистрибутив Tauri не попадает.
'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const {isCurrentView,listAnchorAt,listAnchorOffset,selectionAnchorIndex}=require('../../ui/modules/message-view.js');

const rowsOf=(...ids)=>ids.map(id=>({id}));

// S-001: результат ожидания применяется, только пока поколение показа то же.
test('S-001: поколение не менялось - результат применяется',()=>{
  assert.equal(isCurrentView(3,3),true);
  assert.equal(isCurrentView(0,0),true);
});
test('S-001: поколение выросло - результат первого запроса отбрасывается',()=>{
  assert.equal(isCurrentView(1,2),false);
  assert.equal(isCurrentView(1,5),false); // подряд несколько кликов
});
// Поколение только растёт, но меньшее текущего тоже не своё: применять чужой
// результат нельзя ни в какую сторону.
test('S-001: поколение меньше запрошенного - тоже не свое',()=>{
  assert.equal(isCurrentView(4,2),false);
});
test('S-001: неизвестное поколение считается чужим',()=>{
  assert.equal(isCurrentView(undefined,1),false);
  assert.equal(isCurrentView(1,undefined),false);
  assert.equal(isCurrentView(null,0),false);
  assert.equal(isCurrentView(Number.NaN,Number.NaN),false);
});
// S-004: ветка ошибки пользуется той же проверкой - сообщение об ошибке
// устаревшего запроса на экран не идет.
test('S-004: ошибка устаревшего запроса не проходит проверку, ошибка актуального проходит',()=>{
  assert.equal(isCurrentView(7,8),false);
  assert.equal(isCurrentView(8,8),true);
});

// S-009: снятие якоря - верхнее видимое письмо и смещение внутри его строки.
test('S-009: якорь - верхняя видимая строка, смещение внутри нее',()=>{
  const rows=rowsOf(10,11,12,13);
  assert.deepEqual(listAnchorAt(rows,0,76),{id:10,offset:0});
  assert.deepEqual(listAnchorAt(rows,76,76),{id:11,offset:0});
  assert.deepEqual(listAnchorAt(rows,90,76),{id:11,offset:14});
});
test('S-009: положение ниже последней строки дает последнее письмо',()=>{
  assert.deepEqual(listAnchorAt(rowsOf(10,11),100000,76).id,11);
});
test('S-009: пустой список и нулевая высота строки якоря не дают',()=>{
  assert.deepEqual(listAnchorAt([],120,76),{id:null,offset:0});
  assert.deepEqual(listAnchorAt(undefined,120,76),{id:null,offset:0});
  assert.deepEqual(listAnchorAt(rowsOf(10),120,0),{id:null,offset:0});
});
test('S-009: отрицательное положение прокрутки дает первое письмо',()=>{
  assert.deepEqual(listAnchorAt(rowsOf(10,11),-40,76),{id:10,offset:0});
});

// S-009: восстановление - строка якоря встает на прежнее место, даже если
// сверху вставились новые письма.
test('S-009: два письма сверху не сдвигают строку под курсором',()=>{
  const before=rowsOf(10,11,12),after=rowsOf(8,9,10,11,12);
  const anchor=listAnchorAt(before,76,76); // верхнее видимое - письмо 11
  assert.equal(anchor.id,11);
  assert.equal(listAnchorOffset(after,anchor.id,76,76),3*76); // 11 стало четвертым
});
test('S-009: смещение внутри строки сохраняется',()=>{
  const after=rowsOf(8,9,10,11);
  assert.equal(listAnchorOffset(after,11,90,76),3*76+14);
});
test('S-009: якоря нет или письмо ушло из списка - прежнее положение по пикселям',()=>{
  assert.equal(listAnchorOffset(rowsOf(10,11),null,120,76),120);
  assert.equal(listAnchorOffset(rowsOf(10,11),99,120,76),120);
  assert.equal(listAnchorOffset([],11,120,76),120);
  assert.equal(listAnchorOffset(rowsOf(10,11),11,120,0),120);
});
test('S-009: якорь на первой строке возвращает то же положение',()=>{
  assert.equal(listAnchorOffset(rowsOf(10,11,12),10,0,76),0);
});
test('S-009: испорченное прежнее положение не дает отрицательного результата',()=>{
  assert.equal(listAnchorOffset(rowsOf(10,11),10,-500,76),0);
  assert.equal(listAnchorOffset(rowsOf(10,11),10,undefined,76),0);
});

// S-010: опорная позиция выделения с Shift пересчитывается из id в номер строки.
test('S-010: опорное письмо на месте - его номер в текущем списке',()=>{
  assert.equal(selectionAnchorIndex(rowsOf(10,11,12),12,-1),2);
});
test('S-010: перестройка списка сдвинула строки - номер пересчитан',()=>{
  assert.equal(selectionAnchorIndex(rowsOf(8,9,10,11,12),11,-1),3);
});
test('S-010: опорного письма в списке нет - берется запасная позиция',()=>{
  assert.equal(selectionAnchorIndex(rowsOf(10,11),99,4),4); // письмо ушло из выборки
  assert.equal(selectionAnchorIndex(rowsOf(10,11),null,4),4); // якоря не было вовсе
  assert.equal(selectionAnchorIndex([],10,4),4);
  assert.equal(selectionAnchorIndex(undefined,10,4),4);
});
// Свернутая беседа: письма-дети из строк списка исчезают, и опорным должно
// стать то письмо, по которому кликнули, а не невидимая строка.
test('S-010: опорное письмо скрыто в свернутой беседе - запасная позиция',()=>{
  const expanded=rowsOf(10,11,12),collapsed=rowsOf(10);
  assert.equal(selectionAnchorIndex(expanded,12,0),2);
  assert.equal(selectionAnchorIndex(collapsed,12,0),0);
});
