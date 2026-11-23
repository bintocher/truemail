// Проверки видимого хода подключения аккаунта.
// Ключ этапа сверяется с настоящими словарями ui/locales на обоих языках, а
// показ этапа идёт настоящим путём: событие ядра - handleConnectStage -
// кнопка и заметная область мастера. Раньше проверялось только то, что ключ
// этапа - строка: пропавшая запись перевода давала пустую строку состояния, и
// проверки этого не видели.
// Спецификация: specs/account-connect-progress.md.
// Запуск: node --test apps/desktop/tests/js/connect-progress.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {createUiWindow} = require('./ui-window.js');
const {CONNECT_STAGES, connectStageMessageKey, isConnectAttemptCurrent, connectErrorText} = require('../../ui/modules/connect-progress.js');

const uiDir = path.join(__dirname, '..', '..', 'ui');
const locale = name => JSON.parse(fs.readFileSync(path.join(uiDir, 'locales', `${name}.json`), 'utf8'));
const ru = locale('ru');
const en = locale('en');

test('S-003: у каждого подтверждённого этапа есть своя запись в словарях обоих языков', () => {
  const keys = [];
  CONNECT_STAGES.forEach(stage => {
    const key = connectStageMessageKey(stage);
    assert.ok(key, `этап ${stage} остался без ключа перевода: состояние будет пустым`);
    assert.ok(!keys.includes(key), `этапы ${stage} и предыдущий делят один ключ ${key}: два состояния сольются в одно`);
    keys.push(key);
    [['ru', ru], ['en', en]].forEach(([language, dictionary]) => {
      assert.ok(typeof dictionary[key] === 'string' && dictionary[key].trim(),
        `в словаре ${language} нет текста для ключа ${key}: этап ${stage} покажется пустой строкой`);
    });
  });

  // Выдуманный этап (например, проценты выполнения) текста не получает:
  // показывать пользователю нечего, и кнопка не должна опустеть.
  assert.equal(connectStageMessageKey('percent-42'), null, 'неизвестный этап подставил текст');
  assert.equal(connectStageMessageKey(undefined), null, 'отсутствующий этап подставил текст');
});

test('S-001, S-002, S-003: пришедший из ядра этап показывается словарным текстом на языке окна', async () => {
  const ui = createUiWindow({answers: {allSettings: () => ({})}});
  await ui.clock.drain();
  await ui.evaluate('window.localizationReady');

  const status = () => ui.byId('wzConnectStatus').textContent;
  const button = () => ui.byId('wzConnect').textContent;
  for (const [language, dictionary] of [['ru', ru], ['en', en]]) {
    ui.evaluate(`applyWizardLanguage(${JSON.stringify(language)},false);`);
    await ui.clock.drain();
    ui.byId('wzEmail').value = 'me@example.test';
    // Попытка начинается так же, как по нажатию "Подключить": кнопка уходит в
    // ожидание с номером попытки, дальше этапы приходят событиями ядра.
    ui.evaluate('setConnectBusy(document.getElementById("wzConnect"),document.getElementById("wzConnectStatus"),"detecting",true,7)');
    assert.equal(status(), dictionary.connectStageDetecting,
      `начальный этап показан не словарным текстом языка ${language}`);

    for (const stage of CONNECT_STAGES) {
      ui.evaluate(`window.handleConnectStage({email:'me@example.test',stage:${JSON.stringify(stage)},attempt_id:7})`);
      await ui.clock.drain();
      const expected = dictionary[connectStageMessageKey(stage)];
      assert.equal(status(), expected, `этап ${stage} не дошёл до области состояния мастера (${language})`);
      assert.equal(button(), expected, `этап ${stage} не показан внутри самой кнопки подключения (${language})`);
    }

    // S-016 на настоящем пути: событие прежней попытки того же адреса приходит
    // с другим номером и экран не трогает.
    ui.evaluate('window.handleConnectStage({email:"me@example.test",stage:"detecting",attempt_id:6})');
    await ui.clock.drain();
    assert.equal(status(), dictionary.connectStageConnected,
      `поздний этап прежней попытки перебил состояние текущей (${language})`);
    ui.evaluate('setConnectBusy(document.getElementById("wzConnect"),null,null,false)');
  }
});

