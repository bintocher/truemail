// Общая оснастка проверок разделов очереди отправки, автоответа и истории
// получателей, а также карточки окна отмены. Окно собирается из настоящего
// index.html целиком (ui-window.js), время двигают виртуальные часы, ответы
// ядра задаёт сама проверка.
// Нужна затем, что дефекты этих разделов живут в связке "разметка -
// обработчик - мост": чистые функции оставались целыми, а раздел в окне
// переставал их звать, и ни одна проверка этого не замечала.
// См. specs/undo-send.md, specs/out-of-office.md, specs/recipient-history.md.

'use strict';

const {createUiWindow, createClock} = require('./ui-window.js');

const WINDOW_START = Date.parse('2026-09-20T10:00:00Z');
const TEST_ACCOUNT = {id: 3, email: 'me@example.test'};

// Перечень пределов в том виде, в каком его отдаёт ядро командой
// limit_settings. Числа проверка задаёт нарочно не такими, как у ядра по
// умолчанию: сработавший предел тогда доказывает, что окно взяло его из ответа
// ядра, а не из своей копии.
function limitPayload(values) {
  return {
    sections: [{id: 'test', title_key: 'section:test', hint_key: 'hint:test'}],
    limits: Object.entries(values).map(([key, value]) => ({
      key,
      section: 'test',
      title_key: `title:${key}`,
      hint_key: `hint:${key}`,
      unit_key: `unit:${key}`,
      default: value,
      min: 0,
      max: 1000000,
      value,
    })),
  };
}

// Окно с виртуальными часами. Время и сроки идут по одним часам: карточка окна
// отмены считает остаток от текущего времени, а перерисовывает себя по
// setInterval - разойдись эти два источника, обратный отсчёт нельзя было бы
// проверить вовсе.
function openWindow(options = {}) {
  const {answers = {}, globals = {}, locale = 'ru', accounts = [TEST_ACCOUNT]} = options;
  const clock = createClock(WINDOW_START);
  class ClockDate extends Date {
    constructor(...args) {
      if (!args.length) super(clock.now());
      else super(...args);
    }

    static now() { return clock.now(); }
  }
  const ui = createUiWindow({
    clock,
    locale,
    // Разделы очереди читают эти три перечня при каждом открытии: без ответа
    // мост вернул бы пустоту вместо перечня, и раздел упал бы не по делу.
    answers: {
      listOutboxSends: () => [],
      listRecipientHistory: () => [],
      listOutOfOfficeReplies: () => [],
      ...answers,
    },
    globals: {Date: ClockDate, ...globals},
  });
  ui.evaluate(`coreAccounts=${JSON.stringify(accounts)};coreContacts=[];`);
  return ui;
}

// Открыть раздел на нужном ящике тем же путём, что и человек: список ящиков
// раздел заполняет сам, а выбор ящика перечитывает данные.
async function openQueueSection(ui, selectId, accountId = TEST_ACCOUNT.id) {
  await ui.evaluate('reloadQueueSections()');
  await ui.clock.drain();
  const select = ui.byId(selectId);
  if (select && accountId !== null) {
    select.value = String(accountId);
    select.dispatch('change');
    await ui.clock.drain();
  }
  return ui;
}

// Список отправителей композера заполняет раздел ящиков: без него возврат
// письма не знает, от какого ящика оно было.
function fillAccountSelect(ui, accounts = [TEST_ACCOUNT]) {
  ui.evaluate(`renderAccountSettings(${JSON.stringify(accounts)},[[]],[])`);
  const select = ui.query('.from-sel');
  if (select && accounts.length) select.value = String(accounts[0].id);
  return select;
}

// Смена языка окна идёт настоящим путём: тот же вызов перерисовывает разделы,
// собранные в коде.
async function switchLanguage(ui, locale) {
  ui.evaluate(`applyWizardLanguage('${locale}',false)`);
  await ui.clock.drain();
}

const sectionRows = (ui, id) => ui.byId(id).children;
const rowButtons = row => row.descendants().filter(node => node.tag === 'button');
const rowButton = (row, label) => rowButtons(row).find(node => node.textContent === label) || null;
const rowLabels = row => rowButtons(row).map(node => node.textContent);

// Показанные человеку сообщения: журнал событий собирает сам композер, поэтому
// смотрим его в разметке окна, а не в переменной модуля. Журнал показывает
// новые записи первыми; здесь порядок обращён, чтобы последним шло последнее.
const shownMessages = ui => ui.queryAll('#activityPanel .activity-entry-text').map(node => node.textContent).reverse();

module.exports = {
  WINDOW_START,
  TEST_ACCOUNT,
  limitPayload,
  openWindow,
  openQueueSection,
  fillAccountSelect,
  switchLanguage,
  sectionRows,
  rowButtons,
  rowButton,
  rowLabels,
  shownMessages,
};
