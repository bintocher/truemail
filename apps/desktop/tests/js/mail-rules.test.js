// Проверки чистой логики правил обработки почты.
// Спецификация: specs/mail-rules-conditions-and-actions.md.
// Запуск: node --test apps/desktop/tests/js/mail-rules.test.js (Node 22+).

const test = require('node:test');
const assert = require('node:assert/strict');
const rules = require('../../ui/modules/mail-rules.js');

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

test('S-028 - S-030: пределы групп, условий и действий', () => {
  const manyGroups = Array.from({length: rules.RULE_MAX_GROUPS + 1}, () => GROUP);
  assert.equal(rules.validateRule(rule({groups: manyGroups})).reason, 'groups_limit');
  const manyConditions = [{logic: 'all', conditions: Array.from({length: rules.RULE_MAX_CONDITIONS + 1}, () => CONDITION)}];
  assert.equal(rules.validateRule(rule({groups: manyConditions})).reason, 'conditions_limit');
  const manyActions = Array.from({length: rules.RULE_MAX_ACTIONS + 1}, () => ({kind: 'mark_read'}));
  assert.equal(rules.validateRule(rule({actions: manyActions})).reason, 'actions_limit');
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
