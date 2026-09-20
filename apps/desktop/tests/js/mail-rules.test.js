// Проверки правил обработки почты: словарь условий и действий, проверка
// правила перед отправкой в ядро (ui/modules/mail-rules.js) и настоящий путь
// раздела правил - список, открытый редактор и смена языка (smart-rules.js).
// Словарь полей сверяется со словарём ядра, а не с числом полей: разошедшиеся
// словари - это условие, которое интерфейс показывает, а ядро не понимает.
// Спецификация: specs/mail-rules-conditions-and-actions.md.
// Запуск: node --test apps/desktop/tests/js/mail-rules.test.js (Node 22+).
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const rules = require('../../ui/modules/mail-rules.js');
const {limits, applyTestLimits, clearTestLimits} = require('./limits-fixture.js');
const {startApp} = require('./ui-app.js');

const CONDITION = {field: 'subject', op: 'contains', value: 'счет'};
const GROUP = {logic: 'all', conditions: [CONDITION]};

function rule(overrides = {}) {
  return Object.assign({
    id: 'r1',
    name: 'Правило',
    account_id: null,
    enabled: true,
    groups: [GROUP],
    exceptions: [],
    actions: [{kind: 'archive'}],
  }, overrides);
}

// Словарь ядра читается из исходника: пересказанная в проверке копия разошлась
// бы с ядром так же незаметно, как и копия в интерфейсе.
const ruleRs = fs.readFileSync(path.join(__dirname, '../../../../crates/core/src/model/rule.rs'), 'utf8');

function coreRuleFields() {
  const block = ruleRs.match(/pub const RULE_FIELDS[^=]*=\s*&\[([\s\S]*?)\];/);
  assert.ok(block, 'словарь полей не найден в crates/core/src/model/rule.rs');
  return [...block[1].matchAll(/\("([\w]+)",\s*RuleFieldKind::(\w+)\)/g)]
    .map(match => ({id: match[1], type: match[2].toLowerCase()}));
}

function coreEnumValues(field) {
  const block = ruleRs.match(/pub fn rule_enum_values[\s\S]*?\n}/);
  assert.ok(block, 'перечень значений не найден в crates/core/src/model/rule.rs');
  const line = block[0].split('\n').find(text => text.includes(`"${field}" =>`));
  return line ? [...line.matchAll(/"([\w]+)"/g)].map(match => match[1]).slice(1) : [];
}

// S-020, S-021, S-027: словарь полей интерфейса - тот же, что у ядра, и в том
// же порядке. Адрес отправителя - отдельное поле, роли папок ограничены
// рабочими: по служебным папкам автоматический прогон не ходит.
test('S-020, S-021, S-027: словарь полей и значений совпадает со словарём ядра', () => {
  const core = coreRuleFields();
  assert.deepEqual(
    rules.RULE_FIELDS.map(field => ({id: field.id, type: field.type})),
    core,
    'словарь полей интерфейса разошёлся с crates/core/src/model/rule.rs',
  );
  // Поле без перевода показалось бы в списке пустой строкой.
  rules.RULE_FIELDS.forEach(field => {
    assert.ok(field.ru, `нет русской подписи поля ${field.id}`);
    assert.ok(field.en, `нет английской подписи поля ${field.id}`);
  });
  // Значения перечислимых полей тоже берутся у ядра.
  // Порядок значений в списке - дело интерфейса (непрочитанное первым), а вот
  // сам набор обязан совпадать: значение сверх ядра сохранить не удастся.
  core.filter(field => field.type === 'enum').forEach(field => {
    assert.deepEqual(
      rules.ruleField(field.id).values.map(value => value[0]).sort(),
      coreEnumValues(field.id).slice().sort(),
      `значения поля ${field.id} разошлись с ядром`,
    );
  });
  assert.equal(rules.ruleField('sender_address').id, 'sender_address');
  // Неизвестное поле подменяется первым известным, а не роняет редактор.
  assert.equal(rules.ruleField('вымышленное').id, rules.RULE_FIELDS[0].id);
});

test('S-022: набор операторов зависит от вида поля, неподходящий заменяется первым допустимым', () => {
  assert.deepEqual(rules.ruleOperators('enum'), ['equals', 'not_equals']);
  assert.ok(rules.ruleOperators('text').includes('ends_with'));
  assert.ok(rules.ruleOperators('date').includes('within_last'));
  assert.ok(rules.ruleOperators('size').includes('between'));
  const normalized = rules.normalizeRuleCondition({field: 'read_state', op: 'contains', value: 'read'});
  assert.equal(normalized.op, 'equals');
  assert.equal(normalized.value, 'read');
});

