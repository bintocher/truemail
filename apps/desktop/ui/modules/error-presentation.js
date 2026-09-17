// Единая таблица пользовательских сообщений и действий по виду ошибки.
// Технический текст используется только в подробностях и не определяет вид.
'use strict';

const ERROR_KINDS = Object.freeze({
  invalid_credentials: {messageKey:'errorInvalidCredentials', connectedAction:'reconnect', initialAction:'check_and_retry'},
  needs_reauth: {messageKey:'errorNeedsReauth', connectedAction:'reconnect', initialAction:'check_and_retry'},
  rate_limited: {messageKey:'errorRateLimited', action:'wait'},
  timeout: {messageKey:'errorTimeout', action:'retry'},
  network_unavailable: {messageKey:'errorNetworkUnavailable', action:'retry'},
  certificate_error: {messageKey:'errorCertificate', action:'diagnostics'},
  server_unavailable: {messageKey:'errorServerUnavailable', action:'retry'},
  forbidden: {messageKey:'errorForbidden', action:'diagnostics'},
  account_config: {messageKey:'errorAccountConfig', action:'settings'},
  storage_error: {messageKey:'errorStorage', action:'diagnostics'},
  secret_store_error: {messageKey:'errorSecretStore', action:'diagnostics'},
  crypto_error: {messageKey:'errorCrypto', action:'diagnostics'},
  unknown: {messageKey:'errorUnknown', action:'diagnostics'},
});

const ACTION_KEYS = Object.freeze({
  reconnect:'errorActionReconnect',
  check_and_retry:'errorActionCheckAndRetry',
  retry:'errorActionRetry',
  wait:'errorActionWait',
  settings:'errorActionSettings',
  diagnostics:'errorActionDiagnostics',
});

const IMMEDIATE_SYNC_KINDS = new Set([
  'invalid_credentials',
  'needs_reauth',
  'forbidden',
  'certificate_error',
  'account_config',
]);
const RETRYABLE_SYNC_KINDS = new Set([
  'rate_limited',
  'timeout',
  'network_unavailable',
  'server_unavailable',
]);

const FALLBACK = Object.freeze({
  ru:{
    errorInvalidCredentials:'Не удалось войти в аккаунт. Проверьте данные входа.',
    errorNeedsReauth:'Нужно снова войти в аккаунт.',
    errorRateLimited:'Сервер временно ограничил запросы. Повторите позже.',
    errorTimeout:'Сервер не ответил вовремя.',
    errorNetworkUnavailable:'Нет соединения с сетью.',
    errorCertificate:'Не удалось проверить сертификат сервера.',
    errorServerUnavailable:'Почтовый сервер временно недоступен.',
    errorForbidden:'Сервер запретил это действие.',
    errorAccountConfig:'Проверьте настройки аккаунта.',
    errorStorage:'Не удалось обратиться к локальному хранилищу.',
    errorSecretStore:'Не удалось обратиться к системному хранилищу секретов.',
    errorCrypto:'Не удалось расшифровать локальные данные.',
    errorUnknown:'Не удалось выполнить действие.',
    errorActionReconnect:'Переподключить',
    errorActionCheckAndRetry:'Проверить данные и повторить',
    errorActionRetry:'Повторить',
    errorActionWait:'Повторить позже',
    errorActionSettings:'Открыть настройки',
    errorActionDiagnostics:'Открыть диагностику',
    errorAccountsPrefix:'Аккаунты',
    errorSyncInitialDeferred:'Первая синхронизация отложена.',
    errorSyncRegularDeferred:'Очередная синхронизация отложена.',
    errorDetailServer:'Сервер',
    errorDetailResponseCode:'Код ответа',
    errorDetailAttemptedAt:'Время попытки',
    errorDetailTransport:'Причина транспорта',
    errorActionPending:'Выполняется...',
    errorActionSucceeded:'Действие выполнено.',
  },
  en:{
    errorInvalidCredentials:'Could not sign in. Check the account credentials.',
    errorNeedsReauth:'Sign in to the account again.',
    errorRateLimited:'The server temporarily limited requests. Try again later.',
    errorTimeout:'The server did not respond in time.',
    errorNetworkUnavailable:'There is no network connection.',
    errorCertificate:'The server certificate could not be verified.',
    errorServerUnavailable:'The mail server is temporarily unavailable.',
    errorForbidden:'The server did not allow this action.',
    errorAccountConfig:'Check the account settings.',
    errorStorage:'Local storage could not be accessed.',
    errorSecretStore:'The system secret store could not be accessed.',
    errorCrypto:'Local data could not be decrypted.',
    errorUnknown:'The action could not be completed.',
    errorActionReconnect:'Reconnect',
    errorActionCheckAndRetry:'Check details and retry',
    errorActionRetry:'Retry',
    errorActionWait:'Retry later',
    errorActionSettings:'Open settings',
    errorActionDiagnostics:'Open diagnostics',
    errorAccountsPrefix:'Accounts',
    errorSyncInitialDeferred:'The first synchronization was postponed.',
    errorSyncRegularDeferred:'The current synchronization was postponed.',
    errorDetailServer:'Server',
    errorDetailResponseCode:'Response code',
    errorDetailAttemptedAt:'Attempt time',
    errorDetailTransport:'Transport reason',
    errorActionPending:'Working...',
    errorActionSucceeded:'Action completed.',
  },
});

