// truemail UI module: queue-sections.js
// Разделы настроек "Исходящие", "Автоответ" и "История получателей": чтение
// данных у ядра и отрисовка. Чистая логика этих разделов лежит в outbox.js,
// out-of-office.js и recipient-history.js и проверяется отдельно.
// См. specs/undo-send.md, specs/out-of-office.md, specs/recipient-history.md.

let outboxEntries=[];
let absenceSettings=null;
let absenceReplies=[];
let historyEntries=[];

function queueLang(){return wizardLocale==='en'?'en':'ru';}

/* Список ящиков во всех трёх разделах строится одинаково: настройка автоответа
   и история принадлежат одному ящику (out-of-office.md S-001,
   recipient-history.md S-018 интерфейсов и данных). */
function fillQueueAccountSelect(select,withAll){
  if(!select)return;
  const previous=select.value;
  select.innerHTML='';
  if(withAll){
    const all=document.createElement('option');all.value='';all.textContent=L('Все ящики','All mailboxes');
    select.appendChild(all);
  }
  (coreAccounts||[]).forEach(account=>{
    const option=document.createElement('option');option.value=String(account.id);option.textContent=account.email;
    select.appendChild(option);
  });
  if(previous&&[...select.options].some(option=>option.value===previous))select.value=previous;
}

/* ---------- Раздел "Исходящие" ---------- */

function renderOutboxList(){
  const host=document.getElementById('outboxList');if(!host)return;
  host.innerHTML='';
  if(!outboxEntries.length){
    const empty=document.createElement('p');empty.className='note-muted';
    empty.textContent=L('Очередь отправки пуста.','The sending queue is empty.');
    host.appendChild(empty);return;
  }
  outboxEntries.forEach(entry=>{
    const row=document.createElement('div');row.className='rule-failed-row';
    const text=document.createElement('span');
    text.textContent=window.outboxModel.outboxRowText(entry,queueLang());
    row.appendChild(text);
    if(entry.status==='cancelled'){
      const open=document.createElement('button');open.type='button';open.className='btn sm';
      open.textContent=L('Открыть','Open');
      open.onclick=()=>window.restoreCancelledSend?.(entry.account_id,entry.id).then(()=>reloadQueueSections());
      row.appendChild(open);
    }
    if(['pending','retry'].includes(entry.status)){
      const cancel=document.createElement('button');cancel.type='button';cancel.className='btn sm';
      cancel.textContent=L('Отменить','Cancel');
      cancel.onclick=async()=>{
        try{
          const outcome=await window.tm.cancelSend(entry.account_id,entry.id);
          showToast(window.outboxModel.cancelOutcomeText(outcome,queueLang()));
          await reloadQueueSections();
        }catch(error){showToast(error);}
      };
      row.appendChild(cancel);
    }
    if(['uncertain','failed'].includes(entry.status)){
      const retry=document.createElement('button');retry.type='button';retry.className='btn sm';
      retry.textContent=L('Повторить','Retry');
      retry.onclick=async()=>{
        // S-051: о возможном втором экземпляре письма у получателя говорится
        // до создания новой попытки.
        if(entry.status==='uncertain'&&!await confirmAction(L('Итог прошлой передачи неизвестен. Повтор может создать у получателя второй экземпляр письма. Повторить?','The previous attempt ended with an unknown outcome. Retrying may deliver a second copy. Retry?')))return;
        try{await window.tm.retrySend(entry.account_id,entry.id);await reloadQueueSections();}
        catch(error){showToast(error);}
      };
      row.appendChild(retry);
    }
    if(entry.status!=='processing'){
      const remove=document.createElement('button');remove.type='button';remove.className='btn sm';
      remove.textContent=L('Удалить','Delete');
      remove.onclick=async()=>{
        // S-043: другой копии письма у программы нет, поэтому удаление
        // спрашивают отдельно.
        if(!await confirmAction(L('Удалить письмо вместе с вложениями? Другой копии у программы нет.','Delete the message with its attachments? The app has no other copy.')))return;
        try{await window.tm.deleteSend(entry.account_id,entry.id);await reloadQueueSections();}
        catch(error){showToast(error);}
      };
      row.appendChild(remove);
    }
    host.appendChild(row);
  });
}

/* ---------- Раздел автоответа ---------- */

function localStamp(value){
  const parsed=Date.parse(value||'');
  if(!Number.isFinite(parsed))return '';
  const local=new Date(parsed-new Date(parsed).getTimezoneOffset()*60000);
  return local.toISOString().slice(0,16);
}

