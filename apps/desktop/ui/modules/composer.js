// truemail UI module: composer.js
/* composer: отправка, форматирование, вложения и автосохранение */
const composeEl=document.querySelector('.compose'),compAtt=document.getElementById('compAtt'),compEditEl=document.getElementById('compEdit');
let composerAttachments=[];
const composerFieldIds=['compTo','compCc','compBcc','compSubj'];
function splitAddresses(value){return String(value||'').split(/[;,\n]+/).map(item=>item.trim()).filter(Boolean);}
/* получатели в виде плашек: модель на каждое поле, X удаляет, hover показывает контакт */
const recipientModel={compTo:[],compCc:[],compBcc:[]};
function parseRecipient(raw){const value=String(raw||'').trim();if(!value)return null;const m=value.match(/^(.*?)[<(]([^>)]+)[>)]\s*$/);if(m){const email=m[2].trim(),name=m[1].trim().replace(/^["']|["']$/g,'').trim();return {name:name&&name.toLowerCase()!==email.toLowerCase()?name:'',email};}return {name:'',email:value};}
function recipientDisplay(entry){return entry.name&&entry.name.toLowerCase()!==entry.email.toLowerCase()?entry.name:entry.email;}
function recipientFormat(entry){return entry.name&&entry.name.toLowerCase()!==entry.email.toLowerCase()?`${entry.name} <${entry.email}>`:entry.email;}
function recipientChipTitle(entry){const contact=coreContacts.find(c=>(c.emails||[]).some(item=>String(item.email||'').toLowerCase()===entry.email.toLowerCase()));const parts=[];const name=contact?.display_name||entry.name;if(name)parts.push(name);parts.push(entry.email);if(contact){(contact.phones||[]).forEach(p=>{const num=p.number||p.phone||p;if(num)parts.push(String(num));});if(contact.org)parts.push(contact.org);}return parts.join('\n');}
function renderRecipientChips(id){const input=document.getElementById(id),box=input.parentElement.querySelector('.recipient-chips');if(!box)return;box.innerHTML='';recipientModel[id].forEach((entry,index)=>{const chip=document.createElement('span');chip.className='rcpt-chip'+(validAddress(entry.email)?'':' invalid');chip.title=recipientChipTitle(entry);const label=document.createElement('span');label.className='rcpt-chip-t';label.textContent=recipientDisplay(entry);const close=document.createElement('button');close.type='button';close.className='rcpt-x';close.setAttribute('aria-label',L('Удалить получателя','Remove recipient'));close.innerHTML='&times;';close.onclick=()=>removeRecipientEntry(id,index);chip.appendChild(label);chip.appendChild(close);box.appendChild(chip);});}
function addRecipientEntry(id,raw){const entry=parseRecipient(raw);if(!entry||!entry.email)return false;if(recipientModel[id].some(e=>e.email.toLowerCase()===entry.email.toLowerCase()))return false;recipientModel[id].push(entry);renderRecipientChips(id);return true;}
function removeRecipientEntry(id,index){recipientModel[id].splice(index,1);renderRecipientChips(id);scheduleDraftSave();document.getElementById(id)?.focus();}
function commitRecipientInput(id){const input=document.getElementById(id);let added=false;splitAddresses(input.value).forEach(part=>{if(addRecipientEntry(id,part))added=true;});input.value='';if(added)scheduleDraftSave();return added;}
function setRecipients(id,list){recipientModel[id]=[];(Array.isArray(list)?list:splitAddresses(list)).forEach(item=>{if(typeof item==='string')addRecipientEntry(id,item);else if(item&&item.email){if(!recipientModel[id].some(e=>e.email.toLowerCase()===item.email.toLowerCase()))recipientModel[id].push({name:item.name||'',email:item.email});}});renderRecipientChips(id);}
function recipientFieldAddresses(id){const input=document.getElementById(id);const list=recipientModel[id].map(recipientFormat);splitAddresses(input.value).forEach(part=>list.push(part));return list;}
function validAddress(value){return /^[^\s<>@]+@[^\s<>@]+\.[^\s<>@]+$/.test(value)||/^.+\s<[^\s<>@]+@[^\s<>@]+\.[^\s<>@]+>$/.test(value);}
function setRecipientFieldVisible(id,visible,focus=false){const field=document.querySelector(`[data-recipient-field="${id}"]`);if(!field)return;field.classList.toggle('hidden',!visible);if(focus&&visible)document.getElementById(id)?.focus();}
document.querySelectorAll('[data-recipient-toggle]').forEach(button=>button.onclick=()=>setRecipientFieldVisible(button.dataset.recipientToggle,true,true));
document.querySelectorAll('[data-recipient-hide]').forEach(button=>button.onclick=()=>{const id=button.dataset.recipientHide;if(document.getElementById(id).value.trim()&&!confirm(L('Очистить адреса в этом поле?','Clear addresses in this field?')))return;document.getElementById(id).value='';setRecipientFieldVisible(id,false);scheduleDraftSave();});
/* Каждый сброс композера - новое письмо: вложение, дочитанное после этого,
   уже не наше и в новое письмо не попадает. */
let composerGeneration=0;
function resetComposer(){composerGeneration++;composerFieldIds.forEach(id=>document.getElementById(id).value='');['compTo','compCc','compBcc'].forEach(id=>{recipientModel[id]=[];renderRecipientChips(id);});setRecipientFieldVisible('compCc',false);setRecipientFieldVisible('compBcc',false);document.querySelectorAll('.recipient-suggestions').forEach(menu=>menu.classList.remove('open'));compEditEl.innerHTML='';composerAttachments=[];compAtt.innerHTML='';document.getElementById('composeStatus').textContent='';document.getElementById('compSendAt').classList.add('hidden');}
const signatureCache=new Map();let composerSignatureKind='new';
async function accountSignatures(accountId,refresh=false){if(!refresh&&signatureCache.has(accountId))return signatureCache.get(accountId);const values=await window.tm.listSignatures(accountId);signatureCache.set(accountId,values);return values;}
async function applyComposerSignature(kind=composerSignatureKind){composerSignatureKind=kind;compEditEl.querySelector('.composer-signature')?.remove();const accountId=Number(document.querySelector('.from-sel')?.value);if(!accountId)return;try{const signature=(await accountSignatures(accountId)).find(item=>item.kind===kind&&item.enabled&&item.body_html.trim());if(!signature)return;const node=document.createElement('div');node.className='composer-signature';node.innerHTML=signature.body_html;const quote=compEditEl.querySelector('.mail-quote-head');if(quote)compEditEl.insertBefore(node,quote);else compEditEl.appendChild(node);scheduleDraftSave();}catch(error){console.error(error);}}
async function openComposerForMessage(action){if(!activeMessage)return;resetComposer();
  // Отвечаем/пересылаем с того ящика, на который пришло письмо.
  const fromSel=document.querySelector('.from-sel');if(fromSel&&activeMessage.account_id&&[...fromSel.options].some(opt=>opt.value===String(activeMessage.account_id)))fromSel.value=String(activeMessage.account_id);
  const reply=action!=='forward',from=activeFullMessage?.meta?.from?.email||activeMessage.from?.email||'',subject=activeMessage.subject||'',prefix=action==='forward'?'Fwd: ':'Re: ';document.getElementById('compTitle').textContent=action==='forward'?L('Переслать','Forward'):L('Ответить','Reply');document.getElementById('compSubj').value=new RegExp(`^${prefix}`,'i').test(subject)?subject:prefix+subject;if(reply&&from)setRecipients('compTo',[{name:activeFullMessage?.meta?.from?.name||'',email:from}]);if(action==='replyall'){const own=new Set(coreAccounts.map(account=>account.email.toLowerCase()));const others=[...(activeFullMessage?.meta?.to||[]),...(activeFullMessage?.meta?.cc||[])].filter(address=>address.email&&!own.has(address.email.toLowerCase())&&address.email.toLowerCase()!==from.toLowerCase());const seen=new Set();const uniq=others.filter(a=>{const k=a.email.toLowerCase();if(seen.has(k))return false;seen.add(k);return true;});setRecipients('compCc',uniq.map(a=>({name:a.name||'',email:a.email})));setRecipientFieldVisible('compCc',uniq.length>0);}const dateStr=activeMessage.date?new Date(activeMessage.date).toLocaleString(document.documentElement.lang):'';const bodyHtml=activeFullMessage?.body_html,bodyText=activeFullMessage?.body_text||activeMessage.preview||'';const quote=bodyHtml?bodyHtml:escapeHtml(bodyText).replace(/\n/g,'<br>');const header=`${escapeHtml(dateStr)}${dateStr?', ':''}${escapeHtml(activeFullMessage?.meta?.from?.name||from)} &lt;${escapeHtml(from)}&gt;:`;compEditEl.innerHTML=`<p><br></p><div class="mail-quote-head" style="color:var(--text-3,#888)">${header}</div><blockquote style="margin:6px 0 0;padding:0 0 0 12px;border-left:2px solid var(--border,#ccc)">${quote}</blockquote>`;showView('composeView');await applyComposerSignature('reply');const range=document.createRange(),sel=window.getSelection();range.setStart(compEditEl.firstChild,0);range.collapse(true);sel.removeAllRanges();sel.addRange(range);compEditEl.focus();}
function contactAddresses(){const seen=new Set(),result=[];coreContacts.forEach(contact=>(contact.emails||[]).forEach(item=>{const email=String(item.email||'').trim(),key=email.toLocaleLowerCase();if(!email||seen.has(key))return;seen.add(key);result.push({name:contact.display_name||'',email});}));return result;}
// Ключи транслитерации по адресу (S-012 person-search-translit.md): контакт
// пересчитывается при каждом вводе, поэтому кэш ведём по email, а не по ссылке
// на объект. Сбрасывается в reloadCoreData (mail.js) вместе с coreContacts.
const composerContactKeysCache=personSearch.createPersonSearchCache();
window.invalidateComposerContactCache=()=>composerContactKeysCache.invalidate();
function recipientToken(value){return String(value||'').split(/[;,]/).at(-1).trim();}
function chooseRecipient(input,contact){addRecipientEntry(input.id,recipientFormat({name:contact.name,email:contact.email}));input.value='';input.dispatchEvent(new Event('input',{bubbles:true}));input.focus();scheduleDraftSave();}
['compTo','compCc','compBcc'].forEach(id=>{const input=document.getElementById(id),menu=input.parentElement.querySelector('.recipient-suggestions');let active=-1;const render=()=>{const query=recipientToken(input.value),used=new Set([...recipientModel[id].map(entry=>entry.email.toLocaleLowerCase()),...splitAddresses(input.value).map(value=>(value.match(/<([^>]+)>/)?.[1]||value).trim().toLocaleLowerCase())]),matches=personSearch.suggestRecipients(contactAddresses(),query,used,8,contact=>composerContactKeysCache.get(contact.email.toLocaleLowerCase(),()=>`${contact.name} ${contact.email}`));active=-1;menu.innerHTML='';matches.forEach((contact,index)=>{const option=document.createElement('button');option.type='button';option.className='recipient-option';option.innerHTML='<span></span><small></small>';option.querySelector('span').textContent=contact.name||contact.email;option.querySelector('small').textContent=contact.email;option.onmousedown=event=>{event.preventDefault();chooseRecipient(input,contact);menu.classList.remove('open');};option.dataset.index=index;menu.appendChild(option);});menu.classList.toggle('open',matches.length>0);};input.addEventListener('input',render);input.addEventListener('focus',render);input.addEventListener('keydown',event=>{const options=[...menu.querySelectorAll('.recipient-option')];if((event.key===','||event.key===';')&&!(active>=0&&options.length)){event.preventDefault();commitRecipientInput(id);menu.classList.remove('open');render();return;}if(event.key==='Backspace'&&!input.value&&recipientModel[id].length){event.preventDefault();removeRecipientEntry(id,recipientModel[id].length-1);return;}if(!options.length){if(event.key==='Enter'&&input.value.trim()){event.preventDefault();commitRecipientInput(id);}return;}if(event.key==='ArrowDown'||event.key==='ArrowUp'){event.preventDefault();active=(active+(event.key==='ArrowDown'?1:-1)+options.length)%options.length;options.forEach((option,index)=>option.classList.toggle('active',index===active));options[active].scrollIntoView({block:'nearest'});}else if(event.key==='Enter'){event.preventDefault();if(active>=0)options[active].dispatchEvent(new MouseEvent('mousedown',{bubbles:true}));else{commitRecipientInput(id);menu.classList.remove('open');}}else if(event.key==='Escape')menu.classList.remove('open');});input.addEventListener('blur',()=>{if(input.value.trim())commitRecipientInput(id);});});
document.addEventListener('click',event=>{if(!event.target.closest('.recipient-input'))document.querySelectorAll('.recipient-suggestions').forEach(menu=>menu.classList.remove('open'));});
function showToast(message,actionLabel,action){document.querySelector('.app-toast')?.remove();const toast=document.createElement('div');toast.className='app-toast';const text=document.createElement('span');text.textContent=message;toast.appendChild(text);if(action){const button=document.createElement('button');button.type='button';button.textContent=actionLabel;button.onclick=async()=>{button.disabled=true;await action();toast.remove();};toast.appendChild(button);}document.body.appendChild(toast);setTimeout(()=>toast.remove(),9000);}
window.handleSyncState=function(state){if(!state)return;const info=document.getElementById('calSyncInfo');if(info&&['dav','auxiliary'].includes(state.scope)){if(state.status==='syncing')info.textContent=wizardLocale==='en'?'Syncing calendars, tasks and contacts…':'Синхронизация календарей, задач и контактов…';else if(state.status==='error')info.textContent=wizardLocale==='en'?'Calendar, tasks and contacts sync error':'Ошибка синхронизации календаря, задач и контактов';}const message=state.status==='error'?(state.error||L('Ошибка синхронизации','Sync error')):(state.warnings?.join(' ')||'');if(!message)return;if(/ACCESS_TOKEN_SCOPE_INSUFFICIENT|insufficient authentication scopes|insufficientPermissions/i.test(message)){const account=coreAccounts.find(item=>item.id===Number(state.account_id));showToast(L('Google не выдал приложению доступ к календарю, контактам и задачам. Переподключите аккаунт и подтвердите все запрошенные разрешения.','Google did not grant access to calendar, contacts and tasks. Reconnect the account and approve all requested permissions.'),L('Переподключить','Reconnect'),()=>showAccountWizard(account?.email||''));return;}showToast(message);};
async function performMessageActionForIds(action,ids){if(!ids.length){showToast(L('Сначала выберите письмо','Select a message first'));return;}
  ids=window.expandConversationIds?window.expandConversationIds(ids):ids;
  if(action==='trash'&&ids.length>10&&!await confirmAction(L(`Удалить ${ids.length} писем?`,`Delete ${ids.length} messages?`)))return;
  // Запоминаем соседнее письмо, чтобы после действия перейти к нему, а не терять фокус.
  let nextId=null;
  if(activeMessage&&ids.length===1){const index=currentMessageRows.findIndex(message=>message.id===activeMessage.id);nextId=currentMessageRows[index+1]?.id??currentMessageRows[index-1]?.id??null;}
  try{const queued=await window.tm.messageAction(ids,action);selectedMessageIds.clear();activeMessage=null;activeFullMessage=null;window.forgetMessages?.(ids);await window.reloadCoreData();
    if(nextId!=null){const message=messages.find(item=>item.id===nextId);if(message)showMessage(message);}
    showToast(action==='archive'?L('Письмо перемещено в архив','Message moved to Archive'):action==='spam'?L('Письмо перемещено в спам','Message moved to Spam'):L('Письмо перемещено в корзину','Message moved to Trash'),L('Отменить','Undo'),async()=>{await window.tm.undoMessageAction(queued.operation_ids);await window.reloadCoreData();});}catch(error){showToast(error.message||String(error));}}
window.performMessageActionForIds=performMessageActionForIds;
async function performMessageAction(action){const ids=selectedMessageIds.size?[...selectedMessageIds]:activeMessage?[activeMessage.id]:[];return performMessageActionForIds(action,ids);}
function selectAllCurrentMessages(){currentMessageRows.forEach(message=>selectedMessageIds.add(message.id));updateSelectionUi();}
document.getElementById('bulkSelectAll').onclick=selectAllCurrentMessages;
document.getElementById('bulkClear').onclick=clearMessageSelection;
document.getElementById('bulkArchive').onclick=()=>performMessageAction('archive');
document.getElementById('bulkTrash').onclick=()=>performMessageAction('trash');
document.getElementById('bulkRead').onclick=async()=>{const ids=[...selectedMessageIds];if(!ids.length)return;try{await window.markMessagesSeen?.(ids.map(id=>messages.find(item=>item.id===id)).filter(Boolean),true);clearMessageSelection();await window.reloadCoreData();showToast(L('Письма отмечены прочитанными','Messages marked as read'));}catch(error){showToast(error.message||String(error));}};
function renderComposerAttachment(item){const el=document.createElement('span');el.className='att-mini';el.innerHTML='<i data-i="paperclip"></i><span class="att-name"></span><span class="csub"></span><span class="x">×</span>';el.querySelector('.att-name').textContent=item.filename;el.querySelector('.csub').textContent=formatBytes(item.data.length);renderIcons(el);el.querySelector('.x').onclick=()=>{composerAttachments=composerAttachments.filter(value=>value!==item);el.remove();scheduleDraftSave();};compAtt.appendChild(el);}
/* Потолок на всё письмо разом - вложения и байты встроенных картинок в теле
   вместе (S-007, S-018, S-038; тот же предел, что и при сборке письма в ядре):
   WebView держит их в памяти массивами чисел и целиком укладывает в
   автосохраняемый черновик, поэтому крупное содержимое иначе съедает память
   интерфейса. */
function composerBodyImageSources(){return [...compEditEl.querySelectorAll('img')].map(img=>img.getAttribute('src')||'');}
function composerTotalBytes(){return composerBody.totalMessageBytes(composerAttachments.map(item=>item.data.length),composerBodyImageSources());}
function ensureMessageFits(size,filename){
  if(composerBody.fitsMessageLimit(composerTotalBytes(),size))return;
  throw new Error(L(`Не добавлено: ${filename} не помещается, всё письмо вместе не должно превышать ${formatBytes(composerBody.MAX_MESSAGE_BYTES)}`,`Not attached: ${filename} does not fit, the whole message together must stay under ${formatBytes(composerBody.MAX_MESSAGE_BYTES)}`));
}
async function addCompFile(file,generation=composerGeneration){if(generation!==composerGeneration)return;ensureMessageFits(file.size,file.name||'attachment');const data=Array.from(new Uint8Array(await file.arrayBuffer()));if(generation!==composerGeneration)return;const item={filename:file.name||'attachment',mime_type:file.type||'application/octet-stream',data};composerAttachments.push(item);renderComposerAttachment(item);scheduleDraftSave();}
/* файл с диска по пути: приходит из меню "Отправить" проводника, читает ядро.
   Размер оцениваем по длине base64 - до atob, чтобы слишком большой файл не
   разворачивался в памяти интерфейса ещё раз. */
async function addCompFilePath(path,generation=composerGeneration){if(generation!==composerGeneration)return;const file=await window.tm.readLocalFile(path);if(generation!==composerGeneration)return;ensureMessageFits(Math.floor(file.base64.length*3/4),file.filename);const binary=atob(file.base64);const bytes=new Array(binary.length);for(let i=0;i<binary.length;i++)bytes[i]=binary.charCodeAt(i);const item={filename:file.filename,mime_type:file.mime_type||'application/octet-stream',data:bytes};composerAttachments.push(item);renderComposerAttachment(item);scheduleDraftSave();}
/* Есть ли в композере что терять: открытое письмо или восстановленный при
   запуске черновик, который лежит в полях ещё до открытия composeView.
   Тело из одних картинок без текста тоже считается непустым (S-012). */
function composerHasContent(){
  if(window.pendingComposerDraft)return true;
  if(composerAttachments.length)return true;
  if(['compTo','compCc','compBcc'].some(id=>recipientModel[id].length||document.getElementById(id).value.trim()))return true;
  if(document.getElementById('compSubj').value.trim())return true;
  const body=compEditEl.cloneNode(true);body.querySelector('.composer-signature')?.remove();
  return Boolean(body.textContent.trim())||composerBody.htmlHasImageTag(body.innerHTML);
}
/* "Отправить -> truemail" в проводнике. Написанное не трогаем: сброс уничтожил
   бы и открытое письмо, и восстановленный черновик, а следующее автосохранение
   затёрло бы его в настройках - файлы просто добавляются к тому, что есть.
   Вызовы выстраиваем в цепочку, иначе два подряд события из проводника
   перемешают вложения и собьют друг другу композер. */
async function attachFilesToComposer(paths){
  const composing=document.getElementById('composeView')?.classList.contains('active');
  const keep=composing||composerHasContent();
  // Пока файлы ждали очереди, пользователь мог открыть письмо или ответ:
  // вложения ложатся туда, но молча это делать нельзя.
  if(composing)showToast(L('Файлы добавлены к открытому письму','Files added to the open message'));
  if(!keep){
    resetComposer();document.getElementById('compTitle').textContent=L('Новое письмо','New message');
    showView('composeView');
    await applyComposerSignature('new');
  }else if(!composing)showView('composeView');
  const generation=composerGeneration;
  for(const path of paths){
    if(generation!==composerGeneration)return;
    try{await addCompFilePath(path,generation);}catch(error){showToast(error.message||String(error));}
  }
  if(!composing)document.getElementById('compTo')?.focus();
}
/* Все добавления вложений идут одной очередью: параллельные вызовы считали бы
   общий размер по одному и тому же старому значению и вместе перебирали лимит,
   а два письма из проводника подряд сбивали бы друг другу композер. */
let attachChain=Promise.resolve();
/* Дождаться, пока очередь опустеет целиком: за время ожидания в неё могли
   добавить ещё файл, и хвост, взятый один раз, этого бы не учёл. */
async function settleAttachments(){let chain;do{chain=attachChain;try{await chain;}catch(_){}}while(chain!==attachChain);}
window.composeWithFiles=function(paths){
  attachChain=attachChain.then(()=>attachFilesToComposer(paths)).catch(console.error);
  return attachChain;
};
function queueCompFiles(files){
  // Поколение берём в момент выбора файлов, а не когда до них дойдёт очередь:
  // иначе второй файл дочитался бы уже в другое письмо.
  const generation=composerGeneration;
  for(const file of files)attachChain=attachChain.then(()=>addCompFile(file,generation)).catch(error=>showToast(error.message||String(error)));
  return attachChain;
}
/* Файлы, брошенные в окно (S-015, S-016): то же поведение, что и у "Отправить"
   из проводника (attachFilesToComposer), но содержимое уже готовые File из
   данных переноса, а не путь на диске (S-026). */
async function attachDroppedFiles(files){
  const composing=document.getElementById('composeView')?.classList.contains('active');
  // S-019: письмо открываем, только когда есть чему в нём лечь - хотя бы один
  // файл должен помещаться в предел письма. Иначе пользователь получил бы
  // пустое новое письмо вместо одного сообщения об отказе.
  const fitting=composing?files:files.filter(file=>composerBody.fitsMessageLimit(composerTotalBytes(),file.size));
  if(!composing&&!fitting.length){
    files.forEach(file=>showToast(L(`Не добавлено: ${file.name||'файл'} не помещается, всё письмо вместе не должно превышать ${formatBytes(composerBody.MAX_MESSAGE_BYTES)}`,`Not attached: ${file.name||'file'} does not fit, the whole message together must stay under ${formatBytes(composerBody.MAX_MESSAGE_BYTES)}`)));
    return;
  }
  const keep=composing||composerHasContent();
  if(!keep){
    resetComposer();document.getElementById('compTitle').textContent=L('Новое письмо','New message');
    showView('composeView');
    await applyComposerSignature('new');
  }else if(!composing)showView('composeView');
  const generation=composerGeneration;
  let added=0;
  for(const file of files){
    if(generation!==composerGeneration)return;
    try{await addCompFile(file,generation);added++;}catch(error){showToast(error.message||String(error));}
  }
  // Сообщение после цикла, а не до него: раньше оно обещало добавление даже
  // тогда, когда все файлы отклонены по пределу.
  if(composing&&added)showToast(L('Файлы добавлены к открытому письму','Files added to the open message'));
  // Файл мог не прочитаться уже после того, как письмо открылось: молчать об
  // этом нельзя, иначе пустое новое письмо выглядит сбоем.
  if(!composing&&!added)showToast(L('Письмо открыто, но ни один файл не приложился','The message is open, but no file was attached'));
  if(!composing)document.getElementById('compTo')?.focus();
}
window.dropFilesToComposer=function(files){
  attachChain=attachChain.then(()=>attachDroppedFiles(files)).catch(console.error);
  return attachChain;
};
/* Разбор данных файлового переноса (S-013, S-019, S-023, S-026, S-027):
   содержимое файлов берём из dataTransfer, путь на диске интерфейсу не нужен
   и команда чтения файла по пути не вызывается. Папка в переносе отклоняется
   отдельным сообщением, остальные файлы приложить не мешает (S-019). */
function handleWindowFileDrop(dataTransfer){
  // S-044: до конца мастера настройки или без единого ящика композер не открыть.
  if(!window.tmComposerReady){
    showToast(L('Файлы не приложены: сначала завершите настройку программы','Files were not attached: finish the setup wizard first'));
    return;
  }
  const items=Array.from(dataTransfer?.items||[]).filter(item=>item&&item.kind==='file');
  let folderRejected=false;
  const files=[];
  for(const item of items){
    const entry=item.webkitGetAsEntry?item.webkitGetAsEntry():null;
    if(entry&&entry.isDirectory){folderRejected=true;continue;}
    const file=item.getAsFile();
    if(!file)continue;
    // Там, где признака каталога нет вовсе, папка приходит пустым файлом без
    // типа. Проверяем это только в таком случае: настоящий пустой файл иначе
    // получил бы отказ как папка.
    if(!entry&&!file.size&&!file.type){folderRejected=true;continue;}
    files.push(file);
  }
  if(folderRejected)showToast(L('Папка не приложена к письму: приложены только файлы','A folder was not attached: only files were attached'));
  if(!files.length)return;
  window.dropFilesToComposer(files);
}
/* Обработчики файлового переноса живут на уровне окна, а не области композера
   (S-013): при закрытом письме область скрыта, и события до неё не доходят.
   Подписка в фазе захвата и остановка события при файловом переносе (S-022,
   S-043) - иначе обработчики папок, календаря и перечня кнопок настроек,
   висящие на своих строках без проверки типа переноса, подсветят цель первыми.
   Перенос без Files в данных (внутренний перенос письма/события/строки
   настроек) обработчики окна пропускают дальше без изменений (S-014). */
window.addEventListener('dragover',e=>{
  // S-046: стандартное действие отменяем для любого переноса, включая ссылку
  // и текст из браузера. Без отмены именно здесь событие drop не возникает
  // вовсе, и окно уходит по брошенной ссылке вместо интерфейса программы.
  e.preventDefault();
  if(!composerBody.isFileTransfer(e.dataTransfer?.types))return;
  e.stopPropagation();
  // S-020, S-045: подсветка только пока написание письма открыто.
  if(document.getElementById('composeView')?.classList.contains('active'))composeEl.classList.add('dragover');
},true);
window.addEventListener('dragleave',e=>{
  if(!composerBody.isFileTransfer(e.dataTransfer?.types))return;
  e.stopPropagation();
  // relatedTarget пуст, когда перенос покинул окно целиком, а не просто перешёл
  // на соседний элемент страницы (S-021).
  if(!e.relatedTarget)composeEl.classList.remove('dragover');
},true);
window.addEventListener('drop',e=>{
  // S-046: чужая ссылка, текст или картинка со страницы браузера не должны
  // увести окно с интерфейса программы - отменяем стандартное действие всегда,
  // а не только для файлового переноса.
  e.preventDefault();
  if(!composerBody.isFileTransfer(e.dataTransfer?.types))return;
  e.stopPropagation();
  composeEl.classList.remove('dragover');
  handleWindowFileDrop(e.dataTransfer);
},true);
compEditEl.addEventListener('paste',e=>{
  const items=e.clipboardData&&e.clipboardData.items;
  if(!items)return;
  const {images,rejectedTypes}=composerBody.clipboardImageItems(items);
  if(!images.length&&!rejectedTypes.length)return;
  // S-001, S-004: буфер с картинкой вставляется картинкой, а не стандартной
  // вставкой текста и разметки того же буфера. Отменяем стандартную вставку и
  // когда поддерживаемых картинок нет: иначе редактор сам вставил бы картинку
  // неподдерживаемого типа в тело письма молча.
  e.preventDefault();
  rejectedTypes.forEach(type=>showToast(L(`Не вставлено: неподдерживаемый тип картинки (${type})`,`Not inserted: unsupported image type (${type})`)));
  if(!images.length)return;
  // Курсор, поколение композера и сами файлы берём в момент вставки, а не
  // когда до них дойдёт очередь (S-005, S-006, S-009): к этому моменту фокус
  // или письмо могли смениться, а элементы буфера вне своего события файл уже
  // не отдают.
  const generation=composerGeneration;
  const files=images.map(item=>item.getAsFile()).filter(Boolean);
  if(!files.length)return;
  const sel=window.getSelection();
  const range=sel&&sel.rangeCount&&compEditEl.contains(sel.anchorNode)?sel.getRangeAt(0).cloneRange():null;
  attachChain=attachChain.then(()=>insertClipboardImages(files,range,generation)).catch(console.error);
});
/* Читает содержимое картинки из буфера как строку data: (S-001): FileReader,
   а не Tauri API - файл уже есть в памяти, путь на диске не нужен. */
function fileToDataUrl(file){
  return new Promise((resolve,reject)=>{
    const reader=new FileReader();
    reader.onload=()=>resolve(String(reader.result||''));
    reader.onerror=()=>reject(reader.error||new Error('read failed'));
    reader.readAsDataURL(file);
  });
}
/* Вставка в Range с возвратом позиции сразу после вставленного узла - следующая
   картинка той же вставки встаёт сразу за предыдущей, а не поверх неё (S-003). */
/* Вставляем через тот же механизм, каким вставляет сам редактор: тогда
   картинку снимает обычная отмена (Ctrl+Z), как любую другую правку текста.
   Выделение перед этим возвращаем на сохранённое место: пока картинка
   читалась, фокус мог уйти в другое поле. */
function insertImageAtRange(range,html){
  const target=range||defaultInsertRange();
  compEditEl.focus();
  const sel=window.getSelection();
  sel.removeAllRanges();sel.addRange(target);
  document.execCommand('insertHTML',false,html);
  const after=sel.rangeCount?sel.getRangeAt(0).cloneRange():defaultInsertRange();
  after.collapse(false);
  return after;
}
/* Курсора в теле письма может не быть (например, только что открыто новое
   письмо и поле тела ещё не в фокусе) - вставляем в конец тела (S-006: для
   пустого тела конец совпадает с началом). */
function defaultInsertRange(){
  const r=document.createRange();r.selectNodeContents(compEditEl);r.collapse(false);return r;
}
async function insertClipboardImages(files,initialRange,generation){
  if(generation!==composerGeneration)return;
  let range=initialRange,inserted=false;
  for(const file of files){
    if(generation!==composerGeneration)return;
    let dataUrl;
    try{dataUrl=await fileToDataUrl(file);}catch(error){showToast(error.message||String(error));continue;}
    if(generation!==composerGeneration)return;
    const parsed=composerBody.parseDataUrl(dataUrl);
    if(!parsed)continue; // тип не разобрался как поддерживаемая картинка - пропускаем без сообщения, отбор уже прошёл clipboardImageItems
    if(!composerBody.fitsMessageLimit(composerTotalBytes(),parsed.byteLength)){
      // S-007, S-003: эта картинка не вставляется, следующие проверяются на общих основаниях.
      showToast(L(`Не вставлено: картинка не помещается, всё письмо вместе не должно превышать ${formatBytes(composerBody.MAX_MESSAGE_BYTES)}`,`Not inserted: the image does not fit, the whole message together must stay under ${formatBytes(composerBody.MAX_MESSAGE_BYTES)}`));
      continue;
    }
    range=insertImageAtRange(range,composerBody.buildImageTag(parsed.mimeType,dataUrl.slice(dataUrl.indexOf(',')+1)));
    inserted=true;
  }
  // S-010: сохранение черновика запускается сразу после вставки, не дожидаясь
  // следующего нажатия клавиши - программная вставка события ввода не порождает.
  if(inserted)scheduleDraftSave();
}
document.getElementById('compAttach').onclick=()=>document.getElementById('compFile').click();
document.getElementById('compFile').onchange=e=>{queueCompFiles([...e.target.files||[]]);e.target.value='';};
async function openTemplateDialog(){const accountId=Number(document.querySelector('.from-sel')?.value);if(!accountId){showToast(L('Сначала выберите аккаунт','Select an account first'));return;}const overlay=document.createElement('div');overlay.className='overlay open';overlay.innerHTML=`<div class="modal template-modal"><div class="mh"><i data-i="edit"></i><h3>${L('Шаблоны писем','Message templates')}</h3><button class="iconbtn x" type="button"><i data-i="close"></i></button></div><div class="mb"><div class="template-list"></div><div class="template-empty"></div></div><div class="mf"><button class="btn template-save">${L('Сохранить текущее письмо как шаблон','Save current message as template')}</button><span class="sp"></span><button class="btn template-close">${L('Закрыть','Close')}</button></div></div>`;document.body.appendChild(overlay);renderIcons(overlay);const close=()=>overlay.remove();overlay.querySelectorAll('.x,.template-close').forEach(button=>button.onclick=close);overlay.onclick=event=>{if(event.target===overlay)close();};
  const render=async()=>{const values=await window.tm.listMessageTemplates(accountId),list=overlay.querySelector('.template-list'),empty=overlay.querySelector('.template-empty');list.innerHTML='';empty.textContent=values.length?'':L('Шаблонов пока нет.','No templates yet.');values.forEach(template=>{const row=document.createElement('div');row.className='template-row';const text=document.createElement('div');text.className='grow';const name=document.createElement('div');name.className='t';name.textContent=template.name;const subject=document.createElement('div');subject.className='d';subject.textContent=template.subject||L('Без темы','No subject');text.append(name,subject);const apply=document.createElement('button');apply.className='btn sm';apply.textContent=L('Вставить','Apply');apply.onclick=async()=>{document.getElementById('compSubj').value=template.subject||'';compEditEl.innerHTML=template.body_html||'';await applyComposerSignature(composerSignatureKind);scheduleDraftSave();close();};const remove=document.createElement('button');remove.className='iconbtn';remove.title=L('Удалить шаблон','Delete template');remove.innerHTML=ic.trash;remove.onclick=async()=>{if(!await confirmAction(L(`Удалить шаблон «${template.name}»?`,`Delete template "${template.name}"?`)))return;await window.tm.deleteMessageTemplate(template.id,accountId);await render();};row.append(text,apply,remove);list.appendChild(row);});};
  overlay.querySelector('.template-save').onclick=async()=>{const name=prompt(L('Название шаблона','Template name'),document.getElementById('compSubj').value.trim());if(!name?.trim())return;const body=compEditEl.cloneNode(true);body.querySelector('.composer-signature')?.remove();try{await window.tm.saveMessageTemplate({id:null,accountId,name:name.trim(),subject:document.getElementById('compSubj').value,bodyHtml:body.innerHTML});await render();showToast(L('Шаблон сохранён','Template saved'));}catch(error){showToast(error.message||String(error));}};try{await render();}catch(error){close();showToast(error.message||String(error));}}
document.getElementById('compTemplates').onclick=openTemplateDialog;
document.querySelectorAll('[data-format]').forEach(button=>button.onclick=()=>{compEditEl.focus();document.execCommand(button.dataset.format,false);scheduleDraftSave();});
/* вставка ссылки через кастомную модалку: текст + URL, по центру, с сохранением выделения */
let savedLinkRange=null;
function openLinkDialog(){
  const sel=window.getSelection();savedLinkRange=sel&&sel.rangeCount?sel.getRangeAt(0).cloneRange():null;
  const selectedText=savedLinkRange?savedLinkRange.toString():'';
  const overlay=document.getElementById('linkOverlay'),textEl=document.getElementById('linkText'),hrefEl=document.getElementById('linkHref');
  textEl.value=selectedText;hrefEl.value='';
  overlay.classList.add('open');
  (selectedText?hrefEl:textEl).focus();
}
function closeLinkDialog(){document.getElementById('linkOverlay').classList.remove('open');}
function applyLinkDialog(){
  const textEl=document.getElementById('linkText'),hrefEl=document.getElementById('linkHref');
  let href=hrefEl.value.trim();if(!href){hrefEl.focus();return;}
  if(!/^[a-z][a-z0-9+.-]*:/i.test(href))href='https://'+href;
  const text=(textEl.value.trim()||href);
  compEditEl.focus();
  const sel=window.getSelection();sel.removeAllRanges();
  if(savedLinkRange)sel.addRange(savedLinkRange);
  const anchor=document.createElement('a');anchor.href=href;anchor.textContent=text;
  if(savedLinkRange&&!savedLinkRange.collapsed){savedLinkRange.deleteContents();savedLinkRange.insertNode(anchor);}
  else if(savedLinkRange){savedLinkRange.insertNode(anchor);}
  else{compEditEl.appendChild(anchor);}
  const after=document.createRange();after.setStartAfter(anchor);after.collapse(true);sel.removeAllRanges();sel.addRange(after);
  savedLinkRange=null;closeLinkDialog();scheduleDraftSave();
}
document.getElementById('compLink').onclick=openLinkDialog;
document.getElementById('linkClose').onclick=closeLinkDialog;
document.getElementById('linkCancel').onclick=closeLinkDialog;
document.getElementById('linkApply').onclick=applyLinkDialog;
document.getElementById('linkOverlay').addEventListener('click',e=>{if(e.target.id==='linkOverlay')closeLinkDialog();});
document.getElementById('linkHref').addEventListener('keydown',e=>{if(e.key==='Enter'){e.preventDefault();applyLinkDialog();}});
let draftSaveTimer=null;
function draftPayload(){return {account_id:+document.querySelector('.from-sel').value||coreAccounts[0]?.id||0,to:recipientFieldAddresses('compTo').join(', '),cc:recipientFieldAddresses('compCc').join(', '),bcc:recipientFieldAddresses('compBcc').join(', '),subject:document.getElementById('compSubj').value,body_html:compEditEl.innerHTML,body_text:compEditEl.innerText,attachments:composerAttachments};}
function scheduleDraftSave(){clearTimeout(draftSaveTimer);draftSaveTimer=setTimeout(()=>window.tm?.setSetting('composer_draft',JSON.stringify(draftPayload())).catch(console.error),500);}
composerFieldIds.forEach(id=>document.getElementById(id).addEventListener('input',scheduleDraftSave));compEditEl.addEventListener('input',scheduleDraftSave);
function composerRequest(){const draft=draftPayload(),to=splitAddresses(draft.to),cc=splitAddresses(draft.cc),bcc=splitAddresses(draft.bcc),invalid=[...to,...cc,...bcc].find(address=>!validAddress(address));if(!to.length&&!cc.length&&!bcc.length)throw new Error(L('Укажите хотя бы одного получателя','Add at least one recipient'));if(invalid)throw new Error(L(`Некорректный адрес: ${invalid}`,`Invalid address: ${invalid}`));return {account_id:draft.account_id,to,cc,bcc,subject:draft.subject,body_text:draft.body_text,body_html:draft.body_html,attachments:composerAttachments};}
document.getElementById('compSend').onclick=async()=>{
  // Крупный файл может ещё дочитываться: без ожидания письмо ушло бы без него,
  // а вложение легло бы в уже очищенный композер. Если за это время открыли
  // другое письмо, нажатие относилось к прежнему - отправлять нечего.
  const generation=composerGeneration;
  await settleAttachments();
  if(generation!==composerGeneration)return;
  const request=composerRequest();
  // Окно закрываем сразу, письмо уходит в фоне. Итог показываем коротким toast.
  resetComposer();showView('mailView');window.tm.setSetting('composer_draft','').catch(()=>{});
  try{await window.tm.sendMessage(request);showToast(L('Письмо отправлено','Message sent'));}
  catch(error){showToast(error.message||String(error));}
};
document.getElementById('compSendLater').onclick=async()=>{const generation=composerGeneration;await settleAttachments();if(generation!==composerGeneration)return;const input=document.getElementById('compSendAt'),status=document.getElementById('composeStatus');if(input.classList.contains('hidden')){const date=new Date(Date.now()+15*60*1000);date.setSeconds(0,0);input.value=new Date(date.getTime()-date.getTimezoneOffset()*60000).toISOString().slice(0,16);input.min=new Date(Date.now()-new Date().getTimezoneOffset()*60000).toISOString().slice(0,16);input.classList.remove('hidden');input.focus();return;}try{const date=new Date(input.value);if(Number.isNaN(date.getTime()))throw new Error(L('Выберите дату и время','Choose a date and time'));const id=await window.tm.scheduleMessage(composerRequest(),date.toISOString());await window.tm.setSetting('composer_draft','');status.textContent=L(`Запланировано (задача ${id})`,`Scheduled (task ${id})`);status.dataset.kind='success';setTimeout(()=>{resetComposer();showView('mailView');},700);}catch(error){status.textContent=error.message||String(error);status.dataset.kind='error';}};
document.getElementById('compDeleteDraft').onclick=async()=>{resetComposer();await window.tm?.setSetting('composer_draft','').catch(console.error);showView('mailView');};

