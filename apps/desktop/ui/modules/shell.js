// truemail UI module: shell.js
const S=(p,w=18)=>`<svg width="${w}" height="${w}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">${p}</svg>`;
const ic={
  inbox:S('<path d="M22 12h-6l-2 3h-4l-2-3H2"/><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/>'),
  star:S('<polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/>'),
  send:S('<path d="m22 2-7 20-4-9-9-4Z"/><path d="M22 2 11 13"/>'),
  draft:S('<path d="M12 20h9"/><path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z"/>'),
  archive:S('<rect x="2" y="4" width="20" height="5" rx="1"/><path d="M4 9v9a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9"/><path d="M10 13h4"/>'),
  trash:S('<path d="M3 6h18"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>'),
  spam:S('<path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/><path d="M12 9v4M12 17h.01"/>'),
  cal:S('<rect x="3" y="4" width="18" height="18" rx="2"/><path d="M16 2v4M8 2v4M3 10h18"/>'),
  people:S('<path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M22 21v-2a4 4 0 0 0-3-3.87"/>'),
  sun:S('<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>'),
  compose:S('<path d="M12 20h9"/><path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z"/>',16),
  filter:S('<polygon points="22 3 2 3 10 12.46 10 19 14 21 14 12.46 22 3"/>'),
  sort:S('<path d="M11 5h10M11 9h7M11 13h4M3 17l3 3 3-3M6 18V4"/>'),
  search:S('<circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/>'),
  reply:S('<polyline points="9 17 4 12 9 7"/><path d="M20 18v-2a4 4 0 0 0-4-4H4"/>',16),
  replyall:S('<polyline points="7 17 2 12 7 7"/><polyline points="12 17 7 12 12 7"/><path d="M22 18v-2a4 4 0 0 0-4-4H7"/>',16),
  forward:S('<polyline points="15 17 20 12 15 7"/><path d="M4 18v-2a4 4 0 0 1 4-4h12"/>',16),
  snooze:S('<circle cx="12" cy="13" r="8"/><path d="M12 9v4l2 2M5 3 2 6M22 6l-3-3"/>'),
  chevL:S('<path d="m15 18-6-6 6-6"/>'), chevR:S('<path d="m9 18 6-6-6-6"/>'),
  paperclip:S('<path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48"/>'),
  shield:S('<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/><path d="m9 12 2 2 4-4"/>'),
  settings:S('<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>'),
  palette:S('<circle cx="13.5" cy="6.5" r=".8" fill="currentColor"/><circle cx="17.5" cy="10.5" r=".8" fill="currentColor"/><circle cx="8.5" cy="7.5" r=".8" fill="currentColor"/><circle cx="6.5" cy="12.5" r=".8" fill="currentColor"/><path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.9 0 1.7-.7 1.7-1.6 0-.4-.2-.8-.5-1.1-.3-.3-.5-.7-.5-1.1 0-.9.8-1.6 1.7-1.6H16c3.3 0 6-2.7 6-6 0-4.4-4.5-8-10-8z"/>'),
  user:S('<circle cx="12" cy="8" r="4"/><path d="M4 21v-1a6 6 0 0 1 6-6h4a6 6 0 0 1 6 6v1"/>'),
  globe:S('<circle cx="12" cy="12" r="10"/><path d="M2 12h20M12 2a15 15 0 0 1 0 20M12 2a15 15 0 0 0 0 20"/>'),
  plus:S('<path d="M12 5v14M5 12h14"/>',15), folder:S('<path d="M4 20a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h5l2 3h7a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2z"/>'),
  keyboard:S('<rect x="2" y="6" width="20" height="12" rx="2"/><path d="M6 10h.01M10 10h.01M14 10h.01M18 10h.01M6 14h12"/>'),
  back:S('<path d="M19 12H5M12 19l-7-7 7-7"/>'), check:S('<path d="M20 6 9 17l-5-5"/>'),
  close:S('<path d="M18 6 6 18M6 6l12 12"/>'),
  copy:S('<rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/>'),
  lock:S('<rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>'),
  key:S('<circle cx="7.5" cy="15.5" r="5.5"/><path d="m21 2-9.6 9.6M15.5 7.5l3 3L22 7l-3-3"/>'),
  server:S('<rect x="2" y="2" width="20" height="8" rx="2"/><rect x="2" y="14" width="20" height="8" rx="2"/><path d="M6 6h.01M6 18h.01"/>'),
  edit:S('<path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.12 2.12 0 0 1 3 3L12 15l-4 1 1-4z"/>'),
  flag:S('<path d="M4 15s1-1 4-1 5 2 8 2 4-1 4-1V3s-1 1-4 1-5-2-8-2-4 1-4 1z"/><path d="M4 22v-7"/>'),
  bold:S('<path d="M6 4h8a4 4 0 0 1 0 8H6zM6 12h9a4 4 0 0 1 0 8H6z"/>',16),
  italic:S('<path d="M19 4h-9M14 20H5M15 4 9 20"/>',16), underline:S('<path d="M6 4v6a6 6 0 0 0 12 0V4M4 21h16"/>',16),
  list:S('<path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01"/>',16),
  link:S('<path d="M10 13a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-1.5 1.5"/><path d="M14 11a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l1.5-1.5"/>',16),
  at:S('<circle cx="12" cy="12" r="4"/><path d="M16 8v5a3 3 0 0 0 6 0v-1a10 10 0 1 0-4 8"/>',16),
  upload:S('<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M17 8l-5-5-5 5M12 3v12"/>',16),
  download:S('<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3"/>',16),
  image:S('<rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-4.35-4.35a2 2 0 0 0-2.83 0L4 20"/>',16),
  storage:S('<ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M3 5v14a9 3 0 0 0 18 0V5"/><path d="M3 12a9 3 0 0 0 18 0"/>'),
  dots:S('<circle cx="12" cy="5" r="1.6" fill="currentColor" stroke="none"/><circle cx="12" cy="12" r="1.6" fill="currentColor" stroke="none"/><circle cx="12" cy="19" r="1.6" fill="currentColor" stroke="none"/>'),
  unsub:S('<circle cx="12" cy="12" r="9"/><path d="M5.6 5.6l12.8 12.8"/>'),
  up:S('<path d="m18 15-6-6-6 6"/>'), down:S('<path d="m6 9 6 6 6-6"/>'),
  grip:S('<circle cx="9" cy="6" r="1" fill="currentColor" stroke="none"/><circle cx="9" cy="12" r="1" fill="currentColor" stroke="none"/><circle cx="9" cy="18" r="1" fill="currentColor" stroke="none"/><circle cx="15" cy="6" r="1" fill="currentColor" stroke="none"/><circle cx="15" cy="12" r="1" fill="currentColor" stroke="none"/><circle cx="15" cy="18" r="1" fill="currentColor" stroke="none"/>'),
  print:S('<path d="M6 9V2h12v7M6 18H4a2 2 0 0 1-2-2v-5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v5a2 2 0 0 1-2 2h-2"/><rect x="6" y="14" width="12" height="8"/>'),
  translate:S('<path d="m5 8 6 6M4 14l6-6 2-3M2 5h12M7 2h1M22 22l-5-10-5 10M14 18h6"/>'),
  pin:S('<path d="M12 17v5M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z"/>'),
  openext:S('<path d="M15 3h6v6M10 14 21 3M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>'),
};
document.querySelectorAll('[data-i]').forEach(e=>{const s=ic[e.dataset.i]; if(s)e.innerHTML=s;});

