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
function setRecipients(id,list){clearRecipientField(id);(Array.isArray(list)?list:splitAddresses(list)).forEach(item=>{if(typeof item==='string')addRecipientEntry(id,item);else if(item&&item.email){if(!recipientModel[id].some(e=>e.email.toLowerCase()===item.email.toLowerCase()))recipientModel[id].push({name:item.name||'',email:item.email});}});renderRecipientChips(id);}
/* Очистка поля получателей одна на всех: адреса живут в двух местах - завершённые
   плашками в recipientModel, недовведённый остаток в строке ввода, - и очистка
   только одного из них незаметно уносила бы второй в письмо. */
function clearRecipientField(id){const input=document.getElementById(id);if(input)input.value='';recipientModel[id]=[];renderRecipientChips(id);}
function recipientFieldHasAddresses(id){const input=document.getElementById(id);return Boolean(recipientModel[id].length||input?.value.trim());}
function recipientFieldAddresses(id){const input=document.getElementById(id);const list=recipientModel[id].map(recipientFormat);splitAddresses(input.value).forEach(part=>list.push(part));return list;}
function validAddress(value){return /^[^\s<>@]+@[^\s<>@]+\.[^\s<>@]+$/.test(value)||/^.+\s<[^\s<>@]+@[^\s<>@]+\.[^\s<>@]+>$/.test(value);}
function setRecipientFieldVisible(id,visible,focus=false){const field=document.querySelector(`[data-recipient-field="${id}"]`);if(!field)return;field.classList.toggle('hidden',!visible);if(focus&&visible)document.getElementById(id)?.focus();}
document.querySelectorAll('[data-recipient-toggle]').forEach(button=>button.onclick=()=>setRecipientFieldVisible(button.dataset.recipientToggle,true,true));
/* Спрашиваем по всем адресам поля, а не по одной строке ввода: завершённый адрес
   виден плашкой, а закрытое без вопроса поле оставляло его в письме - получатель,
   убранный с глаз, письмо всё равно получал. */
document.querySelectorAll('[data-recipient-hide]').forEach(button=>button.onclick=async()=>{const id=button.dataset.recipientHide;if(recipientFieldHasAddresses(id)&&!await confirmAction(L('Очистить адреса в этом поле?','Clear addresses in this field?')))return;clearRecipientField(id);setRecipientFieldVisible(id,false);scheduleDraftSave();});
/* Каждый сброс композера - новое письмо: вложение, дочитанное после этого,
   уже не наше и в новое письмо не попадает. */
let composerGeneration=0;
/* Ключ запроса отправки: повторное нажатие "Отправить" узнаётся по нему и
   второй отправки не создаёт (undo-send.md, S-053). Ключ относится к тому
   содержимому композера, с которым он создан: после правки письма и после
   открытия нового письма он сбрасывается, иначе ядро вернуло бы прежнюю
   операцию, а новое содержимое пропало бы вместе с очищенным композером. */
let composerRequestKey='';
function resetComposer(){composerGeneration++;composerRequestKey='';['compTo','compCc','compBcc'].forEach(clearRecipientField);document.getElementById('compSubj').value='';setRecipientFieldVisible('compCc',false);setRecipientFieldVisible('compBcc',false);document.querySelectorAll('.recipient-suggestions').forEach(menu=>menu.classList.remove('open'));compEditEl.innerHTML='';composerAttachments=[];compAtt.innerHTML='';document.getElementById('composeStatus').textContent='';document.getElementById('compSendAt').classList.add('hidden');}
const signatureCache=new Map();let composerSignatureKind='new';
async function accountSignatures(accountId,refresh=false){if(!refresh&&signatureCache.has(accountId))return signatureCache.get(accountId);const values=await window.tm.listSignatures(accountId);signatureCache.set(accountId,values);return values;}
async function applyComposerSignature(kind=composerSignatureKind){composerSignatureKind=kind;compEditEl.querySelector('.composer-signature')?.remove();const accountId=Number(document.querySelector('.from-sel')?.value);if(!accountId)return;try{const signature=(await accountSignatures(accountId)).find(item=>item.kind===kind&&item.enabled&&item.body_html.trim());if(!signature)return;const node=document.createElement('div');node.className='composer-signature';node.innerHTML=signature.body_html;const quote=compEditEl.querySelector('.mail-quote-head');if(quote)compEditEl.insertBefore(node,quote);else compEditEl.appendChild(node);scheduleDraftSave();}catch(error){console.error(error);}}
async function openComposerForMessage(action){if(!activeMessage)return;resetComposer();
  // Отвечаем/пересылаем с того ящика, на который пришло письмо.
  const fromSel=document.querySelector('.from-sel');if(fromSel&&activeMessage.account_id&&[...fromSel.options].some(opt=>opt.value===String(activeMessage.account_id)))fromSel.value=String(activeMessage.account_id);
  const reply=action!=='forward',from=activeFullMessage?.meta?.from?.email||activeMessage.from?.email||'',subject=activeMessage.subject||'',prefix=action==='forward'?'Fwd: ':'Re: ';document.getElementById('compTitle').textContent=action==='forward'?L('Переслать','Forward'):L('Ответить','Reply');document.getElementById('compSubj').value=new RegExp(`^${prefix}`,'i').test(subject)?subject:prefix+subject;if(reply&&from)setRecipients('compTo',[{name:activeFullMessage?.meta?.from?.name||'',email:from}]);if(action==='replyall'){const own=new Set(coreAccounts.map(account=>account.email.toLowerCase()));const others=[...(activeFullMessage?.meta?.to||[]),...(activeFullMessage?.meta?.cc||[])].filter(address=>address.email&&!own.has(address.email.toLowerCase())&&address.email.toLowerCase()!==from.toLowerCase());const seen=new Set();const uniq=others.filter(a=>{const k=a.email.toLowerCase();if(seen.has(k))return false;seen.add(k);return true;});setRecipients('compCc',uniq.map(a=>({name:a.name||'',email:a.email})));setRecipientFieldVisible('compCc',uniq.length>0);}const dateStr=activeMessage.date?new Date(activeMessage.date).toLocaleString(document.documentElement.lang):'';const bodyHtml=activeFullMessage?.body_html,bodyText=activeFullMessage?.body_text||activeMessage.preview||'';const quote=bodyHtml?bodyHtml:escapeHtml(bodyText).replace(/\n/g,'<br>');const header=`${escapeHtml(dateStr)}${dateStr?', ':''}${escapeHtml(activeFullMessage?.meta?.from?.name||from)} &lt;${escapeHtml(from)}&gt;:`;compEditEl.innerHTML=`<p><br></p><div class="mail-quote-head" style="color:var(--text-3,#888)">${header}</div><blockquote style="margin:6px 0 0;padding:0 0 0 12px;border-left:2px solid var(--border,#ccc)">${quote}</blockquote>`;showView('composeView');await applyComposerSignature('reply');const range=document.createRange(),sel=window.getSelection();range.setStart(compEditEl.firstChild,0);range.collapse(true);sel.removeAllRanges();sel.addRange(range);compEditEl.focus();}
