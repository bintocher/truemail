// truemail UI module: commands-accessibility.js
/* theme settings */
const root=document.documentElement,pop=document.getElementById('pop');
document.getElementById('toThemes').onclick=()=>{pop.classList.remove('open');showView('settingsView');setSection('themes');};
function setTheme(t,persist=true){if(t==='auto')root.removeAttribute('data-theme');else root.setAttribute('data-theme',t);
  try{if(t==='auto')localStorage.removeItem('truemail-theme');else localStorage.setItem('truemail-theme',t);}catch(_){}
  document.querySelectorAll('[data-theme]').forEach(b=>{if(b.tagName==='BUTTON')b.classList.toggle('on',b.dataset.theme===t);});
  if(persist)window.tm?.setSetting('theme',t).catch(console.error);}
document.querySelectorAll('#segTheme button, #setTheme button').forEach(b=>b.onclick=()=>setTheme(b.dataset.theme));
document.getElementById('importTheme').onclick=()=>document.getElementById('themeFile').click();
document.getElementById('themeFile').onchange=async event=>{const file=event.target.files?.[0];event.target.value='';if(!file)return;try{const imported=JSON.parse(await file.text());if(imported.format!=='truemail-theme/v1')throw new Error(L('Неподдерживаемый формат темы','Unsupported theme format'));if(imported.theme&&!['light','dark','auto'].includes(imported.theme))throw new Error(L('Некорректный режим темы','Invalid theme mode'));if(imported.density&&!['compact','normal','spacious'].includes(imported.density))throw new Error(L('Некорректная плотность','Invalid density'));if(imported.accent&&!['indigo','teal','rose','amber','blue','violet','cyan','green','orange'].includes(imported.accent))throw new Error(L('Некорректный акцент','Invalid accent'));if(imported.theme)setTheme(imported.theme);if(imported.density)setDensity(imported.density);if(imported.accent)setAccent(imported.accent);if(imported.ui_scale)setUiScale(imported.ui_scale);showToast(L(`Тема «${imported.name||file.name}» импортирована`,`Theme “${imported.name||file.name}” imported`));}catch(error){showToast(error.message||String(error));}};
function setDensity(d,persist=true){if(d==='normal')root.removeAttribute('data-density');else root.setAttribute('data-density',d);
  document.querySelectorAll('#segDensity button, #setDensity button').forEach(x=>x.classList.toggle('on',x.dataset.density===d));
  if(persist)window.tm?.setSetting('density',d).catch(console.error);}
document.querySelectorAll('#segDensity button, #setDensity button').forEach(b=>b.onclick=()=>setDensity(b.dataset.density));
function setAccent(a,persist=true){if(a==='indigo')root.removeAttribute('data-accent');else root.setAttribute('data-accent',a);
  document.querySelectorAll('#accents .dot-accent').forEach(x=>x.classList.toggle('on',x.dataset.accent===a));
  document.querySelectorAll('#setAccent .swatch').forEach(x=>x.classList.toggle('on',x.dataset.accent===a));
  if(persist)window.tm?.setSetting('accent',a).catch(console.error);}
document.querySelectorAll('#accents .dot-accent, #setAccent .swatch').forEach(d=>d.onclick=()=>setAccent(d.dataset.accent));

