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

/* ---------- routing between top views ---------- */
function showView(id){ document.querySelectorAll('.view').forEach(v=>v.classList.toggle('active',v.id===id));
  if(id==='composeView'){const m=document.getElementById('compEdit');if(m){m.focus();}} }
document.getElementById('toSettings').onclick=()=>{showView('settingsView');};
document.getElementById('backToMail').onclick=()=>showView('mailView');
document.getElementById('composeBtn').onclick=()=>{resetComposer();document.getElementById('compTitle').textContent='Новое письмо';showView('composeView');};
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
let coreContacts=[];
let coreCalendarData={calendars:[],events:[]};
let currentFolderId=null;
let currentSmartIndex=0;
let currentMessageRows=[];
let activeMessage=null;
let activeFullMessage=null;
const MESSAGE_PAGE_SIZE=100;
const folderHasMore=new Map();
let loadingMoreMessages=false;
const selectedMessageIds=new Set();
let lastSelectedMessageIndex=-1;
function renderIcons(root){root.querySelectorAll('[data-i]').forEach(e=>{const s=ic[e.dataset.i];if(s)e.innerHTML=s;});}

const msgsEl=document.getElementById('msgs');
async function loadNextMessagePage(){
  if(currentFolderId===null||loadingMoreMessages||folderHasMore.get(currentFolderId)===false)return;
  const loaded=messages.filter(message=>message.folder_id===currentFolderId).sort((a,b)=>String(b.date||'').localeCompare(String(a.date||''))||b.id-a.id);
  const cursor=loaded.at(-1);if(!cursor)return;
  loadingMoreMessages=true;
  try{
    const page=await window.tm?.listMessagesPage(currentFolderId,cursor.date||'',cursor.id,MESSAGE_PAGE_SIZE)||[];
    const known=new Set(messages.map(message=>message.id));messages.push(...page.filter(message=>!known.has(message.id)));
    folderHasMore.set(currentFolderId,page.length===MESSAGE_PAGE_SIZE);
    renderMessageList(messages.filter(message=>message.folder_id===currentFolderId).sort((a,b)=>String(b.date||'').localeCompare(String(a.date||''))||b.id-a.id),coreFolders.find(folder=>folder.id===currentFolderId)?.display_name);
  }catch(error){console.error('truemail pagination:',error);}finally{loadingMoreMessages=false;}
}
msgsEl.addEventListener('scroll',()=>{if(msgsEl.scrollTop+msgsEl.clientHeight>=msgsEl.scrollHeight-240)loadNextMessagePage();},{passive:true});

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
    if(current){cell.onclick=()=>{calendarCursor=new Date(date);};displayEvents.filter(event=>parseDavDate(event.dtstart)?.toDateString()===date.toDateString()).forEach((event,index)=>{visibleEvents++;const item=document.createElement('div');item.className=`ev ev-c${index%4}`;item.textContent=event.summary;cell.appendChild(item);});}cg.appendChild(cell);}
  const info=document.getElementById('calSyncInfo');if(info){const dated=events.map(event=>({date:parseDavDate(event.dtstart)})).filter(item=>item.date).sort((a,b)=>b.date-a.date),latest=dated[0];info.textContent=`${coreCalendarData.calendars?.length||0} календаря · ${events.length} событий${visibleEvents?'':latest?' · показать последние':' · событий нет'}`;info.classList.toggle('clickable',!visibleEvents&&Boolean(latest));info.onclick=!visibleEvents&&latest?()=>{calendarCursor=new Date(latest.date.getFullYear(),latest.date.getMonth(),1);renderCalendarData();}:null;info.title=!visibleEvents&&latest?`Перейти к ${localeName(latest.date,{month:'long',year:'numeric'})}`:'';}
  renderWeekDay(events);
  const count=document.querySelector('[data-nav="calendar"] .count');if(count)count.textContent=events.length||'';
}
function renderWeekDay(events){const base=new Date(calendarCursor);const monday=new Date(base);monday.setDate(base.getDate()-((base.getDay()+6)%7));const expanded=expandCalendarEvents(events,new Date(monday.getFullYear(),monday.getMonth(),monday.getDate()-1),new Date(monday.getFullYear(),monday.getMonth(),monday.getDate()+9));
  const wk=document.getElementById('calweek');let h='<div class="wk-corner"></div>';for(let i=0;i<7;i++){const d=new Date(monday);d.setDate(monday.getDate()+i);const wd=localeName(d,{weekday:wizardLocale==='ru'?'short':'short'}).replace('.','');h+=`<div class="wk-dayhd">${wizardLocale==='ru'?wd.slice(0,2):wd.slice(0,3)}<b>${d.getDate()}</b></div>`;}for(let hr=0;hr<24;hr++){h+=`<div class="wk-time">${String(hr).padStart(2,'0')}:00</div>`;for(let d=0;d<7;d++){const date=new Date(monday);date.setDate(monday.getDate()+d);const evs=expanded.filter(e=>{const x=parseDavDate(e.dtstart);return x&&x.toDateString()===date.toDateString()&&x.getHours()===hr;});h+=`<div class="wk-cell">${evs.map(e=>`<div class="wk-ev">${escapeHtml(e.summary)}</div>`).join('')}</div>`;}}wk.innerHTML=h;
  const dv=document.getElementById('calday');let dh='';for(let hr=0;hr<24;hr++){const evs=expanded.filter(e=>{const x=parseDavDate(e.dtstart);return x&&x.toDateString()===base.toDateString()&&x.getHours()===hr;});dh+=`<div class="wk-time">${String(hr).padStart(2,'0')}:00</div><div class="wk-cell">${evs.map(e=>`<div class="wk-ev dayev">${escapeHtml(e.summary)}</div>`).join('')}</div>`;}dv.innerHTML=dh;
}
function escapeHtml(value){return String(value||'').replace(/[&<>"']/g,ch=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[ch]));}

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
document.querySelectorAll('.acc-h').forEach(h=>h.onclick=()=>{h.classList.toggle('open');h.nextElementSibling.classList.toggle('open');});

/* collapsible sidebar groups */
document.querySelectorAll('.nav .navlabel').forEach(lbl=>{
  lbl.classList.add('clp');
  const chev=document.createElement('span');chev.className='clp-chev';chev.innerHTML=ic.down;lbl.insertBefore(chev,lbl.firstChild);
  lbl.addEventListener('click',e=>{ if(e.target.closest('.add'))return;
    lbl.classList.toggle('collapsed');const hide=lbl.classList.contains('collapsed');let el=lbl.nextElementSibling;
    while(el&&!el.classList.contains('navlabel')){ if(el.classList.contains('navitem')||el.classList.contains('acc-h')||el.classList.contains('acc-sub'))el.classList.toggle('grouphide',hide); el=el.nextElementSibling; } });
});

/* custom right-click menu (suppress browser default) */
const ctxmenu=document.getElementById('ctxmenu'),ctxsmart=document.getElementById('ctxsmart');
function posMenu(menu,e){menu.style.left=Math.min(e.clientX,window.innerWidth-244)+'px';menu.style.top=Math.min(e.clientY,window.innerHeight-330)+'px';menu.classList.add('open');}
document.addEventListener('contextmenu',e=>{if(e.target.closest('input,textarea,select,[contenteditable="true"]'))return;e.preventDefault();
  ctxmenu.classList.remove('open');ctxsmart.classList.remove('open');
  const msg=e.target.closest('.msg'),smart=e.target.closest('[data-smart-index]');
  if(msg){const id=Number(msg.dataset.messageId);activeMessage=messages.find(item=>item.id===id)||activeMessage;posMenu(ctxmenu,e);}else if(smart){ctxsmart.dataset.index=smart.dataset.smartIndex;posMenu(ctxsmart,e);} });
document.addEventListener('click',()=>{ctxmenu.classList.remove('open');ctxsmart.classList.remove('open');});
[ctxmenu,ctxsmart].forEach(m=>m.querySelectorAll('.tmi').forEach(i=>i.onclick=()=>m.classList.remove('open')));
ctxmenu.querySelectorAll('[data-context-action]').forEach(item=>item.addEventListener('click',()=>{const action=item.dataset.contextAction;if(['reply','forward'].includes(action))openComposerForMessage(action);else performMessageAction(action);}));

/* theme / settings popover */
const root=document.documentElement,pop=document.getElementById('pop');
document.getElementById('gear').onclick=(e)=>{e.stopPropagation();pop.classList.toggle('open');};
document.addEventListener('click',e=>{if(!pop.contains(e.target)&&e.target.closest('#gear')===null)pop.classList.remove('open');});
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
document.getElementById('themeBtn').onclick=()=>setTheme(root.getAttribute('data-theme')==='dark'?'light':'dark');

/* command palette */
const overlay=document.getElementById('overlay'),cmdInput=document.getElementById('cmdInput'),cmdlist=document.getElementById('cmdlist');
// раскладко-независимый поиск: as<->фы, ntv<->тем, ыуе<->set
const RU="йцукенгшщзхъфывапролджэячсмитьбю",EN="qwertyuiop[]asdfghjkl;'zxcvbnm,.";
function conv(s,a,b){return s.split('').map(c=>{const i=a.indexOf(c);return i>=0?b[i]:c;}).join('');}
function matchQ(text,q){text=(text||'').toLowerCase();q=q.toLowerCase();return text.includes(q)||text.includes(conv(q,RU,EN))||text.includes(conv(q,EN,RU));}
function goCal(){document.querySelectorAll('.navitem').forEach(x=>x.classList.remove('active'));const a=document.getElementById('app');a.classList.remove('contactsmode');a.classList.add('calmode');showView('mailView');}
function goContacts(){document.querySelectorAll('.navitem').forEach(x=>x.classList.remove('active'));const a=document.getElementById('app');a.classList.remove('calmode');a.classList.add('contactsmode');showView('mailView');}
function goMail(){const a=document.getElementById('app');a.classList.remove('calmode','contactsmode');showView('mailView');}
const S2=(id)=>()=>{showView('settingsView');setSection(id);};
const staticCmds=[
  {g:'Действия',i:'compose',t:'Написать новое письмо',k:['C'],a:()=>document.getElementById('composeBtn').click()},
  {g:'Действия',i:'reply',t:'Ответить',k:['R'],a:()=>openComposerForMessage('reply')},{g:'Действия',i:'replyall',t:'Ответить всем',k:['A'],a:()=>openComposerForMessage('replyall')},{g:'Действия',i:'forward',t:'Переслать',k:['F'],a:()=>openComposerForMessage('forward')},
  {g:'Действия',i:'archive',t:'В архив',k:['E'],a:()=>performMessageAction('archive')},{g:'Действия',i:'trash',t:'Удалить',k:['Del'],a:()=>performMessageAction('trash')},
  {g:'Переход',i:'inbox',t:'Все входящие',a:goMail},{g:'Переход',i:'cal',t:'Календарь',a:goCal},{g:'Переход',i:'people',t:'Контакты',a:goContacts},
  {g:'Переход',i:'cal',t:'Сегодня',a:goMail},{g:'Переход',i:'search',t:'Непрочитанные (все)',a:goMail},{g:'Переход',i:'paperclip',t:'С вложениями',a:goMail},
  {g:'Настройки',i:'settings',t:'Общие',a:S2('general')},{g:'Настройки',i:'settings',t:'Режим эксперта',a:S2('general')},
  {g:'Настройки',i:'grip',t:'Панель письма',a:S2('toolbar')},{g:'Настройки',i:'user',t:'Аккаунты',a:S2('accounts')},{g:'Настройки',i:'user',t:'Добавить аккаунт',a:showAccountWizard},
  {g:'Настройки',i:'folder',t:'Сопоставление папок',a:S2('folders')},{g:'Настройки',i:'cal',t:'Календари',a:S2('calendars')},{g:'Настройки',i:'storage',t:'Хранилище',a:S2('storage')},
  {g:'Настройки',i:'palette',t:'Темы и оформление',a:S2('themes')},{g:'Настройки',i:'shield',t:'Приватность',a:S2('privacy')},{g:'Настройки',i:'keyboard',t:'Горячие клавиши',a:S2('keys')},
  {g:'Настройки',i:'sun',t:'Переключить тему',a:()=>setTheme(root.getAttribute('data-theme')==='dark'?'light':'dark')},
];
let sel=0,currentCommands=[],searchHistory=[];
function searchTerms(q){return q.split(/\s+/).filter(token=>token&&!/^from:/i.test(token)&&!/^has:attachments?$/i.test(token)).join(' ').trim();}
function highlightMatch(value,q){const text=String(value||''),needle=searchTerms(q);if(!needle)return escapeHtml(text);const candidates=[needle,conv(needle.toLocaleLowerCase(),RU,EN),conv(needle.toLocaleLowerCase(),EN,RU)];let found=-1,length=0;for(const candidate of candidates){const index=text.toLocaleLowerCase().indexOf(candidate);if(index>=0){found=index;length=candidate.length;break;}}return found<0?escapeHtml(text):`${escapeHtml(text.slice(0,found))}<mark>${escapeHtml(text.slice(found,found+length))}</mark>${escapeHtml(text.slice(found+length))}`;}
function buildResults(q,coreResults=[]){const base=[...staticCmds];
  if(!q.trim())searchHistory.forEach(value=>base.unshift({g:wizardLocale==='en'?'Recent searches':'Недавние запросы',i:'search',t:value,a:()=>{openCmd();cmdInput.value=value;cmdInput.dispatchEvent(new Event('input'));}}));
  if(q.trim()){
    coreResults.forEach(m=>base.push({g:'Письма',i:'inbox',t:m.subject||'(без темы)',sub:(m.from?.name||m.from?.email||'')+' · '+(m.preview||'').slice(0,80),searchHit:true,a:()=>{goMail();showMessage(m);}}));
    coreContacts.forEach(c=>base.push({g:'Контакты',i:'people',t:c.display_name,sub:c.emails?.[0]?.email||'',a:goContacts}));
  }
  const terms=searchTerms(q);return q.trim()?base.filter(c=>c.searchHit||(terms&&matchQ(c.t+' '+(c.sub||'')+' '+c.g,terms))):base;}
function renderCmd(q='',coreResults=[]){const f=buildResults(q,coreResults);currentCommands=f;sel=0;let html='',lg='';
  f.forEach((c,idx)=>{if(c.g!==lg){html+=`<div class="cmdgrp">${escapeHtml(c.g)}</div>`;lg=c.g;}const icon=Object.hasOwn(ic,c.i)?c.i:'inbox';
    html+=`<div class="cmdrow${idx===0?' sel':''}" data-idx="${idx}"><i data-i="${icon}"></i>${highlightMatch(c.t,q)}${c.sub?`<span class="csub">${highlightMatch(c.sub,q)}</span>`:''}<span class="ck">${(c.k||[]).map(k=>`<span class="kbd">${escapeHtml(k)}</span>`).join('')}</span></div>`;});
  cmdlist.innerHTML=html||`<div class="cmdgrp">Ничего не найдено</div>`;renderIcons(cmdlist);
  cmdlist.querySelectorAll('.cmdrow').forEach(r=>r.onclick=()=>{const c=f[+r.dataset.idx];closeCmd();if(c&&c.a)c.a();});}
function openCmd(){overlay.classList.add('open');cmdInput.value='';renderCmd();cmdInput.focus();}
function closeCmd(){overlay.classList.remove('open');}
document.getElementById('searchBox').onclick=openCmd;
let searchSerial=0;
cmdInput.oninput=async()=>{const q=cmdInput.value,serial=++searchSerial;if(!q.trim()){renderCmd();return;}renderCmd(q,[]);try{const found=await window.tm?.search(q)||[];if(serial===searchSerial){renderCmd(q,found);if(searchTerms(q).length>=2){searchHistory=[q,...searchHistory.filter(item=>item!==q)].slice(0,10);window.tm?.setSetting('search_history',JSON.stringify(searchHistory)).catch(console.error);}}}catch(e){console.error('search',e);}};
overlay.onclick=e=>{if(e.target===overlay)closeCmd();};
document.addEventListener('keydown',e=>{
  if(overlay.classList.contains('open')&&['ArrowDown','ArrowUp','Enter'].includes(e.key)){e.preventDefault();const rows=[...cmdlist.querySelectorAll('.cmdrow')];if(e.key==='Enter'){const command=currentCommands[sel];closeCmd();command?.a?.();return;}sel=e.key==='ArrowDown'?Math.min(rows.length-1,sel+1):Math.max(0,sel-1);rows.forEach((row,index)=>row.classList.toggle('sel',index===sel));rows[sel]?.scrollIntoView({block:'nearest'});return;}
  if(e.ctrlKey&&e.shiftKey&&['KeyC','KeyF','KeyM'].includes(e.code)){e.preventDefault();e.stopPropagation();if(e.code==='KeyC')document.getElementById('composeBtn').click();if(e.code==='KeyF')openCmd();return;}
  if((e.ctrlKey||e.metaKey)&&e.code==='KeyK'){e.preventDefault();overlay.classList.contains('open')?closeCmd():openCmd();}
  const target=e.target;if(!e.ctrlKey&&!e.metaKey&&!e.altKey&&!overlay.classList.contains('open')&&!target.matches('input,textarea,select,[contenteditable="true"]')){
    const actions={KeyC:()=>document.getElementById('composeBtn').click(),KeyR:()=>openComposerForMessage('reply'),KeyA:()=>openComposerForMessage('replyall'),KeyF:()=>openComposerForMessage('forward'),KeyE:()=>performMessageAction('archive'),KeyU:()=>activeMessage&&window.tm?.markSeen(activeMessage.id,false).then(()=>window.reloadCoreData()),Delete:()=>performMessageAction('trash')};
    if(actions[e.code]){e.preventDefault();actions[e.code]();}
    if(e.code==='KeyJ'||e.code==='KeyK'){e.preventDefault();const rows=[...document.querySelectorAll('.msg')],active=rows.findIndex(row=>row.classList.contains('active')),next=e.code==='KeyJ'?Math.min(rows.length-1,active+1):Math.max(0,active-1);rows[next]?.click();rows[next]?.scrollIntoView({block:'nearest'});}
  }
  if(e.key==='Escape'){closeCmd();pop.classList.remove('open');closeSmart();ctxmenu.classList.remove('open');ctxsmart.classList.remove('open');filterMenu?.classList.add('hidden');sortMenu?.classList.add('hidden');}});

/* Keyboard and screen-reader semantics for code-generated controls. */
function enhanceAccessibility(scope=document){scope.querySelectorAll('.navitem,.setnav .sec,.acc-h,.tmi,.ccard,.swatch,.wtheme,.wlang').forEach(element=>{if(!element.hasAttribute('role'))element.setAttribute('role','button');if(!element.hasAttribute('tabindex'))element.tabIndex=0;});scope.querySelectorAll('.toggle').forEach(toggle=>{toggle.setAttribute('role','switch');toggle.tabIndex=0;toggle.setAttribute('aria-checked',String(toggle.classList.contains('on')));});scope.querySelectorAll('.help[data-tip]').forEach(help=>{help.tabIndex=0;help.setAttribute('role','note');help.setAttribute('aria-label',help.dataset.tip);});}
enhanceAccessibility();
document.addEventListener('keydown',event=>{if((event.key==='Enter'||event.key===' ')&&event.target.matches('[role="button"],[role="switch"]')){event.preventDefault();event.target.click();}});
const accessibilityObserver=new MutationObserver(records=>{for(const record of records){if(record.type==='childList')record.addedNodes.forEach(node=>{if(node.nodeType===1)enhanceAccessibility(node);});else if(record.target.matches?.('.toggle'))record.target.setAttribute('aria-checked',String(record.target.classList.contains('on')));}});accessibilityObserver.observe(document.body,{subtree:true,childList:true,attributes:true,attributeFilter:['class']});
document.querySelectorAll('.toggle').forEach(t=>t.onclick=()=>t.classList.toggle('on'));

/* calendar view switch + week/day render */
const calSection=document.getElementById('calSection');
document.querySelectorAll('#calViews button').forEach(b=>b.onclick=()=>{
  document.querySelectorAll('#calViews button').forEach(x=>x.classList.toggle('on',x===b));
  calSection.dataset.cv=b.dataset.cv;
  if(b.dataset.cv==='month')renderCalendarData();else {renderWeekDay(coreCalendarData.events||[]);const wd=localeName(calendarCursor,{weekday:'short'}).replace('.','');document.getElementById('calTitle').textContent=b.dataset.cv==='week'?localeName(calendarCursor,{month:'long',year:'numeric'}):`${wizardLocale==='ru'?wd.slice(0,2):wd.slice(0,3)}, ${localeName(calendarCursor,{day:'numeric',month:'long'})}`;}});
document.querySelectorAll('.calhead > .iconbtn').forEach((button,index)=>button.onclick=()=>{const direction=index===0?-1:1,view=calSection.dataset.cv||'month';if(view==='day')calendarCursor.setDate(calendarCursor.getDate()+direction);else if(view==='week')calendarCursor.setDate(calendarCursor.getDate()+7*direction);else {const day=calendarCursor.getDate();calendarCursor.setDate(1);calendarCursor.setMonth(calendarCursor.getMonth()+direction);calendarCursor.setDate(Math.min(day,new Date(calendarCursor.getFullYear(),calendarCursor.getMonth()+1,0).getDate()));}renderCalendarData();if(view!=='month')document.querySelector(`#calViews button[data-cv="${view}"]`)?.click();});

/* smart folder modal */
const smartOverlay=document.getElementById('smartOverlay');
const smartFields=[['sender','Отправитель','Sender'],['recipient','Получатель','Recipient'],['subject','Тема','Subject'],['body','Текст письма','Message text'],['account','Аккаунт','Account'],['status','Статус','Status'],['attachment','Вложение','Attachment'],['label','Метка','Label'],['folder','Папка','Folder'],['date','Дата','Date']];
const smartOps=[['contains','содержит','contains'],['not_contains','не содержит','does not contain'],['equals','равно','equals']];
const legacySmartFields=Object.fromEntries(smartFields.flatMap(([id,ru,en])=>[[id,id],[ru,id],[en,id]]));
const legacySmartOps=Object.fromEntries(smartOps.flatMap(([id,ru,en])=>[[id,id],[ru,id],[en,id]]));
function smartLabel(item){return item[wizardLocale==='en'?2:1];}
function condRow(f='sender',o='contains',v=''){const r=document.createElement('div');r.className='cond';
  f=legacySmartFields[f]||'sender';o=legacySmartOps[o]||'contains';
  r.innerHTML=`<select>${smartFields.map(x=>`<option value="${x[0]}">${escapeHtml(smartLabel(x))}</option>`).join('')}</select><select>${smartOps.map(x=>`<option value="${x[0]}">${escapeHtml(smartLabel(x))}</option>`).join('')}</select><input placeholder="${wizardLocale==='en'?'value':'значение'}"><span class="del"><i data-i="trash"></i></span>`;
  const selects=r.querySelectorAll('select');selects[0].value=f;selects[1].value=o;r.querySelector('input').value=String(v||'');
  r.querySelector('.del').onclick=()=>r.remove();renderIcons(r);return r;}
let editingSmartIndex=null,selectedSmartIcon='star';
function conditionGroup(conditions=[{}]){const group=document.createElement('div');group.className='cond-group';const head=document.createElement('div');head.className='cond-group-head';head.textContent='Все условия в группе (И)';group.appendChild(head);conditions.forEach(c=>group.appendChild(condRow(c.f,c.o,c.v)));return group;}
function openSmart(index=null){editingSmartIndex=index;const c=document.getElementById('conds');c.innerHTML='';const item=index===null?null:smartFolders[index];document.querySelector('#smartOverlay .mh h3').textContent=item?'Изменить умную папку':'Новая умная папка';document.getElementById('smartCreate').lastChild.textContent=item?' Сохранить':' Создать умную папку';document.getElementById('smartName').value=item?.t||'';selectedSmartIcon=item?.i||'star';updateSmartIconButton();c.appendChild(conditionGroup(item?.groups?.[0]||[{}]));(item?.groups||[]).slice(1).forEach(group=>c.appendChild(conditionGroup(group)));smartOverlay.classList.add('open');}
function closeSmart(){smartOverlay.classList.remove('open');}
document.getElementById('addSmart').onclick=(e)=>{e.stopPropagation();openSmart();};
document.getElementById('addCond').onclick=()=>document.querySelector('#conds .cond-group:last-child')?.appendChild(condRow());
document.getElementById('addCondGroup').onclick=()=>document.getElementById('conds').appendChild(conditionGroup());
document.getElementById('smartClose').onclick=closeSmart;
document.getElementById('smartCancel').onclick=closeSmart;
document.getElementById('smartCreate').onclick=()=>{const name=document.getElementById('smartName').value.trim();if(!name)return;const groups=[...document.querySelectorAll('#conds .cond-group')].map(group=>[...group.querySelectorAll('.cond')].map(row=>{const selects=row.querySelectorAll('select');return {f:selects[0].value,o:selects[1].value,v:row.querySelector('input').value};}));const item={i:selectedSmartIcon,t:name,on:true,groups};if(editingSmartIndex===null)smartFolders.push(item);else smartFolders[editingSmartIndex]=Object.assign(smartFolders[editingSmartIndex],item);renderSmartManagement();bindSmartNavigation();window.tm?.setSetting('smart_folders_ui',JSON.stringify(smartFolders)).catch(console.error);closeSmart();};
smartOverlay.onclick=e=>{if(e.target===smartOverlay)closeSmart();};
document.querySelectorAll('#smartLogic button').forEach(b=>b.onclick=()=>document.querySelectorAll('#smartLogic button').forEach(x=>x.classList.toggle('on',x===b)));
const smartIconKeys=Object.keys(ic).filter(key=>!['chevR','chevL','up','down','back','dots','grip'].includes(key)).slice(0,50);
const smartIconsEl=document.getElementById('smartIcons');smartIconsEl.innerHTML=smartIconKeys.map(key=>`<span class="ic-pick" data-sel="${key}" title="${key}"><i data-i="${key}"></i></span>`).join('');renderIcons(smartIconsEl);
function updateSmartIconButton(){const i=document.querySelector('#smartIconButton i');i.dataset.i=selectedSmartIcon;i.innerHTML=ic[selectedSmartIcon]||ic.star;document.querySelectorAll('#smartIcons .ic-pick').forEach(p=>p.classList.toggle('on',p.dataset.sel===selectedSmartIcon));}
document.getElementById('smartIconButton').onclick=()=>smartIconsEl.classList.toggle('hidden');document.querySelectorAll('#smartIcons .ic-pick').forEach(p=>p.onclick=()=>{selectedSmartIcon=p.dataset.sel;updateSmartIconButton();smartIconsEl.classList.add('hidden');});

/* toolbar customizer */
const tbActions=[
  {k:'reply',t:'Ответить',on:true},{k:'replyall',t:'Ответить всем',on:true},{k:'forward',t:'Переслать',on:true},
  {k:'archive',t:'В архив',on:true},{k:'trash',t:'Удалить',on:true}];
const tbList=document.getElementById('tbList');
tbActions.forEach(a=>{const r=document.createElement('div');r.className='tbrow'+(a.on?'':' off');r.draggable=true;r.dataset.action=a.k;
  r.innerHTML=`<span class="grip"><i data-i="grip"></i></span><i data-i="${a.k}"></i><span class="nm">${a.t}</span>
    <span class="ord"><button class="iconbtn" data-dir="up"><i data-i="up"></i></button><button class="iconbtn" data-dir="down"><i data-i="down"></i></button></span>
    <div class="toggle${a.on?' on':''}"></div>`;
  renderIcons(r);
  const save=()=>{applyToolbar();persistToolbar();};
  r.querySelector('[data-dir="up"]').onclick=()=>{const p=r.previousElementSibling;if(p)tbList.insertBefore(r,p);save();};
  r.querySelector('[data-dir="down"]').onclick=()=>{const n=r.nextElementSibling;if(n)tbList.insertBefore(n,r);save();};
  r.querySelector('.toggle').onclick=(e)=>{e.stopPropagation();const t=e.currentTarget;t.classList.toggle('on');r.classList.toggle('off',!t.classList.contains('on'));save();};
  tbList.appendChild(r);});
let draggedToolbarRow=null;tbList.addEventListener('dragstart',e=>{draggedToolbarRow=e.target.closest('.tbrow');});tbList.addEventListener('dragover',e=>{e.preventDefault();const row=e.target.closest('.tbrow');if(row&&draggedToolbarRow&&row!==draggedToolbarRow){const rect=row.getBoundingClientRect();tbList.insertBefore(draggedToolbarRow,e.clientY<rect.top+rect.height/2?row:row.nextSibling);}});tbList.addEventListener('drop',()=>{applyToolbar();persistToolbar();});
function toolbarState(){return {actions:[...tbList.children].map(row=>({key:row.dataset.action,visible:!row.classList.contains('off')})),align:document.querySelector('#toolbarAlign .on')?.dataset.align||'left',labels:document.querySelector('#toolbarLabels .on')?.dataset.labels||'text'};}
function persistToolbar(){window.tm?.setSetting('toolbar_layout',JSON.stringify(toolbarState())).catch(console.error);}
function applyToolbar(){const state=toolbarState(),bar=document.querySelector('.thread .actions');if(!bar)return;bar.classList.toggle('toolbar-right',state.align==='right');bar.classList.toggle('toolbar-icons',state.labels==='icons');bar.querySelectorAll('[data-toolbar-generated]').forEach(el=>el.remove());const anchor=bar.querySelector('.sp');state.actions.filter(action=>action.visible).forEach(action=>{const meta=tbActions.find(a=>a.k===action.key);if(!meta)return;const button=document.createElement('button');button.className=action.key==='reply'?'btn primary':'btn';button.dataset.toolbarGenerated='1';button.dataset.act=action.key;button.title=meta.t;button.innerHTML=`<i data-i="${action.key}"></i><span>${meta.t}</span>`;renderIcons(button);bar.insertBefore(button,anchor);});bar.querySelectorAll(':scope > button:not([data-toolbar-generated])').forEach(button=>button.classList.add('toolbar-original-hidden'));}
document.querySelectorAll('#toolbarAlign button,#toolbarLabels button').forEach(button=>button.onclick=()=>{button.parentElement.querySelectorAll('button').forEach(x=>x.classList.toggle('on',x===button));applyToolbar();persistToolbar();});
applyToolbar();
document.querySelector('.thread .actions').addEventListener('click',e=>{const button=e.target.closest('[data-toolbar-generated]');if(!button)return;const action=button.dataset.act;if(['reply','replyall','forward'].includes(action))openComposerForMessage(action);else if(['archive','trash'].includes(action))performMessageAction(action);});

/* smart folders management list */
const smartFolders=[{i:'inbox',t:'Все входящие',on:true},{i:'star',t:'Все важные',on:true},{i:'send',t:'Все отправленные',on:true},{i:'draft',t:'Все черновики',on:true},{i:'cal',t:'Сегодня (за 24 часа)',on:true},{i:'search',t:'Непрочитанные (все)',on:true},{i:'paperclip',t:'С вложениями',on:true},{i:'flag',t:'Ждут ответа',on:true}];
const smartListEl=document.getElementById('smartList');
function renderSmartManagement(){smartListEl.innerHTML='';smartFolders.forEach((a,index)=>{const r=document.createElement('div');r.className='tbrow'+(a.on?'':' off');
  r.innerHTML=`<span class="grip"><i data-i="grip"></i></span><i data-i="${a.i}"></i><span class="nm">${a.t}</span><button class="btn sm edit-sf">Изменить</button><span class="ord"><button class="iconbtn" data-dir="up"><i data-i="up"></i></button><button class="iconbtn" data-dir="down"><i data-i="down"></i></button></span><div class="toggle${a.on?' on':''}"></div>`;
  renderIcons(r);
  r.querySelector('[data-dir="up"]').onclick=()=>{if(index>0){[smartFolders[index-1],smartFolders[index]]=[smartFolders[index],smartFolders[index-1]];renderSmartManagement();bindSmartNavigation();window.tm?.setSetting('smart_folders_ui',JSON.stringify(smartFolders)).catch(console.error);}};
  r.querySelector('[data-dir="down"]').onclick=()=>{if(index<smartFolders.length-1){[smartFolders[index+1],smartFolders[index]]=[smartFolders[index],smartFolders[index+1]];renderSmartManagement();bindSmartNavigation();window.tm?.setSetting('smart_folders_ui',JSON.stringify(smartFolders)).catch(console.error);}};
  r.querySelector('.edit-sf').onclick=()=>openSmart(index);
  r.querySelector('.toggle').onclick=(e)=>{e.stopPropagation();const t=e.currentTarget;t.classList.toggle('on');a.on=t.classList.contains('on');r.classList.toggle('off',!a.on);bindSmartNavigation();window.tm?.setSetting('smart_folders_ui',JSON.stringify(smartFolders)).catch(console.error);};
  smartListEl.appendChild(r);});}
renderSmartManagement();
document.getElementById('smartNew2').onclick=()=>openSmart();
function bindSmartNavigation(){document.querySelectorAll('.custom-smart').forEach(row=>row.remove());const nav=document.querySelector('.nav'),accountLabel=[...nav.querySelectorAll('.navlabel')].find(label=>label.textContent.includes('Аккаунты'));smartFolders.slice(8).forEach((folder,offset)=>{const row=document.createElement('div');row.className='navitem custom-smart';row.dataset.nav='mail';row.innerHTML=`<i data-i="${folder.i}"></i><span>${escapeHtml(folder.t)}</span>`;renderIcons(row);accountLabel.before(row);});document.querySelectorAll('.navitem[data-nav="mail"]').forEach((row,index)=>{if(!smartFolders[index])return;row.dataset.smartIndex=index;const label=row.querySelector('span:not(.count)');if(label)label.textContent=smartFolders[index].t;row.style.display=smartFolders[index].on?'':'none';row.onclick=()=>{goMail();document.querySelectorAll('.navitem').forEach(item=>item.classList.remove('active'));row.classList.add('active');filterSmart(index);};});}
bindSmartNavigation();
ctxsmart.querySelector('[data-smart-action="open"]').onclick=()=>filterSmart(+ctxsmart.dataset.index);
ctxsmart.querySelector('[data-smart-action="edit"]').onclick=()=>openSmart(+ctxsmart.dataset.index);
ctxsmart.querySelector('[data-smart-action="settings"]').onclick=()=>{showView('settingsView');setSection('smart');};

calSection.addEventListener('click',e=>{const event=e.target.closest('.ev,.wk-ev');if(event)showToast(event.textContent.trim());});

/* welcome wizard */
const wizardText={
  ru:{languageTitle:'Выберите язык',languageSub:'Его можно изменить позже в настройках.',next:'Далее',back:'Назад',welcome:'Добро пожаловать в truemail',welcomeSub:'Быстрый, удобный и красивый почтовый клиент. Вся почта хранится локально на вашем устройстве.',start:'Начать настройку',skip:'Пропустить',connectTitle:'Подключите Яндекс',connectSub:'Один безопасный вход подключит почту, календарь и контакты. Пароль вводить в truemail не нужно.',emailPlaceholder:'you@yandex.ru',connect:'Войти через Яндекс ID',codePlaceholder:'Код подтверждения',confirm:'Подтвердить подключение',openingYandex:'Открываю Яндекс ID в браузере…',enterCode:'После входа скопируйте сюда код подтверждения.',connecting:'Проверяю доступ и загружаю почту, календарь и контакты…',connected:'Яндекс подключён: почта, календарь и контакты готовы.',themeTitle:'Оформление',themeSub:'Тему, плотность и акцент можно поменять в любой момент.',themeLight:'Светлая',themeDefault:'По умолчанию',themeDark:'Тёмная',themeDarkSub:'Для тёмного окружения',themeSystem:'Системная',themeSystemSub:'Следовать за ОС',securityTitle:'Всё под защитой',securitySub:'Настраивать ничего не нужно — безопасные значения уже действуют.',securityLocal:'Вся почта хранится локально на устройстве',securityTokens:'OAuth-токены — в системном хранилище паролей',securityTracking:'Трекинг-пиксели и UTM-метки блокируются',done:'Всё готово!',openMail:'Открыть почту',invalidEmail:'Введите адрес Яндекс Почты.',oauthUnavailable:'OAuth-мост доступен только внутри приложения truemail.'},
  en:{languageTitle:'Choose your language',languageSub:'You can change it later in Settings.',next:'Continue',back:'Back',welcome:'Welcome to truemail',welcomeSub:'A fast, comfortable and beautiful email client. All your mail stays local on your device.',start:'Start setup',skip:'Skip',connectTitle:'Connect Yandex',connectSub:'One secure sign-in connects mail, calendar and contacts. You never enter your password in truemail.',emailPlaceholder:'you@yandex.com',connect:'Sign in with Yandex ID',codePlaceholder:'Confirmation code',confirm:'Confirm connection',openingYandex:'Opening Yandex ID in your browser…',enterCode:'After signing in, paste the confirmation code here.',connecting:'Checking access and loading mail, calendar and contacts…',connected:'Yandex is connected: mail, calendar and contacts are ready.',themeTitle:'Appearance',themeSub:'You can change the theme, density and accent at any time.',themeLight:'Light',themeDefault:'Default',themeDark:'Dark',themeDarkSub:'For dark environments',themeSystem:'System',themeSystemSub:'Follow the operating system',securityTitle:'Protected by default',securitySub:'Nothing to configure — secure defaults are already active.',securityLocal:'All mail is stored locally on this device',securityTokens:'OAuth tokens are kept in the system credential store',securityTracking:'Tracking pixels and UTM parameters are blocked',done:'All set!',openMail:'Open mail',invalidEmail:'Enter your Yandex Mail address.',oauthUnavailable:'The OAuth bridge is only available inside the truemail app.'}
};
Object.assign(wizardText.ru,{connectTitle:'Подключите почту',connectSub:'Введите любой адрес — truemail определит провайдера и выберет способ входа.',emailPlaceholder:'you@example.com',connect:'Подключить',invalidEmail:'Введите корректный адрес почты.',oauthUnavailable:'Подключение аккаунта работает в desktop-приложении.'});
Object.assign(wizardText.en,{connectTitle:'Connect your email',connectSub:'Enter any address — truemail will detect the provider and choose a sign-in method.',emailPlaceholder:'you@example.com',connect:'Connect',invalidEmail:'Enter a valid email address.',oauthUnavailable:'Account connection is available in the desktop app.'});
Object.assign(wizardText.ru,{codeExpired:'Код истёк или уже был использован. Нажмите «Подключить» и получите новый код.'});
Object.assign(wizardText.en,{codeExpired:'The code expired or was already used. Select Connect to get a new code.'});
Object.assign(wizardText.ru,{storageTitle:'Папка данных',storageSub:'Здесь будут храниться зашифрованная почта, календарь, контакты и индекс.',storagePath:'Путь хранения',chooseFolder:'Выбрать…',storageRequired:'Выберите папку данных.',keyTitle:'Создайте ключи шифрования',keySub:'Водите мышью внутри поля, пока шкала не заполнится. Движения используются один раз и не сохраняются.',keyMove:'Двигайте мышью здесь',createKeys:'Создать защищённое хранилище',creatingStorage:'Создаю ключи и зашифрованную базу…'});
Object.assign(wizardText.en,{storageTitle:'Data folder',storageSub:'Encrypted mail, calendars, contacts and the search index will be stored here.',storagePath:'Storage path',chooseFolder:'Choose…',storageRequired:'Choose a data folder.',keyTitle:'Create encryption keys',keySub:'Move the mouse inside the area until the bar is full. The movements are used once and are never stored.',keyMove:'Move the mouse here',createKeys:'Create encrypted storage',creatingStorage:'Creating keys and the encrypted database…'});
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
  document.querySelectorAll('[data-wlang]').forEach(el=>el.classList.toggle('sel',el.dataset.wlang===locale));
  document.getElementById('wzLanguageNext').disabled=false;
  const languageSetting=document.getElementById('languageSetting');if(languageSetting)languageSetting.value=locale;
  if(window.tm?.localizationCatalog)window.tm.localizationCatalog(locale).then(applyUiCatalog).catch(console.error);
  if(persist&&window.tmStorageReady){window.tm?.setSetting('locale',locale).catch(console.error);}
}
window.applyWizardLanguage=applyWizardLanguage;
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
function showAccountWizard(){document.querySelector('.settings').classList.add('account-wizard-mode');showView('settingsView');setSection('addacct');}
function closeAccountWizard(){document.querySelector('.settings').classList.remove('account-wizard-mode');setSection('accounts');}
window.showAccountWizard=showAccountWizard;
document.getElementById('addAcct').onclick=showAccountWizard;
document.getElementById('settingsAddAccount').onclick=showAccountWizard;
document.querySelector('[data-set="addacct"]').addEventListener('click',showAccountWizard);
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
function sortedFolders(folders){const order={inbox:0,sent:1,drafts:2,archive:3,spam:4,trash:5};return [...folders].sort((a,b)=>{const ar=order[a.role]??20,br=order[b.role]??20;if(ar!==br)return ar-br;return String(a.remote_path||a.display_name||'').localeCompare(String(b.remote_path||b.display_name||''),wizardLocale||'ru',{numeric:true,sensitivity:'base'});});}
function renderContacts(contacts=coreContacts){const query=(document.querySelector('.ct-search input')?.value||'').trim().toLocaleLowerCase(),filtered=contacts.filter(contact=>`${contact.display_name||''} ${(contact.emails||[]).map(item=>item.email).join(' ')}`.toLocaleLowerCase().includes(query)),grid=document.getElementById('cgrid');grid.innerHTML='';filtered.forEach((contact,index)=>{const card=document.createElement('div');card.className='ccard';card.innerHTML=`<span class="ava ava-c${index%8}"></span><div><div class="cn"></div><div class="ce"></div></div>`;card.querySelector('.ava').textContent=(contact.display_name||contact.emails?.[0]?.email||'?').split(/\s+/).map(word=>word[0]).join('').slice(0,2).toUpperCase();card.querySelector('.cn').textContent=contact.display_name||contact.emails?.[0]?.email||'';card.querySelector('.ce').textContent=contact.emails?.[0]?.email||'';grid.appendChild(card);});const count=document.querySelector('.ct-count');if(count)count.textContent=`${filtered.length}${query?` / ${contacts.length}`:''} ${wizardLocale==='en'?'contacts':'контактов'}`;}
document.querySelector('.ct-search input')?.addEventListener('input',()=>renderContacts());
async function renderHtmlMessage(container,html,sender){
  const trustKey=`remote_images_sender:${String(sender||'').trim().toLocaleLowerCase()}`;
  const allowRemote=Boolean(sender)&&await window.tm?.getSetting(trustKey).catch(()=>null)==='true';
  const parsed=new DOMParser().parseFromString(html,'text/html');
  parsed.querySelectorAll('script,iframe,object,embed,form,input,button,textarea,select,base,link,meta,audio,video').forEach(node=>node.remove());
  let blocked=false;
  parsed.querySelectorAll('style').forEach(node=>{node.textContent=node.textContent.replace(/url\(\s*(['"]?)https?:[^)]*\)/gi,'none');});
  parsed.querySelectorAll('*').forEach(node=>{[...node.attributes].forEach(attr=>{const name=attr.name.toLowerCase(),value=attr.value.trim();if(name.startsWith('on')||['srcdoc','formaction','integrity','nonce'].includes(name)||((name==='href'||name==='src'||name==='xlink:href')&&/^\s*(?:javascript|file|data:text\/html):/i.test(value)))node.removeAttribute(attr.name);else if(name==='style')node.setAttribute('style',value.replace(/url\(\s*(['"]?)https?:[^)]*\)/gi,'none'));});});
  parsed.querySelectorAll('a').forEach(link=>{link.target='_blank';link.rel='noopener noreferrer';try{const url=new URL(link.href);[...url.searchParams.keys()].filter(key=>key.toLowerCase().startsWith('utm_')||['fbclid','gclid'].includes(key.toLowerCase())).forEach(key=>url.searchParams.delete(key));link.href=url.toString();}catch(_){}});
  parsed.querySelectorAll('img,source').forEach(image=>{const src=image.getAttribute('src')||image.getAttribute('srcset')||'';if(/^https?:/i.test(src)&&!allowRemote){blocked=true;image.removeAttribute('src');image.removeAttribute('srcset');image.setAttribute('alt',image.getAttribute('alt')||'Удалённое изображение заблокировано');}image.setAttribute('loading','lazy');image.setAttribute('referrerpolicy','no-referrer');image.style.maxWidth='100%';image.style.height='auto';});
  container.classList.add('html');
  if(blocked){const notice=document.createElement('div');notice.className='blocked';const text=document.createElement('span');text.textContent='Удалённые изображения заблокированы для защиты от отслеживания.';const button=document.createElement('button');button.type='button';button.textContent=`Показывать от ${sender}`;button.onclick=async()=>{await window.tm?.setSetting(trustKey,'true');container.replaceChildren();await renderHtmlMessage(container,html,sender);};notice.append(text,button);container.appendChild(notice);}
  const frame=document.createElement('iframe');frame.className='mail-html-frame';frame.title='Содержимое HTML-письма';frame.setAttribute('sandbox','allow-same-origin allow-popups');const styles='<style>html,body{margin:0;padding:0;max-width:100%;overflow-wrap:anywhere;color:#17181c;font:14px/1.55 Arial,sans-serif}*{box-sizing:border-box}img,table{max-width:100%}a{color:#4b52c0}pre{white-space:pre-wrap}</style>';frame.srcdoc=`<!doctype html><html><head><meta charset="utf-8"><base target="_blank">${styles}${parsed.head.innerHTML}</head><body>${parsed.body.innerHTML}</body></html>`;frame.onload=()=>{try{frame.style.height=`${Math.max(120,frame.contentDocument.documentElement.scrollHeight+8)}px`;}catch(_){frame.style.height='480px';}};container.appendChild(frame);
}
function renderMessageList(rows,title){
  currentMessageRows=[...rows];const list=document.getElementById('msgs');list.innerHTML='';
  const heading=document.querySelector('.listhead h2');if(heading)heading.textContent=title||'Письма';
  rows.forEach(message=>{
    const row=document.createElement('div');row.className='msg'+(message.flags?.seen?'':' unread');row.dataset.messageId=message.id;
    const initial=(message.from?.name||message.from?.email||'?').trim()[0].toUpperCase();
    row.innerHTML=`<div class="avawrap"><span class="ava ava-c2"></span><input class="msg-check" type="checkbox" aria-label="Выбрать письмо"></div><div class="body"><div class="l1"><span class="from"></span></div><div class="subj"></div><div class="prev"></div></div><div class="meta"><span class="time"></span></div>`;
    row.querySelector('.ava').textContent=initial;row.querySelector('.from').textContent=message.from?.name||message.from?.email||'';
    row.querySelector('.subj').textContent=message.subject||'';row.querySelector('.prev').textContent=message.preview||'';
    row.querySelector('.time').textContent=message.date?new Date(message.date).toLocaleDateString(document.documentElement.lang):'';
    const checkbox=row.querySelector('.msg-check');checkbox.checked=selectedMessageIds.has(message.id);row.classList.toggle('selected',checkbox.checked);
    const select=(checked,range=false)=>{if(range&&lastSelectedMessageIndex>=0){const index=currentMessageRows.findIndex(item=>item.id===message.id),from=Math.min(index,lastSelectedMessageIndex),to=Math.max(index,lastSelectedMessageIndex);for(let i=from;i<=to;i++)selectedMessageIds.add(currentMessageRows[i].id);}else if(checked)selectedMessageIds.add(message.id);else selectedMessageIds.delete(message.id);lastSelectedMessageIndex=currentMessageRows.findIndex(item=>item.id===message.id);document.querySelectorAll('.msg').forEach(item=>{const on=selectedMessageIds.has(Number(item.dataset.messageId));item.classList.toggle('selected',on);item.querySelector('.msg-check').checked=on;});};
    checkbox.onclick=e=>{e.stopPropagation();select(checkbox.checked,e.shiftKey);};
    row.onclick=e=>{if(e.ctrlKey||e.metaKey||e.shiftKey){select(!selectedMessageIds.has(message.id),e.shiftKey);return;}showMessage(message);};list.appendChild(row);
  });
  if(!rows.length)document.getElementById('tbody').innerHTML=`<div class="mail-empty"><h2>${wizardLocale==='en'?'No messages':'Писем нет'}</h2></div>`;
  else if(activeMessage&&rows.some(message=>message.id===activeMessage.id))document.querySelector(`.msg[data-message-id="${activeMessage.id}"]`)?.classList.add('active');
  else document.getElementById('tbody').innerHTML=`<div class="mail-empty"><h2>${wizardLocale==='en'?'Select a message':'Выберите письмо'}</h2></div>`;
}
async function showMessage(message){
  activeMessage=message;
  document.getElementById('tSubject').textContent=message.subject||'';const body=document.getElementById('tbody');
  body.innerHTML='<div class="mail-loading">Загрузка письма…</div>';
  document.querySelectorAll('.msg').forEach(row=>row.classList.toggle('active',+row.dataset.messageId===message.id));
  try{
    const full=await window.tm?.getMessage(message.id);activeFullMessage=full;body.innerHTML='';const article=document.createElement('article');article.className='mail-content';
    const head=document.createElement('header');head.innerHTML='<div class="mail-from"></div><div class="mail-address"></div>';
    head.querySelector('.mail-from').textContent=full.meta.from?.name||full.meta.from?.email||'';head.querySelector('.mail-address').textContent=full.meta.from?.email||'';
    const content=document.createElement('div');content.className='mail-body';if(full.body_html)await renderHtmlMessage(content,full.body_html,full.meta.from?.email);else{content.classList.add('plain');content.textContent=full.body_text||full.meta.preview||'';}
    article.append(head,content);if(full.attachments?.length){const section=document.createElement('section');section.className='mail-attachments';const title=document.createElement('h3');title.textContent=`${wizardLocale==='en'?'Attachments':'Вложения'} (${full.attachments.length})`;section.appendChild(title);const files=document.createElement('div');files.className='mail-attachment-list';full.attachments.forEach(attachment=>{const file=document.createElement('div');file.className='mail-attachment';file.innerHTML='<i data-i="paperclip"></i><span class="attachment-name"></span><small></small>';file.querySelector('.attachment-name').textContent=attachment.filename;file.querySelector('small').textContent=[attachment.mime_type,formatBytes(attachment.size)].filter(Boolean).join(' · ');files.appendChild(file);});section.appendChild(files);article.appendChild(section);renderIcons(section);}body.appendChild(article);if(!message.flags?.seen){message.flags.seen=true;document.querySelector(`.msg[data-message-id="${message.id}"]`)?.classList.remove('unread');window.tm?.markSeen(message.id,true).catch(console.error);}
  }catch(error){body.innerHTML='';const err=document.createElement('div');err.className='mail-error';err.textContent=error.message||String(error);body.appendChild(err);}
}
function smartRows(index){let rows=[...messages].filter(message=>window.coreUnifiedSettings?.[message.folder_id]!=='0');switch(index){case 0:rows=rows.filter(m=>coreFolders.find(f=>f.id===m.folder_id)?.role==='inbox');break;case 1:rows=rows.filter(m=>m.flags?.flagged);break;case 2:rows=rows.filter(m=>coreFolders.find(f=>f.id===m.folder_id)?.role==='sent');break;case 3:rows=rows.filter(m=>m.flags?.draft||coreFolders.find(f=>f.id===m.folder_id)?.role==='drafts');break;case 4:{const since=Date.now()-86400000;rows=rows.filter(m=>new Date(m.date).getTime()>=since);break;}case 5:rows=rows.filter(m=>!m.flags?.seen);break;case 6:rows=rows.filter(m=>m.has_attachments);break;case 7:rows=rows.filter(m=>m.flags?.answered);break;default:{const groups=smartFolders[index]?.groups||[];const value=(m,field)=>({sender:`${m.from?.name||''} ${m.from?.email||''}`,recipient:(m.to||[]).map(a=>`${a.name||''} ${a.email||''}`).join(' '),subject:m.subject||'',body:m.preview||'',account:coreAccounts.find(a=>a.id===m.account_id)?.email||'',status:m.flags?.seen?'read':'unread',attachment:m.has_attachments?'yes':'no',label:(m.labels||[]).join(' '),folder:coreFolders.find(f=>f.id===m.folder_id)?.display_name||'',date:m.date||''})[legacySmartFields[field]||field]||'';const matches=(m,c)=>{const left=value(m,c.f).toLocaleLowerCase(),right=(c.v||'').toLocaleLowerCase(),op=legacySmartOps[c.o]||c.o;return op==='not_contains'?!left.includes(right):op==='equals'?left===right:left.includes(right);};rows=rows.filter(m=>groups.some(group=>group.every(c=>matches(m,c))));}}
  return rows;}
function filterSmart(index){currentSmartIndex=index;currentFolderId=null;renderMessageList(smartRows(index).sort((a,b)=>String(b.date||'').localeCompare(String(a.date||''))),smartFolders[index]?.t||'Письма');}

window.renderCoreAccounts=function(accounts,foldersByAccount,loadedMessages=[],contacts=[],calendarData={calendars:[],events:[]},savedSmartFolders=[],storage=null){
  const previousFolder=currentFolderId,previousSmart=currentSmartIndex,previousMessageId=activeMessage?.id,navScroll=document.querySelector('.nav')?.scrollTop||0,messageScroll=msgsEl.scrollTop;
  window.clearDemoData(true);
  coreAccounts=accounts;coreFolders=foldersByAccount.flat();messages=loadedMessages;coreContacts=contacts;coreCalendarData=calendarData;
  const accountCount=document.getElementById('mailAccountCount');if(accountCount){const n=accounts.length,label=wizardLocale==='en'?(n===1?'account':'accounts'):(n%10===1&&n%100!==11?'аккаунт':n%10>=2&&n%10<=4&&(n%100<10||n%100>=20)?'аккаунта':'аккаунтов');accountCount.textContent=`${n} ${label}`;}
  coreFolders.forEach(folder=>folderHasMore.set(folder.id,messages.filter(message=>message.folder_id===folder.id).length===MESSAGE_PAGE_SIZE));
  const labels=[...document.querySelectorAll('.nav .navlabel')];
  const accountsLabel=labels.find(el=>el.textContent.includes('Аккаунты'))||labels[1];
  let anchor=accountsLabel;
  accounts.forEach((account,index)=>{
    const header=document.createElement('div');header.className='acc-h open';
    const initial=(account.display_name||account.email||'?').trim()[0].toUpperCase();
    header.innerHTML=`<span class="ava ava-c${index%8}"></span><span class="em"></span><span class="chev"><i data-i="chevR"></i></span>`;
    header.querySelector('.ava').textContent=initial;header.querySelector('.em').textContent=account.email;
    anchor.after(header);anchor=header;
    const sub=document.createElement('div');sub.className='acc-sub open';
    const accountFolders=sortedFolders(foldersByAccount[index]||[]);
    accountFolders.forEach(folder=>{const row=document.createElement('div');row.className='navitem folder-row';row.dataset.folderId=folder.id;
      const icon=folderIcon(folder);const depth=Math.max(0,(folder.remote_path.match(/[\/|]/g)||[]).length);row.style.paddingLeft=`${14+depth*14}px`;
      row.innerHTML=`<i data-i="${icon}"></i><span class="folder-name"></span>${folder.unread_count?'<span class="count"></span>':''}`;
      row.querySelector('.folder-name').textContent=folder.display_name;if(folder.unread_count)row.querySelector('.count').textContent=folder.unread_count;
      row.onclick=()=>{goMail();document.querySelectorAll('.navitem').forEach(item=>item.classList.remove('active'));row.classList.add('active');currentFolderId=folder.id;currentSmartIndex=null;renderMessageList(messages.filter(m=>m.folder_id===folder.id),folder.display_name);};sub.appendChild(row);});
    anchor.after(sub);anchor=sub;
  });
  renderIcons(document.querySelector('.nav'));
  if(previousFolder!==null&&coreFolders.some(folder=>folder.id===previousFolder)){
    currentFolderId=previousFolder;currentSmartIndex=null;const folder=coreFolders.find(item=>item.id===previousFolder);document.querySelector(`.folder-row[data-folder-id="${previousFolder}"]`)?.classList.add('active');renderMessageList(messages.filter(message=>message.folder_id===previousFolder).sort((a,b)=>String(b.date||'').localeCompare(String(a.date||''))||b.id-a.id),folder?.display_name);
  }else filterSmart(previousSmart??0);
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
function isExpiredOauthCode(error){return /invalid_grant|code has expired|verification code.*expired/i.test(error?.message||String(error));}
document.getElementById('accountOauthStart').onclick=async()=>{
  const email=document.getElementById('accountEmail').value.trim(),status=document.getElementById('accountOauthStatus');
  const button=document.getElementById('accountOauthStart');
  if(!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)){status.textContent='Введите корректный адрес почты.';status.dataset.kind='error';return;}
  if(!window.tm?.beginYandexOauth){status.textContent='OAuth доступен внутри приложения truemail.';status.dataset.kind='error';return;}
  try{button.disabled=true;status.textContent='Открываю Яндекс ID в браузере…';status.dataset.kind='';const pending=await window.tm.beginYandexOauth(email);accountOauthState=pending.state;document.getElementById('accountCodeRow').classList.remove('hidden');status.textContent='После входа скопируйте сюда код подтверждения.';document.getElementById('accountOauthCode').focus();}
  catch(e){button.disabled=false;status.textContent=e.message||String(e);status.dataset.kind='error';}
};
document.getElementById('accountOauthConfirm').onclick=async()=>{
  const code=document.getElementById('accountOauthCode').value.trim(),status=document.getElementById('accountOauthStatus');if(!code)return;
  try{status.textContent='Подключаю почту, календарь и контакты…';status.dataset.kind='';document.getElementById('accountOauthConfirm').disabled=true;const connected=await window.tm.completeYandexOauth(accountOauthState,code);status.textContent=connected.warnings?.length?connected.warnings.join(' '):'Аккаунт подключён.';status.dataset.kind=connected.warnings?.length?'warning':'success';setTimeout(async()=>{closeAccountWizard();await window.reloadCoreData?.();await window.tm?.startRealtime();showView('mailView');},connected.warnings?.length?2500:300);}
  catch(e){if(isExpiredOauthCode(e)){accountOauthState='';document.getElementById('accountOauthCode').value='';document.getElementById('accountCodeRow').classList.add('hidden');document.getElementById('accountOauthStart').disabled=false;status.textContent='Код истёк или уже был использован. Нажмите «Подключить» и получите новый код.';}else status.textContent=e.message||String(e);status.dataset.kind='error';document.getElementById('accountOauthConfirm').disabled=false;}
};
document.getElementById('wzConnect').onclick=async()=>{
  const email=document.getElementById('wzEmail').value.trim(),status=document.getElementById('wzConnectStatus');
  const button=document.getElementById('wzConnect');
  if(!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)){status.textContent=wt('invalidEmail');status.dataset.kind='error';return;}
  if(!window.tm?.beginYandexOauth){status.textContent=wt('oauthUnavailable');status.dataset.kind='error';return;}
  try{button.disabled=true;status.textContent=wt('openingYandex');status.dataset.kind='';const pending=await window.tm.beginYandexOauth(email);pendingOauthState=pending.state;document.getElementById('wzCodeBox').classList.remove('hidden');status.textContent=wt('enterCode');document.getElementById('wzOauthCode').focus();}
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
function validAddress(value){return /^[^\s<>@]+@[^\s<>@]+\.[^\s<>@]+$/.test(value)||/^.+\s<[^\s<>@]+@[^\s<>@]+\.[^\s<>@]+>$/.test(value);}
function resetComposer(){composerFieldIds.forEach(id=>document.getElementById(id).value='');compEditEl.innerHTML='';composerAttachments=[];compAtt.innerHTML='';document.getElementById('composeStatus').textContent='';document.getElementById('compSendAt').classList.add('hidden');}
function openComposerForMessage(action){if(!activeMessage)return;resetComposer();const reply=action!=='forward',from=activeFullMessage?.meta?.from?.email||activeMessage.from?.email||'',subject=activeMessage.subject||'',prefix=action==='forward'?'Fwd: ':'Re: ';document.getElementById('compTitle').textContent=action==='forward'?'Переслать':'Ответить';document.getElementById('compSubj').value=new RegExp(`^${prefix}`,'i').test(subject)?subject:prefix+subject;if(reply)document.getElementById('compTo').value=from;if(action==='replyall'){const own=new Set(coreAccounts.map(account=>account.email.toLowerCase()));const others=[...(activeFullMessage?.meta?.to||[]),...(activeFullMessage?.meta?.cc||[])].map(address=>address.email).filter(email=>email&&!own.has(email.toLowerCase())&&email.toLowerCase()!==from.toLowerCase());document.getElementById('compCc').value=[...new Set(others)].join(', ');}const text=activeFullMessage?.body_text||activeMessage.preview||'';compEditEl.textContent=`\n\n--- Исходное сообщение ---\nОт: ${from}\nТема: ${subject}\n\n${text}`;showView('composeView');}
function showToast(message,actionLabel,action){document.querySelector('.app-toast')?.remove();const toast=document.createElement('div');toast.className='app-toast';const text=document.createElement('span');text.textContent=message;toast.appendChild(text);if(action){const button=document.createElement('button');button.type='button';button.textContent=actionLabel;button.onclick=async()=>{button.disabled=true;await action();toast.remove();};toast.appendChild(button);}document.body.appendChild(toast);setTimeout(()=>toast.remove(),9000);}
window.handleSyncState=function(state){if(!state)return;const info=document.getElementById('calSyncInfo');if(info&&state.scope==='dav'){if(state.status==='syncing')info.textContent=wizardLocale==='en'?'Syncing calendars and contacts…':'Синхронизация календарей и контактов…';else if(state.status==='error')info.textContent=wizardLocale==='en'?'Calendar and contacts sync error':'Ошибка синхронизации календаря и контактов';}if(state.status==='error')showToast(state.error||'Ошибка синхронизации');else if(state.warnings?.length)showToast(state.warnings.join(' '));};
async function performMessageAction(action){const ids=selectedMessageIds.size?[...selectedMessageIds]:activeMessage?[activeMessage.id]:[];if(!ids.length){showToast('Сначала выберите письмо');return;}try{const queued=await window.tm.messageAction(ids,action);selectedMessageIds.clear();activeMessage=null;activeFullMessage=null;await window.reloadCoreData();showToast(action==='archive'?'Письмо перемещено в архив':'Письмо перемещено в корзину','Отменить',async()=>{await window.tm.undoMessageAction(queued.operation_ids);await window.reloadCoreData();});}catch(error){showToast(error.message||String(error));}}
function renderComposerAttachment(item){const el=document.createElement('span');el.className='att-mini';el.innerHTML='<i data-i="paperclip"></i><span class="att-name"></span><span class="csub"></span><span class="x">×</span>';el.querySelector('.att-name').textContent=item.filename;el.querySelector('.csub').textContent=formatBytes(item.data.length);renderIcons(el);el.querySelector('.x').onclick=()=>{composerAttachments=composerAttachments.filter(value=>value!==item);el.remove();scheduleDraftSave();};compAtt.appendChild(el);}
async function addCompFile(file){const item={filename:file.name||'attachment',mime_type:file.type||'application/octet-stream',data:Array.from(new Uint8Array(await file.arrayBuffer()))};composerAttachments.push(item);renderComposerAttachment(item);scheduleDraftSave();}
composeEl.addEventListener('dragover',e=>{e.preventDefault();composeEl.classList.add('dragover');});
composeEl.addEventListener('dragleave',e=>{if(!composeEl.contains(e.relatedTarget))composeEl.classList.remove('dragover');});
composeEl.addEventListener('drop',e=>{e.preventDefault();composeEl.classList.remove('dragover');
  const files=e.dataTransfer&&e.dataTransfer.files;if(files&&files.length){for(const file of files)addCompFile(file).catch(console.error);}});
compEditEl.addEventListener('paste',e=>{const items=e.clipboardData&&e.clipboardData.items;if(!items)return;
  for(const item of items){if(item.type.indexOf('image')===0){const file=item.getAsFile();if(file){e.preventDefault();addCompFile(new File([file],'изображение из буфера.png',{type:file.type})).catch(console.error);}}}});
document.getElementById('compAttach').onclick=()=>document.getElementById('compFile').click();
document.getElementById('compFile').onchange=e=>{for(const file of e.target.files||[])addCompFile(file).catch(console.error);e.target.value='';};
document.querySelectorAll('[data-format]').forEach(button=>button.onclick=()=>{compEditEl.focus();document.execCommand(button.dataset.format,false);scheduleDraftSave();});
document.getElementById('compLink').onclick=()=>{const href=prompt('Ссылка');if(href&&/^https?:\/\//i.test(href)){compEditEl.focus();document.execCommand('createLink',false,href);scheduleDraftSave();}};
let draftSaveTimer=null;
function draftPayload(){return {account_id:+document.querySelector('.from-sel').value||coreAccounts[0]?.id||0,to:document.getElementById('compTo').value,cc:document.getElementById('compCc').value,bcc:document.getElementById('compBcc').value,subject:document.getElementById('compSubj').value,body_html:compEditEl.innerHTML,body_text:compEditEl.innerText,attachments:composerAttachments};}
function scheduleDraftSave(){clearTimeout(draftSaveTimer);draftSaveTimer=setTimeout(()=>window.tm?.setSetting('composer_draft',JSON.stringify(draftPayload())).catch(console.error),500);}
composerFieldIds.forEach(id=>document.getElementById(id).addEventListener('input',scheduleDraftSave));compEditEl.addEventListener('input',scheduleDraftSave);
function composerRequest(){const draft=draftPayload(),to=splitAddresses(draft.to),cc=splitAddresses(draft.cc),bcc=splitAddresses(draft.bcc),invalid=[...to,...cc,...bcc].find(address=>!validAddress(address));if(!to.length&&!cc.length&&!bcc.length)throw new Error('Укажите хотя бы одного получателя');if(invalid)throw new Error(`Некорректный адрес: ${invalid}`);return {account_id:draft.account_id,to,cc,bcc,subject:draft.subject,body_text:draft.body_text,body_html:draft.body_html,attachments:composerAttachments};}
document.getElementById('compSend').onclick=async()=>{const status=document.getElementById('composeStatus'),button=document.getElementById('compSend');try{const request=composerRequest();button.disabled=true;status.textContent='Отправляю…';status.dataset.kind='';await window.tm.sendMessage(request);await window.tm.setSetting('composer_draft','');status.textContent='Отправлено';status.dataset.kind='success';setTimeout(()=>{resetComposer();showView('mailView');},500);}catch(error){status.textContent=error.message||String(error);status.dataset.kind='error';}finally{button.disabled=false;}};
document.getElementById('compSendLater').onclick=async()=>{const input=document.getElementById('compSendAt'),status=document.getElementById('composeStatus');if(input.classList.contains('hidden')){const date=new Date(Date.now()+15*60*1000);date.setSeconds(0,0);input.value=new Date(date.getTime()-date.getTimezoneOffset()*60000).toISOString().slice(0,16);input.min=new Date(Date.now()-new Date().getTimezoneOffset()*60000).toISOString().slice(0,16);input.classList.remove('hidden');input.focus();return;}try{const date=new Date(input.value);if(Number.isNaN(date.getTime()))throw new Error('Выберите дату и время');const id=await window.tm.scheduleMessage(composerRequest(),date.toISOString());await window.tm.setSetting('composer_draft','');status.textContent=`Запланировано (задача ${id})`;status.dataset.kind='success';setTimeout(()=>{resetComposer();showView('mailView');},700);}catch(error){status.textContent=error.message||String(error);status.dataset.kind='error';}};
document.getElementById('compDeleteDraft').onclick=async()=>{resetComposer();await window.tm?.setSetting('composer_draft','').catch(console.error);showView('mailView');};

/* expert mode toggle */
function renderAccountSettings(accounts,foldersByAccount,calendars){
  const page=document.getElementById('set-accounts');page.querySelectorAll('.account-card').forEach(card=>card.remove());
  accounts.forEach((account,index)=>{const folders=foldersByAccount[index]||[],accountCalendars=calendars.filter(cal=>cal.account_id===account.id);const card=document.createElement('div');card.className='card account-card';card.innerHTML=`<div class="ch"><span class="ava ava-c${index%8} ava-26"></span><div class="grow"><div class="account-email"></div><div class="account-stats"></div></div></div><div class="cb"><div class="t">Календари и адресные книги определяются автоматически по адресу аккаунта.</div><div class="account-calendars"></div></div>`;card.querySelector('.ava').textContent=(account.display_name||account.email)[0].toUpperCase();card.querySelector('.account-email').textContent=account.email;card.querySelector('.account-stats').textContent=`${folders.length} папок · ${accountCalendars.length} календарей`;const chips=card.querySelector('.account-calendars');(accountCalendars.length?accountCalendars:[{name:'Календарь ещё синхронизируется'}]).forEach(cal=>{const chip=document.createElement('span');chip.className='calendar-chip';chip.textContent=cal.name;chips.appendChild(chip);});renderIcons(card);page.appendChild(card);});
  const mapping=document.getElementById('set-folders');mapping.querySelectorAll('.mapping-generated').forEach(el=>el.remove());accounts.forEach((account,index)=>{const card=document.createElement('div');card.className='card mapping-generated';card.innerHTML=`<div class="ch">${escapeHtml(account.email)}</div><div class="cb"></div>`;const body=card.querySelector('.cb'),folders=sortedFolders(foldersByAccount[index]||[]);['inbox','sent','drafts','archive','spam','trash'].forEach(role=>{const row=document.createElement('div');row.className='map-row map-2';row.innerHTML=`<div class="role">${({inbox:'Входящие',sent:'Отправленные',drafts:'Черновики',archive:'Архив',spam:'Спам',trash:'Корзина'})[role]}</div><select class="sel"><option value="">Не назначено</option>${folders.map(folder=>`<option value="${folder.id}"${folder.role===role?' selected':''}>${escapeHtml(folder.display_name)}</option>`).join('')}</select>`;row.querySelector('select').onchange=async()=>{const value=row.querySelector('select').value;try{await window.tm?.setFolderRole(account.id,role,value?+value:null);await window.reloadCoreData?.();showToast('Сопоставление папки сохранено');}catch(error){showToast(error.message||String(error));}};body.appendChild(row);});mapping.appendChild(card);});
  const unified=document.getElementById('set-unified');unified.querySelectorAll('.unified-generated').forEach(el=>el.remove());const info=document.createElement('div');info.className='card unified-generated';info.innerHTML='<div class="ch">Источники объединённых папок</div><div class="cb"></div>';coreFolders.forEach(folder=>{if(!folder.role)return;const row=document.createElement('label');row.className='frow';const enabled=window.coreUnifiedSettings?.[folder.id]!=='0';row.innerHTML=`<input type="checkbox"${enabled?' checked':''}><span>${escapeHtml(accounts.find(a=>a.id===folder.account_id)?.email||'')} / ${escapeHtml(folder.display_name)}</span>`;row.querySelector('input').onchange=e=>{window.coreUnifiedSettings=window.coreUnifiedSettings||{};window.coreUnifiedSettings[folder.id]=e.target.checked?'1':'0';window.tm?.setSetting(`unified_${folder.id}`,window.coreUnifiedSettings[folder.id]).catch(console.error);if(currentSmartIndex!==null)filterSmart(currentSmartIndex);};info.querySelector('.cb').appendChild(row);});unified.appendChild(info);
  const from=document.querySelector('.from-sel');if(from){from.innerHTML=accounts.map(account=>`<option value="${account.id}">${escapeHtml(account.email)}</option>`).join('');if(window.pendingComposerDraft?.account_id)from.value=String(window.pendingComposerDraft.account_id);}
  if(window.pendingComposerDraft){const draft=window.pendingComposerDraft;document.getElementById('compTo').value=draft.to||'';document.getElementById('compCc').value=draft.cc||'';document.getElementById('compBcc').value=draft.bcc||'';document.getElementById('compSubj').value=draft.subject||'';compEditEl.innerHTML=draft.body_html||'';composerAttachments=Array.isArray(draft.attachments)?draft.attachments:[];compAtt.innerHTML='';composerAttachments.forEach(renderComposerAttachment);window.pendingComposerDraft=null;}
}
function applyStorageStatus(storage){document.querySelector('.storage-big').textContent=formatBytes(storage.total_bytes);const path=document.querySelector('#set-storage .d.mono');if(path)path.textContent=storage.data_dir;document.querySelector('.storage-sub').textContent=`${wizardLocale==='en'?'database':'база'} ${formatBytes(storage.database_bytes)} · ${wizardLocale==='en'?'files':'файлы'} ${formatBytes(storage.blob_bytes)}`;const measured=Math.max(1,(storage.database_bytes||0)+(storage.blob_bytes||0));const db=document.querySelector('.usebar .seg-db'),blob=document.querySelector('.usebar .seg-blob');if(db)db.style.width=`${100*(storage.database_bytes||0)/measured}%`;if(blob)blob.style.width=`${100*(storage.blob_bytes||0)/measured}%`;}

const filterMenu=document.getElementById('filterMenu'),sortMenu=document.getElementById('sortMenu');
document.getElementById('filterBtn').onclick=e=>{e.stopPropagation();filterMenu.classList.toggle('hidden');sortMenu.classList.add('hidden');};document.getElementById('sortBtn').onclick=e=>{e.stopPropagation();sortMenu.classList.toggle('hidden');filterMenu.classList.add('hidden');};
function applyListOptions(){let rows=currentFolderId!==null?messages.filter(m=>m.folder_id===currentFolderId):smartRows(currentSmartIndex??0);const active=[...filterMenu.querySelectorAll('input:checked')].map(input=>input.dataset.filter);if(active.includes('unread'))rows=rows.filter(m=>!m.flags?.seen);if(active.includes('attachments'))rows=rows.filter(m=>m.has_attachments);if(active.includes('flagged'))rows=rows.filter(m=>m.flags?.flagged);const sort=sortMenu.dataset.sort||'date-desc';rows.sort((a,b)=>sort==='date-asc'?String(a.date||'').localeCompare(String(b.date||'')):sort==='sender'?String(a.from?.name||a.from?.email||'').localeCompare(String(b.from?.name||b.from?.email||'')):sort==='subject'?String(a.subject||'').localeCompare(String(b.subject||'')):String(b.date||'').localeCompare(String(a.date||'')));renderMessageList(rows,document.querySelector('.listhead h2').textContent);}
filterMenu.querySelectorAll('input').forEach(input=>input.onchange=applyListOptions);sortMenu.querySelectorAll('button').forEach(button=>button.onclick=()=>{sortMenu.dataset.sort=button.dataset.sort;sortMenu.classList.add('hidden');applyListOptions();});

const sidebarWidth=document.getElementById('sidebarWidth'),navResizer=document.getElementById('navResizer');function setSidebarWidth(value,persist=true){value=Math.max(180,Math.min(420,+value||250));root.style.setProperty('--nav-w',`${value}px`);sidebarWidth.value=value;document.getElementById('sidebarWidthValue').textContent=`${value} px`;if(persist)window.tm?.setSetting('sidebar_width',String(value)).catch(console.error);}sidebarWidth.oninput=e=>setSidebarWidth(e.target.value);navResizer.addEventListener('pointerdown',e=>{navResizer.classList.add('dragging');navResizer.setPointerCapture(e.pointerId);});navResizer.addEventListener('pointermove',e=>{if(navResizer.hasPointerCapture(e.pointerId))setSidebarWidth(e.clientX,false);});navResizer.addEventListener('pointerup',e=>{if(navResizer.hasPointerCapture(e.pointerId)){navResizer.releasePointerCapture(e.pointerId);navResizer.classList.remove('dragging');setSidebarWidth(e.clientX,true);}});
document.getElementById('sidebarWidthQuick').oninput=e=>setSidebarWidth(e.target.value);const originalSetSidebarWidth=setSidebarWidth;setSidebarWidth=(value,persist=true)=>{originalSetSidebarWidth(value,persist);document.getElementById('sidebarWidthQuick').value=Math.max(180,Math.min(420,+value||250));};

function setUiScale(value,persist=true){value=Math.max(80,Math.min(150,+value||100));document.getElementById('uiScale').value=value;root.style.setProperty('--fs',`${13.5*value/100}px`);document.querySelectorAll('#scalePresets button').forEach(b=>b.classList.toggle('on',+b.dataset.scale===value));if(persist)window.tm?.setSetting('ui_scale',String(value)).catch(console.error);}document.getElementById('uiScale').oninput=e=>setUiScale(e.target.value);document.querySelectorAll('#scalePresets button').forEach(b=>b.onclick=()=>setUiScale(b.dataset.scale));

function confirmAction(message){return new Promise(resolve=>{const overlay=document.createElement('div');overlay.className='overlay open';const modal=document.createElement('div');modal.className='modal compact-modal';const body=document.createElement('div');body.className='mb';body.textContent=message;const foot=document.createElement('div');foot.className='mf';const ok=document.createElement('button');ok.className='btn primary';ok.textContent='Продолжить';const cancel=document.createElement('button');cancel.className='btn';cancel.textContent='Отмена';const done=value=>{overlay.remove();resolve(value);};ok.onclick=()=>done(true);cancel.onclick=()=>done(false);overlay.onclick=e=>{if(e.target===overlay)done(false);};foot.append(ok,cancel);modal.append(body,foot);overlay.appendChild(modal);document.body.appendChild(overlay);cancel.focus();});}
document.getElementById('openDataDir').onclick=()=>window.tm?.openDataDir().catch(error=>showToast(error.message||String(error)));document.getElementById('changeDataDir').onclick=async()=>{try{const current=document.querySelector('#set-storage .d.mono').textContent,chosen=await window.tm.chooseDataDir(current);if(chosen){await window.tm.moveStorage(chosen);showToast('Данные перенесены, новый путь уже используется.');document.querySelector('#set-storage .d.mono').textContent=chosen;}}catch(error){showToast(error.message||String(error));}};document.querySelectorAll('[data-clear]').forEach(button=>button.onclick=async()=>{if(!await confirmAction('Очистить выбранные локальные данные? Данные на сервере не удаляются.'))return;try{await window.tm.clearLocalData(button.dataset.clear);await window.reloadCoreData();showToast('Локальные данные очищены');}catch(error){showToast(error.message||String(error));}});

let tooltipPortal=null;document.addEventListener('mouseover',e=>{const help=e.target.closest('.help[data-tip]');if(!help)return;tooltipPortal=document.createElement('div');tooltipPortal.className='help-portal';tooltipPortal.textContent=help.dataset.tip;document.body.appendChild(tooltipPortal);const rect=help.getBoundingClientRect(),box=tooltipPortal.getBoundingClientRect();let left=Math.max(12,Math.min(window.innerWidth-box.width-12,rect.left+rect.width/2-box.width/2)),top=rect.top-box.height-10;if(top<12)top=rect.bottom+10;tooltipPortal.style.left=`${left}px`;tooltipPortal.style.top=`${top}px`;});document.addEventListener('mouseout',e=>{if(e.target.closest('.help[data-tip]')){tooltipPortal?.remove();tooltipPortal=null;}});

document.getElementById('expertToggle').addEventListener('click',()=>{
  const enabled=document.getElementById('expertToggle').classList.contains('on');
  root.setAttribute('data-mode',enabled?'expert':'simple');
  window.tm?.setSetting('expert_mode',enabled?'1':'0').catch(console.error);});

window.applyCoreSettings=function(settings){
  if(settings.theme)setTheme(settings.theme,false);
  if(settings.density)setDensity(settings.density,false);
  if(settings.accent)setAccent(settings.accent,false);
  const expert=settings.expert_mode==='1';
  document.getElementById('expertToggle').classList.toggle('on',expert);
  root.setAttribute('data-mode',expert?'expert':'simple');
  if(settings.sidebar_width)setSidebarWidth(settings.sidebar_width,false);
  if(settings.ui_scale)setUiScale(settings.ui_scale,false);
  if(settings.toolbar_layout){try{const state=JSON.parse(settings.toolbar_layout);state.actions?.forEach(action=>{const row=tbList.querySelector(`[data-action="${action.key}"]`);if(row){row.classList.toggle('off',!action.visible);row.querySelector('.toggle').classList.toggle('on',action.visible);tbList.appendChild(row);}});document.querySelectorAll('#toolbarAlign button').forEach(b=>b.classList.toggle('on',b.dataset.align===state.align));document.querySelectorAll('#toolbarLabels button').forEach(b=>b.classList.toggle('on',b.dataset.labels===state.labels));applyToolbar();}catch(e){console.error(e);}}
  if(settings.smart_folders_ui){try{const saved=JSON.parse(settings.smart_folders_ui);if(Array.isArray(saved)){smartFolders.splice(0,smartFolders.length,...saved);renderSmartManagement();bindSmartNavigation();}}catch(e){console.error(e);}}
  if(settings.composer_draft){try{window.pendingComposerDraft=JSON.parse(settings.composer_draft);}catch(e){console.error(e);}}
  if(settings.search_history){try{const saved=JSON.parse(settings.search_history);if(Array.isArray(saved))searchHistory=saved.filter(value=>typeof value==='string').slice(0,10);}catch(e){console.error(e);}}
};