function contactAddresses(){const seen=new Set(),result=[];coreContacts.forEach(contact=>(contact.emails||[]).forEach(item=>{const email=String(item.email||'').trim(),key=email.toLocaleLowerCase();if(!email||seen.has(key))return;seen.add(key);result.push({name:contact.display_name||'',email});}));return result;}
/* Кандидаты подсказки получателей: ядро объединяет историю выбранного ящика с
   контактами и упорядочивает их по частоте и давности переписки
   (recipient-history.md, S-029 - S-031). До ответа ядра подсказка работает по
   одним контактам: отсутствие истории ей не мешает. */
let recipientCandidates=[];
let recipientCandidatesAccount=0;
async function loadRecipientCandidates(accountId){
  const id=Number(accountId)||0;
  if(!id||!window.tm?.recipientCandidates)return;
  try{
    recipientCandidates=await window.tm.recipientCandidates(id);
    recipientCandidatesAccount=id;
    // S-056: кэш ключей подсказки принадлежит прежнему набору кандидатов.
    composerContactKeysCache.invalidate();
  }catch(error){console.error(error);}
}
window.loadRecipientCandidates=loadRecipientCandidates;
function composerCandidates(){
  const accountId=Number(document.querySelector('.from-sel')?.value)||0;
  // S-041: до загрузки истории нового ящика подсказка показывает контакты, а не
  // чужую историю.
  if(accountId&&accountId!==recipientCandidatesAccount){
    loadRecipientCandidates(accountId);
    return contactAddresses();
  }
  return recipientCandidates.length?recipientCandidates:contactAddresses();
}
// Ключи транслитерации по адресу (S-012 person-search-translit.md): контакт
// пересчитывается при каждом вводе, поэтому кэш ведём по email, а не по ссылке
// на объект. Сбрасывается в reloadCoreData (mail.js) вместе с coreContacts.
const composerContactKeysCache=personSearch.createPersonSearchCache();
window.invalidateComposerContactCache=()=>composerContactKeysCache.invalidate();
function recipientToken(value){return String(value||'').split(/[;,]/).at(-1).trim();}
function chooseRecipient(input,contact){addRecipientEntry(input.id,recipientFormat({name:contact.name,email:contact.email}));input.value='';input.dispatchEvent(new Event('input',{bubbles:true}));input.focus();scheduleDraftSave();}
['compTo','compCc','compBcc'].forEach(id=>{const input=document.getElementById(id),menu=input.parentElement.querySelector('.recipient-suggestions');let active=-1;const render=()=>{const query=recipientToken(input.value),used=new Set([...recipientModel[id].map(entry=>entry.email.toLocaleLowerCase()),...splitAddresses(input.value).map(value=>(value.match(/<([^>]+)>/)?.[1]||value).trim().toLocaleLowerCase())]),matches=window.recipientHistoryModel.historySuggestions(composerCandidates(),query,used,contact=>composerContactKeysCache.get(contact.email.toLocaleLowerCase(),()=>`${contact.name} ${contact.email}`),personSearch);active=-1;menu.innerHTML='';matches.forEach((contact,index)=>{const option=document.createElement('button');option.type='button';option.className='recipient-option';option.innerHTML='<span></span><small></small>';option.querySelector('span').textContent=window.recipientHistoryModel.historyCandidateLabel(contact);const badge=window.recipientHistoryModel.historyCandidateBadge(contact,composerLang());option.querySelector('small').textContent=badge?`${contact.email} - ${badge}`:contact.email;option.onmousedown=event=>{event.preventDefault();chooseRecipient(input,contact);menu.classList.remove('open');};option.dataset.index=index;menu.appendChild(option);});menu.classList.toggle('open',matches.length>0);};input.addEventListener('input',render);input.addEventListener('focus',render);input.addEventListener('keydown',event=>{const options=[...menu.querySelectorAll('.recipient-option')];if((event.key===','||event.key===';')&&!(active>=0&&options.length)){event.preventDefault();commitRecipientInput(id);menu.classList.remove('open');render();return;}if(event.key==='Backspace'&&!input.value&&recipientModel[id].length){event.preventDefault();removeRecipientEntry(id,recipientModel[id].length-1);return;}if(!options.length){if(event.key==='Enter'&&input.value.trim()){event.preventDefault();commitRecipientInput(id);}return;}if(event.key==='ArrowDown'||event.key==='ArrowUp'){event.preventDefault();active=(active+(event.key==='ArrowDown'?1:-1)+options.length)%options.length;options.forEach((option,index)=>option.classList.toggle('active',index===active));options[active].scrollIntoView({block:'nearest'});}else if(event.key==='Enter'){event.preventDefault();if(active>=0)options[active].dispatchEvent(new MouseEvent('mousedown',{bubbles:true}));else{commitRecipientInput(id);menu.classList.remove('open');}}else if(event.key==='Escape')menu.classList.remove('open');});input.addEventListener('blur',()=>{if(input.value.trim())commitRecipientInput(id);});});
document.addEventListener('click',event=>{if(!event.target.closest('.recipient-input'))document.querySelectorAll('.recipient-suggestions').forEach(menu=>menu.classList.remove('open'));});
const syncActionWaiters=new Map();
function errorTranslations(){return typeof wizardText==='undefined'?undefined:wizardText;}
function waitForAccountSync(accountId){
  if(accountId==null)return window.tm?.syncAccounts();
  return new Promise((resolve,reject)=>{
    const id=Number(accountId),timer=setTimeout(()=>{syncActionWaiters.delete(id);reject({kind:'timeout',account_id:id,message:'sync result timeout'});},120000);
    syncActionWaiters.set(id,{resolve,reject,timer});
    Promise.resolve(window.tm?.syncAccounts()).catch(error=>{clearTimeout(timer);syncActionWaiters.delete(id);reject(error);});
  });
}
function settleSyncAction(state){
  const waiter=syncActionWaiters.get(Number(state?.account_id));
  if(!waiter||['syncing','retrying'].includes(state.status))return;
  clearTimeout(waiter.timer);syncActionWaiters.delete(Number(state.account_id));
  const warning=Array.isArray(state.warnings)?state.warnings.find(item=>item&&typeof item==='object'):null;
  if(state.error_kind||state.status==='error'||warning)waiter.reject(warning||{kind:state.error_kind,message:state.error_message||state.error,account_id:state.account_id,server:state.server,response_code:state.response_code,attempted_at:state.attempted_at});
  else waiter.resolve(state);
}
function errorAction(presented,context={}){
  if(context.action)return context.action;
  const account=coreAccounts.find(item=>item.id===Number(presented.accountId));
  if(presented.action==='reconnect'||presented.action==='check_and_retry')return ()=>showAccountWizard(account?.email||context.email||'');
  if(presented.action==='retry'||presented.action==='wait')return context.retry||(()=>waitForAccountSync(presented.accountId));
  if(presented.action==='settings')return ()=>{showView('settingsView');setSection('addacct');};
  return ()=>window.tm?.openDataDir();
}
/* ---------- журнал событий строки статуса ----------
   Уведомления, ошибки и действия не всплывают карточками, а пишутся в журнал:
   последняя запись всегда видна в нижней строке окна, остальные открываются
   щелчком по ней (issue #120, specs/status-activity-log.md). Число записей и
   видимых строк - пределы limit_activity_log_entries и limit_activity_log_visible. */
