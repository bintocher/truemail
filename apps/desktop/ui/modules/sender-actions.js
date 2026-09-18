// truemail UI module: sender-actions.js
/* Блокировка отправителя, игнорирование переписки и автоочистка по
   отправителю: пункты меню письма, диалоги подтверждения и разделы настроек.
   Словарь подписей и проверки состава живут в modules/sender-lists.js, здесь
   только отрисовка и обращения к ядру.
   См. specs/blocked-senders.md, specs/ignore-conversation.md,
   specs/sweep-by-sender.md. */
const senderLang=()=>smartIsEnglish()?'en':'ru';
let senderPolicies=[],ignoredConversations=[],senderSweepRules=[];

/* Письмо, к которому относится действие меню: открытое письмо или строка, на
   которой вызвано контекстное меню. */
function senderActionMessage(){return activeMessage||null;}

/* Общая обвязка окна: заголовок, тело и две кнопки. Разметка повторяет
   остальные окна настроек, чтобы диалоги выглядели одинаково. */
function senderDialog(title,bodyHtml,confirmText){
  const overlay=document.createElement('div');overlay.className='overlay open';
  overlay.innerHTML=`<div class="modal compact-modal"><div class="mh"><i data-i="filter"></i><h3></h3><button class="iconbtn x" type="button"><i data-i="close"></i></button></div><div class="mb">${bodyHtml}</div><div class="mf"><span class="sp"></span><button class="btn sender-cancel"></button><button class="btn primary sender-apply"></button></div></div>`;
  overlay.querySelector('h3').textContent=title;
  overlay.querySelector('.sender-cancel').textContent=L('Отмена','Cancel');
  overlay.querySelector('.sender-apply').textContent=confirmText;
  document.body.appendChild(overlay);renderIcons(overlay);
  const close=()=>overlay.remove();
  overlay.querySelectorAll('.x,.sender-cancel').forEach(button=>button.onclick=close);
  overlay.onclick=event=>{if(event.target===overlay)close();};
  return {overlay,close};
}

/* Блокировка отправителя из письма: выбор между адресом и доменом с адресом по
   умолчанию, область списков, число уже полученных писем по каждому ящику и
   выключенный переключатель уборки (S-027 - S-031). */
async function openBlockSenderDialog(message){
  const choice=senderListsModel.senderChoice(message);
  if(!choice){showToast(L('У письма нет разобранного адреса отправителя','This message has no parsed sender address'));return;}
  const body=`<div class="fld"><label>${escapeHtml(L('Что заблокировать','What to block'))}</label><select class="sel sender-kind"><option value="address">${escapeHtml(choice.address)}</option><option value="domain">${escapeHtml(choice.domain)}</option></select></div>
    <p class="note-muted">${escapeHtml(L('Списки заблокированных и доверенных отправителей общие для всех подключённых ящиков.','The blocked and trusted sender lists are shared by all connected mailboxes.'))}</p>
    <p class="note-muted sender-counts"></p>
    <label class="aux-check"><input type="checkbox" class="sender-sweep-consent"> <span>${escapeHtml(L('Убрать уже полученные письма в корзину','Move already received messages to trash'))}</span></label>
    <p class="note-muted">${escapeHtml(L('Письма уходят только в корзину и восстанавливаются оттуда до её очистки.','Messages go to trash only and can be restored until it is emptied.'))}</p>`;
  const {overlay,close}=senderDialog(L('Заблокировать отправителя','Block sender'),body,L('Заблокировать','Block'));
  const kind=overlay.querySelector('.sender-kind'),counts=overlay.querySelector('.sender-counts'),consent=overlay.querySelector('.sender-sweep-consent');
  let preview=null;
  const refresh=async()=>{
    counts.textContent=L('Считаю письма…','Counting messages…');
    try{
      preview=await window.tm.previewSenderPolicy(kind.value,kind.value==='domain'?choice.domain:choice.address);
      const lines=preview.per_account.map(item=>`${item.email}: ${item.count}`).join('; ');
      counts.textContent=`${L('Уже получено писем','Messages already received')}: ${preview.total}${lines?` (${lines})`:''}`;
      if(preview.own_address)counts.textContent+=` - ${L('это адрес подключённого ящика, заблокировать его нельзя','this is a connected mailbox address and cannot be blocked')}`;
      else if(preview.own_domain)counts.textContent+=` - ${L('этому домену принадлежит адрес подключённого ящика, собственные адреса останутся доверенными','a connected mailbox belongs to this domain, your own addresses stay trusted')}`;
    }catch(error){preview=null;counts.textContent=String(error?.message||error);}
  };
  kind.onchange=refresh;await refresh();
  overlay.querySelector('.sender-apply').onclick=async()=>{
    if(!preview){showToast(L('Значение не подходит для списка','This value does not fit the list'));return;}
    if(preview.own_address){showToast(L('Адрес подключённого ящика заблокировать нельзя','A connected mailbox address cannot be blocked'));return;}
    // S-020: блокировка собственного домена требует отдельного подтверждения.
    if(preview.own_domain&&!await confirmAction(L('Этому домену принадлежит адрес подключённого ящика. Собственные адреса останутся доверенными. Заблокировать домен?','A connected mailbox belongs to this domain. Your own addresses stay trusted. Block the domain?')))return;
    try{
      const saved=await window.tm.saveSenderPolicy(preview.kind,preview.value,'blocked',Boolean(preview.own_domain));
      if(consent.checked){
        const reports=await window.tm.startSenderPolicySweep(saved.id,preview.snapshot_key,true);
        const total=reports.reduce((sum,item)=>sum+(item.queued||0),0),left=reports.reduce((sum,item)=>sum+(item.remaining||0),0);
        showToast(`${L('Отправитель заблокирован','Sender blocked')}: ${L('поставлено перемещений','moves queued')} ${total}${left?`, ${L('осталось','left')} ${left}`:''}`);
      }else showToast(L('Отправитель заблокирован','Sender blocked'));
      close();await reloadSenderSections();
    }catch(error){showToast(error);}
  };
}

