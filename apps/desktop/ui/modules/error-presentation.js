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
  };
}

function localeValue(key,locale,translations) {
  const selected=locale==='en'?'en':'ru';
  return translations?.[selected]?.[key]||FALLBACK[selected][key]||FALLBACK.ru[key]||key;
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
  return {
    ...normalized,
    text:ownText||localeValue(row.messageKey,locale,options.translations),
    action,
    actionLabel:localeValue(ACTION_KEYS[action],locale,options.translations),
    requiresReauth:normalized.kind==='invalid_credentials'||normalized.kind==='needs_reauth',
  };
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
  };
  queue.push(card);
  const removed=queue.length>3?queue.splice(0,queue.length-3):[];
  return {cards:queue,collapsedId:null,removedIds:removed.map(value=>value.id)};
}

const errorPresentation={ERROR_KINDS,ACTION_KEYS,normalizeApiError,presentError,errorText,toastFingerprint,planToastQueue};
if(typeof window!=='undefined')window.errorPresentation=errorPresentation;
if(typeof module!=='undefined'&&module.exports)module.exports=errorPresentation;