/* command palette */
const overlay=document.getElementById('overlay'),cmdInput=document.getElementById('cmdInput'),cmdlist=document.getElementById('cmdlist');
// раскладко-независимый поиск: as<->фы, ntv<->тем, ыуе<->set
const RU="йцукенгшщзхъфывапролджэячсмитьбю",EN="qwertyuiop[]asdfghjkl;'zxcvbnm,.";
function conv(s,a,b){return s.split('').map(c=>{const i=a.indexOf(c);return i>=0?b[i]:c;}).join('');}
function matchQ(text,q){text=(text||'').toLowerCase();q=q.toLowerCase();return text.includes(q)||text.includes(conv(q,RU,EN))||text.includes(conv(q,EN,RU));}
function layoutQueries(query){const source=String(query||''),lower=source.toLocaleLowerCase(),variants=[source,conv(lower,RU,EN),conv(lower,EN,RU)];return [...new Set(variants.filter(Boolean))];}
function goCal(){document.querySelectorAll('.navitem').forEach(x=>x.classList.remove('active'));const a=document.getElementById('app');a.classList.remove('contactsmode');a.classList.add('calmode');showView('mailView');}
function goContacts(){document.querySelectorAll('.navitem').forEach(x=>x.classList.remove('active'));const a=document.getElementById('app');a.classList.remove('calmode');a.classList.add('contactsmode');showView('mailView');}
function goMail(){const a=document.getElementById('app');a.classList.remove('calmode','contactsmode');showView('mailView');}
// Открыть письмо по id (вызывается из своего уведомления через bridge).
// Письмо может лежать в любой папке, в том числе вложенной и не входящей в умную папку,
// поэтому переключаем список на папку письма, а не полагаемся на текущий вид.
window.openMessageById=async function(id){
  goMail();
  let message=messages.find(item=>item.id===id);
  if(!message){
    // В messages лежит лишь по странице писем на папку - недостающее письмо берём с бэкенда.
    try{const full=await window.tm?.getMessage(id);if(full?.meta)message={...full.meta};}catch(error){console.error('openMessageById',error);}
    if(!message){showToast(L('Письмо не найдено','Message not found'));return;}
    messages=[...messages,message];
  }
  const folder=coreFolders.find(item=>item.id===message.folder_id);
  if(folder){
    currentFolderId=folder.id;currentSmartIndex=null;
    document.querySelectorAll('.navitem').forEach(item=>item.classList.remove('active'));
    const row=document.querySelector(`.folder-row[data-folder-id="${folder.id}"]`);
    if(row){
      row.classList.add('active');
      const sub=row.closest('.acc-sub'),header=sub?.previousElementSibling;
      if(sub&&!sub.classList.contains('open')){sub.classList.add('open');header?.classList.add('open');if(header?.dataset.accountId)saveAccountNavOpen(header.dataset.accountId,true);}
      row.scrollIntoView({block:'nearest'});
    }
    applyListOptions(true,folderTitle(folder));
    // Активные фильтры списка могут скрыть только что пришедшее письмо - тогда снимаем их.
    if(!currentMessageRows.some(item=>item.id===id)){
      filterMenu.querySelectorAll('input[type="checkbox"]:checked').forEach(input=>{input.checked=false;});
      const filterText=document.getElementById('filterText');if(filterText)filterText.value='';
      applyListOptions(true,folderTitle(folder));
    }
  }
  const index=currentMessageRows.findIndex(item=>item.id===id);
  if(index>=0)focusMessageAt(index);else await showMessage(message);
};
const S2=(id)=>()=>{showView('settingsView');setSection(id);};
function getStaticCmds(){const en=smartIsEnglish(),gAct=en?'Actions':'Действия',gGo=en?'Go to':'Переход',gSet=en?'Settings':'Настройки';return [
  {g:gAct,i:'compose',t:en?'Compose new message':'Написать новое письмо',k:['C'],a:()=>document.getElementById('composeBtn').click()},
  {g:gAct,i:'reply',t:en?'Reply':'Ответить',k:['R'],a:()=>openComposerForMessage('reply')},{g:gAct,i:'replyall',t:en?'Reply all':'Ответить всем',k:['A'],a:()=>openComposerForMessage('replyall')},{g:gAct,i:'forward',t:en?'Forward':'Переслать',k:['F'],a:()=>openComposerForMessage('forward')},
  {g:gAct,i:'archive',t:en?'Archive':'В архив',k:['E'],a:()=>performMessageAction('archive')},{g:gAct,i:'trash',t:en?'Delete':'Удалить',k:['Del'],a:()=>performMessageAction('trash')},
  {g:gGo,i:'inbox',t:en?'All inboxes':'Все входящие',a:goMail},{g:gGo,i:'cal',t:en?'Calendar':'Календарь',a:goCal},{g:gGo,i:'people',t:en?'Contacts':'Контакты',a:goContacts},
  {g:gGo,i:'cal',t:en?'Today':'Сегодня',a:goMail},{g:gGo,i:'search',t:en?'Unread (all)':'Непрочитанные (все)',a:goMail},{g:gGo,i:'paperclip',t:en?'With attachments':'С вложениями',a:goMail},
  {g:gSet,i:'settings',t:en?'General':'Общие',a:S2('general')},{g:gSet,i:'settings',t:en?'Expert mode':'Режим эксперта',a:S2('general')},
  {g:gSet,i:'grip',t:en?'Message toolbar':'Панель письма',a:S2('toolbar')},{g:gSet,i:'user',t:en?'Accounts':'Аккаунты',a:S2('accounts')},{g:gSet,i:'user',t:en?'Add account':'Добавить аккаунт',a:showAccountWizard},
  {g:gSet,i:'folder',t:en?'Folder mapping':'Сопоставление папок',a:S2('folders')},{g:gSet,i:'cal',t:en?'Calendars':'Календари',a:S2('calendars')},{g:gSet,i:'storage',t:en?'Storage':'Хранилище',a:S2('storage')},
  {g:gSet,i:'palette',t:en?'Themes and appearance':'Темы и оформление',a:S2('themes')},{g:gSet,i:'shield',t:en?'Privacy':'Приватность',a:S2('privacy')},{g:gSet,i:'keyboard',t:en?'Keyboard shortcuts':'Горячие клавиши',a:S2('keys')},
  {g:gSet,i:'sun',t:en?'Toggle theme':'Переключить тему',a:()=>setTheme(root.getAttribute('data-theme')==='dark'?'light':'dark')},
];}
let sel=0,currentCommands=[],searchHistory=[];
function searchTerms(q){return q.split(/\s+/).filter(token=>token&&!/^from:/i.test(token)&&!/^has:attachments?$/i.test(token)).join(' ').trim();}
function highlightMatch(value,q){const text=String(value||''),needle=searchTerms(q);if(!needle)return escapeHtml(text);const candidates=[needle,conv(needle.toLocaleLowerCase(),RU,EN),conv(needle.toLocaleLowerCase(),EN,RU)];let found=-1,length=0;for(const candidate of candidates){const index=text.toLocaleLowerCase().indexOf(candidate);if(index>=0){found=index;length=candidate.length;break;}}return found<0?escapeHtml(text):`${escapeHtml(text.slice(0,found))}<mark>${escapeHtml(text.slice(found,found+length))}</mark>${escapeHtml(text.slice(found+length))}`;}
function buildResults(q,coreResults=[]){const base=[...getStaticCmds()];
  if(!q.trim())searchHistory.forEach(value=>base.unshift({g:wizardLocale==='en'?'Recent searches':'Недавние запросы',i:'search',t:value,a:()=>{openCmd();cmdInput.value=value;cmdInput.dispatchEvent(new Event('input'));}}));
  if(q.trim()){
    coreResults.forEach(m=>base.push({g:smartIsEnglish()?'Messages':'Письма',i:'inbox',t:m.subject||(smartIsEnglish()?'(no subject)':'(без темы)'),sub:(m.from?.name||m.from?.email||'')+' · '+(m.preview||'').slice(0,80),searchHit:true,a:()=>{goMail();showMessage(m);}}));
    coreContacts.forEach(c=>base.push({g:smartIsEnglish()?'Contacts':'Контакты',i:'people',t:c.display_name,sub:c.emails?.[0]?.email||'',a:goContacts}));
  }
  const terms=searchTerms(q);return q.trim()?base.filter(c=>c.searchHit||(terms&&matchQ(c.t+' '+(c.sub||'')+' '+c.g,terms))):base;}
