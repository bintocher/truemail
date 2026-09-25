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

// Память о том, про какую беду какого ящика уже сказали. Пока причина та же,
// повторять нечего: программа продолжает пробовать сама, а человек уже знает.
// Сообщение снова появится, если причина сменилась или ящик ожил (issue #77).
function nextSyncToastMemo(memo, state) {
  const next = {...(memo || {})};
  const accountId = state?.account_id ?? state?.accountId;
  if (accountId == null) return {memo: next, show: true};
  const kind = state?.kind || state?.error_kind || null;
  const healthy = !kind && ['ready', 'syncing'].includes(state?.status);
  if (healthy) {
    delete next[accountId];
    return {memo: next, show: false};
  }
  if (!kind) return {memo: next, show: false};
  const told = next[accountId];
  if (told === kind) return {memo: next, show: false};
  next[accountId] = kind;
  return {memo: next, show: true};
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
  nextSyncToastMemo,
  presentConnectedWarnings,
};
if(typeof window!=='undefined')window.errorPresentation=errorPresentation;
if(typeof module!=='undefined'&&module.exports)module.exports=errorPresentation;
