// truemail UI module: mail.js
function formatBytes(bytes){if(!Number.isFinite(+bytes)||bytes<=0)return '0 Б';const units=['Б','КБ','МБ','ГБ','ТБ'];let value=+bytes,index=0;while(value>=1024&&index<units.length-1){value/=1024;index++;}return `${value>=10||index===0?value.toFixed(0):value.toFixed(1)} ${units[index]}`;}
function folderIcon(folder){return folder.role==='sent'?'send':folder.role==='drafts'?'draft':folder.role==='trash'?'trash':folder.role==='archive'?'archive':folder.role==='spam'?'spam':'inbox';}
// Окно "Исходный текст письма": raw MIME, копирование, закрытие.
async function openRawViewer(messageId){
  if(messageId==null)return;
  const overlay=document.createElement('div');overlay.className='raw-overlay';
  overlay.innerHTML=`<div class="raw-box"><div class="raw-head"><button class="btn raw-back">← ${L('Назад','Back')}</button><span class="raw-title">${L('Исходный текст письма','Message source')}</span><button class="btn raw-eml">${L('Сохранить .eml','Save .eml')}</button><button class="btn primary raw-copy">${L('Копировать','Copy')}</button></div><textarea class="raw-text" readonly spellcheck="false"></textarea></div>`;
  document.body.appendChild(overlay);
  const ta=overlay.querySelector('.raw-text');ta.value=L('Загрузка…','Loading…');
  try{ta.value=await window.tm.messageRaw(messageId);}catch(error){ta.value=error.message||String(error);}
  function close(){overlay.remove();document.removeEventListener('keydown',key);}
  function key(e){if(e.key==='Escape')close();}
  overlay.querySelector('.raw-back').onclick=close;
  overlay.querySelector('.raw-eml').onclick=()=>saveMessageAsEml(messageId);
  overlay.querySelector('.raw-copy').onclick=async()=>{
    try{await navigator.clipboard.writeText(ta.value);}catch{ta.select();document.execCommand('copy');}
    showToast(L('Исходный текст скопирован','Source copied'));
  };
  overlay.onclick=e=>{if(e.target===overlay)close();};
  document.addEventListener('keydown',key);
}
// Сохранить письмо на диск как .eml: raw MIME через готовую команду ядра.
async function saveMessageAsEml(messageId){
  if(messageId==null)return;
  const message=(activeMessage&&activeMessage.id===messageId)?activeMessage:messages.find(item=>item.id===messageId);
  const base=String(message?.subject||'message').replace(/[\\/:*?"<>|\r\n\t]+/g,'_').trim().slice(0,80)||'message';
  try{
    const path=await window.tm.saveFileDialog(`${base}.eml`);
    if(!path)return;
    await window.tm.exportMessageEml(messageId,path);
    showToast(L('Письмо сохранено как .eml','Message saved as .eml'));
  }catch(error){showToast(error.message||String(error));}
}
const isImageAttachment=att=>String(att.mime_type||'').toLowerCase().startsWith('image/');
// Компактная панель вложений над телом: 1 строка плашек, "ещё +N" с разворотом.
function buildAttachmentBar(full,messageId){
  const bar=document.createElement('div');bar.className='att-bar collapsed';
  const list=document.createElement('div');list.className='att-list';
  full.attachments.forEach(att=>{
    const chip=document.createElement('button');chip.type='button';chip.className='att-chip';
    chip.title=[att.filename,att.mime_type,formatBytes(att.size)].filter(Boolean).join(' · ');
    chip.innerHTML=`<i data-i="${isImageAttachment(att)?'image':'paperclip'}"></i><span class="att-cname"></span><span class="att-csize"></span>`;
    chip.querySelector('.att-cname').textContent=att.filename;
    chip.querySelector('.att-csize').textContent=formatBytes(att.size);
    chip.ondblclick=()=>openAttachment(full,att,messageId);
    chip.oncontextmenu=e=>{e.preventDefault();attachmentMenu(e,full,att,messageId);};
    list.appendChild(chip);
  });
  const more=document.createElement('button');more.type='button';more.className='att-more';more.hidden=true;
  more.onclick=()=>{const collapsed=bar.classList.toggle('collapsed');more.textContent=collapsed?L(`ещё +${bar.dataset.hidden||0}`,`+${bar.dataset.hidden||0} more`):L('свернуть','collapse');};
  bar.append(list,more);renderIcons(bar);
  // После вставки в DOM считаем, сколько плашек не влезло в первую строку.
  requestAnimationFrame(()=>{
    const first=list.firstElementChild;if(!first)return;const top=first.offsetTop;
    const hidden=[...list.children].filter(c=>c.offsetTop>top+2).length;
    bar.dataset.hidden=hidden;
    if(hidden>0){more.hidden=false;more.textContent=L(`ещё +${hidden}`,`+${hidden} more`);}else{more.hidden=true;bar.classList.remove('collapsed');}
  });
  return bar;
}
function openAttachment(full,att,messageId){
  if(isImageAttachment(att))openGallery(full,att,messageId);
  else saveOneAttachment(messageId,att);
}
async function saveOneAttachment(messageId,att){
  try{const path=await window.tm.saveFileDialog(att.filename);if(!path)return;await window.tm.saveAttachment(messageId,att.id,path);showToast(L('Вложение сохранено','Attachment saved'));}
  catch(error){showToast(error.message||String(error));}
}
async function saveAllAttachments(messageId){
  try{const dir=await window.tm.chooseDir();if(!dir)return;const saved=await window.tm.saveAllAttachments(messageId,dir);showToast(L(`Сохранено вложений: ${saved.length}`,`Attachments saved: ${saved.length}`));}
  catch(error){showToast(error.message||String(error));}
}
function closeAttMenu(){document.querySelector('.att-menu')?.remove();}
function attachmentMenu(event,full,att,messageId){
  closeAttMenu();
  const menu=document.createElement('div');menu.className='att-menu';
  const items=[
    [L('Открыть','Open'),()=>openAttachment(full,att,messageId)],
    [L('Сохранить…','Save…'),()=>saveOneAttachment(messageId,att)],
    [L('Сохранить всё…','Save all…'),()=>saveAllAttachments(messageId)],
    [L('Копировать имя','Copy name'),()=>navigator.clipboard?.writeText(att.filename).catch(()=>{})],
  ];
  items.forEach(([label,fn])=>{const b=document.createElement('button');b.type='button';b.textContent=label;b.onclick=()=>{closeAttMenu();fn();};menu.appendChild(b);});
  document.body.appendChild(menu);
  const w=menu.offsetWidth,h=menu.offsetHeight;
  menu.style.left=Math.min(event.clientX,innerWidth-w-8)+'px';
  menu.style.top=Math.min(event.clientY,innerHeight-h-8)+'px';
  setTimeout(()=>document.addEventListener('click',closeAttMenu,{once:true}),0);
}
// Инлайн-галерея изображений с листанием (стрелки/клавиши).
async function openGallery(full,att,messageId){
  const images=full.attachments.filter(isImageAttachment);
  let idx=Math.max(0,images.indexOf(att));
  const overlay=document.createElement('div');overlay.className='gallery-overlay';
  overlay.innerHTML=`<button class="gallery-close" title="${L('Закрыть','Close')}">×</button><button class="gallery-nav prev" title="${L('Назад','Previous')}">‹</button><img class="gallery-img" alt=""><button class="gallery-nav next" title="${L('Вперёд','Next')}">›</button><div class="gallery-cap"></div><button class="gallery-save" title="${L('Сохранить','Save')}">${L('Сохранить','Save')}</button>`;
  document.body.appendChild(overlay);
  const img=overlay.querySelector('.gallery-img'),cap=overlay.querySelector('.gallery-cap');
  async function show(i){
    idx=(i+images.length)%images.length;const a=images[idx];
    cap.textContent=`${a.filename} · ${idx+1}/${images.length}`;img.removeAttribute('src');
    try{const c=await window.tm.attachmentContent(messageId,a.id);img.src=`data:${c.mime_type||'image/png'};base64,${c.base64}`;}
    catch(error){cap.textContent=error.message||String(error);}
  }
  function key(e){if(['ArrowLeft','ArrowUp'].includes(e.key)){e.preventDefault();show(idx-1);}else if(['ArrowRight','ArrowDown'].includes(e.key)){e.preventDefault();show(idx+1);}else if(e.key==='Escape')close();}
  function close(){overlay.remove();document.removeEventListener('keydown',key);}
  overlay.querySelector('.prev').onclick=()=>show(idx-1);
  overlay.querySelector('.next').onclick=()=>show(idx+1);
  overlay.querySelector('.gallery-close').onclick=close;
  overlay.querySelector('.gallery-save').onclick=()=>saveOneAttachment(messageId,images[idx]);
  overlay.onclick=e=>{if(e.target===overlay)close();};
  document.addEventListener('keydown',key);
  const single=images.length<2;overlay.querySelector('.prev').hidden=single;overlay.querySelector('.next').hidden=single;
  show(idx);
}
function folderTitle(folder){const names=wizardLocale==='en'?{inbox:'Inbox',sent:'Sent',drafts:'Drafts',archive:'Archive',spam:'Spam',trash:'Trash'}:{inbox:'Входящие',sent:'Отправленные',drafts:'Черновики',archive:'Архив',spam:'Спам',trash:'Удалённые'};return names[folder?.role]||folder?.display_name||folder?.remote_path||'';}
function sortedFolders(folders){const order={inbox:0,sent:1,drafts:2,archive:3,spam:4,trash:5};return [...folders].sort((a,b)=>{const ar=order[a.role]??20,br=order[b.role]??20;if(ar!==br)return ar-br;return String(a.remote_path||a.display_name||'').localeCompare(String(b.remote_path||b.display_name||''),wizardLocale||'ru',{numeric:true,sensitivity:'base'});});}
// Порядок и глубина папок. Если бэкенд отдал parent_id (Exchange) - строим
// дерево, дети под родителем. Иначе (IMAP) глубина по разделителям remote_path.
function folderTreeRows(folders){
  const byId=new Map(folders.map(folder=>[folder.id,folder]));
  const hasTree=folders.some(folder=>folder.parent_id&&byId.has(folder.parent_id));
  if(!hasTree)return sortedFolders(folders).map(folder=>({folder,depth:Math.max(0,(folder.remote_path.match(/[\/|]/g)||[]).length)}));
  const children=new Map();folders.forEach(folder=>{const parent=(folder.parent_id&&byId.has(folder.parent_id))?folder.parent_id:0;if(!children.has(parent))children.set(parent,[]);children.get(parent).push(folder);});
  const rows=[];const walk=(list,depth)=>sortedFolders(list).forEach(folder=>{rows.push({folder,depth});walk(children.get(folder.id)||[],depth+1);});walk(children.get(0)||[],0);
  return rows;
}
// Счётчик писем у папки: 'u' непрочитанные, 't' всего, 'ut' оба (непрочит/всего), 'n' ничего.
function folderCounterMode(folder){return folderCounterModes[folder?.id]||'u';}
function folderCountBadge(folder){const mode=folderCounterMode(folder),showU=mode.includes('u'),showT=mode.includes('t');if(!showU&&!showT)return '';const u=folder.unread_count||0,t=folder.total_count||0;if(showU&&showT)return `${u}/${t}`;if(showT)return `${t}`;return u>0?`${u}`:'';}
function updateFolderBadge(row,folder){if(!row)return;let badge=row.querySelector('.count');const text=folderCountBadge(folder);if(text){if(!badge){badge=document.createElement('span');badge.className='count';row.appendChild(badge);}badge.textContent=text;}else if(badge){badge.remove();}}
// Раздел тегов: список меток в навигации, клик - письма с этим тегом.
// Счётчик метки берём из базы: по загруженным в память письмам он показывал
// заниженное число, пока пользователь не прокрутит все папки.
let coreTagCounts=new Map();
function tagMessageCount(name){return coreTagCounts.has(name)?coreTagCounts.get(name):messages.reduce((total,message)=>total+((message.labels||[]).includes(name)?1:0),0);}
// Подзаголовок списка: для папки и метки - число загруженных писем, для
// сводных представлений - число подключённых ящиков. Правило одно на обычную
// отрисовку и на перерисовку после смены языка, иначе переключение языка
// меняло бы смысл подписи (ui-language-switch.md, S-009).
window.renderListSubtitle=function(rowCount){
  const sub=document.getElementById('mailAccountCount');
  if(!sub)return;
  const en=wizardLocale==='en';
  if(currentFolderId!==null||currentTagName!=null){
    const n=rowCount??currentMessageRows.length;
    const word=en?(n===1?'message':'messages'):(n%10===1&&n%100!==11?'письмо':n%10>=2&&n%10<=4&&(n%100<10||n%100>=20)?'письма':'писем');
    sub.textContent=`${n} ${word}`;
    return;
  }
  const n=coreAccounts.length;
  const word=en?(n===1?'account':'accounts'):(n%10===1&&n%100!==11?'аккаунт':n%10>=2&&n%10<=4&&(n%100<10||n%100>=20)?'аккаунта':'аккаунтов');
  sub.textContent=`${n} ${word}`;
};
// Подписи системных папок собраны по языку в момент отрисовки, поэтому после
// смены языка их надо переписать (ui-language-switch.md, S-005, S-006). Дерево
// целиком не пересобираем: пропало бы состояние раскрытия и выделение.
window.relocalizeFolderTree=function(){
  document.querySelectorAll('.folder-row[data-folder-id]').forEach(row=>{
    const folder=coreFolders.find(item=>item.id===Number(row.dataset.folderId));
    if(!folder)return;
    const name=row.querySelector('.folder-name');
    if(name)name.textContent=folderTitle(folder);
  });
  // Заголовок открытой папки собран той же функцией.
  if(currentFolderId!==null){
    const folder=coreFolders.find(item=>item.id===currentFolderId);
    const heading=document.querySelector('.listhead h2');
    // Папка могла исчезнуть (удалена на сервере, а просмотр ещё на неё
    // ссылается) - тогда ставим общий заголовок, а не оставляем прежний язык.
    if(heading)heading.textContent=folder?folderTitle(folder):messagesTitle();
  }
  // Строки списка собраны в коде: подпись ящика и подпись "без получателя"
  // остались бы на прежнем языке до следующей прокрутки.
  renderMessageWindow(true);
  // Пустое состояние области письма тоже собрано в коде (S-009). Карточку
  // "нет подключённых аккаунтов" не трогаем: её ставит и переводит мастер.
  const empty=document.querySelector('#tbody .mail-empty h2');
  if(empty&&!document.querySelector('#tbody .mail-content')&&!document.querySelector('#tbody .wz-logo')){
    empty.textContent=currentMessageRows.length?L('Выберите письмо','Select a message'):L('Писем нет','No messages');
  }
};
function renderTagsNav(){const host=document.getElementById('tagsNav');if(!host)return;host.innerHTML='';coreTags.forEach(tag=>{const row=document.createElement('button');row.type='button';row.className='navitem tag-row'+(currentTagName===tag.name?' active':'');row.dataset.tagId=tag.id;row.innerHTML='<span class="tag-dot"></span><span class="tag-name"></span><span class="count"></span>';row.querySelector('.tag-dot').style.background=tag.color||'#888';row.querySelector('.tag-name').textContent=tag.name;const count=tagMessageCount(tag.name);if(count)row.querySelector('.count').textContent=count;row.onclick=()=>filterTag(tag);host.appendChild(row);});}
function renderTagSettings(){const host=document.getElementById('tagSettingsList');if(!host)return;host.innerHTML='';if(!coreTags.length){host.innerHTML=`<div class="note-muted">${L('Меток пока нет','No tags yet')}</div>`;return;}coreTags.forEach(tag=>{const row=document.createElement('div');row.className='tag-settings-row';row.innerHTML='<span class="tag-dot"></span><span class="tag-name grow"></span><span class="count"></span><button type="button" class="btn sm tag-edit"></button>';row.querySelector('.tag-dot').style.background=tag.color||'#888';row.querySelector('.tag-name').textContent=tag.name;const count=tagMessageCount(tag.name);row.querySelector('.count').textContent=count?`${count}`:'';row.querySelector('.tag-edit').textContent=L('Изменить','Edit');row.querySelector('.tag-edit').onclick=()=>openLabelEditor(tag);host.appendChild(row);});}
async function refreshTagsNav(){try{coreTags=await window.tm.listLabels();}catch(_){coreTags=[];}
  try{coreTagCounts=new Map(await window.tm.labelMessageCounts());}catch(_){coreTagCounts=new Map();}
  renderTagsNav();renderTagSettings();}
document.getElementById('tagNew2')?.addEventListener('click',()=>openLabelCreator(null));
function filterTag(tag){window.setListLoading?.(false);clearMessageSelection();goMail();document.querySelectorAll('.navitem').forEach(item=>item.classList.remove('active'));currentTagName=tag.name;currentFolderId=null;currentSmartIndex=null;window.resetTagPaging?.(tag.name);applyListOptions(true,tag.name);renderTagsNav();window.loadNextTagPage?.();}
window.refreshTagsNav=refreshTagsNav;
function contactPhoneLabel(phone){return phone?`${phone.number||''}${phone.extension?` ${L('доб.','ext.')} ${phone.extension}`:''}`:'';}
// Одна строка адреса для поиска и подписи карточки: пустые компоненты просто
// выпадают, чтобы не оставалось висящих запятых.
function contactAddressLabel(address){return address?[address.street,address.city,address.region,address.postal_code,address.country].filter(Boolean).join(', '):'';}
// Ключи транслитерации раздела контактов (S-012 person-search-translit.md):
// имя, адреса, телефоны и почтовые адреса - свой состав полей, свой кэш.
// Сброс - вместе с coreContacts (renderCoreAccounts, ниже по файлу).
const contactsSearchCache=personSearch.createPersonSearchCache();
function contactSearchText(contact){return `${contact.display_name||''} ${(contact.emails||[]).map(item=>item.email).join(' ')} ${(contact.phones||[]).map(contactPhoneLabel).join(' ')} ${(contact.addresses||[]).map(contactAddressLabel).join(' ')}`;}
function renderContacts(contacts=coreContacts){const query=(document.querySelector('.ct-search input')?.value||'').trim(),queryVariants=query?personSearch.personSearchVariants(query):[],filtered=contacts.filter(contact=>!query||contactsSearchCache.get(contact.id,()=>contactSearchText(contact)).some(key=>queryVariants.some(variant=>key.includes(variant)))),grid=document.getElementById('cgrid');grid.innerHTML='';filtered.forEach((contact,index)=>{const primary=contact.emails?.[0]?.email||contactPhoneLabel(contact.phones?.[0])||contactAddressLabel(contact.addresses?.[0]),card=document.createElement('button');card.type='button';card.className='ccard';card.dataset.contactId=contact.id;card.innerHTML=`<span class="ava ava-c${index%8}"></span><div><div class="cn"></div><div class="ce"></div></div>`;card.querySelector('.ava').textContent=(contact.display_name||primary||'?').split(/\s+/).map(word=>word[0]).join('').slice(0,2).toUpperCase();card.querySelector('.cn').textContent=contact.display_name||primary||'';card.querySelector('.ce').textContent=primary||'';
  // Почтовый адрес показываем отдельной строкой, если он не занял место
  // основной подписи (у контакта без почты и телефона).
  const addressLabel=contactAddressLabel(contact.addresses?.[0]);if(addressLabel&&addressLabel!==primary){const line=document.createElement('div');line.className='ce ca';line.textContent=addressLabel;card.querySelector('div').appendChild(line);}
  // Провайдер аккаунта может не поддерживать запись контактов на сервер (см.
  // is_local_only в ядре) - тогда контакт живёт только в локальной БД, и это
  // не должно выглядеть как обычная синхронизированная запись.
  if(contact.is_local_only){const badge=document.createElement('div');badge.className='note-muted';badge.textContent=L('Только на этом устройстве','This device only');card.querySelector('div').appendChild(badge);card.title=L('Провайдер этого аккаунта не поддерживает синхронизацию контактов - запись хранится только локально','This account provider does not support contact sync - the record is stored locally only');}
  card.onclick=()=>openContactEditor(contact);grid.appendChild(card);});const count=document.querySelector('.ct-count');if(count)count.textContent=`${filtered.length}${query?` / ${contacts.length}`:''} ${wizardLocale==='en'?'contacts':'контактов'}`;}
document.querySelector('.ct-search input')?.addEventListener('input',()=>renderContacts());
const contactViewSwitch=document.getElementById('contactViewSwitch');
if(contactViewSwitch){contactViewSwitch.querySelectorAll('button').forEach(button=>button.onclick=()=>{contactViewSwitch.querySelectorAll('button').forEach(other=>other.classList.toggle('on',other===button));const view=button.dataset.cview;document.getElementById('cgrid')?.classList.toggle('table-view',view==='table');window.tm?.setSetting('contacts_view',view).catch(console.error);});}
/* Ссылки из письма открываем в системном браузере: внутри webview target="_blank"
   означает попап, Tauri его блокирует, и клик молча не делает ничего. */
function bindExternalLinks(scope){
  if(!scope)return;
  scope.addEventListener('click',event=>{
    const link=event.target?.closest?.('a[href]');
    if(!link)return;
    const href=link.href||'';
    if(!/^https?:/i.test(href))return;
    event.preventDefault();
    window.tm?.openExternal(href).catch(error=>showToast(error.message||String(error)));
  });
}

/* Проверка href/src/xlink:href на опасность (замена дырявой регулярки, которая ловила только
   "data:text/html:" с двоеточием после mime, хотя реальные data-URI используют ";" или ",").
   Перед проверкой нормализуем значение: декодируем HTML-entity (числовые "&#106;" и именованные
   "&amp;"), убираем управляющие символы и пробелы по всей строке (не только в начале) - иначе
   "java\tscript:" и "&#106;avascript:" браузер схлопнет и выполнит, а наша проверка их пропустит.
   data: разрешаем только для картиночных mime-типов: repo.rs инлайнит вложения как
   data:<mime>;base64,... прямо из письма без белого списка, и вредоносное вложение может прийти
   с mime "text/html" - это должно быть отловлено именно здесь. */
function isDangerousUrl(value){
  let s=String(value||'');
  s=s.replace(/&#x([0-9a-f]+);?/gi,(_,hex)=>String.fromCodePoint(parseInt(hex,16)))
     .replace(/&#(\d+);?/g,(_,dec)=>String.fromCodePoint(parseInt(dec,10)))
     .replace(/&amp;/gi,'&');
  s=s.replace(/[\x00-\x20\x7f]+/g,'').toLowerCase();
  const schemeMatch=s.match(/^([a-z][a-z0-9+.-]*):/);
  if(!schemeMatch)return false;
  const scheme=schemeMatch[1];
  if(['javascript','vbscript','file','blob'].includes(scheme))return true;
  if(scheme==='data'){
    // Белый список, а не черный: единственный легитимный data: в письме - инлайн-вложение
    // картинки (repo.rs собирает data:<mime>;base64). Все остальное (text/html, svg с скриптами,
    // xml с XSLT, pdf) браузер может трактовать как активный документ, поэтому отсекаем по умолчанию.
    const rest=s.slice(scheme.length+1);
    const mime=rest.split(',')[0].split(';')[0];
    return !['image/png','image/jpeg','image/jpg','image/gif','image/webp','image/bmp','image/x-icon','image/vnd.microsoft.icon','image/avif','image/tiff'].includes(mime);
  }
  return false;
}
// stillCurrent - проверка актуальности показа: тяжёлое письмо с картинками ждёт
// ещё и ответа о доверии отправителю и дорисовывается заметно позже лёгкого,
// выбранного следом (S-002).
async function renderHtmlMessage(container,html,sender,stillCurrent){
  const normalizedSender=String(sender||'').trim().toLocaleLowerCase();
  const allowRemote=Boolean(normalizedSender)&&await window.tm?.imageSenderTrusted(normalizedSender).catch(()=>false);
  if(stillCurrent&&!stillCurrent())return;
  const parsed=new DOMParser().parseFromString(html,'text/html');
  parsed.querySelectorAll('script,iframe,object,embed,form,input,button,textarea,select,base,link,meta,audio,video').forEach(node=>node.remove());
  let blocked=false;
  parsed.querySelectorAll('style').forEach(node=>{node.textContent=node.textContent.replace(/url\(\s*(['"]?)https?:[^)]*\)/gi,'none');});
  parsed.querySelectorAll('*').forEach(node=>{[...node.attributes].forEach(attr=>{const name=attr.name.toLowerCase(),value=attr.value.trim();if(name.startsWith('on')||['srcdoc','formaction','integrity','nonce'].includes(name)||((name==='href'||name==='src'||name==='xlink:href')&&isDangerousUrl(value)))node.removeAttribute(attr.name);else if(name==='style')node.setAttribute('style',value.replace(/url\(\s*(['"]?)https?:[^)]*\)/gi,'none'));});});
  parsed.querySelectorAll('a').forEach(link=>{link.target='_blank';link.rel='noopener noreferrer';try{const url=new URL(link.href);[...url.searchParams.keys()].filter(key=>key.toLowerCase().startsWith('utm_')||['fbclid','gclid'].includes(key.toLowerCase())).forEach(key=>url.searchParams.delete(key));link.href=url.toString();}catch(_){}});
  parsed.querySelectorAll('img,source').forEach(image=>{const src=image.getAttribute('src')||image.getAttribute('srcset')||'';if(/^https?:/i.test(src)&&!allowRemote){blocked=true;image.removeAttribute('src');image.removeAttribute('srcset');image.setAttribute('alt',image.getAttribute('alt')||L('Удалённое изображение заблокировано','Remote image blocked'));}image.setAttribute('loading','lazy');image.setAttribute('referrerpolicy','no-referrer');image.style.maxWidth='100%';image.style.height='auto';});
  container.classList.add('html');
  if(blocked){const notice=document.createElement('div');notice.className='blocked';const text=document.createElement('span');text.textContent=L('Удалённые изображения заблокированы для защиты от отслеживания.','Remote images are blocked to prevent tracking.');const button=document.createElement('button');button.type='button';button.textContent=L(`Показывать от ${sender}`,`Always show from ${sender}`);button.onclick=async()=>{await window.tm?.setImageSenderTrusted(normalizedSender,true);container.replaceChildren();await renderHtmlMessage(container,html,sender,stillCurrent);};notice.append(text,button);container.appendChild(notice);}
  const frame=document.createElement('iframe');frame.className='mail-html-frame';frame.title=L('Содержимое HTML-письма','HTML message content');frame.setAttribute('sandbox','allow-same-origin allow-popups');const styles='<style>html,body{margin:0;padding:0;max-width:100%;overflow-wrap:anywhere;color:#17181c;font:14px/1.55 Arial,sans-serif}*{box-sizing:border-box}img,table{max-width:100%}a{color:#4b52c0}pre{white-space:pre-wrap}</style>';frame.srcdoc=`<!doctype html><html><head><meta charset="utf-8"><base target="_blank">${styles}${parsed.head.innerHTML}</head><body>${parsed.body.innerHTML}</body></html>`;frame.onload=()=>{try{frame.style.height=`${Math.max(120,frame.contentDocument.documentElement.scrollHeight+8)}px`;bindFrameNavigationKeys(frame.contentDocument);bindExternalLinks(frame.contentDocument);}catch(_){frame.style.height='480px';}};container.appendChild(frame);
}
// Беседы (threading): по цепочке ответов (thread_id) в пределах аккаунта. Одна
// строка на беседу со счётчиком, разворот показывает письма.
let conversationsEnabled=false;
const expandedConversations=new Set();
// Ключ беседы - только цепочка письма. Письмо без цепочки составляет группу из
// себя одного: по одной теме объединять нельзя ни в списке, ни тем более в
// действиях - иначе одно нажатие уносило бы в корзину письма разных
// отправителей с темой вида "Счёт", а thread_id есть далеко не у всех писем
// (его получают только ответы и те, кому ответили).
function conversationKey(message){return message.thread_id!=null?`${message.account_id}|t:${message.thread_id}`:`${message.account_id}|m:${message.id}`;}
function collapseConversations(rows){
  const groups=new Map();
  rows.forEach(message=>{const key=conversationKey(message);if(!groups.has(key))groups.set(key,[]);groups.get(key).push(message);});
  // Сортируем беседы по дате самого свежего письма, а письма развёрнутой беседы
  // держим сразу под её строкой: общая сортировка по дате раскидывала их по списку.
  const ordered=[...groups.entries()].map(([key,items])=>{items.sort(byDateDesc);return{key,items};});
  ordered.sort((a,b)=>byDateDesc(a.items[0],b.items[0]));
  const result=[];
  ordered.forEach(({key,items})=>{
    result.push({...items[0],_convKey:key,_convCount:items.length});
    if(items.length>1&&expandedConversations.has(key))for(let i=1;i<items.length;i++)result.push({...items[i],_convKey:key,_convChild:true});
  });
  return result;
}
// В режиме диалогов групповая операция над свёрнутой беседой применяется ко всем
// её письмам. Развёрнутую беседу не расширяем - действие идёт по конкретному письму.
function expandConversationIds(ids){
  if(!conversationsEnabled)return ids;
  const out=new Set();
  ids.forEach(id=>{
    const source=messages.find(item=>item.id===id);
    if(!source){out.add(id);return;}
    const key=conversationKey(source);
    if(expandedConversations.has(key)){out.add(id);return;}
    // Действие идёт по тому же ключу и по тому же набору писем, из которого
    // собрана строка списка. По всему кэшу расширять нельзя: при активном
    // фильтре или в разделе метки строка показывает одно письмо, а под действие
    // ушла бы вся цепочка папки - включая прочитанные и письма без метки.
    out.add(id);
    // Только в пределах папки исходного письма: беседа может жить в нескольких
    // папках (Входящие/Отправленные), но действие над строкой не должно трогать
    // письма из других папок.
    (lastListRows.length?lastListRows:messages).forEach(item=>{if(item.folder_id===source.folder_id&&conversationKey(item)===key)out.add(item.id);});
  });
  return [...out];
}
window.expandConversationIds=expandConversationIds;
let lastListRows=[],lastListTitle='';
function toggleConversation(key){if(expandedConversations.has(key))expandedConversations.delete(key);else expandedConversations.add(key);renderMessageList(lastListRows,lastListTitle);}
async function moveMessagesByDrop(ids,folder){const unique=[...new Set(ids.map(Number).filter(Number.isFinite))];if(!unique.length||unique.every(id=>messages.find(message=>message.id===id)?.folder_id===folder.id))return;try{const queued=await window.tm.moveMessagesToFolder(unique,folder.id);clearMessageSelection();activeMessage=null;activeFullMessage=null;window.forgetMessages?.(unique);await window.reloadCoreData();showToast(L(`Письма перемещены в «${folderTitle(folder)}»`,`Messages moved to “${folderTitle(folder)}”`),L('Отменить','Undo'),async()=>{await window.tm.undoMessageAction(queued.operation_ids);await window.reloadCoreData();});}catch(error){showToast(error.message||String(error));}}
function createMessageRow(message,index){
  const row=document.createElement('div');row.className='msg'+(message.flags?.seen?'':' unread')+(message._convChild?' conv-child':'')+(selectedMessageIds.has(message.id)?' selected':'')+(activeMessage?.id===message.id?' active':'');row.dataset.messageId=message.id;row.draggable=true;
  // Строка - элемент списка, а не кнопка: роли кнопки достался бы общий
  // обработчик Enter и пробела, и письмо открывалось бы дважды. В обход по Tab
  // пускаем одну строку - активную, а без активной первую; к остальным ведут
  // стрелки (S-005).
  const active=activeMessage?.id===message.id;
  row.setAttribute('role','option');row.setAttribute('aria-selected',String(active||selectedMessageIds.has(message.id)));
  row.tabIndex=-1;
  // Сторона строки - роль папки самого письма: в Отправленных и Черновиках
  // показываем получателя. Роль берём из готовой карты, поиск по coreFolders в
  // горячем пути прокрутки запрещён.
  const presentation=mailAddresses.rowPresentation(message,coreFolderRoles);
  row.innerHTML=`<div class="avawrap"><span class="ava" style="background:${accountColorById(message.account_id)};color:${contrastOn(accountColorById(message.account_id))}"></span></div><div class="body"><div class="l1"><span class="from"></span></div><div class="subj"></div><div class="prev"></div></div><div class="meta"><span class="time"></span><span class="time-hm"></span></div>`;
  row.querySelector('.ava').textContent=presentation.initial;
  row.querySelector('.from').textContent=presentation.kind==='empty'?L('Без получателя','No recipient'):presentation.text;
  // Счётчик остальных получателей - отдельный элемент вне обрезки подписи:
  // иначе многоточие длинного имени съедало бы суффикс "+N".
  if(presentation.extra>0){const extra=document.createElement('span');extra.className='from-extra';extra.textContent=`+${presentation.extra}`;row.querySelector('.l1').appendChild(extra);}
  if(message._convCount>1){const expanded=expandedConversations.has(message._convKey);const badge=document.createElement('button');badge.type='button';badge.className='conv-count'+(expanded?' on':'');badge.textContent=message._convCount;badge.title=expanded?L('Свернуть беседу','Collapse conversation'):L(`Показать письма беседы (${message._convCount})`,`Show conversation messages (${message._convCount})`);badge.tabIndex=-1;badge.onclick=event=>{event.stopPropagation();
    // Строку забираем в фокус до перестроения: сама кнопка при нём исчезает, а
    // фокус должен остаться на беседе (S-007).
    row.focus({preventScroll:true});toggleConversation(message._convKey);};row.querySelector('.l1').appendChild(badge);}
  row.querySelector('.subj').textContent=message.subject||'';row.querySelector('.prev').textContent=message.preview||'';
  row.querySelector('.time').textContent=message.date?new Date(message.date).toLocaleDateString(document.documentElement.lang):'';
  row.querySelector('.time-hm').textContent=message.date?new Date(message.date).toLocaleTimeString(document.documentElement.lang,{hour:'2-digit',minute:'2-digit'}):'';
  // Ящик письма (message-mailbox-owner.md, S-004): в объединённых списках одно
  // и то же письмо из двух ящиков выглядит двумя одинаковыми строками, и
  // отличить их можно только подписью.
  const mailbox=mailAddresses.listMailboxLabel(message.account_id,coreAccounts,currentFolderId===null,L('ящик удалён','mailbox removed'));
  if(mailbox){const box=document.createElement('span');box.className='mbox';box.textContent=mailbox;box.title=mailbox;row.querySelector('.meta').appendChild(box);}
  if(message.labels?.length){const meta=row.querySelector('.meta');message.labels.forEach(name=>{const tag=coreTags.find(item=>item.name===name);const badge=document.createElement('span');badge.className='msg-tag';badge.textContent=name;const tagColor=tag?.color||'#888';badge.style.setProperty('--tag-color',tagColor);badge.style.setProperty('--tag-text',contrastOn(tagColor));meta.appendChild(badge);});}
  row.ondragstart=event=>{if(!selectedMessageIds.has(message.id)){selectedMessageIds.clear();selectedMessageIds.add(message.id);selectionAnchorId=message.id;updateSelectionUi();}row.classList.add('mail-dragging');event.dataTransfer.effectAllowed='move';event.dataTransfer.setData('application/x-truemail-messages',JSON.stringify([...selectedMessageIds]));};row.ondragend=()=>{row.classList.remove('mail-dragging');document.querySelectorAll('.folder-row.drop-hi').forEach(item=>item.classList.remove('drop-hi'));};
  let swipe=null,suppressClick=false;row.onpointerdown=event=>{if(event.pointerType==='mouse'||event.button!==0)return;swipe={id:event.pointerId,x:event.clientX,y:event.clientY,dx:0};};row.onpointermove=event=>{if(!swipe||event.pointerId!==swipe.id)return;const dx=event.clientX-swipe.x,dy=event.clientY-swipe.y;if(Math.abs(dy)>Math.abs(dx)&&Math.abs(dy)>10){swipe=null;row.style.transform='';return;}if(Math.abs(dx)<8)return;event.preventDefault();swipe.dx=dx;row.classList.add('swiping');row.classList.toggle('swipe-archive',dx>0);row.classList.toggle('swipe-trash',dx<0);row.style.transform=`translateX(${Math.max(-120,Math.min(120,dx))}px)`;};const finishSwipe=event=>{if(!swipe||event.pointerId!==swipe.id)return;const action=Math.abs(swipe.dx)>=80?(swipe.dx>0?'archive':'trash'):null;swipe=null;row.classList.remove('swiping','swipe-archive','swipe-trash');row.style.transform='';if(action){suppressClick=true;setTimeout(()=>{suppressClick=false;},250);window.performMessageActionForIds?.(action,[message.id]);}};row.onpointerup=finishSwipe;row.onpointercancel=finishSwipe;
  row.onpointerenter=e=>{if(selectionDragMode===null||!(e.buttons&1))return;selectionDragMode?selectedMessageIds.add(message.id):selectedMessageIds.delete(message.id);updateSelectionUi();};
  // Фокус ставим сами: после клика по обычному div он ушёл бы на документ, и
  // дальнейшие нажатия адресовались бы пустому месту (S-005).
  row.onclick=e=>{if(suppressClick)return;row.focus({preventScroll:true});if(e.shiftKey){selectMessageRange(index,e.ctrlKey||e.metaKey);return;}if(e.ctrlKey||e.metaKey){selectedMessageIds.has(message.id)?selectedMessageIds.delete(message.id):selectedMessageIds.add(message.id);selectionAnchorId=message.id;updateSelectionUi();return;}if(selectedMessageIds.size)clearMessageSelection();selectionAnchorId=message.id;showMessage(message);};renderIcons(row);return row;
}
function renderMessageWindow(force=false){
  // Окно скрыто - разметку не строим: её только что освободили ради памяти, и
  // наполнять невидимый список заново незачем. Отрисуется при возврате окна.
  if(document.hidden)return;
  messageRowHeight=Number.parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--message-row-height'))||76;
  const list=msgsEl,total=currentMessageRows.length,viewport=Math.max(list.clientHeight,400),start=Math.max(0,Math.floor(list.scrollTop/messageRowHeight)-MESSAGE_WINDOW_OVERSCAN),end=Math.min(total,Math.ceil((list.scrollTop+viewport)/messageRowHeight)+MESSAGE_WINDOW_OVERSCAN);
  if(!force&&start===messageWindowStart&&end===messageWindowEnd)return;messageWindowStart=start;messageWindowEnd=end;
  // Строки сейчас будут пересозданы: запоминаем письмо, на строке которого стоял
  // фокус, чтобы вернуть его на новую строку того же письма (S-006).
  const focusId=listFocusMessageId();
  // Обёртки виртуализации в дереве доступности не участвуют: иначе связка
  // listbox с его элементами рвалась бы на двух лишних узлах (S-005).
  let canvas=list.querySelector(':scope > .message-list-canvas');if(!canvas){canvas=document.createElement('div');canvas.className='message-list-canvas';canvas.setAttribute('role','presentation');list.replaceChildren(canvas);}canvas.style.height=`${total*messageRowHeight}px`;
  const fragment=document.createDocumentFragment(),windowEl=document.createElement('div');windowEl.className='message-list-window';windowEl.setAttribute('role','presentation');windowEl.style.transform=`translateY(${start*messageRowHeight}px)`;for(let index=start;index<end;index++)windowEl.appendChild(createMessageRow(currentMessageRows[index],index));fragment.appendChild(windowEl);canvas.replaceChildren(fragment);
  // Остановку Tab выбираем по отрисованному окну, а не по номеру письма в
  // списке: при прокрутке первой строки в разметке уже нет, и обход прошёл бы
  // список насквозь (S-005).
  const tabStop=windowEl.querySelector('.msg.active')||windowEl.firstElementChild;if(tabStop)tabStop.tabIndex=0;
  if(focusId!=null)focusMessageRow(focusId);
}
// Клавиатурная навигация и открытие письма из уведомления переносят и фокус:
// пользователь пришёл к письму и продолжит работать со списком (S-006).
function focusMessageAt(index){if(index<0||index>=currentMessageRows.length)return;const top=index*messageRowHeight,bottom=top+messageRowHeight;if(top<msgsEl.scrollTop)setMessageScrollTop(top);else if(bottom>msgsEl.scrollTop+msgsEl.clientHeight)setMessageScrollTop(Math.max(0,bottom-msgsEl.clientHeight));renderMessageWindow(true);showMessage(currentMessageRows[index]);focusMessageRow(currentMessageRows[index].id);}
function renderMessageList(rows,title,resetScroll=false){
  lastListRows=rows;lastListTitle=title;
  if(conversationsEnabled)rows=collapseConversations(rows);
  currentMessageRows=[...rows];const visibleIds=new Set(rows.map(message=>message.id));for(const id of selectedMessageIds)if(!visibleIds.has(id))selectedMessageIds.delete(id);if(resetScroll)setMessageScrollTop(0);messageWindowStart=-1;messageWindowEnd=-1;
  // Заголовок списка - то же пользовательское имя, что и подпись в панели:
  // без пометки словарь автоперевода подменял бы его переводом фразы.
  const heading=document.querySelector('.listhead h2');if(heading){heading.dataset.noI18n='1';heading.textContent=title||messagesTitle();}
  renderListSubtitle(rows.length);
  renderMessageWindow(true);updateSelectionUi();
  // Панель письма очищена - поколение показа растёт вместе с ней: ответ на
  // запрос, начатый до очистки, рисовать уже некуда (S-001).
  if(!rows.length){document.getElementById('tbody').innerHTML=`<div class="mail-empty"><h2>${wizardLocale==='en'?'No messages':'Писем нет'}</h2></div>`;messageViewGeneration++;}
  else if(!activeMessage||!rows.some(message=>message.id===activeMessage.id)){document.getElementById('tbody').innerHTML=`<div class="mail-empty"><h2>${wizardLocale==='en'?'Select a message':'Выберите письмо'}</h2></div>`;messageViewGeneration++;}
}
// Общий строитель строк "Кому"/"Копия" шапки письма: тот же отбор адресов, та же
// свёртка и независимое состояние раскрытия у каждой строки. Возвращает null,
// когда показывать нечего - тогда шапка не получает ни строки, ни отступа.
function buildAddressLine(className,labelText,addresses){
  const maxShown=2,full=mailAddresses.addressLineModel(addresses,true,maxShown);if(!full)return null;
  const line=document.createElement('div');line.className=className;
  const render=expanded=>{
    const model=mailAddresses.addressLineModel(addresses,expanded,maxShown);
    line.innerHTML='';line.classList.toggle('expanded',expanded);
    const label=document.createElement('span');label.className='mail-address-label';label.textContent=labelText;line.appendChild(label);
    const names=document.createElement('span');names.className='mail-address-names';line.appendChild(names);
    model.shown.forEach((item,index)=>{if(index)names.appendChild(document.createTextNode(', '));const span=document.createElement('span');span.className='mail-address-item';span.textContent=item.text;span.title=item.title;names.appendChild(span);});
    if(!expanded&&model.hidden>0){const more=document.createElement('button');more.type='button';more.className='mail-address-toggle';more.textContent=`+${model.hidden}`;more.title=L('Показать всех','Show all');more.onclick=()=>render(true);line.appendChild(more);}
    else if(expanded&&full.shown.length>maxShown){const less=document.createElement('button');less.type='button';less.className='mail-address-toggle';less.textContent=L('Свернуть','Collapse');less.onclick=()=>render(false);line.appendChild(less);}
  };
  render(false);
  return line;
}
// Строка "Ящик" шапки письма (message-mailbox-owner.md, S-001, S-002, S-006).
// null - подключён один ящик, показывать нечего.
function buildMailboxLine(accountId){
  const text=mailAddresses.mailboxLabel(accountId,coreAccounts,L('ящик удалён','mailbox removed'));
  if(!text)return null;
  const line=document.createElement('div');line.className='mail-boxline';line.dataset.accountId=String(accountId??'');
  const label=document.createElement('span');label.className='mail-address-label';label.textContent=L('Ящик: ','Mailbox: ');line.appendChild(label);
  const value=document.createElement('span');value.className='mail-address-item';value.textContent=text;value.title=text;line.appendChild(value);
  return line;
}
// Смена языка не перерисовывает открытое письмо целиком (тело пришлось бы
// собирать заново), поэтому подпись строки "Ящик" обновляем отдельно (S-008).
window.relocalizeMailboxLine=function(){
  const line=document.querySelector('.mail-boxline');
  if(!line)return;
  const accountId=line.dataset.accountId===''?null:Number(line.dataset.accountId);
  const text=mailAddresses.mailboxLabel(accountId,coreAccounts,L('ящик удалён','mailbox removed'));
  if(!text){line.remove();return;}
  line.querySelector('.mail-address-label').textContent=L('Ящик: ','Mailbox: ');
  const value=line.querySelector('.mail-address-item');value.textContent=text;value.title=text;
};
// Поколение показа письма. Ядро при необходимости докачивает письмо с сервера,
// поэтому ответы приходят вразнобой: без сверки поколения ответ на первый клик
// дорисовывал бы своё письмо поверх выбранного вторым (S-001..S-004).
let messageViewGeneration=0;
async function showMessage(message){
  const generation=++messageViewGeneration;
  // Отброшенный ответ пишем в журнал интерфейса: без записи разбирать жалобы
  // "показалось не то письмо" нечем. Пишем один раз на запрос - проверок в пути
  // несколько, а событие одно.
  let discarded=false;
  const stillCurrent=()=>{const current=messageView.isCurrentView(generation,messageViewGeneration);if(!current&&!discarded){discarded=true;window.tm?.uiLog?.(`показ письма ${message.id} отброшен: поколение ${generation} против ${messageViewGeneration}`);}return current;};
  activeMessage=message;
  document.getElementById('tSubject').textContent=message.subject||'';const body=document.getElementById('tbody');
  body.innerHTML=`<div class="mail-loading">${L('Загрузка письма…','Loading message…')}</div>`;
  // Признак активной строки и остановка Tab идут вместе: сюда приходят и клик,
  // и навигация клавишами, а окно списка при этом не перестраивается (S-005).
  const rows=[...document.querySelectorAll('.msg')];rows.forEach(row=>{const active=+row.dataset.messageId===message.id;row.classList.toggle('active',active);row.setAttribute('aria-selected',String(active||selectedMessageIds.has(+row.dataset.messageId)));row.tabIndex=active?0:-1;});
  // Строки активного письма в окне может не быть (открыто из палитры, а список
  // прокручен в другое место) - тогда остановка Tab достаётся первой строке,
  // иначе список выпал бы из обхода целиком.
  if(rows.length&&!rows.some(row=>row.tabIndex===0))rows[0].tabIndex=0;
  try{
    const full=await window.tm?.getMessage(message.id);
    // Пока ядро отвечало, пользователь мог выбрать другое письмо: ни тела, ни
    // шапки, ни activeFullMessage от устаревшего запроса на экране быть не должно.
    if(!stillCurrent())return;
    activeFullMessage=full;body.innerHTML='';const article=document.createElement('article');article.className='mail-content';
    const head=document.createElement('header');head.innerHTML=`<div class="mail-fromline">${L('От:','From:')} <b class="mail-from"></b> <span class="mail-address"></span></div>`;
    const fromName=full.meta.from?.name||'',fromEmail=full.meta.from?.email||'';
    head.querySelector('.mail-from').textContent=fromName||fromEmail;
    head.querySelector('.mail-address').textContent=fromName&&fromEmail?`(${fromEmail})`:'';
    // "Кому" и "Копия" строит одна функция: <=2 адресов - показываем всех без
    // кнопки; 3+ - первые двое и "+N", раскрытие даёт полный список и кнопку
    // "Свернуть". Состояния строк независимы.
    const toLine=buildAddressLine('mail-toline',L('Кому: ','To: '),full.meta.to);if(toLine)head.appendChild(toLine);
    const ccLine=buildAddressLine('mail-ccline',L('Копия: ','Cc: '),full.meta.cc);if(ccLine)head.appendChild(ccLine);
    // Ящик, в который пришло письмо (message-mailbox-owner.md, S-001, S-003).
    // Это не заголовок письма: у копии, забранной другим ящиком по POP3,
    // строка "Кому" остаётся исходной, и без этой строки два ящика неразличимы.
    const mailboxLine=buildMailboxLine(full.meta.account_id);if(mailboxLine)head.appendChild(mailboxLine);
    const content=document.createElement('div');content.className='mail-body';if(full.body_html)await renderHtmlMessage(content,full.body_html,full.meta.from?.email,stillCurrent);else{content.classList.add('plain');content.textContent=full.body_text||full.meta.preview||'';}
    // Отрисовка тела тоже ждёт (проверка доверия отправителю для картинок), и за
    // это время выбор мог смениться - к панели и к отметке прочтения переходим
    // только для актуального письма (S-002, S-003).
    if(!stillCurrent())return;
    article.append(head);if(full.attachments?.length){article.appendChild(buildAttachmentBar(full,message.id));}article.appendChild(content);body.appendChild(article);if(!message.flags?.seen){
      // Признак и счётчики проставляет markMessagesSeen - вызываем её до того,
      // как трогать flags.seen, иначе она сочтёт письмо уже прочитанным и
      // дельта счётчика потеряется.
      stickyReadIds.add(message.id);document.querySelector(`.msg[data-message-id="${message.id}"]`)?.classList.remove('unread');
      window.markMessagesSeen?.(message,true).catch(console.error);
    }
  // Ошибка устаревшего запроса на экран не идёт: там уже актуальное письмо (S-004).
  }catch(error){if(!stillCurrent())return;body.innerHTML='';const err=document.createElement('div');err.className='mail-error';err.textContent=error.message||String(error);body.appendChild(err);}
}
function smartMessageValue(message,field){const folder=coreFolders.find(item=>item.id===message.folder_id);switch(field){
  case 'sender':return `${message.from?.name||''} ${message.from?.email||''}`.trim();case 'recipient':return [...(message.to||[]),...(message.cc||[])].map(address=>`${address.name||''} ${address.email||''}`.trim()).join(' ');case 'subject':return message.subject||'';case 'body':return message.preview||'';case 'account':return coreAccounts.find(account=>account.id===message.account_id)?.email||'';case 'folder':return `${folder?.display_name||''} ${folder?.remote_path||''}`.trim();case 'folder_role':return folder?.role||'other';case 'read_state':return message.flags?.seen?'read':'unread';case 'importance':return message.flags?.flagged?'flagged':'normal';case 'reply_state':return message.flags?.answered?'answered':'unanswered';case 'draft_state':return message.flags?.draft?'draft':'not_draft';case 'attachment':return message.has_attachments?'has':'none';case 'size':return message.size;case 'label':return (message.labels||[]).join(' ');case 'date':return message.date||'';default:return '';}}
function smartConditionMatches(message,source){const condition=normalizeSmartCondition(source);if(!validSmartCondition(condition))return false;const field=smartField(condition.f),raw=smartMessageValue(message,field.id);if(field.type==='date'){const timestamp=new Date(raw).getTime();if(!Number.isFinite(timestamp))return false;if(['within_last','older_than'].includes(condition.o)){const amount=Number(condition.v),multipliers={minutes:60000,hours:3600000,days:86400000,weeks:604800000},threshold=Date.now()-amount*(multipliers[condition.u]||multipliers.hours);return condition.o==='within_last'?timestamp>=threshold:timestamp<threshold;}const target=new Date(`${condition.v}T00:00:00`).getTime();if(!Number.isFinite(target))return false;const next=target+86400000;if(condition.o==='before')return timestamp<target;if(condition.o==='after')return timestamp>=next;return timestamp>=target&&timestamp<next;}
  if(field.type==='size'){const bytes=Number(raw);if(!Number.isFinite(bytes))return false;const multipliers={kb:1024,mb:1048576,gb:1073741824},factor=multipliers[condition.u]||multipliers.mb,min=Number(condition.v)*factor,max=Number(condition.v2)*factor;if(condition.o==='greater_than')return bytes>min;if(condition.o==='greater_or_equal')return bytes>=min;if(condition.o==='less_than')return bytes<min;if(condition.o==='less_or_equal')return bytes<=min;if(condition.o==='between')return bytes>=min&&bytes<=max;return bytes===min;}
  const left=String(raw).toLocaleLowerCase(),right=String(condition.v).toLocaleLowerCase();if(condition.o==='not_contains')return !left.includes(right);if(condition.o==='equals')return left===right;if(condition.o==='not_equals')return left!==right;if(condition.o==='starts_with')return left.startsWith(right);if(condition.o==='ends_with')return left.endsWith(right);return left.includes(right);}
// Подходит ли письмо под условия умной папки. Тот же разбор, что и в
// smartRowsForFolder, но для одного письма: нужен, чтобы поправить счётчики
// сразу после прочтения, не гоняя ядро по всей базе.
function smartFolderMatchesMessage(folder,message){
  const groups=(folder?.groups||[]).map(normalizeSmartGroup).filter(group=>group.conditions.length);
  if(!groups.length||window.coreUnifiedSettings?.[message.folder_id]==='0')return false;
  return groups.some(group=>group.logic==='any'
    ?group.conditions.some(condition=>smartConditionMatches(message,condition))
    :group.conditions.every(condition=>smartConditionMatches(message,condition)));
}
// Пометки прочтения, ещё не подтверждённые ядром: id -> ожидаемое значение.
// Как только данные из ядра приносят то же значение, ожидание снимается.
const pendingSeen=new Map();
function applyPendingSeen(message){
  if(!pendingSeen.has(message.id))return message;
  const expected=pendingSeen.get(message.id);
  if(Boolean(message.flags?.seen)===expected){pendingSeen.delete(message.id);return message;}
  return {...message,flags:{...message.flags,seen:expected}};
}
// Единственная точка смены признака прочтения. Дельту счётчиков применяем
// сразу, а при отказе записи откатываем ровно то письмо, которое не прошло:
// иначе ожидание в pendingSeen осталось бы навсегда и подменяло бы верные
// данные ядра на устаревшие при каждой перезагрузке.
window.markMessagesSeen=function(list,seen){
  const items=(Array.isArray(list)?list:[list]).filter(Boolean);
  if(!items.length)return Promise.resolve();
  // Откатываем только те письма, которые правка действительно затронула: в
  // групповом действии часть писем уже в нужном состоянии, и обратная дельта
  // сделала бы их неверными.
  const changed=window.applySmartCountsSeenChange(items,seen);
  const changedIds=new Set(changed.map(message=>message.id));
  return Promise.all(items.map(message=>window.tm?.markSeen(message.id,seen)
    // Ядро записало значение в базу, ожидание больше не нужно: следующая
    // перезагрузка прочитает уже новое состояние.
    .then(()=>pendingSeen.delete(message.id))
    .catch(error=>{
      pendingSeen.delete(message.id);
      if(changedIds.has(message.id))window.applySmartCountsSeenChange(message,!seen);
      throw error;
    })));
};
// Письма меняют признак "прочитано" - правим числа счётчиков на месте. Проход
// ядра по всей базе придёт позже и уточнит их, но ждать его нельзя: в умной
// папке "Непрочитанные" счётчик и есть смысл списка, и он должен уменьшаться в
// тот же миг, когда письмо открыто. Вызывать до изменения флага у письма.
window.applySmartCountsSeenChange=function(list,seen){
  const changing=(Array.isArray(list)?list:[list]).filter(message=>message&&Boolean(message.flags?.seen)!==seen);
  if(!changing.length)return [];
  let touched=false;
  smartFolders.forEach(folder=>{
    if(folder.on===false||smartCounterMode(folder)==='n')return;
    const counts=smartCounts[folder.id];if(!counts)return;
    changing.forEach(message=>{
      const before={...message,flags:{...message.flags,seen:Boolean(message.flags?.seen)}};
      const after={...message,flags:{...message.flags,seen}};
      const matchedBefore=smartFolderMatchesMessage(folder,before);
      const matchedAfter=smartFolderMatchesMessage(folder,after);
      if(!matchedBefore&&!matchedAfter)return;
      counts.total=Math.max(0,counts.total+(matchedAfter?1:0)-(matchedBefore?1:0));
      counts.unread=Math.max(0,counts.unread+(matchedAfter&&!seen?1:0)-(matchedBefore&&!before.flags.seen?1:0));
      touched=true;
    });
  });
  // Признак проставляем здесь же и во всех представлениях письма: часть путей
  // меняет его только после ответа ядра, а одно письмо живёт разными объектами
  // в общем списке и в страницах умных папок. Правка одного объекта оставила бы
  // соседний непрочитанным, и следующее действие над ним засчиталось бы второй
  // раз - счётчик уехал бы вниз без единого письма.
  const changed=new Set(changing.map(message=>message.id));
  // Запись в ядро идёт без ожидания, а фоновая перезагрузка может прочитать
  // письмо раньше, чем пометка туда доедет, и вернуть его непрочитанным. Держим
  // ожидаемое значение до подтверждения: слияние данных сверяется с ним, иначе
  // список снова показал бы письмо непрочитанным, а счётчик остался бы
  // уменьшенным - и разошёлся бы с содержимым папки.
  changed.forEach(id=>pendingSeen.set(id,seen));
  const mark=message=>{if(changed.has(message.id)){if(!message.flags)message.flags={};message.flags.seen=seen;}};
  changing.forEach(mark);messages.forEach(mark);coreSmartRows.forEach(rows=>rows.forEach(mark));
  if(touched)updateSmartBadges();
  return changing;
};
function smartRowsForFolder(folder){const groups=(folder?.groups||[]).map(normalizeSmartGroup).filter(group=>group.conditions.length);if(!groups.length)return [];return messages.filter(message=>window.coreUnifiedSettings?.[message.folder_id]!=='0'&&groups.some(group=>group.logic==='any'?group.conditions.some(condition=>smartConditionMatches(message,condition)):group.conditions.every(condition=>smartConditionMatches(message,condition))));}
const coreSmartRows=new Map();
// Папки-источники, уже обойдённые догрузкой в текущем круге, по умным папкам.
const smartBackfillVisited=new Map();
function smartRows(index){const folder=smartFolders[index];return coreSmartRows.get(folder?.id)||smartRowsForFolder(folder);}
// serverBackfill=true разрешает ходить за старыми письмами на сервер. Такой
// проход делаем только по прямому действию пользователя (докрутил список до
// конца). Фоновая перезагрузка данных приходит сюда с serverBackfill=false:
// иначе получалась самоподдерживающаяся петля - догрузка писала письма в базу,
// база слала truemail-data-changed, обработчик перезагружал данные и снова
// заходил сюда за догрузкой. Петля крутилась сутками, качала всю историю ящика
// и держала список писем в памяти WebView целиком.
async function loadSmartCoveragePage(index,reset=false,serverBackfill=false){
  if(loadingSmartCoverage){queuedSmartCoverage={index,reset:reset||queuedSmartCoverage?.reset||false,serverBackfill:serverBackfill||queuedSmartCoverage?.serverBackfill||false};return;}const folder=smartFolders[index];if(!folder)return;
  // reset обнуляет страницу умной папки, но не право снова идти на сервер:
  // раньше через reset фоновая перезагрузка обходила признак "сервер больше
  // ничего не даёт" и запускала догрузку заново на каждой синхронизации.
  if(!reset&&smartHasMore.get(folder.id)===false)return;
  if(serverBackfill&&smartServerExhausted.get(folder.id)===true)serverBackfill=false;
  loadingSmartCoverage=true;window.setListLoading?.(true,smartFolderTitle(folder));
  let circleClosed=false;
  try{const existing=reset?[]:(coreSmartRows.get(folder.id)||[]),known=new Set(existing.map(message=>message.id));
    // Курсор - истинный минимум (старейшая дата, затем наименьший id), иначе при
    // равных датах запрос вернёт уже показанные письма и прокрутка встанет.
    const cursor=existing.reduce((min,message)=>{if(!min)return message;const cmp=String(message.date||'').localeCompare(String(min.date||''));return (cmp<0||(cmp===0&&message.id<min.id))?message:min;},null);
    let rows=await window.tm.listSmartFolderMessages(folder.id,cursor?(cursor.date||''):null,cursor?.id||null,SMART_MESSAGE_PAGE_SIZE);
    let fresh=rows.filter(message=>!known.has(message.id));
    // Прогресс - по новым письмам, а не по длине страницы (могут прийти дубли).
    // Нет новых, а на сервере больше - догружаем по папкам-источникам и повторяем.
    // Догружаем по всем папкам-источникам, а не только по тем, что уже дали
    // совпадения: папка без единого совпавшего письма иначе не догружалась бы
    // никогда. Курсор берём по самой папке - общий курсор умной папки мог быть
    // новее её писем, и диапазон между ними оставался бы пропущенным. За один
    // проход обходим не больше SMART_BACKFILL_FOLDERS папок: каждая - отдельное
    // подключение к серверу, остальные подхватит следующий проход.
    if(!fresh.length&&cursor?.date&&serverBackfill){
      // Сначала папки с наибольшим отставанием: при неизменном порядке первые
      // пять крупных папок забирали бы все проходы, а остальные не догрузились
      // бы никогда. Число писем и курсор по папкам считаем за один проход по
      // списку: раньше внутри цикла по папкам шёл messages.filter, и на десятках
      // папок с десятками тысяч писем это давало миллионы итераций на каждый
      // проход догрузки.
      const localCounts=new Map(),folderCursors=new Map();
      messages.forEach(message=>{const id=message.folder_id;localCounts.set(id,(localCounts.get(id)||0)+1);
        const min=folderCursors.get(id);if(!min){folderCursors.set(id,message);return;}
        const cmp=String(message.date||'').localeCompare(String(min.date||''));if(cmp<0||(cmp===0&&message.id<min.id))folderCursors.set(id,message);});
      const candidates=coreFolders.map(source=>({source,behind:(source.total_count||0)-(localCounts.get(source.id)||0)}))
        .filter(item=>window.coreUnifiedSettings?.[item.source.id]!=='0'&&item.behind>0)
        .sort((left,right)=>right.behind-left.behind).map(item=>item.source);
      // Держим множество уже обойдённых папок этой умной папки: список
      // кандидатов пересобирается каждый проход, и позиция в нём ничего не
      // значит. Когда обойдены все, круг начинается заново.
      // Круг обхода живёт на каждую умную папку и переживает перезагрузку
      // данных: фоновая синхронизация идёт постоянно и приходит сюда с reset,
      // а обнуление круга на каждый такой заход означало бы вечный возврат к
      // тем же самым крупным папкам.
      let visited=smartBackfillVisited.get(folder.id);
      if(!visited){visited=new Set();smartBackfillVisited.set(folder.id,visited);}
      // Папки, которых больше нет среди кандидатов, из круга убираем - иначе
      // множество копило бы id удалённых папок.
      visited.forEach(id=>{if(!candidates.some(source=>source.id===id))visited.delete(id);});
      if(candidates.every(source=>visited.has(source.id))){visited.clear();smartCircleFetched.delete(folder.id);}
      const picked=candidates.filter(source=>!visited.has(source.id)).slice(0,SMART_BACKFILL_FOLDERS);
      picked.forEach(source=>visited.add(source.id));
      if(candidates.length>picked.length)window.tm?.uiLog?.(`догрузка умной папки: папок ${candidates.length}, за проход ${picked.length}`);
      let fetchedAny=false,backfillFailed=false;
      for(const source of picked){
        // Курсор папки: сначала тот, что вернуло ядро прошлым проходом, потом
        // самое старое письмо в памяти. По списку писем его вести нельзя -
        // догруженные письма, не подошедшие фильтру умной папки, в память не
        // попадают, курсор стоял бы на месте и сервер отдавал бы ту же страницу.
        const folderCursor=smartBackfillCursor.get(source.id)||folderCursors.get(source.id)?.date||cursor.date;
        try{
          const page=await window.tm?.fetchOlderMessages(source.id,folderCursor,BACKFILL_PAGE_SIZE);
          if(page?.oldest)smartBackfillCursor.set(source.id,page.oldest);
          if((page?.fetched||0)>0){fetchedAny=true;smartCircleFetched.set(folder.id,true);}
        }catch(error){
          // Упавшую папку возвращаем в круг: иначе следующий проход счёл бы её
          // пройденной, круг закрылся бы без неё и умная папка замерла бы.
          backfillFailed=true;visited.delete(source.id);console.error('truemail smart backfill:',error);
        }
      }
      if(fetchedAny){rows=await window.tm.listSmartFolderMessages(folder.id,cursor?(cursor.date||''):null,cursor?.id||null,SMART_MESSAGE_PAGE_SIZE);fresh=rows.filter(message=>!known.has(message.id));}
      // Круг закрыт - все папки-источники опрошены и ни один запрос не упал.
      circleClosed=!backfillFailed&&candidates.every(source=>visited.has(source.id));
      // Исчерпанной папку считаем, только когда круг закрыт и сервер за весь
      // круг не отдал вообще ничего: история ящиков кончилась. Признак берём
      // накопительный по кругу, а не по последнему проходу - иначе папка
      // считалась бы исчерпанной, когда письма пришли в начале круга, а
      // последняя пятёрка папок оказалась пустой. Если письма приходили, но
      // фильтру не подошли, совпадения могут быть глубже - прокрутка должна
      // продолжать копать, а не упираться в потолок.
      if(circleClosed&&!fetchedAny&&smartCircleFetched.get(folder.id)!==true)smartServerExhausted.set(folder.id,true);}
    // Страница за страницей умная папка может набрать больше писем, чем
    // интерфейс вообще способен показать, и все они удерживаются в памяти -
    // и здесь, и через пиннинг в trimMessages. Держим потолок: список
    // отсортирован от новых к старым, поэтому лишнее отрезаем с конца.
    fresh=fresh.map(applyPendingSeen);rows=rows.map(applyPendingSeen);
    const combined=[...existing,...fresh];coreSmartRows.set(folder.id,combined);
    // Умная папка кончилась, только когда сервер обойден полностью и больше
    // ничего не отдаёт. Пустая страница сама по себе концом не считается:
    // локально совпадений нет, но следующий проход по другим папкам-источникам
    // ещё может их принести, и прокрутка не должна вставать.
    if(fresh.length||smartServerExhausted.get(folder.id)===true)smartHasMore.set(folder.id,fresh.length>0);
    const byId=new Map(messages.map(message=>[message.id,message]));rows.forEach(message=>byId.set(message.id,applyPendingSeen(message)));messages=trimMessages([...byId.values()],rows.map(message=>message.id));
  }catch(error){console.error('smart folder coverage',error);}finally{loadingSmartCoverage=false;window.setListLoading?.(false);if(currentSmartIndex===index&&currentFolderId===null){applyListOptions(false);window.ensureListFilled?.(serverBackfill);}if(smartOverlay.classList.contains('open'))updateSmartPreview();const queued=queuedSmartCoverage;queuedSmartCoverage=null;if(queued)loadSmartCoveragePage(queued.index,queued.reset,queued.serverBackfill);}
}
// resetScroll=true - пользователь сам открыл умную папку: разрешаем ей снова
// сходить на сервер. resetScroll=false - это фоновая перезагрузка данных, она
// только обновляет страницу из локальной базы.
function filterSmart(index,resetScroll=true){window.setListLoading?.(false);currentSmartIndex=index;currentFolderId=null;currentTagName=null;const folder=smartFolders[index];if(resetScroll&&folder){smartServerExhausted.delete(folder.id);smartCircleFetched.delete(folder.id);smartBackfillVisited.delete(folder.id);smartBackfillCursor.clear();}applyListOptions(resetScroll,smartFolderTitle(smartFolders[index])||messagesTitle());loadSmartCoveragePage(index,true,resetScroll);}

window.renderCoreAccounts=function(accounts,foldersByAccount,loadedMessages=[],contacts=[],calendarData={calendars:[],events:[]},savedSmartFolders=[],storage=null){
  const previousFolder=currentFolderId,previousTag=currentTagName,previousMessageId=activeMessage?.id,navScroll=document.querySelector('.nav')?.scrollTop||0,messageScroll=msgsEl.scrollTop;let previousSmart=currentSmartIndex;
  // Якорь списка снимаем здесь, до clearDemoData: сразу за ним прокрутка
  // обнуляется, и якорь всегда указывал бы на первое письмо. Между
  // перезагрузками он не хранится (S-009). Заодно запоминаем, стоял ли фокус на
  // строке: разметку списка clearDemoData сносит целиком (S-006).
  const listAnchor=messageView.listAnchorAt(currentMessageRows,messageScroll,messageRowHeight);
  rememberListFocus();
  window.clearDemoData(true);
  coreAccounts=accounts;setCoreFolders(foldersByAccount.flat());coreContacts=contacts;coreCalendarData=calendarData;
  // coreContacts обновился - кэши ключей транслитерации по всем поверхностям
  // устарели (person-search-translit.md, S-012): раздел контактов (эта же
  // область видимости), подсказка адресата и палитра команд (другие модули).
  contactsSearchCache.invalidate();window.invalidateComposerContactCache?.();window.invalidatePaletteContactCache?.();
  // Готовность композера для файлов из меню "Отправить": аккаунт может
  // появиться и вне мастера настройки (добавили второй ящик, закрыли мастер
  // навигацией) - тогда очередь забирается сразу, а не после перезапуска.
  const composerWasReady=window.tmComposerReady===true;
  window.tmComposerReady=coreAccounts.length>0&&window.tmOnboardingDone===true&&!document.getElementById('welcomeView')?.classList.contains('active');
  if(window.tmComposerReady&&!composerWasReady)window.consumePendingAttachments?.();
  // Объединяем догруженные ранее письма со свежей выборкой (свежая версия
  // побеждает по id), иначе перезагрузка данных стирала бы всё, что пользователь
  // подгрузил прокруткой, и список сбрасывался бы на первую страницу.
  // Старую копию храним только если она за границей свежей страницы папки: письмо
  // внутри страницы, которого в выборке нет, из папки ушло (перемещено, удалено,
  // ждёт отправки в очереди) - иначе оно продолжало бы висеть в прежней папке.
  {const page=window.corePageSize||100,freshById=new Map(loadedMessages.map(message=>[message.id,message])),counts=new Map(),edges=new Map();
   loadedMessages.forEach(message=>{const folder=message.folder_id;counts.set(folder,(counts.get(folder)||0)+1);
     const date=message.date||'',edge=edges.get(folder);if(!edge||date<edge.date||(date===edge.date&&message.id<edge.id))edges.set(folder,{date,id:message.id});});
   const survived=messages.filter(message=>{if(freshById.has(message.id))return true;
     const edge=edges.get(message.folder_id);if(!edge||(counts.get(message.folder_id)||0)<page)return false;
     const date=message.date||'';return date<edge.date||(date===edge.date&&message.id<edge.id);});
   const merged=new Map(survived.map(message=>[message.id,message]));loadedMessages.forEach(message=>merged.set(message.id,applyPendingSeen(message)));messages=trimMessages([...merged.values()],loadedMessages.map(message=>message.id));}
  coreSmartRows.clear();smartHasMore.clear();if(savedSmartFolders.length){const activeId=smartFolders[previousSmart]?.id;smartFolders.splice(0,smartFolders.length,...normalizedSmartFolders(savedSmartFolders.map(smartFolderFromCore)));if(activeId){const restored=smartFolders.findIndex(folder=>folder.id===activeId);if(restored>=0)previousSmart=restored;}renderSmartManagement();bindSmartNavigation();}
  // Счётчики умных папок пересчитываем после каждой перезагрузки данных: письма
  // могли прийти, уйти или стать прочитанными. Прежние числа возвращаем на
  // место сразу - clearDemoData стирает подписи счётчиков, а ответ ядра придёт
  // не мгновенно, и счётчик успел бы мигнуть пустотой.
  updateSmartBadges();window.refreshSmartCounts?.();
  // Раз в сотню перезагрузок пишем в журнал, сколько писем держит интерфейс и
  // сколько занято памяти. WebView при исчерпании памяти падает молча, и без
  // этих чисел причину роста потом не восстановить.
  if((coreReloadCount=(coreReloadCount||0)+1)%100===1){
    const heap=performance?.memory?.usedJSHeapSize;
    const smartRows=[...coreSmartRows.values()].reduce((sum,rows)=>sum+rows.length,0);
    window.tm?.uiLog?.(`память интерфейса: писем ${messages.length}, строк умных папок ${smartRows}, событий ${(coreCalendarData.events||[]).length}${heap?`, куча ${Math.round(heap/1048576)} МБ`:''}`);
  }
  // Список правил ждёт загрузки меток: правило с действием "поставить метку"
  // без них показывало метку как "?". Заодно, когда свежий список меток пришёл,
  // проверяем открытую метку - её могли удалить, и тогда уходим в умную папку.
  refreshTagsNav().then(()=>{renderRulesList();
    if(currentTagName!=null&&!coreTags.some(tag=>tag.name===currentTagName)){const gone=currentTagName;currentTagName=null;window.resetTagPaging?.(gone);if(smartFolders.length)filterSmart(currentSmartIndex??0,false);else applyListOptions(true,messagesTitle());}});
  const accountCount=document.getElementById('mailAccountCount');if(accountCount){const n=accounts.length,label=wizardLocale==='en'?(n===1?'account':'accounts'):(n%10===1&&n%100!==11?'аккаунт':n%10>=2&&n%10<=4&&(n%100<10||n%100>=20)?'аккаунта':'аккаунтов');accountCount.textContent=`${n} ${label}`;}
  // Прокрутка активна, если локальная страница заполнена ИЛИ на сервере писем
  // больше, чем загружено локально (тогда при прокрутке идёт догрузка с сервера).
  // Счётчик писем по папкам считаем за один проход: messages.filter внутри
  // цикла по папкам давал квадратичный обход (десятки папок на десятки тысяч
  // писем) и заметно грузил процессор на каждой перезагрузке данных.
  {const localCounts=new Map();messages.forEach(message=>localCounts.set(message.folder_id,(localCounts.get(message.folder_id)||0)+1));
   coreFolders.forEach(folder=>{const localCount=localCounts.get(folder.id)||0;folderHasMore.set(folder.id,localCount===MESSAGE_INITIAL_PAGE_SIZE||(folder.total_count||0)>localCount);});}
  const labels=[...document.querySelectorAll('.nav .navlabel')];
  const accountsLabel=document.querySelector('.nav [data-navlabel="accounts"]')||labels.find(el=>el.textContent.includes('Аккаунты'))||labels[1];
  let anchor=accountsLabel;
  accounts.forEach((account,index)=>{
    const accountOpen=accountNavIsOpen(account.id);
    const header=document.createElement('button');header.type='button';header.className='acc-h'+(accountOpen?' open':'');header.dataset.accountId=account.id;header.dataset.noI18n='1';
    const initial=(account.display_name||account.email||'?').trim()[0].toUpperCase();
    header.innerHTML=`<span class="ava" style="background:${accountColorById(account.id)};color:${contrastOn(accountColorById(account.id))}"></span><span class="em"></span><span class="chev"><i data-i="chevR"></i></span>`;
    header.querySelector('.ava').textContent=initial;header.querySelector('.em').textContent=account.email;
    anchor.after(header);anchor=header;
    // Имена папок ящика приходят с сервера: словарь автоперевода не должен их
    // трогать, иначе папка "Календарь" переводилась бы вместе с интерфейсом.
    const sub=document.createElement('div');sub.className='acc-sub'+(accountOpen?' open':'');sub.dataset.noI18n='1';
    folderTreeRows(foldersByAccount[index]||[]).forEach(({folder,depth})=>{const row=document.createElement('button');row.type='button';row.className='navitem folder-row';row.dataset.folderId=folder.id;
      const icon=folderIcon(folder);row.style.paddingLeft=`${14+depth*14}px`;
      row.innerHTML=`<i data-i="${icon}"></i><span class="folder-name"></span>`;
      row.querySelector('.folder-name').textContent=folderTitle(folder);updateFolderBadge(row,folder);
      const openFolder=()=>{window.setListLoading?.(false);goMail();document.querySelectorAll('.navitem').forEach(item=>item.classList.remove('active'));row.classList.add('active');currentFolderId=folder.id;currentSmartIndex=null;currentTagName=null;applyListOptions(true,folderTitle(folder));window.ensureListFilled?.();};row.onclick=openFolder;row.oncontextmenu=event=>{event.preventDefault();event.stopPropagation();contextFolder=folder;contextFolderOpen=openFolder;ctxfolder.dataset.system=folder.role?'true':'false';ctxfolder.querySelectorAll('[data-folder-action="rename"],[data-folder-action="delete"]').forEach(item=>item.classList.toggle('disabled',Boolean(folder.role)));const mode=folderCounterMode(folder);ctxfolder.querySelector('[data-folder-action="count-unread"]')?.classList.toggle('on',mode.includes('u'));ctxfolder.querySelector('[data-folder-action="count-total"]')?.classList.toggle('on',mode.includes('t'));posMenu(ctxfolder,event);};row.ondragover=event=>{event.preventDefault();event.dataTransfer.dropEffect='move';row.classList.add('drop-hi');};row.ondragleave=event=>{if(!row.contains(event.relatedTarget))row.classList.remove('drop-hi');};row.ondrop=event=>{event.preventDefault();row.classList.remove('drop-hi');try{moveMessagesByDrop(JSON.parse(event.dataTransfer.getData('application/x-truemail-messages')),folder);}catch(_){}};sub.appendChild(row);});
    anchor.after(sub);anchor=sub;
  });
  renderIcons(document.querySelector('.nav'));
  if(previousFolder!==null&&coreFolders.some(folder=>folder.id===previousFolder)){
    currentFolderId=previousFolder;currentSmartIndex=null;const folder=coreFolders.find(item=>item.id===previousFolder);document.querySelector(`.folder-row[data-folder-id="${previousFolder}"]`)?.classList.add('active');applyListOptions(false,folderTitle(folder));
  // Просмотр метки переживает перезагрузку данных: раньше любая фоновая
  // синхронизация выбрасывала из метки в первую умную папку.
  }else if(previousTag!=null&&coreTags.some(tag=>tag.name===previousTag)){
    // Список писем метки перезагрузка обнуляет вместе с остальным - страницу
    // метки набираем заново, иначе раздел остался бы пустым без возможности
    // прокрутки.
    currentTagName=previousTag;currentFolderId=null;currentSmartIndex=null;renderTagsNav();window.resetTagPaging?.(previousTag);applyListOptions(false,previousTag);window.loadNextTagPage?.();
  }else filterSmart(previousSmart??0,false);
  if(previousMessageId&&messages.some(message=>message.id===previousMessageId)){activeMessage=messages.find(message=>message.id===previousMessageId);document.querySelector(`.msg[data-message-id="${previousMessageId}"]`)?.classList.add('active');}else if(previousMessageId){activeMessage=null;activeFullMessage=null;document.getElementById('tSubject').textContent='';document.getElementById('tbody').innerHTML=`<div class="mail-empty"><h2>${wizardLocale==='en'?'Select a message':'Выберите письмо'}</h2></div>`;}
  if(messages.length)document.querySelector('.thread .actions')?.classList.remove('hidden');
  renderContacts(contacts);
  renderCalendarData(calendarData);
  renderAccountSettings(accounts,foldersByAccount,calendarData.calendars||[]);
  if(storage)applyStorageStatus(storage);
  if(Object.keys(uiCatalog).length)applyUiCatalog(uiCatalog);
  requestAnimationFrame(()=>{const nav=document.querySelector('.nav');if(nav)nav.scrollTop=navScroll;
    // Положение списка держим по верхнему видимому письму: вставленные сверху
    // письма сдвигают строки, и прежняя пиксельная позиция подводила курсор к
    // соседнему письму. Якоря в новом списке нет - остаются прежние пиксели.
    setMessageScrollTop(messageView.listAnchorOffset(currentMessageRows,listAnchor.id,messageScroll,messageRowHeight));
    // Если окно было скрыто, позицию задаёт запомненное письмо, а не пиксели:
    // список пришёл из базы заново и его высота изменилась.
    window.restoreHiddenAnchor?.();
    renderMessageWindow(true);
    // Фокус возвращаем последним: до восстановления прокрутки строки нужного
    // письма в окне виртуализации может не быть.
    applyPendingListFocus();});
};
let accountOauthState='';
let accountPasswordProvider='generic';
function isExpiredOauthCode(error){return /invalid_grant|code has expired|verification code.*expired/i.test(error?.message||String(error));}
function updateAccountConnectionType(){const type=document.getElementById('accountConnectionType').value,exchange=type==='exchange',jmap=type==='jmap',title=document.getElementById('accountPasswordTitle'),desc=document.getElementById('accountPasswordDesc');document.getElementById('accountEwsField').classList.toggle('hidden',!exchange);document.getElementById('accountJmapField').classList.toggle('hidden',!jmap);document.querySelectorAll('#accountPasswordRow .server-pair').forEach(row=>row.classList.toggle('hidden',exchange||jmap));if(exchange){title.dataset.i18n='exchangeConnectionTitle';desc.dataset.i18n='exchangeConnectionDesc';title.textContent=L('Подключение Exchange','Connect Exchange');desc.textContent=L('Введите пароль доменной учётной записи. Адрес EWS уже определён автоматически — меняйте его только если сервер использует другой путь. Пароль хранится только в системном хранилище Windows.','Enter the domain account password. The EWS address was detected automatically; change it only if the server uses a different path. The password is stored only in Windows Credential Manager.');}else if(jmap){title.dataset.i18n='jmapConnectionTitle';desc.dataset.i18n='jmapConnectionDesc';title.textContent=L('Подключение JMAP','Connect JMAP');desc.textContent=L('Введите отдельный пароль приложения и проверьте адрес JMAP Session. Пароль хранится только в системном хранилище.','Enter an app password and check the JMAP Session address. The password is stored only in the system credential store.');}else{title.dataset.i18n='imapConnectionTitle';desc.dataset.i18n='imapConnectionDesc';title.textContent=L('Подключение IMAP / SMTP','Connect IMAP / SMTP');desc.textContent=L('Проверьте серверы входящей и исходящей почты. Для Mail.ru и iCloud используйте отдельный пароль приложения.','Check the incoming and outgoing mail servers. Use an app password for Mail.ru and iCloud.');}}
document.getElementById('accountConnectionType').onchange=updateAccountConnectionType;
function showPasswordConnection(config){accountPasswordProvider=config.provider;document.getElementById('accountConnectionType').value=config.backend_kind==='ews'?'exchange':config.backend_kind==='jmap'?'jmap':'imap';document.getElementById('accountUsername').value=config.username||document.getElementById('accountEmail').value.trim();document.getElementById('accountEwsServer').value=config.ews_url||'';document.getElementById('accountJmapServer').value=config.jmap_url||'';document.getElementById('accountImapHost').value=config.imap?.host||'';document.getElementById('accountImapPort').value=config.imap?.port||993;document.getElementById('accountImapSecurity').value=config.imap?.security||'ssl';document.getElementById('accountSmtpHost').value=config.smtp?.host||'';document.getElementById('accountSmtpPort').value=config.smtp?.port||465;document.getElementById('accountSmtpSecurity').value=config.smtp?.security||'ssl';updateAccountConnectionType();document.getElementById('accountConnectionDetectRow').classList.add('hidden');document.getElementById('accountPasswordRow').classList.remove('hidden');document.getElementById('accountPassword').focus();}
document.getElementById('accountOauthStart').onclick=async()=>{
  const email=document.getElementById('accountEmail').value.trim(),status=document.getElementById('accountOauthStatus');
  const button=document.getElementById('accountOauthStart');
  if(!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)){status.textContent=L('Введите корректный адрес почты.','Enter a valid email address.');status.dataset.kind='error';return;}
  if(!window.tm?.beginAccountConnection){status.textContent=L('OAuth доступен внутри приложения truemail.','OAuth is available inside the truemail app.');status.dataset.kind='error';return;}
  try{button.disabled=true;status.textContent=L('Определяю провайдера и способ входа…','Detecting provider and sign-in method…');status.dataset.kind='';const pending=await window.tm.beginAccountConnection(email);if(pending.mode==='connected'&&pending.connected){const connected=pending.connected;status.textContent=connected.warnings?.length?connected.warnings.join(' '):L('Аккаунт подключён.','Account connected.');status.dataset.kind=connected.warnings?.length?'warning':'success';setTimeout(async()=>{closeAccountWizard();await window.reloadCoreData?.();await window.tm?.startRealtime();showView('mailView');},connected.warnings?.length?2500:300);return;}if(pending.mode==='password'){showPasswordConnection(pending.password_config);status.textContent=L('Проверьте серверы и введите пароль приложения или почтовый пароль.','Check the servers and enter an app password or mail password.');return;}accountOauthState=pending.state;document.getElementById('accountCodeRow').classList.remove('hidden');status.textContent=L('После входа скопируйте сюда код подтверждения.','After signing in, paste the confirmation code here.');document.getElementById('accountOauthCode').focus();}
  catch(e){button.disabled=false;status.textContent=e.message||String(e);status.dataset.kind='error';}
};
document.getElementById('accountPasswordConfirm').onclick=async()=>{const button=document.getElementById('accountPasswordConfirm'),status=document.getElementById('accountOauthStatus'),password=document.getElementById('accountPassword').value,email=document.getElementById('accountEmail').value.trim(),username=document.getElementById('accountUsername').value.trim(),type=document.getElementById('accountConnectionType').value,exchange=type==='exchange',jmap=type==='jmap';if(!password){status.textContent=L('Введите пароль.','Enter the password.');status.dataset.kind='error';return;}try{button.disabled=true;status.textContent=exchange?L('Ищу EWS через Autodiscover и проверяю Exchange…','Discovering EWS and checking Exchange…'):jmap?L('Проверяю JMAP Session и доступ к почте…','Checking the JMAP Session and mail access…'):L('Проверяю IMAP и подключаю аккаунт…','Checking IMAP and connecting the account…');status.dataset.kind='';const connected=exchange?await window.tm.completeExchangeEws({email,username,password,serverHint:document.getElementById('accountEwsServer').value.trim()}):jmap?await window.tm.completeJmap({email,username,password,sessionUrl:document.getElementById('accountJmapServer').value.trim()}):await window.tm.completePasswordImap({email,username,password,provider:accountPasswordProvider,imapHost:document.getElementById('accountImapHost').value.trim(),imapPort:Number(document.getElementById('accountImapPort').value),imapSecurity:document.getElementById('accountImapSecurity').value,smtpHost:document.getElementById('accountSmtpHost').value.trim(),smtpPort:Number(document.getElementById('accountSmtpPort').value),smtpSecurity:document.getElementById('accountSmtpSecurity').value});document.getElementById('accountPassword').value='';status.textContent=connected.warnings?.length?connected.warnings.join(' '):L('Аккаунт подключён.','Account connected.');status.dataset.kind=connected.warnings?.length?'warning':'success';setTimeout(async()=>{closeAccountWizard();await window.reloadCoreData?.();await window.tm?.startRealtime();showView('mailView');},connected.warnings?.length?2500:300);}catch(error){status.textContent=error.message||String(error);status.dataset.kind='error';button.disabled=false;}};
document.getElementById('accountOauthConfirm').onclick=async()=>{
  const code=document.getElementById('accountOauthCode').value.trim(),status=document.getElementById('accountOauthStatus');if(!code)return;
  try{status.textContent=L('Подключаю почту, календарь и контакты…','Connecting mail, calendar and contacts…');status.dataset.kind='';document.getElementById('accountOauthConfirm').disabled=true;const connected=await window.tm.completeYandexOauth(accountOauthState,code);status.textContent=connected.warnings?.length?connected.warnings.join(' '):L('Аккаунт подключён.','Account connected.');status.dataset.kind=connected.warnings?.length?'warning':'success';setTimeout(async()=>{closeAccountWizard();await window.reloadCoreData?.();await window.tm?.startRealtime();showView('mailView');},connected.warnings?.length?2500:300);}
  catch(e){if(isExpiredOauthCode(e)){accountOauthState='';document.getElementById('accountOauthCode').value='';document.getElementById('accountCodeRow').classList.add('hidden');document.getElementById('accountOauthStart').disabled=false;status.textContent=L('Код истёк или уже был использован. Нажмите «Подключить» и получите новый код.','The code expired or was already used. Select Connect to get a new code.');}else status.textContent=e.message||String(e);status.dataset.kind='error';document.getElementById('accountOauthConfirm').disabled=false;}
};
document.getElementById('wzConnect').onclick=async()=>{
  const email=document.getElementById('wzEmail').value.trim(),status=document.getElementById('wzConnectStatus');
  const button=document.getElementById('wzConnect');
  if(!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)){status.textContent=wt('invalidEmail');status.dataset.kind='error';return;}
  if(!window.tm?.beginAccountConnection){status.textContent=wt('oauthUnavailable');status.dataset.kind='error';return;}
  try{button.disabled=true;status.textContent=wizardLocale==='en'?'Detecting provider and sign-in method…':'Определяю провайдера и способ входа…';status.dataset.kind='';const pending=await window.tm.beginAccountConnection(email);if(pending.mode==='connected'&&pending.connected){const connected=pending.connected;status.textContent=connected.warnings?.length?connected.warnings.join(' '):wt('connected');status.dataset.kind=connected.warnings?.length?'warning':'success';document.getElementById('wzAccountNext').disabled=false;return;}if(pending.mode==='password'){showAccountWizard(email);showPasswordConnection(pending.password_config);document.getElementById('accountOauthStart').disabled=true;document.getElementById('accountOauthStatus').textContent=L('Проверьте серверы и введите пароль приложения или почтовый пароль.','Check the servers and enter an app password or mail password.');return;}pendingOauthState=pending.state;document.getElementById('wzCodeBox').classList.remove('hidden');status.textContent=wt('enterCode');document.getElementById('wzOauthCode').focus();}
  catch(e){button.disabled=false;status.textContent=e.message||String(e);status.dataset.kind='error';}
};
document.getElementById('wzConfirm').onclick=async()=>{
  const code=document.getElementById('wzOauthCode').value.trim(),status=document.getElementById('wzConnectStatus');if(!code)return;
  try{status.textContent=wt('connecting');status.dataset.kind='';document.getElementById('wzConfirm').disabled=true;const connected=await window.tm.completeYandexOauth(pendingOauthState,code);status.textContent=connected.warnings?.length?connected.warnings.join(' '):wt('connected');status.dataset.kind=connected.warnings?.length?'warning':'success';document.getElementById('wzAccountNext').disabled=false;}
  catch(e){if(isExpiredOauthCode(e)){pendingOauthState='';document.getElementById('wzOauthCode').value='';document.getElementById('wzCodeBox').classList.add('hidden');document.getElementById('wzConnect').disabled=false;status.textContent=wt('codeExpired');}else status.textContent=e.message||String(e);status.dataset.kind='error';document.getElementById('wzConfirm').disabled=false;}
};

