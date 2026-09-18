// Проверки быстрых действий: словарь действий общий с правилами, состав
// цепочки, набор панели письма, сочетания слотов по коду клавиши и настоящий
// путь запуска цепочки через мост команд. Разметка здесь не участвует.
// Спецификация: specs/quick-steps.md.
// Запуск: node --test apps/desktop/tests/js/quick-steps.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const mailRules = require('../../ui/modules/mail-rules.js');

// Модуль быстрых действий появляется вместе с реализацией. Пока его нет,
// каждая проверка падает на своём месте и своими словами.
function loadQuickModule() {
  try {
    return require('../../ui/modules/quick-steps.js');
  } catch (error) {
    if (error && error.code === 'MODULE_NOT_FOUND') return null;
    throw error;
  }
}

const quick = loadQuickModule();

function fn(name, whatBreaks) {
  if (quick && typeof quick[name] === 'function') return quick[name];
  return () => {
    throw new Error(`${name}: ${whatBreaks}`);
  };
}

function value(name, whatBreaks) {
  if (quick && quick[name] !== undefined) return quick[name];
  throw new Error(`${name}: ${whatBreaks}`);
}

// Имена границы выбраны автором кода: модуль отдаёт availableActions,
// validateQuickStep, normalizeCombo, eventCombo, proposedSlotCombo, toolbarKey
// и reportText. Ниже только приведение обращений к этим именам.
function quickStepActionChoices() {
  // Словарь берётся модулем у правил самостоятельно, отдельным доводом его не
  // передают.
  return fn('availableActions', 'редактор цепочки пуст: собрать быстрое действие не из чего')();
}

// Проверка состава цепочки отдаёт причину отказа, а не готовый текст.
function quickStepError(step) {
  const check = fn('validateQuickStep', 'цепочка сохраняется без проверки: письмо уйдёт неизвестно куда')(step);
  return check.ok ? null : check.reason;
}

const builtinToolbar = [
  {id: 'reply', visible: true},
  {id: 'replyall', visible: true},
  {id: 'forward', visible: true},
  {id: 'archive', visible: true},
  {id: 'trash', visible: true},
  {id: 'spam', visible: false},
  {id: 'snooze', visible: false},
  {id: 'unread', visible: false},
  {id: 'unsub', visible: false},
  {id: 'print', visible: false},
];

test('S-001 - S-005: виды действий берутся из словаря правил, а остановка обработки в них не предлагается', () => {
  const choices = quickStepActionChoices();
  const ids = choices.map(choice => choice.id);
  for (const wanted of ['move', 'archive', 'spam', 'trash', 'delete', 'label_add', 'label_remove', 'mark_read', 'mark_flagged']) {
    assert.ok(ids.includes(wanted), `в наборе нет действия ${wanted}: ${ids.join(', ')}`);
  }
  assert.ok(!ids.includes('stop'), 'остановка обработки предложена быстрому действию, хотя прогона правил при нажатии нет');
  // Подписи и признаки берутся из общего объекта модуля правил, а не из своей
  // копии: два перечня разошлись бы при первом же изменении словаря.
  const move = choices.find(choice => choice.id === 'move');
  const ruleMove = mailRules.RULE_ACTIONS.find(action => action.id === 'move');
  assert.equal(move.ru, ruleMove.ru);
  assert.equal(move.needs, 'folder');
  assert.equal(choices.find(choice => choice.id === 'label_add').needs, 'label');
});

test('S-007 - S-012, S-015: состав цепочки проверяется до сохранения', () => {
  const check = quickStepError;
  const step = actions => ({name: 'В работу', actions});
  const move = {kind: 'move', folder_id: 3};
  assert.equal(check(step([move]), 'ru'), null);
  assert.ok(check(step([]), 'ru'), 'пустая цепочка принята: кнопка ничего не делала бы');
  assert.ok(check({name: '', actions: [move]}, 'ru'), 'быстрое действие без имени принято');
  assert.ok(check({name: 'я'.repeat(41), actions: [move]}, 'ru'), 'имя длиннее 40 символов принято');
  assert.ok(
    check(step(Array.from({length: 11}, () => ({kind: 'mark_read'}))), 'ru'),
    'одиннадцатое действие принято, хотя предел взят у правил',
  );
  assert.ok(check(step([move, {kind: 'trash'}]), 'ru'), 'два уводящих действия приняты: письмо можно увести только один раз');
  assert.ok(check(step([move, {kind: 'mark_read'}]), 'ru'), 'действие после уводящего принято: оно не выполнится');
  assert.ok(check(step([{kind: 'move'}]), 'ru'), 'перемещение без папки принято');
  assert.ok(check(step([{kind: 'label_add'}]), 'ru'), 'действие метки без метки принято');
  assert.ok(check(step([{kind: 'stop'}]), 'ru'), 'остановка обработки принята в цепочке быстрого действия');
  assert.equal(check(step([{kind: 'mark_read'}, {kind: 'mark_flagged'}, move]), 'ru'), null,
    'уводящее действие последним - обычная цепочка, а не ошибка');
});

