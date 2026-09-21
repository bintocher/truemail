// Проверки фоновых циклов интерфейса: возврат отложенных писем и фоновая
// синхронизация читают свой срок из реестра пределов на каждом обороте, поэтому
// записанное значение действует со следующего прохода, а не после перезапуска
// программы. Проверяется настоящий путь: файл моста выполняется целиком в
// подставном окружении, время двигают виртуальные часы, а срок меняется тем же
// вызовом, каким его записывает раздел настроек.
// Спецификация: specs/configurable-limits.md, S-008, S-014.
// Запуск: node --test apps/desktop/tests/js/background-limit-loops.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {limits, applyTestLimits} = require('./limits-fixture.js');

// Виртуальные часы: фоновые сроки измеряются минутами, и ждать их по-настоящему
// значит не проверять их вовсе. Часы двигают только назначенные задания, между
// заданиями даётся отработать промисам - иначе следующий оборот цикла, который
// назначается после завершения прохода, не успел бы появиться.
function createClock() {
  let now = 0;
  let sequence = 0;
  const jobs = new Map();
  const drain = async () => { for (let step = 0; step < 50; step += 1) await Promise.resolve(); };
  const api = {
    now: () => now,
    setTimeout: (fn, ms) => {
      const id = (sequence += 1);
      jobs.set(id, {at: now + Math.max(0, Number(ms) || 0), fn, every: 0});
      return id;
    },
    setInterval: (fn, ms) => {
      const id = (sequence += 1);
      const every = Math.max(1, Number(ms) || 0);
      jobs.set(id, {at: now + every, fn, every});
      return id;
    },
    clearTimeout: id => jobs.delete(id),
    clearInterval: id => jobs.delete(id),
    async advance(ms) {
      const target = now + ms;
      for (;;) {
        let next = null;
        for (const [id, job] of jobs) {
          if (job.at <= target && (next === null || job.at < next.job.at)) next = {id, job};
        }
        if (!next) break;
        now = next.job.at;
        if (next.job.every) next.job.at = now + next.job.every;
        else jobs.delete(next.id);
        try { next.job.fn(); } catch (error) { void error; }
        await drain();
      }
      now = target;
      await drain();
    },
    drain,
  };
  return api;
}

// Ответы ядра на команды, которые мост зовёт при запуске. Счётчик обращений
// нужен только двум командам фоновых циклов, остальным хватает пустого ответа.
function createCore() {
  const calls = {release_due_snoozes: 0, sync_accounts: 0};
  const answers = {
    bootstrap_status: () => ({ready: true, data_dir: 'C:/truemail'}),
    list_accounts: () => [{id: 1, email: 'me@example.test'}],
    all_settings: () => ({onboarding_completed: 'true'}),
    list_folders: () => [],
    list_unified_sources: () => [],
    list_contacts: () => [],
    list_calendar_data: () => ({}),
    list_smart_folders: () => [],
    storage_status: () => ({}),
    list_pinned_messages: () => ({messages: [], ordinary: []}),
    overdue_message_task_count: () => 0,
    release_due_snoozes: () => 0,
  };
  const invoke = async command => {
    if (command in calls) calls[command] += 1;
    return command in answers ? answers[command]() : null;
  };
  return {calls, invoke};
}

// Мост целиком: своих экспортов у него нет, поэтому проверяется он так же, как
// работает в окне - выполнением файла.
function loadBridge(limitValues) {
  applyTestLimits(limitValues);
  const clock = createClock();
  const core = createCore();
  const documentStub = {
    hidden: false,
    visibilityState: 'visible',
    listeners: new Map(),
    addEventListener: (type, handler) => documentStub.listeners.set(type, handler),
    getElementById: () => null,
    querySelector: () => null,
    querySelectorAll: () => [],
  };
  const context = {
    console: {log() {}, info() {}, warn() {}, error() {}},
    document: documentStub,
    performance: {},
    setTimeout: clock.setTimeout,
    clearTimeout: clock.clearTimeout,
    setInterval: clock.setInterval,
    clearInterval: clock.clearInterval,
    // Соседи моста по общей области имён: мост зовёт их напрямую по дороге к
    // запуску фоновых циклов.
    showView: () => {},
    showToast: () => {},
  };
  context.window = context;
  context.globalThis = context;
  context.window.__TAURI__ = {
    core: {invoke: core.invoke},
    event: {listen: async () => () => {}},
    dialog: {},
  };
  context.window.localizationReady = Promise.resolve();
  context.window.limitsModel = limits;
  context.window.addEventListener = () => {};
  context.window.consumePendingAttachments = () => {};
  vm.createContext(context);
  vm.runInContext(
    fs.readFileSync(path.join(__dirname, '..', '..', 'ui', 'bridge.js'), 'utf8'),
    context,
  );
  return {clock, core, context};
}