/* Автоочистка писем отправителя: разовая уборка и три постоянных режима,
   число писем с распределением по папкам, согласие на архив (S-006 - S-011,
   S-016). */
async function openSweepSenderDialog(message){
  const choice=senderListsModel.senderChoice(message);
  // S-007: без разобранного адреса отправителя автоочистку не запустить.
  if(!choice){showToast(L('У письма нет разобранного адреса отправителя','This message has no parsed sender address'));return;}
  const lang=senderLang();
  const modes=senderListsModel.SWEEP_MODES.map(mode=>`<option value="${mode.id}">${escapeHtml(lang==='en'?mode.en:mode.ru)}</option>`).join('');
  const accounts=(coreAccounts||[]).map(account=>`<option value="${account.id}">${escapeHtml(account.email)}</option>`).join('');
  const body=`<p class="note-muted">${escapeHtml(L('Отправитель','Sender'))}: ${escapeHtml(choice.address)}</p>
    <div class="fld"><label>${escapeHtml(L('Что сделать','What to do'))}</label><select class="sel sweep-mode">${modes}</select></div>
    <div class="fld sweep-days hidden"><label>${escapeHtml(L('Старше скольких дней','Older than how many days'))}</label><input class="inp sweep-days-value" type="number" min="1" max="3650" step="1" value="30"></div>
    <div class="fld"><label>${escapeHtml(L('Область','Scope'))}</label><select class="sel sweep-scope"><option value="all">${escapeHtml(L('Все ящики','All mailboxes'))}</option>${accounts}</select></div>
    <label class="aux-check"><input type="checkbox" class="sweep-archive"> <span>${escapeHtml(L('Убирать письма и из архива','Clean up archived messages too'))}</span></label>
    <p class="note-muted sweep-counts"></p>
    <p class="note-muted">${escapeHtml(L('Письма уходят только в корзину и восстанавливаются оттуда до её очистки.','Messages go to trash only and can be restored until it is emptied.'))}</p>`;
  const {overlay,close}=senderDialog(L('Автоочистка по отправителю','Clean up by sender'),body,L('Выполнить','Run'));
  const mode=overlay.querySelector('.sweep-mode'),days=overlay.querySelector('.sweep-days'),daysValue=overlay.querySelector('.sweep-days-value');
  const scope=overlay.querySelector('.sweep-scope'),archive=overlay.querySelector('.sweep-archive'),counts=overlay.querySelector('.sweep-counts');
  if(message?.account_id)scope.value=String(message.account_id);
  let preview=null;
  const form=()=>({mode:mode.value,accountId:scope.value==='all'?null:Number(scope.value),days:daysValue.value,sweepArchive:archive.checked});
  const refresh=async()=>{
    days.classList.toggle('hidden',mode.value!=='older_than');
    if(mode.value==='new_now'){preview=null;counts.textContent=L('Будет создано обычное правило: новые письма этого отправителя сразу уйдут в корзину.','An ordinary rule will be created: new messages from this sender go straight to trash.');return;}
    if(mode.value==='older_than'&&!senderListsModel.validSweepDays(daysValue.value)){preview=null;counts.textContent=L('Число дней задаётся целым от 1 до 3650','The number of days is a whole number from 1 to 3650');return;}
    counts.textContent=L('Считаю письма…','Counting messages…');
    try{
      preview=await window.tm.previewSenderSweep(senderListsModel.sweepInput(choice,form()));
      const folders=preview.folders.map(item=>`${item.name}: ${item.count}`).join('; ');
      counts.textContent=`${L('Уйдёт в корзину писем','Messages going to trash')}: ${preview.total}${folders?` (${folders})`:''}`;
    }catch(error){preview=null;counts.textContent=String(error?.message||error);}
  };
  mode.onchange=refresh;scope.onchange=refresh;archive.onchange=refresh;daysValue.oninput=refresh;
  await refresh();
  overlay.querySelector('.sender-apply').onclick=async()=>{
    const input=senderListsModel.sweepInput(choice,form());
    if(input.mode==='older_than'&&!senderListsModel.validSweepDays(input.days)){showToast(L('Число дней задаётся целым от 1 до 3650','The number of days is a whole number from 1 to 3650'));return;}
    if(input.mode!=='new_now'&&!preview){showToast(L('Список писем устарел, откройте окно заново','The message list is out of date, open the dialog again'));return;}
    // S-010: подтверждение с числом писем требуется всегда, даже когда их мало.
    if(input.mode!=='new_now'&&!await confirmAction(`${L('Убрать в корзину писем','Move to trash')}: ${preview.total}. ${L('Продолжить?','Continue?')}`))return;
    try{
      const report=await window.tm.startSenderSweep(input,preview?preview.snapshot_key:'');
      showToast(`${L('Автоочистка запущена','Clean-up started')}: ${senderListsModel.sweepReportText(report,senderLang())}`);
      close();await reloadSenderSections();
    }catch(error){showToast(error);}
  };
}

