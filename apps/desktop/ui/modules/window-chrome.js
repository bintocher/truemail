// truemail UI module: window-chrome.js
/* Своя строка заголовка: перетаскивание, кнопки окна, рамки изменения размера
   и кнопка обновления. Системного оформления у окна нет (decorations: false). */
(function(){
  const api=window.__TAURI__?.window;
  if(!api){document.getElementById('titlebar')?.classList.add('hidden');return;}
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

  document.getElementById('winMinimize')?.addEventListener('click',()=>appWindow.minimize().catch(console.error));
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
  window.showUpdateButton=function(version,downloaded){
    if(!updateButton)return;
    updateButton.classList.remove('hidden');
    updateButton.disabled=false;
    updateButton.title=downloaded
      ?L(`truemail ${version} скачан, установить и перезапустить`,`truemail ${version} is downloaded, install and restart`)
      :L(`Доступен truemail ${version}, скачивается`,`truemail ${version} is available, downloading`);
    if(updateText)updateText.textContent=L('Обновить','Update');
  };
  updateButton?.addEventListener('click',async()=>{
    updateButton.disabled=true;
    if(updateText)updateText.textContent=L('Обновление…','Updating…');
    try{await window.tm.installUpdate();}
    catch(error){
      showToast(error.message||String(error));
      updateButton.disabled=false;
      if(updateText)updateText.textContent=L('Обновить','Update');
    }
  });
})();
