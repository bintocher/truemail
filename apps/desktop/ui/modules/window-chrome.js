// truemail UI module: window-chrome.js
/* Своя строка заголовка: перетаскивание, кнопки окна, рамки изменения размера
   и кнопка обновления. Системного оформления у окна нет (decorations: false). */
(function(){
  // Без window API окно нечем двигать и закрывать. Полосу заголовка при этом
  // не прячем: без неё окно выглядело бы вовсе безголовым, а выход остаётся
  // через меню в трее. Случай возможен только если ядро Tauri не поднялось -
  // тогда и остальной интерфейс работать не будет.
  const api=window.__TAURI__?.window;
  if(!api){console.error('truemail: window API недоступен, кнопки окна выключены');return;}
  const appWindow=api.getCurrentWindow();

  const maximizeButton=document.getElementById('winMaximize');
  // Иконка и подсказка кнопки зависят от состояния окна: развернуть/восстановить.
  async function syncMaximizeState(){
    let maximized=false;
    try{maximized=await appWindow.isMaximized();}catch(_){return;}
    document.body.classList.toggle('window-maximized',maximized);
    if(!maximizeButton)return;
    maximizeButton.innerHTML=maximized
      ?'<svg viewBox="0 0 12 12" aria-hidden="true"><rect x="1.5" y="3.5" width="7" height="7" rx="1"/><path d="M3.5 3.5V2.5a1 1 0 0 1 1-1h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-1"/></svg>'
      :'<svg viewBox="0 0 12 12" aria-hidden="true"><rect x="1.5" y="1.5" width="9" height="9" rx="1"/></svg>';
    const title=maximized?L('Восстановить','Restore'):L('Развернуть','Maximize');
    maximizeButton.title=title;maximizeButton.setAttribute('aria-label',title);
  }
  window.syncWindowMaximizeState=syncMaximizeState;

  // Куда сворачивать - выбирает пользователь в настройках. По умолчанию в трей:
  // программа и так живёт там, а скрытое окно освобождает память интерфейса.
  // Клик до загрузки настроек ждёт их: иначе сохранённое "в панель задач" не
  // действовало бы первые мгновения после запуска.
  document.getElementById('winMinimize')?.addEventListener('click',async()=>{
    // Ждём настоящее значение настройки, но не дольше секунды: кнопка окна не
    // должна зависеть от того, дошла ли загрузка настроек до конца.
    if(window.tmMinimizeToTray===undefined){
      const waited=window.tmSettingsReady?.catch?.(()=>{})||Promise.resolve();
      await Promise.race([waited,new Promise(resolve=>setTimeout(resolve,1000))]);
    }
    const toTray=window.tmMinimizeToTray!==false;
    try{await (toTray?appWindow.hide():appWindow.minimize());}
    catch(error){
      console.error(error);
      // Если спрятать окно не вышло, сворачиваем обычным способом: мёртвая
      // кнопка хуже, чем сворачивание не туда, куда просил пользователь.
      if(toTray)try{await appWindow.minimize();}catch(fallbackError){console.error(fallbackError);}
    }
  });
  // Подпись кнопки следует за настройкой: иначе она обещала бы трей тем, кто
  // выбрал обычное сворачивание.
  const minimizeButton=document.getElementById('winMinimize');
  window.updateMinimizeButtonLabel=function(){
    if(!minimizeButton)return;
    const toTray=window.tmMinimizeToTray!==false;
    const label=toTray?L('Свернуть в трей','Minimize to tray'):L('Свернуть','Minimize');
    minimizeButton.title=label;minimizeButton.setAttribute('aria-label',label);
  };
  maximizeButton?.addEventListener('click',()=>appWindow.toggleMaximize().then(syncMaximizeState).catch(console.error));
  // Закрытие прячет программу в трей - это решает обработчик CloseRequested в ядре.
  document.getElementById('winClose')?.addEventListener('click',()=>appWindow.close().catch(console.error));
  appWindow.onResized?.(()=>syncMaximizeState()).catch?.(console.error);
  syncMaximizeState();

  // Тянем окно за края. data-tauri-drag-region перетаскивает само, а размер
  // меняем вручную: у окна без рамок системных зон захвата нет.
  document.querySelectorAll('[data-resize]').forEach(edge=>{
    edge.addEventListener('mousedown',event=>{
      if(event.button!==0)return;
      event.preventDefault();
      appWindow.startResizeDragging(edge.dataset.resize).catch(console.error);
    });
  });

  /* Кнопка "Обновить" рядом с кнопками окна: появляется, когда вышла новая
     версия, и остаётся на виду, в отличие от исчезающего уведомления. */
  const updateButton=document.getElementById('titlebarUpdate');
  const updateText=document.getElementById('titlebarUpdateText');
  // Пока установка идёт, состояние кнопки не трогаем: ядро шлёт событие о
  // доступной версии дважды (до и после фоновой загрузки), и второе оживляло
  // кнопку поверх уже запущенной установки.
  let installing=false;
  // Смена языка переписывает подписи по data-i18n у всех элементов, включая
  // кнопки окна и кнопку обновления. После неё возвращаем то, что зависит от
  // состояния: "Восстановить" у развёрнутого окна и "Обновление…" во время
  // установки.
  window.restoreTitlebarState=function(){
    syncMaximizeState();
    window.updateMinimizeButtonLabel?.();
    if(installing&&updateText)updateText.textContent=L('Обновление…','Updating…');
  };
  // Что нового в выпуске: окно показывается сразу по наведению на кнопку, без
  // задержки и без нажатия - список изменений виден до установки (issue #83).
  const notesBox=document.getElementById('updateNotes');
  const notesHead=document.getElementById('updateNotesHead');
  const notesBody=document.getElementById('updateNotesBody');
  let updateNotes={version:'',text:''};
  // Описание выпуска приходит из манифеста обновления, то есть снаружи
  // программы: узлы собираем сами, строку в разметку не подставляем никогда.
  function fillUpdateNotes(){
    notesBody.textContent='';
    const nodes=releaseNotes.parseReleaseNotes(updateNotes.text);
    if(!nodes.length){
      notesBody.textContent=L('Описание выпуска не приложено.','The release notes are not attached.');
      return;
    }
    let list=null;
    for(const node of nodes){
      if(node.kind==='item'){
        if(!list){list=document.createElement('ul');list.className='update-notes-list';notesBody.appendChild(list);}
        const item=document.createElement('li');item.textContent=node.text;list.appendChild(item);
        continue;
      }
      list=null;
      const block=document.createElement(node.kind==='heading'?'h4':'p');
      block.className=node.kind==='heading'?'update-notes-section':'update-notes-text';
      block.textContent=node.text;
      notesBody.appendChild(block);
    }
  }
  // Окно держится, пока указатель над кнопкой или над самим окном. Между ними
  // есть зазор, и без задержки окно исчезало раньше, чем мышь успевала его
  // пересечь - длинное описание было невозможно пролистать (issue #99).
  const NOTES_HIDE_DELAY=260;
  let notesHideTimer=null,notesPinned=false;
  function cancelNotesHide(){if(notesHideTimer){clearTimeout(notesHideTimer);notesHideTimer=null;}}
  function showUpdateNotes(){
    if(!notesBox||!updateNotes.version)return;
    cancelNotesHide();
    notesHead.textContent=L(`Что нового в truemail ${updateNotes.version}`,`What is new in truemail ${updateNotes.version}`);
    fillUpdateNotes();
    notesBox.classList.remove('hidden');
    notesBox.setAttribute('aria-hidden','false');
  }
  function hideUpdateNotes(){
    cancelNotesHide();
    if(!notesBox)return;
    notesPinned=false;
    notesBox.classList.remove('pinned');
    notesBox.classList.add('hidden');
    notesBox.setAttribute('aria-hidden','true');
    notesBox.scrollTop=0;
  }
  // Закреплённое окно уводом указателя не закрывается: пользователь читает
  // длинный список и может увести мышь куда угодно.
  function scheduleNotesHide(){
    if(notesPinned)return;
    cancelNotesHide();
    notesHideTimer=setTimeout(hideUpdateNotes,NOTES_HIDE_DELAY);
  }
  function pinUpdateNotes(){
    if(!notesBox||notesBox.classList.contains('hidden'))return;
    cancelNotesHide();
    notesPinned=true;
    notesBox.classList.add('pinned');
  }
  updateButton?.addEventListener('mouseenter',showUpdateNotes);
  updateButton?.addEventListener('focus',showUpdateNotes);
  updateButton?.addEventListener('mouseleave',scheduleNotesHide);
  updateButton?.addEventListener('blur',scheduleNotesHide);
  notesBox?.addEventListener('mouseenter',cancelNotesHide);
  notesBox?.addEventListener('mouseleave',scheduleNotesHide);
  // Прокрутка колесом и нажатие внутри окна означают, что его читают.
  notesBox?.addEventListener('wheel',pinUpdateNotes,{passive:true});
  notesBox?.addEventListener('pointerdown',pinUpdateNotes);
  document.addEventListener('pointerdown',event=>{
    if(!notesPinned)return;
    if(notesBox?.contains(event.target)||updateButton?.contains(event.target))return;
    hideUpdateNotes();
  });
  document.addEventListener('keydown',event=>{
    if(event.key==='Escape'&&notesPinned)hideUpdateNotes();
  });
  window.showUpdateButton=function(version,downloaded,notes){
    if(!updateButton||installing)return;
    updateButton.classList.remove('hidden');
    updateButton.disabled=false;
    updateNotes={version,text:typeof notes==='string'?notes.trim():''};
    updateButton.title=downloaded
      ?L(`truemail ${version} скачан, установить и перезапустить`,`truemail ${version} is downloaded, install and restart`)
      :L(`Доступен truemail ${version} - установить`,`truemail ${version} is available - install`);
    if(updateText)updateText.textContent=L('Обновить','Update');
  };
  // Единственная точка запуска установки: её зовут и кнопка здесь, и кнопка в
  // уведомлении. Иначе состояние знал бы только один путь, и пришедшее следом
  // событие о доступной версии оживляло бы кнопку поверх идущей установки.
  window.startUpdateInstall=async function(){
    if(installing)return;
    installing=true;
    if(updateButton)updateButton.disabled=true;
    if(updateText)updateText.textContent=L('Обновление…','Updating…');
    try{await window.tm.installUpdate();}
    catch(error){
      installing=false;
      if(updateButton)updateButton.disabled=false;
      if(updateText)updateText.textContent=L('Обновить','Update');
      throw error;
    }
  };
  updateButton?.addEventListener('click',()=>{
    window.startUpdateInstall().catch(error=>showToast(error));
  });

  /* Версия в строке состояния окна. Номер и адрес выпуска зашиты в интерфейс
     на сборке (modules/app-version.js), поэтому подпись не зависит ни от ядра,
     ни от готовности моста. Клик открывает сохранённый адрес во внешнем
     браузере: webview ссылку сам не откроет. */
  const versionButton=document.getElementById('appVersionLink');
  if(versionButton&&window.truemailVersion?.version){
    const {version,releaseUrl}=window.truemailVersion;
    versionButton.textContent=`v${version}`;
    versionButton.addEventListener('click',()=>{
      if(!releaseUrl||!window.tm?.openExternal)return;
      window.tm.openExternal(releaseUrl).catch(error=>showToast(error));
    });
  }
})();