/* Игнорирование переписки из письма: тема, ящик и число писем с
   подтверждением, а для уже игнорируемой переписки - прекращение с возвратом
   писем или без него (S-013, S-033, S-034). */
async function toggleIgnoreConversation(message){
  if(!message){showToast(L('Сначала выберите письмо','Select a message first'));return;}
  let preview;
  try{preview=await window.tm.previewIgnoreConversation(message.id);}catch(error){showToast(error);return;}
  if(preview.existing_id){await openStopIgnoringDialog(preview.existing_id);return;}
  const partial=preview.partial?` ${L('Набор переписки достиг предела: новые ветви могут снова появляться во входящих.','The conversation set reached its limit: new branches may show up in the inbox again.')}`:'';
  const question=`${L('Игнорировать переписку','Ignore conversation')} "${preview.subject||L('без темы','no subject')}" (${preview.account_email})? ${L('В корзину уйдёт писем','Messages going to trash')}: ${preview.total}.${partial}`;
  if(!await confirmAction(question))return;
  try{
    await window.tm.enableIgnoreConversation(message.id,preview.snapshot_key,true);
    showToast(L('Переписка игнорируется, её письма уходят в корзину','The conversation is ignored, its messages go to trash'));
    await reloadSenderSections();
    await window.reloadCoreData?.();
  }catch(error){showToast(error);}
}

/* Прекращение игнорирования: возврат обещается честно - возвращаются письма,
   которые удалось опознать в корзине (S-034, S-038). */
