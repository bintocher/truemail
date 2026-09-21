// Проверки сообщений об ошибках: выбор текста и действия по виду ошибки,
// очередь карточек (ui/modules/error-presentation.js) и настоящий путь показа -
// карточка в окне, кнопка действия и снятие по времени (composer.js).
// Тексты берутся из ui/locales: записанное в проверке русское слово
// расходится с интерфейсом молча, а запасная таблица внутри модуля прячет
// пропажу ключа в каталоге переводов.
// Спецификация: specs/error-kinds-and-messages.md.
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
  planToastQueue,
  beginToastAction,
  finishToastAction,
  nextSyncToastMemo,
} = require('../../ui/modules/error-presentation.js');
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

// --- Очередь карточек ---

function item(id, kind, text, accountId = 1, action = 'retry', hasAction = false) {
  return {id, kind, text, accountId, action, hasAction};
}

test('S-008: три одинаковых сообщения схлопываются в одну карточку и продлевают показ', () => {
  let result = planToastQueue([], item('a', 'timeout', 'Ошибка'), 1000);
  result = planToastQueue(result.cards, item('b', 'timeout', 'Ошибка'), 5000);
  result = planToastQueue(result.cards, item('c', 'timeout', 'Ошибка'), 9000);
  assert.equal(result.cards.length, 1);
  assert.equal(result.cards[0].repeatCount, 3);
  assert.equal(result.cards[0].expiresAt, 18000);
});

test('S-009: четыре разные ошибки оставляют три новые в исходном порядке', () => {
  let cards = [];
  for (let index = 1; index <= 4; index += 1) cards = planToastQueue(cards, item(String(index), `kind-${index}`, `text-${index}`), index * 1000).cards;
  assert.deepEqual(cards.map(card => card.id), ['2', '3', '4']);
});

// S-008: ключ повтора строится из вида, аккаунта, текста и действия. Каждое
// поле проверяется своей парой: в общей очереди лишняя карточка тут же
// вытесняется по длине очереди, и выпадение поля из ключа остаётся незаметным.
test('S-008: вид, аккаунт, текст и действие - каждое поле входит в ключ повтора', () => {
  const base = item('a', 'timeout', 'same', 1, 'retry');
  const differences = {
    вид: item('b', 'network_unavailable', 'same', 1, 'retry'),
    аккаунт: item('b', 'timeout', 'same', 2, 'retry'),
    текст: item('b', 'timeout', 'other', 1, 'retry'),
    действие: item('b', 'timeout', 'same', 1, 'diagnostics'),
  };
  Object.entries(differences).forEach(([field, second]) => {
    const cards = planToastQueue(planToastQueue([], base, 1000).cards, second, 2000).cards;
    assert.equal(cards.length, 2, `отличается ${field} - это разные сообщения`);
    assert.deepEqual(cards.map(card => card.id), ['a', 'b'], `отличается ${field} - порядок`);
  });
  // Полное совпадение всех четырёх полей - одна карточка со счётчиком.
  const repeated = planToastQueue(planToastQueue([], base, 1000).cards, {...base, id: 'b'}, 2000).cards;
  assert.equal(repeated.length, 1);
  assert.equal(repeated[0].repeatCount, 2);
});

test('S-015: карточка с действием не получает времени автоматического скрытия', () => {
  const result = planToastQueue([], item('a', 'timeout', 'Ошибка', 1, 'retry', true), 1000);
  assert.equal(result.cards[0].expiresAt, null);
});

test('S-020: действие меняет ту же карточку на ожидание и исход', () => {
  const original = [{id: 'a', text: 'Ошибка', hasAction: true, expiresAt: null}];
  const pending = beginToastAction(original, 'a', ru('errorActionPending'));
  assert.equal(pending[0].id, 'a');
  assert.equal(pending[0].actionState, 'pending');
  assert.equal(pending[0].actionStatus, ru('errorActionPending'));
  const success = finishToastAction(pending, 'a', {ok: true, text: ru('errorActionSucceeded')}, 1000);
  assert.equal(success[0].id, 'a');
  assert.equal(success[0].text, ru('errorActionSucceeded'));
  assert.equal(success[0].hasAction, false);
  const failed = finishToastAction(pending, 'a', {ok: false, item: {text: 'Новая причина', hasAction: true}}, 1000);
  assert.equal(failed[0].id, 'a');
  assert.equal(failed[0].text, 'Новая причина');
  assert.equal(failed[0].actionState, 'failed');
});