test('S-011, S-016: пришедший результат применяется только к текущей попытке на открытом экране', () => {
  const cases = [
    [3, 3, true, true, 'ответ текущей попытки на открытом экране отброшен'],
    [2, 3, true, false, 'ответ прежней попытки применён к новой'],
    [4, 3, true, false, 'ответ попытки из будущего применён к текущей'],
    [3, 3, false, false, 'ответ применён к уже закрытому экрану'],
    [2, 3, false, false, 'ответ прежней попытки применён к закрытому экрану'],
  ];
  cases.forEach(([attempt, generation, screenOpen, expected, why]) => {
    assert.equal(isConnectAttemptCurrent(attempt, generation, screenOpen), expected,
      `${why}: попытка ${attempt}, текущая ${generation}, экран ${screenOpen ? 'открыт' : 'закрыт'}`);
  });
});

test('S-006: истечение общего предела времени объясняется отдельным текстом, прочие ошибки - своим', () => {
  const timeout = 'Подключение заняло слишком много времени.';
  const cases = [
    ['timeout', 'Сервер не ответил вовремя.', timeout, timeout, 'у истечения предела времени остался общий текст'],
    ['invalid_credentials', 'Не удалось войти.', timeout, 'Не удалось войти.',
      'текст про долгое подключение подставлен ошибке входа'],
    ['network', 'Нет сети.', timeout, 'Нет сети.', 'текст про долгое подключение подставлен сетевой ошибке'],
    // Перевода для отдельного текста может не быть вовсе - тогда пользователь
    // получает общий текст, а не пустую строку.
    ['timeout', 'Сервер не ответил вовремя.', '', 'Сервер не ответил вовремя.',
      'без отдельного текста пользователь остался без объяснения'],
  ];
  cases.forEach(([kind, fallback, timeoutText, expected, why]) => {
    assert.equal(connectErrorText(kind, fallback, timeoutText), expected, `${why}: вид ошибки ${kind}`);
  });
});

const settle = async () => {
  for (let round = 0; round < 10; round += 1) await new Promise(resolve => setImmediate(resolve));
};