function renderAbsenceSection(){
  const host=document.getElementById('absenceMode');if(!host)return;
  host.textContent=window.outOfOfficeModel.oofModeExplanation(absenceSettings,queueLang());
  const settings=absenceSettings||{};
  const available=settings.available!==false;
  document.getElementById('absenceToggle')?.classList.toggle('on',Boolean(settings.enabled));
  const start=document.getElementById('absenceStart'),end=document.getElementById('absenceEnd');
  if(start)start.value=localStamp(settings.starts_at);
  if(end)end.value=localStamp(settings.ends_at);
  const internal=document.getElementById('absenceInternal'),external=document.getElementById('absenceExternal');
  if(internal)internal.value=settings.internal_text||'';
  if(external)external.value=settings.external_text||'';
  const domains=document.getElementById('absenceDomains');
  if(domains){
    domains.value=(settings.internal_domains||[]).join(', ');
    // Перечень доменов относится только к локальному режиму: на сервере
    // деление на своих и внешних определяет сам Exchange (S-020).
    domains.disabled=settings.mode==='server';
  }
  ['absenceSave','absenceDisable'].forEach(id=>{
    const button=document.getElementById(id);if(button)button.disabled=!available;
  });
  const status=document.getElementById('absenceStatus');
  if(status){
    const lines=[];
    if(settings.last_error)lines.push(`${L('Последняя ошибка','Last error')}: ${settings.last_error}`);
    // S-065: письма периода отсутствия старше суток остались без ответа -
    // их число называется прямо, а не прячется.
    if(settings.skipped_old>0)lines.push(L(`Писем без ответа как слишком старых: ${settings.skipped_old}`,`Messages left unanswered as too old: ${settings.skipped_old}`));
    status.textContent=lines.join('. ');
    status.dataset.kind=settings.last_error?'error':'';
  }
  const replies=document.getElementById('absenceReplies');
  if(replies){
    replies.innerHTML='';
    if(!absenceReplies.length){
      const empty=document.createElement('p');empty.className='note-muted';
      empty.textContent=L('Автоответы пока не отправлялись.','No auto-replies have been sent yet.');
      replies.appendChild(empty);
    }else{
      absenceReplies.forEach(reply=>{
        const row=document.createElement('div');row.className='rule-failed-row';
        const text=document.createElement('span');
        text.textContent=`${reply.recipient_key} - ${reply.replied_at} - ${reply.state}`;
        row.appendChild(text);replies.appendChild(row);
      });
    }
  }
}

function absenceFormInput(){
  const accountId=Number(document.getElementById('absenceAccount')?.value)||0;
  return window.outOfOfficeModel.oofInput({
    accountId,
    enabled:document.getElementById('absenceToggle')?.classList.contains('on'),
    startsAt:document.getElementById('absenceStart')?.value,
    endsAt:document.getElementById('absenceEnd')?.value,
    internalText:document.getElementById('absenceInternal')?.value,
    externalText:document.getElementById('absenceExternal')?.value,
    domains:document.getElementById('absenceDomains')?.value,
  });
}

/* ---------- Раздел истории получателей ---------- */

function renderHistoryList(){
  const host=document.getElementById('historyList');if(!host)return;
  host.innerHTML='';
  if(!historyEntries.length){
    const empty=document.createElement('p');empty.className='note-muted';
    empty.textContent=L('История пуста.','The history is empty.');
    host.appendChild(empty);return;
  }
  historyEntries.forEach(entry=>{
    const row=document.createElement('div');row.className='rule-failed-row';
    const text=document.createElement('span');
    text.textContent=window.recipientHistoryModel.historyRowText(entry,queueLang());
    row.appendChild(text);
    const rename=document.createElement('button');rename.type='button';rename.className='btn sm';
    rename.textContent=L('Изменить','Edit');
    rename.onclick=async()=>{
      const name=prompt(L('Показываемое имя','Display name'),entry.name||'');
      if(name===null)return;
      const address=prompt(L('Адрес','Address'),entry.address||'');
      if(address===null)return;
      try{
        await window.tm.updateRecipientHistory(entry.account_id,entry.id,name,address);
        await reloadQueueSections();
        window.invalidateComposerContactCache?.();
      }catch(error){showToast(error);}
    };
    const remove=document.createElement('button');remove.type='button';remove.className='btn sm';
    remove.textContent=L('Убрать','Remove');
    remove.onclick=async()=>{
      try{
        await window.tm.deleteRecipientHistoryEntry(entry.account_id,entry.id);
        await reloadQueueSections();
        window.invalidateComposerContactCache?.();
      }catch(error){showToast(error);}
    };
    row.append(rename,remove);host.appendChild(row);
  });
}

/* ---------- Общая перезагрузка разделов ---------- */

