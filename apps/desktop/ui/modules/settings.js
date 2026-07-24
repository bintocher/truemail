// truemail UI module: settings.js
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
function applyListOptions(resetScroll=false,title=null){if(resetScroll)stickyReadIds.clear();let rows=currentTagName!=null?messages.filter(m=>(m.labels||[]).includes(currentTagName)):currentFolderId!==null?messages.filter(m=>m.folder_id===currentFolderId):smartRows(currentSmartIndex??0);const active=[...filterMenu.querySelectorAll('input[type="checkbox"]:checked')].map(input=>input.dataset.filter);if(active.includes('unread'))rows=rows.filter(m=>!m.flags?.seen);if(active.includes('attachments'))rows=rows.filter(m=>m.has_attachments);if(active.includes('flagged'))rows=rows.filter(m=>m.flags?.flagged);
  // Удержать письма, прочитанные в этом показе списка: они выпали из smartRows
  // (умная папка "непрочитанные") или из unread-фильтра только из-за смены seen.
  if(stickyReadIds.size){const present=new Set(rows.map(m=>m.id));stickyReadIds.forEach(id=>{const held=messages.find(m=>m.id===id);if(held&&!present.has(id))rows.push(held);});}
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

async function refreshApiSettings(){if(!window.tm?.externalApiStatus)return;try{const [status,clients,audit]=await Promise.all([window.tm.externalApiStatus(),window.tm.listApiClients(),window.tm.listApiAudit(50)]);const state=document.getElementById('apiState'),toggle=document.getElementById('apiToggle'),port=document.getElementById('apiPort'),url=document.getElementById('apiUrl');state.textContent=status.running?L('Запущен','Running'):L('Остановлен','Stopped');state.classList.toggle('on',status.running);toggle.textContent=status.running?L('Остановить','Stop'):L('Запустить','Start');toggle.dataset.running=status.running?'1':'0';port.disabled=status.running;if(status.port)port.value=status.port;url.textContent=status.url||'';const list=document.getElementById('apiClientList');list.innerHTML='';clients.forEach(client=>{const row=document.createElement('div');row.className='api-client-row';const text=document.createElement('div');text.className='grow';const name=document.createElement('div');name.className='t';name.textContent=client.name;const caps=document.createElement('div');caps.className='api-caps-text';caps.textContent=`${client.caps.join(', ')}${client.last_used?` · ${L('последний вызов','last call')}: ${client.last_used}`:''}`;text.append(name,caps);const revoke=document.createElement('button');revoke.className='btn sm danger-btn';revoke.textContent=L('Отозвать','Revoke');revoke.onclick=async()=>{if(!await confirmAction(L(`Отозвать токен «${client.name}»?`,`Revoke token "${client.name}"?`)))return;await window.tm.revokeApiClient(client.id);await refreshApiSettings();};row.append(text,revoke);list.appendChild(row);});if(!clients.length)list.textContent=L('Клиентов пока нет.','No clients yet.');const auditList=document.getElementById('apiAuditList');auditList.innerHTML='';audit.forEach(entry=>{const row=document.createElement('div');row.className='api-audit-row';const time=document.createElement('span');time.className='api-audit-time';time.textContent=entry.at;const detail=document.createElement('div');detail.className='grow';const action=document.createElement('div');action.className='t';action.textContent=`${entry.client_name||L('Удалённый клиент','Revoked client')} · ${entry.action}`;const body=document.createElement('div');body.className='api-audit-detail';body.textContent=entry.detail||'';detail.append(action,body);row.append(time,detail);auditList.appendChild(row);});if(!audit.length)auditList.textContent=L('Вызовов пока нет.','No calls yet.');}catch(error){showToast(error.message||String(error));}}
document.querySelector('[data-set="api"]')?.addEventListener('click',refreshApiSettings);
document.getElementById('apiToggle').onclick=async()=>{const button=document.getElementById('apiToggle');try{button.disabled=true;if(button.dataset.running==='1')await window.tm.stopExternalApi();else await window.tm.startExternalApi(Number(document.getElementById('apiPort').value)||34981);await refreshApiSettings();}catch(error){showToast(error.message||String(error));}finally{button.disabled=false;}};
document.getElementById('apiClientCreate').onclick=async()=>{const name=document.getElementById('apiClientName').value.trim(),caps=[...document.querySelectorAll('#apiCaps input:checked')].map(input=>input.value);try{const created=await window.tm.createApiClient(name,caps);document.getElementById('apiToken').value=created.token;document.getElementById('apiTokenBox').classList.remove('hidden');document.getElementById('apiClientName').value='';await refreshApiSettings();}catch(error){showToast(error.message||String(error));}};
document.getElementById('apiTokenCopy').onclick=async()=>{const token=document.getElementById('apiToken').value;if(token){await navigator.clipboard.writeText(token);showToast(L('Токен скопирован','Token copied'));}};
document.getElementById('apiAuditRefresh').onclick=refreshApiSettings;
document.getElementById('apiAuditClear').onclick=async()=>{if(await confirmAction(L('Очистить журнал вызовов API?','Clear the API call log?'))){await window.tm.clearApiAudit();await refreshApiSettings();}};

function confirmAction(message){return new Promise(resolve=>{const overlay=document.createElement('div');overlay.className='overlay open';const modal=document.createElement('div');modal.className='modal compact-modal';const body=document.createElement('div');body.className='mb';body.textContent=message;const foot=document.createElement('div');foot.className='mf';const ok=document.createElement('button');ok.className='btn primary';ok.textContent=L('Продолжить','Continue');const cancel=document.createElement('button');cancel.className='btn confirm-cancel';cancel.textContent=L('Отмена','Cancel');const done=value=>{overlay.remove();resolve(value);};ok.onclick=()=>done(true);cancel.onclick=()=>done(false);overlay.onclick=e=>{if(e.target===overlay)done(false);};foot.append(ok,cancel);modal.append(body,foot);overlay.appendChild(modal);document.body.appendChild(overlay);cancel.focus();});}
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
const checkUpdatesButton=document.getElementById('checkUpdates');
if(checkUpdatesButton)checkUpdatesButton.onclick=async()=>{const status=document.getElementById('updateStatus');checkUpdatesButton.disabled=true;if(status)status.textContent=L('Проверяю новую версию…','Checking for a new version…');try{const info=await window.tm.checkForUpdate();if(info.available_version){if(status)status.textContent=L(`Доступна версия ${info.available_version}`,`Version ${info.available_version} is available`);showToast(L(`Доступен truemail ${info.available_version}`,`truemail ${info.available_version} is available`),L('Обновить','Update'),async()=>{if(status)status.textContent=L('Скачиваю и устанавливаю обновление…','Downloading and installing the update…');await window.tm.installUpdate();});}else if(status)status.textContent=L(`Установлена актуальная версия ${info.current_version}`,`Version ${info.current_version} is up to date`);}catch(error){if(status)status.textContent=error.message||String(error);}finally{checkUpdatesButton.disabled=false;}};
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
  try{folderCounterModes=JSON.parse(settings.folder_counters||'{}')||{};}catch(_){folderCounterModes={};}
  if(settings.tags_nav_collapsed==='1'){document.getElementById('tagsNav')?.classList.add('collapsed');document.querySelector('[data-navlabel="tags"]')?.classList.add('collapsed');}
  if(settings.external_api_port)document.getElementById('apiPort').value=settings.external_api_port;if(settings.external_api_enabled==='1')window.tm?.startExternalApi(Number(settings.external_api_port)||34981).then(refreshApiSettings).catch(console.error);
  // Без сохранённого значения показываем платформенный дефолт (как в NotifyAnchor).
  if(notifyPositionSelect)notifyPositionSelect.value=settings.notify_position||(/mac/i.test(navigator.platform)?'top-right':'bottom-right');
  if(settings.theme)setTheme(settings.theme,false);
  if(settings.density)setDensity(settings.density,false);
  if(settings.accent)setAccent(settings.accent,false);
  const expert=settings.expert_mode==='1';
  document.getElementById('expertToggle').classList.toggle('on',expert);
  root.setAttribute('data-mode',expert?'expert':'simple');
  if(settings.preview_lines){document.documentElement.style.setProperty('--preview-lines',settings.preview_lines);const sel=document.getElementById('previewLines');if(sel)sel.value=settings.preview_lines;}
  window.applyCalendarHourRange?.(settings.calendar_hour_start||'08:00',settings.calendar_hour_end||'20:00',false);
  // Вид календаря (месяц/неделя/день): восстанавливаем до первой отрисовки данных, чтобы не мигало дефолтом.
  const calendarView=['month','week','day'].includes(settings.calendar_view)?settings.calendar_view:'month';
  document.getElementById('calSection').dataset.cv=calendarView;
  document.querySelectorAll('#calViews button').forEach(b=>b.classList.toggle('on',b.dataset.cv===calendarView));
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