const activityModel=window.activityLogModel;
let activityLog=activityModel.createLog();
let activityPanelOpen=false;
let activityTimer=null;
function activityLimit(name){return window.limitsModel?.limitValue(window.limitsModel.KEYS[name])??null;}
function activityTime(time){try{return new Date(time).toLocaleTimeString(composerLang()==='en'?'en-US':'ru-RU',{hour:'2-digit',minute:'2-digit'});}catch(_){return '';}}
function activityAccountsText(baseText,accounts,item){return window.errorPresentation.formatAccountErrorText(baseText,accounts,item.locale,item.translations);}
function logActivity(item){const result=activityModel.addEntry(activityLog,item,Date.now(),activityLimit('activityLogEntries'),activityAccountsText);activityLog=result.log;renderActivity();return result;}
function activityActionButton(entry,className){
  if(!entry.hasAction||(entry.actionUntil!=null&&Number(entry.actionUntil)<=Date.now()))return null;
  const button=document.createElement('button');button.type='button';button.className=className;
  button.textContent=entry.actionState==='pending'?wt('errorActionPending'):entry.actionLabel;
  button.disabled=!activityModel.actionAvailable(entry,Date.now());
  button.onclick=event=>{event?.stopPropagation?.();return runActivityAction(entry.id);};
  return button;
}
async function runActivityAction(id){
  const entry=activityLog.entries.find(item=>item.id===id);
  if(!entry||!activityModel.actionAvailable(entry,Date.now()))return;
  activityLog=activityModel.beginAction(activityLog,id,wt('errorActionPending'));renderActivity();
  try{await Promise.all(entry.callbacks.map(callback=>callback()));activityLog=activityModel.finishAction(activityLog,id,{ok:true,text:wt('errorActionSucceeded')},Date.now());}
  catch(error){activityLog=activityModel.finishAction(activityLog,id,{ok:false,item:errorToastItem(error,{connected:true})},Date.now());}
  renderActivity();
}
// Перерисовка к ближайшему сроку: истекает окно отмены или наступает время,
// после которого действие "Повторить позже" становится доступным.
function scheduleActivityRefresh(){
  if(activityTimer){clearTimeout(activityTimer);activityTimer=null;}
  const now=Date.now();
  const deadlines=activityLog.entries.flatMap(entry=>[entry.actionUntil!=null?Number(entry.actionUntil):NaN,entry.action==='wait'&&entry.retryAt?Date.parse(entry.retryAt):NaN]).filter(time=>Number.isFinite(time)&&time>now);
  if(deadlines.length)activityTimer=setTimeout(renderActivity,Math.min(...deadlines)-now+50);
}
function renderActivityStatus(){
  const line=document.getElementById('statusbarText');if(!line)return;
  line.innerHTML='';
  document.getElementById('statusbar')?.classList.toggle('has-unseen-error',activityLog.unseenError);
  const progress=window.activityProgress;
  if(progress){line.dataset.level='progress';const text=document.createElement('span');text.className='statusbar-entry-text';text.textContent=progress;line.appendChild(text);return;}
  const entry=activityModel.latestEntry(activityLog);
  if(!entry){delete line.dataset.level;return;}
  line.dataset.level=entry.level;
  const time=document.createElement('span');time.className='statusbar-entry-time';time.textContent=activityTime(entry.time);
  const text=document.createElement('span');text.className='statusbar-entry-text';text.textContent=entry.text;
  line.append(time,text);
  if(entry.count>1){const count=document.createElement('span');count.className='activity-count';count.textContent=`x${entry.count}`;line.appendChild(count);}
  const button=activityActionButton(entry,'statusbar-entry-action');if(button)line.appendChild(button);
}
function renderActivityPanel(){
  const panel=document.getElementById('activityPanel');if(!panel)return;
  panel.classList.toggle('hidden',!activityPanelOpen);
  const rows=activityLimit('activityLogVisible');
  if(rows)panel.style.setProperty('--activity-rows',String(rows));
  const list=panel.querySelector('.activity-list');if(!list)return;
  // Список пересобирается и на каждом тике отсчёта отмены: раскрытые
  // подробности и прокрутка переносятся, иначе их нельзя было бы дочитать.
  const opened=new Set([...list.querySelectorAll('.activity-entry')].filter(row=>row.querySelector('details')?.open).map(row=>row.dataset.activityId));
  const scroll=list.scrollTop;
  list.innerHTML='';
  if(!activityLog.entries.length){const empty=document.createElement('div');empty.className='activity-empty';empty.textContent=wt('activityLogEmpty');list.appendChild(empty);return;}
  activityLog.entries.forEach(entry=>{
    const row=document.createElement('div');row.className='activity-entry';row.dataset.level=entry.level;row.dataset.activityId=entry.id;
    const head=document.createElement('div');head.className='activity-entry-head';
    const time=document.createElement('span');time.className='activity-entry-time';time.textContent=activityTime(entry.time);
    const text=document.createElement('span');text.className='activity-entry-text';text.textContent=entry.text;
    head.append(time,text);
    if(entry.count>1){const count=document.createElement('span');count.className='activity-count';count.textContent=`x${entry.count}`;head.appendChild(count);}
    const button=activityActionButton(entry,'activity-entry-action');if(button)head.appendChild(button);
    row.appendChild(head);
    if(entry.actionStatus){const status=document.createElement('div');status.className='activity-entry-status';status.textContent=entry.actionStatus;row.appendChild(status);}
    if(entry.details){const details=document.createElement('details');details.open=opened.has(entry.id);const summary=document.createElement('summary');summary.textContent=wt('errorDetails');const pre=document.createElement('pre');pre.textContent=entry.details;details.append(summary,pre);row.appendChild(details);}
    list.appendChild(row);
  });
  list.scrollTop=scroll;
}
function renderActivity(){
  const capacity=activityLimit('activityLogEntries');
  if(capacity&&activityLog.entries.length>capacity)activityLog={...activityLog,entries:activityLog.entries.slice(0,capacity)};
  renderActivityStatus();renderActivityPanel();scheduleActivityRefresh();
}
function setActivityPanel(open){activityPanelOpen=open;if(open)activityLog=activityModel.markSeen(activityLog);document.getElementById('statusbarText')?.setAttribute('aria-expanded',String(open));renderActivity();}
window.renderActivityStatus=renderActivityStatus;
window.renderActivity=renderActivity;
window.logActivity=logActivity;
(function bindActivityPanel(){
  const line=document.getElementById('statusbarText'),panel=document.getElementById('activityPanel');
  if(!line||!panel)return;
  line.addEventListener('click',()=>setActivityPanel(!activityPanelOpen));
  // Только нажатие на самой строке: Enter на вложенной кнопке действия
  // должен выполнить действие, а не открыть журнал.
  line.addEventListener('keydown',event=>{if(event.target!==line)return;if(event.key==='Enter'||event.key===' '){event.preventDefault();setActivityPanel(!activityPanelOpen);}});
  panel.querySelector('.activity-close')?.addEventListener('click',()=>setActivityPanel(false));
  panel.querySelector('.activity-clear')?.addEventListener('click',()=>{activityLog=activityModel.clearLog(activityLog);renderActivity();});
  document.addEventListener('keydown',event=>{if(event.key==='Escape'&&activityPanelOpen)setActivityPanel(false);});
  document.addEventListener('click',event=>{if(activityPanelOpen&&!event.target?.closest?.('#activityPanel,#statusbarText'))setActivityPanel(false);});
  renderActivity();
})();
function errorToastItem(error,context={}){const account=coreAccounts.find(item=>item.id===Number(error?.account_id));const translations=errorTranslations();const presented=window.errorPresentation.presentError(error,{locale:wizardLocale,translations,connected:context.connected,account});return {level:'error',kind:presented.kind,accountId:presented.accountId,accounts:presented.accounts,baseText:presented.baseText,text:presented.text,details:presented.details,action:presented.action,actionLabel:presented.actionLabel,retryAt:presented.retryAt,hasAction:true,callback:errorAction(presented,context),groupByKind:presented.accountId!=null,locale:presented.locale,translations};}
function showApiError(error,context={}){return logActivity(errorToastItem(error,context));}
function showToast(message,actionLabel,action){if(message&&typeof message==='object')return showApiError(message);return logActivity({level:'info',kind:'notice',accountId:null,text:String(message||''),details:'',action:action?String(actionLabel||'action'):'',actionLabel,hasAction:Boolean(action),callback:action});}
window.showApiError=showApiError;
// Одна беда одного ящика - одно сообщение: без этого при каждом проходе
// счётчик повторов рос до десятков, а нового человеку не сообщалось (issue #77).
let syncToastMemo={};
function rememberSyncToast(failure){
  const decision=window.errorPresentation.nextSyncToastMemo(syncToastMemo,failure);
  syncToastMemo=decision.memo;
  return decision.show;
}
window.handleSyncState=function(state){if(!state)return;
  if(!state.error_kind&&!state.error&&state.status==='ready')syncToastMemo=window.errorPresentation.nextSyncToastMemo(syncToastMemo,state).memo;
  settleSyncAction(state);
  // Постоянное состояние (needs_reauth/last_sync_error) читается из аккаунта
  // после перезагрузки данных; переходные статусы "syncing"/"retrying" видны
  // раньше - сразу по этому событию, без ожидания truemail-data-changed
  // (mail-sync-visible-state.md, S-006, S-011).
  if(window.mailSyncIndicator?.isMailSyncScope(state.scope)){window.mailSyncTransient=window.mailSyncIndicator.nextMailSyncTransient(window.mailSyncTransient,state);window.refreshAccountSyncIndicator?.(state.account_id);}
  const info=document.getElementById('calSyncInfo');if(info&&['dav','auxiliary'].includes(state.scope)){if(state.status==='syncing')info.textContent=wizardLocale==='en'?'Syncing calendars, tasks and contacts…':'Синхронизация календарей, задач и контактов…';else if(state.status==='error')info.textContent=wizardLocale==='en'?'Calendar, tasks and contacts sync error':'Ошибка синхронизации календаря, задач и контактов';}
  const warnings=Array.isArray(state.warnings)?state.warnings:[];
  if(warnings.length){warnings.forEach(warning=>{if(typeof warning==='string'){showToast(warning);return;}const failure={...warning,account_id:state.account_id,retries_exhausted:state.retries_exhausted};if(window.errorPresentation.shouldShowSyncToast(failure)&&rememberSyncToast(failure))showApiError(failure,{connected:true});});return;}
  if(state.error_kind||['error','retrying'].includes(state.status)){const failure={kind:state.error_kind,message:state.error_message||state.error,account_id:state.account_id,retry_at:state.retry_at,server:state.server,response_code:state.response_code,attempted_at:state.attempted_at,retries_exhausted:state.retries_exhausted};if(window.errorPresentation.shouldShowSyncToast(failure)&&rememberSyncToast(failure))showApiError(failure,{connected:true});}};
