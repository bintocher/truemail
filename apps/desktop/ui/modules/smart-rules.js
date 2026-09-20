// truemail UI module: smart-rules.js
/* calendar view switch + week/day render */
const calSection=document.getElementById('calSection');
document.querySelectorAll('#calViews button').forEach(b=>b.onclick=()=>{
  document.querySelectorAll('#calViews button').forEach(x=>x.classList.toggle('on',x===b));
  calSection.dataset.cv=b.dataset.cv;
  window.tm?.setSetting('calendar_view',b.dataset.cv).catch(console.error);
  if(b.dataset.cv==='month')renderCalendarData();else {renderWeekDay(visibleCalendarEvents());document.getElementById('calTitle').textContent=calendarTitleText(b.dataset.cv);}});
/* "Сегодня": возврат к текущей дате в том представлении, что выбрано сейчас */
function renderCalendarCursor(){const view=calSection.dataset.cv||'month';if(view==='month')renderCalendarData();else{renderWeekDay(visibleCalendarEvents());document.getElementById('calTitle').textContent=calendarTitleText(view);}}
const calTodayButton=document.getElementById('calToday');
if(calTodayButton)calTodayButton.onclick=()=>{calendarCursor=new Date();renderCalendarCursor();};
document.querySelectorAll('[data-cal-nav]').forEach(button=>button.onclick=()=>{const direction=button.dataset.calNav==='prev'?-1:1,view=calSection.dataset.cv||'month';if(view==='day')calendarCursor.setDate(calendarCursor.getDate()+direction);else if(view==='week')calendarCursor.setDate(calendarCursor.getDate()+7*direction);else {const day=calendarCursor.getDate();calendarCursor.setDate(1);calendarCursor.setMonth(calendarCursor.getMonth()+direction);calendarCursor.setDate(Math.min(day,new Date(calendarCursor.getFullYear(),calendarCursor.getMonth()+1,0).getDate()));}renderCalendarData();if(view!=='month')document.querySelector(`#calViews button[data-cv="${view}"]`)?.click();});

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
function openSmart(index=null){editingSmartIndex=index;const c=document.getElementById('conds');c.innerHTML='';const item=index===null?null:smartFolders[index];document.querySelector('#smartOverlay .mh h3').textContent=item?L('Изменить умную папку','Edit smart folder'):L('Новая умная папка','New smart folder');document.getElementById('smartCreate').lastChild.textContent=item?L(' Сохранить',' Save'):L(' Создать умную папку',' Create smart folder');document.getElementById('smartDelete').classList.toggle('hidden',!item||item.builtin);const nameInput=document.getElementById('smartName');
  // У встроенной папки поле показывает подпись по умолчанию подсказкой, а не
  // значением: пустое поле однозначно значит "подпись даёт локализация", и это
  // единственный способ вернуть её после переименования. Предзаполнение
  // значением такой разницы не давало - имя, набранное вручную и совпавшее с
  // подписью текущего языка, было не отличить от нетронутого поля.
  nameInput.value=item?.t||'';
  // Подсказку для обычной папки берём из каталога локализации, а не из литерала:
  // ключ smartNamePlaceholder остаётся живым, и правка каталога на неё влияет.
  updateSmartNamePlaceholder();selectedSmartIcon=item?.i||'star';updateSmartIconButton();(item?.groups?.length?item.groups:[{conditions:[{}]}]).forEach(group=>c.appendChild(conditionGroup(group)));renumberConditionGroups();smartOverlay.classList.add('open');updateSmartPreview();if(index!==null)loadSmartCoveragePage(index);}
// Подсказку поля имени переставляем и при смене языка: окно правки может быть
// открыто, и тогда она осталась бы на прежнем языке. Для обычной папки текст
// берём из каталога локализации, для встроенной - её подпись по умолчанию.
function updateSmartNamePlaceholder(){
  const input=document.getElementById('smartName');if(!input)return;
  const item=editingSmartIndex===null?null:smartFolders[editingSmartIndex];
  input.placeholder=item?.builtin?smartFolderTitle({...item,t:''}):(wizardText[wizardLocale]?.smartNamePlaceholder||L('Например: Важное от коллег','For example: Important from colleagues'));
}
window.updateSmartNamePlaceholder=updateSmartNamePlaceholder;
function closeSmart(){smartOverlay.classList.remove('open');}
document.getElementById('addSmart').onclick=(e)=>{e.stopPropagation();openSmart();};
document.getElementById('addCond').onclick=()=>{document.querySelector('#conds .cond-group:last-child')?.appendChild(condRow());updateSmartPreview();};
document.getElementById('addCondGroup').onclick=()=>{document.getElementById('conds').appendChild(conditionGroup());renumberConditionGroups();updateSmartPreview();};
document.getElementById('smartClose').onclick=closeSmart;
document.getElementById('smartCancel').onclick=closeSmart;
document.getElementById('smartCreate').onclick=()=>{const name=document.getElementById('smartName').value.trim(),groups=readSmartGroups();const editing=editingSmartIndex===null?null:smartFolders[editingSmartIndex];
  // Пустое имя разрешено только встроенной папке - там его заменит локализация.
  if(!name&&!editing?.builtin){showToast(L('Введите название умной папки','Enter a smart folder name'));return;}if(!groups.length||groups.some(group=>!group.conditions.length||group.conditions.some(condition=>!validSmartCondition(condition)))){showToast(L('Заполните все условия умной папки','Fill in all smart folder conditions'));return;}const previous=editing;
  const item={...(previous||{}),id:previous?.id||`custom-${Date.now()}`,builtin:Boolean(previous?.builtin),i:selectedSmartIcon,t:name,on:previous?.on??true,groups};if(editingSmartIndex===null)smartFolders.push(item);else smartFolders[editingSmartIndex]=item;renderSmartManagement();bindSmartNavigation();persistSmartFolders().then(()=>{if(currentSmartIndex===editingSmartIndex)filterSmart(editingSmartIndex);}).catch(error=>showToast(error));closeSmart();};
document.getElementById('smartDelete').onclick=async()=>{const folder=editingSmartIndex===null?null:smartFolders[editingSmartIndex];if(!folder||folder.builtin||!await confirmAction(L(`Удалить умную папку «${smartFolderTitle(folder)}»?`,`Delete the smart folder "${smartFolderTitle(folder)}"?`)))return;const activeId=smartFolders[currentSmartIndex]?.id;forgetSmartFolderState(folder.id);smartFolders.splice(editingSmartIndex,1);renderSmartManagement();bindSmartNavigation();persistSmartFolders().catch(error=>showToast(error));closeSmart();if(activeId===folder.id)filterSmart(0);};
smartOverlay.onclick=e=>{if(e.target===smartOverlay)closeSmart();};
/* Значок выбирается из закрытого перечня: тот же перечень принимает значок
   быстрого действия, и оба попадают в разметку окна (pin-message.md S-070,
   quick-steps.md S-005). */
const smartIconKeys=quickStepsModel.ICONS.filter(key=>ic[key]);
const smartIconsEl=document.getElementById('smartIcons');smartIconsEl.innerHTML=smartIconKeys.map(key=>`<span class="ic-pick" data-sel="${key}" title="${key}"><i data-i="${key}"></i></span>`).join('');renderIcons(smartIconsEl);
function updateSmartIconButton(){const i=document.querySelector('#smartIconButton i');i.dataset.i=selectedSmartIcon;i.innerHTML=ic[selectedSmartIcon]||ic.star;document.querySelectorAll('#smartIcons .ic-pick').forEach(p=>p.classList.toggle('on',p.dataset.sel===selectedSmartIcon));}
document.getElementById('smartIconButton').onclick=event=>{event.stopPropagation();if(!smartIconsEl.classList.contains('hidden')){closePopupMenus(['icon']);return;}openPopupMenu('icon','icon');smartIconsEl.classList.remove('hidden');};document.querySelectorAll('#smartIcons .ic-pick').forEach(p=>p.onclick=()=>{selectedSmartIcon=p.dataset.sel;updateSmartIconButton();smartIconsEl.classList.add('hidden');});

/* toolbar customizer */
const tbActions=[
  {k:'reply',t:'Ответить',en:'Reply',on:true},{k:'replyall',t:'Ответить всем',en:'Reply all',on:true},{k:'forward',t:'Переслать',en:'Forward',on:true},
  {k:'archive',t:'В архив',en:'Archive',on:true},{k:'trash',t:'Удалить',en:'Delete',on:true},{k:'spam',t:'В спам',en:'Spam',on:false},{k:'snooze',t:'Отложить',en:'Snooze',on:false},
  {k:'unread',t:'Непрочитанное',en:'Mark unread',i:'inbox',on:false},{k:'unsub',t:'Отписаться',en:'Unsubscribe',on:false},{k:'print',t:'Печать',en:'Print',on:false}];
const tbLabel=a=>smartIsEnglish()&&a.en?a.en:a.t;
const tbList=document.getElementById('tbList');
let quickStepSnapshot=[];
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
function appendQuickStepToolbarRow(step){const action={k:quickStepsModel.toolbarKey(step),t:step.name,en:step.name,i:quickStepsModel.normalizeIcon(step.icon),on:false,quick:true};tbActions.push(action);const row=document.createElement('div');row.className='tbrow off';row.draggable=true;row.dataset.action=action.k;row.dataset.quickStep='1';row.dataset.labels='text';row.innerHTML=`<span class="grip"><i data-i="grip"></i></span><i data-i="${escapeHtml(action.i)}"></i><span class="nm">${escapeHtml(step.name)}</span><button type="button" class="btn sm action-label-mode">${smartIsEnglish()?'Icon + text':'Значок + текст'}</button><span class="ord"><button class="iconbtn" data-dir="up"><i data-i="up"></i></button><button class="iconbtn" data-dir="down"><i data-i="down"></i></button></span><div class="toggle"></div>`;renderIcons(row);const save=()=>{applyToolbar();persistToolbar();};row.querySelector('[data-dir="up"]').onclick=()=>{const previous=row.previousElementSibling;if(previous)tbList.insertBefore(row,previous);save();};row.querySelector('[data-dir="down"]').onclick=()=>{const next=row.nextElementSibling;if(next)tbList.insertBefore(next,row);save();};row.querySelector('.action-label-mode').onclick=event=>{event.stopPropagation();row.dataset.labels=row.dataset.labels==='icons'?'text':'icons';event.currentTarget.textContent=row.dataset.labels==='icons'?(smartIsEnglish()?'Icon only':'Только значок'):(smartIsEnglish()?'Icon + text':'Значок + текст');save();};row.querySelector('.toggle').onclick=event=>{event.stopPropagation();event.currentTarget.classList.toggle('on');row.classList.toggle('off',!event.currentTarget.classList.contains('on'));save();};tbList.appendChild(row);}
function applyQuickStepToolbarSet(layout){
  const builtin=[...tbList.children].filter(row=>!row.dataset.quickStep).map(row=>({id:row.dataset.action,visible:!row.classList.contains('off'),labels:row.dataset.labels||'text'}));
  quickStepsModel.toolbarSet(builtin,quickStepSnapshot,layout).forEach(item=>{const row=tbList.querySelector(`[data-action="${item.id}"]`);if(!row)return;row.classList.toggle('off',!item.visible);row.querySelector('.toggle')?.classList.toggle('on',item.visible);row.dataset.labels=item.labels||'text';tbList.appendChild(row);});
}
/* Редактор быстрого действия: имя, значок, вид действия и его цель выбираются
   списками. Ввод номера папки строкой уносил письма в чужую папку одной
   опечаткой, а свободная строка значка попадала в разметку окна, у которого
   есть весь мост команд (S-004, S-005, S-012, S-013). */
