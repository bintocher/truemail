// truemail UI module: calendar-contacts.js
/* calendar */
const cg=document.getElementById('calgrid');
let calendarCursor=new Date();
function parseDavDate(value){if(!value)return null;if(/^\d{8}/.test(value)){const m=value.match(/^(\d{4})(\d{2})(\d{2})(?:T(\d{2})(\d{2})(\d{2}))?/);if(m){const parts=[+m[1],+m[2]-1,+m[3],+(m[4]||0),+(m[5]||0),+(m[6]||0)];return /Z$/i.test(value)?new Date(Date.UTC(...parts)):new Date(...parts);}}const date=new Date(value);return Number.isNaN(date.getTime())?null:date;}
function isDateOnlyValue(value){return /^\d{4}-\d{2}-\d{2}$/.test(String(value||''))||/^\d{8}$/.test(String(value||''));}
function occurrenceDateValue(date,dateOnly){return dateOnly?calendarDateKey(date):date.toISOString();}
function expandCalendarEvents(events,rangeStart,rangeEnd){
  const overrides=new Map(),output=[];events.filter(event=>event.recurrence_id).forEach(event=>{const date=parseDavDate(event.recurrence_id);if(date)overrides.set(`${event.uid||''}:${date.getTime()}`,event);});
  const add=(event,date)=>{if(date>=rangeStart&&date<rangeEnd){const override=overrides.get(`${event.uid||''}:${date.getTime()}`),sourceStart=parseDavDate(event.dtstart),sourceEnd=parseDavDate(event.dtend),dateOnly=Boolean(event.all_day)||isDateOnlyValue(event.dtstart),dtstart=override?.dtstart||occurrenceDateValue(date,dateOnly),occurrenceStart=parseDavDate(dtstart);let dtend=override?.dtend||event.dtend||null;if(!override?.dtend&&sourceStart&&sourceEnd&&sourceEnd>sourceStart&&occurrenceStart){dtend=occurrenceDateValue(new Date(occurrenceStart.getTime()+(sourceEnd-sourceStart)),dateOnly);}output.push({...event,...(override||{}),dtstart,dtend});}};
  events.filter(event=>!event.recurrence_id).forEach(event=>{const first=parseDavDate(event.dtstart);if(!first)return;const excluded=new Set(String(event.exdates||'').split(',').map(parseDavDate).filter(Boolean).map(date=>date.getTime()));if(!excluded.has(first.getTime()))add(event,first);
    String(event.rdates||'').split(',').map(parseDavDate).filter(Boolean).forEach(date=>{if(!excluded.has(date.getTime()))add(event,date);});if(!event.rrule)return;
    const rule=Object.fromEntries(String(event.rrule).split(';').map(part=>part.split('=',2)));const frequency=rule.FREQ,interval=Math.max(1,+rule.INTERVAL||1),count=+rule.COUNT||Infinity,until=parseDavDate(rule.UNTIL)||rangeEnd,byDay=(rule.BYDAY||'').split(',').filter(Boolean).map(value=>value.slice(-2)),byMonthDay=(rule.BYMONTHDAY||'').split(',').map(Number).filter(Number.isFinite);const weekDays=['SU','MO','TU','WE','TH','FR','SA'];let emitted=1,cursor=new Date(first);cursor.setDate(cursor.getDate()+1);for(let scanned=0;cursor<=until&&cursor<rangeEnd&&emitted<count&&scanned<36600;scanned++,cursor.setDate(cursor.getDate()+1)){const days=Math.floor((cursor-first)/86400000),months=(cursor.getFullYear()-first.getFullYear())*12+cursor.getMonth()-first.getMonth(),years=cursor.getFullYear()-first.getFullYear();let matches=false;if(frequency==='DAILY')matches=days%interval===0;else if(frequency==='WEEKLY')matches=Math.floor(days/7)%interval===0&&(byDay.length?byDay.includes(weekDays[cursor.getDay()]):cursor.getDay()===first.getDay());else if(frequency==='MONTHLY')matches=months>=0&&months%interval===0&&(byMonthDay.length?byMonthDay.includes(cursor.getDate()):cursor.getDate()===first.getDate());else if(frequency==='YEARLY')matches=years>=0&&years%interval===0&&cursor.getMonth()===first.getMonth()&&cursor.getDate()===first.getDate();if(matches){emitted++;if(!excluded.has(cursor.getTime()))add(event,new Date(cursor));}}
  });
  overrides.forEach(event=>{const date=parseDavDate(event.dtstart);if(date&&!output.some(item=>item.id===event.id))add(event,date);});return output;
}
function localeName(date,options){return new Intl.DateTimeFormat(wizardLocale||'ru',options).format(date);}
function calendarDateKey(date){const pad=value=>String(value).padStart(2,'0');return `${date.getFullYear()}-${pad(date.getMonth()+1)}-${pad(date.getDate())}`;}
function visibleCalendarEvents(data=coreCalendarData){const calendars=data?.calendars||[],events=data?.events||[];if(!calendars.length)return events;const visibleIds=new Set(calendars.filter(calendar=>calendar.visible!==false).map(calendar=>calendar.id));return events.filter(event=>visibleIds.has(event.calendar_id));}
const calSidebar=document.getElementById('calendarSidebar'),calSidebarList=document.getElementById('calendarSidebarList'),calSidebarToggle=document.getElementById('calSidebarToggle');
function setCalendarSidebarOpen(open){document.getElementById('calSection').classList.toggle('sidebar-open',open);calSidebarToggle.setAttribute('aria-expanded',String(open));calSidebarToggle.dataset.i18nTitle=open?'calendarSidebarHide':'calendarSidebarShow';const english=document.documentElement.lang==='en';calSidebarToggle.title=open?(english?'Hide calendars':'Скрыть календари'):(english?'Show calendars':'Показать календари');localStorage.setItem('calendar_sidebar_open',open?'1':'0');}
async function setCalendarsVisible(ids,visible){const changed=(coreCalendarData.calendars||[]).filter(calendar=>ids.includes(calendar.id));changed.forEach(calendar=>{calendar.visible=visible;});renderCalendarData();try{await Promise.all(changed.map(calendar=>window.tm.setCalendarVisible(calendar.id,visible)));}catch(error){await window.reloadCoreData?.();showToast(error.message||String(error));}}
function calendarSidebarRow(label,color,checked,onChange,className){const row=document.createElement('label');row.className=className;const input=document.createElement('input');input.type='checkbox';input.checked=checked;input.onchange=()=>onChange(input.checked);const dot=document.createElement('span');dot.className=className==='cal-account-title'?'cal-account-dot':'cal-source-dot';dot.style.background=color;const text=document.createElement('span');text.className=className==='cal-account-title'?'cal-account-label':'cal-source-label';text.textContent=label;text.title=label;row.append(input,dot,text);return {row,input};}
function renderCalendarSidebar(){if(!calSidebarList)return;calSidebarList.innerHTML='';const calendars=coreCalendarData.calendars||[];const accounts=[...coreAccounts,...calendars.filter(calendar=>!coreAccounts.some(account=>account.id===calendar.account_id)).map(calendar=>({id:calendar.account_id,email:L('Неизвестный аккаунт','Unknown account')}))].filter((account,index,array)=>array.findIndex(item=>item.id===account.id)===index);accounts.forEach(account=>{const sources=calendars.filter(calendar=>calendar.account_id===account.id);if(!sources.length)return;const group=document.createElement('div');group.className='cal-account';const enabled=sources.filter(calendar=>calendar.visible!==false).length,accountRow=calendarSidebarRow(account.display_name||account.email,accountColorById(account.id),enabled===sources.length,value=>setCalendarsVisible(sources.map(calendar=>calendar.id),value),'cal-account-title');accountRow.input.indeterminate=enabled>0&&enabled<sources.length;group.appendChild(accountRow.row);sources.forEach(calendar=>{const sourceRow=calendarSidebarRow(calendar.name,calendar.color||accountColorById(account.id),calendar.visible!==false,value=>setCalendarsVisible([calendar.id],value),'cal-source');group.appendChild(sourceRow.row);});calSidebarList.appendChild(group);});if(!calendars.length){const empty=document.createElement('div');empty.className='cal-sidebar-empty';empty.textContent=L('Календари ещё синхронизируются…','Calendars are still syncing…');calSidebarList.appendChild(empty);}}
calSidebarToggle.onclick=()=>setCalendarSidebarOpen(!document.getElementById('calSection').classList.contains('sidebar-open'));
document.getElementById('calSidebarClose').onclick=()=>setCalendarSidebarOpen(false);
setCalendarSidebarOpen(localStorage.getItem('calendar_sidebar_open')==='1');
function renderCalendarData(data=coreCalendarData){
  coreCalendarData=data||{calendars:[],events:[]};renderCalendarSidebar();const allEvents=coreCalendarData.events||[],events=visibleCalendarEvents(coreCalendarData),visibleCalendars=(coreCalendarData.calendars||[]).filter(calendar=>calendar.visible!==false);
  const year=calendarCursor.getFullYear(),month=calendarCursor.getMonth(),displayEvents=expandCalendarEvents(events,new Date(year,month-1,20),new Date(year,month+2,10));document.getElementById('calTitle').textContent=localeName(calendarCursor,{month:'long',year:'numeric'});
  cg.innerHTML='';const start=(new Date(year,month,1).getDay()+6)%7,days=new Date(year,month+1,0).getDate(),prevDays=new Date(year,month,0).getDate();
  let visibleEvents=0;for(let i=0;i<42;i++){const day=i-start+1,current=i>=start&&day<=days,date=new Date(year,month,current?day:day<1?day:day);const number=current?day:day<1?prevDays+day:day-days;
    const cell=document.createElement('div');cell.className='calcell'+(!current?' other':'')+(new Date().toDateString()===date.toDateString()?' today':'');cell.innerHTML=`<div class="d${current?'':' d-dim'}">${number}</div>`;
    if(current){cell.dataset.date=calendarDateKey(date);cell.onclick=()=>{calendarCursor=new Date(date);};displayEvents.filter(event=>parseDavDate(event.dtstart)?.toDateString()===date.toDateString()).forEach((event,index)=>{visibleEvents++;const item=document.createElement('div');item.className=`ev ev-c${index%4}`;item.dataset.eventId=event.id;item.dataset.eventStart=event.dtstart;item.draggable=true;item.title=L('Перетащите на другую дату','Drag to another date');item.textContent=event.summary;item.style.cssText=eventColorStyle(event);cell.appendChild(item);});}cg.appendChild(cell);}
  const info=document.getElementById('calSyncInfo');if(info){const dated=events.map(event=>({date:parseDavDate(event.dtstart)})).filter(item=>item.date).sort((a,b)=>b.date-a.date),latest=dated[0];info.textContent=L(`${visibleCalendars.length} календаря · ${events.length} событий${visibleEvents?'':latest?' · показать последние':' · событий нет'}`,`${visibleCalendars.length} calendars · ${events.length} events${visibleEvents?'':latest?' · show latest':' · no events'}`);info.classList.toggle('clickable',!visibleEvents&&Boolean(latest));info.onclick=!visibleEvents&&latest?()=>{calendarCursor=new Date(latest.date.getFullYear(),latest.date.getMonth(),1);renderCalendarData();}:null;info.title=!visibleEvents&&latest?L(`Перейти к ${localeName(latest.date,{month:'long',year:'numeric'})}`,`Go to ${localeName(latest.date,{month:'long',year:'numeric'})}`):'';}
  renderWeekDay(events);
  const count=document.querySelector('[data-nav="calendar"] .count');if(count)count.textContent=allEvents.length||'';
}
const WK_HOUR=48; // высота одного часа в пикселях (совпадает с --wk-hour в CSS)
let calendarHourStartMinutes=8*60,calendarHourEndMinutes=20*60;
function parseCalendarClock(value,fallback){const match=String(value||'').match(/^(\d{2}):(\d{2})$/);if(!match)return fallback;const minutes=Number(match[1])*60+Number(match[2]);return Number.isFinite(minutes)&&minutes>=0&&minutes<24*60?minutes:fallback;}
function calendarRangeHeight(){return (calendarHourEndMinutes-calendarHourStartMinutes)/60*WK_HOUR;}
window.applyCalendarHourRange=function(startValue='08:00',endValue='20:00',persist=false){const startInput=document.getElementById('calendarHourStart'),endInput=document.getElementById('calendarHourEnd'),status=document.getElementById('calendarHoursStatus'),start=parseCalendarClock(startValue,8*60),end=parseCalendarClock(endValue,20*60);if(startInput)startInput.value=startValue;if(endInput)endInput.value=endValue;if(startInput&&endInput){startInput.max=endInput.value;endInput.min=startInput.value;}if(start>=end){if(status){status.textContent=L('Время «с» должно быть раньше времени «по».','Start time must be earlier than end time.');status.dataset.kind='error';}return false;}calendarHourStartMinutes=start;calendarHourEndMinutes=end;if(status){status.textContent='';status.dataset.kind='';}if(persist){window.tm?.setSetting('calendar_hour_start',startValue).catch(console.error);window.tm?.setSetting('calendar_hour_end',endValue).catch(console.error);renderWeekDay(visibleCalendarEvents());}return true;};
const calendarHourStartInput=document.getElementById('calendarHourStart'),calendarHourEndInput=document.getElementById('calendarHourEnd');
if(calendarHourStartInput&&calendarHourEndInput){const applyHours=()=>window.applyCalendarHourRange(calendarHourStartInput.value,calendarHourEndInput.value,true);calendarHourStartInput.onchange=applyHours;calendarHourEndInput.onchange=applyHours;}
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
  const viewStart=new Date(dayStart);viewStart.setMinutes(calendarHourStartMinutes);const viewEnd=new Date(dayStart);viewEnd.setMinutes(calendarHourEndMinutes);
  const clipped=items.map(it=>({...it,start:it.start<viewStart?viewStart:it.start,end:it.end>viewEnd?viewEnd:it.end})).filter(it=>it.end>it.start);
  return layoutColumns(clipped).map(it=>{
    const minutesTop=(it.start-viewStart)/60000;
    const durationMin=Math.max((it.end-it.start)/60000,20);
    const top=minutesTop/60*WK_HOUR,height=durationMin/60*WK_HOUR;
    const width=100/it.cols,left=it.col*width;
    const color=eventAccountColor(it.event);
    const paint=color?`border-left:3px solid ${color};background:${color}22;`:'';
    const style=`top:${top}px;height:${Math.max(height-2,16)}px;left:calc(${left}% + 2px);width:calc(${width}% - 4px);${paint}`;
    return `<div class="wk-ev" draggable="true" data-event-id="${it.event.id}" data-event-start="${escapeHtml(it.event.dtstart)}" style="${style}" title="${escapeHtml(L('Перетащите на другое время','Drag to another time'))}">${escapeHtml(it.event.summary)}</div>`;
  }).join('');
}
function timesColumn(){let out=`<div class="wk-times" style="min-height:${calendarRangeHeight()}px">`;for(let minute=calendarHourStartMinutes;minute<calendarHourEndMinutes;minute+=60){const hour=Math.floor(minute/60),mins=minute%60,top=(minute-calendarHourStartMinutes)/60*WK_HOUR;out+=`<div class="wk-tlabel" style="top:${top}px">${String(hour).padStart(2,'0')}:${String(mins).padStart(2,'0')}</div>`;}return out+'</div>';}
function renderWeekDay(events){
  const base=new Date(calendarCursor),monday=new Date(base);monday.setDate(base.getDate()-((base.getDay()+6)%7));
  const expanded=expandCalendarEvents(events,new Date(monday.getFullYear(),monday.getMonth(),monday.getDate()-1),new Date(monday.getFullYear(),monday.getMonth(),monday.getDate()+9));
  const intervals=expanded.map(eventInterval).filter(Boolean);
  const dayItems=(d)=>{const next=new Date(d.getFullYear(),d.getMonth(),d.getDate()+1);return intervals.filter(it=>it.start<next&&it.end>d);};
  // Неделя
  let head='<div class="wk-corner"></div>',cols='';
  for(let i=0;i<7;i++){const d=new Date(monday);d.setDate(monday.getDate()+i);const wd=localeName(d,{weekday:'short'}).replace('.','');const today=new Date().toDateString()===d.toDateString();head+=`<div class="wk-dayhd${today?' today':''}">${wizardLocale==='ru'?wd.slice(0,2):wd.slice(0,3)}<b>${d.getDate()}</b></div>`;const day=new Date(d.getFullYear(),d.getMonth(),d.getDate());cols+=`<div class="wk-daycol" data-date="${calendarDateKey(day)}" style="min-height:${calendarRangeHeight()}px">${renderDayColumn(day,dayItems(day))}</div>`;}
  document.getElementById('calweek').innerHTML=`<div class="wk-head">${head}</div><div class="wk-scroll">${timesColumn()}<div class="wk-cols">${cols}</div></div>`;
  // День
  const dayD=new Date(base.getFullYear(),base.getMonth(),base.getDate()),dToday=new Date().toDateString()===base.toDateString(),dwd=localeName(base,{weekday:'short'}).replace('.','');
  document.getElementById('calday').innerHTML=`<div class="wk-head wk-head-day"><div class="wk-corner"></div><div class="wk-dayhd${dToday?' today':''}">${wizardLocale==='ru'?dwd.slice(0,2):dwd.slice(0,3)}<b>${base.getDate()}</b></div></div><div class="wk-scroll">${timesColumn()}<div class="wk-cols wk-cols-day"><div class="wk-daycol" data-date="${calendarDateKey(dayD)}" style="min-height:${calendarRangeHeight()}px">${renderDayColumn(dayD,dayItems(dayD))}</div></div></div>`;
}
function escapeHtml(value){return String(value||'').replace(/[&<>"']/g,ch=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[ch]));}
// Цвет события = цвет его аккаунта (через календарь), как у писем в списке.
function eventAccountColor(event){const cal=(coreCalendarData.calendars||[]).find(item=>item.id===event.calendar_id);return cal?accountColorById(cal.account_id):null;}
function eventColorStyle(event){const color=eventAccountColor(event);return color?`border-left:3px solid ${color};background:${color}22`:'';}
const calendarSection=document.getElementById('calSection');
calendarSection.addEventListener('dragstart',event=>{const item=event.target.closest('.ev,.wk-ev');if(!item)return;item.classList.add('dragging');event.dataTransfer.effectAllowed='move';event.dataTransfer.setData('application/x-truemail-event',JSON.stringify({id:Number(item.dataset.eventId),start:item.dataset.eventStart}));});
calendarSection.addEventListener('dragend',event=>{event.target.closest('.ev,.wk-ev')?.classList.remove('dragging');calendarSection.querySelectorAll('.drop-hi').forEach(item=>item.classList.remove('drop-hi'));});
calendarSection.addEventListener('dragover',event=>{const target=event.target.closest('.calcell[data-date],.wk-daycol[data-date]');if(!target)return;event.preventDefault();event.dataTransfer.dropEffect='move';calendarSection.querySelectorAll('.drop-hi').forEach(item=>item.classList.toggle('drop-hi',item===target));});
calendarSection.addEventListener('dragleave',event=>{const target=event.target.closest('.calcell[data-date],.wk-daycol[data-date]');if(target&&!target.contains(event.relatedTarget))target.classList.remove('drop-hi');});
calendarSection.addEventListener('drop',event=>{const target=event.target.closest('.calcell[data-date],.wk-daycol[data-date]');if(!target)return;event.preventDefault();target.classList.remove('drop-hi');let payload;try{payload=JSON.parse(event.dataTransfer.getData('application/x-truemail-event'));}catch(_){return;}let destination=target.dataset.date;if(target.classList.contains('wk-daycol')){const match=destination.match(/^(\d{4})-(\d{2})-(\d{2})$/),date=new Date(+match[1],+match[2]-1,+match[3]),minutes=Math.max(calendarHourStartMinutes,Math.min(calendarHourEndMinutes-15,calendarHourStartMinutes+Math.round(((event.clientY-target.getBoundingClientRect().top)/WK_HOUR*60)/15)*15));date.setMinutes(minutes);destination=date.toISOString();}window.calendarDropJustHappened=true;setTimeout(()=>{window.calendarDropJustHappened=false;},100);window.prepareCalendarEventMove?.(payload.id,payload.start,destination);});

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
function accountNavIsOpen(accountId){try{const saved=JSON.parse(localStorage.getItem('account_nav_open')||'{}');return saved[String(accountId)]!==false;}catch(_){return true;}}
function saveAccountNavOpen(accountId,open){try{const saved=JSON.parse(localStorage.getItem('account_nav_open')||'{}');saved[String(accountId)]=open;localStorage.setItem('account_nav_open',JSON.stringify(saved));}catch(_){}}
document.addEventListener('click',event=>{const h=event.target.closest('.acc-h');if(!h)return;const open=h.classList.toggle('open');h.nextElementSibling?.classList.toggle('open',open);if(h.dataset.accountId)saveAccountNavOpen(h.dataset.accountId,open);});

/* collapsible sidebar groups */
document.querySelectorAll('.nav .navlabel').forEach(lbl=>{
  lbl.classList.add('clp');
  const chev=document.createElement('span');chev.className='clp-chev';chev.innerHTML=ic.down;lbl.insertBefore(chev,lbl.firstChild);
  lbl.addEventListener('click',e=>{ if(e.target.closest('.add'))return;
    lbl.classList.toggle('collapsed');const hide=lbl.classList.contains('collapsed');let el=lbl.nextElementSibling;
    while(el&&!el.classList.contains('navlabel')){ if(el.classList.contains('navitem')||el.classList.contains('acc-h')||el.classList.contains('acc-sub'))el.classList.toggle('grouphide',hide); el=el.nextElementSibling; } });
});

/* custom right-click menu (suppress browser default) */
const ctxmenu=document.getElementById('ctxmenu'),ctxsmart=document.getElementById('ctxsmart'),ctxfolder=document.getElementById('ctxfolder'),ctxcontact=document.getElementById('ctxcontact');
let contextFolder=null,contextFolderOpen=null,contextContact=null;
function posMenu(menu,e){menu.style.left=Math.min(e.clientX,window.innerWidth-244)+'px';menu.style.top=Math.min(e.clientY,window.innerHeight-330)+'px';menu.classList.add('open');}
document.addEventListener('contextmenu',e=>{if(e.target.closest('input,textarea,select,[contenteditable="true"]'))return;e.preventDefault();
  ctxmenu.classList.remove('open');ctxsmart.classList.remove('open');ctxfolder.classList.remove('open');ctxcontact.classList.remove('open');
  const msg=e.target.closest('.msg'),smart=e.target.closest('[data-smart-index]'),contactCard=e.target.closest('.ccard[data-contact-id]');
  if(msg){const id=Number(msg.dataset.messageId);activeMessage=messages.find(item=>item.id===id)||activeMessage;buildContextMenu();posMenu(ctxmenu,e);}else if(smart){ctxsmart.dataset.index=smart.dataset.smartIndex;posMenu(ctxsmart,e);}else if(contactCard){contextContact=coreContacts.find(contact=>contact.id===Number(contactCard.dataset.contactId))||null;if(contextContact){const hasEmail=Boolean(contextContact.emails?.[0]?.email);ctxcontact.querySelectorAll('[data-contact-action="compose"],[data-contact-action="copy"]').forEach(item=>item.classList.toggle('disabled',!hasEmail));posMenu(ctxcontact,e);}} });
document.addEventListener('click',()=>{ctxmenu.classList.remove('open');ctxsmart.classList.remove('open');ctxfolder.classList.remove('open');ctxcontact.classList.remove('open');});
[ctxsmart,ctxfolder,ctxcontact].forEach(m=>m.querySelectorAll('.tmi').forEach(i=>i.onclick=()=>m.classList.remove('open')));
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
  (smartIsEnglish()?[['flag','flag','Flag'],['raw','edit','View source'],['eml','download','Save as .eml'],['create-rule','filter','Create rule']]:[['flag','flag','Флажок'],['raw','edit','Исходный текст'],['eml','download','Сохранить как .eml'],['create-rule','filter','Создать правило']]).forEach(([act,icon,label])=>{const item=document.createElement('div');item.className='tmi';item.dataset.contextAction=act;item.innerHTML=`<i data-i="${icon}"></i>${label}`;ctxmenu.appendChild(item);});
  renderIcons(ctxmenu);
}
ctxmenu.addEventListener('click',async event=>{const item=event.target.closest('[data-context-action]');if(!item)return;ctxmenu.classList.remove('open');const action=item.dataset.contextAction;
  if(action==='raw'){openRawViewer(activeMessage?.id);return;}
  if(action==='eml'){saveMessageAsEml(activeMessage?.id);return;}
  if(action==='create-rule'){openRuleEditor(activeMessage);return;}
  if(action==='flag'){openFlagMenu(activeMessage,event);return;}
  executeToolbarAction(action);
});
ctxfolder.querySelectorAll('[data-folder-action]').forEach(item=>item.addEventListener('click',async()=>{if(item.classList.contains('disabled')||!contextFolder)return;const action=item.dataset.folderAction;if(action==='open'){contextFolderOpen?.();return;}if(action==='settings'){showView('settingsView');setSection('folders');return;}if(action==='rename'){const name=prompt(L('Новое имя папки','New folder name'),contextFolder.display_name);if(!name||name.trim()===contextFolder.display_name)return;try{await window.tm.renameFolder(contextFolder.id,name.trim());await window.reloadCoreData();showToast(L('Папка переименована на сервере','Folder renamed on the server'));}catch(error){showToast(error.message||String(error));}return;}if(action==='delete'){if(!confirm(L(`Удалить папку «${contextFolder.display_name}» на сервере? Письма внутри также будут удалены.`,`Delete the folder "${contextFolder.display_name}" on the server? Messages inside will also be deleted.`)))return;try{await window.tm.deleteFolder(contextFolder.id);await window.reloadCoreData();showToast(L('Папка удалена на сервере','Folder deleted on the server'));}catch(error){showToast(error.message||String(error));}}}));
ctxcontact.querySelectorAll('[data-contact-action]').forEach(item=>item.addEventListener('click',async()=>{if(item.classList.contains('disabled')||!contextContact)return;const action=item.dataset.contactAction,email=contextContact.emails?.[0]?.email;if(action==='edit'){openContactEditor(contextContact);return;}if(action==='compose'){resetComposer();setRecipients('compTo',[{name:contextContact.display_name||'',email}]);document.getElementById('compTitle').textContent=L('Новое письмо','New message');showView('composeView');await applyComposerSignature('new');return;}if(action==='copy'){try{await navigator.clipboard.writeText(email);showToast(L('Email скопирован','Email copied'));}catch(error){showToast(error.message||String(error));}return;}if(action==='delete'){if(!confirm(L(`Удалить контакт «${contextContact.display_name||email||''}»?`,`Delete contact "${contextContact.display_name||email||''}"?`)))return;try{await window.tm.deleteContact(contextContact.id);await window.reloadCoreData();showToast(L('Контакт удалён','Contact deleted'));}catch(error){showToast(error.message||String(error));}}}));

