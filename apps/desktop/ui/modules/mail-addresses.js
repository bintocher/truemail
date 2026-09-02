// truemail UI module: mail-addresses.js
// Чистые функции без DOM и Tauri API: сторона строки списка, отбор
// отображаемых адресов, формат "первый +N", инициал аватара и модель
// свёртки строк "Кому"/"Копия". Подключается в index.html перед mail.js
// как обычный скрипт и отдаёт функции через один глобальный объект.
// См. docs/specs/sent-recipient-display.md.

// Отображаемый адрес - после trim непусто хотя бы одно из полей name/email.
function isDisplayableAddr(addr){if(!addr)return false;return !!((addr.name||'').trim()||(addr.email||'').trim());}
// Подпись адреса: name после trim, иначе email после trim, иначе ''.
function displayName(addr){if(!addr)return '';const name=(addr.name||'').trim();if(name)return name;return (addr.email||'').trim();}
// Полный вид для подсказки: "Имя (email)", а если чего-то нет - то, что есть.
function addressTitle(addr){const name=(addr.name||'').trim(),email=(addr.email||'').trim();return name&&email?`${name} (${email})`:(name||email);}
// Инициал - первая кодовая точка подписи в верхнем регистре; '?' при пустой
// подписи. Через [...text] суррогатная пара (например эмодзи) не режется.
function initialOf(text){if(!text)return '?';return [...text][0].toUpperCase();}

// Роль папки письма и сторона строки списка (S-001, S-002, S-003, S-004, S-005).
// folderRoles - Map folder_id -> role; роль ищется внутри функции, чтобы
// проверка покрывала и сам поиск, а не только форматирование.
function rowPresentation(message,folderRoles){
  const role=folderRoles?.get(message.folder_id);
  if(role==='sent'||role==='drafts'){
    const to=(message.to||[]).filter(isDisplayableAddr);
    // Пусто в to - пробуем cc: письмо только в копию не должно выглядеть
    // как письмо без получателя.
    const list=to.length?to:(message.cc||[]).filter(isDisplayableAddr);
    if(!list.length)return {kind:'empty',text:'',extra:0,initial:'?'};
    const text=displayName(list[0]);
    return {kind:'recipient',text,extra:list.length-1,initial:initialOf(text)};
  }
  const text=isDisplayableAddr(message.from)?displayName(message.from):'';
  return {kind:'sender',text,extra:0,initial:initialOf(text)};
}

// Модель свёртки адресной строки шапки (S-008, S-009): null при пустом
// входе, иначе {shown:[{text,title}], hidden}. expanded=true - показать всё.
function addressLineModel(addresses,expanded,maxShown){
  const list=(addresses||[]).filter(isDisplayableAddr);
  if(!list.length)return null;
  const cut=!expanded&&list.length>maxShown?list.slice(0,maxShown):list;
  // S-008: в тексте строки "Имя (email)", как и в подсказке - тот же полный вид.
  return {shown:cut.map(addr=>({text:addressTitle(addr),title:addressTitle(addr)})),hidden:cut.length<list.length?list.length-cut.length:0};
}

const mailAddresses={rowPresentation,addressLineModel,displayName};
if(typeof module!=='undefined'&&module.exports)module.exports=mailAddresses;