// S-025: условие без значения совпало бы с каждым письмом - такое правило в
// ядро не уходит.
test('S-025: условие без пригодного значения правилом не считается', () => {
  const size = (value, extra = {}) => Object.assign({field: 'size', op: 'greater_than', value, unit: 'mb'}, extra);
  const cases = [
    [{field: 'subject', op: 'contains', value: '   '}, false, 'текст из пробелов'],
    [{field: 'subject', op: 'contains', value: 'счет'}, true, 'текст'],
    [size(''), false, 'очищенное поле размера'],
    [size('  '), false, 'пробелы вместо размера'],
    [size('десять'), false, 'слово вместо числа'],
    [size('-1'), false, 'отрицательный размер'],
    [size('10'), true, 'размер числом'],
    [size('10', {op: 'between', value2: ''}), false, 'у промежутка нет второй границы'],
    [size('10', {op: 'between', value2: '50'}), true, 'промежуток с обеими границами'],
  ];
  cases.forEach(([condition, expected, reason]) => assert.equal(rules.validRuleCondition(condition), expected, reason));
});

test('S-018, S-019: правило без условий и группа без условий отклоняются с разными причинами', () => {
  assert.equal(rules.validateRule(rule({groups: []})).reason, 'no_conditions');
  assert.equal(rules.validateRule(rule({groups: [{logic: 'all', conditions: []}]})).reason, 'no_conditions');
  // Пустая группа исключений называется ошибкой, а не выбрасывается молча:
  // иначе правило сработало бы шире, чем его составили.
  const exceptions = rules.validateRule(rule({exceptions: [{logic: 'all', conditions: []}]}));
  assert.equal(exceptions.ok, false);
  assert.equal(exceptions.reason, 'empty_group');
  assert.ok(rules.ruleErrorText('empty_group', 'ru').includes('без условий'));
});

test('S-028 - S-030: пределы групп, условий и действий берутся из настроек', t => {
  // Реестр пределов общий на весь файл: без снятия в after падение проверки
  // оставило бы чужие числа остальным.
  t.after(clearTestLimits);
  // Числа нарочно не те, что у ядра по умолчанию: правило из трёх групп
  // прежде проходило, а теперь упирается в предел, и это доказывает, что
  // модуль читает реестр, а не собственную константу.
  applyTestLimits({
    [limits.KEYS.ruleGroups]: 2,
    [limits.KEYS.groupConditions]: 2,
    [limits.KEYS.ruleActions]: 2,
  });
  const groups = count => Array.from({length: count}, () => GROUP);
  assert.equal(rules.validateRule(rule({groups: groups(2)})).ok, true);
  assert.equal(rules.validateRule(rule({groups: groups(3)})).reason, 'groups_limit');
  assert.equal(rules.validateRule(rule({exceptions: groups(3)})).reason, 'groups_limit');
  const conditions = count => [{logic: 'all', conditions: Array.from({length: count}, () => CONDITION)}];
  assert.equal(rules.validateRule(rule({groups: conditions(2)})).ok, true);
  assert.equal(rules.validateRule(rule({groups: conditions(3)})).reason, 'conditions_limit');
  const actions = count => Array.from({length: count}, () => ({kind: 'mark_read'}));
  assert.equal(rules.validateRule(rule({actions: actions(2)})).ok, true);
  assert.equal(rules.validateRule(rule({actions: actions(3)})).reason, 'actions_limit');
  // Отказ называет то же число, что стоит в настройке: прежде в тексте стояла
  // вторая копия предела и расходилась с проверкой.
  assert.equal(rules.ruleErrorText('groups_limit', 'ru'), 'Групп в правиле не больше 2');
  // Перечень пределов ещё не загружен - решение остаётся за ядром, а
  // интерфейс не выдумывает границу сам.
  clearTestLimits();
  assert.equal(rules.validateRule(rule({groups: groups(3)})).ok, true);
});

