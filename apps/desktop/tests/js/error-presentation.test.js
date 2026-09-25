// Проверки сообщений об ошибках: выбор текста и действия по виду ошибки,
// журнал событий строки статуса (ui/modules/activity-log.js) и настоящий путь
// показа - запись в строке и журнале, кнопка действия (composer.js).
// Тексты берутся из ui/locales: записанное в проверке русское слово
// расходится с интерфейсом молча, а запасная таблица внутри модуля прячет
// пропажу ключа в каталоге переводов.
// Спецификации: specs/error-kinds-and-messages.md, specs/status-activity-log.md.
// Запуск: node --test apps/desktop/tests/js/error-presentation.test.js (Node 22+).
'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {
  ERROR_KINDS,
  ACTION_KEYS,
  presentError,
  formatErrorDetails,
  shouldShowSyncToast,
  formatAccountErrorText,
  nextSyncToastMemo,
} = require('../../ui/modules/error-presentation.js');
const {
  createLog,
  addEntry,
  beginAction,
  finishAction,
  actionAvailable,
} = require('../../ui/modules/activity-log.js');
const {startApp} = require('./ui-window.js');

const uiRoot = path.join(__dirname, '../../ui');
const translations = {
  ru: JSON.parse(fs.readFileSync(path.join(uiRoot, 'locales/ru.json'), 'utf8')),
  en: JSON.parse(fs.readFileSync(path.join(uiRoot, 'locales/en.json'), 'utf8')),
};
const ru = key => translations.ru[key];

// S-005, S-006: каждый известный вид имеет два перевода и одно действие.
test('S-005, S-006: известные виды выбирают локализованный текст и действие', () => {
  Object.keys(ERROR_KINDS).filter(kind => kind !== 'unknown').forEach(kind => {
    const shown = presentError({kind, message: 'internal text'}, {locale: 'ru', translations, connected: true});
    const english = presentError({kind, message: 'internal text'}, {locale: 'en', translations, connected: true});
    assert.notEqual(shown.text, 'internal text', kind);
    assert.notEqual(english.text, 'internal text', kind);
    assert.notEqual(shown.text, english.text, kind);
    assert.ok(shown.action, kind);
    assert.ok(shown.actionLabel, kind);
  });
});

// S-006, S-018: у известного вида текст всегда берётся из таблицы переводов,
// а собственный текст сервера остаётся в подробностях. Иначе на экран попадал
// бы внутренний текст транспорта вместо объяснения.
test('S-006, S-018: текст известного вида - только из таблицы переводов', () => {
  const known = presentError({kind: 'timeout', message: 'какой-то внутренний текст'}, {locale: 'ru', translations});
  assert.equal(known.text, ru('errorTimeout'));
  const warning = presentError({kind: 'server_unavailable', message: 'транспорт (ews-http): HTTP 500'}, {locale: 'ru', translations});
  assert.equal(warning.text, ru('errorServerUnavailable'));
  assert.match(warning.details, /HTTP 500/);
});

test('S-006: вход выбирает действие по наличию сохранённого аккаунта', () => {
  assert.equal(presentError({kind: 'invalid_credentials', account_id: 7}, {translations, connected: true}).action, 'reconnect');
  assert.equal(presentError({kind: 'needs_reauth'}, {translations, connected: false}).action, 'check_and_retry');
  assert.equal(presentError({kind: 'timeout'}, {translations}).action, 'retry');
  assert.equal(presentError({kind: 'rate_limited', retry_at: '2026-09-16T20:00:00Z'}, {translations}).action, 'wait');
  assert.equal(presentError({kind: 'account_config'}, {translations}).action, 'settings');
  assert.equal(presentError({kind: 'crypto_error'}, {translations}).action, 'diagnostics');
});

test('S-007, S-011: неизвестный и старый ответ не классифицируются по тексту', () => {
  const old = presentError({message: 'invalid_grant rateLimitExceeded'}, {locale: 'ru', translations});
  const strange = presentError({kind: 'future_kind', message: 'password rejected'}, {locale: 'ru', translations});
  assert.equal(old.kind, 'unknown');
  assert.equal(strange.kind, 'unknown');
  // F11: вид остаётся unknown, но собственный текст программы (проверка полей,
  // отказ файловой операции) показывается, а не теряется за общим текстом.
  assert.equal(old.text, 'invalid_grant rateLimitExceeded');
  assert.equal(old.action, 'diagnostics');
});

