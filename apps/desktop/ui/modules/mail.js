// truemail UI module: mail.js
function formatBytes(bytes){if(!Number.isFinite(+bytes)||bytes<=0)return '0 Б';const units=['Б','КБ','МБ','ГБ','ТБ'];let value=+bytes,index=0;while(value>=1024&&index<units.length-1){value/=1024;index++;}return `${value>=10||index===0?value.toFixed(0):value.toFixed(1)} ${units[index]}`;}
function folderIcon(folder){return folder.role==='sent'?'send':folder.role==='drafts'?'draft':folder.role==='trash'?'trash':folder.role==='archive'?'archive':folder.role==='spam'?'spam':'inbox';}
// Окно "Исходный текст письма": raw MIME, копирование, закрытие.
async function openRawViewer(messageId){
  if(messageId==null)return;
  const overlay=document.createElement('div');overlay.className='raw-overlay';
  overlay.innerHTML=`<div class="raw-box"><div class="raw-head"><button class="btn raw-back">← ${L('Назад','Back')}</button><span class="raw-title">${L('Исходный текст письма','Message source')}</span><button class="btn primary raw-copy">${L('Копировать','Copy')}</button></div><textarea class="raw-text" readonly spellcheck="false"></textarea></div>`;
  document.body.appendChild(overlay);
  const ta=overlay.querySelector('.raw-text');ta.value=L('Загрузка…','Loading…');
  try{ta.value=await window.tm.messageRaw(messageId);}catch(error){ta.value=error.message||String(error);}
  function close(){overlay.remove();document.removeEventListener('keydown',key);}
  function key(e){if(e.key==='Escape')close();}
  overlay.querySelector('.raw-back').onclick=close;
  overlay.querySelector('.raw-copy').onclick=async()=>{
    try{await navigator.clipboard.writeText(ta.value);}catch{ta.select();document.execCommand('copy');}
    showToast(L('Исходный текст скопирован','Source copied'));
  };
  overlay.onclick=e=>{if(e.target===overlay)close();};
  document.addEventListener('keydown',key);
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
function contactPhoneLabel(phone){return phone?`${phone.number||''}${phone.extension?` ${L('доб.','ext.')} ${phone.extension}`:''}`:'';}
function renderContacts(contacts=coreContacts){const query=(document.querySelector('.ct-search input')?.value||'').trim(),filtered=contacts.filter(contact=>matchQ(`${contact.display_name||''} ${(contact.emails||[]).map(item=>item.email).join(' ')} ${(contact.phones||[]).map(contactPhoneLabel).join(' ')}`,query)),grid=document.getElementById('cgrid');grid.innerHTML='';filtered.forEach((contact,index)=>{const primary=contact.emails?.[0]?.email||contactPhoneLabel(contact.phones?.[0]),card=document.createElement('button');card.type='button';card.className='ccard';card.dataset.contactId=contact.id;card.innerHTML=`<span class="ava ava-c${index%8}"></span><div><div class="cn"></div><div class="ce"></div></div>`;card.querySelector('.ava').textContent=(contact.display_name||primary||'?').split(/\s+/).map(word=>word[0]).join('').slice(0,2).toUpperCase();card.querySelector('.cn').textContent=contact.display_name||primary||'';card.querySelector('.ce').textContent=primary||'';card.onclick=()=>openContactEditor(contact);grid.appendChild(card);});const count=document.querySelector('.ct-count');if(count)count.textContent=`${filtered.length}${query?` / ${contacts.length}`:''} ${wizardLocale==='en'?'contacts':'контактов'}`;}
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

async function renderHtmlMessage(container,html,sender){
  const normalizedSender=String(sender||'').trim().toLocaleLowerCase();
  const allowRemote=Boolean(normalizedSender)&&await window.tm?.imageSenderTrusted(normalizedSender).catch(()=>false);
  const parsed=new DOMParser().parseFromString(html,'text/html');
  parsed.querySelectorAll('script,iframe,object,embed,form,input,button,textarea,select,base,link,meta,audio,video').forEach(node=>node.remove());
  let blocked=false;
  parsed.querySelectorAll('style').forEach(node=>{node.textContent=node.textContent.replace(/url\(\s*(['"]?)https?:[^)]*\)/gi,'none');});
  parsed.querySelectorAll('*').forEach(node=>{[...node.attributes].forEach(attr=>{const name=attr.name.toLowerCase(),value=attr.value.trim();if(name.startsWith('on')||['srcdoc','formaction','integrity','nonce'].includes(name)||((name==='href'||name==='src'||name==='xlink:href')&&/^\s*(?:javascript|file|data:text\/html):/i.test(value)))node.removeAttribute(attr.name);else if(name==='style')node.setAttribute('style',value.replace(/url\(\s*(['"]?)https?:[^)]*\)/gi,'none'));});});
  parsed.querySelectorAll('a').forEach(link=>{link.target='_blank';link.rel='noopener noreferrer';try{const url=new URL(link.href);[...url.searchParams.keys()].filter(key=>key.toLowerCase().startsWith('utm_')||['fbclid','gclid'].includes(key.toLowerCase())).forEach(key=>url.searchParams.delete(key));link.href=url.toString();}catch(_){}});
  parsed.querySelectorAll('img,source').forEach(image=>{const src=image.getAttribute('src')||image.getAttribute('srcset')||'';if(/^https?:/i.test(src)&&!allowRemote){blocked=true;image.removeAttribute('src');image.removeAttribute('srcset');image.setAttribute('alt',image.getAttribute('alt')||L('Удалённое изображение заблокировано','Remote image blocked'));}image.setAttribute('loading','lazy');image.setAttribute('referrerpolicy','no-referrer');image.style.maxWidth='100%';image.style.height='auto';});
  container.classList.add('html');
  if(blocked){const notice=document.createElement('div');notice.className='blocked';const text=document.createElement('span');text.textContent=L('Удалённые изображения заблокированы для защиты от отслеживания.','Remote images are blocked to prevent tracking.');const button=document.createElement('button');button.type='button';button.textContent=L(`Показывать от ${sender}`,`Always show from ${sender}`);button.onclick=async()=>{await window.tm?.setImageSenderTrusted(normalizedSender,true);container.replaceChildren();await renderHtmlMessage(container,html,sender);};notice.append(text,button);container.appendChild(notice);}
  const frame=document.createElement('iframe');frame.className='mail-html-frame';frame.title=L('Содержимое HTML-письма','HTML message content');frame.setAttribute('sandbox','allow-same-origin allow-popups');const styles='<style>html,body{margin:0;padding:0;max-width:100%;overflow-wrap:anywhere;color:#17181c;font:14px/1.55 Arial,sans-serif}*{box-sizing:border-box}img,table{max-width:100%}a{color:#4b52c0}pre{white-space:pre-wrap}</style>';frame.srcdoc=`<!doctype html><html><head><meta charset="utf-8"><base target="_blank">${styles}${parsed.head.innerHTML}</head><body>${parsed.body.innerHTML}</body></html>`;frame.onload=()=>{try{frame.style.height=`${Math.max(120,frame.contentDocument.documentElement.scrollHeight+8)}px`;bindExternalLinks(frame.contentDocument);}catch(_){frame.style.height='480px';}};container.appendChild(frame);
}
// Беседы (threading): гибрид по цепочке ответов (thread_id) и нормализованной теме,
// в пределах аккаунта. Одна строка на беседу со счётчиком, разворот показывает письма.
let conversationsEnabled=false;
const expandedConversations=new Set();
function normalizeSubject(subject){return String(subject||'').replace(/^(\s*(re|fwd?|fw|отв|пересл)\s*:\s*)+/i,'').trim().toLowerCase();}
function conversationKey(message){const subject=normalizeSubject(message.subject);return `${message.account_id}|${subject||('thread-'+(message.thread_id??message.id))}`;}
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
let lastListRows=[],lastListTitle='';
function toggleConversation(key){if(expandedConversations.has(key))expandedConversations.delete(key);else expandedConversations.add(key);renderMessageList(lastListRows,lastListTitle);}
async function moveMessagesByDrop(ids,folder){const unique=[...new Set(ids.map(Number).filter(Number.isFinite))];if(!unique.length||unique.every(id=>messages.find(message=>message.id===id)?.folder_id===folder.id))return;try{const queued=await window.tm.moveMessagesToFolder(unique,folder.id);clearMessageSelection();activeMessage=null;activeFullMessage=null;await window.reloadCoreData();showToast(L(`Письма перемещены в «${folderTitle(folder)}»`,`Messages moved to “${folderTitle(folder)}”`),L('Отменить','Undo'),async()=>{await window.tm.undoMessageAction(queued.operation_ids);await window.reloadCoreData();});}catch(error){showToast(error.message||String(error));}}
function createMessageRow(message,index){
  const row=document.createElement('div');row.className='msg'+(message.flags?.seen?'':' unread')+(message._convChild?' conv-child':'')+(selectedMessageIds.has(message.id)?' selected':'')+(activeMessage?.id===message.id?' active':'');row.dataset.messageId=message.id;row.draggable=true;
  const initial=(message.from?.name||message.from?.email||'?').trim()[0].toUpperCase();
  row.innerHTML=`<div class="avawrap"><span class="ava" style="background:${accountColorById(message.account_id)}"></span></div><div class="body"><div class="l1"><span class="from"></span></div><div class="subj"></div><div class="prev"></div></div><div class="meta"><span class="time"></span><span class="time-hm"></span></div>`;
  row.querySelector('.ava').textContent=initial;row.querySelector('.from').textContent=message.from?.name||message.from?.email||'';
  if(message._convCount>1){const expanded=expandedConversations.has(message._convKey);const badge=document.createElement('button');badge.type='button';badge.className='conv-count'+(expanded?' on':'');badge.textContent=message._convCount;badge.title=expanded?L('Свернуть беседу','Collapse conversation'):L(`Показать письма беседы (${message._convCount})`,`Show conversation messages (${message._convCount})`);badge.onclick=event=>{event.stopPropagation();toggleConversation(message._convKey);};row.querySelector('.l1').appendChild(badge);}
  row.querySelector('.subj').textContent=message.subject||'';row.querySelector('.prev').textContent=message.preview||'';
  row.querySelector('.time').textContent=message.date?new Date(message.date).toLocaleDateString(document.documentElement.lang):'';
  row.querySelector('.time-hm').textContent=message.date?new Date(message.date).toLocaleTimeString(document.documentElement.lang,{hour:'2-digit',minute:'2-digit'}):'';
  row.ondragstart=event=>{if(!selectedMessageIds.has(message.id)){selectedMessageIds.clear();selectedMessageIds.add(message.id);lastSelectedMessageIndex=index;updateSelectionUi();}row.classList.add('mail-dragging');event.dataTransfer.effectAllowed='move';event.dataTransfer.setData('application/x-truemail-messages',JSON.stringify([...selectedMessageIds]));};row.ondragend=()=>{row.classList.remove('mail-dragging');document.querySelectorAll('.folder-row.drop-hi').forEach(item=>item.classList.remove('drop-hi'));};
  let swipe=null,suppressClick=false;row.onpointerdown=event=>{if(event.pointerType==='mouse'||event.button!==0)return;swipe={id:event.pointerId,x:event.clientX,y:event.clientY,dx:0};};row.onpointermove=event=>{if(!swipe||event.pointerId!==swipe.id)return;const dx=event.clientX-swipe.x,dy=event.clientY-swipe.y;if(Math.abs(dy)>Math.abs(dx)&&Math.abs(dy)>10){swipe=null;row.style.transform='';return;}if(Math.abs(dx)<8)return;event.preventDefault();swipe.dx=dx;row.classList.add('swiping');row.classList.toggle('swipe-archive',dx>0);row.classList.toggle('swipe-trash',dx<0);row.style.transform=`translateX(${Math.max(-120,Math.min(120,dx))}px)`;};const finishSwipe=event=>{if(!swipe||event.pointerId!==swipe.id)return;const action=Math.abs(swipe.dx)>=80?(swipe.dx>0?'archive':'trash'):null;swipe=null;row.classList.remove('swiping','swipe-archive','swipe-trash');row.style.transform='';if(action){suppressClick=true;setTimeout(()=>{suppressClick=false;},250);window.performMessageActionForIds?.(action,[message.id]);}};row.onpointerup=finishSwipe;row.onpointercancel=finishSwipe;
  row.onpointerenter=e=>{if(selectionDragMode===null||!(e.buttons&1))return;selectionDragMode?selectedMessageIds.add(message.id):selectedMessageIds.delete(message.id);updateSelectionUi();};
  row.onclick=e=>{if(suppressClick)return;if(e.shiftKey){selectMessageRange(index,e.ctrlKey||e.metaKey);return;}if(e.ctrlKey||e.metaKey){selectedMessageIds.has(message.id)?selectedMessageIds.delete(message.id):selectedMessageIds.add(message.id);lastSelectedMessageIndex=index;updateSelectionUi();return;}if(selectedMessageIds.size)clearMessageSelection();lastSelectedMessageIndex=index;showMessage(message);};renderIcons(row);return row;
}
function renderMessageWindow(force=false){
  const list=msgsEl,total=currentMessageRows.length,viewport=Math.max(list.clientHeight,400),start=Math.max(0,Math.floor(list.scrollTop/messageRowHeight)-MESSAGE_WINDOW_OVERSCAN),end=Math.min(total,Math.ceil((list.scrollTop+viewport)/messageRowHeight)+MESSAGE_WINDOW_OVERSCAN);
  if(!force&&start===messageWindowStart&&end===messageWindowEnd)return;messageWindowStart=start;messageWindowEnd=end;
  const fragment=document.createDocumentFragment(),top=document.createElement('div'),bottom=document.createElement('div');top.className='message-list-spacer';bottom.className='message-list-spacer';top.setAttribute('aria-hidden','true');bottom.setAttribute('aria-hidden','true');top.style.height=`${start*messageRowHeight}px`;bottom.style.height=`${Math.max(0,(total-end)*messageRowHeight)}px`;fragment.appendChild(top);for(let index=start;index<end;index++)fragment.appendChild(createMessageRow(currentMessageRows[index],index));fragment.appendChild(bottom);list.replaceChildren(fragment);
  const sample=list.querySelector('.msg');if(sample)requestAnimationFrame(()=>{if(!sample.isConnected)return;const measured=sample.getBoundingClientRect().height;if(measured>20&&Math.abs(measured-messageRowHeight)>1){const anchor=start,offset=list.scrollTop-start*messageRowHeight;messageRowHeight=measured;list.scrollTop=Math.max(0,anchor*messageRowHeight+offset);messageWindowStart=-1;renderMessageWindow(true);}});
}
function focusMessageAt(index){if(index<0||index>=currentMessageRows.length)return;const top=index*messageRowHeight,bottom=top+messageRowHeight;if(top<msgsEl.scrollTop)msgsEl.scrollTop=top;else if(bottom>msgsEl.scrollTop+msgsEl.clientHeight)msgsEl.scrollTop=Math.max(0,bottom-msgsEl.clientHeight);renderMessageWindow(true);showMessage(currentMessageRows[index]);}
function renderMessageList(rows,title,resetScroll=false){
  lastListRows=rows;lastListTitle=title;
  if(conversationsEnabled)rows=collapseConversations(rows);
  currentMessageRows=[...rows];const visibleIds=new Set(rows.map(message=>message.id));for(const id of selectedMessageIds)if(!visibleIds.has(id))selectedMessageIds.delete(id);if(lastSelectedMessageIndex>=rows.length)lastSelectedMessageIndex=-1;if(resetScroll)msgsEl.scrollTop=0;messageWindowStart=-1;messageWindowEnd=-1;
  const heading=document.querySelector('.listhead h2');if(heading)heading.textContent=title||messagesTitle();renderMessageWindow(true);updateSelectionUi();
  if(!rows.length)document.getElementById('tbody').innerHTML=`<div class="mail-empty"><h2>${wizardLocale==='en'?'No messages':'Писем нет'}</h2></div>`;
  else if(!activeMessage||!rows.some(message=>message.id===activeMessage.id))document.getElementById('tbody').innerHTML=`<div class="mail-empty"><h2>${wizardLocale==='en'?'Select a message':'Выберите письмо'}</h2></div>`;
}
async function showMessage(message){
  activeMessage=message;
  document.getElementById('tSubject').textContent=message.subject||'';const body=document.getElementById('tbody');
  body.innerHTML=`<div class="mail-loading">${L('Загрузка письма…','Loading message…')}</div>`;
  document.querySelectorAll('.msg').forEach(row=>row.classList.toggle('active',+row.dataset.messageId===message.id));
  try{
    const full=await window.tm?.getMessage(message.id);activeFullMessage=full;body.innerHTML='';const article=document.createElement('article');article.className='mail-content';
    const head=document.createElement('header');head.innerHTML=`<div class="mail-fromline">${L('От:','From:')} <b class="mail-from"></b> <span class="mail-address"></span></div>`;
    const fromName=full.meta.from?.name||'',fromEmail=full.meta.from?.email||'';
    head.querySelector('.mail-from').textContent=fromName||fromEmail;
    head.querySelector('.mail-address').textContent=fromName&&fromEmail?`(${fromEmail})`:'';
    const ccList=(full.meta.cc||[]).map(address=>address.name||address.email).filter(Boolean);
    if(ccList.length){const line=document.createElement('div');line.className='mail-ccline';const shown=ccList.length<=3?ccList.join(', '):L(`${ccList.slice(0,3).join(', ')} и ещё ${ccList.length-3}`,`${ccList.slice(0,3).join(', ')} and ${ccList.length-3} more`);line.textContent=L(`Копия: ${shown}`,`Cc: ${shown}`);line.title=ccList.join(', ');head.appendChild(line);}
    const content=document.createElement('div');content.className='mail-body';if(full.body_html)await renderHtmlMessage(content,full.body_html,full.meta.from?.email);else{content.classList.add('plain');content.textContent=full.body_text||full.meta.preview||'';}
    article.append(head);if(full.attachments?.length){article.appendChild(buildAttachmentBar(full,message.id));}article.appendChild(content);body.appendChild(article);if(!message.flags?.seen){message.flags.seen=true;document.querySelector(`.msg[data-message-id="${message.id}"]`)?.classList.remove('unread');window.tm?.markSeen(message.id,true).catch(console.error);}
  }catch(error){body.innerHTML='';const err=document.createElement('div');err.className='mail-error';err.textContent=error.message||String(error);body.appendChild(err);}
}
function smartMessageValue(message,field){const folder=coreFolders.find(item=>item.id===message.folder_id);switch(field){
  case 'sender':return `${message.from?.name||''} ${message.from?.email||''}`.trim();case 'recipient':return [...(message.to||[]),...(message.cc||[])].map(address=>`${address.name||''} ${address.email||''}`.trim()).join(' ');case 'subject':return message.subject||'';case 'body':return message.preview||'';case 'account':return coreAccounts.find(account=>account.id===message.account_id)?.email||'';case 'folder':return `${folder?.display_name||''} ${folder?.remote_path||''}`.trim();case 'folder_role':return folder?.role||'other';case 'read_state':return message.flags?.seen?'read':'unread';case 'importance':return message.flags?.flagged?'flagged':'normal';case 'reply_state':return message.flags?.answered?'answered':'unanswered';case 'draft_state':return message.flags?.draft?'draft':'not_draft';case 'attachment':return message.has_attachments?'has':'none';case 'size':return message.size;case 'label':return (message.labels||[]).join(' ');case 'date':return message.date||'';default:return '';}}
function smartConditionMatches(message,source){const condition=normalizeSmartCondition(source);if(!validSmartCondition(condition))return false;const field=smartField(condition.f),raw=smartMessageValue(message,field.id);if(field.type==='date'){const timestamp=new Date(raw).getTime();if(!Number.isFinite(timestamp))return false;if(['within_last','older_than'].includes(condition.o)){const amount=Number(condition.v),multipliers={minutes:60000,hours:3600000,days:86400000,weeks:604800000},threshold=Date.now()-amount*(multipliers[condition.u]||multipliers.hours);return condition.o==='within_last'?timestamp>=threshold:timestamp<threshold;}const target=new Date(`${condition.v}T00:00:00`).getTime();if(!Number.isFinite(target))return false;const next=target+86400000;if(condition.o==='before')return timestamp<target;if(condition.o==='after')return timestamp>=next;return timestamp>=target&&timestamp<next;}
  if(field.type==='size'){const bytes=Number(raw);if(!Number.isFinite(bytes))return false;const multipliers={kb:1024,mb:1048576,gb:1073741824},factor=multipliers[condition.u]||multipliers.mb,min=Number(condition.v)*factor,max=Number(condition.v2)*factor;if(condition.o==='greater_than')return bytes>min;if(condition.o==='greater_or_equal')return bytes>=min;if(condition.o==='less_than')return bytes<min;if(condition.o==='less_or_equal')return bytes<=min;if(condition.o==='between')return bytes>=min&&bytes<=max;return bytes===min;}
  const left=String(raw).toLocaleLowerCase(),right=String(condition.v).toLocaleLowerCase();if(condition.o==='not_contains')return !left.includes(right);if(condition.o==='equals')return left===right;if(condition.o==='not_equals')return left!==right;if(condition.o==='starts_with')return left.startsWith(right);if(condition.o==='ends_with')return left.endsWith(right);return left.includes(right);}
function smartRowsForFolder(folder){const groups=(folder?.groups||[]).map(normalizeSmartGroup).filter(group=>group.conditions.length);if(!groups.length)return [];return messages.filter(message=>window.coreUnifiedSettings?.[message.folder_id]!=='0'&&groups.some(group=>group.logic==='any'?group.conditions.some(condition=>smartConditionMatches(message,condition)):group.conditions.every(condition=>smartConditionMatches(message,condition))));}
const coreSmartRows=new Map();
function smartRows(index){const folder=smartFolders[index];return coreSmartRows.get(folder?.id)||smartRowsForFolder(folder);}
async function loadCompleteSmartCoverage(index){
  if(loadingSmartCoverage){queuedSmartCoverageIndex=index;return;}const folder=smartFolders[index];if(!folder)return;loadingSmartCoverage=true;
  try{const rows=await window.tm.listSmartFolderMessages(folder.id,20000);coreSmartRows.set(folder.id,rows);const byId=new Map(messages.map(message=>[message.id,message]));rows.forEach(message=>byId.set(message.id,message));messages=[...byId.values()];
  }catch(error){console.error('smart folder coverage',error);}finally{loadingSmartCoverage=false;if(currentSmartIndex===index&&currentFolderId===null)applyListOptions(false);if(smartOverlay.classList.contains('open'))updateSmartPreview();const queued=queuedSmartCoverageIndex;queuedSmartCoverageIndex=null;if(queued!==null&&queued!==index)loadCompleteSmartCoverage(queued);}
}
function filterSmart(index,resetScroll=true){currentSmartIndex=index;currentFolderId=null;applyListOptions(resetScroll,smartFolderTitle(smartFolders[index])||messagesTitle());loadCompleteSmartCoverage(index);}

window.renderCoreAccounts=function(accounts,foldersByAccount,loadedMessages=[],contacts=[],calendarData={calendars:[],events:[]},savedSmartFolders=[],storage=null){
  const previousFolder=currentFolderId,previousMessageId=activeMessage?.id,navScroll=document.querySelector('.nav')?.scrollTop||0,messageScroll=msgsEl.scrollTop;let previousSmart=currentSmartIndex;
  window.clearDemoData(true);
  coreAccounts=accounts;coreFolders=foldersByAccount.flat();messages=loadedMessages;coreContacts=contacts;coreCalendarData=calendarData;
  coreSmartRows.clear();if(savedSmartFolders.length){const activeId=smartFolders[previousSmart]?.id;smartFolders.splice(0,smartFolders.length,...normalizedSmartFolders(savedSmartFolders.map(smartFolderFromCore)));if(activeId){const restored=smartFolders.findIndex(folder=>folder.id===activeId);if(restored>=0)previousSmart=restored;}renderSmartManagement();bindSmartNavigation();}
  renderRulesList();
  const accountCount=document.getElementById('mailAccountCount');if(accountCount){const n=accounts.length,label=wizardLocale==='en'?(n===1?'account':'accounts'):(n%10===1&&n%100!==11?'аккаунт':n%10>=2&&n%10<=4&&(n%100<10||n%100>=20)?'аккаунта':'аккаунтов');accountCount.textContent=`${n} ${label}`;}
  coreFolders.forEach(folder=>folderHasMore.set(folder.id,messages.filter(message=>message.folder_id===folder.id).length===MESSAGE_PAGE_SIZE));
  const labels=[...document.querySelectorAll('.nav .navlabel')];
  const accountsLabel=document.querySelector('.nav [data-navlabel="accounts"]')||labels.find(el=>el.textContent.includes('Аккаунты'))||labels[1];
  let anchor=accountsLabel;
  accounts.forEach((account,index)=>{
    const header=document.createElement('button');header.type='button';header.className='acc-h open';
    const initial=(account.display_name||account.email||'?').trim()[0].toUpperCase();
    header.innerHTML=`<span class="ava" style="background:${accountColorById(account.id)}"></span><span class="em"></span><span class="chev"><i data-i="chevR"></i></span>`;
    header.querySelector('.ava').textContent=initial;header.querySelector('.em').textContent=account.email;
    anchor.after(header);anchor=header;
    const sub=document.createElement('div');sub.className='acc-sub open';
    const accountFolders=sortedFolders(foldersByAccount[index]||[]);
    accountFolders.forEach(folder=>{const row=document.createElement('button');row.type='button';row.className='navitem folder-row';row.dataset.folderId=folder.id;
      const icon=folderIcon(folder);const depth=Math.max(0,(folder.remote_path.match(/[\/|]/g)||[]).length);row.style.paddingLeft=`${14+depth*14}px`;
      row.innerHTML=`<i data-i="${icon}"></i><span class="folder-name"></span>${folder.unread_count?'<span class="count"></span>':''}`;
      row.querySelector('.folder-name').textContent=folderTitle(folder);if(folder.unread_count)row.querySelector('.count').textContent=folder.unread_count;
      const openFolder=()=>{goMail();document.querySelectorAll('.navitem').forEach(item=>item.classList.remove('active'));row.classList.add('active');currentFolderId=folder.id;currentSmartIndex=null;applyListOptions(true,folderTitle(folder));};row.onclick=openFolder;row.oncontextmenu=event=>{event.preventDefault();event.stopPropagation();contextFolder=folder;contextFolderOpen=openFolder;ctxfolder.dataset.system=folder.role?'true':'false';ctxfolder.querySelectorAll('[data-folder-action="rename"],[data-folder-action="delete"]').forEach(item=>item.classList.toggle('disabled',Boolean(folder.role)));ctxfolder.style.left=`${Math.min(event.clientX,innerWidth-250)}px`;ctxfolder.style.top=`${Math.min(event.clientY,innerHeight-190)}px`;ctxfolder.classList.add('open');};row.ondragover=event=>{event.preventDefault();event.dataTransfer.dropEffect='move';row.classList.add('drop-hi');};row.ondragleave=event=>{if(!row.contains(event.relatedTarget))row.classList.remove('drop-hi');};row.ondrop=event=>{event.preventDefault();row.classList.remove('drop-hi');try{moveMessagesByDrop(JSON.parse(event.dataTransfer.getData('application/x-truemail-messages')),folder);}catch(_){}};sub.appendChild(row);});
    anchor.after(sub);anchor=sub;
  });
  renderIcons(document.querySelector('.nav'));
  if(previousFolder!==null&&coreFolders.some(folder=>folder.id===previousFolder)){
    currentFolderId=previousFolder;currentSmartIndex=null;const folder=coreFolders.find(item=>item.id===previousFolder);document.querySelector(`.folder-row[data-folder-id="${previousFolder}"]`)?.classList.add('active');applyListOptions(false,folderTitle(folder));
  }else filterSmart(previousSmart??0,false);
  if(previousMessageId&&messages.some(message=>message.id===previousMessageId)){activeMessage=messages.find(message=>message.id===previousMessageId);document.querySelector(`.msg[data-message-id="${previousMessageId}"]`)?.classList.add('active');}else if(previousMessageId){activeMessage=null;activeFullMessage=null;document.getElementById('tSubject').textContent='';document.getElementById('tbody').innerHTML=`<div class="mail-empty"><h2>${wizardLocale==='en'?'Select a message':'Выберите письмо'}</h2></div>`;}
  if(messages.length)document.querySelector('.thread .actions')?.classList.remove('hidden');
  renderContacts(contacts);
  renderCalendarData(calendarData);
  renderAccountSettings(accounts,foldersByAccount,calendarData.calendars||[]);
  if(storage)applyStorageStatus(storage);
  if(Object.keys(uiCatalog).length)applyUiCatalog(uiCatalog);
  requestAnimationFrame(()=>{const nav=document.querySelector('.nav');if(nav)nav.scrollTop=navScroll;msgsEl.scrollTop=messageScroll;});
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