test('S-058: слоты названы закрытым перечнем', () => {
  const slots = value('QUICK_STEP_SLOT_ACTIONS', 'имён слотов горячих клавиш нет: привязать быстрое действие к клавише нечем');
  assert.deepEqual(slots, Array.from({length: 10}, (_, index) => `quick_step_${index + 1}`));
});

test('S-062: редактор предлагает слотам сочетания', () => {
  const suggest = fn('proposedSlotCombo', 'редактор не предлагает сочетаний слотам: пользователь подбирает их вслепую');
  assert.equal(suggest(1), 'Ctrl+Shift+1');
  assert.equal(suggest(9), 'Ctrl+Shift+9');
  assert.equal(suggest(10), 'Ctrl+Shift+0');
});

test('S-063, S-065: сочетание собирается по коду клавиши, а порядок модификаторов приводится к общему виду', () => {
  // При Ctrl+Shift+1 браузер отдаёт восклицательный знак: собранное по
  // значению клавиши сочетание не совпало бы с сохранённым никогда, и все
  // десять слотов молча не работали бы.
  const combo = fn('eventCombo', 'сочетание собирается по значению клавиши: слоты с цифрами не сработают ни разу');
  assert.equal(combo({ctrlKey: true, shiftKey: true, altKey: false, metaKey: false, key: '!', code: 'Digit1'}), 'Ctrl+Shift+1');
  assert.equal(combo({ctrlKey: true, shiftKey: true, altKey: false, metaKey: false, key: ')', code: 'Digit0'}), 'Ctrl+Shift+0');
  assert.equal(combo({ctrlKey: false, shiftKey: false, altKey: false, metaKey: false, key: 'r', code: 'KeyR'}), 'R');
  assert.equal(combo({ctrlKey: true, shiftKey: false, altKey: false, metaKey: false, key: 'Control', code: 'ControlLeft'}), '');

  const normalize = fn('normalizeCombo', 'порядок модификаторов не приводится к общему виду: одно сочетание займёт два слота');
  assert.equal(normalize('Shift+Ctrl+1'), normalize('Ctrl+Shift+1'));
  assert.equal(normalize('Meta+Alt+Ctrl+K'), 'Ctrl+Alt+Meta+K');
});

test('S-049 - S-053, S-055 - S-057, S-077: набор панели строится один раз и помнит порядок быстрых действий', () => {
  const build = fn('toolbarSet', 'быстрые действия не попадают в панель: нажать их негде');
  const steps = [
    {id: 4, name: 'В работу', icon: 'bolt', position: 1, state: 'ok', hotkey_slot: 1},
    {id: 7, name: 'Разобрать', icon: 'inbox', position: 0, state: 'ok', hotkey_slot: null},
  ];
  const set = build(builtinToolbar, steps, null);
  const quickIds = set.filter(item => item.quickStepId).map(item => item.quickStepId);
  assert.deepEqual(quickIds, [7, 4], 'быстрые действия показаны не в своём порядке');
  assert.ok(
    set.filter(item => item.quickStepId).every(item => item.visible === false),
    'новое быстрое действие само перестроило панель письма',
  );
  assert.equal(set.length, builtinToolbar.length + steps.length, 'действующие десять действий панели потерялись');

  // Настройка панели читается уже после того, как быстрые действия попали в
  // набор: иначе её записи о них были бы пропущены как неизвестные.
  const layout = [
    {id: 'quick_step:4', visible: true},
    {id: 'reply', visible: true},
    {id: 'quick_step:99', visible: true},
  ];
  const restored = build(builtinToolbar, steps, layout);
  assert.equal(
    restored.find(item => item.quickStepId === 4).visible,
    true,
    'сохранённый порядок и видимость быстрого действия потерялись при запуске',
  );
  assert.equal(restored[0].id, 'quick_step:4', 'порядок из настройки панели не применён');
  assert.ok(
    !restored.some(item => item.id === 'quick_step:99'),
    'запись удалённого быстрого действия осталась в панели',
  );

  // Скрытые действия набора - это меню "Ещё", а не потерянные кнопки.
  const more = fn('toolbarMoreMenu', 'меню "Ещё" не знает быстрых действий')(restored);
  assert.ok(more.some(item => item.quickStepId === 7), 'скрытое быстрое действие пропало и из панели, и из меню');
});