test('F11: unknown без текста показывает общий текст, unknown с текстом - свой', () => {
  const withText = presentError({kind: 'unknown', message: 'укажите имя пользователя и IMAP-сервер'}, {locale: 'ru', translations});
  assert.equal(withText.text, 'укажите имя пользователя и IMAP-сервер');
  [{kind: 'unknown', message: ''}, {kind: 'unknown'}, {kind: 'unknown', message: '   '}].forEach(error => {
    assert.equal(presentError(error, {locale: 'ru', translations}).text, ru('errorUnknown'), JSON.stringify(error));
  });
});

test('S-016: только два вида требуют повторного входа', () => {
  Object.keys(ERROR_KINDS).forEach(kind => {
    assert.equal(
      presentError({kind}, {translations}).requiresReauth,
      kind === 'invalid_credentials' || kind === 'needs_reauth',
      kind,
    );
  });
});

test('S-017: основной текст называет нужный аккаунт', () => {
  const account = {id: 2, email: 'work@example.com', display_name: 'Работа'};
  const shown = presentError({kind: 'server_unavailable', account_id: 2, message: 'HTTP 500'}, {locale: 'ru', translations, account});
  assert.match(shown.text, /Работа \(work@example\.com\)/);
  assert.doesNotMatch(shown.text, /HTTP 500/);
  const unnamed = presentError({kind: 'timeout', account_id: 3, message: 'timeout'}, {locale: 'ru', translations, account: {id: 3, email: 'home@example.com', display_name: ''}});
  assert.match(unnamed.text, /^home@example\.com:/);
});

test('S-019: подробности содержат сервер, код, местное время и транспорт', () => {
  const details = formatErrorDetails({
    kind: 'server_unavailable',
    message: 'transport failed',
    server: 'mail.example.com:443',
    response_code: 503,
    attempted_at: '2026-09-17T10:20:30Z',
  }, {locale: 'ru', translations});
  assert.ok(details.includes(`${ru('errorDetailServer')}: mail.example.com:443`), details);
  assert.ok(details.includes(`${ru('errorDetailResponseCode')}: 503`), details);
  assert.ok(details.includes(`${ru('errorDetailAttemptedAt')}:`), details);
  assert.ok(details.includes(`${ru('errorDetailTransport')}: transport failed`), details);
  // Время показывается местным, а не так, как его прислало ядро.
  assert.doesNotMatch(details, /2026-09-17T10:20:30Z/);
});

test('S-021: первый и очередной проход имеют разные формулировки', () => {
  const first = presentError({kind: 'timeout', message: 'raw', sync_phase: 'initial'}, {locale: 'ru', translations});
  const regular = presentError({kind: 'timeout', message: 'raw', sync_phase: 'regular'}, {locale: 'ru', translations});
  assert.equal(first.text, `${ru('errorSyncInitialDeferred')} ${ru('errorTimeout')}`);
  assert.equal(regular.text, `${ru('errorSyncRegularDeferred')} ${ru('errorTimeout')}`);
  assert.notEqual(first.text, regular.text);
});

// S-007: каждый ключ, которым пользуется модуль, обязан быть в обоих каталогах
// переводов. Запасная таблица внутри модуля подменяет пропавший ключ русской
// строкой, и потеря перевода доходит до пользователя незамеченной.
test('S-007: у каждого вида ошибки и каждого действия есть перевод в обоих каталогах', () => {
  const used = [
    ...Object.values(ERROR_KINDS).map(row => row.messageKey),
    ...Object.values(ACTION_KEYS),
    'errorAccountsPrefix', 'errorSyncInitialDeferred', 'errorSyncRegularDeferred',
    'errorDetailServer', 'errorDetailResponseCode', 'errorDetailAttemptedAt', 'errorDetailTransport',
    'errorActionPending', 'errorActionSucceeded', 'errorDetails',
  ];
  used.forEach(key => {
    assert.ok(translations.ru[key], `нет русского перевода ключа ${key}`);
    assert.ok(translations.en[key], `нет английского перевода ключа ${key}`);
  });
  // И показывается именно значение каталога, а не запасная строка модуля.
  Object.entries(ERROR_KINDS).forEach(([kind, row]) => {
    ['ru', 'en'].forEach(locale => {
      assert.equal(
        presentError({kind}, {locale, translations}).text,
        translations[locale][row.messageKey],
        `${kind} на языке ${locale}`,
      );
    });
  });
});