async function performMessageActionForIds(action,ids){if(!ids.length){showToast(L('Сначала выберите письмо','Select a message first'));return;}
  ids=window.expandConversationIds?window.expandConversationIds(ids):ids;
  if(action==='trash'&&ids.length>10&&!await confirmAction(L(`Удалить ${ids.length} писем?`,`Delete ${ids.length} messages?`)))return;
  // S-012, S-013: перенос в корзину письма, уже лежащего в корзине, ничего не
  // делает и в безвозвратное удаление не превращается. Удаление навсегда
  // выбирается отдельным действием меню письма.
  if(action==='trash'){
    const messageFolder=id=>{const message=currentMessageRows.find(item=>item.id===id)||messages.find(item=>item.id===id);return coreFolders.find(folder=>folder.id===message?.folder_id);};
    if(ids.every(id=>messageFolder(id)?.role==='trash')){
      showToast(L('Письма уже в корзине. Чтобы стереть их без возврата, выберите "Удалить навсегда".','These messages are already in Trash. To erase them for good choose "Delete permanently".'));
      return;
    }
  }
  // S-013, S-048: безвозвратное удаление подтверждается отдельно и отмены не
  // имеет.
  if(action==='delete'){const hasActiveTask=ids.some(id=>(currentMessageRows.find(item=>item.id===id)||messages.find(item=>item.id===id))?.task_state==='active');const warning=hasActiveTask?L(`Удалить навсегда писем: ${ids.length}? Среди них есть невыполненное дело. Отмены не будет.`,`Delete ${ids.length} message(s) permanently? An unfinished task will be deleted. There is no undo.`):L(`Удалить навсегда писем: ${ids.length}? Отмены не будет.`,`Delete ${ids.length} message(s) permanently? There is no undo.`);if(!await confirmAction(warning))return;}
  // Запоминаем соседнее письмо, чтобы после действия перейти к нему, а не терять фокус.
  let nextId=null;
  if(activeMessage&&ids.length===1){const index=currentMessageRows.findIndex(message=>message.id===activeMessage.id);nextId=currentMessageRows[index+1]?.id??currentMessageRows[index-1]?.id??null;}
  try{const queued=await window.tm.messageAction(ids,action);selectedMessageIds.clear();activeMessage=null;activeFullMessage=null;window.forgetMessages?.(ids);await window.reloadCoreData();
    if(nextId!=null){const message=messages.find(item=>item.id===nextId);if(message)showMessage(message);}
    if(action==='delete'){showToast(L('Письмо удаляется навсегда','The message is being deleted permanently'));return;}
    // Письмо, по которому уже стоит незавершённая операция увода, пропущено
    // ограничением очереди (S-005) - об этом честно говорим вместе с причиной.
    showSkippedMessages(queued);
    showToast(action==='archive'?L('Письмо перемещено в архив','Message moved to Archive'):action==='spam'?L('Письмо перемещено в спам','Message moved to Spam'):L('Письмо перемещено в корзину','Message moved to Trash'),L('Отменить','Undo'),async()=>{await window.tm.undoMessageAction(queued.operation_ids);await window.reloadCoreData();});}catch(error){showToast(error);}}