function quickStepFolderOptions(){
  const groups=(coreAccounts||[]).map(account=>{
    const folders=ruleAccountFolders(account.id).map(folder=>`<option value="folder:${folder.id}">${escapeHtml(folderTitle(folder))}</option>`).join('');
    return folders?`<optgroup label="${escapeHtml(account.email)}">${folders}</optgroup>`:'';
  }).join('');
  const roles=[['inbox',L('Тип: Входящие','Type: Inbox')],['archive',L('Тип: Архив','Type: Archive')],['spam',L('Тип: Спам','Type: Spam')],['trash',L('Тип: Корзина','Type: Trash')]];
  return groups+roles.filter(([id])=>quickStepsModel.FOLDER_ROLES.includes(id)).map(([id,title])=>`<option value="role:${id}">${escapeHtml(title)}</option>`).join('');
}
function renderQuickStepTarget(row,action){
  const meta=mailRulesModel.ruleAction(action.kind),host=row.querySelector('.quick-step-target');host.innerHTML='';
  if(meta?.needs==='folder'){
    const select=document.createElement('select');select.className='sel quick-step-folder';select.innerHTML=quickStepFolderOptions();
    const wanted=action.folder_id?`folder:${action.folder_id}`:action.folder_role?`role:${action.folder_role}`:'';
    if(wanted&&select.querySelector(`option[value="${wanted}"]`))select.value=wanted;
    host.appendChild(select);
  }else if(meta?.needs==='label'){
    const select=document.createElement('select');select.className='sel quick-step-label';select.innerHTML=(coreTags||[]).map(tag=>`<option value="${tag.id}">${escapeHtml(tag.name)}</option>`).join('');
    if(action.label_id&&select.querySelector(`option[value="${action.label_id}"]`))select.value=String(action.label_id);
    host.appendChild(select);
  }
}
function readQuickStepActionRow(row){
  const kind=row.querySelector('.quick-step-kind').value,action={kind,folder_id:null,folder_role:null,label_id:null};
  const folder=row.querySelector('.quick-step-folder'),label=row.querySelector('.quick-step-label');
  if(folder&&folder.value.startsWith('folder:'))action.folder_id=Number(folder.value.slice(7));
  else if(folder&&folder.value.startsWith('role:'))action.folder_role=folder.value.slice(5);
  if(label&&label.value)action.label_id=Number(label.value);
  return action;
}
function quickStepActionRow(source,host){
  const action=Object.assign({kind:'mark_read'},source||{}),row=document.createElement('div');row.className='rule-action-row quick-step-action';
  row.innerHTML=`<span class="grip"><i data-i="grip"></i></span><select class="sel quick-step-kind">${quickStepsModel.availableActions().map(item=>`<option value="${escapeHtml(item.id)}">${escapeHtml(mailRulesModel.ruleText(item,ruleLang()))}</option>`).join('')}</select><div class="quick-step-target"></div><span class="ord"><button type="button" class="iconbtn" data-dir="up"><i data-i="up"></i></button><button type="button" class="iconbtn" data-dir="down"><i data-i="down"></i></button></span><button type="button" class="del iconbtn" title="${escapeHtml(L('Удалить действие','Delete action'))}"><i data-i="trash"></i></button>`;
  const kind=row.querySelector('.quick-step-kind');if(row.querySelector(`option[value="${action.kind}"]`))kind.value=action.kind;
  kind.onchange=()=>renderQuickStepTarget(row,{...readQuickStepActionRow(row),kind:kind.value});
  renderQuickStepTarget(row,action);
  row.querySelector('[data-dir="up"]').onclick=()=>{const previous=row.previousElementSibling;if(previous)host.insertBefore(row,previous);};
  row.querySelector('[data-dir="down"]').onclick=()=>{const next=row.nextElementSibling;if(next)host.insertBefore(next,row);};
  row.querySelector('.del').onclick=()=>{if(host.children.length>1)row.remove();};
  renderIcons(row);return row;
}
function openQuickStepEditor(source){
  const step=quickStepsModel.normalizeQuickStep(source||{}),overlay=document.createElement('div');overlay.className='overlay open';
  overlay.innerHTML=`<div class="modal task-modal"><div class="mh"><i data-i="star"></i><h3>${escapeHtml(L('Быстрое действие','Quick step'))}</h3><button class="iconbtn x" type="button"><i data-i="close"></i></button></div><div class="mb"><label class="template-field">${escapeHtml(L('Название','Name'))}<input class="inp quick-step-name" maxlength="40"></label><label class="template-field">${escapeHtml(L('Значок','Icon'))}<select class="sel quick-step-icon">${quickStepsModel.ICONS.map(icon=>`<option value="${escapeHtml(icon)}">${escapeHtml(icon)}</option>`).join('')}</select></label><div class="quick-step-actions"></div><button type="button" class="btn sm quick-step-action-add">${escapeHtml(L('Добавить действие','Add action'))}</button></div><div class="mf"><span class="sp"></span><button class="btn quick-step-cancel">${escapeHtml(L('Отмена','Cancel'))}</button><button class="btn primary quick-step-save">${escapeHtml(L('Сохранить','Save'))}</button></div></div>`;
  document.body.appendChild(overlay);renderIcons(overlay);
  const close=()=>overlay.remove(),host=overlay.querySelector('.quick-step-actions'),name=overlay.querySelector('.quick-step-name'),icon=overlay.querySelector('.quick-step-icon');
  name.value=step.name;icon.value=step.icon;
  (step.actions.length?step.actions:[{kind:'mark_read'}]).forEach(action=>host.appendChild(quickStepActionRow(action,host)));
  overlay.querySelector('.quick-step-action-add').onclick=()=>{if(host.children.length<(quickStepsModel.maxActions()??Infinity))host.appendChild(quickStepActionRow({kind:'mark_read'},host));else showToast(quickStepsModel.quickStepErrorText('actions_limit',ruleLang()));};
  overlay.querySelector('.quick-step-save').onclick=async()=>{
    const input={id:step.id,name:name.value.trim(),icon:quickStepsModel.normalizeIcon(icon.value),sort_order:step.sort_order,hotkey_slot:step.hotkey_slot,actions:[...host.querySelectorAll('.quick-step-action')].map(readQuickStepActionRow)};
    // Цель сверяется с настоящими папками и метками программы до записи.
    const check=quickStepsModel.validateQuickStep(input,{folders:coreFolders,labels:coreTags||[]});
    if(!check.ok){showToast(quickStepsModel.quickStepErrorText(check.reason,ruleLang()));return;}
    try{await window.tm.saveQuickStep(input);close();await reloadQuickSteps();persistToolbar();}catch(error){showToast(error);}
  };
  overlay.querySelectorAll('.x,.quick-step-cancel').forEach(button=>button.onclick=close);overlay.onclick=event=>{if(event.target===overlay)close();};name.focus();
}
function renderQuickStepList(){const host=document.getElementById('quickStepList');if(!host)return;host.innerHTML='';quickStepSnapshot.forEach(step=>{const row=document.createElement('div');row.className='rule-row';row.innerHTML=`<div class="rule-row-main"><div class="rule-row-title"></div><div class="rule-row-description"></div></div><button class="btn sm quick-step-edit">${escapeHtml(L('Изменить','Edit'))}</button><button class="btn sm quick-step-slot"></button><button class="iconbtn quick-step-delete"><i data-i="trash"></i></button>`;row.querySelector('.rule-row-title').textContent=step.name;row.querySelector('.rule-row-description').textContent=step.actions.map(action=>mailRulesModel.ruleText(mailRulesModel.ruleAction(action.kind||action.id),ruleLang())).join(' - ')+(step.state==='needs_attention'?` - ${L('требует выбрать цель','target required')}`:'');row.querySelector('.quick-step-edit').onclick=()=>openQuickStepEditor(step);row.querySelector('.quick-step-slot').textContent=step.hotkey_slot?L(`Слот ${step.hotkey_slot}`,`Slot ${step.hotkey_slot}`):L('Назначить слот','Assign slot');row.querySelector('.quick-step-slot').onclick=()=>openQuickStepSlotPicker(step);row.querySelector('.quick-step-delete').onclick=async()=>{if(!await confirmAction(L('Удалить быстрое действие?','Delete quick step?')))return;try{await window.tm.deleteQuickStep(step.id);await reloadQuickSteps();persistToolbar();}catch(error){showToast(error);}};host.appendChild(row);renderIcons(row);});if(!quickStepSnapshot.length)host.textContent=L('Быстрых действий пока нет','No quick steps yet');}
/* Слот выбирается списком свободных номеров: занятый слот принадлежит одному
   быстрому действию, и его имя названо прямо в перечне (S-066, S-067). */
function openQuickStepSlotPicker(step){
  const taken=new Map(quickStepSnapshot.filter(item=>item.hotkey_slot&&item.id!==step.id).map(item=>[Number(item.hotkey_slot),item.name]));
  const options=[`<option value="">${escapeHtml(L('Без слота','No slot'))}</option>`].concat(Array.from({length:10},(_,index)=>{
    const slot=index+1,owner=taken.get(slot);
    return `<option value="${slot}"${owner?' disabled':''}>${escapeHtml(owner?L(`Слот ${slot} - занят: ${owner}`,`Slot ${slot} - taken by ${owner}`):L(`Слот ${slot} (${quickStepsModel.proposedSlotCombo(slot)})`,`Slot ${slot} (${quickStepsModel.proposedSlotCombo(slot)})`))}</option>`;
  })).join('');
  const overlay=document.createElement('div');overlay.className='overlay open';
  overlay.innerHTML=`<div class="modal compact-modal"><div class="mh"><i data-i="keyboard"></i><h3>${escapeHtml(L('Слот горячей клавиши','Hotkey slot'))}</h3><button class="iconbtn x" type="button"><i data-i="close"></i></button></div><div class="mb"><label class="template-field">${escapeHtml(L('Слот','Slot'))}<select class="sel quick-step-slot-value">${options}</select></label></div><div class="mf"><span class="sp"></span><button class="btn quick-step-slot-cancel">${escapeHtml(L('Отмена','Cancel'))}</button><button class="btn primary quick-step-slot-save">${escapeHtml(L('Сохранить','Save'))}</button></div></div>`;
  document.body.appendChild(overlay);renderIcons(overlay);
  const close=()=>overlay.remove(),select=overlay.querySelector('.quick-step-slot-value');
  select.value=step.hotkey_slot?String(step.hotkey_slot):'';
  overlay.querySelector('.quick-step-slot-save').onclick=async()=>{try{await window.tm.bindQuickStepSlot(step.id,select.value?Number(select.value):null);close();await reloadQuickSteps();}catch(error){showToast(error);}};
  overlay.querySelectorAll('.x,.quick-step-slot-cancel').forEach(button=>button.onclick=close);overlay.onclick=event=>{if(event.target===overlay)close();};
}
async function reloadQuickSteps(){if(!window.tm?.listQuickSteps)return;const layout=toolbarState().actions.map(action=>({id:action.key,visible:action.visible,labels:action.labels}));quickStepSnapshot=await window.tm.listQuickSteps();tbList.querySelectorAll('[data-quick-step]').forEach(row=>row.remove());for(let index=tbActions.length-1;index>=0;index--)if(tbActions[index].quick)tbActions.splice(index,1);quickStepSnapshot.forEach(appendQuickStepToolbarRow);applyQuickStepToolbarSet(layout);renderQuickStepList();document.querySelectorAll('#ctxmenu .quick-step-context').forEach(item=>item.remove());quickStepSnapshot.forEach(step=>{const item=document.createElement('div');item.className='tmi quick-step-context';item.dataset.contextAction=`quick-step:${step.id}`;item.innerHTML=`<i data-i="${escapeHtml(quickStepsModel.normalizeIcon(step.icon))}"></i>${escapeHtml(step.name)}`;ctxmenu?.appendChild(item);});renderIcons(ctxmenu);applyToolbar();}
window.reloadQuickSteps=reloadQuickSteps;
// Предел числа быстрых действий спрашивается у реестра пределов. Прежде эта
// строка читала несуществующее поле модуля: сравнение с undefined всегда было
// ложным, предел не срабатывал ни разу, а в отказе стояло бы "undefined".
document.getElementById('quickStepAdd')?.addEventListener('click',()=>{const maxSteps=quickStepsModel.maxSteps();if(maxSteps!==null&&quickStepSnapshot.length>=maxSteps){showToast(L(`Предел быстрых действий: ${maxSteps}`,`Quick step limit: ${maxSteps}`));return;}openQuickStepEditor({sort_order:quickStepSnapshot.length});});
let draggedToolbarRow=null;tbList.addEventListener('dragstart',e=>{draggedToolbarRow=e.target.closest('.tbrow');});tbList.addEventListener('dragover',e=>{e.preventDefault();const row=e.target.closest('.tbrow');if(row&&draggedToolbarRow&&row!==draggedToolbarRow){const rect=row.getBoundingClientRect();tbList.insertBefore(draggedToolbarRow,e.clientY<rect.top+rect.height/2?row:row.nextSibling);}});tbList.addEventListener('drop',()=>{applyToolbar();persistToolbar();});
tbList.addEventListener('pointerdown',event=>{const grip=event.target.closest('.grip'),row=grip?.closest('.tbrow');if(!row||event.button!==0)return;event.preventDefault();draggedToolbarRow=row;row.classList.add('pointer-dragging');grip.setPointerCapture(event.pointerId);});
tbList.addEventListener('pointermove',event=>{if(!draggedToolbarRow)return;const target=document.elementFromPoint(event.clientX,event.clientY)?.closest('.tbrow');if(!target||target===draggedToolbarRow||target.parentElement!==tbList)return;const rect=target.getBoundingClientRect();tbList.insertBefore(draggedToolbarRow,event.clientY<rect.top+rect.height/2?target:target.nextSibling);});
tbList.addEventListener('pointerup',event=>{if(!draggedToolbarRow)return;event.target.closest('.grip')?.releasePointerCapture?.(event.pointerId);draggedToolbarRow.classList.remove('pointer-dragging');draggedToolbarRow=null;applyToolbar();persistToolbar();});
function toolbarState(){return {actions:[...tbList.children].map(row=>({key:row.dataset.action,visible:!row.classList.contains('off'),labels:row.dataset.labels||'text'})),align:document.querySelector('#toolbarAlign .on')?.dataset.align||'left'};}
function persistToolbar(){const state=toolbarState();window.tm?.setSetting('toolbar_layout',JSON.stringify(state)).catch(console.error);const quickIds=state.actions.map(action=>quickStepsModel.toolbarStepId(action.key)).filter(id=>id!==null);if(quickIds.length)window.tm?.reorderQuickSteps(quickIds).catch(console.error);}
function applyToolbar(){const state=toolbarState(),bar=document.querySelector('.thread .actions');if(!bar)return;bar.classList.toggle('toolbar-right',state.align==='right');bar.querySelectorAll('[data-toolbar-generated]').forEach(el=>el.remove());const anchor=bar.querySelector('.sp');state.actions.filter(action=>action.visible).forEach(action=>{const meta=tbActions.find(a=>a.k===action.key);if(!meta)return;const button=document.createElement('button');button.className=`${action.key==='reply'?'btn primary':'btn'}${action.labels==='icons'?' toolbar-action-icons':''}`;button.dataset.toolbarGenerated='1';button.dataset.act=action.key;button.title=tbLabel(meta);button.innerHTML=`<i data-i="${escapeHtml(meta.i||action.key)}"></i><span>${escapeHtml(tbLabel(meta))}</span>`;renderIcons(button);bar.insertBefore(button,anchor);});bar.querySelectorAll(':scope > button:not([data-toolbar-generated]):not([data-toolbar-persistent])').forEach(button=>button.classList.add('toolbar-original-hidden'));
  // Меню "Ещё" показывает только те действия, что скрыты из панели - без дублей.
  const menuDyn=document.getElementById('threadMenuDynamic');
  if(menuDyn){menuDyn.innerHTML='';quickStepsModel.toolbarMoreMenu(state.actions).forEach(action=>{const meta=tbActions.find(a=>a.k===action.key);if(!meta)return;const button=document.createElement('button');button.type='button';button.dataset.toolbarMenu=action.key;button.innerHTML=`<i data-i="${escapeHtml(meta.i||action.key)}"></i><span>${escapeHtml(tbLabel(meta))}</span>`;menuDyn.appendChild(button);});renderIcons(menuDyn);const sep=document.getElementById('threadMenuDynSep');if(sep)sep.style.display=menuDyn.children.length?'':'none';}}
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
  const apply=async date=>{try{await window.tm.snoozeMessages(ids,date.toISOString());clearMessageSelection();activeMessage=null;activeFullMessage=null;await window.reloadCoreData();close();showToast(L(`Отложено писем: ${ids.length}`,`Snoozed messages: ${ids.length}`),L('Отменить','Undo'),async()=>{await window.tm.unsnoozeMessages(ids);await window.reloadCoreData();});}catch(error){showToast(error);}};
  overlay.querySelector('[data-offset="hour"]').onclick=()=>apply(new Date(Date.now()+60*60*1000));overlay.querySelector('[data-offset="tomorrow"]').onclick=()=>{const d=new Date();d.setDate(d.getDate()+1);d.setHours(9,0,0,0);apply(d);};overlay.querySelector('[data-offset="monday"]').onclick=()=>apply(nextMondayMorning());overlay.querySelector('.snooze-apply').onclick=()=>{const d=new Date(custom.value);if(Number.isNaN(d.getTime())||d<=new Date()){showToast(L('Выберите будущее время','Choose a future time'));return;}apply(d);};overlay.querySelectorAll('.x,.snooze-cancel').forEach(button=>button.onclick=close);overlay.onclick=event=>{if(event.target===overlay)close();};custom.focus();}