// --- Журнал событий строки статуса (ui/modules/activity-log.js) ---

function item(kind, text, accountId = 1, action = 'retry', hasAction = false) {
  return {level: 'error', kind, text, accountId, action, hasAction};
}
const add = (log, value, time, capacity = null) => addEntry(log, value, time, capacity).log;

test('S-008: три одинаковых сообщения - одна запись со счётчиком и временем последнего', () => {
  let log = add(createLog(), item('timeout', 'Ошибка'), 1000);
  log = add(log, item('network_unavailable', 'Другая'), 2000);
  log = add(log, item('timeout', 'Ошибка'), 5000);
  log = add(log, item('timeout', 'Ошибка'), 9000);
  assert.equal(log.entries.length, 2);
  // Повтор поднимает запись наверх: строка статуса показывает именно её.
  assert.equal(log.entries[0].text, 'Ошибка');
  assert.equal(log.entries[0].count, 3);
  assert.equal(log.entries[0].time, 9000);
});

test('S-009: переполненный журнал теряет самые старые записи, новые идут первыми', () => {
  let log = createLog();
  for (let index = 1; index <= 4; index += 1) log = add(log, item(`kind-${index}`, `text-${index}`), index * 1000, 3);
  assert.deepEqual(log.entries.map(entry => entry.text), ['text-4', 'text-3', 'text-2']);
});

// S-008: ключ повтора строится из вида, аккаунта, текста и действия. Каждое
// поле проверяется своей парой.
test('S-008: вид, аккаунт, текст и действие - каждое поле входит в ключ повтора', () => {
  const base = item('timeout', 'same', 1, 'retry');
  const differences = {
    вид: item('network_unavailable', 'same', 1, 'retry'),
    аккаунт: item('timeout', 'same', 2, 'retry'),
    текст: item('timeout', 'other', 1, 'retry'),
    действие: item('timeout', 'same', 1, 'diagnostics'),
  };
  Object.entries(differences).forEach(([field, second]) => {
    const log = add(add(createLog(), base, 1000), second, 2000);
    assert.equal(log.entries.length, 2, `отличается ${field} - это разные записи`);
  });
  const repeated = add(add(createLog(), base, 1000), {...base}, 2000);
  assert.equal(repeated.entries.length, 1);
  assert.equal(repeated.entries[0].count, 2);
});

test('S-020: действие меняет ту же запись на ожидание и исход, история остаётся', () => {
  const start = add(createLog(), {...item('timeout', 'Ошибка', 1, 'retry', true), callback: () => {}}, 1000);
  const id = start.entries[0].id;
  const pending = beginAction(start, id, ru('errorActionPending'));
  assert.equal(pending.entries[0].actionState, 'pending');
  assert.equal(actionAvailable(pending.entries[0], 1000), false, 'повторное нажатие во время работы невозможно');
  const success = finishAction(pending, id, {ok: true, text: ru('errorActionSucceeded')}, 2000);
  assert.equal(success.entries[0].text, 'Ошибка', 'запись остаётся в истории как была');
  assert.equal(success.entries[0].actionStatus, ru('errorActionSucceeded'));
  assert.equal(success.entries[0].hasAction, false);
  const failed = finishAction(pending, id, {ok: false, item: {text: 'Новая причина', hasAction: true}}, 2000);
  assert.equal(failed.entries[0].id, id);
  assert.equal(failed.entries[0].text, 'Новая причина');
  assert.equal(failed.unseenError, true);
});