window.performMessageActionForIds=performMessageActionForIds;
/* Пропуски называются причиной: занятое письмо, письмо с отказавшей операцией и
   письмо без единственной папки нужного типа - разные беды (S-005, S-006,
   S-046). */
function showSkippedMessages(queued){
  if(queued?.traits_at_risk)showToast(L(`У ${queued.traits_at_risk} писем нет единственного Message-ID. Сохранение дела и закрепления после переноса не гарантируется.`,`For ${queued.traits_at_risk} message(s), Message-ID is not unique. Task and pin preservation after moving is not guaranteed.`));
  if(!queued?.skipped)return;
  const parts=[];
  if(queued.skipped_busy)parts.push(L(`уже переносятся: ${queued.skipped_busy}`,`already being moved: ${queued.skipped_busy}`));
  if(queued.skipped_failed)parts.push(L(`ждут решения после отказа: ${queued.skipped_failed}`,`waiting for your decision after a failure: ${queued.skipped_failed}`));
  if(queued.skipped_no_folder)parts.push(L(`нет единственной папки нужного типа: ${queued.skipped_no_folder}`,`no single folder of the required type: ${queued.skipped_no_folder}`));
  const reason=parts.length?` (${parts.join(', ')})`:'';
  showToast(L(`Пропущено писем: ${queued.skipped}${reason}`,`Messages skipped: ${queued.skipped}${reason}`));
}
window.showSkippedMessages=showSkippedMessages;
/* S-013: безвозвратное удаление - отдельное явно названное действие, а не
   следствие переноса в корзину. */