function normalizeApiError(error) {
  const object=error&&typeof error==='object'?error:null;
  const requested=object&&typeof object.kind==='string'?object.kind:'';
  const kind=Object.hasOwn(ERROR_KINDS,requested)?requested:'unknown';
  // F11: у объекта без строкового message (например {kind:'unknown'} без
  // текста) details должен остаться пустым, а не строковым представлением
  // самого объекта ("[object Object]") - иначе после показа собственного
  // текста unknown-ошибок эта заглушка попала бы на экран как есть.
  const details=object
    ?(typeof object.message==='string'?object.message:'')
    :typeof error==='string'?error:String(error??'');
  return {
    kind,
    message:details,
    accountId:object?.account_id??null,
    retryAt:object?.retry_at??null,
    server:object?.server??null,
    responseCode:object?.response_code??null,
    attemptedAt:object?.attempted_at??null,
    syncPhase:object?.sync_phase??null,
  };
}

function localeValue(key,locale,translations) {
  const selected=locale==='en'?'en':'ru';
  return translations?.[selected]?.[key]||FALLBACK[selected][key]||FALLBACK.ru[key]||key;
}

function accountLabel(account) {
  if(!account)return '';
  const email=String(account.email||'').trim();
  const name=String(account.display_name||'').trim();
  return name&&name!==email?`${name} (${email})`:email;
}

function formatAccountErrorText(baseText,accounts,locale='ru',translations) {
  const labels=(accounts||[]).map(accountLabel).filter(Boolean);
  if(labels.length===0)return baseText;
  if(labels.length===1)return `${labels[0]}: ${baseText}`;
  return `${localeValue('errorAccountsPrefix',locale,translations)} ${labels.join(', ')}: ${baseText}`;
}

function formatErrorDetails(error,options={}) {
  const normalized=normalizeApiError(error);
  const locale=options.locale==='en'?'en':'ru';
  const lines=[];
  if(normalized.server)lines.push(`${localeValue('errorDetailServer',locale,options.translations)}: ${normalized.server}`);
  if(normalized.responseCode!==null&&normalized.responseCode!==undefined)lines.push(`${localeValue('errorDetailResponseCode',locale,options.translations)}: ${normalized.responseCode}`);
  if(normalized.attemptedAt){
    const date=new Date(normalized.attemptedAt);
    const value=Number.isNaN(date.getTime())?normalized.attemptedAt:date.toLocaleString(locale==='en'?'en-US':'ru-RU');
    lines.push(`${localeValue('errorDetailAttemptedAt',locale,options.translations)}: ${value}`);
  }
  if(normalized.message)lines.push(`${localeValue('errorDetailTransport',locale,options.translations)}: ${normalized.message}`);
  return lines.join('\n');
}

function presentError(error,options={}) {
  const normalized=normalizeApiError(error);
  const row=ERROR_KINDS[normalized.kind];
  const connected=options.connected??normalized.accountId!==null;
  const action=row.connectedAction
    ?(connected?row.connectedAction:row.initialAction)
    :row.action;
  const locale=options.locale==='en'?'en':'ru';
  // F11: у вида unknown message - это уже собственный текст программы
  // (проверка полей, отказ второй попытки подключения, локальный отказ
  // файловой операции), а не строка от внешнего сервера - его разбор по
  // смыслу здесь не идёт (S-011 остаётся про определение вида, не про выбор
  // текста), поэтому есть смысл показать его вместо общего "Не удалось
  // выполнить действие". Общий текст остаётся только когда своего текста нет
  // вовсе. Для всех остальных, уже классифицированных видов поведение не
  // меняется - текст всегда локализованный по таблице.
  const ownText=normalized.kind==='unknown'&&typeof normalized.message==='string'
    ?normalized.message.trim()
    :'';
  let baseText=ownText||localeValue(row.messageKey,locale,options.translations);
  if(normalized.syncPhase==='initial')baseText=`${localeValue('errorSyncInitialDeferred',locale,options.translations)} ${baseText}`;
  if(normalized.syncPhase==='regular')baseText=`${localeValue('errorSyncRegularDeferred',locale,options.translations)} ${baseText}`;
  const accounts=options.accounts||[options.account].filter(Boolean);
  return {
    ...normalized,
    baseText,
    accounts,
    text:formatAccountErrorText(baseText,accounts,locale,options.translations),
    details:formatErrorDetails(error,options),
    locale,
    action,
    actionLabel:localeValue(ACTION_KEYS[action],locale,options.translations),
    requiresReauth:normalized.kind==='invalid_credentials'||normalized.kind==='needs_reauth',
  };
}