test('S-032, S-039, S-042: отчёт применения называет число писем и причины пропуска', () => {
  const text = fn('reportText', 'отчёта о применении нет: пользователь не узнает, что часть писем пропущена')(
    {applied: 7, skipped: 4, skipped_busy: 1, skipped_failed: 1, skipped_no_folder: 1, skipped_foreign_account: 1},
    'ru',
  );
  assert.ok(text.includes('7'), `в отчёте нет числа применённых писем: ${text}`);
  assert.ok(text.includes('4'), `в отчёте нет числа пропущенных писем: ${text}`);
  for (const reason of [/занят/i, /отказ|жд[её]т решения/i, /папк/i, /ящик/i]) {
    assert.ok(reason.test(text), `в отчёте нет причины ${reason}: ${text}`);
  }
});

test('S-033, S-034, S-035: применение идёт по выделению, упирается в 500 писем и расширяет беседу загруженными строками', () => {
  const targets = fn('quickStepTargets', 'выбор писем для быстрого действия не задан: цепочка уйдёт не туда');
  const loaded = [
    {id: 1, thread_id: 5, folder_id: 10},
    {id: 2, thread_id: 5, folder_id: 10},
    {id: 3, thread_id: 5, folder_id: 11},
    {id: 4, thread_id: 6, folder_id: 10},
  ];
  assert.deepEqual(targets({selection: [1, 4], loaded}).ids, [1, 4]);
  // Свёрнутая беседа расширяется только по загруженным строкам текущего показа
  // и только в пределах папки исходного письма.
  assert.deepEqual(targets({collapsedThread: {id: 1, thread_id: 5, folder_id: 10}, loaded}).ids, [1, 2]);

  const limit = fn('quickStepLimitError', 'предел в 500 писем не проверяется: одно нажатие унесёт всю папку');
  assert.equal(limit(500, 'ru'), null);
  const over = limit(501, 'ru');
  assert.ok(over, '501 письмо принято к применению');
  assert.ok(over.includes('500'), `отказ не называет предел: ${over}`);
});

test('S-036, S-044, S-064, S-069 - S-073: нажатие слота ведёт цепочку через одну команду моста', async () => {
  // Сквозная проверка настоящего пути: клавиша, панель и контекстное меню
  // ведут в один обработчик, а он - в одну команду запуска.
  const calls = [];
  const bridge = {
    runQuickStep: async (stepId, messageIds) => {
      calls.push({stepId, messageIds});
      return {applied: messageIds.length, skipped: 0};
    },
  };
  const steps = [
    {id: 4, name: 'В корзину', state: 'ok', hotkey_slot: 1, actions: [{kind: 'delete'}]},
    {id: 5, name: 'В работу', state: 'needs_attention', hotkey_slot: 2, actions: [{kind: 'move', folder_id: null}]},
  ];
  const press = fn('handleSlotPress', 'нажатие слота ничего не запускает: сочетание только лежит в базе');
  const notices = [];
  const context = {
    bridge,
    steps,
    selection: [11, 12],
    activeMessage: null,
    inTextField: false,
    lang: 'ru',
    confirm: async () => true,
    notify: text => notices.push(String(text)),
  };

  const done = await press('quick_step_1', context);
  assert.equal(done.applied, 2, 'цепочка не применилась ко всем выделенным письмам');
  assert.deepEqual(calls, [{stepId: 4, messageIds: [11, 12]}], 'запуск не прошёл одной командой моста');

  // Безвозвратное удаление подтверждается при каждом запуске, а отказ отменяет
  // цепочку до её начала.
  calls.length = 0;
  await press('quick_step_1', Object.assign({}, context, {confirm: async () => false}));
  assert.deepEqual(calls, [], 'отказ от подтверждения не остановил безвозвратное удаление');

  // Быстрое действие, потерявшее цель, отказывает сразу и называет причину.
  calls.length = 0;
  notices.length = 0;
  await press('quick_step_2', context);
  assert.deepEqual(calls, [], 'быстрое действие без цели всё равно запустилось');
  assert.ok(notices.length === 1 && /цел|папк|внимани/i.test(notices[0]), `причина отказа не названа: ${notices.join(' | ')}`);

  // Свободный слот молчит, поле ввода нажатие не пускает, а пустое выделение
  // без открытого письма получает внятное сообщение.
  calls.length = 0;
  notices.length = 0;
  await press('quick_step_7', context);
  assert.deepEqual(calls, [], 'свободный слот что-то выполнил');
  assert.deepEqual(notices, [], 'свободный слот показал ошибку');
  await press('quick_step_1', Object.assign({}, context, {inTextField: true}));
  assert.deepEqual(calls, [], 'слот сработал в поле ввода и испортил набранный текст');
  await press('quick_step_1', Object.assign({}, context, {selection: [], activeMessage: null}));
  assert.deepEqual(calls, [], 'цепочка выполнилась без выбранного письма');
  assert.ok(notices.some(text => /письмо/i.test(text)), `пользователю не сказано, что письмо не выбрано: ${notices.join(' | ')}`);
});
