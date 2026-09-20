// Проверки чистой логики правил обработки почты.
// Спецификация: specs/mail-rules-conditions-and-actions.md.
// Запуск: node --test apps/desktop/tests/js/mail-rules.test.js (Node 22+).

const test = require('node:test');
const assert = require('node:assert/strict');
const rules = require('../../ui/modules/mail-rules.js');
const {limits, applyTestLimits, clearTestLimits} = require('./limits-fixture.js');

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

test('S-020, S-021: словарь полей перечислен целиком, адрес отправителя отдельным полем', () => {
  assert.equal(rules.RULE_FIELDS.length, 16);
  assert.equal(rules.ruleField('sender_address').id, 'sender_address');
  assert.equal(rules.ruleField('sender').id, 'sender');
  assert.equal(rules.ruleField('вымышленное').id, 'sender');
});

test('S-022: набор операторов зависит от вида поля', () => {
  assert.deepEqual(rules.ruleOperators('enum'), ['equals', 'not_equals']);
  assert.ok(rules.ruleOperators('text').includes('ends_with'));
  assert.ok(rules.ruleOperators('date').includes('within_last'));
  assert.ok(rules.ruleOperators('size').includes('between'));
});

test('S-022: неподходящий оператор заменяется первым допустимым', () => {
  const normalized = rules.normalizeRuleCondition({field: 'read_state', op: 'contains', value: 'read'});
  assert.equal(normalized.op, 'equals');
  assert.equal(normalized.value, 'read');
});

test('S-025: текстовое условие без значения не проходит проверку', () => {
  assert.equal(rules.validRuleCondition({field: 'subject', op: 'contains', value: '   '}), false);
  assert.equal(rules.validRuleCondition({field: 'subject', op: 'contains', value: 'счет'}), true);
});

test('S-027: тип папки предлагает только рабочие папки', () => {
  const values = rules.ruleField('folder_role').values.map(item => item[0]);
  assert.deepEqual(values, ['inbox', 'archive', 'other']);
});

test('S-018, S-019: пустая группа и правило без условий отклоняются', () => {
  assert.equal(rules.validateRule(rule({groups: []})).reason, 'no_conditions');
  assert.equal(rules.validateRule(rule({groups: [{logic: 'all', conditions: []}]})).reason, 'no_conditions');
  assert.equal(rules.validateRule(rule({exceptions: [{logic: 'all', conditions: []}]})).reason, 'empty_group');
});