function shouldShowSyncToast(state) {
  const kind=state?.kind||state?.error_kind||'unknown';
  if(IMMEDIATE_SYNC_KINDS.has(kind))return true;
  return RETRYABLE_SYNC_KINDS.has(kind)&&state?.retries_exhausted===true;
}

function presentConnectedWarnings(warnings,options={}) {
  return (warnings||[]).map(warning=>{
    if(typeof warning==='string')return warning;
    return presentError(warning,options).text;
  });
}

function errorText(error,options={}) {
  if(error instanceof Error&&!Object.hasOwn(error,'kind'))return error.message;
  return presentError(error,options).text;
}

function toastFingerprint(item) {
  return JSON.stringify([
    item.kind||'notice',
    item.accountId??null,
    item.text||'',
    item.action||'',
  ]);
}

function planToastQueue(cards,item,now=Date.now()) {
  const queue=(cards||[]).map(card=>({...card}));
  const grouped=item.groupByKind&&item.accountId!=null
    ?queue.find(card=>card.groupByKind&&card.kind===item.kind)
    :null;
  if(grouped){
    const accounts=[...(grouped.accounts||[])];
    const alreadyIncluded=accounts.some(account=>Number(account.id)===Number(item.accountId));
    if(!alreadyIncluded&&item.accounts?.[0])accounts.push(item.accounts[0]);
    grouped.accounts=accounts;
    grouped.accountIds=[...new Set([...(grouped.accountIds||[grouped.accountId]),item.accountId])];
    if(!alreadyIncluded){
      // Обработчик действия добавляется только для нового аккаунта в карточке -
      // повторный сбой уже учтённого аккаунта не должен плодить в списке
      // ещё одну копию того же callback, которая сработает по тому же
      // нажатию (G3, error-kinds-and-messages.md).
      grouped.callbacks=[...(grouped.callbacks||[grouped.callback]).filter(Boolean),...(item.callbacks||[item.callback]).filter(Boolean)];
      // Подробности относились только к первому аккаунту и вводили в
      // заблуждение насчёт второго - при объединении общие подробности не
      // показываем, они остаются только у карточки с одним аккаунтом (G4).
      grouped.details='';
    }
    grouped.text=formatAccountErrorText(grouped.baseText||item.baseText,accounts,item.locale,item.translations);
    if(alreadyIncluded)grouped.repeatCount=(grouped.repeatCount||1)+1;
    grouped.lastSeen=now;
    grouped.expiresAt=item.hasAction?null:now+9000;
    return {cards:queue,collapsedId:grouped.id,removedIds:[]};
  }
  const fingerprint=toastFingerprint(item);
  const repeated=queue.find(card=>card.fingerprint===fingerprint&&now-card.lastSeen<=10000);
  if(repeated){
    repeated.repeatCount=(repeated.repeatCount||1)+1;
    repeated.lastSeen=now;
    repeated.expiresAt=item.hasAction?null:now+9000;
    return {cards:queue,collapsedId:repeated.id,removedIds:[]};
  }
  const card={
    ...item,
    id:item.id??`toast-${now}`,
    fingerprint,
    repeatCount:1,
    lastSeen:now,
    expiresAt:item.hasAction?null:now+9000,
    accountIds:item.accountId==null?[]:[item.accountId],
    callbacks:(item.callbacks||[item.callback]).filter(Boolean),
  };
  queue.push(card);
  const removed=queue.length>3?queue.splice(0,queue.length-3):[];
  return {cards:queue,collapsedId:null,removedIds:removed.map(value=>value.id)};
}

function beginToastAction(cards,id,pendingText) {
  return (cards||[]).map(card=>card.id===id?{
    ...card,
    actionState:'pending',
    actionStatus:pendingText,
    expiresAt:null,
  }:{...card});
}

function finishToastAction(cards,id,replacement,now=Date.now()) {
  return (cards||[]).map(card=>{
    if(card.id!==id)return {...card};
    if(replacement.ok)return {
      ...card,
      text:replacement.text,
      details:'',
      hasAction:false,
      actionState:'success',
      actionStatus:'',
      expiresAt:now+9000,
    };
    return {
      ...card,
      ...replacement.item,
      id:card.id,
      actionState:'failed',
      actionStatus:'',
      lastSeen:now,
      expiresAt:replacement.item.hasAction?null:now+9000,
    };
  });
}

const errorPresentation={
  ERROR_KINDS,
  ACTION_KEYS,
  normalizeApiError,
  presentError,
  errorText,
  accountLabel,
  formatAccountErrorText,
  formatErrorDetails,
  shouldShowSyncToast,
  presentConnectedWarnings,
  toastFingerprint,
  planToastQueue,
  beginToastAction,
  finishToastAction,
};
if(typeof window!=='undefined')window.errorPresentation=errorPresentation;
if(typeof module!=='undefined'&&module.exports)module.exports=errorPresentation;
