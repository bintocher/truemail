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
