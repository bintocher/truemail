// truemail UI module: i18n-onboarding.js
/* welcome wizard */
let wizardText={ru:{},en:{}};
window.localizationReady=Promise.all(['ru','en'].map(async locale=>{const response=await fetch(`locales/${locale}.json?v=20260720-1`);if(!response.ok)throw new Error(`locale ${locale}: HTTP ${response.status}`);wizardText[locale]=await response.json();}));
let wizardLocale='';
let pendingOauthState='';
function wt(key){return (wizardText[wizardLocale]||wizardText.en)[key]||key;}
let uiCatalog={};
const uiKeyByRussian={
  'Умные папки':'navSmartFolders','Аккаунты':'navAccounts','Календарь':'navCalendar','Контакты':'navContacts',
  'Все входящие':'navAllInbox','Все важные':'navAllImportant','Все отправленные':'navAllSent','Все черновики':'navAllDrafts',
  'Сегодня':'navToday','Непрочитанные (все)':'navUnread','С вложениями':'navWithAttachments','Ждут ответа':'navWaitingReply',
  'Ответить':'actionReply','Ответить всем':'actionReplyAll','Переслать':'actionForward','В архив':'actionArchive','Удалить':'actionDelete','Написать':'actionCompose','Отправить':'send',
  'Настройки':'settingsTitle','Общие':'setGeneral','Панель письма':'setToolbar','Сквозные папки':'setUnified','Сопоставление папок':'setFolders','Календари':'setCalendars','Хранилище':'setStorage','Темы и оформление':'setThemes','Приватность':'setPrivacy','Горячие клавиши':'setKeys'
};
function applyUiCatalog(catalog){
  uiCatalog=catalog||{};
  const walker=document.createTreeWalker(document.body,NodeFilter.SHOW_TEXT);const nodes=[];while(walker.nextNode())nodes.push(walker.currentNode);
  nodes.forEach(node=>{const raw=node.nodeValue||'',trimmed=raw.trim(),key=node.__truemailI18nKey||uiKeyByRussian[trimmed];if(key&&uiCatalog[key]){node.__truemailI18nKey=key;node.nodeValue=raw.replace(trimmed,uiCatalog[key]);}});
  const palette=document.getElementById('cmdInput');if(palette&&uiCatalog.commandPlaceholder)palette.placeholder=uiCatalog.commandPlaceholder;
  const actionKeys={reply:'actionReply',replyall:'actionReplyAll',forward:'actionForward',archive:'actionArchive',trash:'actionDelete'};
  document.querySelectorAll('.tbrow').forEach(row=>{const key=actionKeys[row.dataset.action];const label=row.querySelector('.nm');if(label&&uiCatalog[key])label.textContent=uiCatalog[key];});
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
  applyUiCatalog(wizardText[locale]);
  if(typeof relocalizeDynamic==='function')relocalizeDynamic();
  document.getElementById('wzLanguageNext').disabled=false;
  const languageSetting=document.getElementById('languageSetting');if(languageSetting)languageSetting.value=locale;
  if(persist&&window.tmStorageReady){window.tm?.setSetting('locale',locale).catch(console.error);}
}
window.applyWizardLanguage=applyWizardLanguage;
function relocalizeDynamic(){
  try{
    if(typeof setCalendarSidebarOpen==='function')setCalendarSidebarOpen(document.getElementById('calSection')?.classList.contains('sidebar-open'));
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
document.querySelectorAll('[data-wlang]').forEach(o=>o.onclick=async()=>{await window.localizationReady;applyWizardLanguage(o.dataset.wlang);});
if(wizardLocale&&wizardText[wizardLocale])applyWizardLanguage(wizardLocale,false);
document.getElementById('languageSetting').onchange=async e=>{await window.localizationReady;applyWizardLanguage(e.target.value);};
document.querySelectorAll('[data-wtheme]').forEach(o=>o.onclick=()=>{document.querySelectorAll('[data-wtheme]').forEach(x=>x.classList.toggle('sel',x===o));setTheme(o.dataset.wtheme);});
async function finishOnboarding(){try{await window.tm?.setSetting('onboarding_completed','true');await window.reloadCoreData?.();}catch(e){console.error(e);}showView('mailView');}
document.getElementById('wzFinish').onclick=finishOnboarding;
document.getElementById('restartWizard').onclick=()=>showWizard(window.tmStorageReady?5:1);

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
    passwordInput.value='';window.tmStorageReady=true;status.textContent='';configureStorageWizard(await window.tm.bootstrapStatus());wzGo(5);
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
    window.tmStorageReady=true;entropy.fill(0);entropyChunks.forEach(chunk=>chunk.fill(0));entropyChunks.length=0;entropyBytes=0;lastEntropySample=null;status.textContent='';wzGo(5);
  }catch(error){entropy.fill(0);entropyCreationStarted=false;createKeysButton.disabled=false;status.textContent=error.message||String(error);status.dataset.kind='error';}
}
createKeysButton.onclick=createStorageFromEntropy;
function showAccountWizard(prefillEmail=''){
  accountOauthState='';accountPasswordProvider='generic';
  const status=document.getElementById('accountOauthStatus'),start=document.getElementById('accountOauthStart'),confirm=document.getElementById('accountOauthConfirm'),code=document.getElementById('accountOauthCode');
  status.textContent='';status.dataset.kind='';start.disabled=false;confirm.disabled=false;code.value='';document.getElementById('accountEmail').value=typeof prefillEmail==='string'?prefillEmail:'';document.getElementById('accountConnectionDetectRow').classList.remove('hidden');document.getElementById('accountCodeRow').classList.add('hidden');document.getElementById('accountPasswordRow').classList.add('hidden');document.getElementById('accountPassword').value='';
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