async function reloadQueueSections(){
  if(!window.tm)return;
  fillQueueAccountSelect(document.getElementById('outboxAccount'),true);
  fillQueueAccountSelect(document.getElementById('absenceAccount'),false);
  fillQueueAccountSelect(document.getElementById('historyAccount'),false);
  const outboxAccount=document.getElementById('outboxAccount')?.value;
  try{outboxEntries=await window.tm.listOutboxSends(outboxAccount?Number(outboxAccount):null,100,0);}
  catch(_){outboxEntries=[];}
  const absenceAccount=Number(document.getElementById('absenceAccount')?.value)||0;
  if(absenceAccount){
    try{absenceSettings=await window.tm.outOfOffice(absenceAccount);}catch(error){absenceSettings=null;showToast(error);}
    try{absenceReplies=await window.tm.listOutOfOfficeReplies(absenceAccount,50);}catch(_){absenceReplies=[];}
  }else{absenceSettings=null;absenceReplies=[];}
  const historyAccount=Number(document.getElementById('historyAccount')?.value)||0;
  if(historyAccount){
    try{historyEntries=await window.tm.listRecipientHistory(historyAccount,100,0);}catch(_){historyEntries=[];}
  }else historyEntries=[];
  window.setOutboxEntriesForCard?.(outboxEntries);
  renderOutboxList();renderAbsenceSection();renderHistoryList();
}
window.reloadQueueSections=reloadQueueSections;
/* Смена языка перерисовывает уже прочитанные записи, не обращаясь к ядру
   заново: подписи собраны в коде. */
window.relocalizeQueueSections=()=>{renderOutboxList();renderAbsenceSection();renderHistoryList();};

document.getElementById('outboxRefresh')?.addEventListener('click',()=>reloadQueueSections());
document.getElementById('outboxAccount')?.addEventListener('change',()=>reloadQueueSections());
document.getElementById('absenceAccount')?.addEventListener('change',()=>reloadQueueSections());
document.getElementById('historyAccount')?.addEventListener('change',()=>reloadQueueSections());
document.getElementById('absenceToggle')?.addEventListener('click',function(){this.classList.toggle('on');});
document.getElementById('absenceSave')?.addEventListener('click',async()=>{
  const input=absenceFormInput();
  const status=document.getElementById('absenceStatus');
  const error=window.outOfOfficeModel.oofValidationError(input,queueLang());
  if(error){if(status){status.textContent=error;status.dataset.kind='error';}return;}
  try{
    absenceSettings=await window.tm.saveOutOfOffice(input);
    if(status){status.textContent=L('Автоответ сохранён','Auto-reply saved');status.dataset.kind='success';}
    await reloadQueueSections();
  }catch(saveError){
    if(status){status.textContent=window.errorPresentation.errorText(saveError,{locale:wizardLocale,translations:wizardText});status.dataset.kind='error';}
  }
});
document.getElementById('absenceDisable')?.addEventListener('click',async()=>{
  const accountId=Number(document.getElementById('absenceAccount')?.value)||0;
  if(!accountId)return;
  try{absenceSettings=await window.tm.disableOutOfOffice(accountId);await reloadQueueSections();}
  catch(error){showToast(error);}
});
document.getElementById('historyClear')?.addEventListener('click',async()=>{
  const accountId=Number(document.getElementById('historyAccount')?.value)||0;
  if(!accountId)return;
  // S-047: очистка сохраняет границу очистки, поэтому старые письма не вернут
  // адреса обратно; об этом сказано до подтверждения.
  if(!await confirmAction(L('Очистить историю получателей этого ящика? Адреса вернутся только после новых писем.','Clear the recipient history of this mailbox? Addresses return only after new messages.')))return;
  try{
    await window.tm.clearRecipientHistory(accountId);
    await reloadQueueSections();
    window.invalidateComposerContactCache?.();
  }catch(error){showToast(error);}
});

/* ---------- Длительность окна отмены ---------- */

/* Настройка одна на все ящики и хранится в ядре: интерфейс только показывает
   её и отдаёт новое значение (undo-send.md, S-011 - S-014). */
async function loadUndoSendSetting(){
  const field=document.getElementById('undoSendSeconds');
  if(!field||!window.tm?.undoSendSeconds)return;
  try{field.value=String(await window.tm.undoSendSeconds());}catch(error){console.error(error);}
}
window.loadUndoSendSetting=loadUndoSendSetting;
document.getElementById('undoSendSeconds')?.addEventListener('change',async event=>{
  const field=event.target;
  // Значение поля идёт в проверку как есть: приведение к числу превращало
  // очищенное поле в ноль, и оно молча выключало окно отмены (S-012).
  if(!window.outboxModel.validUndoSeconds(field.value)){
    showToast(L('Окно отмены задаётся целым числом секунд от 0 до 60','The undo window is a whole number of seconds between 0 and 60'));
    await loadUndoSendSetting();
    return;
  }
  try{field.value=String(await window.tm.setUndoSendSeconds(Number(field.value)));}
  catch(error){showToast(error);await loadUndoSendSetting();}
});

/* Раздел читает данные при открытии: держать их свежими всё время незачем,
   а показывать устаревшую очередь нельзя. */
document.querySelectorAll('[data-set]').forEach(button=>{
  if(!['outbox','absence','history'].includes(button.dataset.set))return;
  button.addEventListener('click',()=>{reloadQueueSections().catch(console.error);});
});
