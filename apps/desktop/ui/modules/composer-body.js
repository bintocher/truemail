// truemail UI module: composer-body.js
// Чистые функции без DOM и Tauri API: отбор элементов буфера обмена по типу,
// разбор строки data: на тип и байты, подсчёт суммарного размера письма,
// признак непустого тела, сборка тега картинки, проверка предела размера,
// распознавание файлового переноса. Подключается
// в index.html перед composer.js как обычный скрипт и отдаёт функции через
// один глобальный объект. См. specs/composer-image-paste-and-file-drop.md.

// Поддерживаемые типы встроенных картинок (S-004, S-035): svg+xml намеренно
// не входит - это разметка, которая может содержать исполняемый код.
const SUPPORTED_IMAGE_TYPES=['image/png','image/jpeg','image/gif','image/webp','image/bmp'];
// Предел суммарного размера письма - вложения и байты встроенных картинок
// вместе (S-007, S-018, S-038): 25 МБ ровно в байтах.
const MAX_MESSAGE_BYTES=25*1024*1024;

function isSupportedImageType(type){
  return SUPPORTED_IMAGE_TYPES.includes(String(type||'').toLowerCase());
}

// Распознаёт файловый перенос по перечню типов данных переноса (S-013, S-014,
// S-023): у файлового там есть 'Files'. Внутренние переносы (письмо, событие
// календаря, строка настроек) этого типа не содержат.
function isFileTransfer(types){
  return Array.from(types||[]).includes('Files');
}

// Отбор элементов буфера обмена по типу (S-001, S-003, S-004): возвращает
// файловые элементы поддерживаемых типов картинок в порядке следования в
// буфере и отдельно - неподдерживаемые типы картинок (для сообщения об
// отказе), без дублей. Текстовые и разметочные элементы того же буфера в
// результат не попадают - вставлять их не нужно (S-001).
function clipboardImageItems(items){
  const list=Array.from(items||[]).filter(item=>item&&item.kind==='file');
  const images=list.filter(item=>isSupportedImageType(item.type));
  const rejectedTypes=[...new Set(list.filter(item=>/^image\//i.test(item.type||'')&&!isSupportedImageType(item.type)).map(item=>item.type))];
  return {images,rejectedTypes};
}

// Разбор строки data: (S-008, S-028, S-035, S-041 по слову data): возвращает
// {mimeType,byteLength} для поддерживаемого типа с разбираемыми base64
// данными, иначе null. byteLength - количество двоичных байтов после
// раскодирования base64, а не длина текстовой записи (см. термин "байты
// картинки" в спецификации).
function parseDataUrl(value){
  const text=String(value||'');
  if(!/^data:/i.test(text))return null;
  const rest=text.slice(5);
  const comma=rest.indexOf(',');
  if(comma===-1)return null;
  const segments=rest.slice(0,comma).split(';').map(part=>part.trim());
  if(!segments.some(part=>part.toLowerCase()==='base64'))return null;
  const mimeType=(segments.find(part=>part&&part.toLowerCase()!=='base64')||'').toLowerCase();
  if(!isSupportedImageType(mimeType))return null;
  try{
    const bytes=atob(rest.slice(comma+1).replace(/\s+/g,''));
    return {mimeType,byteLength:bytes.length};
  }catch(error){
    return null;
  }
}

// Сборка тега картинки (S-001, S-003): строка data: с типом и данными
// base64, которую вызывающая сторона уже проверила по SUPPORTED_IMAGE_TYPES.
function buildImageTag(mimeType,base64Data){
  return `<img src="data:${mimeType};base64,${base64Data}">`;
}

// Признак наличия тега img в разметке тела письма (S-012): не зависит от
// регистра и атрибутов тега, ищет только сам факт открывающего тега.
function htmlHasImageTag(html){
  return /<img[\s/>]/i.test(String(html||''));
}

// Суммарный размер письма (S-008): сумма байтов вложений и байтов картинок
// поддерживаемых типов с разбираемыми данными, найденных в перечне ссылок
// img.src тела письма в этот момент. Строка data: неподдерживаемого типа или
// с неразбираемыми данными не учитывается - как и при сборке письма в ядре.
// Одинаковые картинки считаются один раз: при сборке письма ядро выносит их
// одной частью (S-032), и подсчёт с повторами дал бы отказ по пределу там,
// где письмо на самом деле помещается.
function totalMessageBytes(attachmentSizes,imgSources){
  const attachmentsTotal=(attachmentSizes||[]).reduce((sum,size)=>sum+(Number(size)||0),0);
  const seen=new Set();
  let imagesTotal=0;
  for(const src of imgSources||[]){
    if(seen.has(src))continue;
    const parsed=parseDataUrl(src);
    if(!parsed)continue;
    seen.add(src);imagesTotal+=parsed.byteLength;
  }
  return attachmentsTotal+imagesTotal;
}

// Уложится ли дополнительный размер в предел письма (S-007, S-018, S-038) -
// граница включительная: ровно предел допустим, предел плюс один байт - нет.
function fitsMessageLimit(currentTotalBytes,additionalBytes,maxBytes=MAX_MESSAGE_BYTES){
  return (Number(currentTotalBytes)||0)+(Number(additionalBytes)||0)<=maxBytes;
}

const composerBody={SUPPORTED_IMAGE_TYPES,MAX_MESSAGE_BYTES,isSupportedImageType,isFileTransfer,clipboardImageItems,parseDataUrl,buildImageTag,htmlHasImageTag,totalMessageBytes,fitsMessageLimit};
if(typeof module!=='undefined'&&module.exports)module.exports=composerBody;