/* ---------- status bar ---------- */
/* Нижняя строка состояния: одна строка текста, слева основной статус, справа - дополнительный. */
const statusbarText=document.getElementById('statusbarText'),statusbarRight=document.getElementById('statusbarRight');
window.setStatus=function(text,right){if(statusbarText)statusbarText.textContent=text||'';if(right!==undefined&&statusbarRight)statusbarRight.textContent=right||'';};

/* ---------- routing between top views ---------- */
function showView(id){ document.querySelectorAll('.view').forEach(v=>v.classList.toggle('active',v.id===id));
  if(id==='composeView'){const m=document.getElementById('compEdit');if(m){m.focus();}} }
document.getElementById('toSettings').onclick=()=>{showView('settingsView');};
document.getElementById('backToMail').onclick=()=>showView('mailView');
document.getElementById('composeBtn').onclick=async()=>{resetComposer();document.getElementById('compTitle').textContent=L('Новое письмо','New message');showView('composeView');await applyComposerSignature('new');};
document.getElementById('compClose').onclick=()=>showView('mailView');

/* settings section switching */
function setSection(id){ document.querySelectorAll('.setnav .sec').forEach(s=>s.classList.toggle('active',s.dataset.set===id));
  document.querySelectorAll('.setpage').forEach(p=>p.classList.toggle('active',p.id==='set-'+id)); }
document.querySelectorAll('[data-set]').forEach(el=>el.addEventListener('click',()=>{showView('settingsView');setSection(el.dataset.set);}));
document.querySelectorAll('[data-openacct]').forEach(el=>el.onclick=()=>setSection('accounts'));

