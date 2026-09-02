// truemail UI module: message-view.js
// Чистые функции без DOM и Tauri API: актуальность поколения показа письма,
// положение списка по якорю и опорная позиция выделения. Подключается в
// index.html перед mail.js как обычный скрипт и отдаёт функции через один
// глобальный объект. См. docs/specs/message-click-focus.md.

// Результат ожидания применяем, только если поколение показа не менялось
// (S-001..S-004). Нечисловое поколение считаем чужим: лучше отбросить ответ,
// чем нарисовать письмо поверх чужого.
function isCurrentView(generation,current){return Number.isFinite(generation)&&Number.isFinite(current)&&generation===current;}

// Якорь списка по положению прокрутки: id верхнего видимого письма и смещение
// внутри его строки (S-009). Тот же расчёт нужен якорю скрытого окна - хранят
// они его по-разному, арифметика одна.
function listAnchorAt(rows,scrollTop,rowHeight){
  const list=rows||[],height=rowHeight>0?rowHeight:0,top=Math.max(0,Number(scrollTop)||0);
  if(!list.length||!height)return {id:null,offset:0};
  const index=Math.min(Math.floor(top/height),list.length-1);
  return {id:list[index]?.id??null,offset:Math.max(0,top-index*height)};
}

// Положение списка после перестройки: строка якоря встаёт туда же, где стояла,
// с тем же смещением внутри строки. Смещение выводим из прежней позиции, а не
// храним отдельно: якорь снят как верхняя видимая строка, значит внутристрочный
// остаток - это previousOffset % rowHeight. Якоря нет или письмо ушло из списка -
// возвращаем прежнюю позицию, то есть прежнее поведение по пикселям.
function listAnchorOffset(rows,anchorId,previousOffset,rowHeight){
  const previous=Math.max(0,Number(previousOffset)||0),height=rowHeight>0?rowHeight:0;
  if(anchorId==null||!height)return previous;
  const index=(rows||[]).findIndex(row=>row?.id===anchorId);
  if(index<0)return previous;
  return index*height+previous%height;
}

// Опорная позиция выделения с Shift (S-010): якорь хранится по id письма и
// пересчитывается в номер строки того списка, который сейчас видит пользователь.
// Письма нет в списке (ушло из выборки, скрыто в свёрнутой беседе) - опорной
// становится строка, по которой кликнули.
function selectionAnchorIndex(rows,anchorId,fallbackIndex){
  if(anchorId==null)return fallbackIndex;
  const index=(rows||[]).findIndex(row=>row?.id===anchorId);
  return index<0?fallbackIndex:index;
}

const messageView={isCurrentView,listAnchorAt,listAnchorOffset,selectionAnchorIndex};
if(typeof module!=='undefined'&&module.exports)module.exports=messageView;