test('повтор после выполненного действия снова даёт действие, во время работы - не трогает его', () => {
  const first = () => {};
  const second = () => {};
  const undo = callback => ({level: 'info', kind: 'notice', text: 'Письмо перемещено в архив', action: 'Отменить', hasAction: true, callback});
  let log = add(createLog(), undo(first), 1000);
  const id = log.entries[0].id;
  // Пока отмена первого переноса выполняется, повтор не снимает ожидание.
  log = add(beginAction(log, id, 'ждём'), undo(second), 2000);
  assert.equal(log.entries[0].actionState, 'pending');
  assert.deepEqual(log.entries[0].callbacks, [first]);
  assert.equal(actionAvailable(log.entries[0], 2000), false);
  // Отмена выполнена, следующий перенос с тем же текстом снова отменяем.
  log = finishAction(log, id, {ok: true, text: 'готово'}, 3000);
  log = add(log, undo(second), 4000);
  assert.equal(log.entries.length, 1);
  assert.equal(actionAvailable(log.entries[0], 4000), true);
  assert.deepEqual(log.entries[0].callbacks, [second]);
});

test('ключ предмета различает записи с одинаковым текстом: две отправки - две отмены', () => {
  const send = key => ({level: 'info', kind: 'notice', text: 'Отправка через 5 с', action: 'undo', hasAction: true, callback: () => {}, key});
  const log = add(add(createLog(), send('send-1-8'), 1000), send('send-1-9'), 1500);
  assert.equal(log.entries.length, 2);
});

test('после отказа действия повтор прежней причины не склеивается с новой', () => {
  let log = add(createLog(), {...item('timeout', 'Таймаут', 1, 'retry', true), callback: () => {}}, 1000);
  const id = log.entries[0].id;
  log = finishAction(beginAction(log, id, 'ждём'), id, {ok: false, item: {kind: 'invalid_credentials', text: 'Нужно войти', action: 'reconnect', hasAction: true, callback: () => {}}}, 2000);
  log = add(log, {...item('timeout', 'Таймаут', 1, 'retry', true), callback: () => {}}, 3000);
  assert.deepEqual(log.entries.map(entry => entry.text), ['Таймаут', 'Нужно войти']);
});

test('новая причина после отказа не наследует ключ и срок прежнего действия', () => {
  let log = add(createLog(), {level: 'info', kind: 'notice', text: 'Отправка через 3 с', action: 'undo', hasAction: true, callback: () => {}, key: 'send-1-8', actionUntil: 5000}, 1000);
  const id = log.entries[0].id;
  const reason = {kind: 'timeout', text: 'Сервер не ответил', action: 'retry', hasAction: true, callback: () => {}};
  log = finishAction(beginAction(log, id, 'ждём'), id, {ok: false, item: reason}, 6000);
  assert.equal(actionAvailable(log.entries[0], 6000), true, 'истёкшее окно отмены не переходит к новой причине');
  log = add(log, {...reason, level: 'error'}, 7000);
  assert.equal(log.entries.length, 1, 'повтор той же причины склеивается с ней');
});

test('объединённая запись после выполненного действия берёт обработчик нового события', () => {
  const old = () => {};
  const fresh = () => {};
  let log = addEntry(createLog(), grouped('one', 1, 'one@example.com', {hasAction: true, callback: old}), 1000, null, formatAccounts).log;
  const id = log.entries[0].id;
  log = finishAction(beginAction(log, id, 'ждём'), id, {ok: true, text: 'готово'}, 2000);
  log = addEntry(log, grouped('one', 1, 'one@example.com', {hasAction: true, callback: fresh}), 3000, null, formatAccounts).log;
  assert.deepEqual(log.entries[0].callbacks, [fresh]);
  assert.equal(actionAvailable(log.entries[0], 3000), true);
});

test('действие с истёкшим сроком недоступно, запись остаётся строкой истории', () => {
  const log = add(createLog(), {level: 'info', kind: 'notice', text: 'Письмо перемещено', action: 'undo', hasAction: true, callback: () => {}, actionUntil: 5000}, 1000);
  assert.equal(actionAvailable(log.entries[0], 4999), true);
  assert.equal(actionAvailable(log.entries[0], 5000), false);
  assert.equal(log.entries.length, 1);
});

