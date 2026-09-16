// truemail UI module: connect-progress.js
// Общая логика видимого хода подключения аккаунта (specs/account-connect-progress.md).
// Чистые функции без DOM и без Tauri API: мастер первичной настройки и мастер
// добавления аккаунта в настройках применяют одни и те же правила (S-010).
// DOM-обвязка (кнопки, статусные строки) - в mail.js.
'use strict';

// Порядок и состав подтверждённых этапов (S-003): проценты не показываем,
// только эти пять состояний. "waiting_code" - отдельное состояние по S-009,
// в общий предел времени команды не входит; код совпадает с тем, что Rust
// присылает в событии "truemail-connect-stage" (commands.rs, emit_connect_stage).
const CONNECT_STAGES = Object.freeze(['detecting', 'waiting_code', 'checking_server', 'saving', 'connected']);

const STAGE_MESSAGE_KEYS = Object.freeze({
  detecting: 'connectStageDetecting',
  waiting_code: 'connectStageWaitingCode',
  checking_server: 'connectStageCheckingServer',
  saving: 'connectStageSaving',
  connected: 'connectStageConnected',
});

// Ключ локализации подтверждённого этапа - null для неизвестного значения
// (вызывающий код не должен подставлять выдуманный текст).
function connectStageMessageKey(stage) {
  return STAGE_MESSAGE_KEYS[stage] || null;
}

// S-016: поздний ответ прежней попытки не должен менять экран. Применяем
// пришедший этап/результат только если номер попытки - тот же, что сейчас
// активен для этого адреса, и сам экран всё ещё показан (screenOpen решает
// вызывающий код: для мастера - welcomeView.active, для настроек -
// account-wizard-mode, обе проверки уже есть в i18n-onboarding.js).
function isConnectAttemptCurrent(attempt, generation, screenOpen) {
  return attempt === generation && Boolean(screenOpen);
}

// S-006: у истечения общего предела времени - отдельный, более конкретный
// текст, чем общий "Сервер не ответил вовремя" у прочих таймаутов. Вид
// ошибки и действие ("Повторить") по-прежнему берутся из
// error-kinds-and-messages.md - здесь переопределяется только пояснение.
function connectErrorText(kind, fallbackText, timeoutText) {
  return kind === 'timeout' && timeoutText ? timeoutText : fallbackText;
}

const connectProgress = {
  CONNECT_STAGES,
  connectStageMessageKey,
  isConnectAttemptCurrent,
  connectErrorText,
};
if (typeof window !== 'undefined') window.connectProgress = connectProgress;
if (typeof module !== 'undefined' && module.exports) module.exports = connectProgress;