async function exportActiveMessageEml(){if(!activeMessage){showToast(L('Сначала выберите письмо','Select a message first'));return;}const safe=(activeMessage.subject||'message').replace(/[<>:"/\\|?*\x00-\x1f]/g,'_').trim().slice(0,100)||'message';try{const path=await window.tm.saveFileDialog(`${safe}.eml`);if(!path)return;await window.tm.exportMessageEml(activeMessage.id,path);showToast(L('Письмо сохранено в .eml','Message saved as .eml'));}catch(error){showToast(error);}}
function quickStepRunContext(){
  const collapsedThread=conversationsEnabled&&activeMessage&&!expandedConversations.has(conversationKey(activeMessage))?activeMessage:null;
  return {bridge:window.tm,steps:quickStepSnapshot,selection:[...selectedMessageIds],activeMessage,collapsedThread,loaded:lastListRows.length?lastListRows:messages,inTextField:Boolean(document.activeElement?.matches?.('input,textarea,select,[contenteditable="true"]')),lang:wizardLocale,confirm:text=>confirmAction(text),notify:showToast,onApplied:async(report,ids)=>{window.forgetMessages?.(report.operation_ids?.length?ids:[]);await window.reloadCoreData?.();showToast(quickStepsModel.reportText(report,wizardLocale));if(report.operation_ids?.length)showToast(L('Перенос можно отменить. Метки и отметки цепочки не снимаются','The move can be undone. Labels and flags are not reverted'),L('Отменить','Undo'),async()=>{await window.tm.undoMessageAction(report.operation_ids);await window.reloadCoreData();});}};
}
async function executeQuickStep(id){const step=quickStepSnapshot.find(item=>item.id===Number(id));if(!step)return;try{return await quickStepsModel.runQuickStep(step,quickStepRunContext());}catch(error){showToast(error);return null;}}
window.executeQuickStep=executeQuickStep;
window.executeQuickStepSlot=slot=>quickStepsModel.handleSlotPress(`quick_step_${slot}`,quickStepRunContext()).catch(showToast);
async function executeToolbarAction(action){const toolbarStepId=quickStepsModel.toolbarStepId(action);if(toolbarStepId!==null){await executeQuickStep(toolbarStepId);return;}if(['reply','replyall','forward'].includes(action)){openComposerForMessage(action);return;}if(['archive','trash','spam'].includes(action)){performMessageAction(action);return;}if(action==='snooze'){openSnoozeDialog();return;}if(action==='unread'){if(activeMessage){const ids=window.expandConversationIds?window.expandConversationIds([activeMessage.id]):[activeMessage.id];await window.markMessagesSeen?.(ids.map(id=>messages.find(item=>item.id===id)).filter(Boolean),false);await window.reloadCoreData?.();showToast(L('Письмо отмечено непрочитанным','Message marked as unread'));}return;}if(action==='print'){const frame=document.querySelector('.mail-html-frame');if(frame?.contentWindow)frame.contentWindow.print();else window.print();return;}if(action==='unsub'){const uns=activeFullMessage?.unsubscribe;
  if(uns?.one_click_url){showToast(L('Отправляю запрос на отписку…','Sending unsubscribe request…'));const fallback=()=>window.tm?.openExternal(uns.http||uns.one_click_url).catch(error=>showToast(error));try{const status=await window.tm.unsubscribeOneClick(uns.one_click_url);if(status>=200&&status<300)showToast(L('Готово: вы отписаны от рассылки (сервер подтвердил, код '+status+')','Done: you have been unsubscribed (server confirmed, code '+status+')'));else{showToast(L('Сервер отписки ответил кодом '+status+'. Открываю страницу отписки…','The unsubscribe server responded with code '+status+'. Opening the unsubscribe page…'));fallback();}}catch(error){showToast(error);fallback();}return;}
  const target=uns?.http||embeddedUnsubscribeUrl(activeFullMessage);if(target){try{await window.tm.openExternal(target);showToast(L('Открыл страницу отписки в браузере — завершите отписку там','Opened the unsubscribe page in your browser — finish there'));}catch(error){showToast(error);}return;}
  const mailto=uns?.mailto;if(mailto){resetComposer();setRecipients('compTo',[String(mailto).replace(/^mailto:/i,'').split('?')[0]]);document.getElementById('compSubj').value=L('Отписаться','Unsubscribe');showView('composeView');showToast(L('Отправьте это письмо, чтобы отписаться','Send this message to unsubscribe'));return;}
  showToast(L('В письме нет ссылки для автоматической отписки','This message has no automatic unsubscribe link'));}}
document.querySelector('.thread .actions').addEventListener('click',e=>{const button=e.target.closest('[data-toolbar-generated]');if(button)executeToolbarAction(button.dataset.act);});
const threadMoreButton=document.getElementById('threadMoreButton'),threadMoreMenu=document.getElementById('threadMoreMenu');
function closeThreadMore(){closePopupMenus(['more']);}
threadMoreButton.onclick=event=>{event.stopPropagation();if(threadMoreMenu.classList.contains('open')){closeThreadMore();return;}openPopupMenu('more','more');threadMoreMenu.classList.add('open');threadMoreButton.setAttribute('aria-expanded','true');};
threadMoreMenu.onclick=async event=>{const toolbarItem=event.target.closest('[data-toolbar-menu]');if(toolbarItem){closeThreadMore();executeToolbarAction(toolbarItem.dataset.toolbarMenu);return;}const button=event.target.closest('[data-thread-action]');if(!button)return;closeThreadMore();const action=button.dataset.threadAction;if(action==='settings'){showView('settingsView');setSection('toolbar');return;}if(action==='rules'){showView('settingsView');setSection('rules');return;}if(action==='raw'){openRawViewer(activeMessage?.id);return;}if(action==='export-eml'){exportActiveMessageEml();return;}if(action==='create-rule'){openRuleEditor(activeMessage);return;}if(action==='unread'){if(activeMessage){const ids=window.expandConversationIds?window.expandConversationIds([activeMessage.id]):[activeMessage.id];await window.markMessagesSeen?.(ids.map(id=>messages.find(item=>item.id===id)).filter(Boolean),false);await window.reloadCoreData?.();showToast(L('Письмо отмечено непрочитанным','Message marked as unread'));}return;}if(['archive','trash'].includes(action))performMessageAction(action);};


/* Правила обработки почты: группы условий, группы исключений и цепочка
   действий. Словарь и проверки состава живут в modules/mail-rules.js, здесь
   только отрисовка и обращения к ядру. См.
   specs/mail-rules-conditions-and-actions.md. */
const ruleEditor=document.getElementById('ruleEditor'),ruleAccount=document.getElementById('ruleAccount');
const ruleGroupsHost=document.getElementById('ruleGroups'),ruleExceptionsHost=document.getElementById('ruleExceptions'),ruleActionsHost=document.getElementById('ruleActions');
const ruleLang=()=>smartIsEnglish()?'en':'ru';
const ruleFieldLabel=field=>mailRulesModel.ruleText(field,ruleLang());
const ruleOpLabel=id=>(mailRulesModel.RULE_OPS[id]||[id,id])[smartIsEnglish()?1:0];
let lastRuleRun=null,ruleListSnapshot=[];
function ruleAccountFolders(accountId){return coreFolders.filter(folder=>folder.account_id===accountId);}
/* Значение условия: перечисление, дата, размер или обычный текст. Разметка
   повторяет редактор умной папки, чтобы оба редактора выглядели одинаково. */
function renderRuleConditionValue(row,condition){
  const field=mailRulesModel.ruleField(condition.field),host=row.querySelector('.cond-value');host.className='cond-value';host.innerHTML='';
  if(field.type==='enum'){
    const select=document.createElement('select');select.className='cond-input';select.innerHTML=field.values.map(item=>`<option value="${item[0]}">${escapeHtml(mailRulesModel.ruleOptionText(item,ruleLang()))}</option>`).join('');select.value=field.values.some(item=>item[0]===condition.value)?condition.value:field.values[0][0];host.appendChild(select);
  }else if(field.type==='date'&&['within_last','older_than'].includes(condition.op)){
    host.classList.add('relative-date');const input=document.createElement('input');input.className='cond-input';input.type='number';input.min='1';input.step='1';input.value=/^\d+$/.test(condition.value)?condition.value:'24';
    const unit=document.createElement('select');unit.className='cond-unit';unit.innerHTML=mailRulesModel.RULE_DATE_UNITS.map(item=>`<option value="${item[0]}">${escapeHtml(mailRulesModel.ruleOptionText(item,ruleLang()))}</option>`).join('');unit.value=mailRulesModel.RULE_DATE_UNITS.some(item=>item[0]===condition.unit)?condition.unit:'hours';host.append(input,unit);
  }else if(field.type==='size'){
    host.classList.add('relative-date');const input=document.createElement('input');input.className='cond-input';input.type='number';input.min='0';input.step='0.1';input.value=/^\d+(?:\.\d+)?$/.test(condition.value)?condition.value:'10';host.appendChild(input);
    if(condition.op==='between'){const second=document.createElement('input');second.className='cond-max';second.type='number';second.min='0';second.step='0.1';second.value=/^\d+(?:\.\d+)?$/.test(condition.value2)?condition.value2:'50';host.appendChild(second);}
    const unit=document.createElement('select');unit.className='cond-unit';unit.innerHTML=mailRulesModel.RULE_SIZE_UNITS.map(item=>`<option value="${item[0]}">${escapeHtml(mailRulesModel.ruleOptionText(item,ruleLang()))}</option>`).join('');unit.value=mailRulesModel.RULE_SIZE_UNITS.some(item=>item[0]===condition.unit)?condition.unit:'mb';host.appendChild(unit);
  }else{
    const input=document.createElement('input');input.className='cond-input';input.type=field.type==='date'?'date':'text';input.placeholder=L('значение','value');input.value=condition.value||'';host.appendChild(input);
  }
}
function readRuleConditionRow(row){
  return mailRulesModel.normalizeRuleCondition({
    field:row.querySelector('.cond-field').value,
    op:row.querySelector('.cond-op').value,
    value:row.querySelector('.cond-input')?.value||'',
    value2:row.querySelector('.cond-max')?.value||'',
    unit:row.querySelector('.cond-unit')?.value,
  });
}
function ruleConditionRow(source={}){
  const condition=mailRulesModel.normalizeRuleCondition(source),row=document.createElement('div');row.className='cond';
  row.innerHTML=`<select class="cond-field">${mailRulesModel.RULE_FIELDS.map(field=>`<option value="${field.id}">${escapeHtml(ruleFieldLabel(field))}</option>`).join('')}</select><select class="cond-op"></select><div class="cond-value"></div><button type="button" class="del iconbtn" title="${escapeHtml(L('Удалить условие','Delete condition'))}"><i data-i="trash"></i></button>`;
  const fieldSelect=row.querySelector('.cond-field'),opSelect=row.querySelector('.cond-op');fieldSelect.value=condition.field;
  const rebuild=selected=>{const field=mailRulesModel.ruleField(fieldSelect.value),operators=mailRulesModel.ruleOperators(field.type);opSelect.innerHTML=operators.map(id=>`<option value="${id}">${escapeHtml(ruleOpLabel(id))}</option>`).join('');opSelect.value=operators.includes(selected)?selected:operators[0];renderRuleConditionValue(row,mailRulesModel.normalizeRuleCondition({field:field.id,op:opSelect.value,value:field.id===condition.field?condition.value:'',unit:condition.unit,value2:condition.value2}));};
  fieldSelect.onchange=()=>rebuild();opSelect.onchange=()=>rebuild(opSelect.value);rebuild(condition.op);
  row.querySelector('.del').onclick=()=>{row.remove();renumberRuleGroups();};renderIcons(row);return row;
}
function renumberRuleGroups(){
  [ruleGroupsHost,ruleExceptionsHost].forEach(host=>{
    host.querySelectorAll('.cond-group').forEach((group,index)=>{group.querySelector('.cond-group-title').textContent=`${L('Группа','Group')} ${index+1}`;});
  });
}
function ruleConditionGroup(source={conditions:[{}]}){
  const state=mailRulesModel.normalizeRuleGroup(source),group=document.createElement('div');group.className='cond-group';group.dataset.logic=state.logic;
  group.innerHTML=`<div class="cond-group-head"><span class="cond-group-title"></span><div class="logic"><button type="button" data-l="all">${escapeHtml(L('Все (И)','All (AND)'))}</button><button type="button" data-l="any">${escapeHtml(L('Любое (ИЛИ)','Any (OR)'))}</button></div><button type="button" class="btn sm cond-group-add">${escapeHtml(L('Условие','Condition'))}</button><button type="button" class="iconbtn cond-group-remove" title="${escapeHtml(L('Удалить группу','Delete group'))}"><i data-i="trash"></i></button></div>`;
  group.querySelectorAll('.logic button').forEach(button=>{button.classList.toggle('on',button.dataset.l===state.logic);button.onclick=()=>{group.dataset.logic=button.dataset.l;group.querySelectorAll('.logic button').forEach(item=>item.classList.toggle('on',item===button));};});
  (state.conditions.length?state.conditions:[{}]).forEach(condition=>group.appendChild(ruleConditionRow(condition)));
  group.querySelector('.cond-group-add').onclick=()=>{group.appendChild(ruleConditionRow());};
  group.querySelector('.cond-group-remove').onclick=()=>{group.remove();renumberRuleGroups();};
  renderIcons(group);return group;
}
/* Группы читаются как есть, включая пустые: отброшенная группа исключений
   молча сохраняла бы правило, которое уводит защищённые письма (S-018). */
function readRuleGroups(host){
  return [...host.querySelectorAll('.cond-group')].map(group=>({logic:group.dataset.logic==='any'?'any':'all',conditions:[...group.querySelectorAll('.cond')].map(readRuleConditionRow)}));
}
/* Дополнительное поле действия: папка назначения или метка. Правило для всех
   ящиков выбирает папку по её типу, а не по номеру (S-045). */
function renderRuleActionTarget(row,action){
  const meta=mailRulesModel.ruleAction(action.kind),host=row.querySelector('.rule-action-target');host.innerHTML='';
  if(meta.needs==='folder'){
    const accountId=ruleAccount.value==='all'?null:Number(ruleAccount.value),select=document.createElement('select');select.className='sel rule-action-folder';
    if(accountId){select.innerHTML=ruleAccountFolders(accountId).map(folder=>`<option value="folder:${folder.id}">${escapeHtml(folderTitle(folder))}</option>`).join('');}
    const roles=[['inbox',L('Тип: Входящие','Type: Inbox')],['archive',L('Тип: Архив','Type: Archive')],['spam',L('Тип: Спам','Type: Spam')],['trash',L('Тип: Корзина','Type: Trash')]];
    select.innerHTML+=roles.map(([id,title])=>`<option value="role:${id}">${escapeHtml(title)}</option>`).join('');
    const wanted=action.folder_id?`folder:${action.folder_id}`:action.folder_role?`role:${action.folder_role}`:'';
    if(wanted&&select.querySelector(`option[value="${wanted}"]`))select.value=wanted;
    host.appendChild(select);
  }else if(meta.needs==='label'){
    const select=document.createElement('select');select.className='sel rule-action-label';select.innerHTML=(coreTags||[]).map(tag=>`<option value="${tag.id}">${escapeHtml(tag.name)}</option>`).join('');
    if(action.label_id&&select.querySelector(`option[value="${action.label_id}"]`))select.value=String(action.label_id);
    host.appendChild(select);
  }
}
function ruleActionRow(source={}){
  const action=mailRulesModel.normalizeRuleAction(source),row=document.createElement('div');row.className='rule-action-row';
  row.innerHTML=`<span class="grip"><i data-i="grip"></i></span><select class="sel rule-action-kind">${mailRulesModel.RULE_ACTIONS.map(item=>`<option value="${item.id}">${escapeHtml(mailRulesModel.ruleText(item,ruleLang()))}</option>`).join('')}</select><div class="rule-action-target"></div><span class="ord"><button type="button" class="iconbtn" data-dir="up"><i data-i="up"></i></button><button type="button" class="iconbtn" data-dir="down"><i data-i="down"></i></button></span><button type="button" class="del iconbtn" title="${escapeHtml(L('Удалить действие','Delete action'))}"><i data-i="trash"></i></button>`;
  const kind=row.querySelector('.rule-action-kind');kind.value=action.kind;
  // Смена вида действия и смена ящика сохраняют уже выбранную папку или
  // метку: молчаливый возврат к первому варианту терял выбор пользователя.
  kind.onchange=()=>renderRuleActionTarget(row,{...readRuleActionRow(row),kind:kind.value});
  renderRuleActionTarget(row,action);
  row.querySelector('[data-dir="up"]').onclick=()=>{const previous=row.previousElementSibling;if(previous)ruleActionsHost.insertBefore(row,previous);};
  row.querySelector('[data-dir="down"]').onclick=()=>{const next=row.nextElementSibling;if(next)ruleActionsHost.insertBefore(next,row);};
  row.querySelector('.del').onclick=()=>row.remove();
  renderIcons(row);return row;
}
function readRuleActionRow(row){
  const kind=row.querySelector('.rule-action-kind').value,target=row.querySelector('.rule-action-folder')?.value||'',label=row.querySelector('.rule-action-label')?.value||'';
  return mailRulesModel.normalizeRuleAction({
    kind,
    folder_id:target.startsWith('folder:')?Number(target.slice(7)):null,
    folder_role:target.startsWith('role:')?target.slice(5):null,
    label_id:label?Number(label):null,
  });
}
function readRuleActions(){
  return [...ruleActionsHost.querySelectorAll('.rule-action-row')].map(readRuleActionRow);
}
function editorRule(){
  const accountValue=ruleAccount.value;
  return {
    id:editingRuleId||`rule-${Date.now()}`,
    name:document.getElementById('ruleName').value.trim(),
    account_id:accountValue==='all'?null:Number(accountValue),
    enabled:mailRules.find(rule=>rule.id===editingRuleId)?.enabled??true,
    groups:readRuleGroups(ruleGroupsHost),
    exceptions:readRuleGroups(ruleExceptionsHost),
    actions:readRuleActions(),
  };
}
function openRuleEditor(source=null,rule=null){
  showView('settingsView');setSection('rules');editingRuleId=rule?.id??null;ruleEditor.classList.remove('hidden');
  document.getElementById('ruleEditorTitle').textContent=rule?.id?L('Изменить правило','Edit rule'):L('Новое правило','New rule');
  ruleAccount.innerHTML=`<option value="all">${escapeHtml(L('Все аккаунты','All accounts'))}</option>`+coreAccounts.map(account=>`<option value="${account.id}">${escapeHtml(account.email)}</option>`).join('');
  const sourceEmail=source?.from?.email||'';
  document.getElementById('ruleName').value=rule?.name||(sourceEmail?L(`Письма от ${sourceEmail}`,`Mail from ${sourceEmail}`):'');
  ruleAccount.value=String(rule?.account_id??source?.account_id??'all');
  ruleGroupsHost.innerHTML='';ruleExceptionsHost.innerHTML='';ruleActionsHost.innerHTML='';
  const groups=rule?.groups?.length?rule.groups:[{logic:'all',conditions:[sourceEmail?{field:'sender_address',op:'equals',value:sourceEmail}:{}]}];
  groups.forEach(group=>ruleGroupsHost.appendChild(ruleConditionGroup(group)));
  (rule?.exceptions||[]).forEach(group=>ruleExceptionsHost.appendChild(ruleConditionGroup(group)));
  (rule?.actions?.length?rule.actions:[{kind:'move'}]).forEach(action=>ruleActionsHost.appendChild(ruleActionRow(action)));
  renumberRuleGroups();
  document.getElementById('ruleExisting').checked=false;
  document.getElementById('ruleDelete').classList.toggle('hidden',!rule);
  document.getElementById('ruleName').focus();
}
function closeRuleEditor(){ruleEditor.classList.add('hidden');editingRuleId=null;}
function ruleDescription(rule){
  return mailRulesModel.ruleSummary(rule,{lang:ruleLang(),accounts:coreAccounts,folders:coreFolders,labels:coreTags||[]});
}
function renderRulesList(){
  // Имена правил задаёт пользователь: словарь автоперевода их не трогает.
  const list=document.getElementById('rulesList');list.dataset.noI18n='1';list.innerHTML='';
  if(lastRuleRun){const report=document.createElement('p');report.className='note-muted';report.textContent=`${L('Последний ручной прогон','Last manual run')}: ${mailRulesModel.runReportText(lastRuleRun,ruleLang())}`;list.appendChild(report);}
  if(!mailRules.length){const empty=document.createElement('p');empty.className='rule-empty';empty.textContent=L('Правил пока нет.','No rules yet.');list.appendChild(empty);return;}
  mailRules.forEach((rule,index)=>{
    const row=document.createElement('div');row.className='rule-row';row.dataset.ruleId=rule.id;row.draggable=true;
    row.innerHTML=`<span class="grip"><i data-i="grip"></i></span><div class="toggle${rule.enabled!==false?' on':''}" role="switch"></div><div class="rule-row-body"><div class="rule-row-title"></div><div class="rule-row-description"></div><div class="rule-row-state"></div></div><button type="button" class="btn sm"><i data-i="edit"></i>${escapeHtml(L('Изменить','Edit'))}</button>`;
    row.querySelector('.rule-row-title').textContent=`${index+1}. ${rule.name}`;
    row.querySelector('.rule-row-description').textContent=ruleDescription(rule);
    const state=row.querySelector('.rule-row-state');state.textContent=mailRulesModel.ruleStateText(rule,ruleLang());state.classList.toggle('hidden',!state.textContent);
    row.querySelector('.toggle').onclick=async()=>{try{await window.tm.setMailRuleEnabled(rule.id,!rule.enabled);await reloadMailRules();}catch(error){showToast(error);}};
    row.querySelector('button').onclick=()=>openRuleEditor(null,rule);
    list.appendChild(row);
  });
  renderIcons(list);
}
/* Перетаскивание правила меняет его порядковый номер: ядро получает полный
   перечень идентификаторов в новом порядке (S-059). */
let draggedRuleRow=null;
/* Новый порядок строк вычисляет mailRulesModel.moveRule: перестановка в
   списке и проверка порядка идут по одному и тому же коду (S-059). */
function placeRuleRow(list,target,before){
  if(!draggedRuleRow||target===draggedRuleRow)return;
  const rows=[...list.querySelectorAll('.rule-row')],ids=rows.map(row=>row.dataset.ruleId);
  const from=ids.indexOf(draggedRuleRow.dataset.ruleId);let to=ids.indexOf(target.dataset.ruleId);
  if(from<0||to<0)return;
  if(!before&&to<from)to+=1;
  if(before&&to>from)to-=1;
  const order=mailRulesModel.moveRule(ids,from,to);
  order.forEach(id=>{const row=rows.find(item=>item.dataset.ruleId===id);if(row)list.appendChild(row);});
}
function finishRuleDragging(event){
  if(!draggedRuleRow)return;
  event?.target?.closest?.('.grip')?.releasePointerCapture?.(event.pointerId);
  draggedRuleRow.classList.remove('pointer-dragging');
  draggedRuleRow=null;
  persistRuleOrder();
}
function bindRuleDragging(){
  const list=document.getElementById('rulesList');
  list.addEventListener('dragstart',event=>{draggedRuleRow=event.target.closest('.rule-row');});
  list.addEventListener('dragover',event=>{event.preventDefault();const row=event.target.closest('.rule-row');if(!row||!draggedRuleRow)return;const rect=row.getBoundingClientRect();placeRuleRow(list,row,event.clientY<rect.top+rect.height/2);});
  list.addEventListener('drop',event=>{event.preventDefault();finishRuleDragging(event);});
  // Перетаскивание, брошенное вне списка, тоже обязано завершиться: иначе
  // следующее движение указателя продолжило бы переставлять строки.
  list.addEventListener('dragend',event=>finishRuleDragging(event));
  list.addEventListener('pointerdown',event=>{const grip=event.target.closest('.grip'),row=grip?.closest('.rule-row');if(!row||event.button!==0)return;event.preventDefault();draggedRuleRow=row;row.classList.add('pointer-dragging');grip.setPointerCapture(event.pointerId);});
  list.addEventListener('pointermove',event=>{if(!draggedRuleRow)return;const target=document.elementFromPoint(event.clientX,event.clientY)?.closest('.rule-row');if(!target||target.parentElement!==list)return;const rect=target.getBoundingClientRect();placeRuleRow(list,target,event.clientY<rect.top+rect.height/2);});
  list.addEventListener('pointerup',event=>finishRuleDragging(event));
  list.addEventListener('pointercancel',event=>finishRuleDragging(event));
}
async function persistRuleOrder(){
  const ids=[...document.querySelectorAll('#rulesList .rule-row')].map(row=>row.dataset.ruleId);
  if(!ids.length||ids.join()===mailRules.map(rule=>rule.id).join())return;
  try{await window.tm.reorderMailRules(ids);await reloadMailRules();showToast(L('Порядок правил сохранён','Rule order saved'));}catch(error){showToast(error);await reloadMailRules();}
}
async function reloadMailRules(){
  mailRules=await window.tm.listMailRules();
  ruleListSnapshot=mailRules.map(rule=>rule.id);
  try{lastRuleRun=await window.tm.lastMailRuleRun();}catch(_){lastRuleRun=null;}
  renderRulesList();
  renderFailedOperations();
  renderPendingRuleRuns();
  warnAboutMissingTrash();
}
/* S-052, S-053: операция, дошедшая до состояния отказа, держит письмо на
   месте. Выйти из этого состояния можно только решением пользователя:
   повторить операцию или отказаться от неё. */
async function renderFailedOperations(){
  const host=document.getElementById('ruleFailedOps');if(!host)return;
  host.innerHTML='';
  let failed=[];
  try{failed=await window.tm.failedMessageOperations();}catch(_){return;}
  if(!failed.length)return;
  const block=document.createElement('div');block.className='rule-failed-ops';
  const title=document.createElement('div');title.className='rule-failed-title';
  title.textContent=L('Операции с письмами, завершившиеся отказом','Message operations that ended in failure');
  block.appendChild(title);
  failed.forEach(operation=>{
    const row=document.createElement('div');row.className='rule-failed-row';
    const text=document.createElement('span');
    const kind=operation.op_kind==='delete'?L('удаление','deletion'):L('перемещение','move');
    text.textContent=`${kind}: ${operation.last_error||L('без объяснения сервера','no server explanation')}`;
    const retry=document.createElement('button');retry.type='button';retry.className='btn sm';retry.textContent=L('Повторить','Retry');
    retry.onclick=async()=>{try{await window.tm.retryMessageOperation(operation.id);await reloadMailRules();}catch(error){showToast(error);}};
    const discard=document.createElement('button');discard.type='button';discard.className='btn sm';discard.textContent=L('Отказаться','Discard');
    discard.onclick=async()=>{try{await window.tm.discardMessageOperation(operation.id);await reloadMailRules();}catch(error){showToast(error);}};
    row.append(text,retry,discard);block.appendChild(row);
  });
  host.appendChild(block);
}
/* S-070, S-072: незавершённое задание ручного прогона продолжается кнопкой в
   разделе правил, а не только из отчёта текущей сессии - иначе прерванный
   закрытием программы прогон некому было бы довести до конца. */
async function renderPendingRuleRuns(){
  const host=document.getElementById('rulePendingRuns');if(!host)return;
  host.innerHTML='';
  let runs=[];
  try{runs=await window.tm.pendingMailRuleRuns();}catch(_){return;}
  if(!runs.length)return;
  const block=document.createElement('div');block.className='rule-failed-ops';
  const title=document.createElement('div');title.className='rule-failed-title';
  title.textContent=L('Незавершённые прогоны правил','Unfinished rule runs');
  block.appendChild(title);
  runs.forEach(run=>{
    const row=document.createElement('div');row.className='rule-failed-row';
    const text=document.createElement('span');
    text.textContent=mailRulesModel.runReportText(run,ruleLang());
    const resume=document.createElement('button');resume.type='button';resume.className='btn sm';
    resume.textContent=L('Продолжить','Continue');
    resume.onclick=async()=>{try{showRuleRunReport(await window.tm.continueMailRuleRun(run.run_id));await reloadMailRules();}catch(error){showToast(error);}};
    row.append(text,resume);block.appendChild(row);
  });
  host.appendChild(block);
}
window.reloadMailRules=reloadMailRules;
document.getElementById('ruleNew').onclick=()=>openRuleEditor();
document.getElementById('ruleCancel').onclick=closeRuleEditor;
document.getElementById('ruleAddGroup').onclick=()=>{ruleGroupsHost.appendChild(ruleConditionGroup());renumberRuleGroups();};
document.getElementById('ruleAddException').onclick=()=>{ruleExceptionsHost.appendChild(ruleConditionGroup());renumberRuleGroups();};
document.getElementById('ruleAddAction').onclick=()=>{ruleActionsHost.appendChild(ruleActionRow());};
// Смена ящика меняет перечень папок назначения у действий перемещения.
ruleAccount.onchange=()=>{[...ruleActionsHost.querySelectorAll('.rule-action-row')].forEach(row=>renderRuleActionTarget(row,readRuleActionRow(row)));};
document.getElementById('ruleSave').onclick=async()=>{
  const rule=editorRule(),check=mailRulesModel.validateRule(rule);
  if(!check.ok){showToast(mailRulesModel.ruleErrorText(check.reason,ruleLang()));return;}
  const applyExisting=document.getElementById('ruleExisting').checked;
  try{
    // S-048: удаление навсегда подтверждается отдельно, с названием правила и
    // его областью, и ключ подтверждения выдаёт ядро.
    if(rule.actions.some(action=>action.kind==='delete')){
      const scope=rule.account_id?(coreAccounts.find(account=>account.id===rule.account_id)?.email||''):L('все аккаунты','all accounts');
      const question=L(`Правило "${rule.name}" (${scope}) будет удалять письма навсегда, без корзины и без отмены. Продолжить?`,`The rule "${rule.name}" (${scope}) will delete messages permanently, with no trash and no undo. Continue?`);
      if(!await confirmAction(question))return;
      rule.confirm_key=await window.tm.mailRuleDeleteConfirmation(rule);
    }
    await window.tm.saveMailRule(rule,applyExisting,ruleListSnapshot);
    await reloadMailRules();closeRuleEditor();showToast(L('Правило сохранено','Rule saved'));
    setTimeout(()=>window.reloadCoreData?.().catch(console.error),350);
  }catch(error){showToast(error);}
};
document.getElementById('ruleDelete').onclick=async()=>{const rule=mailRules.find(item=>item.id===editingRuleId);if(!rule||!await confirmAction(L(`Удалить правило "${rule.name}"?`,`Delete the rule "${rule.name}"?`)))return;try{await window.tm.deleteMailRule(rule.id);await reloadMailRules();closeRuleEditor();}catch(error){showToast(error);}};
/* Ручной прогон: выбранные папки, отчёт и продолжение по курсору задания
   (S-065, S-070, S-073). */
const ruleRunPanel=document.getElementById('ruleRunPanel'),ruleRunFolders=document.getElementById('ruleRunFolders'),ruleRunReport=document.getElementById('ruleRunReport'),ruleRunContinue=document.getElementById('ruleRunContinue');
const RULE_RUN_SKIPPED_ROLES=['sent','drafts','spam','trash'];
function renderRuleRunFolders(){
  ruleRunFolders.innerHTML='';
  coreAccounts.forEach(account=>{
    // Отправленные, черновики, спам и корзина не рабочие папки ни в одной
    // стадии: прогон по ним увёл бы почту, которую никто не получал.
    const folders=ruleAccountFolders(account.id).filter(folder=>!RULE_RUN_SKIPPED_ROLES.includes(folder.role));if(!folders.length)return;
    const block=document.createElement('div');block.className='rule-run-account';
    block.innerHTML=`<div class="rule-run-account-title">${escapeHtml(account.email)}</div>`;
    folders.forEach(folder=>{
      const label=document.createElement('label');label.className='aux-check';
      label.innerHTML=`<input type="checkbox" data-account="${account.id}" value="${folder.id}"> <span></span>`;
      label.querySelector('span').textContent=folderTitle(folder);
      block.appendChild(label);
    });
    ruleRunFolders.appendChild(block);
  });
}
document.getElementById('ruleRunOpen').onclick=()=>{renderRuleRunFolders();ruleRunReport.textContent='';ruleRunContinue.classList.add('hidden');ruleRunPanel.classList.remove('hidden');};
document.getElementById('ruleRunClose').onclick=()=>ruleRunPanel.classList.add('hidden');
function showRuleRunReport(report){
  lastRuleRun=report;
  ruleRunReport.textContent=`${L('Прогон завершён','The run has finished')}: ${mailRulesModel.runReportText(report,ruleLang())}`;
  // Остаток писем означает, что прогон упёрся в предел запуска и продолжается
  // с сохранённого курсора задания (S-069, S-070).
  ruleRunContinue.classList.toggle('hidden',!report||report.remaining<=0);
  ruleRunContinue.dataset.runId=report?.run_id||'';
  renderRulesList();
}
document.getElementById('ruleRunStart').onclick=async()=>{
  const checked=[...ruleRunFolders.querySelectorAll('input:checked')];
  if(!checked.length){showToast(L('Выберите хотя бы одну папку','Choose at least one folder'));return;}
  const accounts=new Set(checked.map(input=>input.dataset.account));
  if(accounts.size>1){showToast(L('Выберите папки одного ящика','Choose folders of one mailbox'));return;}
  const accountId=Number([...accounts][0]),folderIds=checked.map(input=>Number(input.value));
  try{showRuleRunReport(await window.tm.runMailRules(accountId,folderIds,null));await reloadMailRules();}catch(error){showToast(error);}
};
ruleRunContinue.onclick=async()=>{
  const runId=Number(ruleRunContinue.dataset.runId||0);if(!runId)return;
  try{showRuleRunReport(await window.tm.continueMailRuleRun(runId));await reloadMailRules();}catch(error){showToast(error);}
};
bindRuleDragging();
/* S-083: смена языка перерисовывает не только список правил, но и открытый
   редактор с панелью прогона - иначе часть экрана осталась бы на прежнем
   языке до повторного открытия. */
function relocalizeRuleSection(){
  if(!ruleEditor.classList.contains('hidden')){
    const draft=editorRule(),existing=mailRules.find(rule=>rule.id===editingRuleId)||null;
    // Несохранённое правило остаётся новым: пустой идентификатор оставляет
    // заголовок "Новое правило" и не привязывает черновик к списку.
    openRuleEditor(null,existing?{...existing,...draft}:{...draft,id:null});
  }
  if(!ruleRunPanel.classList.contains('hidden')){
    renderRuleRunFolders();
    if(lastRuleRun)showRuleRunReport(lastRuleRun);
  }
  renderPendingRuleRuns();
  renderFailedOperations();
  warnAboutMissingTrash();
}
window.relocalizeRuleSection=relocalizeRuleSection;
/* S-014: у ящика без папки корзины стадии обработки и действия с письмами
   оставляют почту на месте, поэтому раздел правил говорит об этом прямо. */
async function warnAboutMissingTrash(){
  try{
    const ids=await window.tm.accountsWithoutTrash();
    const host=document.getElementById('ruleTrashWarnings');if(!host)return;
    host.innerHTML='';
    if(!ids.length)return;
    ids.forEach(id=>{
      const name=coreAccounts.find(account=>account.id===id)?.email||id;
      const warning=document.createElement('p');warning.className='note-muted rule-trash-warning';
      warning.textContent=L(`Ящик ${name}: не назначена папка корзины. Правила и действия с письмами оставят его почту на месте.`,`Mailbox ${name}: no trash folder assigned. Rules and message actions will leave its mail where it is.`);
      host.appendChild(warning);
    });
  }catch(_){/* Предупреждение не обязано мешать работе со списком правил. */}
}
window.warnAboutMissingTrash=warnAboutMissingTrash;

/* smart folders management list */
const builtinSmartDefaults=[
  {id:'all-inbox',builtin:true,i:'inbox',t:'',on:true,groups:[{logic:'all',conditions:[{f:'folder_role',o:'equals',v:'inbox'}]}]},
  {id:'all-important',builtin:true,i:'star',t:'',on:true,groups:[{logic:'all',conditions:[{f:'importance',o:'equals',v:'flagged'}]}]},
  {id:'all-sent',builtin:true,i:'send',t:'',on:true,groups:[{logic:'all',conditions:[{f:'folder_role',o:'equals',v:'sent'}]}]},
  {id:'all-drafts',builtin:true,i:'draft',t:'',on:true,groups:[{logic:'all',conditions:[{f:'folder_role',o:'equals',v:'drafts'}]},{logic:'all',conditions:[{f:'draft_state',o:'equals',v:'draft'}]}]},
  {id:'last-24-hours',builtin:true,i:'cal',t:'',on:true,groups:[{logic:'all',conditions:[{f:'date',o:'within_last',v:'24',u:'hours'}]}]},
  {id:'all-unread',builtin:true,i:'search',t:'',on:true,groups:[{logic:'all',conditions:[{f:'read_state',o:'equals',v:'unread'}]}]},
  {id:'with-attachments',builtin:true,i:'paperclip',t:'',on:true,groups:[{logic:'all',conditions:[{f:'attachment',o:'equals',v:'has'}]}]},
  {id:'awaiting-my-reply',builtin:true,i:'flag',t:'',on:true,groups:[{logic:'all',conditions:[{f:'folder_role',o:'equals',v:'inbox'},{f:'reply_state',o:'equals',v:'unanswered'}]}]},
];
// Имена, под которыми встроенные папки приходили из прежних сборок и старых
// баз: текущие ru/en и то, что стояло в первой миграции. Пустое имя у встроенной
// папки означает "пользователь его не менял" - тогда подпись берётся из
// локализации и следует за языком. Известные имена по умолчанию приводим к
// пустому один раз, при загрузке, а не при каждой отрисовке: иначе имя,
// набранное пользователем и случайно совпавшее с подписью другого языка,
// навсегда считалось бы подписью по умолчанию.
// Только те имена, которые программа записывала сама: подписи по умолчанию
// прежних сборок. Английских подписей и коротких форм здесь нет - в набор они
// могли попасть лишь через переименование, а чужое имя стирать нельзя.
const builtinSmartDefaultNames={
  'all-inbox':['Все входящие'],
  'all-important':['Все важные'],
  'all-sent':['Все отправленные'],
  'all-drafts':['Все черновики'],
  'last-24-hours':['Сегодня','Сегодня (за 24 часа)'],
  'all-unread':['Непрочитанные (все)'],
  'with-attachments':['С вложениями'],
  'awaiting-my-reply':['Ждут ответа'],
};
function isBuiltinDefaultName(id,name){return (builtinSmartDefaultNames[id]||[]).includes(String(name||'').trim());}
const cloneSmart=value=>JSON.parse(JSON.stringify(value));
// legacy=true - разбор набора из настройки smart_folders_ui, оставшейся от
// сборок без стабильных идентификаторов: там встроенную папку узнать можно
// только по имени, иконке и месту в списке. Данные из ядра приходят со
// стабильным id, и угадывать по имени для них нельзя: пользовательская папка,
// названная "Сегодня", получила бы чужой идентификатор и слилась бы со
// встроенной, а имя, набранное вручную, стёрлось бы как подпись по умолчанию.
function normalizedSmartFolders(saved,legacy=false){
  if(!Array.isArray(saved))return cloneSmart(builtinSmartDefaults);const unused=new Map(builtinSmartDefaults.map(folder=>[folder.id,folder])),result=[];
  saved.forEach((raw,index)=>{if(!raw||typeof raw!=='object')return;let base=builtinSmartDefaults.find(folder=>folder.id===raw.id);
    if(!base&&legacy)base=builtinSmartDefaults.find(folder=>unused.has(folder.id)&&isBuiltinDefaultName(folder.id,raw.t));
    if(!base&&legacy&&index<8)base=builtinSmartDefaults.find(folder=>unused.has(folder.id)&&folder.i===raw.i);
    if(!base&&legacy&&index<8)base=builtinSmartDefaults.find(folder=>unused.has(folder.id));
    if(base)unused.delete(base.id);
    const groups=Array.isArray(raw.groups)&&raw.groups.some(group=>(Array.isArray(group)?group:group?.conditions)?.length)?raw.groups.map(normalizeSmartGroup):cloneSmart(base?.groups||[]);const item={...cloneSmart(base||{}),...raw,id:base?.id||raw.id||`custom-${Date.now()}-${index}`,builtin:Boolean(base||raw.builtin),on:raw.on!==false,groups};
    // Имена по умолчанию из старого набора приводим к пустому здесь: в базе то
    // же самое один раз делает миграция 0037.
    if(base&&legacy&&isBuiltinDefaultName(base.id,item.t))item.t='';
    result.push(item);});
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
// Пустое имя у встроенной папки означает "пользователь его не менял": подпись
// берётся из локализации и следует за языком. Всё остальное - имя, заданное
// пользователем, и подменять его локализацией нельзя, иначе переименование
// встроенной папки не доходит до боковой панели и списка настроек. Известные
// имена по умолчанию приводит к пустому normalizedSmartFolders при загрузке.
function smartFolderTitle(folder){if(folder&&folder.builtin&&builtinSmartTitles[folder.id]&&!String(folder.t||'').trim())return builtinSmartTitles[folder.id][smartIsEnglish()?'en':'ru'];return folder?.t||'';}
/* Счётчик писем умной папки в боковой панели: режим на папку ('u', 't', 'ut',
   'n'), сами числа считает ядро - локально в памяти лежит лишь часть писем. */
function smartCounterMode(folder){return smartCounterModes[folder?.id]||'n';}
function smartCountBadge(folder){const mode=smartCounterMode(folder),showU=mode.includes('u'),showT=mode.includes('t');if(!showU&&!showT)return '';const counts=smartCounts[folder?.id];if(!counts)return '';const unread=counts.unread||0,total=counts.total||0;if(showU&&showT)return `${unread}/${total}`;if(showT)return String(total);return unread>0?String(unread):'';}
function smartNavRow(folder){return [...document.querySelectorAll('.nav [data-smart-id]')].find(row=>row.dataset.smartId===folder?.id)||null;}
function updateSmartBadge(row,folder){if(!row)return;let badge=row.querySelector('.count');const text=smartCountBadge(folder);if(text){if(!badge){badge=document.createElement('span');badge.className='count';row.appendChild(badge);}badge.textContent=text;}else if(badge)badge.remove();}
function updateSmartBadges(){smartFolders.forEach(folder=>updateSmartBadge(smartNavRow(folder),folder));}
// Один пересчёт - это проход ядра по всей базе писем: своей таблицы у умной
// папки нет, число берётся перебором. Синхронизация шлёт "данные изменились"
// пачками, поэтому фоновые проходы разводим во времени - не чаще одного раза в
// SMART_COUNTS_MIN_INTERVAL и никогда параллельно. Полминуты выбраны по цене
// прохода: на большом ящике он читает всю таблицу писем, а счётчик в панели -
// подсказка, которой точность до секунды не нужна. Действия пользователя
// (включил счётчик, изменил условия папки) ждать этой паузы не должны и идут с
// immediate.
const SMART_COUNTS_MIN_INTERVAL=30000;
let smartCountsRunning=false,smartCountsQueued=false,smartCountsQueuedImmediate=false,smartCountsLastRun=0,smartCountsTimer=null;
async function runSmartCounts(ids){
  smartCountsRunning=true;
  // Помечаем заказанными, а не посчитанными: папку, которой ядро не вернуло
  // число, иначе считали бы новой на каждом заходе, и пауза не действовала бы.
  ids.forEach(id=>smartCountsRequested.add(id));
  try{const rows=await window.tm?.countSmartFolderMessages(ids)||[];const next={};rows.forEach(row=>{next[row.id]={total:row.total,unread:row.unread};});smartCounts=next;updateSmartBadges();}
  catch(error){console.error('smart folder counts',error);}
  // Очередь помнит, была ли она набрана действием пользователя: иначе запрос,
  // пришедший во время идущего прохода, терял бы своё право на обход паузы и
  // ждал бы её целиком - ровно тогда, когда пользователь смотрит на счётчик.
  finally{smartCountsRunning=false;smartCountsLastRun=Date.now();if(smartCountsQueued){const wasImmediate=smartCountsQueuedImmediate;smartCountsQueued=false;smartCountsQueuedImmediate=false;refreshSmartCounts(wasImmediate);}}
}
function refreshSmartCounts(immediate=false){
  const ids=smartFolders.filter(folder=>folder.on!==false&&smartCounterMode(folder)!=='n').map(folder=>folder.id);
  // Счётчиков не осталось - убирать подписи надо сразу, ждать тут нечего.
  if(!ids.length){if(smartCountsTimer){clearTimeout(smartCountsTimer);smartCountsTimer=null;}smartCountsQueued=false;smartCountsQueuedImmediate=false;smartCounts={};updateSmartBadges();return;}
  if(smartCountsRunning){smartCountsQueued=true;smartCountsQueuedImmediate=smartCountsQueuedImmediate||immediate;return;}
  // Пауза бережёт от повторного счёта одного и того же набора. Папку, для
  // которой числа ещё не заказывали, она задерживать не должна: иначе счётчик
  // пользовательской папки пустует полминуты после каждого запуска.
  const awaited=ids.some(id=>!smartCountsRequested.has(id));
  const wait=immediate||awaited?0:SMART_COUNTS_MIN_INTERVAL-(Date.now()-smartCountsLastRun);
  if(wait>0){if(!smartCountsTimer)smartCountsTimer=setTimeout(()=>{smartCountsTimer=null;refreshSmartCounts();},wait);return;}
  if(smartCountsTimer){clearTimeout(smartCountsTimer);smartCountsTimer=null;}
  runSmartCounts(ids);
}
window.refreshSmartCounts=refreshSmartCounts;
// Галочки в контекстном меню умной папки ставим при его открытии: режим мог
// смениться в другом меню или прийти из настроек.
window.syncSmartContextMenu=function(index){
  const folder=smartFolders[index];if(!folder)return;const mode=smartCounterMode(folder);
  ctxsmart.querySelector('[data-smart-action="count-unread"]')?.classList.toggle('on',mode.includes('u'));
  ctxsmart.querySelector('[data-smart-action="count-total"]')?.classList.toggle('on',mode.includes('t'));
};
function messagesTitle(){return smartIsEnglish()?'Messages':'Письма';}
function smartFolderToCore(folder,index){return {id:String(folder.id),name:folder.t||'',icon:folder.i||null,is_builtin:Boolean(folder.builtin),enabled:folder.on!==false,sort_order:index,groups:(folder.groups||[]).map(group=>{const normalized=normalizeSmartGroup(group);return {logic:normalized.logic,conditions:normalized.conditions.map(condition=>({field:condition.f,op:condition.o,value:String(condition.v??''),unit:condition.u||null,value2:condition.v2||null}))};})};}
function smartFolderFromCore(folder){return {id:String(folder.id),builtin:Boolean(folder.is_builtin),i:folder.icon||'star',t:folder.name||'',on:folder.enabled!==false,groups:(folder.groups||[]).map(group=>({logic:group.logic==='any'?'any':'all',conditions:(group.conditions||[]).map(condition=>({f:condition.field,o:condition.op,v:String(condition.value??''),...(condition.unit?{u:condition.unit}:{}),...(condition.value2!=null?{v2:String(condition.value2)}:{})}))}))};}
// Любая правка набора меняет и то, что описывают счётчики: условия папки, её
// видимость, состав списка. Пересчёт висит здесь, а не на каждой ветке правки -
// иначе следующая мутация снова забыла бы его позвать, и в панели осталось бы
// число, посчитанное по прежним условиям.
function persistSmartFolders(){
  const saved=window.tm?.saveSmartFolders(smartFolders.map(smartFolderToCore))||Promise.resolve();
  // Подписи по уже известным числам поправляем сразу (выключенная папка теряет
  // счётчик мгновенно), а пересчёт просим после записи: условия читает ядро из
  // базы, и проход до сохранения посчитал бы прежние.
  updateSmartBadges();
  saved.then(()=>refreshSmartCounts(true)).catch(()=>{});
  return saved;
}
function moveSmartFolder(from,to){if(to<0||to>=smartFolders.length)return;const activeId=smartFolders[currentSmartIndex]?.id;[smartFolders[from],smartFolders[to]]=[smartFolders[to],smartFolders[from]];if(activeId)currentSmartIndex=smartFolders.findIndex(folder=>folder.id===activeId);renderSmartManagement();bindSmartNavigation();persistSmartFolders().catch(error=>showToast(error));}
function renderSmartManagement(){smartListEl.innerHTML='';smartFolders.forEach((a,index)=>{const r=document.createElement('div');r.className='tbrow smart-list-row'+(a.on?'':' off');
  r.innerHTML=`<span class="grip"><i data-i="grip"></i></span><i data-i="${a.i}"></i><span class="smart-name"><span class="nm"></span><span class="smart-summary"></span></span><button class="btn sm edit-sf">${smartIsEnglish()?'Edit':'Изменить'}</button><span class="ord"><button class="iconbtn" data-dir="up"><i data-i="up"></i></button><button class="iconbtn" data-dir="down"><i data-i="down"></i></button></span><div class="toggle${a.on?' on':''}"></div>`;
  r.querySelector('.nm').dataset.noI18n='1';r.querySelector('.nm').textContent=smartFolderTitle(a);r.querySelector('.smart-summary').textContent=smartFolderDescription(a);renderIcons(r);
  r.querySelector('[data-dir="up"]').onclick=()=>moveSmartFolder(index,index-1);
  r.querySelector('[data-dir="down"]').onclick=()=>moveSmartFolder(index,index+1);
  r.querySelector('.edit-sf').onclick=()=>openSmart(index);
  r.querySelector('.toggle').onclick=(e)=>{e.stopPropagation();const t=e.currentTarget;t.classList.toggle('on');a.on=t.classList.contains('on');r.classList.toggle('off',!a.on);bindSmartNavigation();persistSmartFolders().catch(error=>showToast(error));};
  smartListEl.appendChild(r);});}
renderSmartManagement();
document.getElementById('smartNew2').onclick=()=>openSmart();
function bindSmartNavigation(){document.querySelectorAll('.custom-smart').forEach(row=>row.remove());const nav=document.querySelector('.nav'),accountLabel=nav.querySelector('[data-navlabel="accounts"]')||[...nav.querySelectorAll('.navlabel')].find(label=>label.textContent.includes('Аккаунты'));smartFolders.forEach((folder,index)=>{let row=folder.builtin?nav.querySelector(`[data-smart-id="${folder.id}"]`):null;if(!row){row=document.createElement('button');row.type='button';row.className='navitem custom-smart';row.dataset.nav='mail';row.innerHTML='<i></i><span class="smart-label"></span>';}
    row.dataset.smartIndex=index;row.dataset.smartId=folder.id;const icon=row.querySelector('i');icon.dataset.i=folder.i;icon.innerHTML=ic[folder.i]||ic.star;const label=row.querySelector('.smart-label');label.dataset.noI18n='1';label.textContent=smartFolderTitle(folder);updateSmartBadge(row,folder);row.style.display=folder.on?'':'none';row.onclick=()=>{clearMessageSelection();goMail();document.querySelectorAll('.navitem').forEach(item=>item.classList.remove('active'));row.classList.add('active');filterSmart(index);};accountLabel.before(row);});}
bindSmartNavigation();
ctxsmart.querySelector('[data-smart-action="open"]').onclick=()=>filterSmart(+ctxsmart.dataset.index);
ctxsmart.querySelector('[data-smart-action="edit"]').onclick=()=>openSmart(+ctxsmart.dataset.index);
ctxsmart.querySelector('[data-smart-action="settings"]').onclick=()=>{showView('settingsView');setSection('smart');};
ctxsmart.querySelectorAll('[data-smart-action="count-unread"],[data-smart-action="count-total"]').forEach(item=>item.onclick=()=>{
  const folder=smartFolders[+ctxsmart.dataset.index];if(!folder)return;
  const key=item.dataset.smartAction==='count-total'?'t':'u',modes=new Set(smartCounterMode(folder).split('').filter(part=>part==='u'||part==='t'));
  modes.has(key)?modes.delete(key):modes.add(key);
  smartCounterModes[folder.id]=['u','t'].filter(part=>modes.has(part)).join('')||'n';
  window.tm?.setSetting('smart_counters',JSON.stringify(smartCounterModes)).catch(console.error);
  item.classList.toggle('on',modes.has(key));
  // Сначала показываем то, что уже посчитано (или убираем подпись), потом
  // просим ядро пересчитать: иначе снятая галочка убирала бы счётчик лишь
  // через паузу между проходами.
  updateSmartBadges();refreshSmartCounts(true);
});

const auxOverlay=document.getElementById('auxOverlay'),eventForm=document.getElementById('eventForm'),contactForm=document.getElementById('contactForm');
let editingEvent=null,editingContact=null;
function closeAuxEditor(){auxOverlay.classList.remove('open');editingEvent=null;editingContact=null;}
function fillAccountSelect(select,selected){select.innerHTML='';coreAccounts.forEach(account=>{const option=document.createElement('option');option.value=account.id;option.textContent=account.email;select.appendChild(option);});if(selected!=null)select.value=String(selected);}
function fillCalendarSelect(accountId,selected){const select=document.getElementById('eventCalendar');select.innerHTML='';(coreCalendarData.calendars||[]).filter(calendar=>calendar.account_id===Number(accountId)).forEach(calendar=>{const option=document.createElement('option');option.value=calendar.id;option.textContent=calendar.name;select.appendChild(option);});if(selected!=null)select.value=String(selected);}
function localDateValue(value){const date=parseDavDate(value);if(!date)return '';const p=n=>String(n).padStart(2,'0');return `${date.getFullYear()}-${p(date.getMonth()+1)}-${p(date.getDate())}T${p(date.getHours())}:${p(date.getMinutes())}`;}
function remoteDateValue(value,allDay){if(allDay)return String(value).slice(0,10);const date=new Date(value);return Number.isNaN(date.getTime())?value:date.toISOString();}
function eventLineValues(value){return String(value||'').split(/[\n,]/).map(item=>item.trim()).filter(Boolean);}
function attendeeLabel(attendee){return attendee.name?`${attendee.name} <${attendee.email}>`:attendee.email;}
// --- Кликабельные настройки события (#14): конструктор повторения, чипы дат,
// список участников, пресеты напоминаний вместо сырых текстовых полей. ---
// rruleExtra и rruleRaw хранят части правила повторения, которых нет в форме
// (BYMONTHDAY, BYSETPOS, WKST и прочее). Без них сохранение события пересобирало
// RRULE из одних видимых полей и молча затирало правило на сервере.
const eventFormState={exdates:[],rdates:[],attendees:[],alarms:new Set(),rruleExtra:[],rruleRaw:null,untilRaw:null};
const RECUR_WEEKDAYS=[['MO','Пн','Mon'],['TU','Вт','Tue'],['WE','Ср','Wed'],['TH','Чт','Thu'],['FR','Пт','Fri'],['SA','Сб','Sat'],['SU','Вс','Sun']];
const ALARM_PRESETS=[[0,'В момент начала','At start'],[5,'За 5 минут','5 min before'],[10,'За 10 минут','10 min before'],[30,'За 30 минут','30 min before'],[60,'За час','1 hour before'],[1440,'За день','1 day before']];
function updateRecurrenceUi(){const freq=document.getElementById('eventRecurrence').value;document.querySelector('.event-recur-options').classList.toggle('hidden',!freq);document.querySelector('.event-recur-weekdays').classList.toggle('hidden',freq!=='WEEKLY');const units={DAILY:['дн.','days'],WEEKLY:['нед.','weeks'],MONTHLY:['мес.','months'],YEARLY:['лет','years']}[freq]||['',''];document.querySelector('.event-recur-unit').textContent=smartIsEnglish()?units[1]:units[0];const end=document.getElementById('eventRecurEnd').value;document.querySelector('.event-recur-until-field').classList.toggle('hidden',end!=='until');document.querySelector('.event-recur-count-field').classList.toggle('hidden',end!=='count');}
function renderRecurWeekdays(selected=[]){const host=document.getElementById('eventRecurWeekdays');host.innerHTML='';RECUR_WEEKDAYS.forEach(([code,ru,en])=>{const button=document.createElement('button');button.type='button';button.className='weekday-btn'+(selected.includes(code)?' on':'');button.dataset.wd=code;button.textContent=smartIsEnglish()?en:ru;button.onclick=()=>button.classList.toggle('on');host.appendChild(button);});}
function buildEventRrule(){const freq=document.getElementById('eventRecurrence').value;
  // Форма не показывает правило целиком. Если частота не из поддерживаемых, но
  // правило было, - возвращаем исходное, иначе оно потерялось бы при сохранении.
  if(!freq)return eventFormState.rruleRaw||null;
  const parts=[`FREQ=${freq}`];const interval=Math.max(1,Number(document.getElementById('eventRecurInterval').value)||1);if(interval>1)parts.push(`INTERVAL=${interval}`);if(freq==='WEEKLY'){const days=[...document.querySelectorAll('#eventRecurWeekdays .weekday-btn.on')].map(button=>button.dataset.wd);if(days.length)parts.push(`BYDAY=${days.join(',')}`);}
  parts.push(...eventFormState.rruleExtra);
  const end=document.getElementById('eventRecurEnd').value;if(end==='until'){const date=document.getElementById('eventRecurUntilDate').value;
    // Время окончания берём из исходного правила, пока пользователь не сменил
    // саму дату: иначе UNTIL каждый раз сбрасывался на полночь.
    if(date){const compact=date.replace(/-/g,'');parts.push(`UNTIL=${eventFormState.untilRaw&&eventFormState.untilRaw.startsWith(compact)?eventFormState.untilRaw:`${compact}T000000Z`}`);}}
  else if(end==='count'){parts.push(`COUNT=${Math.max(1,Number(document.getElementById('eventRecurCountNum').value)||1)}`);}
  return parts.join(';');}
function parseEventRrule(rrule){const map={};String(rrule||'').split(';').forEach(part=>{const[key,value]=part.split('=');if(key)map[key.toUpperCase()]=value;});const freq=map.FREQ||'';
  // Всё, чего нет в форме, сохраняем как есть и дописываем обратно при записи.
  // BYDAY показывается только для недельного повторения - в остальных случаях
  // ("второй вторник месяца") он тоже уходит в неизменяемую часть.
  eventFormState.rruleRaw=String(rrule||'')||null;
  eventFormState.untilRaw=map.UNTIL||null;
  const shown=freq==='WEEKLY'?['FREQ','INTERVAL','BYDAY','UNTIL','COUNT']:['FREQ','INTERVAL','UNTIL','COUNT'];
  eventFormState.rruleExtra=Object.entries(map).filter(([key,value])=>!shown.includes(key)&&value!=null).map(([key,value])=>`${key}=${value}`);document.getElementById('eventRecurrence').value=['DAILY','WEEKLY','MONTHLY','YEARLY'].includes(freq)?freq:'';document.getElementById('eventRecurInterval').value=map.INTERVAL||'1';renderRecurWeekdays(String(map.BYDAY||'').split(',').map(item=>item.trim()).filter(Boolean));const end=document.getElementById('eventRecurEnd'),until=document.getElementById('eventRecurUntilDate'),count=document.getElementById('eventRecurCountNum');if(map.UNTIL){end.value='until';const match=map.UNTIL.match(/^(\d{4})(\d{2})(\d{2})/);until.value=match?`${match[1]}-${match[2]}-${match[3]}`:'';count.value='10';}else if(map.COUNT){end.value='count';count.value=map.COUNT;until.value='';}else{end.value='never';count.value='10';until.value='';}updateRecurrenceUi();}
function formatChipDate(value){const date=parseDavDate(value);return date?date.toLocaleDateString(document.documentElement.lang):value;}
function renderDateChips(hostId,list){const host=document.getElementById(hostId);host.innerHTML='';list.forEach((value,index)=>{const chip=document.createElement('span');chip.className='chip';const label=document.createElement('span');label.textContent=formatChipDate(value);const remove=document.createElement('button');remove.type='button';remove.className='chip-x';remove.textContent='×';remove.onclick=()=>{list.splice(index,1);renderDateChips(hostId,list);};chip.append(label,remove);host.appendChild(chip);});}
function renderAttendeeChips(){const host=document.getElementById('eventAttendeesChips');host.innerHTML='';eventFormState.attendees.forEach((attendee,index)=>{const chip=document.createElement('span');chip.className='chip';const label=document.createElement('span');label.textContent=attendeeLabel(attendee);const remove=document.createElement('button');remove.type='button';remove.className='chip-x';remove.textContent='×';remove.onclick=()=>{eventFormState.attendees.splice(index,1);renderAttendeeChips();};chip.append(label,remove);host.appendChild(chip);});}
function renderAlarmChips(){const host=document.getElementById('eventAlarmsChips');host.innerHTML='';ALARM_PRESETS.forEach(([minutes,ru,en])=>{const button=document.createElement('button');button.type='button';button.className='chip toggle-chip'+(eventFormState.alarms.has(minutes)?' on':'');button.textContent=smartIsEnglish()?en:ru;button.onclick=()=>{if(eventFormState.alarms.has(minutes))eventFormState.alarms.delete(minutes);else eventFormState.alarms.add(minutes);renderAlarmChips();};host.appendChild(button);});}
// Пользователь сам сменил частоту - прежние тонкие части правила (день месяца,
// порядковый номер недели) к новой частоте не относятся.
document.getElementById('eventRecurrence').onchange=()=>{eventFormState.rruleExtra=[];eventFormState.rruleRaw=null;eventFormState.untilRaw=null;updateRecurrenceUi();};
document.getElementById('eventRecurEnd').onchange=updateRecurrenceUi;
const addDateChip=(inputId,list,hostId)=>{const input=document.getElementById(inputId),value=input.value;if(!value)return;const remote=remoteDateValue(value,true);if(!list.includes(remote)){list.push(remote);renderDateChips(hostId,list);}input.value='';};
document.getElementById('eventExdateAddBtn').onclick=()=>addDateChip('eventExdateAdd',eventFormState.exdates,'eventExdatesChips');
document.getElementById('eventRdateAddBtn').onclick=()=>addDateChip('eventRdateAdd',eventFormState.rdates,'eventRdatesChips');
document.getElementById('eventAttendeeAddBtn').onclick=()=>{const input=document.getElementById('eventAttendeeAdd'),raw=input.value.trim();if(!raw)return;const match=raw.match(/^(.*?)\s*<([^<>]+)>$/),email=(match?.[2]||raw).trim();if(!email.includes('@')){showToast(L('Введите корректный email','Enter a valid email'));return;}if(!eventFormState.attendees.some(attendee=>attendee.email.toLowerCase()===email.toLowerCase()))eventFormState.attendees.push({email,name:match?.[1]?.trim()||null,role:'REQ-PARTICIPANT',partstat:'NEEDS-ACTION',rsvp:true});renderAttendeeChips();input.value='';};
function eventInputFromForm(source){const allDay=document.getElementById('eventAllDay').checked;return{summary:document.getElementById('eventSummary').value.trim(),description:document.getElementById('eventDescription').value.trim()||null,location:document.getElementById('eventLocation').value.trim()||null,dtstart:remoteDateValue(document.getElementById('eventStart').value,allDay),dtend:document.getElementById('eventEnd').value?remoteDateValue(document.getElementById('eventEnd').value,allDay):null,all_day:allDay,attendees:eventFormState.attendees.map(attendee=>({...attendee})),alarms:[...eventFormState.alarms].map(trigger_minutes=>{const previous=(source?.alarms||[]).find(alarm=>alarm.trigger_minutes===trigger_minutes);return{trigger_minutes,action:previous?.action||'DISPLAY'};}),rrule:buildEventRrule(),recurrence_id:source?.recurrence_id||null,exdates:eventFormState.exdates.join(',')||null,rdates:eventFormState.rdates.join(',')||null,timezone:document.getElementById('eventTimezone').value.trim()||null,transp:document.getElementById('eventTransp').value||null,class:document.getElementById('eventClass').value||null,categories:eventLineValues(document.getElementById('eventCategories').value),url:document.getElementById('eventUrl').value.trim()||null,organizer:document.getElementById('eventOrganizer').value.trim()||null,sequence:Number(source?.sequence||0)};}
function setAuxMode(mode){const isEvent=mode==='event';eventForm.classList.toggle('hidden',!isEvent);contactForm.classList.toggle('hidden',isEvent);document.getElementById('auxTitle').textContent=isEvent?(editingEvent?L('Изменить событие / задачу','Edit event / task'):L('Новое событие / задача','New event / task')):(editingContact?L('Изменить контакт','Edit contact'):L('Новый контакт','New contact'));document.getElementById('auxIcon').innerHTML=isEvent?ic.cal:ic.people;}
function openEventEditor(event=null){editingEvent=event;editingContact=null;setAuxMode('event');fillAccountSelect(document.getElementById('eventAccount'),event?coreCalendarData.calendars.find(calendar=>calendar.id===event.calendar_id)?.account_id:coreAccounts[0]?.id);fillCalendarSelect(document.getElementById('eventAccount').value,event?.calendar_id);document.getElementById('eventAccount').disabled=Boolean(event);document.getElementById('eventCalendar').disabled=Boolean(event);const editingExisting=Boolean(event);document.getElementById('eventAccountField').classList.toggle('hidden',editingExisting);document.getElementById('eventCalendarField').classList.toggle('hidden',editingExisting);const sourceText=document.getElementById('eventSourceText');sourceText.classList.toggle('hidden',!editingExisting);if(editingExisting){const calendar=coreCalendarData.calendars.find(item=>item.id===event.calendar_id);const account=coreAccounts.find(item=>item.id===calendar?.account_id);sourceText.querySelector('.event-source-line').textContent=[account?.email,calendar?.name||calendar?.display_name].filter(Boolean).join(' · ');}document.getElementById('eventSummary').value=event?.summary||'';const start=event?.dtstart?localDateValue(event.dtstart):localDateValue(new Date(Math.ceil(Date.now()/1800000)*1800000).toISOString());document.getElementById('eventStart').value=start;document.getElementById('eventEnd').value=event?.dtend?localDateValue(event.dtend):localDateValue(new Date(new Date(start).getTime()+3600000).toISOString());document.getElementById('eventAllDay').checked=Boolean(event?.all_day)||/^\d{4}-\d{2}-\d{2}$/.test(event?.dtstart||'')||/^\d{8}$/.test(event?.dtstart||'');document.getElementById('eventLocation').value=event?.location||'';document.getElementById('eventDescription').value=event?.description||'';parseEventRrule(event?.rrule);eventFormState.exdates=String(event?.exdates||'').split(',').map(item=>item.trim()).filter(Boolean);eventFormState.rdates=String(event?.rdates||'').split(',').map(item=>item.trim()).filter(Boolean);eventFormState.attendees=(event?.attendees||[]).map(attendee=>({...attendee}));eventFormState.alarms=new Set((event?.alarms||[]).map(alarm=>alarm.trigger_minutes));renderDateChips('eventExdatesChips',eventFormState.exdates);renderDateChips('eventRdatesChips',eventFormState.rdates);renderAttendeeChips();renderAlarmChips();document.getElementById('eventOrganizer').value=event?.organizer||'';document.getElementById('eventTimezone').value=event?.timezone||Intl.DateTimeFormat().resolvedOptions().timeZone||'';document.getElementById('eventTransp').value=event?.transp||'opaque';document.getElementById('eventClass').value=event?.class||'public';document.getElementById('eventCategories').value=(event?.categories||[]).join(', ');document.getElementById('eventUrl').value=event?.url||'';document.querySelector('.event-advanced').open=Boolean(event&&(event.rrule||event.exdates||event.rdates||event.attendees?.length||event.alarms?.length||event.organizer||event.categories?.length||event.url));document.getElementById('eventDelete').classList.toggle('hidden',!event);document.getElementById('eventStatus').textContent='';const hasOwnStatus=Boolean(event&&event.my_partstat);document.getElementById('eventRsvpBox').classList.toggle('hidden',!hasOwnStatus);document.getElementById('eventRsvpActions').classList.toggle('hidden',!(event&&event.needs_response));if(hasOwnStatus){document.getElementById('eventRsvpStatus').textContent=rsvpStatusText(event.my_partstat);updateRsvpButtons(event.my_partstat);}auxOverlay.classList.add('open');document.getElementById('eventSummary').focus();}
// Текст своего статуса участия и подсветка активной кнопки ответа - см.
// resolve_my_attendance в ядре (my_partstat приходит уже вычисленным).
function rsvpStatusText(partstat){switch(String(partstat||'NEEDS-ACTION').toUpperCase()){case 'ACCEPTED':return L('Вы приняли приглашение','You accepted the invitation');case 'DECLINED':return L('Вы отклонили приглашение','You declined the invitation');case 'TENTATIVE':return L('Вы ответили «возможно»','You replied "maybe"');case 'DELEGATED':return L('Ответ делегирован другому участнику','Response delegated to another attendee');default:return L('Ответ ещё не отправлен','No response sent yet');}}
function updateRsvpButtons(partstat){const value=String(partstat||'').toUpperCase();document.getElementById('eventRsvpAccept').classList.toggle('primary',value==='ACCEPTED');document.getElementById('eventRsvpTentative').classList.toggle('primary',value==='TENTATIVE');document.getElementById('eventRsvpDecline').classList.toggle('primary',value==='DECLINED');}
// Ответ на приглашение из карточки события: шлём на сервер, перечитываем
// данные и обновляем статус на месте - форму не закрываем, в отличие от
// карточки своего уведомления (там кнопки одноразовые, здесь можно передумать).
async function respondToEditingEvent(response){if(!editingEvent)return;const status=document.getElementById('eventStatus');status.textContent=L('Отправляю ответ…','Sending your response…');status.dataset.kind='';try{await window.tm.respondToEvent(editingEvent.id,response);await window.reloadCoreData();const updated=(coreCalendarData.events||[]).find(item=>item.id===editingEvent.id);if(updated){editingEvent=updated;document.getElementById('eventRsvpStatus').textContent=rsvpStatusText(updated.my_partstat);updateRsvpButtons(updated.my_partstat);}status.textContent=L('Ответ отправлен','Response sent');status.dataset.kind='';}catch(error){status.textContent=window.errorPresentation.errorText(error,{locale:wizardLocale,translations:wizardText});status.dataset.kind='error';}}
document.getElementById('eventRsvpAccept').onclick=()=>respondToEditingEvent('accepted');
document.getElementById('eventRsvpTentative').onclick=()=>respondToEditingEvent('tentative');
document.getElementById('eventRsvpDecline').onclick=()=>respondToEditingEvent('declined');
function addContactPhoneRow(phone={}){const row=document.createElement('div');row.className='contact-phone-row';row.innerHTML=`<select class="sel contact-phone-kind"><option value="mobile">${L('Мобильный','Mobile')}</option><option value="work">${L('Рабочий','Work')}</option><option value="home">${L('Домашний','Home')}</option><option value="fax">${L('Факс','Fax')}</option><option value="other">${L('Другой','Other')}</option></select><input class="inp contact-phone-number" type="tel" placeholder="+7 999 000-00-00"><input class="inp contact-phone-extension" inputmode="numeric" placeholder="${L('Добавочный','Extension')}"><button class="iconbtn" type="button" title="${L('Удалить','Delete')}"><i data-i="trash"></i></button>`;row.querySelector('.contact-phone-kind').value=phone.kind||'mobile';row.querySelector('.contact-phone-number').value=phone.number||'';row.querySelector('.contact-phone-extension').value=phone.extension||'';row.querySelector('button').onclick=()=>row.remove();document.getElementById('contactPhones').appendChild(row);renderIcons(row);}
function contactPhonesFromForm(){return [...document.querySelectorAll('.contact-phone-row')].map(row=>({number:row.querySelector('.contact-phone-number').value.trim(),kind:row.querySelector('.contact-phone-kind').value,extension:row.querySelector('.contact-phone-extension').value.trim()||null})).filter(phone=>phone.number);}
document.getElementById('contactAddPhone').onclick=()=>addContactPhoneRow();
// Почтовый адрес контакта: те же компоненты, что в ADR из vCard и в
// contacts:PhysicalAddress:* у Exchange (улица, город, регион, индекс, страна).
function addContactAddressRow(address={}){const row=document.createElement('div');row.className='contact-address-row';row.innerHTML=`<div class="contact-address-head"><select class="sel contact-address-kind"><option value="home">${L('Домашний','Home')}</option><option value="work">${L('Рабочий','Work')}</option><option value="other">${L('Другой','Other')}</option></select><button class="iconbtn" type="button" title="${L('Удалить','Delete')}"><i data-i="trash"></i></button></div><input class="inp contact-address-street" placeholder="${L('Улица, дом','Street address')}"><div class="contact-address-grid"><input class="inp contact-address-city" placeholder="${L('Город','City')}"><input class="inp contact-address-region" placeholder="${L('Регион','Region')}"><input class="inp contact-address-postal" placeholder="${L('Индекс','Postal code')}"><input class="inp contact-address-country" placeholder="${L('Страна','Country')}"></div>`;row.querySelector('.contact-address-kind').value=address.kind||'home';row.querySelector('.contact-address-street').value=address.street||'';row.querySelector('.contact-address-city').value=address.city||'';row.querySelector('.contact-address-region').value=address.region||'';row.querySelector('.contact-address-postal').value=address.postal_code||'';row.querySelector('.contact-address-country').value=address.country||'';row.querySelector('button').onclick=()=>row.remove();document.getElementById('contactAddresses').appendChild(row);renderIcons(row);}
function contactAddressesFromForm(){const value=(row,selector)=>row.querySelector(selector).value.trim()||null;return [...document.querySelectorAll('.contact-address-row')].map(row=>({kind:row.querySelector('.contact-address-kind').value,street:value(row,'.contact-address-street'),city:value(row,'.contact-address-city'),region:value(row,'.contact-address-region'),postal_code:value(row,'.contact-address-postal'),country:value(row,'.contact-address-country')})).filter(address=>address.street||address.city||address.region||address.postal_code||address.country);}
document.getElementById('contactAddAddress').onclick=()=>addContactAddressRow();
function openContactEditor(contact=null){editingContact=contact;editingEvent=null;setAuxMode('contact');fillAccountSelect(document.getElementById('contactAccount'),contact?.account_id||coreAccounts[0]?.id);document.getElementById('contactAccount').disabled=Boolean(contact);document.getElementById('contactDisplayName').value=contact?.display_name||'';document.getElementById('contactFirstName').value=contact?.first_name||'';document.getElementById('contactLastName').value=contact?.last_name||'';document.getElementById('contactOrganization').value=contact?.organization||'';document.getElementById('contactEmails').value=(contact?.emails||[]).map(item=>item.email).join('\n');document.getElementById('contactPhones').innerHTML='';(contact?.phones||[]).forEach(addContactPhoneRow);document.getElementById('contactAddresses').innerHTML='';(contact?.addresses||[]).forEach(addContactAddressRow);document.getElementById('contactDelete').classList.toggle('hidden',!contact);document.getElementById('contactStatus').textContent='';
  // Существующий контакт без серверной копии (is_local_only) - предупреждаем
  // заранее, а не когда сохранение молча ничего никуда не отправит.
  document.getElementById('contactLocalOnlyNotice').classList.toggle('hidden',!contact?.is_local_only);
  auxOverlay.classList.add('open');document.getElementById('contactDisplayName').focus();}
document.getElementById('eventAccount').onchange=e=>fillCalendarSelect(e.target.value);
document.getElementById('newEventBtn').onclick=()=>openEventEditor();
document.getElementById('newContactBtn').onclick=()=>openContactEditor();
document.getElementById('auxClose').onclick=closeAuxEditor;document.getElementById('eventCancel').onclick=closeAuxEditor;document.getElementById('contactCancel').onclick=closeAuxEditor;
auxOverlay.addEventListener('click',event=>{if(event.target===auxOverlay)closeAuxEditor();});
eventForm.onsubmit=async event=>{event.preventDefault();const status=document.getElementById('eventStatus'),input=eventInputFromForm(editingEvent);status.textContent=L('Сохраняю на сервере…','Saving to the server…');status.dataset.kind='';try{if(editingEvent)await window.tm.updateEvent(editingEvent.id,input);else await window.tm.createEvent(Number(document.getElementById('eventAccount').value),Number(document.getElementById('eventCalendar').value),input);await window.reloadCoreData();closeAuxEditor();showToast(L('Событие сохранено и синхронизировано','Event saved and synced'));}catch(error){status.textContent=String(error);status.dataset.kind='error';}};
document.getElementById('eventDelete').onclick=async()=>{if(!editingEvent||!await confirmAction(L('Удалить событие или задачу на сервере?','Delete this event or task on the server?')))return;const status=document.getElementById('eventStatus');status.textContent=L('Удаляю…','Deleting…');try{await window.tm.deleteEvent(editingEvent.id);await window.reloadCoreData();closeAuxEditor();showToast(L('Удалено на сервере','Deleted on the server'));}catch(error){status.textContent=String(error);status.dataset.kind='error';}};
contactForm.onsubmit=async event=>{event.preventDefault();const status=document.getElementById('contactStatus'),input={display_name:document.getElementById('contactDisplayName').value.trim(),first_name:document.getElementById('contactFirstName').value.trim()||null,last_name:document.getElementById('contactLastName').value.trim()||null,organization:document.getElementById('contactOrganization').value.trim()||null,emails:document.getElementById('contactEmails').value.split(/[\n,;]/).map(value=>value.trim()).filter(Boolean),phones:contactPhonesFromForm(),addresses:contactAddressesFromForm()};status.textContent=L('Сохраняю на сервере…','Saving to the server…');status.dataset.kind='';try{if(editingContact)await window.tm.updateContact(editingContact.id,input);else await window.tm.createContact(Number(document.getElementById('contactAccount').value),input);await window.reloadCoreData();closeAuxEditor();showToast(L('Контакт сохранён и синхронизирован','Contact saved and synced'));}catch(error){status.textContent=String(error);status.dataset.kind='error';}};
document.getElementById('contactDelete').onclick=async()=>{if(!editingContact||!await confirmAction(L('Удалить контакт?','Delete this contact?')))return;const status=document.getElementById('contactStatus');status.textContent=L('Удаляю…','Deleting…');try{await window.tm.deleteContact(editingContact.id);await window.reloadCoreData();closeAuxEditor();showToast(L('Контакт удалён','Contact deleted'));}catch(error){status.textContent=String(error);status.dataset.kind='error';}};
calSection.addEventListener('click',e=>{if(window.calendarDropJustHappened)return;const item=e.target.closest('.ev,.wk-ev');if(!item)return;e.stopPropagation();const event=coreCalendarData.events.find(value=>value.id===Number(item.dataset.eventId));if(event)openEventEditor(event);});
window.prepareCalendarEventMove=(eventId,sourceValue,targetValue)=>{const source=coreCalendarData.events.find(value=>value.id===Number(eventId));if(!source)return;const parseTarget=value=>{const match=String(value).match(/^(\d{4})-(\d{2})-(\d{2})$/);return match?new Date(+match[1],+match[2]-1,+match[3]):parseDavDate(value);},sourceOccurrence=parseDavDate(sourceValue)||parseDavDate(source.dtstart),target=parseTarget(targetValue);if(!sourceOccurrence||!target)return;const targetHasTime=!/^\d{4}-\d{2}-\d{2}$/.test(String(targetValue));if(!targetHasTime&&!source.all_day)target.setHours(sourceOccurrence.getHours(),sourceOccurrence.getMinutes(),sourceOccurrence.getSeconds(),sourceOccurrence.getMilliseconds());const sourceStart=parseDavDate(source.dtstart),sourceEnd=parseDavDate(source.dtend),duration=sourceStart&&sourceEnd&&sourceEnd>sourceStart?sourceEnd-sourceStart:60*60000,movedStart=target,movedEnd=new Date(movedStart.getTime()+duration),becomesTimed=targetHasTime&&Boolean(source.all_day),pad=value=>String(value).padStart(2,'0'),dateOnly=value=>`${value.getFullYear()}-${pad(value.getMonth()+1)}-${pad(value.getDate())}`,draft={...source,all_day:becomesTimed?false:Boolean(source.all_day),dtstart:source.all_day&&!becomesTimed?dateOnly(movedStart):movedStart.toISOString(),dtend:source.all_day&&!becomesTimed?dateOnly(movedEnd):movedEnd.toISOString()};openEventEditor(draft);const status=document.getElementById('eventStatus');status.textContent=L('Проверьте новое время и нажмите «Сохранить».','Check the new time and click Save.');status.dataset.kind='';};