async function openStopIgnoringDialog(conversationId){
  const body=`<p>${escapeHtml(L('Прекратить игнорирование этой переписки?','Stop ignoring this conversation?'))}</p>
    <label class="aux-check"><input type="checkbox" class="ignore-return"> <span>${escapeHtml(L('Вернуть письма из корзины в их прежние папки','Return messages from trash to their previous folders'))}</span></label>
    <p class="note-muted">${escapeHtml(L('Вернутся те письма, которые удастся опознать в корзине по адресу отправителя, дате и заголовку Message-ID. Число непрошедших возврат программа назовёт.','Only messages that can be identified in trash by sender address, date and Message-ID will be returned. The number of messages that could not be returned will be reported.'))}</p>`;
  const {overlay,close}=senderDialog(L('Прекратить игнорирование','Stop ignoring'),body,L('Прекратить','Stop'));
  overlay.querySelector('.sender-apply').onclick=async()=>{
    const back=overlay.querySelector('.ignore-return').checked;
    try{
      const report=await window.tm.disableIgnoreConversation(conversationId,back);
      showToast(back?`${L('Возврат писем','Message return')}: ${senderListsModel.returnReportText(report,senderLang())}`:L('Игнорирование снято','Ignoring is off'));
      close();await reloadSenderSections();await window.reloadCoreData?.();
    }catch(error){showToast(error);}
  };
}

/* Раздел отправителей: раздельные списки заблокированных и доверенных
   значений с видом записи и датой добавления (S-045). */
function renderSenderPolicies(){
  const host=document.getElementById('senderPoliciesList');if(!host)return;
  host.dataset.noI18n='1';host.innerHTML='';
  if(!senderPolicies.length){const empty=document.createElement('p');empty.className='note-muted';empty.textContent=L('Списки пока пусты.','The lists are empty.');host.appendChild(empty);return;}
  ['blocked','trusted'].forEach(decision=>{
    const rows=senderPolicies.filter(policy=>policy.decision===decision);
    if(!rows.length)return;
    const title=document.createElement('div');title.className='rule-failed-title';
    title.textContent=senderListsModel.policyDecisionText(decision,senderLang());
    host.appendChild(title);
    rows.forEach(policy=>{
      const row=document.createElement('div');row.className='rule-failed-row';
      const text=document.createElement('span');
      text.textContent=`${senderListsModel.policyRowText(policy,senderLang())} (${policy.created_at})`;
      if(policy.job_state)text.textContent+=` - ${L('идёт уборка','clean-up in progress')}`;
      if(policy.last_error)text.textContent+=` - ${policy.last_error}`;
      if(policy.failed)text.textContent+=` - ${L('необработанных писем','unprocessed messages')}: ${policy.failed}`;
      const remove=document.createElement('button');remove.type='button';remove.className='btn sm';
      remove.textContent=L('Удалить','Delete');
      remove.onclick=async()=>{
        // S-040: уже убранные письма остаются в корзине, и об этом говорится
        // до удаления записи.
        if(!await confirmAction(L('Удалить запись? Уже убранные письма останутся в корзине.','Delete the entry? Messages already moved stay in trash.')))return;
        try{
          const report=await window.tm.deleteSenderPolicy(policy.id);
          showToast(`${L('Запись удалена','Entry deleted')}: ${L('отменено перемещений','moves cancelled')} ${report.cancelled}, ${L('уже выполняется','already running')} ${report.irreversible}`);
          await reloadSenderSections();
        }catch(error){showToast(error);}
      };
      row.append(text,remove);host.appendChild(row);
    });
  });
}

/* Список игнорируемых переписок: тема, ящик, участники, состояние, признак
   частичного покрытия и число убранных писем (S-031). */