test('S-022, S-024: сразу всплывают только виды, требующие решения человека', () => {
  ['network_unavailable', 'timeout', 'server_unavailable', 'rate_limited'].forEach(kind => {
    assert.equal(shouldShowSyncToast({kind, retries_exhausted: false}), false, kind);
  });
  ['invalid_credentials', 'needs_reauth', 'forbidden', 'certificate_error', 'account_config'].forEach(kind => {
    assert.equal(shouldShowSyncToast({kind, retries_exhausted: false}), true, kind);
  });
  assert.equal(shouldShowSyncToast({kind: 'storage_error', retries_exhausted: true}), false);
  assert.equal(shouldShowSyncToast({kind: 'server_unavailable', retries_exhausted: true}), true);
});

test('S-023: один вид объединяет аккаунты в одной карточке', () => {
  const first = {...item('a', 'server_unavailable', 'one', 1), groupByKind: true, baseText: ru('errorServerUnavailable'), accounts: [{id: 1, email: 'one@example.com'}], locale: 'ru', translations};
  const second = {...item('b', 'server_unavailable', 'two', 2), groupByKind: true, baseText: ru('errorServerUnavailable'), accounts: [{id: 2, email: 'two@example.com'}], locale: 'ru', translations};
  let cards = planToastQueue([], first, 1000).cards;
  cards = planToastQueue(cards, second, 2000).cards;
  assert.equal(cards.length, 1);
  assert.deepEqual(cards[0].accountIds, [1, 2]);
  assert.match(cards[0].text, /one@example\.com/);
  assert.match(cards[0].text, /two@example\.com/);
});

test('G3: повторный сбой того же аккаунта не дублирует обработчик действия', () => {
  const callbackA1 = () => {};
  const callbackA2 = () => {};
  const shared = {groupByKind: true, baseText: ru('errorServerUnavailable'), locale: 'ru', translations};
  const first = {...item('a', 'server_unavailable', 'one', 1), ...shared, accounts: [{id: 1, email: 'one@example.com'}], callback: callbackA1};
  const repeat = {...item('b', 'server_unavailable', 'one-again', 1), ...shared, accounts: [{id: 1, email: 'one@example.com'}], callback: callbackA2};
  let cards = planToastQueue([], first, 1000).cards;
  cards = planToastQueue(cards, repeat, 2000).cards;
  assert.equal(cards.length, 1);
  // Аккаунт уже учтён - второй обработчик не добавляется, иначе одно нажатие
  // запускало бы действие дважды.
  assert.deepEqual(cards[0].callbacks, [callbackA1]);
});