/* ---------- account colors + messages ---------- */
const avc=['#e5342a','#0058ff','#5b63d3','#2f9e5f','#c2456b','#b5761c','#0f9b8e','#7a4fd0'];
let messages=[];
let coreFolders=[];
let coreAccounts=[];
// 16 нейтральных цветов аккаунта, читаемых в светлой и тёмной теме.
/* Палитра цветов аккаунта: сетка 5x5 в выпадающей панели. */
const ACCOUNT_COLORS=[
  '#d64545','#d9773b','#c9a227','#7a9e3a','#3f9d54',
  '#2fa39a','#3b8ed0','#5a63d8','#8158d6','#b355c0',
  '#c65a8e','#8a6d4b','#6b7280','#4f7a6a','#9c6b52',
  '#5f6b7a','#a33a3a','#b8622a','#5d8a2f','#2f7d6b',
  '#2b6ca3','#43489e','#6b3fa0','#96407d','#7a5230',
];
function accountColorById(accountId){const account=coreAccounts.find(item=>item.id===accountId);if(account&&account.color)return account.color;const index=coreAccounts.findIndex(item=>item.id===accountId);return ACCOUNT_COLORS[((index<0?Number(accountId)||0:index)%ACCOUNT_COLORS.length+ACCOUNT_COLORS.length)%ACCOUNT_COLORS.length];}
// Сортировка писем по реальному времени (учитывает таймзоны), а не по строке даты.
function messageTime(message){const value=new Date(message?.date||0).getTime();return Number.isFinite(value)?value:0;}
function byDateDesc(a,b){return messageTime(b)-messageTime(a)||(b.id-a.id);}
function byDateAsc(a,b){return messageTime(a)-messageTime(b)||(a.id-b.id);}
let coreContacts=[];
let coreCalendarData={calendars:[],events:[]};
let currentFolderId=null;
let currentSmartIndex=0;
let currentMessageRows=[];
let activeMessage=null;
let activeFullMessage=null;
let mailRules=[];
let editingRuleId=null;
const MESSAGE_INITIAL_PAGE_SIZE=100;
const MESSAGE_PAGE_SIZE=500;
const SMART_MESSAGE_PAGE_SIZE=500;
const MESSAGE_WINDOW_OVERSCAN=16;
const folderHasMore=new Map();
let loadingMoreMessages=false;
let loadingSmartCoverage=false;
let queuedSmartCoverage=null;
const smartHasMore=new Map();
let messageRowHeight=76;
let messageWindowStart=-1;
let messageWindowEnd=-1;
let messageWindowFrame=0;
const selectedMessageIds=new Set();
let lastSelectedMessageIndex=-1;
let selectionDragMode=null;
function updateSelectionUi(){
  document.querySelectorAll('.msg').forEach(row=>row.classList.toggle('selected',selectedMessageIds.has(Number(row.dataset.messageId))));
  const count=selectedMessageIds.size,bar=document.getElementById('selectionBar');bar.classList.toggle('hidden',count===0);document.getElementById('selectionCount').textContent=L(`${count} выбрано`,`${count} selected`);
}
function clearMessageSelection(){selectedMessageIds.clear();lastSelectedMessageIndex=-1;updateSelectionUi();}
function selectMessageRange(index,preserve=false){if(lastSelectedMessageIndex<0){selectedMessageIds.add(currentMessageRows[index].id);lastSelectedMessageIndex=index;updateSelectionUi();return;}if(!preserve)selectedMessageIds.clear();const from=Math.min(index,lastSelectedMessageIndex),to=Math.max(index,lastSelectedMessageIndex);for(let i=from;i<=to;i++)selectedMessageIds.add(currentMessageRows[i].id);updateSelectionUi();}
document.addEventListener('pointerup',()=>{selectionDragMode=null;});
function renderIcons(root){root.querySelectorAll('[data-i]').forEach(e=>{const s=ic[e.dataset.i];if(s)e.innerHTML=s;});}

const msgsEl=document.getElementById('msgs');
async function loadNextMessagePage(){
  if(currentFolderId===null){if(currentSmartIndex!==null)loadSmartCoveragePage(currentSmartIndex);return;}if(loadingMoreMessages)return;const folderIds=folderHasMore.get(currentFolderId)===false?[]:[currentFolderId];if(!folderIds.length)return;
  loadingMoreMessages=true;
  try{
    const known=new Set(messages.map(message=>message.id));for(const folderId of folderIds){const loaded=messages.filter(message=>message.folder_id===folderId).sort(byDateDesc),cursor=loaded.at(-1);if(!cursor){folderHasMore.set(folderId,false);continue;}const page=await window.tm?.listMessagesPage(folderId,cursor.date||'',cursor.id,MESSAGE_PAGE_SIZE)||[];messages.push(...page.filter(message=>!known.has(message.id)));page.forEach(message=>known.add(message.id));folderHasMore.set(folderId,page.length===MESSAGE_PAGE_SIZE);}
    if(currentFolderId!==null||currentSmartIndex!==null)applyListOptions(false);
  }catch(error){console.error('truemail pagination:',error);}finally{loadingMoreMessages=false;}
}
msgsEl.addEventListener('scroll',()=>{if(!messageWindowFrame)messageWindowFrame=requestAnimationFrame(()=>{messageWindowFrame=0;renderMessageWindow();});if(msgsEl.scrollTop+msgsEl.clientHeight>=msgsEl.scrollHeight-240)loadNextMessagePage();},{passive:true});

/* thread action buttons -> compose */
document.querySelectorAll('.thead [data-act]').forEach(b=>b.onclick=()=>{
  if(['reply','replyall','forward'].includes(b.dataset.act))openComposerForMessage(b.dataset.act);
  else if(['archive','trash'].includes(b.dataset.act))performMessageAction(b.dataset.act);});