function renderCmd(q='',coreResults=[]){const f=buildResults(q,coreResults);currentCommands=f;sel=0;let html='',lg='';
  f.forEach((c,idx)=>{if(c.g!==lg){html+=`<div class="cmdgrp">${escapeHtml(c.g)}</div>`;lg=c.g;}const icon=Object.hasOwn(ic,c.i)?c.i:'inbox';
    html+=`<div class="cmdrow${idx===0?' sel':''}" data-idx="${idx}"><i data-i="${icon}"></i><span class="ctitle">${highlightMatch(c.t,q)}</span>${c.sub?`<span class="csub">${highlightMatch(c.sub,q)}</span>`:''}<span class="ck">${(c.k||[]).map(k=>`<span class="kbd">${escapeHtml(k)}</span>`).join('')}</span></div>`;});
  cmdlist.innerHTML=html||`<div class="cmdgrp">${smartIsEnglish()?'Nothing found':'Ничего не найдено'}</div>`;renderIcons(cmdlist);
  cmdlist.querySelectorAll('.cmdrow').forEach(r=>r.onclick=()=>{const c=f[+r.dataset.idx];closeCmd();if(c&&c.a)c.a();});}
function openCmd(){overlay.classList.add('open');cmdInput.value='';renderCmd();cmdInput.focus();}
function closeCmd(){overlay.classList.remove('open');}
document.getElementById('searchBox').onclick=openCmd;
let searchSerial=0;
cmdInput.oninput=async()=>{const q=cmdInput.value,serial=++searchSerial;if(!q.trim()){renderCmd();return;}renderCmd(q,[]);try{const batches=await Promise.all(layoutQueries(q).map(value=>window.tm?.search(value)||[])),seen=new Set(),found=batches.flat().filter(item=>{const key=item.id??`${item.folder_id}:${item.uid}`;if(seen.has(key))return false;seen.add(key);return true;});if(serial===searchSerial){renderCmd(q,found);if(searchTerms(q).length>=2){searchHistory=[q,...searchHistory.filter(item=>item!==q)].slice(0,10);window.tm?.setSetting('search_history',JSON.stringify(searchHistory)).catch(console.error);}}}catch(e){console.error('search',e);}};
overlay.onclick=e=>{if(e.target===overlay)closeCmd();};
const activeKeybindings=new Map([
  ['toggle_window','Ctrl+Shift+M'],['compose_global','Ctrl+Shift+C'],['quick_search','Ctrl+Shift+F'],
  ['palette','Ctrl+K'],['compose','C'],['reply','R'],['reply_all','A'],['forward','F'],
  ['archive','E'],['snooze','H'],['next_message','J'],['prev_message','K'],['delete','Del'],
]);
function eventCombo(event){const parts=[];if(event.ctrlKey)parts.push('Ctrl');if(event.altKey)parts.push('Alt');if(event.shiftKey)parts.push('Shift');if(event.metaKey)parts.push('Meta');let key=event.key;if(['Control','Alt','Shift','Meta'].includes(key))return '';if(key==='Delete')key='Del';else if(key===' ')key='Space';else if(key.length===1)key=key.toUpperCase();parts.push(key);return parts.join('+');}
function bindingMatches(action,event){return activeKeybindings.get(action)?.toLocaleLowerCase()===eventCombo(event).toLocaleLowerCase();}
async function refreshKeybindings(){if(!window.tm?.listKeybindings)return;const bindings=await window.tm.listKeybindings();bindings.forEach(binding=>activeKeybindings.set(binding.action,binding.combo));document.querySelectorAll('[data-key-action]').forEach(input=>{input.value=activeKeybindings.get(input.dataset.keyAction)||'';});}
window.refreshKeybindings=refreshKeybindings;
document.querySelectorAll('[data-key-action]').forEach(input=>{input.addEventListener('keydown',async event=>{event.preventDefault();event.stopPropagation();const combo=eventCombo(event);if(!combo)return;const previous=activeKeybindings.get(input.dataset.keyAction)||'';input.value=combo;input.disabled=true;try{await window.tm.setKeybinding(input.dataset.keyAction,combo);activeKeybindings.set(input.dataset.keyAction,combo);showToast(L('Сочетание сохранено','Shortcut saved'));}catch(error){input.value=previous;showToast(error.message||String(error));}finally{input.disabled=false;input.focus();}});});
document.addEventListener('keydown',e=>{
  const target=e.target;if(target.matches?.('[data-key-action]'))return;
  if(overlay.classList.contains('open')&&['ArrowDown','ArrowUp','Enter'].includes(e.key)){e.preventDefault();const rows=[...cmdlist.querySelectorAll('.cmdrow')];if(e.key==='Enter'){const command=currentCommands[sel];closeCmd();command?.a?.();return;}sel=e.key==='ArrowDown'?Math.min(rows.length-1,sel+1):Math.max(0,sel-1);rows.forEach((row,index)=>row.classList.toggle('sel',index===sel));rows[sel]?.scrollIntoView({block:'nearest'});return;}
  if(bindingMatches('palette',e)){e.preventDefault();overlay.classList.contains('open')?closeCmd():openCmd();}
  if(!overlay.classList.contains('open')&&!target.matches('input,textarea,select,[contenteditable="true"]')){
    const actions={compose:()=>document.getElementById('composeBtn').click(),reply:()=>openComposerForMessage('reply'),reply_all:()=>openComposerForMessage('replyall'),forward:()=>openComposerForMessage('forward'),archive:()=>performMessageAction('archive'),delete:()=>performMessageAction('trash'),snooze:()=>document.querySelector('[data-act="snooze"]')?.click()};
    const matched=Object.keys(actions).find(action=>bindingMatches(action,e));if(matched){e.preventDefault();actions[matched]();}
    const forward=bindingMatches('next_message',e)||e.code==='ArrowDown',backward=bindingMatches('prev_message',e)||e.code==='ArrowUp';if(forward||backward){e.preventDefault();const active=currentMessageRows.findIndex(message=>message.id===activeMessage?.id),next=forward?Math.min(currentMessageRows.length-1,active+1):Math.max(0,active<0?0:active-1);focusMessageAt(next);}
    if(e.code==='KeyU'&&!e.ctrlKey&&!e.metaKey&&!e.altKey){e.preventDefault();activeMessage&&window.tm?.markSeen(activeMessage.id,false).then(()=>window.reloadCoreData());}
    if(e.code==='Enter'&&activeMessage){e.preventDefault();const row=document.querySelector(`.msg[data-message-id="${activeMessage.id}"]`);row?.click();}
  }
  if((e.ctrlKey||e.metaKey)&&!e.shiftKey&&!e.altKey&&e.code==='KeyA'&&document.getElementById('mailView').classList.contains('active')&&!overlay.classList.contains('open')&&!target.matches('input,textarea,select,[contenteditable="true"]')){e.preventDefault();selectAllCurrentMessages();}
  if(e.key==='Escape'){closeCmd();pop.classList.remove('open');closeSmart();ctxmenu.classList.remove('open');ctxsmart.classList.remove('open');ctxfolder.classList.remove('open');filterMenu?.classList.add('hidden');sortMenu?.classList.add('hidden');}});