test('S-028 - S-030: пределы групп, условий и действий берутся из настроек', () => {
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

test('S-038: пересылки по адресу в наборе действий нет', () => {
  assert.equal(rules.RULE_ACTIONS.some(action => action.id === 'forward'), false);
  assert.deepEqual(
    rules.RULE_ACTIONS.filter(action => action.takeaway).map(action => action.id),
    ['move', 'archive', 'spam', 'trash', 'delete'],
  );
});

test('S-059: перетаскивание правила даёт новый порядок без изменения исходного списка', () => {
  const ids = ['a', 'b', 'c', 'd'];
  assert.deepEqual(rules.moveRule(ids, 3, 0), ['d', 'a', 'b', 'c']);
  assert.deepEqual(rules.moveRule(ids, 0, 2), ['b', 'c', 'a', 'd']);
  assert.deepEqual(rules.moveRule(ids, 1, 1), ids);
  assert.deepEqual(rules.moveRule(ids, 1, 9), ids);
  assert.deepEqual(ids, ['a', 'b', 'c', 'd']);
});

test('S-060: нормализация правила не трогает признак включения', () => {
  assert.equal(rules.normalizeRule({enabled: false}).enabled, false);
  assert.equal(rules.normalizeRule({}).enabled, true);
});

test('S-083: подписи правила и отчёта следуют выбранному языку', () => {
  const context = {
    accounts: [{id: 1, email: 'me@example.test'}],
    folders: [{id: 5, display_name: 'Счета', remote_path: 'Bills'}],
    labels: [{id: 2, name: 'Важное'}],
  };
  const source = rule({
    account_id: 1,
    actions: [{kind: 'label_add', label_id: 2}, {kind: 'move', folder_id: 5}, {kind: 'stop'}],
  });
  const ru = rules.ruleSummary(source, Object.assign({lang: 'ru'}, context));
  const en = rules.ruleSummary(source, Object.assign({lang: 'en'}, context));
  assert.ok(ru.includes('Тема содержит'), ru);
  assert.ok(ru.includes('Поставить метку: Важное'), ru);
  assert.ok(en.includes('Subject contains'), en);
  assert.ok(en.includes('Add label: Важное'), en);
  assert.notEqual(ru, en);
  const report = {scanned: 10, applied: 3, queued: 2, skipped: 1, remaining: 0};
  assert.ok(rules.runReportText(report, 'ru').includes('просмотрено 10'));
  assert.ok(rules.runReportText(report, 'en').includes('scanned 10'));
});

test('S-054, S-083: состояние внимания объясняется на обоих языках', () => {
  const needy = {state: 'needs_attention', attention_reason: 'label_missing'};
  assert.ok(rules.ruleStateText(needy, 'ru').includes('метка удалена'));
  assert.ok(rules.ruleStateText(needy, 'en').includes('label removed'));
  assert.equal(rules.ruleStateText({state: 'ok'}, 'ru'), '');
});

test('S-025, S-083: отказ проверки объясняется текстом выбранного языка', () => {
  assert.equal(rules.ruleErrorText('two_takeaways', 'ru'), 'Письмо можно увести только один раз');
  assert.equal(rules.ruleErrorText('two_takeaways', 'en'), 'A message can be taken away only once');
});

test('S-016: логика группы принимает только "все" и "любое"', () => {
  assert.equal(rules.normalizeRuleGroup({logic: 'any', conditions: [CONDITION]}).logic, 'any');
  assert.equal(rules.normalizeRuleGroup({logic: 'что-то', conditions: [CONDITION]}).logic, 'all');
});

// Часть проверок смотрит на живые файлы интерфейса: чистых функций там нет, а
// дефект сидел именно в разметке и в связке модулей.
const readUiFile = name => require('node:fs')
  .readFileSync(require('node:path').join(__dirname, '../../ui', name), 'utf8');

test('S-025: пустое и нечисловое значение размера условием не считается', () => {
  const size = (value, extra = {}) => Object.assign({field: 'size', op: 'greater_than', value, unit: 'mb'}, extra);
  assert.equal(rules.validRuleCondition(size('')), false, 'очищенное поле совпадало бы с каждым письмом');
  assert.equal(rules.validRuleCondition(size('  ')), false);
  assert.equal(rules.validRuleCondition(size('десять')), false);
  assert.equal(rules.validRuleCondition(size('-1')), false);
  assert.equal(rules.validRuleCondition(size('10')), true);
  assert.equal(rules.validRuleCondition(size('10', {op: 'between', value2: ''})), false);
  assert.equal(rules.validRuleCondition(size('10', {op: 'between', value2: '50'})), true);
});

test('S-020, S-022: описание правила показывает подписи значений и единицы', () => {
  const summary = rules.ruleSummary({
    id: 'r9',
    name: 'Правило',
    groups: [{logic: 'all', conditions: [
      {field: 'folder_role', op: 'equals', value: 'inbox'},
      {field: 'size', op: 'greater_than', value: '10', unit: 'mb'},
      {field: 'date', op: 'within_last', value: '24', unit: 'hours'},
    ]}],
    actions: [{kind: 'archive'}],
  }, {lang: 'ru'});
  assert.ok(summary.includes('Тип папки равно Входящие'), summary);
  assert.ok(summary.includes('Размер письма больше 10 МБ'), summary);
  assert.ok(summary.includes('Дата письма за последние 24 часов'), summary);
  const english = rules.ruleConditionText({field: 'size', op: 'between', value: '1', value2: '5', unit: 'gb'}, 'en');
  assert.equal(english, 'Message size between 1 - 5 GB');
});

test('S-018: пустая группа исключений называется ошибкой, а не выбрасывается молча', () => {
  const check = rules.validateRule(rule({exceptions: [{logic: 'all', conditions: []}]}));
  assert.equal(check.ok, false);
  assert.equal(check.reason, 'empty_group');
  assert.ok(rules.ruleErrorText('empty_group', 'ru').includes('без условий'));
});

test('S-083: смена языка перерисовывает открытый редактор правила и панель прогона', () => {
  const editor = readUiFile('modules/smart-rules.js');
  assert.ok(editor.includes('window.relocalizeRuleSection=relocalizeRuleSection'), 'редактор отдаёт перерисовку наружу');
  assert.ok(/relocalizeRuleSection\(\)\{[\s\S]*renderRuleRunFolders\(\)/.test(editor), 'панель прогона пересобирается');
  const language = readUiFile('modules/i18n-onboarding.js');
  assert.ok(language.includes('window.relocalizeRuleSection?.()'), 'смена языка вызывает перерисовку раздела правил');
  const html = readUiFile('index.html');
  assert.ok(/<span id="ruleEditorTitle"><\/span>/.test(html), 'заголовок редактора собирается в коде, а не словарём подписей');
});