test('G4: объединение карточки очищает подробности от первого аккаунта', () => {
  const shared = {groupByKind: true, baseText: ru('errorServerUnavailable'), locale: 'ru', translations};
  const first = {...item('a', 'server_unavailable', 'one', 1), ...shared, accounts: [{id: 1, email: 'one@example.com'}], details: 'Код ответа: 503'};
  const second = {...item('b', 'server_unavailable', 'two', 2), ...shared, accounts: [{id: 2, email: 'two@example.com'}], details: 'Код ответа: 500'};
  let cards = planToastQueue([], first, 1000).cards;
  assert.equal(cards[0].details, 'Код ответа: 503');
  cards = planToastQueue(cards, second, 2000).cards;
  // Подробности относились только к первому аккаунту и вводили в заблуждение
  // насчёт второго.
  assert.equal(cards[0].details, '');
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

// --- Настоящий путь: карточка ошибки в окне ---

const ACCOUNT = {id: 1, email: 'me@example.test', display_name: 'Мой ящик'};
const cards = app => app.document.querySelectorAll('.app-toast');
const cardText = card => card.querySelector('.app-toast-line span').textContent;

async function errorApp() {
  const app = startApp({accounts: [ACCOUNT]});
  await app.ready();
  await app.setLanguage('ru');
  return app;
}

test('S-005, S-019: карточка ошибки показывает текст вида, подробности и подпись действия', async () => {
  const app = await errorApp();
  app.sandbox.window.showApiError({
    kind: 'server_unavailable', account_id: 1, message: 'transport failed',
    server: 'mail.example.test:443', response_code: 503,
  });
  const shown = cards(app);
  assert.equal(shown.length, 1);
  assert.equal(cardText(shown[0]), `Мой ящик (me@example.test): ${ru('errorServerUnavailable')}`);
  // Технические подробности спрятаны под раскрытием, а не вынесены в текст.
  assert.equal(shown[0].querySelector('summary').textContent, ru('errorDetails'));
  assert.match(shown[0].querySelector('pre').textContent, /mail\.example\.test:443/);
  assert.equal(shown[0].querySelector('button').textContent, ru('errorActionRetry'));
});

test('S-008: повтор той же ошибки показывает счётчик, а не вторую карточку', async () => {
  const app = await errorApp();
  const failure = {kind: 'timeout', account_id: 1, message: 'raw'};
  app.sandbox.window.showApiError(failure);
  app.sandbox.window.showApiError(failure);
  assert.equal(cards(app).length, 1);
  assert.equal(cards(app)[0].querySelector('.app-toast-count').textContent, 'x2');
});

test('S-020: кнопка карточки запускает действие и сообщает о ходе и исходе', async () => {
  const app = await errorApp();
  let started = 0;
  let finish = null;
  app.sandbox.window.showApiError({kind: 'timeout', account_id: 1, message: 'raw'}, {
    action: () => new Promise(resolve => {
      started += 1;
      finish = resolve;
    }),
  });
  const button = cards(app)[0].querySelector('button');
  button.onclick();
  await app.settle(3);
  assert.equal(started, 1, 'нажатие запускает действие');
  assert.equal(cards(app)[0].querySelector('.app-toast-action-status').textContent, ru('errorActionPending'));
  assert.equal(cards(app)[0].querySelector('button').disabled, true, 'повторное нажатие во время работы невозможно');
  finish();
  await app.settle(3);
  assert.equal(cardText(cards(app)[0]), ru('errorActionSucceeded'));
  assert.equal(cards(app)[0].querySelector('.app-toast-action-status'), null);
});

test('S-020: отказ действия оставляет ту же карточку с новой причиной', async () => {
  const app = await errorApp();
  app.sandbox.window.showApiError({kind: 'timeout', account_id: 1, message: 'raw'}, {
    action: () => Promise.reject({kind: 'invalid_credentials', account_id: 1}),
  });
  cards(app)[0].querySelector('button').onclick();
  await app.settle(5);
  assert.equal(cards(app).length, 1, 'вторая карточка не появляется');
  assert.equal(cardText(cards(app)[0]), `Мой ящик (me@example.test): ${ru('errorInvalidCredentials')}`);
});

test('S-015: сообщение без действия снимается по времени, карточка с действием остаётся', async () => {
  const app = await errorApp();
  app.sandbox.window.showToast('Письма отмечены прочитанными');
  assert.equal(cards(app).length, 1);
  await app.advanceTimers(9000);
  assert.equal(cards(app).length, 0, 'сообщение снято по времени');
  assert.equal(app.document.querySelector('.app-toast-stack'), null, 'пустая стопка убрана из разметки');
  // Ошибка с действием ждёт человека и сама не исчезает.
  app.sandbox.window.showApiError({kind: 'timeout', account_id: 1, message: 'raw'});
  await app.advanceTimers(60000);
  assert.equal(cards(app).length, 1);
});

test('S-015: карточку можно снять крестиком', async () => {
  const app = await errorApp();
  app.sandbox.window.showApiError({kind: 'timeout', account_id: 1, message: 'raw'});
  cards(app)[0].querySelector('.app-toast-close').onclick();
  assert.equal(cards(app).length, 0);
});