// Запись значения идёт тем же путём, что и из раздела настроек: ядро отвечает
// принятым числом, реестр обновляется его ответом.
async function saveLimit(key, value) {
  await limits.saveLimit({setLimitSetting: async (_key, saved) => saved}, key, value);
}

test('S-008, S-014: укороченный срок возврата отложенных писем действует со следующего прохода', async () => {
  const {clock, core} = loadBridge({
    [limits.KEYS.snoozeReleaseSeconds]: 60,
    [limits.KEYS.backgroundSyncMinutes]: 30,
  });
  await clock.drain();
  const afterStart = core.calls.release_due_snoozes;
  assert.ok(afterStart >= 1, 'при запуске отложенные письма не проверялись ни разу');

  await clock.advance(60000);
  assert.equal(core.calls.release_due_snoozes, afterStart + 1,
    'проход по прежнему сроку не состоялся: цикл возврата отложенных писем не работает');

  // Настройка записана на ходу. Начатое ожидание доигрывает свой срок, а
  // дальше цикл обязан идти по новому значению: прочитанный один раз срок
  // держал бы прежний ритм до перезагрузки интерфейса, и отложенное письмо
  // возвращалось бы минутой позже обещанного.
  await saveLimit(limits.KEYS.snoozeReleaseSeconds, 5);
  await clock.advance(60000);
  const beforeNewPace = core.calls.release_due_snoozes;
  await clock.advance(5000);
  assert.equal(core.calls.release_due_snoozes, beforeNewPace + 1,
    'записанный срок не применился: следующий проход пошёл по прежнему ритму, а не по новому');

  // Повторная запись того же значения не добавляет второго цикла: иначе каждое
  // изменение настройки множило бы проходы, и ядро получало бы их пачками.
  await saveLimit(limits.KEYS.snoozeReleaseSeconds, 5);
  await saveLimit(limits.KEYS.snoozeReleaseSeconds, 5);
  const beforeRepeat = core.calls.release_due_snoozes;
  await clock.advance(20000);
  assert.equal(core.calls.release_due_snoozes, beforeRepeat + 4,
    'проходов вышло не по одному на срок: после повторной записи значения работает больше одного цикла');
});

test('S-008, S-014: удлинённый срок фоновой синхронизации действует со следующего прохода', async () => {
  const {clock, core} = loadBridge({
    [limits.KEYS.snoozeReleaseSeconds]: 3600,
    [limits.KEYS.backgroundSyncMinutes]: 1,
  });
  await clock.drain();
  const afterStart = core.calls.sync_accounts;
  assert.ok(afterStart >= 1, 'при запуске синхронизация ящиков не запускалась');

  await clock.advance(60000);
  assert.equal(core.calls.sync_accounts, afterStart + 1,
    'проход по прежнему сроку не состоялся: фоновая синхронизация не работает');

  // Пользователь просит ходить на сервер реже. Начатое ожидание доигрывает
  // прежнюю минуту, а дальше проходы обязаны идти по новому сроку.
  await saveLimit(limits.KEYS.backgroundSyncMinutes, 5);
  await clock.advance(60000);
  const beforeSlow = core.calls.sync_accounts;
  await clock.advance(5 * 60000 - 1000);
  assert.equal(core.calls.sync_accounts, beforeSlow,
    'проходы продолжились по прежней минуте: записанный срок фоновой синхронизации не применился');
  await clock.advance(1000);
  assert.equal(core.calls.sync_accounts, beforeSlow + 1,
    'проход по новому сроку не состоялся: цикл фоновой синхронизации остановился');
});