function renderIgnoredConversations(){
  const host=document.getElementById('ignoredConversationsList');if(!host)return;
  host.dataset.noI18n='1';host.innerHTML='';
  if(!ignoredConversations.length){const empty=document.createElement('p');empty.className='note-muted';empty.textContent=L('Игнорируемых переписок нет.','No ignored conversations.');host.appendChild(empty);return;}
  ignoredConversations.forEach(record=>{
    const row=document.createElement('div');row.className='rule-failed-row';
    const text=document.createElement('span');
    const partial=record.partial?` - ${L('частичное покрытие','partial coverage')}`:'';
    const failed=record.failed?`, ${L('не вернулось','not returned')}: ${record.failed}`:'';
    const skipped=record.skipped?`, ${L('пропущено','skipped')}: ${record.skipped}`:'';
    text.textContent=`${record.subject||L('без темы','no subject')} (${record.account_email}) - ${senderListsModel.ignoreStateText(record.state,senderLang())}${partial}, ${L('убрано писем','messages moved')}: ${record.moved}${skipped}${failed}`;
    if(record.last_error)text.textContent+=` - ${record.last_error}`;
    row.appendChild(text);
    if(record.state!=='disabled'){
      const stop=document.createElement('button');stop.type='button';stop.className='btn sm';
      stop.textContent=L('Прекратить','Stop');
      stop.onclick=()=>openStopIgnoringDialog(record.id);
      row.appendChild(stop);
    }
    host.appendChild(row);
  });
}

/* Записи автоочистки живут в общем списке правил с пометкой автоочистки,
   адресом, областью, режимом и временем последнего прохода (S-037). */
function renderSenderSweepRules(){
  const host=document.getElementById('senderSweepList');if(!host)return;
  host.dataset.noI18n='1';host.innerHTML='';
  if(!senderSweepRules.length)return;
  const title=document.createElement('div');title.className='rule-failed-title';
  title.textContent=L('Автоочистка по отправителю','Clean-up by sender');
  host.appendChild(title);
  senderSweepRules.forEach(rule=>{
    const row=document.createElement('div');row.className='rule-failed-row';
    const toggle=document.createElement('div');toggle.className='toggle'+(rule.enabled?' on':'');toggle.setAttribute('role','switch');
    toggle.onclick=async()=>{try{await window.tm.setSenderSweepEnabled(rule.id,!rule.enabled);await reloadSenderSections();}catch(error){showToast(error);}};
    const text=document.createElement('span');
    const pass=rule.last_full_pass_at?`, ${L('последний проход','last pass')}: ${rule.last_full_pass_at}`:'';
    const failed=rule.failed?`, ${L('отказов','failures')}: ${rule.failed}`:'';
    text.textContent=`${rule.address} - ${senderListsModel.sweepModeText(rule.mode,rule.days,senderLang())} (${senderListsModel.sweepScopeText(rule,coreAccounts,senderLang())}), ${L('убрано писем','messages moved')}: ${rule.queued}${failed}${pass}`;
    if(rule.last_error)text.textContent+=` - ${rule.last_error}`;
    const remove=document.createElement('button');remove.type='button';remove.className='btn sm';
    remove.textContent=L('Удалить','Delete');
    remove.onclick=async()=>{
      // S-039: удаление записи не возвращает письма из корзины.
      if(!await confirmAction(L('Удалить автоочистку? Уже убранные письма останутся в корзине.','Delete the clean-up? Messages already moved stay in trash.')))return;
      try{await window.tm.deleteSenderSweepRule(rule.id);await reloadSenderSections();}catch(error){showToast(error);}
    };
    row.append(toggle,text,remove);host.appendChild(row);
  });
  renderIcons(host);
}

/* Незавершённые уборки: продолжение не должно зависеть от окна, в котором
   уборку запустили (blocked-senders.md S-036, sweep-by-sender.md S-018). */