test('S-035, S-039, S-040: цепочка действий проверяется до обращения к ядру', () => {
  assert.equal(rules.validateRule(rule({actions: [{kind: 'archive'}, {kind: 'trash'}]})).reason, 'two_takeaways');
  assert.equal(rules.validateRule(rule({actions: [{kind: 'archive'}, {kind: 'mark_read'}]})).reason, 'after_takeaway');
  assert.equal(rules.validateRule(rule({actions: [{kind: 'stop'}, {kind: 'mark_read'}]})).reason, 'stop_last');
  assert.equal(rules.validateRule(rule({actions: [{kind: 'mark_read'}, {kind: 'archive'}, {kind: 'stop'}]})).ok, true);
});

test('S-038: действию с папкой нужна папка, действию с меткой - метка', () => {
  assert.equal(rules.validateRule(rule({actions: [{kind: 'move'}]})).reason, 'action_folder');
  assert.equal(rules.validateRule(rule({actions: [{kind: 'label_add'}]})).reason, 'action_label');
  assert.equal(rules.validateRule(rule({actions: [{kind: 'move', folder_id: 3}]})).ok, true);
  assert.equal(rules.validateRule(rule({actions: [{kind: 'move', folder_role: 'archive'}]})).ok, true);
});

test('S-038: пересылки по адресу в наборе действий нет, увести письмо можно одним действием', () => {
  assert.equal(rules.RULE_ACTIONS.some(action => action.id === 'forward'), false);
  assert.deepEqual(
    rules.RULE_ACTIONS.filter(action => action.takeaway).map(action => action.id),
    ['move', 'archive', 'spam', 'trash', 'delete'],
  );
});

test('S-059: перетаскивание правила даёт новый порядок, не трогая исходный список', () => {
  const ids = ['a', 'b', 'c', 'd'];
  assert.deepEqual(rules.moveRule(ids, 3, 0), ['d', 'a', 'b', 'c']);
  assert.deepEqual(rules.moveRule(ids, 0, 2), ['b', 'c', 'a', 'd']);
  assert.deepEqual(rules.moveRule(ids, 1, 1), ids);
  assert.deepEqual(rules.moveRule(ids, 1, 9), ids);
  assert.deepEqual(ids, ['a', 'b', 'c', 'd']);
});

test('S-016, S-060: нормализация не трогает признак включения и сводит логику группы к двум значениям', () => {
  assert.equal(rules.normalizeRule({enabled: false}).enabled, false);
  assert.equal(rules.normalizeRule({}).enabled, true);
  assert.equal(rules.normalizeRuleGroup({logic: 'any', conditions: [CONDITION]}).logic, 'any');
  assert.equal(rules.normalizeRuleGroup({logic: 'что-то', conditions: [CONDITION]}).logic, 'all');
});

// S-020, S-022, S-083: описание правила показывает подписи значений, единицы и
// следует выбранному языку.
test('S-020, S-022, S-083: описание правила называет значения и единицы на выбранном языке', () => {
  const context = {
    accounts: [{id: 1, email: 'me@example.test'}],
    folders: [{id: 5, display_name: 'Счета', remote_path: 'Bills'}],
    labels: [{id: 2, name: 'Важное'}],
  };
  const source = rule({account_id: 1, actions: [{kind: 'label_add', label_id: 2}, {kind: 'move', folder_id: 5}, {kind: 'stop'}]});
  const ru = rules.ruleSummary(source, Object.assign({lang: 'ru'}, context));
  const en = rules.ruleSummary(source, Object.assign({lang: 'en'}, context));
  assert.ok(ru.includes('Тема содержит'), ru);
  assert.ok(ru.includes('Поставить метку: Важное'), ru);
  assert.ok(en.includes('Subject contains'), en);
  // Имя метки задал пользователь - переводить его нельзя.
  assert.ok(en.includes('Add label: Важное'), en);
  const withUnits = rules.ruleSummary({
    id: 'r9', name: 'Правило', groups: [{logic: 'all', conditions: [
      {field: 'folder_role', op: 'equals', value: 'inbox'},
      {field: 'size', op: 'greater_than', value: '10', unit: 'mb'},
      {field: 'date', op: 'within_last', value: '24', unit: 'hours'},
    ]}], actions: [{kind: 'archive'}],
  }, {lang: 'ru'});
  assert.ok(withUnits.includes('Тип папки равно Входящие'), withUnits);
  assert.ok(withUnits.includes('Размер письма больше 10 МБ'), withUnits);
  assert.ok(withUnits.includes('Дата письма за последние 24 часов'), withUnits);
  assert.equal(rules.ruleConditionText({field: 'size', op: 'between', value: '1', value2: '5', unit: 'gb'}, 'en'), 'Message size between 1 - 5 GB');
  // Отчёт о ручном прогоне следует тому же языку.
  const report = {scanned: 10, applied: 3, queued: 2, skipped: 1, remaining: 0};
  assert.ok(rules.runReportText(report, 'ru').includes('просмотрено 10'));
  assert.ok(rules.runReportText(report, 'en').includes('scanned 10'));
});