test('S-018, S-019, S-020, S-021: the sign-in wait is cancelled through the real screen path', async () => {
  let finishAttempt = null;
  const ui = createUiWindow({
    answers: {
      allSettings: () => ({}),
      getSetting: key => (key === 'onboarding_completed' ? 'true' : null),
      beginAccountConnection: () => new Promise((resolve, reject) => { finishAttempt = {resolve, reject}; }),
      cancelAccountConnect: () => true,
    },
    // The reload of mail data never finishes: a cancelled wait and a rejected
    // start must not wait for it.
    globals: {reloadCoreData: () => new Promise(() => {})},
  });
  await ui.clock.drain();
  await ui.evaluate('window.localizationReady');
  ui.evaluate('applyWizardLanguage("ru",false)');
  ui.evaluate('showWizard(5)');
  await settle();

  const email = 'me@example.test';
  const attempt = ui.evaluate('window.currentWizardAttempt()');
  const start = ui.byId('wzConnect');
  const status = ui.byId('wzConnectStatus');
  const cancel = ui.byId('wzConnectCancel');
  const shown = node => !node.classList.contains('hidden');
  const stage = name => ui.evaluate(
    `window.handleConnectStage({email:'${email}',stage:'${name}',attempt_id:${attempt}})`);
  const cancellations = () => ui.callsOf('cancelAccountConnect').map(call => call.args);

  ui.byId('wzEmail').value = email;
  start.onclick();
  await settle();
  assert.equal(typeof finishAttempt, 'object', 'the attempt did not reach the core');
  assert.equal(shown(cancel), false, 'cancel is offered before the sign-in wait');

  stage('waiting_code');
  assert.equal(shown(cancel), true, 'cancel is not offered during the sign-in wait');
  stage('checking_server');
  assert.equal(shown(cancel), false, 'cancel stays offered on a network stage');
  stage('waiting_code');
  ui.evaluate(
    `window.handleConnectStage({email:'${email}',stage:'detecting',attempt_id:${attempt - 1}})`);
  assert.equal(shown(cancel), true, 'a stale attempt changed the cancel button');

  cancel.onclick();
  await settle();
  assert.deepEqual(cancellations(), [[email]], 'cancel was not sent to the core for the address');
  assert.equal(shown(cancel), false, 'cancel stays on the screen after the click');
  assert.equal(start.disabled, true, 'the running attempt is still in progress');
  assert.equal(status.textContent, ru.connectStageWaitingCode, 'the stage text changed before the core answered');

  finishAttempt.resolve({mode: 'cancelled'});
  await settle();
  assert.equal(start.disabled, false, 'the connect button stays disabled after the cancellation');
  assert.equal(status.textContent, ru.connectWaitCancelled, 'the cancelled wait is not explained');
  assert.notEqual(status.dataset.kind, 'error', 'the user decision is shown as an error');

  // The wait started from another screen: the rejection offers the same cancel.
  start.onclick();
  await settle();
  finishAttempt.reject({kind: 'connect_in_progress', message: 'raw'});
  await settle();
  assert.equal(status.textContent, ru.errorConnectInProgress, 'the busy address is not explained');
  assert.equal(status.dataset.kind, 'error');
  assert.equal(shown(cancel), true, 'the busy address leaves the user without a way out');

  // The cancel follows the address: another one typed in the field hides it,
  // and a new attempt does not keep it either.
  ui.byId('wzEmail').value = 'other@example.test';
  ui.byId('wzEmail').dispatch('input');
  assert.equal(shown(cancel), false, 'the cancel of the previous address stays after the field changed');
  ui.evaluate(`window.setConnectWaitCancellable('wzConnectCancel','${email}');`);
  start.onclick();
  await settle();
  assert.equal(shown(cancel), false, 'a new attempt kept the cancel of the previous rejection');
  finishAttempt.resolve({mode: 'cancelled'});
  await settle();
  assert.equal(cancellations().length, 1, 'a cancel of the previous address was sent');

  ui.byId('wzEmail').value = email;
  start.onclick();
  await settle();
  finishAttempt.reject({kind: 'connect_in_progress', message: 'raw'});
  await settle();
  assert.equal(shown(cancel), true, 'the busy address is not offered a cancel again');
  cancel.onclick();
  await settle();
  assert.deepEqual(cancellations(), [[email], [email]], 'the cancels went for other addresses');
  assert.equal(status.textContent, ru.connectWaitCancelled, 'the stopped wait is not reported');
  assert.notEqual(status.dataset.kind, 'error', 'the stopped wait is shown as an error');

  // The same address in another case is the same address for the cancel.
  ui.byId('wzEmail').value = email.toUpperCase();
  start.onclick();
  await settle();
  finishAttempt.reject({kind: 'connect_in_progress', message: 'raw'});
  await settle();
  assert.equal(shown(cancel), true, 'the busy address in another case is not offered a cancel');
  ui.byId('wzEmail').dispatch('input');
  assert.equal(shown(cancel), true, 'the cancel was hidden although the address did not change');

  // Nothing to cancel: the stale rejection goes away.
  ui.bridge.answer('cancelAccountConnect', () => false);
  cancel.onclick();
  await settle();
  assert.equal(shown(cancel), false);
  assert.equal(status.textContent, '', 'the stale rejection stays when there was nothing to cancel');
  assert.notEqual(status.dataset.kind, 'error');
  ui.bridge.answer('cancelAccountConnect', () => true);

  // The account settings wizard follows the same path.
  ui.evaluate(`window.showAccountWizard('${email}')`);
  const settingsStart = ui.byId('accountOauthStart');
  const settingsStatus = ui.byId('accountOauthStatus');
  const settingsCancel = ui.byId('accountOauthCancel');
  assert.equal(shown(settingsCancel), false, 'a reopened screen offers a cancel of the previous one');
  settingsStart.onclick();
  await settle();
  finishAttempt.reject({kind: 'connect_in_progress', message: 'raw'});
  await settle();
  assert.equal(settingsStatus.textContent, ru.errorConnectInProgress,
    'the settings wizard does not explain the busy address');
  assert.equal(shown(settingsCancel), true, 'the settings wizard leaves the user without a way out');
  settingsCancel.onclick();
  await settle();
  assert.deepEqual(cancellations().at(-1), [email], 'the settings wizard cancelled another address');
  assert.equal(settingsStatus.textContent, ru.connectWaitCancelled, 'the settings wizard does not report the stopped wait');

  ui.evaluate('showWizard(5)');
  assert.equal(shown(cancel), false, 'a reopened wizard offers a cancel of the previous one');
});