test('S-022, S-024: сразу сообщаются только виды, требующие решения человека', () => {
  ['network_unavailable', 'timeout', 'server_unavailable', 'rate_limited'].forEach(kind => {
    assert.equal(shouldShowSyncToast({kind, retries_exhausted: false}), false, kind);
  });
  ['invalid_credentials', 'needs_reauth', 'forbidden', 'certificate_error', 'account_config'].forEach(kind => {
    assert.equal(shouldShowSyncToast({kind, retries_exhausted: false}), true, kind);
  });
  assert.equal(shouldShowSyncToast({kind: 'storage_error', retries_exhausted: true}), false);
  assert.equal(shouldShowSyncToast({kind: 'server_unavailable', retries_exhausted: true}), true);
});

const formatAccounts = (baseText, accounts, value) => formatAccountErrorText(baseText, accounts, value.locale, value.translations);
const grouped = (text, accountId, email, extra = {}) => ({
  ...item('server_unavailable', text, accountId), groupByKind: true, baseText: ru('errorServerUnavailable'),
  accounts: [{id: accountId, email}], locale: 'ru', translations, ...extra,
});

test('S-023: один вид объединяет аккаунты в одной записи', () => {
  let log = addEntry(createLog(), grouped('one', 1, 'one@example.com'), 1000, null, formatAccounts).log;
  log = addEntry(log, grouped('two', 2, 'two@example.com'), 2000, null, formatAccounts).log;
  assert.equal(log.entries.length, 1);
  assert.deepEqual(log.entries[0].accountIds, [1, 2]);
  assert.match(log.entries[0].text, /one@example\.com/);
  assert.match(log.entries[0].text, /two@example\.com/);
});

test('G3: повторный сбой того же аккаунта не дублирует обработчик действия', () => {
  const callbackA1 = () => {};
  const callbackA2 = () => {};
  let log = addEntry(createLog(), grouped('one', 1, 'one@example.com', {callback: callbackA1}), 1000, null, formatAccounts).log;
  log = addEntry(log, grouped('one-again', 1, 'one@example.com', {callback: callbackA2}), 2000, null, formatAccounts).log;
  assert.equal(log.entries.length, 1);
  assert.deepEqual(log.entries[0].callbacks, [callbackA1]);
  assert.equal(log.entries[0].count, 2);
});

test('G4: объединение записи очищает подробности от первого аккаунта', () => {
  let log = addEntry(createLog(), grouped('one', 1, 'one@example.com', {details: 'Код ответа: 503'}), 1000, null, formatAccounts).log;
  assert.equal(log.entries[0].details, 'Код ответа: 503');
  log = addEntry(log, grouped('two', 2, 'two@example.com', {details: 'Код ответа: 500'}), 2000, null, formatAccounts).log;
  assert.equal(log.entries[0].details, '');
});

test('одна беда одного ящика показывается один раз, смена причины - снова', () => {
  const failure = {account_id: 7, kind: 'server_unavailable', status: 'error'};
  const first = nextSyncToastMemo({}, failure);
  assert.equal(first.show, true);
  assert.equal(nextSyncToastMemo(first.memo, failure).show, false);
  assert.equal(nextSyncToastMemo(first.memo, {account_id: 7, kind: 'invalid_credentials'}).show, true);
  // Успешный проход снимает память о беде, и та же причина сообщается заново.
  const healed = nextSyncToastMemo(first.memo, {account_id: 7, status: 'ready'});
  assert.equal(healed.show, false);
  assert.equal(nextSyncToastMemo(healed.memo, failure).show, true);
  // Разные ящики считаются отдельно.
  assert.equal(nextSyncToastMemo(first.memo, {account_id: 1, kind: 'server_unavailable'}).show, true);
});

// --- Настоящий путь: строка статуса и журнал в окне ---

const ACCOUNT = {id: 1, email: 'me@example.test', display_name: 'Мой ящик'};
const entries = app => app.document.querySelectorAll('#activityPanel .activity-entry');
const entryText = entry => entry.querySelector('.activity-entry-text').textContent;
const statusText = app => app.document.querySelector('#statusbarText .statusbar-entry-text')?.textContent ?? '';