test('S-025, S-054, S-083: состояние внимания и отказ проверки объясняются на обоих языках', () => {
  const needy = {state: 'needs_attention', attention_reason: 'label_missing'};
  assert.ok(rules.ruleStateText(needy, 'ru').includes('метка удалена'));
  assert.ok(rules.ruleStateText(needy, 'en').includes('label removed'));
  assert.equal(rules.ruleStateText({state: 'ok'}, 'ru'), '');
  assert.equal(rules.ruleErrorText('two_takeaways', 'ru'), 'Письмо можно увести только один раз');
  assert.equal(rules.ruleErrorText('two_takeaways', 'en'), 'A message can be taken away only once');
});

// --- Настоящий путь: раздел правил и смена языка ---

const RULE_ROW = {
  id: 'r1', name: 'Счета', account_id: 1, enabled: true,
  groups: [{logic: 'all', conditions: [{field: 'subject', op: 'contains', value: 'счет'}]}],
  exceptions: [], actions: [{kind: 'label_add', label_id: 2}, {kind: 'move', folder_id: 5}, {kind: 'stop'}],
  state: 'needs_attention', attention_reason: 'label_missing',
};

async function rulesApp() {
  const app = startApp({
    accounts: [{id: 1, email: 'me@example.test'}],
    folders: [{id: 5, display_name: 'Счета', remote_path: 'Bills', role: null, account_id: 1}],
    tags: [{id: 2, name: 'Важное'}],
  });
  await app.ready();
  await app.setLanguage('ru');
  app.set('mailRules', [RULE_ROW]);
  app.run('renderRulesList()');
  return app;
}

const rulesList = app => app.document.getElementById('rulesList');

test('S-083: смена языка переводит список правил вместе с описанием и состоянием', async () => {
  const app = await rulesApp();
  const row = rulesList(app).querySelector('.rule-row');
  assert.match(row.querySelector('.rule-row-description').textContent, /Тема содержит/);
  assert.match(row.querySelector('.rule-row-state').textContent, /Требует внимания: метка удалена/);
  await app.setLanguage('en');
  const translated = rulesList(app).querySelector('.rule-row');
  assert.match(translated.querySelector('.rule-row-description').textContent, /Subject contains/);
  assert.match(translated.querySelector('.rule-row-state').textContent, /Needs attention: label removed/);
  // Имя правила задал пользователь - оно остаётся как есть.
  assert.match(translated.querySelector('.rule-row-title').textContent, /Счета/);
});

test('S-083: смена языка перерисовывает открытый редактор правила вместе с набранным условием', async () => {
  const app = await rulesApp();
  app.run('openRuleEditor(null, mailRules[0])');
  const title = app.document.getElementById('ruleEditorTitle');
  assert.equal(title.textContent, 'Изменить правило');
  const fieldOptions = () => app.document.querySelector('#ruleGroups .cond-field').querySelectorAll('option').map(option => option.textContent);
  assert.ok(fieldOptions().includes('Адрес отправителя'), 'подписи полей на русском');
  // Набранное в редакторе значение переживает перерисовку: иначе смена языка
  // стирала бы несохранённое условие.
  const value = app.document.querySelector('#ruleGroups .cond-input');
  value.value = 'накладная';
  await app.setLanguage('en');
  assert.equal(title.textContent, 'Edit rule', 'заголовок редактора переведён');
  assert.ok(fieldOptions().includes('Sender address'), 'подписи полей переведены');
  assert.equal(app.document.querySelector('#ruleGroups .cond-input').value, 'накладная');
});

test('S-083: закрытый редактор смена языка не открывает', async () => {
  const app = await rulesApp();
  const editor = app.document.getElementById('ruleEditor');
  assert.equal(editor.classes.has('hidden'), true);
  await app.setLanguage('en');
  assert.equal(editor.classes.has('hidden'), true);
});