/* Keyboard and screen-reader semantics for code-generated controls. */
function enhanceAccessibility(scope=document){scope.querySelectorAll('.acc-h,.tmi,.ccard,.swatch,.wtheme,.wlang').forEach(element=>{if(!element.hasAttribute('role'))element.setAttribute('role','button');if(!element.hasAttribute('tabindex'))element.tabIndex=0;});scope.querySelectorAll('.toggle').forEach(toggle=>{toggle.setAttribute('role','switch');toggle.tabIndex=0;toggle.setAttribute('aria-checked',String(toggle.classList.contains('on')));});scope.querySelectorAll('.help[data-tip]').forEach(help=>{help.tabIndex=0;help.setAttribute('role','note');help.setAttribute('aria-label',help.dataset.tip);});}
enhanceAccessibility();
document.addEventListener('keydown',event=>{if((event.key==='Enter'||event.key===' ')&&event.target.matches('[role="button"],[role="switch"]')){event.preventDefault();event.target.click();}});
const accessibilityObserver=new MutationObserver(records=>{for(const record of records){if(record.type==='childList')record.addedNodes.forEach(node=>{if(node.nodeType===1)enhanceAccessibility(node);});else if(record.target.matches?.('.toggle'))record.target.setAttribute('aria-checked',String(record.target.classList.contains('on')));}});accessibilityObserver.observe(document.body,{subtree:true,childList:true,attributes:true,attributeFilter:['class']});
document.querySelectorAll('.toggle').forEach(t=>t.onclick=()=>t.classList.toggle('on'));

/* Keep scrollbars out of the way until their surface is actually moving. */
const scrollbarIdleTimers=new WeakMap();
function revealActiveScrollbar(target){
  if(!(target instanceof HTMLElement))return;
  target.classList.add('is-scrolling');
  clearTimeout(scrollbarIdleTimers.get(target));
  scrollbarIdleTimers.set(target,setTimeout(()=>{target.classList.remove('is-scrolling');scrollbarIdleTimers.delete(target);},900));
}
document.addEventListener('scroll',event=>revealActiveScrollbar(event.target),{capture:true,passive:true});