async function errorApp(options = {}) {
  const app = startApp({accounts: [ACCOUNT], ...options});
  await app.ready();
  await app.setLanguage('ru');
  return app;
}

test('S-005, S-019: ошибка идёт в строку статуса и журнал с подробностями и действием', async () => {
  const app = await errorApp();
  app.sandbox.window.showApiError({
    kind: 'server_unavailable', account_id: 1, message: 'transport failed',
    server: 'mail.example.test:443', response_code: 503,
  });
  const expected = `Мой ящик (me@example.test): ${ru('errorServerUnavailable')}`;
  assert.equal(statusText(app), expected, 'последняя запись видна в строке статуса');
  assert.equal(app.document.querySelectorAll('.app-toast').length, 0, 'всплывающих карточек нет');
  const shown = entries(app);
  assert.equal(shown.length, 1);
  assert.equal(entryText(shown[0]), expected);
  // Технические подробности спрятаны под раскрытием, а не вынесены в текст.
  assert.equal(shown[0].querySelector('summary').textContent, ru('errorDetails'));
  assert.match(shown[0].querySelector('pre').textContent, /mail\.example\.test:443/);
  assert.equal(shown[0].querySelector('.activity-entry-action').textContent, ru('errorActionRetry'));
});

test('S-008: повтор той же ошибки показывает счётчик, а не вторую запись', async () => {
  const app = await errorApp();
  const failure = {kind: 'timeout', account_id: 1, message: 'raw'};
  app.sandbox.window.showApiError(failure);
  app.sandbox.window.showApiError(failure);
  assert.equal(entries(app).length, 1);
  assert.equal(app.document.querySelector('#statusbarText .activity-count').textContent, 'x2');
});

test('S-020: кнопка записи запускает действие и сообщает о ходе и исходе', async () => {
  const app = await errorApp();
  let started = 0;
  let finish = null;
  app.sandbox.window.showApiError({kind: 'timeout', account_id: 1, message: 'raw'}, {
    action: () => new Promise(resolve => {
      started += 1;
      finish = resolve;
    }),
  });
  entries(app)[0].querySelector('.activity-entry-action').onclick();
  await app.settle(3);
  assert.equal(started, 1, 'нажатие запускает действие');
  assert.equal(entries(app)[0].querySelector('.activity-entry-status').textContent, ru('errorActionPending'));
  assert.equal(entries(app)[0].querySelector('.activity-entry-action').disabled, true, 'повторное нажатие во время работы невозможно');
  finish();
  await app.settle(3);
  assert.equal(entries(app)[0].querySelector('.activity-entry-status').textContent, ru('errorActionSucceeded'));
  assert.equal(entries(app)[0].querySelector('.activity-entry-action'), null, 'выполненное действие снято');
});

test('S-020: отказ действия оставляет ту же запись с новой причиной', async () => {
  const app = await errorApp();
  app.sandbox.window.showApiError({kind: 'timeout', account_id: 1, message: 'raw'}, {
    action: () => Promise.reject({kind: 'invalid_credentials', account_id: 1}),
  });
  entries(app)[0].querySelector('.activity-entry-action').onclick();
  await app.settle(5);
  assert.equal(entries(app).length, 1, 'вторая запись не появляется');
  assert.equal(entryText(entries(app)[0]), `Мой ящик (me@example.test): ${ru('errorInvalidCredentials')}`);
});

test('журнал держит число записей из предела, старые уходят', async () => {
  const app = await errorApp({limits: {limit_activity_log_entries: 3, limit_activity_log_visible: 2}});
  for (let index = 1; index <= 5; index += 1) app.sandbox.window.showToast(`событие ${index}`);
  assert.deepEqual(entries(app).map(entryText), ['событие 5', 'событие 4', 'событие 3']);
  assert.equal(statusText(app), 'событие 5');
  // Сообщение без действия не исчезает по времени - это история.
  await app.advanceTimers(60000);
  assert.equal(entries(app).length, 3);
});