async function deleteMessagesForever(ids){
  return performMessageActionForIds('delete',ids);
}
window.deleteMessagesForever=deleteMessagesForever;
async function performMessageAction(action){const ids=selectedMessageIds.size?[...selectedMessageIds]:activeMessage?[activeMessage.id]:[];return performMessageActionForIds(action,ids);}
function selectAllCurrentMessages(){currentMessageRows.forEach(message=>{if(!message?.kind)selectedMessageIds.add(message.id);});updateSelectionUi();}
document.getElementById('bulkSelectAll').onclick=selectAllCurrentMessages;
document.getElementById('bulkClear').onclick=clearMessageSelection;
document.getElementById('bulkArchive').onclick=()=>performMessageAction('archive');
document.getElementById('bulkTrash').onclick=()=>performMessageAction('trash');
document.getElementById('bulkRead').onclick=async()=>{const ids=[...selectedMessageIds];if(!ids.length)return;try{await window.markMessagesSeen?.(ids.map(id=>messages.find(item=>item.id===id)).filter(Boolean),true);clearMessageSelection();await window.reloadCoreData();showToast(L('Письма отмечены прочитанными','Messages marked as read'));}catch(error){showToast(error);}};
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
    try{await addCompFilePath(path,generation);}catch(error){showToast(error);}
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
  for(const file of files)attachChain=attachChain.then(()=>addCompFile(file,generation)).catch(error=>showToast(error));
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
    try{await addCompFile(file,generation);added++;}catch(error){showToast(error);}
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
    try{dataUrl=await fileToDataUrl(file);}catch(error){showToast(error);continue;}
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
  overlay.querySelector('.template-save').onclick=async()=>{const name=prompt(L('Название шаблона','Template name'),document.getElementById('compSubj').value.trim());if(!name?.trim())return;const body=compEditEl.cloneNode(true);body.querySelector('.composer-signature')?.remove();try{await window.tm.saveMessageTemplate({id:null,accountId,name:name.trim(),subject:document.getElementById('compSubj').value,bodyHtml:body.innerHTML});await render();showToast(L('Шаблон сохранён','Template saved'));}catch(error){showToast(error);}};try{await render();}catch(error){close();showToast(error);}}
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
function scheduleDraftSave(){
  // S-053: письмо изменилось - прежний ключ запроса к нему уже не относится.
  composerRequestKey='';
  clearTimeout(draftSaveTimer);
  draftSaveTimer=setTimeout(()=>window.tm?.setSetting('composer_draft',JSON.stringify(draftPayload())).catch(console.error),500);
}
/* Записать черновик немедленно и дождаться подтверждения: отложенная на 500 мс
   запись не годится там, где следом удаляется единственная долговечная копия
   письма (S-003, S-042). */
