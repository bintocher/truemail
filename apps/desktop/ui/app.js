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
const MESSAGE_PAGE_SIZE=100;
const MESSAGE_WINDOW_OVERSCAN=16;
const folderHasMore=new Map();
let loadingMoreMessages=false;
let loadingSmartCoverage=false;
let queuedSmartCoverageIndex=null;
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
  if(loadingMoreMessages)return;const folderIds=currentFolderId!==null?[currentFolderId]:coreFolders.filter(folder=>window.coreUnifiedSettings?.[folder.id]!=='0'&&folderHasMore.get(folder.id)!==false).map(folder=>folder.id);if(!folderIds.length)return;
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

/* calendar */
const cg=document.getElementById('calgrid');
let calendarCursor=new Date();
function parseDavDate(value){if(!value)return null;if(/^\d{8}/.test(value)){const m=value.match(/^(\d{4})(\d{2})(\d{2})(?:T(\d{2})(\d{2})(\d{2}))?/);if(m){const parts=[+m[1],+m[2]-1,+m[3],+(m[4]||0),+(m[5]||0),+(m[6]||0)];return /Z$/i.test(value)?new Date(Date.UTC(...parts)):new Date(...parts);}}const date=new Date(value);return Number.isNaN(date.getTime())?null:date;}
function expandCalendarEvents(events,rangeStart,rangeEnd){
  const overrides=new Map(),output=[];events.filter(event=>event.recurrence_id).forEach(event=>{const date=parseDavDate(event.recurrence_id);if(date)overrides.set(`${event.uid||''}:${date.getTime()}`,event);});
  const add=(event,date)=>{if(date>=rangeStart&&date<rangeEnd){const override=overrides.get(`${event.uid||''}:${date.getTime()}`);output.push({...event,...(override||{}),dtstart:(override?.dtstart)||date.toISOString()});}};
  events.filter(event=>!event.recurrence_id).forEach(event=>{const first=parseDavDate(event.dtstart);if(!first)return;const excluded=new Set(String(event.exdates||'').split(',').map(parseDavDate).filter(Boolean).map(date=>date.getTime()));if(!excluded.has(first.getTime()))add(event,first);
    String(event.rdates||'').split(',').map(parseDavDate).filter(Boolean).forEach(date=>{if(!excluded.has(date.getTime()))add(event,date);});if(!event.rrule)return;
    const rule=Object.fromEntries(String(event.rrule).split(';').map(part=>part.split('=',2)));const frequency=rule.FREQ,interval=Math.max(1,+rule.INTERVAL||1),count=+rule.COUNT||Infinity,until=parseDavDate(rule.UNTIL)||rangeEnd,byDay=(rule.BYDAY||'').split(',').filter(Boolean).map(value=>value.slice(-2)),byMonthDay=(rule.BYMONTHDAY||'').split(',').map(Number).filter(Number.isFinite);const weekDays=['SU','MO','TU','WE','TH','FR','SA'];let emitted=1,cursor=new Date(first);cursor.setDate(cursor.getDate()+1);for(let scanned=0;cursor<=until&&cursor<rangeEnd&&emitted<count&&scanned<36600;scanned++,cursor.setDate(cursor.getDate()+1)){const days=Math.floor((cursor-first)/86400000),months=(cursor.getFullYear()-first.getFullYear())*12+cursor.getMonth()-first.getMonth(),years=cursor.getFullYear()-first.getFullYear();let matches=false;if(frequency==='DAILY')matches=days%interval===0;else if(frequency==='WEEKLY')matches=Math.floor(days/7)%interval===0&&(byDay.length?byDay.includes(weekDays[cursor.getDay()]):cursor.getDay()===first.getDay());else if(frequency==='MONTHLY')matches=months>=0&&months%interval===0&&(byMonthDay.length?byMonthDay.includes(cursor.getDate()):cursor.getDate()===first.getDate());else if(frequency==='YEARLY')matches=years>=0&&years%interval===0&&cursor.getMonth()===first.getMonth()&&cursor.getDate()===first.getDate();if(matches){emitted++;if(!excluded.has(cursor.getTime()))add(event,new Date(cursor));}}
  });
  overrides.forEach(event=>{const date=parseDavDate(event.dtstart);if(date&&!output.some(item=>item.id===event.id))add(event,date);});return output;
}
function localeName(date,options){return new Intl.DateTimeFormat(wizardLocale||'ru',options).format(date);}
function renderCalendarData(data=coreCalendarData){
  coreCalendarData=data||{calendars:[],events:[]};const events=coreCalendarData.events||[];
  const year=calendarCursor.getFullYear(),month=calendarCursor.getMonth(),displayEvents=expandCalendarEvents(events,new Date(year,month-1,20),new Date(year,month+2,10));document.getElementById('calTitle').textContent=localeName(calendarCursor,{month:'long',year:'numeric'});
  cg.innerHTML='';const start=(new Date(year,month,1).getDay()+6)%7,days=new Date(year,month+1,0).getDate(),prevDays=new Date(year,month,0).getDate();
  let visibleEvents=0;for(let i=0;i<42;i++){const day=i-start+1,current=i>=start&&day<=days,date=new Date(year,month,current?day:day<1?day:day);const number=current?day:day<1?prevDays+day:day-days;
    const cell=document.createElement('div');cell.className='calcell'+(!current?' other':'')+(new Date().toDateString()===date.toDateString()?' today':'');cell.innerHTML=`<div class="d${current?'':' d-dim'}">${number}</div>`;
    if(current){cell.onclick=()=>{calendarCursor=new Date(date);};displayEvents.filter(event=>parseDavDate(event.dtstart)?.toDateString()===date.toDateString()).forEach((event,index)=>{visibleEvents++;const item=document.createElement('div');item.className=`ev ev-c${index%4}`;item.dataset.eventId=event.id;item.textContent=event.summary;item.style.cssText=eventColorStyle(event);cell.appendChild(item);});}cg.appendChild(cell);}
  const info=document.getElementById('calSyncInfo');if(info){const dated=events.map(event=>({date:parseDavDate(event.dtstart)})).filter(item=>item.date).sort((a,b)=>b.date-a.date),latest=dated[0];info.textContent=L(`${coreCalendarData.calendars?.length||0} календаря · ${events.length} событий${visibleEvents?'':latest?' · показать последние':' · событий нет'}`,`${coreCalendarData.calendars?.length||0} calendars · ${events.length} events${visibleEvents?'':latest?' · show latest':' · no events'}`);info.classList.toggle('clickable',!visibleEvents&&Boolean(latest));info.onclick=!visibleEvents&&latest?()=>{calendarCursor=new Date(latest.date.getFullYear(),latest.date.getMonth(),1);renderCalendarData();}:null;info.title=!visibleEvents&&latest?L(`Перейти к ${localeName(latest.date,{month:'long',year:'numeric'})}`,`Go to ${localeName(latest.date,{month:'long',year:'numeric'})}`):'';}
  renderWeekDay(events);
  const count=document.querySelector('[data-nav="calendar"] .count');if(count)count.textContent=events.length||'';
}
const WK_HOUR=48; // высота одного часа в пикселях (совпадает с --wk-hour в CSS)
// Интервал события: начало и конец (dtend или +30 мин по умолчанию).
function eventInterval(event){const start=parseDavDate(event.dtstart);if(!start)return null;let end=event.dtend?parseDavDate(event.dtend):null;if(!end||end<=start)end=new Date(start.getTime()+30*60000);return {event,start,end};}
// Раскладка пересекающихся событий по колонкам: каждое кладём в первую свободную
// колонку кластера; ширина колонки = 100%/число колонок кластера. Наложений нет.
function layoutColumns(items){
  const sorted=items.map(it=>({...it,s:it.start.getTime(),e:it.end.getTime()})).sort((a,b)=>a.s-b.s||a.e-b.e);
  let cluster=[],clusterEnd=-Infinity;
  const flush=()=>{const colEnds=[];cluster.forEach(it=>{let col=0;for(;col<colEnds.length;col++)if(colEnds[col]<=it.s)break;it.col=col;colEnds[col]=it.e;});const cols=colEnds.length;cluster.forEach(it=>it.cols=cols);};
  sorted.forEach(it=>{if(it.s>=clusterEnd&&cluster.length){flush();cluster=[];clusterEnd=-Infinity;}cluster.push(it);clusterEnd=Math.max(clusterEnd,it.e);});
  if(cluster.length)flush();
  return sorted;
}
// HTML событий одного дня: top/height по времени, left/width по колонкам.
function renderDayColumn(dayStart,items){
  const dayEnd=new Date(dayStart.getFullYear(),dayStart.getMonth(),dayStart.getDate()+1);
  const clipped=items.map(it=>({...it,start:it.start<dayStart?dayStart:it.start,end:it.end>dayEnd?dayEnd:it.end})).filter(it=>it.end>it.start);
  return layoutColumns(clipped).map(it=>{
    const minutesTop=it.start.getHours()*60+it.start.getMinutes();
    const durationMin=Math.max((it.end-it.start)/60000,20);
    const top=minutesTop/60*WK_HOUR,height=durationMin/60*WK_HOUR;
    const width=100/it.cols,left=it.col*width;
    const color=eventAccountColor(it.event);
    const paint=color?`border-left:3px solid ${color};background:${color}22;`:'';
    const style=`top:${top}px;height:${Math.max(height-2,16)}px;left:calc(${left}% + 2px);width:calc(${width}% - 4px);${paint}`;
    return `<div class="wk-ev" data-event-id="${it.event.id}" style="${style}" title="${escapeHtml(it.event.summary)}">${escapeHtml(it.event.summary)}</div>`;
  }).join('');
}
function timesColumn(){let out='<div class="wk-times">';for(let hr=0;hr<24;hr++)out+=`<div class="wk-tlabel">${String(hr).padStart(2,'0')}:00</div>`;return out+'</div>';}
function renderWeekDay(events){
  const base=new Date(calendarCursor),monday=new Date(base);monday.setDate(base.getDate()-((base.getDay()+6)%7));
  const expanded=expandCalendarEvents(events,new Date(monday.getFullYear(),monday.getMonth(),monday.getDate()-1),new Date(monday.getFullYear(),monday.getMonth(),monday.getDate()+9));
  const intervals=expanded.map(eventInterval).filter(Boolean);
  const dayItems=(d)=>{const next=new Date(d.getFullYear(),d.getMonth(),d.getDate()+1);return intervals.filter(it=>it.start<next&&it.end>d);};
  // Неделя
  let head='<div class="wk-corner"></div>',cols='';
  for(let i=0;i<7;i++){const d=new Date(monday);d.setDate(monday.getDate()+i);const wd=localeName(d,{weekday:'short'}).replace('.','');const today=new Date().toDateString()===d.toDateString();head+=`<div class="wk-dayhd${today?' today':''}">${wizardLocale==='ru'?wd.slice(0,2):wd.slice(0,3)}<b>${d.getDate()}</b></div>`;const day=new Date(d.getFullYear(),d.getMonth(),d.getDate());cols+=`<div class="wk-daycol">${renderDayColumn(day,dayItems(day))}</div>`;}
  document.getElementById('calweek').innerHTML=`<div class="wk-head">${head}</div><div class="wk-scroll">${timesColumn()}<div class="wk-cols">${cols}</div></div>`;
  // День
  const dayD=new Date(base.getFullYear(),base.getMonth(),base.getDate()),dToday=new Date().toDateString()===base.toDateString(),dwd=localeName(base,{weekday:'short'}).replace('.','');
  document.getElementById('calday').innerHTML=`<div class="wk-head wk-head-day"><div class="wk-corner"></div><div class="wk-dayhd${dToday?' today':''}">${wizardLocale==='ru'?dwd.slice(0,2):dwd.slice(0,3)}<b>${base.getDate()}</b></div></div><div class="wk-scroll">${timesColumn()}<div class="wk-cols wk-cols-day"><div class="wk-daycol">${renderDayColumn(dayD,dayItems(dayD))}</div></div></div>`;
}
function escapeHtml(value){return String(value||'').replace(/[&<>"']/g,ch=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[ch]));}
// Цвет события = цвет его аккаунта (через календарь), как у писем в списке.
function eventAccountColor(event){const cal=(coreCalendarData.calendars||[]).find(item=>item.id===event.calendar_id);return cal?accountColorById(cal.account_id):null;}
function eventColorStyle(event){const color=eventAccountColor(event);return color?`border-left:3px solid ${color};background:${color}22`:'';}

/* contacts */
const cts=[];
const cgrid=document.getElementById('cgrid');
cts.forEach(([n,e,c])=>{const ini=n.split(' ').map(x=>x[0]).join('');const card=document.createElement('div');card.className='ccard';
  card.innerHTML=`<span class="ava ava-c${c}">${ini}</span><div><div class="cn">${n}</div><div class="ce">${e}</div></div>`;cgrid.appendChild(card);});

/* nav sections (mail/calendar/contacts) */
document.querySelectorAll('.navitem[data-nav]').forEach(n=>n.onclick=()=>{
  document.querySelectorAll('.navitem').forEach(x=>x.classList.remove('active'));n.classList.add('active');
  const app=document.getElementById('app');app.classList.toggle('calmode',n.dataset.nav==='calendar');app.classList.toggle('contactsmode',n.dataset.nav==='contacts');});
/* account collapse */
// Делегирование: работает и для аккаунтов, отрисованных динамически после загрузки.
document.addEventListener('click',event=>{const h=event.target.closest('.acc-h');if(!h)return;h.classList.toggle('open');h.nextElementSibling?.classList.toggle('open');});

/* collapsible sidebar groups */
document.querySelectorAll('.nav .navlabel').forEach(lbl=>{
  lbl.classList.add('clp');
  const chev=document.createElement('span');chev.className='clp-chev';chev.innerHTML=ic.down;lbl.insertBefore(chev,lbl.firstChild);
  lbl.addEventListener('click',e=>{ if(e.target.closest('.add'))return;
    lbl.classList.toggle('collapsed');const hide=lbl.classList.contains('collapsed');let el=lbl.nextElementSibling;
    while(el&&!el.classList.contains('navlabel')){ if(el.classList.contains('navitem')||el.classList.contains('acc-h')||el.classList.contains('acc-sub'))el.classList.toggle('grouphide',hide); el=el.nextElementSibling; } });
});

/* custom right-click menu (suppress browser default) */
const ctxmenu=document.getElementById('ctxmenu'),ctxsmart=document.getElementById('ctxsmart'),ctxfolder=document.getElementById('ctxfolder');
let contextFolder=null,contextFolderOpen=null;
function posMenu(menu,e){menu.style.left=Math.min(e.clientX,window.innerWidth-244)+'px';menu.style.top=Math.min(e.clientY,window.innerHeight-330)+'px';menu.classList.add('open');}
document.addEventListener('contextmenu',e=>{if(e.target.closest('input,textarea,select,[contenteditable="true"]'))return;e.preventDefault();
  ctxmenu.classList.remove('open');ctxsmart.classList.remove('open');ctxfolder.classList.remove('open');
  const msg=e.target.closest('.msg'),smart=e.target.closest('[data-smart-index]');
  if(msg){const id=Number(msg.dataset.messageId);activeMessage=messages.find(item=>item.id===id)||activeMessage;buildContextMenu();posMenu(ctxmenu,e);}else if(smart){ctxsmart.dataset.index=smart.dataset.smartIndex;posMenu(ctxsmart,e);} });
document.addEventListener('click',()=>{ctxmenu.classList.remove('open');ctxsmart.classList.remove('open');ctxfolder.classList.remove('open');});
[ctxsmart,ctxfolder].forEach(m=>m.querySelectorAll('.tmi').forEach(i=>i.onclick=()=>m.classList.remove('open')));
// Меню флажков (пользовательских меток) для письма.
async function openFlagMenu(message,event){
  if(!message){showToast(L('Сначала выберите письмо','Select a message first'));return;}
  document.querySelector('.att-menu')?.remove();
  let labels=[],active=[];
  try{[labels,active]=await Promise.all([window.tm.listLabels(),window.tm.messageLabelIds(message.id)]);}catch(error){showToast(error.message||String(error));return;}
  const activeSet=new Set(active);
  const menu=document.createElement('div');menu.className='att-menu flag-menu';
  labels.forEach(label=>{
    const item=document.createElement('button');item.type='button';item.className='flag-item';
    item.innerHTML='<span class="flag-dot"></span><span class="flag-name"></span><span class="flag-check"></span>';
    item.querySelector('.flag-dot').style.background=label.color||'#888';
    item.querySelector('.flag-name').textContent=label.name;
    item.querySelector('.flag-check').textContent=activeSet.has(label.id)?'✓':'';
    item.onclick=async e=>{e.stopPropagation();const on=!activeSet.has(label.id);try{await window.tm.toggleMessageLabel(message.id,label.id,on);if(on)activeSet.add(label.id);else activeSet.delete(label.id);item.querySelector('.flag-check').textContent=on?'✓':'';await window.reloadCoreData?.();}catch(error){showToast(error.message||String(error));}};
    menu.appendChild(item);
  });
  if(labels.length){const sep=document.createElement('div');sep.className='tmsep';menu.appendChild(sep);}
  const create=document.createElement('button');create.type='button';create.className='flag-item flag-create';create.innerHTML=`<span class="flag-dot flag-dot-new"></span><span>${L('Создать метку…','Create label…')}</span>`;
  create.onclick=e=>{e.stopPropagation();menu.remove();openLabelCreator(message);};
  menu.appendChild(create);
  document.body.appendChild(menu);
  const w=menu.offsetWidth,h=menu.offsetHeight;
  menu.style.left=Math.min(event.clientX,innerWidth-w-8)+'px';menu.style.top=Math.min(event.clientY,innerHeight-h-8)+'px';
  setTimeout(()=>document.addEventListener('click',()=>menu.remove(),{once:true}),0);
}
// Создание метки: имя + цвет (16 нейтральных).
function openLabelCreator(message){
  const overlay=document.createElement('div');overlay.className='raw-overlay';
  overlay.innerHTML=`<div class="label-box"><h3>${L('Новая метка','New label')}</h3><input class="inp label-name" placeholder="${L('Название метки','Label name')}" maxlength="40"><div class="label-colors"></div><div class="label-actions"><button type="button" class="btn label-cancel">${L('Отмена','Cancel')}</button><button type="button" class="btn primary label-save">${L('Создать','Create')}</button></div></div>`;
  document.body.appendChild(overlay);
  let chosen=ACCOUNT_COLORS[0];
  const colors=overlay.querySelector('.label-colors');
  ACCOUNT_COLORS.forEach((color,index)=>{const swatch=document.createElement('button');swatch.type='button';swatch.className='color-swatch'+(index===0?' on':'');swatch.style.background=color;swatch.onclick=()=>{chosen=color;colors.querySelectorAll('.color-swatch').forEach(item=>item.classList.toggle('on',item===swatch));};colors.appendChild(swatch);});
  const close=()=>overlay.remove();
  overlay.querySelector('.label-cancel').onclick=close;
  overlay.onclick=e=>{if(e.target===overlay)close();};
  overlay.querySelector('.label-save').onclick=async()=>{const name=overlay.querySelector('.label-name').value.trim();if(!name){showToast(L('Введите название метки','Enter a label name'));return;}try{const id=await window.tm.createLabel(name,chosen);if(message)await window.tm.toggleMessageLabel(message.id,id,true);await window.reloadCoreData?.();close();showToast(L('Метка создана','Label created'));}catch(error){showToast(error.message||String(error));}};
  overlay.querySelector('.label-name').focus();
}
// ПКМ-меню письма = все действия панели письма (tbActions, даже выключенные) + доп.
function buildContextMenu(){
  ctxmenu.innerHTML='';
  tbActions.forEach(action=>{const item=document.createElement('div');item.className='tmi';item.dataset.contextAction=action.k;item.innerHTML=`<i data-i="${action.i||action.k}"></i>${escapeHtml(tbLabel(action))}`;ctxmenu.appendChild(item);});
  const sep=document.createElement('div');sep.className='tmsep';ctxmenu.appendChild(sep);
  (smartIsEnglish()?[['flag','flag','Flag'],['raw','edit','View source'],['create-rule','filter','Create rule']]:[['flag','flag','Флажок'],['raw','edit','Исходный текст'],['create-rule','filter','Создать правило']]).forEach(([act,icon,label])=>{const item=document.createElement('div');item.className='tmi';item.dataset.contextAction=act;item.innerHTML=`<i data-i="${icon}"></i>${label}`;ctxmenu.appendChild(item);});
  renderIcons(ctxmenu);
}
ctxmenu.addEventListener('click',async event=>{const item=event.target.closest('[data-context-action]');if(!item)return;ctxmenu.classList.remove('open');const action=item.dataset.contextAction;
  if(action==='raw'){openRawViewer(activeMessage?.id);return;}
  if(action==='create-rule'){openRuleEditor(activeMessage);return;}
  if(action==='flag'){openFlagMenu(activeMessage,event);return;}
  executeToolbarAction(action);
});
ctxfolder.querySelectorAll('[data-folder-action]').forEach(item=>item.addEventListener('click',async()=>{if(item.classList.contains('disabled')||!contextFolder)return;const action=item.dataset.folderAction;if(action==='open'){contextFolderOpen?.();return;}if(action==='settings'){showView('settingsView');setSection('folders');return;}if(action==='rename'){const name=prompt(L('Новое имя папки','New folder name'),contextFolder.display_name);if(!name||name.trim()===contextFolder.display_name)return;try{await window.tm.renameFolder(contextFolder.id,name.trim());await window.reloadCoreData();showToast(L('Папка переименована на сервере','Folder renamed on the server'));}catch(error){showToast(error.message||String(error));}return;}if(action==='delete'){if(!confirm(L(`Удалить папку «${contextFolder.display_name}» на сервере? Письма внутри также будут удалены.`,`Delete the folder "${contextFolder.display_name}" on the server? Messages inside will also be deleted.`)))return;try{await window.tm.deleteFolder(contextFolder.id);await window.reloadCoreData();showToast(L('Папка удалена на сервере','Folder deleted on the server'));}catch(error){showToast(error.message||String(error));}}}));

/* theme settings */
const root=document.documentElement,pop=document.getElementById('pop');
document.getElementById('toThemes').onclick=()=>{pop.classList.remove('open');showView('settingsView');setSection('themes');};
function setTheme(t,persist=true){if(t==='auto')root.removeAttribute('data-theme');else root.setAttribute('data-theme',t);
  try{if(t==='auto')localStorage.removeItem('truemail-theme');else localStorage.setItem('truemail-theme',t);}catch(_){}
  document.querySelectorAll('[data-theme]').forEach(b=>{if(b.tagName==='BUTTON')b.classList.toggle('on',b.dataset.theme===t);});
  if(persist)window.tm?.setSetting('theme',t).catch(console.error);}
document.querySelectorAll('#segTheme button, #setTheme button').forEach(b=>b.onclick=()=>setTheme(b.dataset.theme));
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
window.openMessageById=function(id){const message=messages.find(item=>item.id===id);if(!message)return;goMail();showMessage(message);};
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
document.addEventListener('keydown',e=>{
  if(overlay.classList.contains('open')&&['ArrowDown','ArrowUp','Enter'].includes(e.key)){e.preventDefault();const rows=[...cmdlist.querySelectorAll('.cmdrow')];if(e.key==='Enter'){const command=currentCommands[sel];closeCmd();command?.a?.();return;}sel=e.key==='ArrowDown'?Math.min(rows.length-1,sel+1):Math.max(0,sel-1);rows.forEach((row,index)=>row.classList.toggle('sel',index===sel));rows[sel]?.scrollIntoView({block:'nearest'});return;}
  if(e.ctrlKey&&e.shiftKey&&['KeyC','KeyF','KeyM'].includes(e.code)){e.preventDefault();e.stopPropagation();if(e.code==='KeyC')document.getElementById('composeBtn').click();if(e.code==='KeyF')openCmd();return;}
  if((e.ctrlKey||e.metaKey)&&e.code==='KeyK'){e.preventDefault();overlay.classList.contains('open')?closeCmd():openCmd();}
  const target=e.target;if(!e.ctrlKey&&!e.metaKey&&!e.altKey&&!overlay.classList.contains('open')&&!target.matches('input,textarea,select,[contenteditable="true"]')){
    const actions={KeyC:()=>document.getElementById('composeBtn').click(),KeyR:()=>openComposerForMessage('reply'),KeyA:()=>openComposerForMessage('replyall'),KeyF:()=>openComposerForMessage('forward'),KeyE:()=>performMessageAction('archive'),KeyU:()=>activeMessage&&window.tm?.markSeen(activeMessage.id,false).then(()=>window.reloadCoreData()),Delete:()=>performMessageAction('trash')};
    if(actions[e.code]){e.preventDefault();actions[e.code]();}
    if(['KeyJ','KeyK','ArrowDown','ArrowUp'].includes(e.code)){e.preventDefault();const active=currentMessageRows.findIndex(message=>message.id===activeMessage?.id),forward=e.code==='KeyJ'||e.code==='ArrowDown',next=forward?Math.min(currentMessageRows.length-1,active+1):Math.max(0,active<0?0:active-1);focusMessageAt(next);}
    if(e.code==='Enter'&&activeMessage){e.preventDefault();const row=document.querySelector(`.msg[data-message-id="${activeMessage.id}"]`);row?.click();}
  }
  if((e.ctrlKey||e.metaKey)&&!e.shiftKey&&!e.altKey&&e.code==='KeyA'&&document.getElementById('mailView').classList.contains('active')&&!overlay.classList.contains('open')&&!target.matches('input,textarea,select,[contenteditable="true"]')){e.preventDefault();selectAllCurrentMessages();}
  if(e.key==='Escape'){closeCmd();pop.classList.remove('open');closeSmart();ctxmenu.classList.remove('open');ctxsmart.classList.remove('open');ctxfolder.classList.remove('open');filterMenu?.classList.add('hidden');sortMenu?.classList.add('hidden');}});

/* Keyboard and screen-reader semantics for code-generated controls. */
function enhanceAccessibility(scope=document){scope.querySelectorAll('.navitem,.setnav .sec,.acc-h,.tmi,.ccard,.swatch,.wtheme,.wlang').forEach(element=>{if(!element.hasAttribute('role'))element.setAttribute('role','button');if(!element.hasAttribute('tabindex'))element.tabIndex=0;});scope.querySelectorAll('.toggle').forEach(toggle=>{toggle.setAttribute('role','switch');toggle.tabIndex=0;toggle.setAttribute('aria-checked',String(toggle.classList.contains('on')));});scope.querySelectorAll('.help[data-tip]').forEach(help=>{help.tabIndex=0;help.setAttribute('role','note');help.setAttribute('aria-label',help.dataset.tip);});}
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

/* calendar view switch + week/day render */
const calSection=document.getElementById('calSection');
document.querySelectorAll('#calViews button').forEach(b=>b.onclick=()=>{
  document.querySelectorAll('#calViews button').forEach(x=>x.classList.toggle('on',x===b));
  calSection.dataset.cv=b.dataset.cv;
  if(b.dataset.cv==='month')renderCalendarData();else {renderWeekDay(coreCalendarData.events||[]);const wd=localeName(calendarCursor,{weekday:'short'}).replace('.','');document.getElementById('calTitle').textContent=b.dataset.cv==='week'?localeName(calendarCursor,{month:'long',year:'numeric'}):`${wizardLocale==='ru'?wd.slice(0,2):wd.slice(0,3)}, ${localeName(calendarCursor,{day:'numeric',month:'long'})}`;}});
document.querySelectorAll('.calhead > .iconbtn').forEach((button,index)=>button.onclick=()=>{const direction=index===0?-1:1,view=calSection.dataset.cv||'month';if(view==='day')calendarCursor.setDate(calendarCursor.getDate()+direction);else if(view==='week')calendarCursor.setDate(calendarCursor.getDate()+7*direction);else {const day=calendarCursor.getDate();calendarCursor.setDate(1);calendarCursor.setMonth(calendarCursor.getMonth()+direction);calendarCursor.setDate(Math.min(day,new Date(calendarCursor.getFullYear(),calendarCursor.getMonth()+1,0).getDate()));}renderCalendarData();if(view!=='month')document.querySelector(`#calViews button[data-cv="${view}"]`)?.click();});

/* smart folder modal */
const smartOverlay=document.getElementById('smartOverlay');
const smartFields=[
  {id:'sender',ru:'Отправитель',en:'Sender',type:'text'},
  {id:'recipient',ru:'Получатель',en:'Recipient',type:'text'},
  {id:'subject',ru:'Тема',en:'Subject',type:'text'},
  {id:'body',ru:'Текст и предпросмотр',en:'Text and preview',type:'text'},
  {id:'account',ru:'Аккаунт',en:'Account',type:'text'},
  {id:'folder',ru:'Название папки',en:'Folder name',type:'text'},
  {id:'folder_role',ru:'Тип папки',en:'Folder type',type:'enum',values:[['inbox','Входящие','Inbox'],['sent','Отправленные','Sent'],['drafts','Черновики','Drafts'],['archive','Архив','Archive'],['spam','Спам','Spam'],['trash','Корзина','Trash'],['other','Другая','Other']]},
  {id:'read_state',ru:'Прочтение',en:'Read state',type:'enum',values:[['unread','Непрочитано','Unread'],['read','Прочитано','Read']]},
  {id:'importance',ru:'Важность',en:'Importance',type:'enum',values:[['flagged','Важное','Important'],['normal','Обычное','Normal']]},
  {id:'reply_state',ru:'Ответ',en:'Reply state',type:'enum',values:[['answered','На письмо отвечено','Answered'],['unanswered','На письмо не отвечено','Not answered']]},
  {id:'draft_state',ru:'Черновик',en:'Draft state',type:'enum',values:[['draft','Черновик','Draft'],['not_draft','Не черновик','Not a draft']]},
  {id:'attachment',ru:'Вложения',en:'Attachments',type:'enum',values:[['has','Есть вложения','Has attachments'],['none','Нет вложений','No attachments']]},
  {id:'size',ru:'Размер письма',en:'Message size',type:'size'},
  {id:'label',ru:'Метка',en:'Label',type:'text'},
  {id:'date',ru:'Дата письма',en:'Message date',type:'date'},
];
const smartOps={
  contains:['содержит','contains'],not_contains:['не содержит','does not contain'],equals:['равно','equals'],not_equals:['не равно','does not equal'],starts_with:['начинается с','starts with'],ends_with:['заканчивается на','ends with'],
  within_last:['за последние','within last'],older_than:['старше чем','older than'],before:['раньше даты','before'],after:['позже даты','after'],on:['точно в дату','on'],
  greater_than:['больше','greater than'],greater_or_equal:['не меньше','at least'],less_than:['меньше','less than'],less_or_equal:['не больше','at most'],between:['между','between'],
};
const smartUnits=[['minutes','минут','minutes'],['hours','часов','hours'],['days','дней','days'],['weeks','недель','weeks']];
const smartSizeUnits=[['kb','КБ','KB'],['mb','МБ','MB'],['gb','ГБ','GB']];
const legacySmartFields={Отправитель:'sender',Sender:'sender',Получатель:'recipient',Recipient:'recipient',Тема:'subject',Subject:'subject','Текст письма':'body','Message text':'body',Аккаунт:'account',Account:'account',Статус:'read_state',Status:'read_state',Вложение:'attachment',Attachment:'attachment',Метка:'label',Label:'label',Папка:'folder',Folder:'folder',Дата:'date',Date:'date'};
const legacySmartOps={содержит:'contains',contains:'contains','не содержит':'not_contains','does not contain':'not_contains',равно:'equals',equals:'equals'};
const smartIsEnglish=()=>document.documentElement.lang==='en';
const L=(ru,en)=>smartIsEnglish()?en:ru;
const smartLabel=item=>item[smartIsEnglish()?'en':'ru'];
const smartOptionLabel=item=>item[smartIsEnglish()?2:1];
function smartUnitLabel(unit,value){if(smartIsEnglish())return smartOptionLabel(unit);const number=Math.abs(Number(value)||0)%100,last=number%10,forms={minutes:['минута','минуты','минут'],hours:['час','часа','часов'],days:['день','дня','дней'],weeks:['неделя','недели','недель']}[unit[0]];return number>=11&&number<=19?forms[2]:last===1?forms[0]:last>=2&&last<=4?forms[1]:forms[2];}
function smartField(id){return smartFields.find(field=>field.id===id)||smartFields[0];}
function smartOperators(field){return field.type==='text'?['contains','not_contains','equals','not_equals','starts_with','ends_with']:field.type==='date'?['within_last','older_than','before','after','on']:field.type==='size'?['greater_than','greater_or_equal','less_than','less_or_equal','equals','between']:['equals','not_equals'];}
function normalizeSmartCondition(condition={}){
  let field=legacySmartFields[condition.f]||condition.f||'sender',operator=legacySmartOps[condition.o]||condition.o||'contains',value=condition.v??'';
  if(field==='status'){field='read_state';}if(field==='attachment'){value=value==='yes'?'has':value==='no'?'none':value;}
  if(field==='read_state'&&value==='seen')value='read';if(field==='read_state'&&value==='not_seen')value='unread';
  const definition=smartField(field),allowed=smartOperators(definition);if(!allowed.includes(operator))operator=allowed[0];
  return {f:definition.id,o:operator,v:String(value),...(definition.type==='date'&&['within_last','older_than'].includes(operator)?{u:condition.u||'hours'}:{}),...(definition.type==='size'?{u:condition.u||'mb',v2:String(condition.v2??'')}:{})};
}
function normalizeSmartGroup(group={}){const source=Array.isArray(group)?group:group.conditions;return {logic:Array.isArray(group)?'all':group.logic==='any'?'any':'all',conditions:(Array.isArray(source)?source:[]).map(normalizeSmartCondition)};}
function renderConditionValue(row,condition){
  const field=smartField(condition.f),host=row.querySelector('.cond-value');host.className='cond-value';host.innerHTML='';
  if(field.type==='enum'){
    const select=document.createElement('select');select.className='cond-input';select.innerHTML=field.values.map(item=>`<option value="${item[0]}">${escapeHtml(smartOptionLabel(item))}</option>`).join('');select.value=field.values.some(item=>item[0]===condition.v)?condition.v:field.values[0][0];host.appendChild(select);
  }else if(field.type==='date'&&['within_last','older_than'].includes(condition.o)){
    host.classList.add('relative-date');const input=document.createElement('input');input.className='cond-input';input.type='number';input.min='1';input.step='1';input.value=/^\d+(?:\.\d+)?$/.test(condition.v)?condition.v:'24';const unit=document.createElement('select');unit.className='cond-unit';unit.innerHTML=smartUnits.map(item=>`<option value="${item[0]}">${escapeHtml(smartOptionLabel(item))}</option>`).join('');unit.value=smartUnits.some(item=>item[0]===condition.u)?condition.u:'hours';host.append(input,unit);
  }else if(field.type==='size'){
    host.classList.add('relative-date');const input=document.createElement('input');input.className='cond-input';input.type='number';input.min='0';input.step='0.1';input.placeholder=smartIsEnglish()?'size':'размер';input.value=/^\d+(?:\.\d+)?$/.test(condition.v)?condition.v:'10';host.appendChild(input);if(condition.o==='between'){const second=document.createElement('input');second.className='cond-max';second.type='number';second.min='0';second.step='0.1';second.placeholder=smartIsEnglish()?'to':'до';second.value=/^\d+(?:\.\d+)?$/.test(condition.v2)?condition.v2:'50';host.appendChild(second);}const unit=document.createElement('select');unit.className='cond-unit';unit.innerHTML=smartSizeUnits.map(item=>`<option value="${item[0]}">${escapeHtml(smartOptionLabel(item))}</option>`).join('');unit.value=smartSizeUnits.some(item=>item[0]===condition.u)?condition.u:'mb';host.appendChild(unit);
  }else{
    const input=document.createElement('input');input.className='cond-input';input.type=field.type==='date'?'date':'text';input.placeholder=smartIsEnglish()?'value':'значение';input.value=condition.v||'';host.appendChild(input);
  }
  host.querySelectorAll('input,select').forEach(control=>{control.addEventListener('input',updateSmartPreview);control.addEventListener('change',updateSmartPreview);});
}
function readConditionRow(row){return normalizeSmartCondition({f:row.querySelector('.cond-field').value,o:row.querySelector('.cond-op').value,v:row.querySelector('.cond-input')?.value||'',v2:row.querySelector('.cond-max')?.value||'',u:row.querySelector('.cond-unit')?.value});}
function validSmartCondition(source){const condition=normalizeSmartCondition(source),field=smartField(condition.f);if(field.type==='enum')return field.values.some(item=>item[0]===condition.v);if(field.type==='date'&&['within_last','older_than'].includes(condition.o))return Number(condition.v)>0&&smartUnits.some(item=>item[0]===condition.u);if(field.type==='date')return /^\d{4}-\d{2}-\d{2}$/.test(condition.v);if(field.type==='size')return Number(condition.v)>=0&&smartSizeUnits.some(item=>item[0]===condition.u)&&(condition.o!=='between'||Number(condition.v2)>Number(condition.v));return Boolean(condition.v.trim());}
function condRow(source={}){const condition=normalizeSmartCondition(typeof source==='object'?source:{f:source});const r=document.createElement('div');r.className='cond';
  r.innerHTML=`<select class="cond-field">${smartFields.map(field=>`<option value="${field.id}">${escapeHtml(smartLabel(field))}</option>`).join('')}</select><select class="cond-op"></select><div class="cond-value"></div><button type="button" class="del iconbtn" title="${smartIsEnglish()?'Delete condition':'Удалить условие'}"><i data-i="trash"></i></button>`;
  const fieldSelect=r.querySelector('.cond-field'),operatorSelect=r.querySelector('.cond-op');fieldSelect.value=condition.f;
  const rebuildOperator=(selected)=>{const field=smartField(fieldSelect.value),operators=smartOperators(field);operatorSelect.innerHTML=operators.map(id=>`<option value="${id}">${escapeHtml(smartOps[id][smartIsEnglish()?1:0])}</option>`).join('');operatorSelect.value=operators.includes(selected)?selected:operators[0];renderConditionValue(r,{...condition,f:field.id,o:operatorSelect.value,v:field.id===condition.f?condition.v:'',u:condition.u});updateSmartPreview();};
  fieldSelect.onchange=()=>rebuildOperator();operatorSelect.onchange=()=>{const current=readConditionRow(r);renderConditionValue(r,current);updateSmartPreview();};rebuildOperator(condition.o);
  r.querySelector('.del').onclick=()=>{r.remove();updateSmartPreview();};renderIcons(r);return r;}
let editingSmartIndex=null,selectedSmartIcon='star';
function renumberConditionGroups(){document.querySelectorAll('#conds .cond-group').forEach((group,index)=>{group.querySelector('.cond-group-title').textContent=`${smartIsEnglish()?'Group':'Группа'} ${index+1}`;group.querySelector('.cond-group-remove').classList.toggle('hidden',document.querySelectorAll('#conds .cond-group').length===1);});}
function conditionGroup(source={conditions:[{}]}){const state=normalizeSmartGroup(source),group=document.createElement('div');group.className='cond-group';group.dataset.logic=state.logic;
  group.innerHTML=`<div class="cond-group-head"><span class="cond-group-title"></span><div class="logic"><button type="button" data-l="all">${smartIsEnglish()?'All (AND)':'Все (И)'}</button><button type="button" data-l="any">${smartIsEnglish()?'Any (OR)':'Любое (ИЛИ)'}</button></div><button type="button" class="iconbtn cond-group-remove" title="${smartIsEnglish()?'Delete group':'Удалить группу'}"><i data-i="trash"></i></button></div>`;
  group.querySelectorAll('.logic button').forEach(button=>{button.classList.toggle('on',button.dataset.l===state.logic);button.onclick=()=>{group.dataset.logic=button.dataset.l;group.querySelectorAll('.logic button').forEach(item=>item.classList.toggle('on',item===button));updateSmartPreview();};});
  (state.conditions.length?state.conditions:[{}]).forEach(condition=>group.appendChild(condRow(condition)));group.querySelector('.cond-group-remove').onclick=()=>{group.remove();renumberConditionGroups();updateSmartPreview();};renderIcons(group);return group;}
function readSmartGroups(){return [...document.querySelectorAll('#conds .cond-group')].map(group=>({logic:group.dataset.logic==='any'?'any':'all',conditions:[...group.querySelectorAll('.cond')].map(readConditionRow)})).filter(group=>group.conditions.length);}
function editorSmartFolder(){return {groups:readSmartGroups()};}
function updateSmartPreview(){const preview=document.getElementById('smartPreview');if(!preview)return;try{preview.textContent=String(smartRowsForFolder(editorSmartFolder()).length);}catch(_){preview.textContent='0';}}
function openSmart(index=null){editingSmartIndex=index;const c=document.getElementById('conds');c.innerHTML='';const item=index===null?null:smartFolders[index];document.querySelector('#smartOverlay .mh h3').textContent=item?L('Изменить умную папку','Edit smart folder'):L('Новая умная папка','New smart folder');document.getElementById('smartCreate').lastChild.textContent=item?L(' Сохранить',' Save'):L(' Создать умную папку',' Create smart folder');document.getElementById('smartDelete').classList.toggle('hidden',!item||item.builtin);document.getElementById('smartName').value=item?.t||'';selectedSmartIcon=item?.i||'star';updateSmartIconButton();(item?.groups?.length?item.groups:[{conditions:[{}]}]).forEach(group=>c.appendChild(conditionGroup(group)));renumberConditionGroups();smartOverlay.classList.add('open');updateSmartPreview();loadCompleteSmartCoverage(index??-1);}
function closeSmart(){smartOverlay.classList.remove('open');}
document.getElementById('addSmart').onclick=(e)=>{e.stopPropagation();openSmart();};
document.getElementById('addCond').onclick=()=>{document.querySelector('#conds .cond-group:last-child')?.appendChild(condRow());updateSmartPreview();};
document.getElementById('addCondGroup').onclick=()=>{document.getElementById('conds').appendChild(conditionGroup());renumberConditionGroups();updateSmartPreview();};
document.getElementById('smartClose').onclick=closeSmart;
document.getElementById('smartCancel').onclick=closeSmart;
document.getElementById('smartCreate').onclick=()=>{const name=document.getElementById('smartName').value.trim(),groups=readSmartGroups();if(!name){showToast(L('Введите название умной папки','Enter a smart folder name'));return;}if(!groups.length||groups.some(group=>!group.conditions.length||group.conditions.some(condition=>!validSmartCondition(condition)))){showToast(L('Заполните все условия умной папки','Fill in all smart folder conditions'));return;}const previous=editingSmartIndex===null?null:smartFolders[editingSmartIndex],item={...(previous||{}),id:previous?.id||`custom-${Date.now()}`,builtin:Boolean(previous?.builtin),i:selectedSmartIcon,t:name,on:previous?.on??true,groups};if(editingSmartIndex===null)smartFolders.push(item);else smartFolders[editingSmartIndex]=item;renderSmartManagement();bindSmartNavigation();persistSmartFolders().then(()=>{if(currentSmartIndex===editingSmartIndex)filterSmart(editingSmartIndex);}).catch(error=>showToast(error.message||String(error)));closeSmart();};
document.getElementById('smartDelete').onclick=async()=>{const folder=editingSmartIndex===null?null:smartFolders[editingSmartIndex];if(!folder||folder.builtin||!await confirmAction(L(`Удалить умную папку «${folder.t}»?`,`Delete the smart folder "${folder.t}"?`)))return;const activeId=smartFolders[currentSmartIndex]?.id;smartFolders.splice(editingSmartIndex,1);renderSmartManagement();bindSmartNavigation();persistSmartFolders().catch(error=>showToast(error.message||String(error)));closeSmart();if(activeId===folder.id)filterSmart(0);};
smartOverlay.onclick=e=>{if(e.target===smartOverlay)closeSmart();};
const smartIconKeys=Object.keys(ic).filter(key=>!['chevR','chevL','up','down','back','dots','grip'].includes(key)).slice(0,50);
const smartIconsEl=document.getElementById('smartIcons');smartIconsEl.innerHTML=smartIconKeys.map(key=>`<span class="ic-pick" data-sel="${key}" title="${key}"><i data-i="${key}"></i></span>`).join('');renderIcons(smartIconsEl);
function updateSmartIconButton(){const i=document.querySelector('#smartIconButton i');i.dataset.i=selectedSmartIcon;i.innerHTML=ic[selectedSmartIcon]||ic.star;document.querySelectorAll('#smartIcons .ic-pick').forEach(p=>p.classList.toggle('on',p.dataset.sel===selectedSmartIcon));}
document.getElementById('smartIconButton').onclick=()=>smartIconsEl.classList.toggle('hidden');document.querySelectorAll('#smartIcons .ic-pick').forEach(p=>p.onclick=()=>{selectedSmartIcon=p.dataset.sel;updateSmartIconButton();smartIconsEl.classList.add('hidden');});

/* toolbar customizer */
const tbActions=[
  {k:'reply',t:'Ответить',en:'Reply',on:true},{k:'replyall',t:'Ответить всем',en:'Reply all',on:true},{k:'forward',t:'Переслать',en:'Forward',on:true},
  {k:'archive',t:'В архив',en:'Archive',on:true},{k:'trash',t:'Удалить',en:'Delete',on:true},{k:'spam',t:'В спам',en:'Spam',on:false},{k:'snooze',t:'Отложить',en:'Snooze',on:false},
  {k:'unread',t:'Непрочитанное',en:'Mark unread',i:'inbox',on:false},{k:'unsub',t:'Отписаться',en:'Unsubscribe',on:false},{k:'print',t:'Печать',en:'Print',on:false}];
const tbLabel=a=>smartIsEnglish()&&a.en?a.en:a.t;
const tbList=document.getElementById('tbList');
tbActions.forEach(a=>{const r=document.createElement('div');r.className='tbrow'+(a.on?'':' off');r.draggable=true;r.dataset.action=a.k;
  r.dataset.labels='text';r.innerHTML=`<span class="grip"><i data-i="grip"></i></span><i data-i="${a.i||a.k}"></i><span class="nm">${escapeHtml(tbLabel(a))}</span><button type="button" class="btn sm action-label-mode" title="${smartIsEnglish()?'Toggle label':'Переключить подпись'}">${smartIsEnglish()?'Icon + text':'Значок + текст'}</button>
    <span class="ord"><button class="iconbtn" data-dir="up"><i data-i="up"></i></button><button class="iconbtn" data-dir="down"><i data-i="down"></i></button></span>
    <div class="toggle${a.on?' on':''}"></div>`;
  renderIcons(r);
  const save=()=>{applyToolbar();persistToolbar();};
  r.querySelector('[data-dir="up"]').onclick=()=>{const p=r.previousElementSibling;if(p)tbList.insertBefore(r,p);save();};
  r.querySelector('[data-dir="down"]').onclick=()=>{const n=r.nextElementSibling;if(n)tbList.insertBefore(n,r);save();};
  r.querySelector('.action-label-mode').onclick=event=>{event.stopPropagation();r.dataset.labels=r.dataset.labels==='icons'?'text':'icons';event.currentTarget.textContent=r.dataset.labels==='icons'?(smartIsEnglish()?'Icon only':'Только значок'):(smartIsEnglish()?'Icon + text':'Значок + текст');save();};
  r.querySelector('.toggle').onclick=(e)=>{e.stopPropagation();const t=e.currentTarget;t.classList.toggle('on');r.classList.toggle('off',!t.classList.contains('on'));save();};
  tbList.appendChild(r);});
let draggedToolbarRow=null;tbList.addEventListener('dragstart',e=>{draggedToolbarRow=e.target.closest('.tbrow');});tbList.addEventListener('dragover',e=>{e.preventDefault();const row=e.target.closest('.tbrow');if(row&&draggedToolbarRow&&row!==draggedToolbarRow){const rect=row.getBoundingClientRect();tbList.insertBefore(draggedToolbarRow,e.clientY<rect.top+rect.height/2?row:row.nextSibling);}});tbList.addEventListener('drop',()=>{applyToolbar();persistToolbar();});
tbList.addEventListener('pointerdown',event=>{const grip=event.target.closest('.grip'),row=grip?.closest('.tbrow');if(!row||event.button!==0)return;event.preventDefault();draggedToolbarRow=row;row.classList.add('pointer-dragging');grip.setPointerCapture(event.pointerId);});
tbList.addEventListener('pointermove',event=>{if(!draggedToolbarRow)return;const target=document.elementFromPoint(event.clientX,event.clientY)?.closest('.tbrow');if(!target||target===draggedToolbarRow||target.parentElement!==tbList)return;const rect=target.getBoundingClientRect();tbList.insertBefore(draggedToolbarRow,event.clientY<rect.top+rect.height/2?target:target.nextSibling);});
tbList.addEventListener('pointerup',event=>{if(!draggedToolbarRow)return;event.target.closest('.grip')?.releasePointerCapture?.(event.pointerId);draggedToolbarRow.classList.remove('pointer-dragging');draggedToolbarRow=null;applyToolbar();persistToolbar();});
function toolbarState(){return {actions:[...tbList.children].map(row=>({key:row.dataset.action,visible:!row.classList.contains('off'),labels:row.dataset.labels||'text'})),align:document.querySelector('#toolbarAlign .on')?.dataset.align||'left'};}
function persistToolbar(){window.tm?.setSetting('toolbar_layout',JSON.stringify(toolbarState())).catch(console.error);}
function applyToolbar(){const state=toolbarState(),bar=document.querySelector('.thread .actions');if(!bar)return;bar.classList.toggle('toolbar-right',state.align==='right');bar.querySelectorAll('[data-toolbar-generated]').forEach(el=>el.remove());const anchor=bar.querySelector('.sp');state.actions.filter(action=>action.visible).forEach(action=>{const meta=tbActions.find(a=>a.k===action.key);if(!meta)return;const button=document.createElement('button');button.className=`${action.key==='reply'?'btn primary':'btn'}${action.labels==='icons'?' toolbar-action-icons':''}`;button.dataset.toolbarGenerated='1';button.dataset.act=action.key;button.title=tbLabel(meta);button.innerHTML=`<i data-i="${meta.i||action.key}"></i><span>${escapeHtml(tbLabel(meta))}</span>`;renderIcons(button);bar.insertBefore(button,anchor);});bar.querySelectorAll(':scope > button:not([data-toolbar-generated]):not([data-toolbar-persistent])').forEach(button=>button.classList.add('toolbar-original-hidden'));
  // Меню "Ещё" показывает только те действия, что скрыты из панели - без дублей.
  const menuDyn=document.getElementById('threadMenuDynamic');
  if(menuDyn){menuDyn.innerHTML='';state.actions.filter(action=>!action.visible).forEach(action=>{const meta=tbActions.find(a=>a.k===action.key);if(!meta)return;const button=document.createElement('button');button.type='button';button.dataset.toolbarMenu=action.key;button.innerHTML=`<i data-i="${meta.i||action.key}"></i><span>${escapeHtml(tbLabel(meta))}</span>`;menuDyn.appendChild(button);});renderIcons(menuDyn);const sep=document.getElementById('threadMenuDynSep');if(sep)sep.style.display=menuDyn.children.length?'':'none';}}
document.querySelectorAll('#toolbarAlign button').forEach(button=>button.onclick=()=>{button.parentElement.querySelectorAll('button').forEach(x=>x.classList.toggle('on',x===button));applyToolbar();persistToolbar();});
applyToolbar();
function embeddedUnsubscribeUrl(message){
  if(!message?.body_html)return null;
  const parsed=new DOMParser().parseFromString(message.body_html,'text/html');
  const unsubscribeWords=/(?:unsubscribe|opt[\s_-]*out|remove[\s_-]*(?:me|email)|отпис(?:аться|ка|ать)|отказаться[\s\S]{0,20}рассылк)/i;
  for(const link of parsed.querySelectorAll('a[href]')){
    const href=link.getAttribute('href')?.trim()||'';
    if(!/^https?:\/\//i.test(href))continue;
    const context=link.closest('p,li,td,div')?.textContent||'';
    let searchable=`${link.textContent||''} ${context} ${link.getAttribute('title')||''} ${link.getAttribute('aria-label')||''} ${href}`;
    try{searchable+=` ${decodeURIComponent(href)}`;}catch(_){/* Keep matching against the original malformed URL. */}
    if(unsubscribeWords.test(searchable))return href;
  }
  return null;
}
function selectedOrActiveMessageIds(){return selectedMessageIds.size?[...selectedMessageIds]:activeMessage?[activeMessage.id]:[];}
function nextMondayMorning(){const value=new Date();const days=(8-value.getDay())%7||7;value.setDate(value.getDate()+days);value.setHours(9,0,0,0);return value;}
function openSnoozeDialog(){const ids=selectedOrActiveMessageIds();if(!ids.length){showToast(L('Сначала выберите письмо','Select a message first'));return;}
  const overlay=document.createElement('div');overlay.className='overlay open';overlay.innerHTML=`<div class="modal compact-modal snooze-modal"><div class="mh"><i data-i="snooze"></i><h3>${L('Отложить письмо','Snooze message')}</h3><button class="iconbtn x" type="button"><i data-i="close"></i></button></div><div class="mb"><div class="snooze-presets"><button class="btn" data-offset="hour">${L('Через час','In one hour')}</button><button class="btn" data-offset="tomorrow">${L('Завтра в 09:00','Tomorrow at 09:00')}</button><button class="btn" data-offset="monday">${L('В понедельник в 09:00','Monday at 09:00')}</button></div><label class="template-field">${L('Другое время','Custom time')}<input class="inp snooze-custom" type="datetime-local"></label></div><div class="mf"><span class="sp"></span><button class="btn snooze-cancel">${L('Отмена','Cancel')}</button><button class="btn primary snooze-apply">${L('Отложить','Snooze')}</button></div></div>`;
  document.body.appendChild(overlay);renderIcons(overlay);const close=()=>overlay.remove();const custom=overlay.querySelector('.snooze-custom');const initial=new Date(Date.now()+60*60*1000);initial.setSeconds(0,0);custom.value=new Date(initial.getTime()-initial.getTimezoneOffset()*60000).toISOString().slice(0,16);
  const apply=async date=>{try{await window.tm.snoozeMessages(ids,date.toISOString());clearMessageSelection();activeMessage=null;activeFullMessage=null;await window.reloadCoreData();close();showToast(L(`Отложено писем: ${ids.length}`,`Snoozed messages: ${ids.length}`),L('Отменить','Undo'),async()=>{await window.tm.unsnoozeMessages(ids);await window.reloadCoreData();});}catch(error){showToast(error.message||String(error));}};
  overlay.querySelector('[data-offset="hour"]').onclick=()=>apply(new Date(Date.now()+60*60*1000));overlay.querySelector('[data-offset="tomorrow"]').onclick=()=>{const d=new Date();d.setDate(d.getDate()+1);d.setHours(9,0,0,0);apply(d);};overlay.querySelector('[data-offset="monday"]').onclick=()=>apply(nextMondayMorning());overlay.querySelector('.snooze-apply').onclick=()=>{const d=new Date(custom.value);if(Number.isNaN(d.getTime())||d<=new Date()){showToast(L('Выберите будущее время','Choose a future time'));return;}apply(d);};overlay.querySelectorAll('.x,.snooze-cancel').forEach(button=>button.onclick=close);overlay.onclick=event=>{if(event.target===overlay)close();};custom.focus();}
async function exportActiveMessageEml(){if(!activeMessage){showToast(L('Сначала выберите письмо','Select a message first'));return;}const safe=(activeMessage.subject||'message').replace(/[<>:"/\\|?*\x00-\x1f]/g,'_').trim().slice(0,100)||'message';try{const path=await window.tm.saveFileDialog(`${safe}.eml`);if(!path)return;await window.tm.exportMessageEml(activeMessage.id,path);showToast(L('Письмо сохранено в .eml','Message saved as .eml'));}catch(error){showToast(error.message||String(error));}}
async function executeToolbarAction(action){if(['reply','replyall','forward'].includes(action)){openComposerForMessage(action);return;}if(['archive','trash','spam'].includes(action)){performMessageAction(action);return;}if(action==='snooze'){openSnoozeDialog();return;}if(action==='unread'){if(activeMessage){await window.tm?.markSeen(activeMessage.id,false);await window.reloadCoreData?.();showToast(L('Письмо отмечено непрочитанным','Message marked as unread'));}return;}if(action==='print'){const frame=document.querySelector('.mail-html-frame');if(frame?.contentWindow)frame.contentWindow.print();else window.print();return;}if(action==='unsub'){const uns=activeFullMessage?.unsubscribe;
  if(uns?.one_click_url){showToast(L('Отправляю запрос на отписку…','Sending unsubscribe request…'));const fallback=()=>window.tm?.openExternal(uns.http||uns.one_click_url).catch(error=>showToast(error.message||String(error)));try{const status=await window.tm.unsubscribeOneClick(uns.one_click_url);if(status>=200&&status<300)showToast(L('Готово: вы отписаны от рассылки (сервер подтвердил, код '+status+')','Done: you have been unsubscribed (server confirmed, code '+status+')'));else{showToast(L('Сервер отписки ответил кодом '+status+'. Открываю страницу отписки…','The unsubscribe server responded with code '+status+'. Opening the unsubscribe page…'));fallback();}}catch(error){showToast(L('Автоотписка не удалась: '+(error.message||error)+'. Открываю страницу…','Automatic unsubscribe failed: '+(error.message||error)+'. Opening the page…'));fallback();}return;}
  const target=uns?.http||embeddedUnsubscribeUrl(activeFullMessage);if(target){try{await window.tm.openExternal(target);showToast(L('Открыл страницу отписки в браузере — завершите отписку там','Opened the unsubscribe page in your browser — finish there'));}catch(error){showToast(error.message||String(error));}return;}
  const mailto=uns?.mailto;if(mailto){resetComposer();setRecipients('compTo',[String(mailto).replace(/^mailto:/i,'').split('?')[0]]);document.getElementById('compSubj').value=L('Отписаться','Unsubscribe');showView('composeView');showToast(L('Отправьте это письмо, чтобы отписаться','Send this message to unsubscribe'));return;}
  showToast(L('В письме нет ссылки для автоматической отписки','This message has no automatic unsubscribe link'));}}
document.querySelector('.thread .actions').addEventListener('click',e=>{const button=e.target.closest('[data-toolbar-generated]');if(button)executeToolbarAction(button.dataset.act);});
const threadMoreButton=document.getElementById('threadMoreButton'),threadMoreMenu=document.getElementById('threadMoreMenu');
function closeThreadMore(){threadMoreMenu.classList.remove('open');threadMoreButton.setAttribute('aria-expanded','false');}
threadMoreButton.onclick=event=>{event.stopPropagation();const open=threadMoreMenu.classList.toggle('open');threadMoreButton.setAttribute('aria-expanded',String(open));};
threadMoreMenu.onclick=async event=>{const toolbarItem=event.target.closest('[data-toolbar-menu]');if(toolbarItem){closeThreadMore();executeToolbarAction(toolbarItem.dataset.toolbarMenu);return;}const button=event.target.closest('[data-thread-action]');if(!button)return;closeThreadMore();const action=button.dataset.threadAction;if(action==='settings'){showView('settingsView');setSection('toolbar');return;}if(action==='rules'){showView('settingsView');setSection('rules');return;}if(action==='raw'){openRawViewer(activeMessage?.id);return;}if(action==='export-eml'){exportActiveMessageEml();return;}if(action==='create-rule'){openRuleEditor(activeMessage);return;}if(action==='unread'){if(activeMessage){await window.tm?.markSeen(activeMessage.id,false);await window.reloadCoreData?.();showToast(L('Письмо отмечено непрочитанным','Message marked as unread'));}return;}if(['archive','trash'].includes(action))performMessageAction(action);};
document.addEventListener('click',event=>{if(!threadMoreMenu.contains(event.target)&&event.target!==threadMoreButton)closeThreadMore();});

/* Server-side mail actions driven by locally stored, editable rules. */
const ruleEditor=document.getElementById('ruleEditor'),ruleAccount=document.getElementById('ruleAccount'),ruleAction=document.getElementById('ruleAction'),ruleTarget=document.getElementById('ruleTarget');
function ruleAccountFolders(){const accountId=Number(ruleAccount.value);return coreFolders.filter(folder=>folder.account_id===accountId);}
function populateRuleTargets(selectedFolderId=null){
  const previous=selectedFolderId??(Number(ruleTarget.value)||null);
  ruleTarget.innerHTML=ruleAccountFolders().map(folder=>`<option value="${folder.id}">${escapeHtml(folderTitle(folder))}</option>`).join('');
  if(previous&&ruleTarget.querySelector(`option[value="${previous}"]`))ruleTarget.value=String(previous);
}
function updateRuleActionFields(){
  const moving=ruleAction.value==='move';document.querySelector('.rule-target-field').classList.toggle('hidden',!moving);
  if(moving&&ruleAccount.value==='all'){ruleAccount.value=String(coreAccounts[0]?.id||'');populateRuleTargets();}
}
function openRuleEditor(source=null,rule=null){
  showView('settingsView');setSection('rules');editingRuleId=rule?.id??null;ruleEditor.classList.remove('hidden');
  document.getElementById('ruleEditorTitle').textContent=rule?L('Изменить правило','Edit rule'):L('Новое правило','New rule');
  ruleAccount.innerHTML=`<option value="all">${L('Все аккаунты','All accounts')}</option>`+coreAccounts.map(account=>`<option value="${account.id}">${escapeHtml(account.email)}</option>`).join('');
  const sourceEmail=source?.from?.email||'',sourceAccount=source?.account_id||coreAccounts[0]?.id||'all';ruleEditor.dataset.sourceSender=sourceEmail;ruleEditor.dataset.sourceSubject=source?.subject||'';
  document.getElementById('ruleName').value=rule?.name||(sourceEmail?L(`Письма от ${sourceEmail}`,`Mail from ${sourceEmail}`):'');document.getElementById('ruleField').value=rule?.field||'sender';document.getElementById('ruleOperator').value=rule?.operator||'contains';document.getElementById('ruleValue').value=rule?.value||sourceEmail;ruleAccount.value=String(rule?.account_id??sourceAccount);ruleAction.value=rule?.action||'move';populateRuleTargets(rule?.folder_id);updateRuleActionFields();document.getElementById('ruleExisting').checked=false;document.getElementById('ruleDelete').classList.toggle('hidden',!rule);document.getElementById('ruleName').focus();
}
function closeRuleEditor(){ruleEditor.classList.add('hidden');editingRuleId=null;}
function ruleDescription(rule){
  const en=smartIsEnglish(),fields=en?{sender:'Sender',subject:'Subject'}:{sender:'Отправитель',subject:'Тема'},operators=en?{contains:'contains',equals:'equals'}:{contains:'содержит',equals:'равно'},actions=en?{archive:'archive',spam:'move to spam',trash:'delete'}:{archive:'в архив',spam:'в спам',trash:'удалить'};const account=rule.account_id?coreAccounts.find(item=>item.id===rule.account_id)?.email:L('все аккаунты','all accounts');let action=actions[rule.action];if(rule.action==='move')action=L(`в папку «${folderTitle(coreFolders.find(folder=>folder.id===rule.folder_id))}»`,`to folder "${folderTitle(coreFolders.find(folder=>folder.id===rule.folder_id))}"`);return `${fields[rule.field]} ${operators[rule.operator]} «${rule.value}» → ${action||L('действие не настроено','action not set')} · ${account||L('аккаунт удалён','account removed')}`;
}
function renderRulesList(){
  const list=document.getElementById('rulesList');list.innerHTML='';if(!mailRules.length){list.innerHTML=`<p class="rule-empty">${L('Правил пока нет.','No rules yet.')}</p>`;return;}
  mailRules.forEach(rule=>{const row=document.createElement('div');row.className='rule-row';row.innerHTML=`<div class="toggle${rule.enabled!==false?' on':''}" role="switch"></div><div><div class="rule-row-title"></div><div class="rule-row-description"></div></div><button type="button" class="btn sm"><i data-i="edit"></i>${L('Изменить','Edit')}</button>`;row.querySelector('.rule-row-title').textContent=rule.name;row.querySelector('.rule-row-description').textContent=ruleDescription(rule);row.querySelector('.toggle').onclick=async()=>{try{await window.tm.setMailRuleEnabled(rule.id,!rule.enabled);await reloadMailRules();}catch(error){showToast(error.message||String(error));}};row.querySelector('button').onclick=()=>openRuleEditor(null,rule);list.appendChild(row);});renderIcons(list);
}
async function reloadMailRules(){mailRules=await window.tm.listMailRules();renderRulesList();}
window.reloadMailRules=reloadMailRules;
document.getElementById('ruleNew').onclick=()=>openRuleEditor();document.getElementById('ruleCancel').onclick=closeRuleEditor;ruleAccount.onchange=()=>populateRuleTargets();ruleAction.onchange=updateRuleActionFields;document.getElementById('ruleField').onchange=event=>{if(editingRuleId!==null)return;const value=event.target.value==='subject'?ruleEditor.dataset.sourceSubject:ruleEditor.dataset.sourceSender;if(value)document.getElementById('ruleValue').value=value;};
document.getElementById('ruleSave').onclick=async()=>{const name=document.getElementById('ruleName').value.trim(),value=document.getElementById('ruleValue').value.trim(),accountValue=ruleAccount.value,action=ruleAction.value,folderId=action==='move'?Number(ruleTarget.value):null;if(!name||!value){showToast(L('Заполните название и значение условия','Fill in the name and condition value'));return;}if(action==='move'&&!folderId){showToast(L('Выберите папку назначения','Choose a destination folder'));return;}const existing=mailRules.find(rule=>rule.id===editingRuleId),applyExisting=document.getElementById('ruleExisting').checked,rule={id:existing?.id||`rule-${Date.now()}`,name,field:document.getElementById('ruleField').value,operator:document.getElementById('ruleOperator').value,value,account_id:accountValue==='all'?null:Number(accountValue),action,folder_id:folderId,enabled:existing?.enabled??true};try{await window.tm.saveMailRule(rule,applyExisting);await reloadMailRules();closeRuleEditor();showToast(L('Правило сохранено','Rule saved'));setTimeout(()=>window.reloadCoreData?.().catch(console.error),350);}catch(error){showToast(error.message||String(error));}};
document.getElementById('ruleDelete').onclick=async()=>{const rule=mailRules.find(item=>item.id===editingRuleId);if(!rule||!await confirmAction(L(`Удалить правило «${rule.name}»?`,`Delete the rule "${rule.name}"?`)))return;try{await window.tm.deleteMailRule(rule.id);await reloadMailRules();closeRuleEditor();}catch(error){showToast(error.message||String(error));}};

/* smart folders management list */
const builtinSmartDefaults=[
  {id:'all-inbox',builtin:true,i:'inbox',t:'Все входящие',on:true,groups:[{logic:'all',conditions:[{f:'folder_role',o:'equals',v:'inbox'}]}]},
  {id:'all-important',builtin:true,i:'star',t:'Все важные',on:true,groups:[{logic:'all',conditions:[{f:'importance',o:'equals',v:'flagged'}]}]},
  {id:'all-sent',builtin:true,i:'send',t:'Все отправленные',on:true,groups:[{logic:'all',conditions:[{f:'folder_role',o:'equals',v:'sent'}]}]},
  {id:'all-drafts',builtin:true,i:'draft',t:'Все черновики',on:true,groups:[{logic:'all',conditions:[{f:'folder_role',o:'equals',v:'drafts'}]},{logic:'all',conditions:[{f:'draft_state',o:'equals',v:'draft'}]}]},
  {id:'last-24-hours',builtin:true,i:'cal',t:'Сегодня (за 24 часа)',on:true,groups:[{logic:'all',conditions:[{f:'date',o:'within_last',v:'24',u:'hours'}]}]},
  {id:'all-unread',builtin:true,i:'search',t:'Непрочитанные (все)',on:true,groups:[{logic:'all',conditions:[{f:'read_state',o:'equals',v:'unread'}]}]},
  {id:'with-attachments',builtin:true,i:'paperclip',t:'С вложениями',on:true,groups:[{logic:'all',conditions:[{f:'attachment',o:'equals',v:'has'}]}]},
  {id:'awaiting-my-reply',builtin:true,i:'flag',t:'Ждут ответа',on:true,groups:[{logic:'all',conditions:[{f:'folder_role',o:'equals',v:'inbox'},{f:'reply_state',o:'equals',v:'unanswered'}]}]},
];
const cloneSmart=value=>JSON.parse(JSON.stringify(value));
function normalizedSmartFolders(saved){
  if(!Array.isArray(saved))return cloneSmart(builtinSmartDefaults);const unused=new Map(builtinSmartDefaults.map(folder=>[folder.id,folder])),result=[];
  saved.forEach((raw,index)=>{if(!raw||typeof raw!=='object')return;let base=builtinSmartDefaults.find(folder=>folder.id===raw.id||folder.t===raw.t);if(!base&&index<8)base=builtinSmartDefaults.find(folder=>unused.has(folder.id)&&folder.i===raw.i);if(!base&&index<8)base=builtinSmartDefaults.find(folder=>unused.has(folder.id));if(base)unused.delete(base.id);
    const groups=Array.isArray(raw.groups)&&raw.groups.some(group=>(Array.isArray(group)?group:group?.conditions)?.length)?raw.groups.map(normalizeSmartGroup):cloneSmart(base?.groups||[]);result.push({...cloneSmart(base||{}),...raw,id:base?.id||raw.id||`custom-${Date.now()}-${index}`,builtin:Boolean(base||raw.builtin),on:raw.on!==false,groups});});
  unused.forEach(folder=>result.push(cloneSmart(folder)));return result;
}
const smartFolders=cloneSmart(builtinSmartDefaults);
function smartConditionDescription(source){const condition=normalizeSmartCondition(source),field=smartField(condition.f),operator=smartOps[condition.o]?.[smartIsEnglish()?1:0]||condition.o;let value=condition.v;
  if(field.values)value=smartOptionLabel(field.values.find(item=>item[0]===condition.v)||[condition.v,condition.v,condition.v]);else if(field.type==='date'&&['within_last','older_than'].includes(condition.o)){const unit=smartUnits.find(item=>item[0]===condition.u)||smartUnits[1];value=`${condition.v} ${smartUnitLabel(unit,condition.v)}`;}else if(field.type==='size'){const unit=smartSizeUnits.find(item=>item[0]===condition.u)||smartSizeUnits[1],label=smartOptionLabel(unit);value=condition.o==='between'?`${condition.v}–${condition.v2} ${label}`:`${condition.v} ${label}`;}else if(field.type==='text')value=`«${value}»`;
  return `${smartLabel(field)} ${operator} ${value}`;
}
function smartFolderDescription(folder){return (folder.groups||[]).map(source=>{const group=normalizeSmartGroup(source),joiner=group.logic==='any'?(smartIsEnglish()?' OR ':' ИЛИ '):(smartIsEnglish()?' AND ':' И ');return group.conditions.map(smartConditionDescription).join(joiner);}).filter(Boolean).join(smartIsEnglish()?'  • OR •  ':'  • ИЛИ •  ');}
const smartListEl=document.getElementById('smartList');
const builtinSmartTitles={
  'all-inbox':{ru:'Все входящие',en:'All inboxes'},
  'all-important':{ru:'Все важные',en:'All important'},
  'all-sent':{ru:'Все отправленные',en:'All sent'},
  'all-drafts':{ru:'Все черновики',en:'All drafts'},
  'last-24-hours':{ru:'Сегодня (за 24 часа)',en:'Today (last 24 hours)'},
  'all-unread':{ru:'Непрочитанные (все)',en:'Unread (all)'},
  'with-attachments':{ru:'С вложениями',en:'With attachments'},
  'awaiting-my-reply':{ru:'Ждут ответа',en:'Awaiting reply'},
};
function smartFolderTitle(folder){if(folder&&folder.builtin&&builtinSmartTitles[folder.id])return builtinSmartTitles[folder.id][smartIsEnglish()?'en':'ru'];return folder?.t||'';}
function messagesTitle(){return smartIsEnglish()?'Messages':'Письма';}
function smartFolderToCore(folder,index){return {id:String(folder.id),name:folder.t||'',icon:folder.i||null,is_builtin:Boolean(folder.builtin),enabled:folder.on!==false,sort_order:index,groups:(folder.groups||[]).map(group=>{const normalized=normalizeSmartGroup(group);return {logic:normalized.logic,conditions:normalized.conditions.map(condition=>({field:condition.f,op:condition.o,value:String(condition.v??''),unit:condition.u||null,value2:condition.v2||null}))};})};}
function smartFolderFromCore(folder){return {id:String(folder.id),builtin:Boolean(folder.is_builtin),i:folder.icon||'star',t:folder.name||'',on:folder.enabled!==false,groups:(folder.groups||[]).map(group=>({logic:group.logic==='any'?'any':'all',conditions:(group.conditions||[]).map(condition=>({f:condition.field,o:condition.op,v:String(condition.value??''),...(condition.unit?{u:condition.unit}:{}),...(condition.value2!=null?{v2:String(condition.value2)}:{})}))}))};}
function persistSmartFolders(){return window.tm?.saveSmartFolders(smartFolders.map(smartFolderToCore))||Promise.resolve();}
function moveSmartFolder(from,to){if(to<0||to>=smartFolders.length)return;const activeId=smartFolders[currentSmartIndex]?.id;[smartFolders[from],smartFolders[to]]=[smartFolders[to],smartFolders[from]];if(activeId)currentSmartIndex=smartFolders.findIndex(folder=>folder.id===activeId);renderSmartManagement();bindSmartNavigation();persistSmartFolders().catch(error=>showToast(error.message||String(error)));}
function renderSmartManagement(){smartListEl.innerHTML='';smartFolders.forEach((a,index)=>{const r=document.createElement('div');r.className='tbrow smart-list-row'+(a.on?'':' off');
  r.innerHTML=`<span class="grip"><i data-i="grip"></i></span><i data-i="${a.i}"></i><span class="smart-name"><span class="nm"></span><span class="smart-summary"></span></span><button class="btn sm edit-sf">${smartIsEnglish()?'Edit':'Изменить'}</button><span class="ord"><button class="iconbtn" data-dir="up"><i data-i="up"></i></button><button class="iconbtn" data-dir="down"><i data-i="down"></i></button></span><div class="toggle${a.on?' on':''}"></div>`;
  r.querySelector('.nm').textContent=smartFolderTitle(a);r.querySelector('.smart-summary').textContent=smartFolderDescription(a);renderIcons(r);
  r.querySelector('[data-dir="up"]').onclick=()=>moveSmartFolder(index,index-1);
  r.querySelector('[data-dir="down"]').onclick=()=>moveSmartFolder(index,index+1);
  r.querySelector('.edit-sf').onclick=()=>openSmart(index);
  r.querySelector('.toggle').onclick=(e)=>{e.stopPropagation();const t=e.currentTarget;t.classList.toggle('on');a.on=t.classList.contains('on');r.classList.toggle('off',!a.on);bindSmartNavigation();persistSmartFolders().catch(error=>showToast(error.message||String(error)));};
  smartListEl.appendChild(r);});}
renderSmartManagement();
document.getElementById('smartNew2').onclick=()=>openSmart();
function bindSmartNavigation(){document.querySelectorAll('.custom-smart').forEach(row=>row.remove());const nav=document.querySelector('.nav'),accountLabel=nav.querySelector('[data-navlabel="accounts"]')||[...nav.querySelectorAll('.navlabel')].find(label=>label.textContent.includes('Аккаунты'));smartFolders.forEach((folder,index)=>{let row=folder.builtin?nav.querySelector(`[data-smart-id="${folder.id}"]`):null;if(!row){row=document.createElement('div');row.className='navitem custom-smart';row.dataset.nav='mail';row.innerHTML='<i></i><span class="smart-label"></span>';}
    row.dataset.smartIndex=index;row.dataset.smartId=folder.id;const icon=row.querySelector('i');icon.dataset.i=folder.i;icon.innerHTML=ic[folder.i]||ic.star;const label=row.querySelector('.smart-label');label.textContent=smartFolderTitle(folder);row.style.display=folder.on?'':'none';row.onclick=()=>{clearMessageSelection();goMail();document.querySelectorAll('.navitem').forEach(item=>item.classList.remove('active'));row.classList.add('active');filterSmart(index);};accountLabel.before(row);});}
bindSmartNavigation();
ctxsmart.querySelector('[data-smart-action="open"]').onclick=()=>filterSmart(+ctxsmart.dataset.index);
ctxsmart.querySelector('[data-smart-action="edit"]').onclick=()=>openSmart(+ctxsmart.dataset.index);
ctxsmart.querySelector('[data-smart-action="settings"]').onclick=()=>{showView('settingsView');setSection('smart');};

const auxOverlay=document.getElementById('auxOverlay'),eventForm=document.getElementById('eventForm'),contactForm=document.getElementById('contactForm');
let editingEvent=null,editingContact=null;
function closeAuxEditor(){auxOverlay.classList.remove('open');editingEvent=null;editingContact=null;}
function fillAccountSelect(select,selected){select.innerHTML='';coreAccounts.forEach(account=>{const option=document.createElement('option');option.value=account.id;option.textContent=account.email;select.appendChild(option);});if(selected!=null)select.value=String(selected);}
function fillCalendarSelect(accountId,selected){const select=document.getElementById('eventCalendar');select.innerHTML='';(coreCalendarData.calendars||[]).filter(calendar=>calendar.account_id===Number(accountId)).forEach(calendar=>{const option=document.createElement('option');option.value=calendar.id;option.textContent=calendar.name;select.appendChild(option);});if(selected!=null)select.value=String(selected);}
function localDateValue(value){const date=parseDavDate(value);if(!date)return '';const p=n=>String(n).padStart(2,'0');return `${date.getFullYear()}-${p(date.getMonth()+1)}-${p(date.getDate())}T${p(date.getHours())}:${p(date.getMinutes())}`;}
function remoteDateValue(value,allDay){if(allDay)return String(value).slice(0,10);const date=new Date(value);return Number.isNaN(date.getTime())?value:date.toISOString();}
function setAuxMode(mode){const isEvent=mode==='event';eventForm.classList.toggle('hidden',!isEvent);contactForm.classList.toggle('hidden',isEvent);document.getElementById('auxTitle').textContent=isEvent?(editingEvent?L('Изменить событие / задачу','Edit event / task'):L('Новое событие / задача','New event / task')):(editingContact?L('Изменить контакт','Edit contact'):L('Новый контакт','New contact'));document.getElementById('auxIcon').innerHTML=isEvent?ic.cal:ic.people;}
function openEventEditor(event=null){editingEvent=event;editingContact=null;setAuxMode('event');fillAccountSelect(document.getElementById('eventAccount'),event?coreCalendarData.calendars.find(calendar=>calendar.id===event.calendar_id)?.account_id:coreAccounts[0]?.id);fillCalendarSelect(document.getElementById('eventAccount').value,event?.calendar_id);document.getElementById('eventAccount').disabled=Boolean(event);document.getElementById('eventCalendar').disabled=Boolean(event);document.getElementById('eventSummary').value=event?.summary||'';const start=event?.dtstart?localDateValue(event.dtstart):localDateValue(new Date(Math.ceil(Date.now()/1800000)*1800000).toISOString());document.getElementById('eventStart').value=start;document.getElementById('eventEnd').value=event?.dtend?localDateValue(event.dtend):localDateValue(new Date(new Date(start).getTime()+3600000).toISOString());document.getElementById('eventAllDay').checked=Boolean(event?.all_day)||/^\d{4}-\d{2}-\d{2}$/.test(event?.dtstart||'')||/^\d{8}$/.test(event?.dtstart||'');document.getElementById('eventLocation').value=event?.location||'';document.getElementById('eventDescription').value=event?.description||'';document.getElementById('eventDelete').classList.toggle('hidden',!event);document.getElementById('eventStatus').textContent='';auxOverlay.classList.add('open');document.getElementById('eventSummary').focus();}
function openContactEditor(contact=null){editingContact=contact;editingEvent=null;setAuxMode('contact');fillAccountSelect(document.getElementById('contactAccount'),contact?.account_id||coreAccounts[0]?.id);document.getElementById('contactAccount').disabled=Boolean(contact);document.getElementById('contactDisplayName').value=contact?.display_name||'';document.getElementById('contactFirstName').value=contact?.first_name||'';document.getElementById('contactLastName').value=contact?.last_name||'';document.getElementById('contactOrganization').value=contact?.organization||'';document.getElementById('contactEmails').value=(contact?.emails||[]).map(item=>item.email).join('\n');document.getElementById('contactDelete').classList.toggle('hidden',!contact);document.getElementById('contactStatus').textContent='';auxOverlay.classList.add('open');document.getElementById('contactDisplayName').focus();}
document.getElementById('eventAccount').onchange=e=>fillCalendarSelect(e.target.value);
document.getElementById('newEventBtn').onclick=()=>openEventEditor();
document.getElementById('newContactBtn').onclick=()=>openContactEditor();
document.getElementById('auxClose').onclick=closeAuxEditor;document.getElementById('eventCancel').onclick=closeAuxEditor;document.getElementById('contactCancel').onclick=closeAuxEditor;
auxOverlay.addEventListener('click',event=>{if(event.target===auxOverlay)closeAuxEditor();});
eventForm.onsubmit=async event=>{event.preventDefault();const status=document.getElementById('eventStatus'),allDay=document.getElementById('eventAllDay').checked,input={summary:document.getElementById('eventSummary').value.trim(),description:document.getElementById('eventDescription').value.trim()||null,location:document.getElementById('eventLocation').value.trim()||null,dtstart:remoteDateValue(document.getElementById('eventStart').value,allDay),dtend:document.getElementById('eventEnd').value?remoteDateValue(document.getElementById('eventEnd').value,allDay):null,all_day:allDay};status.textContent=L('Сохраняю на сервере…','Saving to the server…');status.dataset.kind='';try{if(editingEvent)await window.tm.updateEvent(editingEvent.id,input);else await window.tm.createEvent(Number(document.getElementById('eventAccount').value),Number(document.getElementById('eventCalendar').value),input);await window.reloadCoreData();closeAuxEditor();showToast(L('Событие сохранено и синхронизировано','Event saved and synced'));}catch(error){status.textContent=String(error);status.dataset.kind='error';}};
document.getElementById('eventDelete').onclick=async()=>{if(!editingEvent||!confirm(L('Удалить событие или задачу на сервере?','Delete this event or task on the server?')))return;const status=document.getElementById('eventStatus');status.textContent=L('Удаляю…','Deleting…');try{await window.tm.deleteEvent(editingEvent.id);await window.reloadCoreData();closeAuxEditor();showToast(L('Удалено на сервере','Deleted on the server'));}catch(error){status.textContent=String(error);status.dataset.kind='error';}};
contactForm.onsubmit=async event=>{event.preventDefault();const status=document.getElementById('contactStatus'),input={display_name:document.getElementById('contactDisplayName').value.trim(),first_name:document.getElementById('contactFirstName').value.trim()||null,last_name:document.getElementById('contactLastName').value.trim()||null,organization:document.getElementById('contactOrganization').value.trim()||null,emails:document.getElementById('contactEmails').value.split(/[\n,;]/).map(value=>value.trim()).filter(Boolean)};status.textContent=L('Сохраняю на сервере…','Saving to the server…');status.dataset.kind='';try{if(editingContact)await window.tm.updateContact(editingContact.id,input);else await window.tm.createContact(Number(document.getElementById('contactAccount').value),input);await window.reloadCoreData();closeAuxEditor();showToast(L('Контакт сохранён и синхронизирован','Contact saved and synced'));}catch(error){status.textContent=String(error);status.dataset.kind='error';}};
document.getElementById('contactDelete').onclick=async()=>{if(!editingContact||!confirm(L('Удалить контакт на сервере?','Delete this contact on the server?')))return;const status=document.getElementById('contactStatus');status.textContent=L('Удаляю…','Deleting…');try{await window.tm.deleteContact(editingContact.id);await window.reloadCoreData();closeAuxEditor();showToast(L('Контакт удалён на сервере','Contact deleted on the server'));}catch(error){status.textContent=String(error);status.dataset.kind='error';}};
calSection.addEventListener('click',e=>{const item=e.target.closest('.ev,.wk-ev');if(!item)return;e.stopPropagation();const event=coreCalendarData.events.find(value=>value.id===Number(item.dataset.eventId));if(event)openEventEditor(event);});

/* welcome wizard */
const wizardText={
  ru:{languageTitle:'Выберите язык',languageSub:'Его можно изменить позже в настройках.',next:'Далее',back:'Назад',welcome:'Добро пожаловать в truemail',welcomeSub:'Быстрый, удобный и красивый почтовый клиент. Вся почта хранится локально на вашем устройстве.',start:'Начать настройку',skip:'Пропустить',connectTitle:'Подключите Яндекс',connectSub:'Один безопасный вход подключит почту, календарь и контакты. Пароль вводить в truemail не нужно.',emailPlaceholder:'you@yandex.ru',connect:'Войти через Яндекс ID',codePlaceholder:'Код подтверждения',confirm:'Подтвердить подключение',openingYandex:'Открываю Яндекс ID в браузере…',enterCode:'После входа скопируйте сюда код подтверждения.',connecting:'Проверяю доступ и загружаю почту, календарь и контакты…',connected:'Аккаунт подключён: почта, календарь и контакты готовы.',themeTitle:'Оформление',themeSub:'Тему, плотность и акцент можно поменять в любой момент.',themeLight:'Светлая',themeDefault:'По умолчанию',themeDark:'Тёмная',themeDarkSub:'Для тёмного окружения',themeSystem:'Системная',themeSystemSub:'Следовать за ОС',securityTitle:'Всё под защитой',securitySub:'Настраивать ничего не нужно — безопасные значения уже действуют.',securityLocal:'Вся почта хранится локально на устройстве',securityTokens:'OAuth-токены — в системном хранилище паролей',securityTracking:'Трекинг-пиксели и UTM-метки блокируются',done:'Всё готово!',openMail:'Открыть почту',invalidEmail:'Введите адрес Яндекс Почты.',oauthUnavailable:'OAuth-мост доступен только внутри приложения truemail.'},
  en:{languageTitle:'Choose your language',languageSub:'You can change it later in Settings.',next:'Continue',back:'Back',welcome:'Welcome to truemail',welcomeSub:'A fast, comfortable and beautiful email client. All your mail stays local on your device.',start:'Start setup',skip:'Skip',connectTitle:'Connect Yandex',connectSub:'One secure sign-in connects mail, calendar and contacts. You never enter your password in truemail.',emailPlaceholder:'you@yandex.com',connect:'Sign in with Yandex ID',codePlaceholder:'Confirmation code',confirm:'Confirm connection',openingYandex:'Opening Yandex ID in your browser…',enterCode:'After signing in, paste the confirmation code here.',connecting:'Checking access and loading mail, calendar and contacts…',connected:'Account connected: mail, calendar and contacts are ready.',themeTitle:'Appearance',themeSub:'You can change the theme, density and accent at any time.',themeLight:'Light',themeDefault:'Default',themeDark:'Dark',themeDarkSub:'For dark environments',themeSystem:'System',themeSystemSub:'Follow the operating system',securityTitle:'Protected by default',securitySub:'Nothing to configure — secure defaults are already active.',securityLocal:'All mail is stored locally on this device',securityTokens:'OAuth tokens are kept in the system credential store',securityTracking:'Tracking pixels and UTM parameters are blocked',done:'All set!',openMail:'Open mail',invalidEmail:'Enter your Yandex Mail address.',oauthUnavailable:'The OAuth bridge is only available inside the truemail app.'}
};
Object.assign(wizardText.ru,{connectTitle:'Подключите почту',connectSub:'Введите любой адрес — truemail определит провайдера и выберет способ входа.',emailPlaceholder:'you@example.com',connect:'Подключить',invalidEmail:'Введите корректный адрес почты.',oauthUnavailable:'Подключение аккаунта работает в desktop-приложении.'});
Object.assign(wizardText.en,{connectTitle:'Connect your email',connectSub:'Enter any address — truemail will detect the provider and choose a sign-in method.',emailPlaceholder:'you@example.com',connect:'Connect',invalidEmail:'Enter a valid email address.',oauthUnavailable:'Account connection is available in the desktop app.'});
Object.assign(wizardText.ru,{codeExpired:'Код истёк или уже был использован. Нажмите «Подключить» и получите новый код.'});
Object.assign(wizardText.en,{codeExpired:'The code expired or was already used. Select Connect to get a new code.'});
Object.assign(wizardText.ru,{storageTitle:'Папка данных',storageSub:'Здесь будут храниться зашифрованная почта, календарь, контакты и индекс.',storagePath:'Путь хранения',chooseFolder:'Выбрать…',storageRequired:'Выберите папку данных.',keyTitle:'Создайте ключи шифрования',keySub:'Водите мышью внутри поля, пока шкала не заполнится. Движения используются один раз и не сохраняются.',keyMove:'Двигайте мышью здесь',createKeys:'Создать защищённое хранилище',creatingStorage:'Создаю ключи и зашифрованную базу…'});
Object.assign(wizardText.en,{storageTitle:'Data folder',storageSub:'Encrypted mail, calendars, contacts and the search index will be stored here.',storagePath:'Storage path',chooseFolder:'Choose…',storageRequired:'Choose a data folder.',keyTitle:'Create encryption keys',keySub:'Move the mouse inside the area until the bar is full. The movements are used once and are never stored.',keyMove:'Move the mouse here',createKeys:'Create encrypted storage',creatingStorage:'Creating keys and the encrypted database…'});
Object.assign(wizardText.ru,{securityRecovery:'Сохраните парольный backup ключей в разделе «Хранилище»: он восстановит доступ к локальному архиву после переустановки.'});
Object.assign(wizardText.en,{securityRecovery:'Save a password-protected key backup in Storage: it restores access to the local archive after reinstalling.'});
Object.assign(wizardText.ru,{restoreArchive:'Или восстановите ключи существующего архива',chooseBackup:'Выбрать backup…',backupPassword:'Пароль backup',restoreKeys:'Восстановить архив',keyBackupTitle:'Резервная копия ключей',keyBackupDesc:'Зашифрованный backup позволяет открыть локальный архив после переустановки или потери системного хранилища ключей.',backupPasswordDesc:'Не менее 12 символов. Без этого пароля восстановление невозможно.',backupPasswordConfirm:'Повторите пароль',exportKeyBackup:'Сохранить backup ключей'});
Object.assign(wizardText.en,{restoreArchive:'Or restore the keys for an existing archive',chooseBackup:'Choose backup…',backupPassword:'Backup password',restoreKeys:'Restore archive',keyBackupTitle:'Key backup',keyBackupDesc:'An encrypted backup lets you open the local archive after reinstalling or losing the system credential-store keys.',backupPasswordDesc:'At least 12 characters. Recovery is impossible without this password.',backupPasswordConfirm:'Repeat password',exportKeyBackup:'Save key backup'});
// Static UI strings shared by index.html (data-i18n / data-i18n-title / -placeholder / -aria / -tip / -ph).
Object.assign(wizardText.ru,{
  tipResize:'Изменить ширину панели',composeTip:'Написать письмо',tipSettings:'Настройки',navCalendar:'Календарь',navContacts:'Контакты',
  navSmartFolders:'Умные папки',createSmartFolder:'Создать умную папку',navAccounts:'Аккаунты',addAccount:'Добавить аккаунт',navAllInbox:'Все входящие',
  tipFilter:'Фильтр',tipSort:'Сортировка',filterPlaceholder:'Фильтр по тексту…',filterUnread:'Только непрочитанные',filterAttachments:'С вложениями',filterFlagged:'Важные',
  sortNewest:'Сначала новые',sortOldest:'Сначала старые',sortSender:'По отправителю',sortSubject:'По теме',
  selectAll:'Выбрать все',markRead:'Прочитать',actionArchive:'В архив',actionDelete:'Удалить',tipClearSelection:'Снять выделение',searchAllPlaceholder:'Поиск по всем ящикам',
  actionReply:'Ответить',actionReplyAll:'Ответить всем',actionForward:'Переслать',tipMore:'Ещё действия',viewSource:'Исходный текст',createRuleFromMessage:'Создать правило из письма',processingRules:'Правила обработки',configureMailToolbar:'Настроить панель письма',
  calMonth:'Месяц',calWeek:'Неделя',calDay:'День',newEvent:'Событие / задача',contactSearchPlaceholder:'Поиск по контактам',viewCards:'Плашки',viewTable:'Таблица',newContact:'Контакт',
  backToMail:'Назад к почте',setGeneral:'Общие',setToolbar:'Панель письма',setUnified:'Сквозные папки',setFolders:'Сопоставление папок',setCalendars:'Календари',setStorageNav:'Хранилище и кэш',setThemes:'Темы и оформление',setPrivacy:'Приватность',setKeys:'Горячие клавиши',
  notifyPosition:'Где показывать уведомления',notifyPositionDesc:'Угол основного монитора, в котором появляются карточки о новых письмах и напоминания.',
  notifyTopLeft:'Сверху слева',notifyTopCenter:'Сверху по центру',notifyTopRight:'Сверху справа',notifyBottomLeft:'Снизу слева',notifyBottomCenter:'Снизу по центру',notifyBottomRight:'Снизу справа',
  generalLead:'Основные параметры программы.',expertMode:'Режим эксперта',expertModeDesc:'Показывать продвинутые настройки и поля (большинству не нужны). По умолчанию - только необходимое для работы.',uiLanguage:'Язык интерфейса',
  showConversations:'Показывать беседы',showConversationsDesc:'Группировать переписку в одну строку со счётчиком (по теме и цепочке ответов, в пределах аккаунта). Клик разворачивает все письма беседы.',
  previewLinesLabel:'Строк превью в списке',previewLinesDesc:'Сколько строк текста письма показывать под темой в списке.',previewLines1:'1 строка',previewLines2:'2 строки',previewLines3:'3 строки',
  autostart:'Запускать при старте системы',autostartDesc:'truemail будет запускаться автоматически и сворачиваться в трей. Окно можно открыть из значка в трее.',setupWizard:'Мастер настройки',setupWizardDesc:'Повторно выбрать язык и подключить аккаунт.',run:'Запустить',
  toolbarTitle:'Панель над письмом',toolbarLead:'Выберите, какие кнопки показывать над открытым письмом и в каком порядке. Тумблер справа - показывать кнопку; стрелки - менять порядок.',alignment:'Расположение',alignmentDesc:'Слева — рядом с началом письма; справа — у правого края.',alignLeft:'Слева',alignRight:'Справа',
  accountsLead:'Подключённые почтовые ящики. Клик - настройки аккаунта, подпись и серверы.',accountWizardTitle:'Мастер подключения аккаунта',secureOauth:'Безопасный вход OAuth',emailAddress:'Адрес почты',connection:'Подключение',connectionDesc:'Провайдер и доступные сервисы определятся автоматически.',
  confirmationCode:'Код подтверждения',confirmationCodeDesc:'Скопируйте код со страницы Яндекс ID после входа.',codeShort:'Код',confirmShort:'Подтвердить',cancel:'Отмена',
  foldersLead:'Сопоставьте папки ящика с ролями truemail - чтобы сквозные "Отправленные", "Черновики" и т.д. работали одинаково для всех аккаунтов.',foldersNote:'Спецпапки определяются автоматически, но их можно переназначить вручную.',
  smartLead:'Папки, которые сами собирают письма по условиям (все входящие, с вложениями, за сегодня и любые ваши). Меняйте порядок, показывайте/скрывайте, редактируйте условия.',
  rulesLead:'Автоматически обрабатывайте новые письма по отправителю или теме: перемещайте в папку, архив, спам или корзину.',createRule:'Создать правило',newRule:'Новое правило',ruleNameLabel:'Название',ruleNamePlaceholder:'Например: Чеки в Финансы',ruleFieldLabel:'Поле',fieldSender:'Отправитель',fieldSubject:'Тема',ruleOperatorLabel:'Сравнение',opContains:'содержит',opEquals:'равно',ruleValueLabel:'Значение',ruleAccountLabel:'Аккаунт',ruleActionLabel:'Действие',actMoveToFolder:'Переместить в папку',actToSpam:'В спам',ruleFolderLabel:'Папка',ruleApplyExisting:'Применить также к уже загруженным письмам',save:'Сохранить',deleteRule:'Удалить правило',noRules:'Правил пока нет.',
  unifiedLead:'Выберите физические папки почтовых ящиков, из которых умные папки могут собирать письма. По умолчанию участвуют все папки. Например, «Все входящие» дополнительно оставляет только источники с ролью «Входящие».',unifiedNote:'Отключённая здесь папка не попадёт ни в одну умную папку. Сами условия — «входящие», «за 24 часа», «с вложениями» — редактируются в разделе «Умные папки».',
  calendarsLead:'Календари теперь настраиваются внутри соответствующего аккаунта: truemail сам определяет CalDAV, Exchange или iCal.',goToAccounts:'Перейти к аккаунтам',
  themesLead:'Темы применяются на лету и меняют не только цвет, но и размеры элементов. Можно импортировать чужие темы.',theme:'Тема',mode:'Режим',density:'Плотность',densityDesc:'Меняет высоту строк и отступы',densityCompact:'Плотно',densityNormal:'Обычно',densitySpacious:'Просторно',accent:'Акцент',accentIndigo:'Индиго',accentTeal:'Бирюза',accentRose:'Роза',accentAmber:'Янтарь',accentBlue:'Синий',accentViolet:'Фиолет',accentCyan:'Голубой',accentGreen:'Зелёный',accentOrange:'Оранжевый',scale:'Масштаб',uiTextSize:'Размер интерфейсного текста',uiTextSizeDesc:'От 50% до 250%, применяется сразу.',preview:'Предпросмотр',
  setStorage:'Хранилище',storageLead:'truemail держит зашифрованную локальную копию почты для скорости и работы без сети. Здесь видно, что занимает место, где оно лежит и как настроено хранение.',diskUsage:'Занято на диске',localData:'локальных данных',localDataDistribution:'Распределение локальных данных',encryptedDb:'Зашифрованная база',encryptedFiles:'Зашифрованные файлы',locationEncryption:'Расположение и шифрование',dataFolder:'Папка для хранения данных',dataFolderTip:'Где truemail держит зашифрованную локальную копию почты, вложений и индекса. Можно вынести на отдельный или зашифрованный диск.',change:'Изменить',openFolder:'Открыть папку',inExplorer:'В проводнике',freeUpSpace:'Освободить место',oldAttachments:'Вложения старше года',oldAttachmentsDesc:'Останутся на сервере, скачаются при открытии',clear:'Очистить',trashSpamData:'Локальные данные корзины и спама',clearAllLocal:'Полностью очистить локальные данные',clearAllLocalDesc:'Письма перекачаются с серверов заново',clearAll:'Очистить всё',
  privacyTitle:'Приватность и безопасность',privacyLead:'Безопасные настройки уже действуют. Здесь - только то, где выбор реально за вами.',protectionOn:'Защита включена',protectionDesc:'HTML-письма открываются без скриптов в изолированном контейнере. Удалённые изображения блокируются; разрешение можно дать отдельно для конкретного отправителя. База и локальные файлы зашифрованы.',
  keysLead:'Текущие рабочие сочетания клавиш.',globalShortcuts:'Глобальные (работают даже когда окно свёрнуто)',globalShortcutsTip:'Системные горячие клавиши: срабатывают во всей ОС, даже если truemail свёрнут или вы в другой программе. Например, быстро написать письмо или показать окно, не открывая приложение мышью.',keyToggleApp:'Показать / скрыть truemail',keyComposeGlobal:'Написать письмо (из любого места)',keyQuickSearch:'Быстрый поиск по почте',localShortcuts:'Локальные (внутри приложения)',localShortcutsTip:'Действуют, когда открыто окно truemail. Классические почтовые сочетания для быстрой работы без мыши.',keySearchCommands:'Поиск и команды',keyCompose:'Написать письмо',keyNextPrev:'Следующее / предыдущее письмо',
  tipBackToList:'Назад к списку',newMessage:'Новое письмо',fromLabel:'От кого',toLabel:'Кому',recipientsPlaceholder:'Получатели',cc:'Копия',bccShort:'СК',hideCcField:'Скрыть поле Копия',bcc:'Скрытая',bccFull:'Скрытая копия',hideBccField:'Скрыть поле Скрытая копия',subjectLabel:'Тема',subjectPlaceholder:'Тема письма',bodyPlaceholder:'Текст письма...',fmtBold:'Жирный',fmtItalic:'Курсив',fmtUnderline:'Подчёркнутый',fmtList:'Список',fmtLink:'Ссылка',fmtMention:'Упомянуть',fmtAttach:'Вложение',send:'Отправить',sendLater:'Отправить позже',sendDateTime:'Дата и время отправки',deleteDraft:'Удалить черновик',
  allThemeSettings:'Все настройки тем',searchAndCommands:'Поиск и команды',commandPlaceholder:'Поиск: письма, контакты, команды, настройки - на любой раскладке',
  newSmartFolder:'Новая умная папка',nameAndIcon:'Название и значок',smartNamePlaceholder:'Например: Важное от коллег',changeIcon:'Изменить значок',conditions:'Условия отбора',smartLogicHelp:'Внутри каждой группы выберите «И» или «ИЛИ». Между группами всегда действует «ИЛИ».',addCondition:'Добавить условие',addOrGroup:'Добавить группу ИЛИ',matchingNow:'Сейчас подходит писем:',delete:'Удалить',
  insertLink:'Вставить ссылку',close:'Закрыть',linkTextLabel:'Текст ссылки',linkTextPlaceholder:'Как показать в письме',linkUrlLabel:'Адрес (URL)',insert:'Вставить',
  event:'Событие',calendarOrTaskList:'Календарь или список задач',titleLabel:'Название',startDue:'Начало / срок',endLabel:'Окончание',allDay:'Весь день',locationLabel:'Место',descriptionLabel:'Описание',displayName:'Отображаемое имя',firstName:'Имя',lastName:'Фамилия',organization:'Организация',emailsOnePerLine:'Email (по одному в строке)',
  open:'Открыть',editConditions:'Изменить условия',smartFolderSettings:'Настройки умных папок',configureMapping:'Настроить сопоставление',rename:'Переименовать',deleteFolder:'Удалить папку',
});
Object.assign(wizardText.en,{
  tipResize:'Resize panel',composeTip:'Compose',tipSettings:'Settings',navCalendar:'Calendar',navContacts:'Contacts',
  navSmartFolders:'Smart folders',createSmartFolder:'Create smart folder',navAccounts:'Accounts',addAccount:'Add account',navAllInbox:'All inboxes',
  tipFilter:'Filter',tipSort:'Sort',filterPlaceholder:'Filter by text…',filterUnread:'Unread only',filterAttachments:'With attachments',filterFlagged:'Flagged',
  sortNewest:'Newest first',sortOldest:'Oldest first',sortSender:'By sender',sortSubject:'By subject',
  selectAll:'Select all',markRead:'Mark read',actionArchive:'Archive',actionDelete:'Delete',tipClearSelection:'Clear selection',searchAllPlaceholder:'Search all mailboxes',
  actionReply:'Reply',actionReplyAll:'Reply all',actionForward:'Forward',tipMore:'More actions',viewSource:'View source',createRuleFromMessage:'Create rule from message',processingRules:'Rules',configureMailToolbar:'Customize message toolbar',
  calMonth:'Month',calWeek:'Week',calDay:'Day',newEvent:'Event / task',contactSearchPlaceholder:'Search contacts',viewCards:'Cards',viewTable:'Table',newContact:'Contact',
  backToMail:'Back to mail',setGeneral:'General',setToolbar:'Message toolbar',setUnified:'Unified folders',setFolders:'Folder mapping',setCalendars:'Calendars',setStorageNav:'Storage and cache',setThemes:'Themes and appearance',setPrivacy:'Privacy',setKeys:'Keyboard shortcuts',
  notifyPosition:'Notification position',notifyPositionDesc:'Corner of the primary monitor where new mail cards and reminders appear.',
  notifyTopLeft:'Top left',notifyTopCenter:'Top center',notifyTopRight:'Top right',notifyBottomLeft:'Bottom left',notifyBottomCenter:'Bottom center',notifyBottomRight:'Bottom right',
  generalLead:'Core application settings.',expertMode:'Expert mode',expertModeDesc:'Show advanced settings and fields (most people do not need them). By default, only what is essential for daily work.',uiLanguage:'Interface language',
  showConversations:'Conversation view',showConversationsDesc:'Group a thread into a single row with a counter (by subject and reply chain, within an account). Click to expand every message in the conversation.',
  previewLinesLabel:'Preview lines in list',previewLinesDesc:'How many lines of message text to show under the subject in the list.',previewLines1:'1 line',previewLines2:'2 lines',previewLines3:'3 lines',
  autostart:'Launch on system startup',autostartDesc:'truemail will start automatically and minimize to the tray. Open the window from the tray icon.',setupWizard:'Setup wizard',setupWizardDesc:'Choose the language again and connect an account.',run:'Launch',
  toolbarTitle:'Toolbar above the message',toolbarLead:'Choose which buttons appear above an open message and in what order. The toggle on the right shows the button; the arrows reorder it.',alignment:'Alignment',alignmentDesc:'Left - next to the start of the message; right - at the right edge.',alignLeft:'Left',alignRight:'Right',
  accountsLead:'Connected mailboxes. Click for account settings, signature and servers.',accountWizardTitle:'Account connection wizard',secureOauth:'Secure OAuth sign-in',emailAddress:'Email address',connection:'Connection',connectionDesc:'The provider and available services are detected automatically.',
  confirmationCode:'Confirmation code',confirmationCodeDesc:'Copy the code from the Yandex ID page after signing in.',codeShort:'Code',confirmShort:'Confirm',cancel:'Cancel',
  foldersLead:'Map mailbox folders to truemail roles so unified "Sent", "Drafts" and so on work the same for every account.',foldersNote:'Special folders are detected automatically, but you can reassign them manually.',
  smartLead:'Folders that gather messages by conditions (all inboxes, with attachments, from today and any of your own). Reorder, show or hide, and edit conditions.',
  rulesLead:'Automatically process new messages by sender or subject: move them to a folder, archive, spam or trash.',createRule:'Create rule',newRule:'New rule',ruleNameLabel:'Name',ruleNamePlaceholder:'For example: Receipts to Finance',ruleFieldLabel:'Field',fieldSender:'Sender',fieldSubject:'Subject',ruleOperatorLabel:'Comparison',opContains:'contains',opEquals:'equals',ruleValueLabel:'Value',ruleAccountLabel:'Account',ruleActionLabel:'Action',actMoveToFolder:'Move to folder',actToSpam:'Spam',ruleFolderLabel:'Folder',ruleApplyExisting:'Also apply to already loaded messages',save:'Save',deleteRule:'Delete rule',noRules:'No rules yet.',
  unifiedLead:'Choose the physical mailbox folders that smart folders can gather messages from. By default all folders take part. For example, "All inboxes" additionally keeps only sources with the Inbox role.',unifiedNote:'A folder disabled here will not appear in any smart folder. The conditions themselves - "inbox", "last 24 hours", "with attachments" - are edited in the "Smart folders" section.',
  calendarsLead:'Calendars are now configured inside the relevant account: truemail detects CalDAV, Exchange or iCal automatically.',goToAccounts:'Go to accounts',
  themesLead:'Themes apply on the fly and change not only color but also element sizes. You can import other themes.',theme:'Theme',mode:'Mode',density:'Density',densityDesc:'Changes row height and spacing',densityCompact:'Compact',densityNormal:'Normal',densitySpacious:'Comfortable',accent:'Accent',accentIndigo:'Indigo',accentTeal:'Teal',accentRose:'Rose',accentAmber:'Amber',accentBlue:'Blue',accentViolet:'Violet',accentCyan:'Cyan',accentGreen:'Green',accentOrange:'Orange',scale:'Scale',uiTextSize:'Interface text size',uiTextSizeDesc:'From 50% to 250%, applied instantly.',preview:'Preview',
  setStorage:'Storage',storageLead:'truemail keeps an encrypted local copy of your mail for speed and offline work. Here you can see what takes up space, where it is stored and how storage is configured.',diskUsage:'Disk usage',localData:'of local data',localDataDistribution:'Local data distribution',encryptedDb:'Encrypted database',encryptedFiles:'Encrypted files',locationEncryption:'Location and encryption',dataFolder:'Data storage folder',dataFolderTip:'Where truemail keeps the encrypted local copy of mail, attachments and the index. You can place it on a separate or encrypted drive.',change:'Change',openFolder:'Open folder',inExplorer:'In file explorer',freeUpSpace:'Free up space',oldAttachments:'Attachments older than a year',oldAttachmentsDesc:'They stay on the server and download when opened',clear:'Clear',trashSpamData:'Local Trash and Spam data',clearAllLocal:'Clear all local data',clearAllLocalDesc:'Messages will be re-downloaded from the servers',clearAll:'Clear all',
  privacyTitle:'Privacy and security',privacyLead:'Secure settings are already active. Here is only what is genuinely up to you.',protectionOn:'Protection enabled',protectionDesc:'HTML messages open without scripts in an isolated container. Remote images are blocked; you can allow them per sender. The database and local files are encrypted.',
  keysLead:'Current active keyboard shortcuts.',globalShortcuts:'Global (work even when the window is minimized)',globalShortcutsTip:'System-wide shortcuts: they fire across the whole OS, even if truemail is minimized or you are in another app. For example, quickly compose a message or show the window without opening the app with the mouse.',keyToggleApp:'Show / hide truemail',keyComposeGlobal:'Compose message (from anywhere)',keyQuickSearch:'Quick mail search',localShortcuts:'Local (inside the app)',localShortcutsTip:'Work when the truemail window is open. Classic mail shortcuts for fast, mouse-free work.',keySearchCommands:'Search and commands',keyCompose:'Compose message',keyNextPrev:'Next / previous message',
  tipBackToList:'Back to list',newMessage:'New message',fromLabel:'From',toLabel:'To',recipientsPlaceholder:'Recipients',cc:'Cc',bccShort:'Bcc',hideCcField:'Hide Cc field',bcc:'Bcc',bccFull:'Blind carbon copy',hideBccField:'Hide Bcc field',subjectLabel:'Subject',subjectPlaceholder:'Message subject',bodyPlaceholder:'Message text...',fmtBold:'Bold',fmtItalic:'Italic',fmtUnderline:'Underline',fmtList:'List',fmtLink:'Link',fmtMention:'Mention',fmtAttach:'Attachment',send:'Send',sendLater:'Send later',sendDateTime:'Send date and time',deleteDraft:'Delete draft',
  allThemeSettings:'All theme settings',searchAndCommands:'Search and commands',commandPlaceholder:'Search: mail, contacts, commands, settings - any keyboard layout',
  newSmartFolder:'New smart folder',nameAndIcon:'Name and icon',smartNamePlaceholder:'For example: Important from colleagues',changeIcon:'Change icon',conditions:'Match conditions',smartLogicHelp:'Within each group choose "AND" or "OR". Groups are always combined with "OR".',addCondition:'Add condition',addOrGroup:'Add OR group',matchingNow:'Matching messages now:',delete:'Delete',
  insertLink:'Insert link',close:'Close',linkTextLabel:'Link text',linkTextPlaceholder:'How it appears in the message',linkUrlLabel:'Address (URL)',insert:'Insert',
  event:'Event',calendarOrTaskList:'Calendar or task list',titleLabel:'Title',startDue:'Start / due',endLabel:'End',allDay:'All day',locationLabel:'Location',descriptionLabel:'Description',displayName:'Display name',firstName:'First name',lastName:'Last name',organization:'Organization',emailsOnePerLine:'Email (one per line)',
  open:'Open',editConditions:'Edit conditions',smartFolderSettings:'Smart folder settings',configureMapping:'Configure mapping',rename:'Rename',deleteFolder:'Delete folder',
});
let wizardLocale='';
let pendingOauthState='';
function wt(key){return (wizardText[wizardLocale]||wizardText.en)[key]||key;}
let uiCatalog={};
const uiKeyByRussian={
  'Умные папки':'nav-smart-folders','Аккаунты':'nav-accounts','Календарь':'nav-calendar','Контакты':'nav-contacts',
  'Все входящие':'nav-all-inbox','Все важные':'nav-all-important','Все отправленные':'nav-all-sent','Все черновики':'nav-all-drafts',
  'Сегодня':'nav-today','Непрочитанные (все)':'nav-unread','С вложениями':'nav-with-attachments','Ждут ответа':'nav-waiting-reply',
  'Ответить':'action-reply','Ответить всем':'action-reply-all','Переслать':'action-forward','В архив':'action-archive','Удалить':'action-delete','Написать':'action-compose','Отправить':'action-send',
  'Настройки':'settings','Общие':'settings-general','Панель письма':'settings-toolbar','Сквозные папки':'settings-unified','Сопоставление папок':'settings-folders','Календари':'settings-calendars','Хранилище':'settings-storage','Темы и оформление':'settings-themes','Приватность':'settings-privacy','Горячие клавиши':'settings-keys'
};
function applyUiCatalog(catalog){
  uiCatalog=catalog||{};
  const walker=document.createTreeWalker(document.body,NodeFilter.SHOW_TEXT);const nodes=[];while(walker.nextNode())nodes.push(walker.currentNode);
  nodes.forEach(node=>{const raw=node.nodeValue||'',trimmed=raw.trim(),key=node.__truemailI18nKey||uiKeyByRussian[trimmed];if(key&&uiCatalog[key]){node.__truemailI18nKey=key;node.nodeValue=raw.replace(trimmed,uiCatalog[key]);}});
  const palette=document.getElementById('cmdInput');if(palette&&uiCatalog['palette-placeholder'])palette.placeholder=uiCatalog['palette-placeholder'];
  document.querySelectorAll('.tbrow').forEach(row=>{const key=`action-${row.dataset.action==='trash'?'delete':row.dataset.action}`;const label=row.querySelector('.nm');if(label&&uiCatalog[key])label.textContent=uiCatalog[key];});
}
window.applyUiCatalog=applyUiCatalog;
function applyWizardLanguage(locale,persist=true){
  wizardLocale=locale;document.documentElement.lang=locale;
  document.querySelectorAll('[data-i18n]').forEach(el=>{const value=wizardText[locale][el.dataset.i18n];if(value)el.textContent=value;});
  document.querySelectorAll('[data-i18n-placeholder]').forEach(el=>{const value=wizardText[locale][el.dataset.i18nPlaceholder];if(value)el.placeholder=value;});
  document.querySelectorAll('[data-i18n-title]').forEach(el=>{const value=wizardText[locale][el.dataset.i18nTitle];if(value)el.title=value;});
  document.querySelectorAll('[data-i18n-aria]').forEach(el=>{const value=wizardText[locale][el.dataset.i18nAria];if(value)el.setAttribute('aria-label',value);});
  document.querySelectorAll('[data-i18n-tip]').forEach(el=>{const value=wizardText[locale][el.dataset.i18nTip];if(value){el.dataset.tip=value;if(el.hasAttribute('aria-label'))el.setAttribute('aria-label',value);}});
  document.querySelectorAll('[data-i18n-ph]').forEach(el=>{const value=wizardText[locale][el.dataset.i18nPh];if(value)el.dataset.ph=value;});
  document.querySelectorAll('[data-wlang]').forEach(el=>el.classList.toggle('sel',el.dataset.wlang===locale));
  if(typeof relocalizeDynamic==='function')relocalizeDynamic();
  document.getElementById('wzLanguageNext').disabled=false;
  const languageSetting=document.getElementById('languageSetting');if(languageSetting)languageSetting.value=locale;
  if(window.tm?.localizationCatalog)window.tm.localizationCatalog(locale).then(applyUiCatalog).catch(console.error);
  if(persist&&window.tmStorageReady){window.tm?.setSetting('locale',locale).catch(console.error);}
}
window.applyWizardLanguage=applyWizardLanguage;
function relocalizeDynamic(){
  try{
    if(typeof renderSmartManagement==='function')renderSmartManagement();
    if(typeof bindSmartNavigation==='function')bindSmartNavigation();
    if(typeof applyToolbar==='function')applyToolbar();
    if(typeof renderRulesList==='function')renderRulesList();
    if(typeof renderContacts==='function')renderContacts();
    if(typeof updateSelectionUi==='function')updateSelectionUi();
    const heading=document.querySelector('.listhead h2');
    if(heading){
      if(currentFolderId!==null){const folder=coreFolders.find(item=>item.id===currentFolderId);if(folder)heading.textContent=folderTitle(folder);}
      else if(currentSmartIndex!=null&&smartFolders[currentSmartIndex])heading.textContent=smartFolderTitle(smartFolders[currentSmartIndex])||messagesTitle();
    }
    const accountCount=document.getElementById('mailAccountCount');
    if(accountCount&&coreAccounts.length){const n=coreAccounts.length,label=smartIsEnglish()?(n===1?'account':'accounts'):(n%10===1&&n%100!==11?'аккаунт':n%10>=2&&n%10<=4&&(n%100<10||n%100>=20)?'аккаунта':'аккаунтов');accountCount.textContent=`${n} ${label}`;}
  }catch(error){console.error('relocalize',error);}
}
function wzGo(n){document.querySelectorAll('.wzstep').forEach(s=>s.classList.remove('active'));document.getElementById('wz'+n).classList.add('active');
  document.querySelectorAll('.wzdot').forEach((d,i)=>d.classList.toggle('on',i<n));}
function showWizard(step=1){showView('welcomeView');wzGo(step);}
window.showWizard=showWizard;
document.querySelectorAll('[data-wz]').forEach(b=>b.onclick=()=>wzGo(b.dataset.wz));
document.querySelectorAll('[data-wlang]').forEach(o=>o.onclick=()=>applyWizardLanguage(o.dataset.wlang));
if(wizardLocale&&wizardText[wizardLocale])applyWizardLanguage(wizardLocale,false);
document.getElementById('languageSetting').onchange=e=>applyWizardLanguage(e.target.value);
document.querySelectorAll('[data-wtheme]').forEach(o=>o.onclick=()=>{document.querySelectorAll('[data-wtheme]').forEach(x=>x.classList.toggle('sel',x===o));setTheme(o.dataset.wtheme);});
async function finishOnboarding(){try{await window.tm?.setSetting('onboarding_completed','true');await window.reloadCoreData?.();}catch(e){console.error(e);}showView('mailView');}
document.getElementById('wzSkipMain').onclick=finishOnboarding;
document.getElementById('wzFinish').onclick=finishOnboarding;
document.getElementById('restartWizard').onclick=()=>showWizard(window.tmStorageReady?4:1);

const entropyTargetBytes=4*1024;
const entropyChunks=[];
let entropyBytes=0;
let entropyEvents=0;
let lastEntropySample=null;
let selectedDataDir='';
let selectedBackupPath='';
const entropyPad=document.getElementById('entropyPad');
const entropyProgress=document.getElementById('entropyProgress');
const entropyCaption=document.getElementById('entropyCaption');
const createKeysButton=document.getElementById('wzCreateKeys');

function configureStorageWizard(status){
  window.tmStorageReady=Boolean(status.ready);
  selectedDataDir=status.data_dir||'';
  document.getElementById('wzDataDir').value=selectedDataDir;
  const storagePath=document.querySelector('#set-storage .d.mono');if(storagePath)storagePath.textContent=selectedDataDir;
}
window.configureStorageWizard=configureStorageWizard;

document.getElementById('wzChooseDataDir').onclick=async()=>{
  const status=document.getElementById('wzStorageStatus');
  try{
    const chosen=await window.tm?.chooseDataDir(document.getElementById('wzDataDir').value||window.tmDefaultDataDir);
    if(typeof chosen==='string'&&chosen){document.getElementById('wzDataDir').value=chosen;selectedDataDir=chosen;status.textContent='';}
  }catch(error){status.textContent=error.message||String(error);status.dataset.kind='error';}
};
document.getElementById('wzChooseBackup').onclick=async()=>{
  const status=document.getElementById('wzRestoreStatus');
  try{
    const chosen=await window.tm?.chooseKeyBackup(selectedBackupPath||selectedDataDir);
    if(typeof chosen==='string'&&chosen){selectedBackupPath=chosen;document.getElementById('wzBackupPath').value=chosen;status.textContent='';}
  }catch(error){status.textContent=error.message||String(error);status.dataset.kind='error';}
};
document.getElementById('wzRestoreKeys').onclick=async()=>{
  const button=document.getElementById('wzRestoreKeys'),status=document.getElementById('wzRestoreStatus'),passwordInput=document.getElementById('wzRestorePassword');
  selectedDataDir=document.getElementById('wzDataDir').value.trim();const password=passwordInput.value;
  if(!selectedDataDir||!selectedBackupPath||!password){status.textContent=wizardLocale==='en'?'Choose the data folder and key backup, then enter its password.':'Выберите папку архива и backup ключей, затем введите пароль.';status.dataset.kind='error';return;}
  try{
    button.disabled=true;status.textContent=wizardLocale==='en'?'Opening encrypted archive…':'Открываю зашифрованный архив…';status.dataset.kind='';
    await window.tm.restoreKeyBackup(selectedDataDir,selectedBackupPath,password);
    passwordInput.value='';window.tmStorageReady=true;status.textContent='';configureStorageWizard(await window.tm.bootstrapStatus());wzGo(4);
  }catch(error){passwordInput.value='';status.textContent=error.message||String(error);status.dataset.kind='error';}
  finally{button.disabled=false;}
};
document.getElementById('wzStorageNext').onclick=()=>{
  const input=document.getElementById('wzDataDir');
  const status=document.getElementById('wzStorageStatus');
  selectedDataDir=input.value.trim();
  if(!selectedDataDir){status.textContent=wt('storageRequired');status.dataset.kind='error';return;}
  status.textContent='';wzGo(3);
};

entropyPad.addEventListener('pointermove',event=>{
  if(!event.isTrusted||entropyBytes>=entropyTargetBytes||window.tmStorageReady)return;
  const now=performance.now();
  const sample=[event.clientX,event.clientY,event.screenX,event.screenY,event.movementX,event.movementY,Math.floor(now*1000),Math.floor((now%1)*0xffffffff),event.buttons];
  if(lastEntropySample&&sample[0]===lastEntropySample[0]&&sample[1]===lastEntropySample[1]&&sample[6]===lastEntropySample[6])return;
  const bytes=new Uint8Array(40);
  const view=new DataView(bytes.buffer);
  sample.forEach((value,index)=>view.setInt32(index*4,value|0,true));
  view.setUint32(36,entropyEvents++,true);
  entropyChunks.push(bytes);entropyBytes+=bytes.length;lastEntropySample=sample;
  const rect=entropyPad.getBoundingClientRect();
  const cursor=document.getElementById('entropyCursor');cursor.style.left=`${event.clientX-rect.left}px`;cursor.style.top=`${event.clientY-rect.top}px`;
  const percent=Math.min(100,Math.floor(entropyBytes/entropyTargetBytes*100));
  entropyProgress.style.width=`${percent}%`;entropyCaption.textContent=`${percent}%`;
  if(percent===100){createKeysButton.disabled=false;document.getElementById('entropyHint').classList.add('hidden');}
});

let entropyCreationStarted=false;
async function createStorageFromEntropy(){
  const status=document.getElementById('wzEntropyStatus');
  if(entropyCreationStarted||entropyBytes<entropyTargetBytes||!selectedDataDir)return;entropyCreationStarted=true;
  const entropy=new Uint8Array(entropyBytes);let offset=0;
  for(const chunk of entropyChunks){entropy.set(chunk,offset);offset+=chunk.length;}
  try{
    createKeysButton.disabled=true;status.textContent=wt('creatingStorage');status.dataset.kind='';
    await window.tm.initializeStorage(selectedDataDir,wizardLocale||'ru',Array.from(entropy));
    window.tmStorageReady=true;entropy.fill(0);entropyChunks.forEach(chunk=>chunk.fill(0));entropyChunks.length=0;entropyBytes=0;lastEntropySample=null;status.textContent='';wzGo(4);
  }catch(error){entropy.fill(0);entropyCreationStarted=false;createKeysButton.disabled=false;status.textContent=error.message||String(error);status.dataset.kind='error';}
}
createKeysButton.onclick=createStorageFromEntropy;
function showAccountWizard(prefillEmail=''){
  accountOauthState='';accountPasswordProvider='generic';
  const status=document.getElementById('accountOauthStatus'),start=document.getElementById('accountOauthStart'),confirm=document.getElementById('accountOauthConfirm'),code=document.getElementById('accountOauthCode');
  status.textContent='';status.dataset.kind='';start.disabled=false;confirm.disabled=false;code.value='';document.getElementById('accountEmail').value=typeof prefillEmail==='string'?prefillEmail:'';document.getElementById('accountCodeRow').classList.add('hidden');document.getElementById('accountPasswordRow').classList.add('hidden');document.getElementById('accountPassword').value='';
  document.querySelector('.settings').classList.add('account-wizard-mode');showView('settingsView');setSection('addacct');
}
function closeAccountWizard(){document.querySelector('.settings').classList.remove('account-wizard-mode');setSection('accounts');}
window.showAccountWizard=showAccountWizard;
document.getElementById('addAcct').onclick=showAccountWizard;
document.getElementById('settingsAddAccount').onclick=showAccountWizard;
document.querySelector('[data-set="addacct"]')?.addEventListener('click',showAccountWizard);
document.getElementById('accountWizardCancel').onclick=closeAccountWizard;

window.clearDemoData=function(preserveMessage=false){
  document.querySelectorAll('.acc-h,.acc-sub,.acct-row').forEach(el=>el.remove());
  document.querySelectorAll('.nav .count').forEach(el=>{el.textContent='';});
  document.getElementById('msgs').innerHTML='';if(!preserveMessage){document.getElementById('tSubject').textContent='';const actions=document.querySelector('.thread .actions');if(actions)actions.classList.add('hidden');document.getElementById('tbody').innerHTML='';}
  ['calgrid','calweek','calday','cgrid'].forEach(id=>{const el=document.getElementById(id);if(el)el.innerHTML='';});
  const contactCount=document.querySelector('.ct-count');if(contactCount)contactCount.textContent=wizardLocale==='en'?'0 contacts':'0 контактов';
  document.getElementById('acctDetail')?.remove();
  document.querySelectorAll('#set-folders .card,#set-unified .card,#set-calendars .card').forEach(card=>{if(card.querySelector('.acct-line'))card.remove();});
  document.querySelectorAll('#set-storage .acct-line').forEach(row=>row.closest('.frow')?.remove());
  const storageSize=document.querySelector('.storage-big');if(storageSize)storageSize.textContent='0 Б';
  document.querySelectorAll('.legend').forEach(legend=>legend.remove());
  const from=document.querySelector('.from-sel');if(from)from.innerHTML='';
  const signature=document.querySelector('.compose .sig');if(signature)signature.textContent='';
};
window.showEmptyMailbox=function(){
  window.clearDemoData();
  document.getElementById('tbody').innerHTML=`<div class="mail-empty"><div class="wz-logo brand-mark">${document.querySelector('.wz-logo.brand-mark').innerHTML}</div><h2>${wizardLocale==='en'?'No accounts connected':'Нет подключённых аккаунтов'}</h2><p>${wizardLocale==='en'?'Connect an account to load your data.':'Подключите аккаунт, чтобы загрузить данные.'}</p><button class="btn primary" id="emptyConnect">${wizardLocale==='en'?'Connect account':'Подключить аккаунт'}</button></div>`;
  document.getElementById('emptyConnect').onclick=showAccountWizard;
};
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
function renderContacts(contacts=coreContacts){const query=(document.querySelector('.ct-search input')?.value||'').trim(),filtered=contacts.filter(contact=>matchQ(`${contact.display_name||''} ${(contact.emails||[]).map(item=>item.email).join(' ')}`,query)),grid=document.getElementById('cgrid');grid.innerHTML='';filtered.forEach((contact,index)=>{const card=document.createElement('div');card.className='ccard';card.dataset.contactId=contact.id;card.innerHTML=`<span class="ava ava-c${index%8}"></span><div><div class="cn"></div><div class="ce"></div></div>`;card.querySelector('.ava').textContent=(contact.display_name||contact.emails?.[0]?.email||'?').split(/\s+/).map(word=>word[0]).join('').slice(0,2).toUpperCase();card.querySelector('.cn').textContent=contact.display_name||contact.emails?.[0]?.email||'';card.querySelector('.ce').textContent=contact.emails?.[0]?.email||'';card.onclick=()=>openContactEditor(contact);grid.appendChild(card);});const count=document.querySelector('.ct-count');if(count)count.textContent=`${filtered.length}${query?` / ${contacts.length}`:''} ${wizardLocale==='en'?'contacts':'контактов'}`;}
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
  const trustKey=`remote_images_sender:${String(sender||'').trim().toLocaleLowerCase()}`;
  const allowRemote=Boolean(sender)&&await window.tm?.getSetting(trustKey).catch(()=>null)==='true';
  const parsed=new DOMParser().parseFromString(html,'text/html');
  parsed.querySelectorAll('script,iframe,object,embed,form,input,button,textarea,select,base,link,meta,audio,video').forEach(node=>node.remove());
  let blocked=false;
  parsed.querySelectorAll('style').forEach(node=>{node.textContent=node.textContent.replace(/url\(\s*(['"]?)https?:[^)]*\)/gi,'none');});
  parsed.querySelectorAll('*').forEach(node=>{[...node.attributes].forEach(attr=>{const name=attr.name.toLowerCase(),value=attr.value.trim();if(name.startsWith('on')||['srcdoc','formaction','integrity','nonce'].includes(name)||((name==='href'||name==='src'||name==='xlink:href')&&/^\s*(?:javascript|file|data:text\/html):/i.test(value)))node.removeAttribute(attr.name);else if(name==='style')node.setAttribute('style',value.replace(/url\(\s*(['"]?)https?:[^)]*\)/gi,'none'));});});
  parsed.querySelectorAll('a').forEach(link=>{link.target='_blank';link.rel='noopener noreferrer';try{const url=new URL(link.href);[...url.searchParams.keys()].filter(key=>key.toLowerCase().startsWith('utm_')||['fbclid','gclid'].includes(key.toLowerCase())).forEach(key=>url.searchParams.delete(key));link.href=url.toString();}catch(_){}});
  parsed.querySelectorAll('img,source').forEach(image=>{const src=image.getAttribute('src')||image.getAttribute('srcset')||'';if(/^https?:/i.test(src)&&!allowRemote){blocked=true;image.removeAttribute('src');image.removeAttribute('srcset');image.setAttribute('alt',image.getAttribute('alt')||L('Удалённое изображение заблокировано','Remote image blocked'));}image.setAttribute('loading','lazy');image.setAttribute('referrerpolicy','no-referrer');image.style.maxWidth='100%';image.style.height='auto';});
  container.classList.add('html');
  if(blocked){const notice=document.createElement('div');notice.className='blocked';const text=document.createElement('span');text.textContent=L('Удалённые изображения заблокированы для защиты от отслеживания.','Remote images are blocked to prevent tracking.');const button=document.createElement('button');button.type='button';button.textContent=L(`Показывать от ${sender}`,`Always show from ${sender}`);button.onclick=async()=>{await window.tm?.setSetting(trustKey,'true');container.replaceChildren();await renderHtmlMessage(container,html,sender);};notice.append(text,button);container.appendChild(notice);}
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
function createMessageRow(message,index){
  const row=document.createElement('div');row.className='msg'+(message.flags?.seen?'':' unread')+(message._convChild?' conv-child':'')+(selectedMessageIds.has(message.id)?' selected':'')+(activeMessage?.id===message.id?' active':'');row.dataset.messageId=message.id;
  const initial=(message.from?.name||message.from?.email||'?').trim()[0].toUpperCase();
  row.innerHTML=`<div class="avawrap"><span class="ava" style="background:${accountColorById(message.account_id)}"></span></div><div class="body"><div class="l1"><span class="from"></span></div><div class="subj"></div><div class="prev"></div></div><div class="meta"><span class="time"></span><span class="time-hm"></span></div>`;
  row.querySelector('.ava').textContent=initial;row.querySelector('.from').textContent=message.from?.name||message.from?.email||'';
  if(message._convCount>1){const expanded=expandedConversations.has(message._convKey);const badge=document.createElement('button');badge.type='button';badge.className='conv-count'+(expanded?' on':'');badge.textContent=message._convCount;badge.title=expanded?L('Свернуть беседу','Collapse conversation'):L(`Показать письма беседы (${message._convCount})`,`Show conversation messages (${message._convCount})`);badge.onclick=event=>{event.stopPropagation();toggleConversation(message._convKey);};row.querySelector('.l1').appendChild(badge);}
  row.querySelector('.subj').textContent=message.subject||'';row.querySelector('.prev').textContent=message.preview||'';
  row.querySelector('.time').textContent=message.date?new Date(message.date).toLocaleDateString(document.documentElement.lang):'';
  row.querySelector('.time-hm').textContent=message.date?new Date(message.date).toLocaleTimeString(document.documentElement.lang,{hour:'2-digit',minute:'2-digit'}):'';
  row.onpointerenter=e=>{if(selectionDragMode===null||!(e.buttons&1))return;selectionDragMode?selectedMessageIds.add(message.id):selectedMessageIds.delete(message.id);updateSelectionUi();};
  row.onclick=e=>{if(e.shiftKey){selectMessageRange(index,e.ctrlKey||e.metaKey);return;}if(e.ctrlKey||e.metaKey){selectedMessageIds.has(message.id)?selectedMessageIds.delete(message.id):selectedMessageIds.add(message.id);lastSelectedMessageIndex=index;updateSelectionUi();return;}if(selectedMessageIds.size)clearMessageSelection();lastSelectedMessageIndex=index;showMessage(message);};renderIcons(row);return row;
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
    const header=document.createElement('div');header.className='acc-h open';
    const initial=(account.display_name||account.email||'?').trim()[0].toUpperCase();
    header.innerHTML=`<span class="ava" style="background:${accountColorById(account.id)}"></span><span class="em"></span><span class="chev"><i data-i="chevR"></i></span>`;
    header.querySelector('.ava').textContent=initial;header.querySelector('.em').textContent=account.email;
    anchor.after(header);anchor=header;
    const sub=document.createElement('div');sub.className='acc-sub open';
    const accountFolders=sortedFolders(foldersByAccount[index]||[]);
    accountFolders.forEach(folder=>{const row=document.createElement('div');row.className='navitem folder-row';row.dataset.folderId=folder.id;
      const icon=folderIcon(folder);const depth=Math.max(0,(folder.remote_path.match(/[\/|]/g)||[]).length);row.style.paddingLeft=`${14+depth*14}px`;
      row.innerHTML=`<i data-i="${icon}"></i><span class="folder-name"></span>${folder.unread_count?'<span class="count"></span>':''}`;
      row.querySelector('.folder-name').textContent=folderTitle(folder);if(folder.unread_count)row.querySelector('.count').textContent=folder.unread_count;
      const openFolder=()=>{goMail();document.querySelectorAll('.navitem').forEach(item=>item.classList.remove('active'));row.classList.add('active');currentFolderId=folder.id;currentSmartIndex=null;applyListOptions(true,folderTitle(folder));};row.onclick=openFolder;row.oncontextmenu=event=>{event.preventDefault();event.stopPropagation();contextFolder=folder;contextFolderOpen=openFolder;ctxfolder.dataset.system=folder.role?'true':'false';ctxfolder.querySelectorAll('[data-folder-action="rename"],[data-folder-action="delete"]').forEach(item=>item.classList.toggle('disabled',Boolean(folder.role)));ctxfolder.style.left=`${Math.min(event.clientX,innerWidth-250)}px`;ctxfolder.style.top=`${Math.min(event.clientY,innerHeight-190)}px`;ctxfolder.classList.add('open');};sub.appendChild(row);});
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
function updateAccountConnectionType(){const exchange=document.getElementById('accountConnectionType').value==='exchange';document.getElementById('accountEwsServer').classList.toggle('hidden',!exchange);document.querySelectorAll('#accountPasswordRow .server-pair').forEach(row=>row.classList.toggle('hidden',exchange));}
document.getElementById('accountConnectionType').onchange=updateAccountConnectionType;
function showPasswordConnection(config){accountPasswordProvider=config.provider;document.getElementById('accountConnectionType').value='imap';document.getElementById('accountUsername').value=config.username||document.getElementById('accountEmail').value.trim();document.getElementById('accountImapHost').value=config.imap?.host||'';document.getElementById('accountImapPort').value=config.imap?.port||993;document.getElementById('accountImapSecurity').value=config.imap?.security||'ssl';document.getElementById('accountSmtpHost').value=config.smtp?.host||'';document.getElementById('accountSmtpPort').value=config.smtp?.port||465;document.getElementById('accountSmtpSecurity').value=config.smtp?.security||'ssl';updateAccountConnectionType();document.getElementById('accountPasswordRow').classList.remove('hidden');document.getElementById('accountPassword').focus();}
document.getElementById('accountOauthStart').onclick=async()=>{
  const email=document.getElementById('accountEmail').value.trim(),status=document.getElementById('accountOauthStatus');
  const button=document.getElementById('accountOauthStart');
  if(!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)){status.textContent=L('Введите корректный адрес почты.','Enter a valid email address.');status.dataset.kind='error';return;}
  if(!window.tm?.beginAccountConnection){status.textContent=L('OAuth доступен внутри приложения truemail.','OAuth is available inside the truemail app.');status.dataset.kind='error';return;}
  try{button.disabled=true;status.textContent=L('Определяю провайдера и способ входа…','Detecting provider and sign-in method…');status.dataset.kind='';const pending=await window.tm.beginAccountConnection(email);if(pending.mode==='connected'&&pending.connected){const connected=pending.connected;status.textContent=connected.warnings?.length?connected.warnings.join(' '):L('Аккаунт подключён.','Account connected.');status.dataset.kind=connected.warnings?.length?'warning':'success';setTimeout(async()=>{closeAccountWizard();await window.reloadCoreData?.();await window.tm?.startRealtime();showView('mailView');},connected.warnings?.length?2500:300);return;}if(pending.mode==='password'){showPasswordConnection(pending.password_config);status.textContent=L('Проверьте серверы и введите пароль приложения или почтовый пароль.','Check the servers and enter an app password or mail password.');return;}accountOauthState=pending.state;document.getElementById('accountCodeRow').classList.remove('hidden');status.textContent=L('После входа скопируйте сюда код подтверждения.','After signing in, paste the confirmation code here.');document.getElementById('accountOauthCode').focus();}
  catch(e){button.disabled=false;status.textContent=e.message||String(e);status.dataset.kind='error';}
};
document.getElementById('accountPasswordConfirm').onclick=async()=>{const button=document.getElementById('accountPasswordConfirm'),status=document.getElementById('accountOauthStatus'),password=document.getElementById('accountPassword').value,email=document.getElementById('accountEmail').value.trim(),username=document.getElementById('accountUsername').value.trim(),exchange=document.getElementById('accountConnectionType').value==='exchange';if(!password){status.textContent=L('Введите пароль.','Enter the password.');status.dataset.kind='error';return;}try{button.disabled=true;status.textContent=exchange?L('Ищу EWS через Autodiscover и проверяю Exchange…','Discovering EWS and checking Exchange…'):L('Проверяю IMAP и подключаю аккаунт…','Checking IMAP and connecting the account…');status.dataset.kind='';const connected=exchange?await window.tm.completeExchangeEws({email,username,password,serverHint:document.getElementById('accountEwsServer').value.trim()}):await window.tm.completePasswordImap({email,username,password,provider:accountPasswordProvider,imapHost:document.getElementById('accountImapHost').value.trim(),imapPort:Number(document.getElementById('accountImapPort').value),imapSecurity:document.getElementById('accountImapSecurity').value,smtpHost:document.getElementById('accountSmtpHost').value.trim(),smtpPort:Number(document.getElementById('accountSmtpPort').value),smtpSecurity:document.getElementById('accountSmtpSecurity').value});document.getElementById('accountPassword').value='';status.textContent=connected.warnings?.length?connected.warnings.join(' '):L('Аккаунт подключён.','Account connected.');status.dataset.kind=connected.warnings?.length?'warning':'success';setTimeout(async()=>{closeAccountWizard();await window.reloadCoreData?.();await window.tm?.startRealtime();showView('mailView');},connected.warnings?.length?2500:300);}catch(error){status.textContent=error.message||String(error);status.dataset.kind='error';button.disabled=false;}};
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
function resetComposer(){composerFieldIds.forEach(id=>document.getElementById(id).value='');['compTo','compCc','compBcc'].forEach(id=>{recipientModel[id]=[];renderRecipientChips(id);});setRecipientFieldVisible('compCc',false);setRecipientFieldVisible('compBcc',false);document.querySelectorAll('.recipient-suggestions').forEach(menu=>menu.classList.remove('open'));compEditEl.innerHTML='';composerAttachments=[];compAtt.innerHTML='';document.getElementById('composeStatus').textContent='';document.getElementById('compSendAt').classList.add('hidden');}
const signatureCache=new Map();let composerSignatureKind='new';
async function accountSignatures(accountId,refresh=false){if(!refresh&&signatureCache.has(accountId))return signatureCache.get(accountId);const values=await window.tm.listSignatures(accountId);signatureCache.set(accountId,values);return values;}
async function applyComposerSignature(kind=composerSignatureKind){composerSignatureKind=kind;compEditEl.querySelector('.composer-signature')?.remove();const accountId=Number(document.querySelector('.from-sel')?.value);if(!accountId)return;try{const signature=(await accountSignatures(accountId)).find(item=>item.kind===kind&&item.enabled&&item.body_html.trim());if(!signature)return;const node=document.createElement('div');node.className='composer-signature';node.innerHTML=signature.body_html;const quote=compEditEl.querySelector('.mail-quote-head');if(quote)compEditEl.insertBefore(node,quote);else compEditEl.appendChild(node);scheduleDraftSave();}catch(error){console.error(error);}}
async function openComposerForMessage(action){if(!activeMessage)return;resetComposer();
  // Отвечаем/пересылаем с того ящика, на который пришло письмо.
  const fromSel=document.querySelector('.from-sel');if(fromSel&&activeMessage.account_id&&[...fromSel.options].some(opt=>opt.value===String(activeMessage.account_id)))fromSel.value=String(activeMessage.account_id);
  const reply=action!=='forward',from=activeFullMessage?.meta?.from?.email||activeMessage.from?.email||'',subject=activeMessage.subject||'',prefix=action==='forward'?'Fwd: ':'Re: ';document.getElementById('compTitle').textContent=action==='forward'?L('Переслать','Forward'):L('Ответить','Reply');document.getElementById('compSubj').value=new RegExp(`^${prefix}`,'i').test(subject)?subject:prefix+subject;if(reply&&from)setRecipients('compTo',[{name:activeFullMessage?.meta?.from?.name||'',email:from}]);if(action==='replyall'){const own=new Set(coreAccounts.map(account=>account.email.toLowerCase()));const others=[...(activeFullMessage?.meta?.to||[]),...(activeFullMessage?.meta?.cc||[])].filter(address=>address.email&&!own.has(address.email.toLowerCase())&&address.email.toLowerCase()!==from.toLowerCase());const seen=new Set();const uniq=others.filter(a=>{const k=a.email.toLowerCase();if(seen.has(k))return false;seen.add(k);return true;});setRecipients('compCc',uniq.map(a=>({name:a.name||'',email:a.email})));setRecipientFieldVisible('compCc',uniq.length>0);}const dateStr=activeMessage.date?new Date(activeMessage.date).toLocaleString(document.documentElement.lang):'';const bodyHtml=activeFullMessage?.body_html,bodyText=activeFullMessage?.body_text||activeMessage.preview||'';const quote=bodyHtml?bodyHtml:escapeHtml(bodyText).replace(/\n/g,'<br>');const header=`${escapeHtml(dateStr)}${dateStr?', ':''}${escapeHtml(activeFullMessage?.meta?.from?.name||from)} &lt;${escapeHtml(from)}&gt;:`;compEditEl.innerHTML=`<p><br></p><div class="mail-quote-head" style="color:var(--text-3,#888)">${header}</div><blockquote style="margin:6px 0 0;padding:0 0 0 12px;border-left:2px solid var(--border,#ccc)">${quote}</blockquote>`;showView('composeView');await applyComposerSignature('reply');const range=document.createRange(),sel=window.getSelection();range.setStart(compEditEl.firstChild,0);range.collapse(true);sel.removeAllRanges();sel.addRange(range);compEditEl.focus();}
function contactAddresses(){const seen=new Set(),result=[];coreContacts.forEach(contact=>(contact.emails||[]).forEach(item=>{const email=String(item.email||'').trim(),key=email.toLocaleLowerCase();if(!email||seen.has(key))return;seen.add(key);result.push({name:contact.display_name||'',email});}));return result;}
function recipientToken(value){return String(value||'').split(/[;,]/).at(-1).trim();}
function chooseRecipient(input,contact){addRecipientEntry(input.id,recipientFormat({name:contact.name,email:contact.email}));input.value='';input.dispatchEvent(new Event('input',{bubbles:true}));input.focus();scheduleDraftSave();}
['compTo','compCc','compBcc'].forEach(id=>{const input=document.getElementById(id),menu=input.parentElement.querySelector('.recipient-suggestions');let active=-1;const render=()=>{const query=recipientToken(input.value),used=new Set([...recipientModel[id].map(entry=>entry.email.toLocaleLowerCase()),...splitAddresses(input.value).map(value=>(value.match(/<([^>]+)>/)?.[1]||value).trim().toLocaleLowerCase())]),matches=query?contactAddresses().filter(contact=>!used.has(contact.email.toLocaleLowerCase())&&matchQ(`${contact.name} ${contact.email}`,query)).slice(0,8):[];active=-1;menu.innerHTML='';matches.forEach((contact,index)=>{const option=document.createElement('button');option.type='button';option.className='recipient-option';option.innerHTML='<span></span><small></small>';option.querySelector('span').textContent=contact.name||contact.email;option.querySelector('small').textContent=contact.email;option.onmousedown=event=>{event.preventDefault();chooseRecipient(input,contact);menu.classList.remove('open');};option.dataset.index=index;menu.appendChild(option);});menu.classList.toggle('open',matches.length>0);};input.addEventListener('input',render);input.addEventListener('focus',render);input.addEventListener('keydown',event=>{const options=[...menu.querySelectorAll('.recipient-option')];if((event.key===','||event.key===';')&&!(active>=0&&options.length)){event.preventDefault();commitRecipientInput(id);menu.classList.remove('open');render();return;}if(event.key==='Backspace'&&!input.value&&recipientModel[id].length){event.preventDefault();removeRecipientEntry(id,recipientModel[id].length-1);return;}if(!options.length){if(event.key==='Enter'&&input.value.trim()){event.preventDefault();commitRecipientInput(id);}return;}if(event.key==='ArrowDown'||event.key==='ArrowUp'){event.preventDefault();active=(active+(event.key==='ArrowDown'?1:-1)+options.length)%options.length;options.forEach((option,index)=>option.classList.toggle('active',index===active));options[active].scrollIntoView({block:'nearest'});}else if(event.key==='Enter'){event.preventDefault();if(active>=0)options[active].dispatchEvent(new MouseEvent('mousedown',{bubbles:true}));else{commitRecipientInput(id);menu.classList.remove('open');}}else if(event.key==='Escape')menu.classList.remove('open');});input.addEventListener('blur',()=>{if(input.value.trim())commitRecipientInput(id);});});
document.addEventListener('click',event=>{if(!event.target.closest('.recipient-input'))document.querySelectorAll('.recipient-suggestions').forEach(menu=>menu.classList.remove('open'));});
function showToast(message,actionLabel,action){document.querySelector('.app-toast')?.remove();const toast=document.createElement('div');toast.className='app-toast';const text=document.createElement('span');text.textContent=message;toast.appendChild(text);if(action){const button=document.createElement('button');button.type='button';button.textContent=actionLabel;button.onclick=async()=>{button.disabled=true;await action();toast.remove();};toast.appendChild(button);}document.body.appendChild(toast);setTimeout(()=>toast.remove(),9000);}
window.handleSyncState=function(state){if(!state)return;const info=document.getElementById('calSyncInfo');if(info&&['dav','auxiliary'].includes(state.scope)){if(state.status==='syncing')info.textContent=wizardLocale==='en'?'Syncing calendars, tasks and contacts…':'Синхронизация календарей, задач и контактов…';else if(state.status==='error')info.textContent=wizardLocale==='en'?'Calendar, tasks and contacts sync error':'Ошибка синхронизации календаря, задач и контактов';}const message=state.status==='error'?(state.error||L('Ошибка синхронизации','Sync error')):(state.warnings?.join(' ')||'');if(!message)return;if(/ACCESS_TOKEN_SCOPE_INSUFFICIENT|insufficient authentication scopes|insufficientPermissions/i.test(message)){const account=coreAccounts.find(item=>item.id===Number(state.account_id));showToast(L('Google не выдал приложению доступ к календарю, контактам и задачам. Переподключите аккаунт и подтвердите все запрошенные разрешения.','Google did not grant access to calendar, contacts and tasks. Reconnect the account and approve all requested permissions.'),L('Переподключить','Reconnect'),()=>showAccountWizard(account?.email||''));return;}showToast(message);};
async function performMessageAction(action){const ids=selectedMessageIds.size?[...selectedMessageIds]:activeMessage?[activeMessage.id]:[];if(!ids.length){showToast(L('Сначала выберите письмо','Select a message first'));return;}
  if(action==='trash'&&ids.length>10&&!await confirmAction(L(`Удалить ${ids.length} писем?`,`Delete ${ids.length} messages?`)))return;
  // Запоминаем соседнее письмо, чтобы после действия перейти к нему, а не терять фокус.
  let nextId=null;
  if(activeMessage&&ids.length===1){const index=currentMessageRows.findIndex(message=>message.id===activeMessage.id);nextId=currentMessageRows[index+1]?.id??currentMessageRows[index-1]?.id??null;}
  try{const queued=await window.tm.messageAction(ids,action);selectedMessageIds.clear();activeMessage=null;activeFullMessage=null;await window.reloadCoreData();
    if(nextId!=null){const message=messages.find(item=>item.id===nextId);if(message)showMessage(message);}
    showToast(action==='archive'?L('Письмо перемещено в архив','Message moved to Archive'):action==='spam'?L('Письмо перемещено в спам','Message moved to Spam'):L('Письмо перемещено в корзину','Message moved to Trash'),L('Отменить','Undo'),async()=>{await window.tm.undoMessageAction(queued.operation_ids);await window.reloadCoreData();});}catch(error){showToast(error.message||String(error));}}
function selectAllCurrentMessages(){currentMessageRows.forEach(message=>selectedMessageIds.add(message.id));updateSelectionUi();}
document.getElementById('bulkSelectAll').onclick=selectAllCurrentMessages;
document.getElementById('bulkClear').onclick=clearMessageSelection;
document.getElementById('bulkArchive').onclick=()=>performMessageAction('archive');
document.getElementById('bulkTrash').onclick=()=>performMessageAction('trash');
document.getElementById('bulkRead').onclick=async()=>{const ids=[...selectedMessageIds];if(!ids.length)return;try{await Promise.all(ids.map(id=>window.tm.markSeen(id,true)));clearMessageSelection();await window.reloadCoreData();showToast(L('Письма отмечены прочитанными','Messages marked as read'));}catch(error){showToast(error.message||String(error));}};
function renderComposerAttachment(item){const el=document.createElement('span');el.className='att-mini';el.innerHTML='<i data-i="paperclip"></i><span class="att-name"></span><span class="csub"></span><span class="x">×</span>';el.querySelector('.att-name').textContent=item.filename;el.querySelector('.csub').textContent=formatBytes(item.data.length);renderIcons(el);el.querySelector('.x').onclick=()=>{composerAttachments=composerAttachments.filter(value=>value!==item);el.remove();scheduleDraftSave();};compAtt.appendChild(el);}
async function addCompFile(file){const item={filename:file.name||'attachment',mime_type:file.type||'application/octet-stream',data:Array.from(new Uint8Array(await file.arrayBuffer()))};composerAttachments.push(item);renderComposerAttachment(item);scheduleDraftSave();}
composeEl.addEventListener('dragover',e=>{e.preventDefault();composeEl.classList.add('dragover');});
composeEl.addEventListener('dragleave',e=>{if(!composeEl.contains(e.relatedTarget))composeEl.classList.remove('dragover');});
composeEl.addEventListener('drop',e=>{e.preventDefault();composeEl.classList.remove('dragover');
  const files=e.dataTransfer&&e.dataTransfer.files;if(files&&files.length){for(const file of files)addCompFile(file).catch(console.error);}});
compEditEl.addEventListener('paste',e=>{const items=e.clipboardData&&e.clipboardData.items;if(!items)return;
  for(const item of items){if(item.type.indexOf('image')===0){const file=item.getAsFile();if(file){e.preventDefault();addCompFile(new File([file],L('изображение из буфера.png','pasted-image.png'),{type:file.type})).catch(console.error);}}}});
document.getElementById('compAttach').onclick=()=>document.getElementById('compFile').click();
document.getElementById('compFile').onchange=e=>{for(const file of e.target.files||[])addCompFile(file).catch(console.error);e.target.value='';};
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
document.getElementById('compSend').onclick=async()=>{const status=document.getElementById('composeStatus'),button=document.getElementById('compSend');try{const request=composerRequest();button.disabled=true;status.textContent=L('Отправляю…','Sending…');status.dataset.kind='';await window.tm.sendMessage(request);await window.tm.setSetting('composer_draft','');status.textContent=L('Отправлено','Sent');status.dataset.kind='success';setTimeout(()=>{resetComposer();showView('mailView');},500);}catch(error){status.textContent=error.message||String(error);status.dataset.kind='error';}finally{button.disabled=false;}};
document.getElementById('compSendLater').onclick=async()=>{const input=document.getElementById('compSendAt'),status=document.getElementById('composeStatus');if(input.classList.contains('hidden')){const date=new Date(Date.now()+15*60*1000);date.setSeconds(0,0);input.value=new Date(date.getTime()-date.getTimezoneOffset()*60000).toISOString().slice(0,16);input.min=new Date(Date.now()-new Date().getTimezoneOffset()*60000).toISOString().slice(0,16);input.classList.remove('hidden');input.focus();return;}try{const date=new Date(input.value);if(Number.isNaN(date.getTime()))throw new Error(L('Выберите дату и время','Choose a date and time'));const id=await window.tm.scheduleMessage(composerRequest(),date.toISOString());await window.tm.setSetting('composer_draft','');status.textContent=L(`Запланировано (задача ${id})`,`Scheduled (task ${id})`);status.dataset.kind='success';setTimeout(()=>{resetComposer();showView('mailView');},700);}catch(error){status.textContent=error.message||String(error);status.dataset.kind='error';}};
document.getElementById('compDeleteDraft').onclick=async()=>{resetComposer();await window.tm?.setSetting('composer_draft','').catch(console.error);showView('mailView');};

/* expert mode toggle */
/* Цвет аккаунта: в строке виден только выбранный, палитра 5x5 - по клику. */
function renderAccountColorPicker(card,account){
  const holder=card.querySelector('.account-colors');
  const current=accountColorById(account.id);
  const button=document.createElement('button');
  button.type='button';button.className='color-current';button.style.background=current;
  button.title=L('Выбрать цвет','Pick a color');
  const grid=document.createElement('div');grid.className='color-grid hidden';
  const close=()=>grid.classList.add('hidden');
  button.onclick=e=>{e.stopPropagation();const open=grid.classList.contains('hidden');document.querySelectorAll('.color-grid').forEach(other=>other.classList.add('hidden'));grid.classList.toggle('hidden',!open);};
  grid.onclick=e=>e.stopPropagation();
  ACCOUNT_COLORS.forEach(color=>{
    const swatch=document.createElement('button');
    swatch.type='button';swatch.className='color-swatch'+(color.toLowerCase()===String(current).toLowerCase()?' on':'');
    swatch.style.background=color;swatch.title=color;
    swatch.onclick=async()=>{
      try{
        await window.tm.setAccountColor(account.id,color);
        account.color=color;
        card.querySelector('.account-ava').style.background=color;
        button.style.background=color;
        grid.querySelectorAll('.color-swatch').forEach(s=>s.classList.toggle('on',s===swatch));
        close();
        await window.reloadCoreData?.();
      }catch(error){showToast(error.message||String(error));}
    };
    grid.appendChild(swatch);
  });
  document.addEventListener('click',close);
  holder.append(button,grid);
}

async function renderSignatureSettings(card,account){const body=card.querySelector('.cb'),panel=document.createElement('section');panel.className='signature-settings';panel.innerHTML=`<div class="t">${L('Подписи','Signatures')}</div><div class="d">${L('Разные подписи для новых писем и ответов.','Separate signatures for new messages and replies.')}</div><div class="signature-grid"></div>`;body.appendChild(panel);const grid=panel.querySelector('.signature-grid');try{const values=await accountSignatures(account.id,true);for(const kind of ['new','reply']){const value=values.find(item=>item.kind===kind)||{body_html:'',enabled:false};const item=document.createElement('div');item.className='signature-item';item.innerHTML=`<div class="signature-item-head"><strong>${kind==='new'?L('Новое письмо','New message'):L('Ответ','Reply')}</strong><label><input type="checkbox"${value.enabled?' checked':''}> ${L('Включена','Enabled')}</label></div><div class="signature-editor" contenteditable="true" data-ph="${L('Текст подписи','Signature text')}"></div><button class="btn sm signature-save" type="button">${L('Сохранить','Save')}</button>`;const editor=item.querySelector('.signature-editor'),enabled=item.querySelector('input');editor.innerHTML=value.body_html||'';item.querySelector('.signature-save').onclick=async()=>{try{await window.tm.saveSignature(account.id,kind,editor.innerHTML,enabled.checked);signatureCache.delete(account.id);await accountSignatures(account.id);showToast(L('Подпись сохранена','Signature saved'));}catch(error){showToast(error.message||String(error));}};grid.appendChild(item);}}catch(error){panel.querySelector('.d').textContent=error.message||String(error);}}

function renderAccountSettings(accounts,foldersByAccount,calendars){
  const page=document.getElementById('set-accounts');page.querySelectorAll('.account-card').forEach(card=>card.remove());
  accounts.forEach((account,index)=>{const folders=foldersByAccount[index]||[],accountCalendars=calendars.filter(cal=>cal.account_id===account.id);const card=document.createElement('div');card.className='card account-card';card.innerHTML=`<div class="ch"><span class="ava ava-26 account-ava" style="background:${accountColorById(account.id)}"></span><div class="grow"><div class="account-name"></div><div class="account-email"></div><div class="account-stats"></div></div><div class="account-actions"><button type="button" class="btn sm account-rename"><i data-i="edit"></i>${L('Переименовать','Rename')}</button><button type="button" class="btn sm account-folders"><i data-i="folder"></i>${L('Папки','Folders')}</button><button type="button" class="btn sm account-reconnect"><i data-i="lock"></i>${L('Переподключить','Reconnect')}</button></div></div><div class="cb"><div class="frow"><div class="fl"><div class="t">${L('Цвет аккаунта','Account color')}</div><div class="d">${L('Для аватаров писем и панели папок.','For message avatars and the folder panel.')}</div></div><div class="fc"><div class="account-colors"></div></div></div><div class="t" style="margin-top:14px">${L('Хранить письма локально','Keep mail locally')}</div><div class="d">${L('Письма старше выбранного срока автоматически удаляются из локального кэша (на сервере остаются). Свежие письма кэшируются целиком и открываются мгновенно.','Messages older than the selected period are automatically removed from the local cache (they stay on the server). Recent messages are cached in full and open instantly.')}</div><div class="account-retention"><input type="number" class="inp ret-num" min="1" max="999" value="1"><select class="sel ret-unit"><option value="1">${L('дней','days')}</option><option value="7">${L('недель','weeks')}</option><option value="30">${L('месяцев','months')}</option></select><label class="ret-unlim"><input type="checkbox" class="ret-unlimited"> ${L('без ограничений','no limit')}</label></div><div class="t" style="margin-top:14px">${L('Календари и адресные книги определяются автоматически по адресу аккаунта.','Calendars and address books are detected automatically from the account address.')}</div><div class="account-calendars"></div></div>`;card.querySelector('.ava').textContent=(account.display_name||account.email)[0].toUpperCase();card.querySelector('.account-name').textContent=account.display_name||account.email;card.querySelector('.account-email').textContent=account.email;card.querySelector('.account-stats').textContent=L(`${folders.length} папок · ${accountCalendars.length} календарей`,`${folders.length} folders · ${accountCalendars.length} calendars`);card.querySelector('.account-rename').onclick=async()=>{const name=prompt(L('Название аккаунта','Account name'),account.display_name||account.email);if(!name||name.trim()===account.display_name)return;try{await window.tm.renameAccount(account.id,name.trim());await window.reloadCoreData();showToast(L('Название аккаунта сохранено','Account name saved'));}catch(error){showToast(error.message||String(error));}};card.querySelector('.account-folders').onclick=()=>setSection('folders');card.querySelector('.account-reconnect').onclick=()=>showAccountWizard(account.email);const chips=card.querySelector('.account-calendars');(accountCalendars.length?accountCalendars:[{name:L('Календарь ещё синхронизируется','Calendar is still syncing')}]).forEach(cal=>{const chip=document.createElement('span');chip.className='calendar-chip';chip.textContent=cal.name;chips.appendChild(chip);});
  renderAccountColorPicker(card,account);
  // Глубина кэша: раскладываем retention_days на число + единицу (дни/недели/месяцы).
  const retNum=card.querySelector('.ret-num'),retUnit=card.querySelector('.ret-unit'),retUnlim=card.querySelector('.ret-unlimited');
  const days=Number(account.retention_days)||0;
  if(days<=0){retUnlim.checked=true;retNum.value=1;retUnit.value='1';}
  else if(days%30===0){retNum.value=days/30;retUnit.value='30';}
  else if(days%7===0){retNum.value=days/7;retUnit.value='7';}
  else{retNum.value=days;retUnit.value='1';}
  const applyRetention=async()=>{retNum.disabled=retUnlim.checked;retUnit.disabled=retUnlim.checked;const value=retUnlim.checked?0:Math.max(1,Number(retNum.value)||1)*Number(retUnit.value);try{await window.tm.setAccountRetention(account.id,value);account.retention_days=value;showToast(value?L(`Кэш: хранить ${value} дн.`,`Cache: keep ${value} days`):L('Кэш без ограничений','Cache without limit'));}catch(error){showToast(error.message||String(error));}};
  retNum.disabled=retUnlim.checked;retUnit.disabled=retUnlim.checked;
  retNum.onchange=applyRetention;retUnit.onchange=applyRetention;retUnlim.onchange=applyRetention;
  renderSignatureSettings(card,account);
  renderIcons(card);page.appendChild(card);});
  const mapping=document.getElementById('set-folders');mapping.querySelectorAll('.mapping-generated').forEach(el=>el.remove());accounts.forEach((account,index)=>{const card=document.createElement('div');card.className='card mapping-generated';card.innerHTML=`<div class="ch">${escapeHtml(account.email)}</div><div class="cb"></div>`;const body=card.querySelector('.cb'),folders=sortedFolders(foldersByAccount[index]||[]),roleTitles=smartIsEnglish()?{inbox:'Inbox',sent:'Sent',drafts:'Drafts',archive:'Archive',spam:'Spam',trash:'Trash'}:{inbox:'Входящие',sent:'Отправленные',drafts:'Черновики',archive:'Архив',spam:'Спам',trash:'Корзина'};['inbox','sent','drafts','archive','spam','trash'].forEach(role=>{const row=document.createElement('div');row.className='map-row map-2';row.innerHTML=`<div class="role">${roleTitles[role]}</div><select class="sel"><option value="">${L('Не назначено','Not assigned')}</option>${folders.map(folder=>`<option value="${folder.id}"${folder.role===role?' selected':''}>${escapeHtml(folder.display_name)}</option>`).join('')}</select>`;row.querySelector('select').onchange=async()=>{const value=row.querySelector('select').value;try{await window.tm?.setFolderRole(account.id,role,value?+value:null);await window.reloadCoreData?.();showToast(L('Сопоставление папки сохранено','Folder mapping saved'));}catch(error){showToast(error.message||String(error));}};body.appendChild(row);});mapping.appendChild(card);});
  const unified=document.getElementById('set-unified');unified.querySelectorAll('.unified-generated').forEach(el=>el.remove());const info=document.createElement('div');info.className='card unified-generated';info.innerHTML=`<div class="ch"><span class="grow">${L('Источники писем для умных папок','Message sources for smart folders')}</span><span class="source-count"></span></div><div class="cb"><div class="unified-toolbar"><button type="button" class="btn sm" data-source-mode="all">${L('Выбрать все','Select all')}</button><button type="button" class="btn sm" data-source-mode="standard">${L('Только стандартные','Standard only')}</button></div><div class="unified-accounts"></div></div>`;const roleNames=smartIsEnglish()?{inbox:'Inbox',sent:'Sent',drafts:'Drafts',archive:'Archive',spam:'Spam',trash:'Trash'}:{inbox:'Входящие',sent:'Отправленные',drafts:'Черновики',archive:'Архив',spam:'Спам',trash:'Корзина'};window.coreUnifiedSettings=window.coreUnifiedSettings||{};const refreshSourceCount=()=>{const enabled=coreFolders.filter(folder=>window.coreUnifiedSettings[folder.id]!=='0').length;info.querySelector('.source-count').textContent=L(`${enabled} из ${coreFolders.length} включено`,`${enabled} of ${coreFolders.length} enabled`);if(currentSmartIndex!==null)filterSmart(currentSmartIndex);};const setSource=async(folder,enabled)=>{await window.tm.setUnifiedSource(folder.id,enabled);window.coreUnifiedSettings[folder.id]=enabled?'1':'0';const checkbox=info.querySelector(`[data-source-folder="${folder.id}"]`);if(checkbox)checkbox.checked=enabled;};const groups=info.querySelector('.unified-accounts');accounts.forEach((account,index)=>{const section=document.createElement('section');section.className='unified-account';const title=document.createElement('div');title.className='unified-account-title';title.textContent=account.email;section.appendChild(title);sortedFolders(foldersByAccount[index]||[]).forEach(folder=>{const row=document.createElement('label');row.className='unified-source';const enabled=window.coreUnifiedSettings[folder.id]!=='0';row.innerHTML=`<input type="checkbox" data-source-folder="${folder.id}"${enabled?' checked':''}><span class="source-path"></span><span class="source-role"></span>`;row.querySelector('.source-path').textContent=folder.remote_path||folder.display_name;row.querySelector('.source-role').textContent=roleNames[folder.role]||L('Без роли','No role');row.querySelector('input').onchange=async event=>{try{await setSource(folder,event.target.checked);refreshSourceCount();}catch(error){event.target.checked=!event.target.checked;showToast(error.message||String(error));}};section.appendChild(row);});groups.appendChild(section);});info.querySelector('[data-source-mode="all"]').onclick=async()=>{try{await Promise.all(coreFolders.map(folder=>setSource(folder,true)));refreshSourceCount();}catch(error){showToast(error.message||String(error));}};info.querySelector('[data-source-mode="standard"]').onclick=async()=>{try{await Promise.all(coreFolders.map(folder=>setSource(folder,Boolean(folder.role))));refreshSourceCount();}catch(error){showToast(error.message||String(error));}};refreshSourceCount();unified.appendChild(info);
  const from=document.querySelector('.from-sel');if(from){from.innerHTML=accounts.map(account=>`<option value="${account.id}">${escapeHtml(account.email)}</option>`).join('');if(window.pendingComposerDraft?.account_id)from.value=String(window.pendingComposerDraft.account_id);from.onchange=()=>{if(document.getElementById('composeView').classList.contains('active'))applyComposerSignature(composerSignatureKind);};}
  if(window.pendingComposerDraft){const draft=window.pendingComposerDraft;setRecipients('compTo',draft.to||'');setRecipients('compCc',draft.cc||'');setRecipients('compBcc',draft.bcc||'');setRecipientFieldVisible('compCc',Boolean(draft.cc));setRecipientFieldVisible('compBcc',Boolean(draft.bcc));document.getElementById('compSubj').value=draft.subject||'';compEditEl.innerHTML=draft.body_html||'';composerAttachments=Array.isArray(draft.attachments)?draft.attachments:[];compAtt.innerHTML='';composerAttachments.forEach(renderComposerAttachment);window.pendingComposerDraft=null;}
}
function applyStorageStatus(storage){document.querySelector('.storage-big').textContent=formatBytes(storage.total_bytes);const path=document.querySelector('#set-storage .d.mono');if(path)path.textContent=storage.data_dir;document.querySelector('.storage-sub').textContent=`${wizardLocale==='en'?'database':'база'} ${formatBytes(storage.database_bytes)} · ${wizardLocale==='en'?'files':'файлы'} ${formatBytes(storage.blob_bytes)}`;const measured=Math.max(1,(storage.database_bytes||0)+(storage.blob_bytes||0));const db=document.querySelector('.usebar .seg-db'),blob=document.querySelector('.usebar .seg-blob');if(db)db.style.width=`${100*(storage.database_bytes||0)/measured}%`;if(blob)blob.style.width=`${100*(storage.blob_bytes||0)/measured}%`;}

const filterMenu=document.getElementById('filterMenu'),sortMenu=document.getElementById('sortMenu'),filterButton=document.getElementById('filterBtn'),sortButton=document.getElementById('sortBtn');
filterButton.onclick=e=>{e.stopPropagation();filterMenu.classList.toggle('hidden');sortMenu.classList.add('hidden');};sortButton.onclick=e=>{e.stopPropagation();sortMenu.classList.toggle('hidden');filterMenu.classList.add('hidden');};
document.addEventListener('click',event=>{if(!filterMenu.contains(event.target)&&!filterButton.contains(event.target))filterMenu.classList.add('hidden');if(!sortMenu.contains(event.target)&&!sortButton.contains(event.target))sortMenu.classList.add('hidden');});
function applyListOptions(resetScroll=false,title=null){let rows=currentFolderId!==null?messages.filter(m=>m.folder_id===currentFolderId):smartRows(currentSmartIndex??0);const active=[...filterMenu.querySelectorAll('input[type="checkbox"]:checked')].map(input=>input.dataset.filter);if(active.includes('unread'))rows=rows.filter(m=>!m.flags?.seen);if(active.includes('attachments'))rows=rows.filter(m=>m.has_attachments);if(active.includes('flagged'))rows=rows.filter(m=>m.flags?.flagged);
  const filterText=(document.getElementById('filterText')?.value||'').trim();if(filterText)rows=rows.filter(m=>matchQ(`${m.from?.name||''} ${m.from?.email||''} ${m.subject||''} ${m.preview||''}`,filterText));const sort=sortMenu.dataset.sort||'date-desc';rows.sort((a,b)=>sort==='date-asc'?byDateAsc(a,b):sort==='sender'?String(a.from?.name||a.from?.email||'').localeCompare(String(b.from?.name||b.from?.email||'')):sort==='subject'?String(a.subject||'').localeCompare(String(b.subject||'')):byDateDesc(a,b));renderMessageList(rows,title||document.querySelector('.listhead h2').textContent,resetScroll);}
filterMenu.querySelectorAll('input[type="checkbox"]').forEach(input=>input.onchange=()=>applyListOptions(true));document.getElementById('filterText')?.addEventListener('input',()=>applyListOptions(true));sortMenu.querySelectorAll('button').forEach(button=>button.onclick=()=>{sortMenu.dataset.sort=button.dataset.sort;sortMenu.classList.add('hidden');applyListOptions(true);});

/* Ширины панелей. Пользователь задаёт их только мышью - за край панели;
   значения хранятся скрыто в настройках и восстанавливаются при старте. */
const NAV_WIDTH_MIN=180,NAV_WIDTH_MAX=420,LIST_WIDTH_MIN=280,LIST_WIDTH_MAX=760;
function setSidebarWidth(value,persist=true){
  value=Math.max(NAV_WIDTH_MIN,Math.min(NAV_WIDTH_MAX,Math.round(+value)||250));
  root.style.setProperty('--nav-w',`${value}px`);
  if(persist)window.tm?.setSetting('sidebar_width',String(value)).catch(console.error);
}
function setListWidth(value,persist=true){
  value=Math.max(LIST_WIDTH_MIN,Math.min(LIST_WIDTH_MAX,Math.round(+value)||392));
  root.style.setProperty('--list-w',`${value}px`);
  if(persist)window.tm?.setSetting('list_width',String(value)).catch(console.error);
}
/* Тянем за разделитель: ширина = позиция курсора минус левый край панели. */
function bindResizer(element,apply){
  if(!element)return;
  element.addEventListener('pointerdown',e=>{element.classList.add('dragging');element.setPointerCapture(e.pointerId);});
  element.addEventListener('pointermove',e=>{if(element.hasPointerCapture(e.pointerId))apply(e.clientX,false);});
  element.addEventListener('pointerup',e=>{if(!element.hasPointerCapture(e.pointerId))return;element.releasePointerCapture(e.pointerId);element.classList.remove('dragging');apply(e.clientX,true);});
}
bindResizer(document.getElementById('navResizer'),(x,persist)=>setSidebarWidth(x,persist));
bindResizer(document.getElementById('listResizer'),(x,persist)=>{
  const navWidth=parseInt(getComputedStyle(root).getPropertyValue('--nav-w'),10)||250;
  setListWidth(x-navWidth,persist);
});

function setUiScale(value,persist=true){value=Math.max(50,Math.min(250,+value||100));document.getElementById('uiScale').value=value;root.style.setProperty('--fs',`${13.5*value/100}px`);document.querySelectorAll('#scalePresets button').forEach(b=>b.classList.toggle('on',+b.dataset.scale===value));if(persist)window.tm?.setSetting('ui_scale',String(value)).catch(console.error);}document.getElementById('uiScale').oninput=e=>setUiScale(e.target.value);document.querySelectorAll('#scalePresets button').forEach(b=>b.onclick=()=>setUiScale(b.dataset.scale));

function confirmAction(message){return new Promise(resolve=>{const overlay=document.createElement('div');overlay.className='overlay open';const modal=document.createElement('div');modal.className='modal compact-modal';const body=document.createElement('div');body.className='mb';body.textContent=message;const foot=document.createElement('div');foot.className='mf';const ok=document.createElement('button');ok.className='btn primary';ok.textContent=L('Продолжить','Continue');const cancel=document.createElement('button');cancel.className='btn';cancel.textContent=L('Отмена','Cancel');const done=value=>{overlay.remove();resolve(value);};ok.onclick=()=>done(true);cancel.onclick=()=>done(false);overlay.onclick=e=>{if(e.target===overlay)done(false);};foot.append(ok,cancel);modal.append(body,foot);overlay.appendChild(modal);document.body.appendChild(overlay);cancel.focus();});}
document.getElementById('openDataDir').onclick=()=>window.tm?.openDataDir().catch(error=>showToast(error.message||String(error)));document.getElementById('changeDataDir').onclick=async()=>{try{const current=document.querySelector('#set-storage .d.mono').textContent,chosen=await window.tm.chooseDataDir(current);if(chosen){await window.tm.moveStorage(chosen);showToast(L('Данные перенесены, новый путь уже используется.','Data moved, the new path is now in use.'));document.querySelector('#set-storage .d.mono').textContent=chosen;}}catch(error){showToast(error.message||String(error));}};document.querySelectorAll('[data-clear]').forEach(button=>button.onclick=async()=>{if(!await confirmAction(L('Очистить выбранные локальные данные? Данные на сервере не удаляются.','Clear the selected local data? Data on the server is not deleted.')))return;try{await window.tm.clearLocalData(button.dataset.clear);await window.reloadCoreData();showToast(L('Локальные данные очищены','Local data cleared'));}catch(error){showToast(error.message||String(error));}});
document.getElementById('exportKeyBackup').onclick=async()=>{
  const passwordInput=document.getElementById('keyBackupPassword'),confirmInput=document.getElementById('keyBackupPasswordConfirm'),status=document.getElementById('keyBackupStatus'),button=document.getElementById('exportKeyBackup');
  const password=passwordInput.value;
  if(password.length<12){status.textContent=L('Пароль должен содержать не менее 12 символов.','The password must contain at least 12 characters.');status.dataset.kind='error';return;}
  if(password!==confirmInput.value){status.textContent=L('Пароли не совпадают.','Passwords do not match.');status.dataset.kind='error';return;}
  try{
    const path=await window.tm.saveFileDialog('truemail-keys.tmkeys');if(!path)return;
    button.disabled=true;status.textContent=L('Шифрую backup ключей…','Encrypting key backup…');status.dataset.kind='';
    await window.tm.exportKeyBackup(path,password);status.textContent=L('Backup ключей сохранён. Храните файл и пароль отдельно.','Key backup saved. Keep the file and password separately.');status.dataset.kind='success';
  }catch(error){status.textContent=error.message||String(error);status.dataset.kind='error';}
  finally{passwordInput.value='';confirmInput.value='';button.disabled=false;}
};

let tooltipPortal=null;document.addEventListener('mouseover',e=>{const help=e.target.closest('.help[data-tip]');if(!help)return;tooltipPortal=document.createElement('div');tooltipPortal.className='help-portal';tooltipPortal.textContent=help.dataset.tip;document.body.appendChild(tooltipPortal);const rect=help.getBoundingClientRect(),box=tooltipPortal.getBoundingClientRect();let left=Math.max(12,Math.min(window.innerWidth-box.width-12,rect.left+rect.width/2-box.width/2)),top=rect.top-box.height-10;if(top<12)top=rect.bottom+10;tooltipPortal.style.left=`${left}px`;tooltipPortal.style.top=`${top}px`;});document.addEventListener('mouseout',e=>{if(e.target.closest('.help[data-tip]')){tooltipPortal?.remove();tooltipPortal=null;}});

document.getElementById('expertToggle').addEventListener('click',()=>{
  const enabled=document.getElementById('expertToggle').classList.contains('on');
  root.setAttribute('data-mode',enabled?'expert':'simple');
  window.tm?.setSetting('expert_mode',enabled?'1':'0').catch(console.error);});

// Автозапуск при старте системы. Глобальный обработчик (.toggle) уже переключил
// класс к моменту вызова, поэтому читаем итоговое состояние.
const previewLinesSel=document.getElementById('previewLines');
if(previewLinesSel){previewLinesSel.onchange=()=>{const n=previewLinesSel.value;document.documentElement.style.setProperty('--preview-lines',n);window.tm?.setSetting('preview_lines',n).catch(console.error);};}
const autostartToggle=document.getElementById('autostartToggle');
if(autostartToggle){
  window.tm?.getAutostart?.().then(on=>autostartToggle.classList.toggle('on',!!on)).catch(()=>{});
  autostartToggle.addEventListener('click',async()=>{
    const enabled=autostartToggle.classList.contains('on');
    try{await window.tm.setAutostart(enabled);}
    catch(error){autostartToggle.classList.toggle('on');showToast(error.message||String(error));}
  });
}
const conversationsToggle=document.getElementById('conversationsToggle');
if(conversationsToggle){
  conversationsToggle.addEventListener('click',()=>{
    conversationsEnabled=conversationsToggle.classList.contains('on');
    if(!conversationsEnabled)expandedConversations.clear();
    window.tm?.setSetting('show_conversations',conversationsEnabled?'1':'0').catch(console.error);
    if(lastListRows.length)renderMessageList(lastListRows,lastListTitle);
  });
}

const notifyPositionSelect=document.getElementById('notifyPosition');
if(notifyPositionSelect)notifyPositionSelect.onchange=e=>{window.tm?.setNotifyPosition(e.target.value).catch(console.error);};

window.applyCoreSettings=function(settings){
  // Без сохранённого значения показываем платформенный дефолт (как в NotifyAnchor).
  if(notifyPositionSelect)notifyPositionSelect.value=settings.notify_position||(/mac/i.test(navigator.platform)?'top-right':'bottom-right');
  if(settings.theme)setTheme(settings.theme,false);
  if(settings.density)setDensity(settings.density,false);
  if(settings.accent)setAccent(settings.accent,false);
  const expert=settings.expert_mode==='1';
  document.getElementById('expertToggle').classList.toggle('on',expert);
  root.setAttribute('data-mode',expert?'expert':'simple');
  if(settings.preview_lines){document.documentElement.style.setProperty('--preview-lines',settings.preview_lines);const sel=document.getElementById('previewLines');if(sel)sel.value=settings.preview_lines;}
  if(settings.contacts_view==='table'){document.getElementById('cgrid')?.classList.add('table-view');const sw=document.getElementById('contactViewSwitch');if(sw)sw.querySelectorAll('button').forEach(b=>b.classList.toggle('on',b.dataset.cview==='table'));}
  conversationsEnabled=settings.show_conversations==='1';const convToggle=document.getElementById('conversationsToggle');if(convToggle)convToggle.classList.toggle('on',conversationsEnabled);
  if(settings.sidebar_width)setSidebarWidth(settings.sidebar_width,false);
  if(settings.list_width)setListWidth(settings.list_width,false);
  if(settings.ui_scale)setUiScale(settings.ui_scale,false);
  if(settings.toolbar_layout){try{const state=JSON.parse(settings.toolbar_layout);state.actions?.forEach(action=>{const row=tbList.querySelector(`[data-action="${action.key}"]`);if(row){row.classList.toggle('off',!action.visible);row.querySelector('.toggle').classList.toggle('on',action.visible);row.dataset.labels=action.labels||state.labels||'text';row.querySelector('.action-label-mode').textContent=row.dataset.labels==='icons'?'Только значок':'Значок + текст';tbList.appendChild(row);}});document.querySelectorAll('#toolbarAlign button').forEach(b=>b.classList.toggle('on',b.dataset.align===state.align));applyToolbar();}catch(e){console.error(e);}}
  if(settings.smart_folders_ui){try{const saved=JSON.parse(settings.smart_folders_ui);smartFolders.splice(0,smartFolders.length,...normalizedSmartFolders(saved));renderSmartManagement();bindSmartNavigation();persistSmartFolders().then(()=>window.tm.setSetting('smart_folders_ui','')).catch(console.error);}catch(e){console.error(e);}}
  const legacyUnified=Object.entries(settings).filter(([key,value])=>key.startsWith('unified_')&&/^\d+$/.test(key.slice(8))&&(value==='0'||value==='1'));if(legacyUnified.length){window.coreUnifiedSettings=window.coreUnifiedSettings||{};Promise.all(legacyUnified.map(([key,value])=>{const folderId=Number(key.slice(8)),included=value!=='0';window.coreUnifiedSettings[folderId]=included?'1':'0';return window.tm.setUnifiedSource(folderId,included).then(()=>window.tm.setSetting(key,''));})).then(()=>{if(currentSmartIndex!==null)filterSmart(currentSmartIndex);}).catch(console.error);}
  if(settings.composer_draft){try{window.pendingComposerDraft=JSON.parse(settings.composer_draft);}catch(e){console.error(e);}}
  if(settings.search_history){try{const saved=JSON.parse(settings.search_history);if(Array.isArray(saved))searchHistory=saved.filter(value=>typeof value==='string').slice(0,10);}catch(e){console.error(e);}}
};