async function renderSenderJobs(){
  const host=document.getElementById('senderPendingJobs');if(!host)return;
  host.innerHTML='';
  let policyJobs=[],sweepJobs=[];
  try{policyJobs=await window.tm.pendingSenderPolicyJobs();}catch(_){policyJobs=[];}
  try{sweepJobs=await window.tm.pendingSenderSweepJobs();}catch(_){sweepJobs=[];}
  if(!policyJobs.length&&!sweepJobs.length)return;
  const block=document.createElement('div');block.className='rule-failed-ops';
  const title=document.createElement('div');title.className='rule-failed-title';
  title.textContent=L('Незавершённые уборки','Unfinished clean-ups');
  block.appendChild(title);
  const addRow=(job,resume,cancel)=>{
    const row=document.createElement('div');row.className='rule-failed-row';
    const text=document.createElement('span');text.textContent=senderListsModel.sweepReportText(job,senderLang());
    const go=document.createElement('button');go.type='button';go.className='btn sm';go.textContent=L('Продолжить','Continue');
    go.onclick=async()=>{try{await resume();await reloadSenderSections();}catch(error){showToast(error);}};
    const stop=document.createElement('button');stop.type='button';stop.className='btn sm';stop.textContent=L('Отменить','Cancel');
    stop.onclick=async()=>{try{const report=await cancel();showToast(`${L('Уборка отменена','Clean-up cancelled')}: ${L('уже выполняется','already running')} ${report.irreversible||0}`);await reloadSenderSections();}catch(error){showToast(error);}};
    row.append(text,go,stop);block.appendChild(row);
  };
  policyJobs.forEach(job=>addRow(job,()=>window.tm.continueSenderPolicySweep(job.id),()=>window.tm.cancelSenderPolicySweep(job.id)));
  sweepJobs.forEach(job=>addRow(job,()=>window.tm.continueSenderSweepJob(job.id),()=>window.tm.cancelSenderSweepJob(job.id)));
  host.appendChild(block);
}

async function reloadSenderSections(){
  if(!window.tm)return;
  try{senderPolicies=await window.tm.listSenderPolicies();}catch(_){senderPolicies=[];}
  try{ignoredConversations=await window.tm.listIgnoredConversations();}catch(_){ignoredConversations=[];}
  try{senderSweepRules=await window.tm.listSenderSweepRules();}catch(_){senderSweepRules=[];}
  renderSenderPolicies();renderIgnoredConversations();renderSenderSweepRules();await renderSenderJobs();
}
window.reloadSenderSections=reloadSenderSections;
/* Смена языка перерисовывает уже прочитанные записи, не обращаясь к ядру
   заново: подписи собраны в коде и сами по себе не меняются. */
window.relocalizeSenderSections=()=>{renderSenderPolicies();renderIgnoredConversations();renderSenderSweepRules();};

/* Ручное добавление значения в список (S-045). */
async function addSenderPolicyByHand(decision){
  const value=prompt(decision==='blocked'?L('Адрес или домен для блокировки','Address or domain to block'):L('Адрес или домен для доверия','Address or domain to trust'),'');
  if(!value||!value.trim())return;
  const kind=value.includes('@')&&!value.trim().startsWith('@')?'address':'domain';
  try{
    await window.tm.saveSenderPolicy(kind,value.trim(),decision,false);
    await reloadSenderSections();
  }catch(error){
    // S-020: блокировка собственного домена продолжается только после
    // отдельного подтверждения.
    const text=String(error?.message||error);
    if(text.includes('подтвердите блокировку')&&await confirmAction(text)){
      try{await window.tm.saveSenderPolicy(kind,value.trim(),decision,true);await reloadSenderSections();}catch(again){showToast(again);}
      return;
    }
    showToast(error);
  }
}

document.getElementById('senderPolicyAddBlocked')?.addEventListener('click',()=>addSenderPolicyByHand('blocked'));
document.getElementById('senderPolicyAddTrusted')?.addEventListener('click',()=>addSenderPolicyByHand('trusted'));

/* Пункты меню открытого письма: меню собрано на признаках data-thread-action,
   и новые действия подхватываются тем же обработчиком (S-027). */
document.getElementById('threadMoreMenu')?.addEventListener('click',event=>{
  const button=event.target.closest('[data-thread-action]');if(!button)return;
  const action=button.dataset.threadAction,message=senderActionMessage();
  if(action==='block-sender')openBlockSenderDialog(message);
  else if(action==='sweep-sender')openSweepSenderDialog(message);
  else if(action==='ignore-conversation')toggleIgnoreConversation(message);
  else if(action==='senders'){showView('settingsView');setSection('senders');}
});

/* Те же действия в контекстном меню строки списка писем (S-028). */
document.getElementById('ctxmenu')?.addEventListener('click',event=>{
  const item=event.target.closest('[data-context-action]');if(!item)return;
  const action=item.dataset.contextAction,message=senderActionMessage();
  if(action==='block-sender')openBlockSenderDialog(message);
  else if(action==='sweep-sender')openSweepSenderDialog(message);
  else if(action==='ignore-conversation')toggleIgnoreConversation(message);
});