async function saveDraftNow(){clearTimeout(draftSaveTimer);await window.tm?.setSetting('composer_draft',JSON.stringify(draftPayload()));}
composerFieldIds.forEach(id=>document.getElementById(id).addEventListener('input',scheduleDraftSave));compEditEl.addEventListener('input',scheduleDraftSave);
function composerRequest(){const draft=draftPayload(),to=splitAddresses(draft.to),cc=splitAddresses(draft.cc),bcc=splitAddresses(draft.bcc),invalid=[...to,...cc,...bcc].find(address=>!validAddress(address));if(!to.length&&!cc.length&&!bcc.length)throw new Error(L('Укажите хотя бы одного получателя','Add at least one recipient'));if(invalid)throw new Error(L(`Некорректный адрес: ${invalid}`,`Invalid address: ${invalid}`));return {account_id:draft.account_id,to,cc,bcc,subject:draft.subject,body_text:draft.body_text,body_html:draft.body_html,attachments:composerAttachments};}
function composerLang(){return wizardLocale==='en'?'en':'ru';}
/* Карточка окна отмены с обратным отсчётом (S-017, S-018). Срок карточки
   определяет срок отмены, а не общий срок исчезновения карточек. */
function showUndoSendCard(queued,request){
  const lang=composerLang();
  const seconds=window.outboxModel.remainingUndoSeconds(queued.cancel_until,Date.now());
  if(!window.outboxModel.showsUndoAction({...queued,origin:'ordinary'},Date.now())){
    // S-019: при нулевом окне отмены действия "Отменить" нет вовсе.
    showToast(L('Письмо принято, отправка началась','The message is accepted and is being sent'));
    return;
  }
  const item={kind:'notice',accountId:null,
    text:window.outboxModel.undoCardText(request,seconds,lang),
    details:'',action:'undo',actionLabel:L('Отменить','Cancel'),hasAction:true,
    callback:async()=>{
      const outcome=await window.tm.cancelSend(queued.account_id,queued.operation_id);
      showToast(window.outboxModel.cancelOutcomeText(outcome,composerLang()));
      if(outcome==='cancelled')await restoreCancelledSend(queued.account_id,queued.operation_id);
    }};
  // Срок - сам cancel_until очереди, а не округлённый отсчёт: иначе кнопка
  // жила бы до секунды дольше настоящего окна отмены. Ключ операции не даёт
  // склеить две отправки с одинаковым текстом в одну отмену.
  const until=window.outboxModel.parseQueueTime(queued.cancel_until);
  item.actionUntil=Number.isFinite(until)?until:Date.now()+seconds*1000;
  item.key=`send-${queued.account_id}-${queued.operation_id}`;
  const entryId=logActivity(item).id;
  const timer=setInterval(()=>{
    const entry=activityLog.entries.find(value=>value.id===entryId);
    const left=window.outboxModel.remainingUndoSeconds(queued.cancel_until,Date.now());
    if(!entry||entry.actionState){clearInterval(timer);return;}
    if(left<=0){
      // Окно отмены вышло: запись остаётся в журнале историей, без действия.
      clearInterval(timer);
      activityLog=activityModel.updateEntry(activityLog,entryId,{text:L('Письмо передано на отправку','The message was handed over for sending'),hasAction:false});
      renderActivity();return;
    }
    activityLog=activityModel.updateEntry(activityLog,entryId,{text:window.outboxModel.undoCardText(request,left,composerLang())});
    renderActivity();
  },1000);
}
/* Возврат отменённого письма в композер целиком: адресаты, тема, оформленное
   тело и все вложения (S-040, S-041). */
async function restoreCancelledSend(accountId,operationId){
  if(composerHasContent()){
    // S-041: несохранённое письмо в композере не затирается, отменённое
    // остаётся в разделе "Исходящие".
    showToast(L('В композере есть другое письмо. Отменённое ждёт в разделе "Исходящие".','The composer holds another message. The cancelled one waits in Outbox.'));
    return;
  }
  try{
    const message=await window.tm.openCancelledSend(accountId,operationId);
    applyCancelledSendToComposer(message);
    // S-003, S-042: операция очереди - единственная долговечная копия письма.
    // Она удаляется только после подтверждённой записи черновика, иначе отказ
    // записи или закрытие программы в этот миг потеряли бы письмо целиком.
    await saveDraftNow();
    await window.tm.deleteSend(accountId,operationId);
    showView('composeView');
  }catch(error){showToast(error);}
}
function applyCancelledSendToComposer(message){
  const draft=window.outboxModel.composerDraftFromCancelled(message);
  resetComposer();
  const fromSel=document.querySelector('.from-sel');
  if(fromSel&&draft.account_id&&[...fromSel.options].some(option=>option.value===String(draft.account_id)))fromSel.value=String(draft.account_id);
  setRecipients('compTo',draft.to);
  setRecipients('compCc',draft.cc);
  setRecipients('compBcc',draft.bcc);
  setRecipientFieldVisible('compCc',draft.cc.length>0);
  setRecipientFieldVisible('compBcc',draft.bcc.length>0);
  document.getElementById('compSubj').value=draft.subject;
  compEditEl.innerHTML=draft.body_html||escapeHtml(draft.body_text).replace(/\n/g,'<br>');
  composerAttachments=draft.attachments.map(item=>({filename:item.filename,mime_type:item.mime_type,data:item.data}));
  compAtt.innerHTML='';composerAttachments.forEach(renderComposerAttachment);
  scheduleDraftSave();
}
window.restoreCancelledSend=restoreCancelledSend;
/* Состояние очереди отправки при запуске программы (S-034 - S-036). */
async function restoreUndoWindows(){
  if(!window.tm?.startupSendState)return;
  const state=await window.tm.startupSendState();
  const lang=composerLang();
  (state.pending||[]).forEach(queued=>{
    // Тема и адресаты ожидающего письма в список не передаются: карточка
    // показывает то, что уже известно очереди.
    const entry=(outboxEntriesForCard||[]).find(item=>item.id===queued.operation_id);
    showUndoSendCard({...queued,origin:'ordinary'},{subject:entry?.subject||'',to:entry?.to||[]});
  });
  if(state.expired>0)showToast(window.outboxModel.expiredWindowsText(state.expired,lang));
  if(state.uncertain>0){
    showToast(L(`Писем с неизвестным итогом передачи: ${state.uncertain}`,`Messages with an unknown outcome: ${state.uncertain}`));
  }
}
/* Список очереди, прочитанный разделом "Исходящие": карточка окна отмены берёт
   из него тему и адресатов восстановленного письма. */
let outboxEntriesForCard=[];
window.setOutboxEntriesForCard=entries=>{outboxEntriesForCard=entries||[];};
window.restoreUndoWindows=restoreUndoWindows;
document.getElementById('compSend').onclick=async()=>{
  // Крупный файл может ещё дочитываться: без ожидания письмо ушло бы без него,
  // а вложение легло бы в уже очищенный композер. Если за это время открыли
  // другое письмо, нажатие относилось к прежнему - отправлять нечего.
  const generation=composerGeneration;
  await settleAttachments();
  if(generation!==composerGeneration)return;
  let request;
  // S-005: адресаты проверяются до записи операции.
  try{request=composerRequest();}catch(error){showToast(error);return;}
  if(!composerRequestKey)composerRequestKey=`send-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  let queued;
  try{queued=await window.tm.sendMessage(request,composerRequestKey);}
  catch(error){
    // S-003: письмо не принято - композер и черновик остаются нетронутыми.
    showToast(error);return;
  }
  // S-002: композер очищается только после подтверждённой записи в очередь;
  // очистка снимает и ключ запроса - следующее письмо получит свой.
  resetComposer();showView('mailView');window.tm.setSetting('composer_draft','').catch(()=>{});
  showUndoSendCard(queued,request);
};
document.getElementById('compSendLater').onclick=async()=>{const generation=composerGeneration;await settleAttachments();if(generation!==composerGeneration)return;const input=document.getElementById('compSendAt'),status=document.getElementById('composeStatus');if(input.classList.contains('hidden')){const date=new Date(Date.now()+15*60*1000);date.setSeconds(0,0);input.value=new Date(date.getTime()-date.getTimezoneOffset()*60000).toISOString().slice(0,16);input.min=new Date(Date.now()-new Date().getTimezoneOffset()*60000).toISOString().slice(0,16);input.classList.remove('hidden');input.focus();return;}try{const date=new Date(input.value);if(Number.isNaN(date.getTime()))throw new Error(L('Выберите дату и время','Choose a date and time'));const id=await window.tm.scheduleMessage(composerRequest(),date.toISOString());await window.tm.setSetting('composer_draft','');status.textContent=L(`Запланировано (задача ${id})`,`Scheduled (task ${id})`);status.dataset.kind='success';setTimeout(()=>{resetComposer();showView('mailView');},700);}catch(error){status.textContent=window.errorPresentation.errorText(error,{locale:wizardLocale,translations:wizardText});status.dataset.kind='error';}};
document.getElementById('compDeleteDraft').onclick=async()=>{resetComposer();await window.tm?.setSetting('composer_draft','').catch(console.error);showView('mailView');};
