// truemail UI module: mail-rules.js
// Чистые функции без DOM и Tauri API: словарь полей, операторов и действий
// правила, нормализация групп и действий, проверки состава, порядок правил и
// подписи для списка и отчёта. Подключается в index.html перед smart-rules.js
// как обычный скрипт и отдаёт функции через один глобальный объект.
// См. specs/mail-rules-conditions-and-actions.md.

// Пределы те же, что проверяет ядро (S-028 - S-030): интерфейс объясняет отказ
// заранее, но решение всё равно принимает ядро.
const RULE_MAX_GROUPS = 10;
const RULE_MAX_CONDITIONS = 10;
const RULE_MAX_ACTIONS = 10;

// Словарь полей условий (S-020). Идентификатор sender означает склейку имени и
// адреса, точный адрес живёт под собственным sender_address (S-021) и в словарь
// умных папок не попадает.
const RULE_FIELDS = [
  {id: 'sender', ru: 'Отправитель', en: 'Sender', type: 'text'},
  {id: 'sender_address', ru: 'Адрес отправителя', en: 'Sender address', type: 'text'},
  {id: 'recipient', ru: 'Получатель', en: 'Recipient', type: 'text'},
  {id: 'subject', ru: 'Тема', en: 'Subject', type: 'text'},
  {id: 'body', ru: 'Текст и предпросмотр', en: 'Text and preview', type: 'text'},
  {id: 'account', ru: 'Ящик', en: 'Mailbox', type: 'text'},
  {id: 'folder', ru: 'Название папки', en: 'Folder name', type: 'text'},
  // Роли ограничены рабочими папками: автоматический прогон служебные папки не
  // берёт, и условие по ним не сработало бы никогда (S-027).
  {id: 'folder_role', ru: 'Тип папки', en: 'Folder type', type: 'enum', values: [
    ['inbox', 'Входящие', 'Inbox'],
    ['archive', 'Архив', 'Archive'],
    ['other', 'Другая', 'Other'],
  ]},
  {id: 'read_state', ru: 'Прочтение', en: 'Read state', type: 'enum', values: [
    ['unread', 'Непрочитано', 'Unread'],
    ['read', 'Прочитано', 'Read'],
  ]},
  {id: 'importance', ru: 'Важность', en: 'Importance', type: 'enum', values: [
    ['flagged', 'Важное', 'Important'],
    ['normal', 'Обычное', 'Normal'],
  ]},
  {id: 'reply_state', ru: 'Ответ', en: 'Reply state', type: 'enum', values: [
    ['answered', 'На письмо отвечено', 'Answered'],
    ['unanswered', 'На письмо не отвечено', 'Not answered'],
  ]},
  {id: 'draft_state', ru: 'Черновик', en: 'Draft state', type: 'enum', values: [
    ['draft', 'Черновик', 'Draft'],
    ['not_draft', 'Не черновик', 'Not a draft'],
  ]},
  {id: 'attachment', ru: 'Вложения', en: 'Attachments', type: 'enum', values: [
    ['has', 'Есть вложения', 'Has attachments'],
    ['none', 'Нет вложений', 'No attachments'],
  ]},
  {id: 'size', ru: 'Размер письма', en: 'Message size', type: 'size'},
  {id: 'label', ru: 'Метка', en: 'Label', type: 'text'},
  {id: 'date', ru: 'Дата письма', en: 'Message date', type: 'date'},
];

// Операторы (S-022).
const RULE_OPS = {
  contains: ['содержит', 'contains'],
  not_contains: ['не содержит', 'does not contain'],
  equals: ['равно', 'equals'],
  not_equals: ['не равно', 'does not equal'],
  starts_with: ['начинается с', 'starts with'],
  ends_with: ['заканчивается на', 'ends with'],
  within_last: ['за последние', 'within last'],
  older_than: ['старше чем', 'older than'],
  before: ['раньше даты', 'before'],
  after: ['позже даты', 'after'],
  on: ['точно в дату', 'on'],
  greater_than: ['больше', 'greater than'],
  greater_or_equal: ['не меньше', 'at least'],
  less_than: ['меньше', 'less than'],
  less_or_equal: ['не больше', 'at most'],
  between: ['между', 'between'],
};

const RULE_DATE_UNITS = [['minutes', 'минут', 'minutes'], ['hours', 'часов', 'hours'], ['days', 'дней', 'days'], ['weeks', 'недель', 'weeks']];
const RULE_SIZE_UNITS = [['kb', 'КБ', 'KB'], ['mb', 'МБ', 'MB'], ['gb', 'ГБ', 'GB']];

// Набор действий (S-038). Пересылки по адресу здесь нет: она вынесена в
// отдельную задачу.
const RULE_ACTIONS = [
  {id: 'move', ru: 'Переместить в папку', en: 'Move to folder', needs: 'folder', takeaway: true},
  {id: 'archive', ru: 'В архив', en: 'Archive', takeaway: true},
  {id: 'spam', ru: 'В спам', en: 'Move to spam', takeaway: true},
  {id: 'trash', ru: 'В корзину', en: 'Move to trash', takeaway: true},
  {id: 'delete', ru: 'Удалить навсегда', en: 'Delete permanently', takeaway: true, confirm: true},
  {id: 'label_add', ru: 'Поставить метку', en: 'Add label', needs: 'label'},
  {id: 'label_remove', ru: 'Снять метку', en: 'Remove label', needs: 'label'},
  {id: 'mark_read', ru: 'Пометить прочитанным', en: 'Mark as read'},
  {id: 'mark_flagged', ru: 'Пометить важным', en: 'Mark as important'},
  {id: 'stop', ru: 'Остановить обработку', en: 'Stop processing'},
];

const ruleLocale = lang => (lang === 'en' ? 'en' : 'ru');
const ruleText = (item, lang) => item[ruleLocale(lang)];
const ruleOptionText = (item, lang) => item[ruleLocale(lang) === 'en' ? 2 : 1];

function ruleField(id) {
  return RULE_FIELDS.find(field => field.id === id) || RULE_FIELDS[0];
}

function ruleAction(id) {
  return RULE_ACTIONS.find(action => action.id === id) || RULE_ACTIONS[0];
}

function ruleOperators(type) {
  if (type === 'text') return ['contains', 'not_contains', 'equals', 'not_equals', 'starts_with', 'ends_with'];
  if (type === 'date') return ['within_last', 'older_than', 'before', 'after', 'on'];
  if (type === 'size') return ['greater_than', 'greater_or_equal', 'less_than', 'less_or_equal', 'equals', 'between'];
  return ['equals', 'not_equals'];
}

function isTakeawayAction(kind) {
  return Boolean(ruleAction(kind).takeaway) && RULE_ACTIONS.some(action => action.id === kind);
}

// Условие приводится к виду, который принимает ядро: неизвестное поле
// становится первым полем словаря, неподходящий оператор - первым допустимым.
function normalizeRuleCondition(source = {}) {
  const field = ruleField(source.field);
  const operators = ruleOperators(field.type);
  const op = operators.includes(source.op) ? source.op : operators[0];
  const condition = {field: field.id, op, value: String(source.value ?? '')};
  if (field.type === 'enum') {
    const known = field.values.some(item => item[0] === condition.value);
    condition.value = known ? condition.value : field.values[0][0];
  }
  if (field.type === 'date' && ['within_last', 'older_than'].includes(op)) {
    condition.unit = RULE_DATE_UNITS.some(item => item[0] === source.unit) ? source.unit : 'hours';
  }
  if (field.type === 'size') {
    condition.unit = RULE_SIZE_UNITS.some(item => item[0] === source.unit) ? source.unit : 'mb';
    condition.value2 = String(source.value2 ?? '');
  }
  return condition;
}

function normalizeRuleGroup(source = {}) {
  const conditions = Array.isArray(source.conditions) ? source.conditions : [];
  return {
    logic: source.logic === 'any' ? 'any' : 'all',
    conditions: conditions.map(normalizeRuleCondition),
  };
}

function normalizeRuleAction(source = {}) {
  const action = ruleAction(source.kind);
  return {
    kind: action.id,
    folder_id: action.needs === 'folder' ? (source.folder_id ?? null) : null,
    folder_role: action.needs === 'folder' ? (source.folder_role ?? null) : null,
    label_id: action.needs === 'label' ? (source.label_id ?? null) : null,
  };
}

function normalizeRule(source = {}) {
  return {
    id: source.id || '',
    name: source.name || '',
    account_id: source.account_id ?? null,
    enabled: source.enabled !== false,
    groups: (Array.isArray(source.groups) ? source.groups : []).map(normalizeRuleGroup),
    exceptions: (Array.isArray(source.exceptions) ? source.exceptions : []).map(normalizeRuleGroup),
    actions: (Array.isArray(source.actions) ? source.actions : []).map(normalizeRuleAction),
  };
}

// Пустое значение текстового условия совпало бы со всем подряд (S-025).
function validRuleCondition(source) {
  const condition = normalizeRuleCondition(source);
  const field = ruleField(condition.field);
  if (field.type === 'enum') return field.values.some(item => item[0] === condition.value);
  if (field.type === 'date' && ['within_last', 'older_than'].includes(condition.op)) {
    return Number(condition.value) > 0 && RULE_DATE_UNITS.some(item => item[0] === condition.unit);
  }
  if (field.type === 'date') return /^\d{4}-\d{2}-\d{2}$/.test(condition.value);
  if (field.type === 'size') {
    const amount = ruleSizeAmount(condition.value);
    if (amount === null || !RULE_SIZE_UNITS.some(item => item[0] === condition.unit)) return false;
    if (condition.op !== 'between') return true;
    const maximum = ruleSizeAmount(condition.value2);
    return maximum !== null && maximum > amount;
  }
  return Boolean(condition.value.trim());
}

// Размер условия: конечное неотрицательное число. Пустое поле прежде
// приводилось к нулю, и условие "размер больше" совпадало с каждым письмом.
function ruleSizeAmount(value) {
  const text = String(value ?? '').trim();
  if (!text) return null;
  const amount = Number(text);
  return Number.isFinite(amount) && amount >= 0 ? amount : null;
}

// Проверка состава правила повторяет проверки ядра (S-018 - S-019, S-028 -
// S-030, S-035, S-039, S-040): интерфейс объясняет отказ до обращения к ядру.
function validateRule(source) {
  const rule = normalizeRule(source);
  if (!rule.name.trim()) return {ok: false, reason: 'name'};
  if (rule.groups.length > RULE_MAX_GROUPS || rule.exceptions.length > RULE_MAX_GROUPS) return {ok: false, reason: 'groups_limit'};
  if (!rule.groups.length || rule.groups.every(group => !group.conditions.length)) return {ok: false, reason: 'no_conditions'};
  for (const group of rule.groups.concat(rule.exceptions)) {
    if (!group.conditions.length) return {ok: false, reason: 'empty_group'};
    if (group.conditions.length > RULE_MAX_CONDITIONS) return {ok: false, reason: 'conditions_limit'};
    if (group.conditions.some(condition => !validRuleCondition(condition))) return {ok: false, reason: 'condition_value'};
  }
  if (!rule.actions.length) return {ok: false, reason: 'no_actions'};
  if (rule.actions.length > RULE_MAX_ACTIONS) return {ok: false, reason: 'actions_limit'};
  for (const action of rule.actions) {
    const meta = ruleAction(action.kind);
    if (meta.needs === 'folder' && !action.folder_id && !action.folder_role) return {ok: false, reason: 'action_folder'};
    if (meta.needs === 'label' && !action.label_id) return {ok: false, reason: 'action_label'};
  }
  if (rule.actions.filter(action => isTakeawayAction(action.kind)).length > 1) return {ok: false, reason: 'two_takeaways'};
  const takeaway = rule.actions.findIndex(action => isTakeawayAction(action.kind));
  if (takeaway >= 0 && rule.actions.slice(takeaway + 1).some(action => action.kind !== 'stop')) return {ok: false, reason: 'after_takeaway'};
  const stop = rule.actions.findIndex(action => action.kind === 'stop');
  if (stop >= 0 && stop !== rule.actions.length - 1) return {ok: false, reason: 'stop_last'};
  return {ok: true};
}

// Тексты отказов на обоих языках: смена языка перерисовывает подписи без
// перезапуска программы (S-083).
function ruleErrorText(reason, lang) {
  const texts = {
    name: ['Введите название правила', 'Enter a rule name'],
    groups_limit: [`Групп в правиле не больше ${RULE_MAX_GROUPS}`, `A rule holds at most ${RULE_MAX_GROUPS} groups`],
    conditions_limit: [`Условий в группе не больше ${RULE_MAX_CONDITIONS}`, `A group holds at most ${RULE_MAX_CONDITIONS} conditions`],
    actions_limit: [`Действий в правиле не больше ${RULE_MAX_ACTIONS}`, `A rule holds at most ${RULE_MAX_ACTIONS} actions`],
    no_conditions: ['Добавьте хотя бы одно условие', 'Add at least one condition'],
    empty_group: ['Группа без условий не сохраняется', 'A group without conditions cannot be saved'],
    condition_value: ['Заполните значения всех условий', 'Fill in every condition value'],
    no_actions: ['Добавьте хотя бы одно действие', 'Add at least one action'],
    action_folder: ['Выберите папку назначения', 'Choose a destination folder'],
    action_label: ['Выберите метку', 'Choose a label'],
    two_takeaways: ['Письмо можно увести только один раз', 'A message can be taken away only once'],
    after_takeaway: ['После ухода письма остальные действия не выполнятся', 'Actions after the message leaves will not run'],
    stop_last: ['Остановка обработки ставится последним действием', 'Stop processing must be the last action'],
  };
  const text = texts[reason] || texts.condition_value;
  return ruleLocale(lang) === 'en' ? text[1] : text[0];
}

// Новый порядок списка после перетаскивания: элемент from встаёт на место to
// (S-059). Возвращает новый массив, исходный не меняется.
function moveRule(ids, from, to) {
  const list = Array.isArray(ids) ? ids.slice() : [];
  if (from < 0 || to < 0 || from >= list.length || to >= list.length || from === to) return list;
  const [moved] = list.splice(from, 1);
  list.splice(to, 0, moved);
  return list;
}

// Условие в описании правила: подписи поля, оператора и значения. Значения
// перечислений и единицы измерения показываются так же, как в редакторе, а не
// сырыми идентификаторами.
function ruleConditionText(source, lang) {
  const condition = normalizeRuleCondition(source);
  const field = ruleField(condition.field);
  const locale = ruleLocale(lang);
  const fieldName = ruleText(field, locale);
  const opName = (RULE_OPS[condition.op] || [condition.op, condition.op])[locale === 'en' ? 1 : 0];
  const unitName = units => {
    const found = units.find(item => item[0] === condition.unit);
    return found ? ruleOptionText(found, locale) : condition.unit || '';
  };
  if (field.type === 'enum') {
    const value = field.values.find(item => item[0] === condition.value);
    return `${fieldName} ${opName} ${value ? ruleOptionText(value, locale) : condition.value}`;
  }
  if (field.type === 'size') {
    const unit = unitName(RULE_SIZE_UNITS);
    const value = condition.op === 'between'
      ? `${condition.value} - ${condition.value2 || ''}`
      : condition.value;
    return `${fieldName} ${opName} ${value} ${unit}`.trim();
  }
  if (field.type === 'date' && ['within_last', 'older_than'].includes(condition.op)) {
    return `${fieldName} ${opName} ${condition.value} ${unitName(RULE_DATE_UNITS)}`.trim();
  }
  return `${fieldName} ${opName} "${condition.value}"`;
}

// Короткое описание правила для списка: условия и цепочка действий.
function ruleSummary(source, context = {}) {
  const rule = normalizeRule(source);
  const lang = ruleLocale(context.lang);
  const groupText = group => group.conditions
    .map(condition => ruleConditionText(condition, lang))
    .join(group.logic === 'any' ? (lang === 'en' ? ' or ' : ' или ') : (lang === 'en' ? ' and ' : ' и '));
  const conditions = rule.groups.map(groupText).join(lang === 'en' ? '; or ' : '; или ');
  const exceptions = rule.exceptions.length
    ? `${lang === 'en' ? ' except: ' : ' кроме: '}${rule.exceptions.map(groupText).join(lang === 'en' ? '; or ' : '; или ')}`
    : '';
  const actionText = action => {
    const meta = ruleAction(action.kind);
    if (action.kind === 'move') {
      const folder = (context.folders || []).find(item => item.id === action.folder_id);
      const name = folder ? (folder.display_name || folder.remote_path) : (action.folder_role || '?');
      return `${ruleText(meta, lang)}: ${name}`;
    }
    if (meta.needs === 'label') {
      const label = (context.labels || []).find(item => item.id === action.label_id);
      return `${ruleText(meta, lang)}: ${label ? label.name : '?'}`;
    }
    return ruleText(meta, lang);
  };
  const actions = rule.actions.map(actionText).join(lang === 'en' ? ', then ' : ', затем ');
  const account = rule.account_id
    ? ((context.accounts || []).find(item => item.id === rule.account_id) || {}).email || (lang === 'en' ? 'mailbox removed' : 'ящик удалён')
    : (lang === 'en' ? 'all mailboxes' : 'все ящики');
  return `${conditions}${exceptions} -> ${actions} - ${account}`;
}

// Подпись состояния правила в списке (S-054).
function ruleStateText(rule, lang) {
  if (!rule || rule.state !== 'needs_attention') return '';
  const reasons = {
    label_missing: ['метка удалена', 'label removed'],
    folder_missing: ['папка удалена', 'folder removed'],
    folder_role_ambiguous: ['несколько папок одного типа', 'several folders of one type'],
    rule_incomplete: ['правило не заполнено', 'rule is incomplete'],
  };
  const reason = reasons[rule.attention_reason] || reasons.rule_incomplete;
  const text = ruleLocale(lang) === 'en' ? reason[1] : reason[0];
  return ruleLocale(lang) === 'en' ? `Needs attention: ${text}` : `Требует внимания: ${text}`;
}

// Отчёт ручного прогона (S-073).
function runReportText(report, lang) {
  if (!report) return '';
  const en = ruleLocale(lang) === 'en';
  const parts = en
    ? [`scanned ${report.scanned}`, `rules applied ${report.applied}`, `operations queued ${report.queued}`, `skipped ${report.skipped}`, `left ${report.remaining}`]
    : [`просмотрено ${report.scanned}`, `применено правил ${report.applied}`, `поставлено операций ${report.queued}`, `пропущено ${report.skipped}`, `осталось ${report.remaining}`];
  return parts.join(', ');
}

const mailRulesModel = {
  RULE_FIELDS,
  RULE_OPS,
  RULE_ACTIONS,
  RULE_DATE_UNITS,
  RULE_SIZE_UNITS,
  RULE_MAX_GROUPS,
  RULE_MAX_CONDITIONS,
  RULE_MAX_ACTIONS,
  ruleField,
  ruleAction,
  ruleOperators,
  ruleText,
  ruleOptionText,
  isTakeawayAction,
  normalizeRuleCondition,
  normalizeRuleGroup,
  normalizeRuleAction,
  normalizeRule,
  validRuleCondition,
  validateRule,
  ruleErrorText,
  moveRule,
  ruleSummary,
  ruleConditionText,
  ruleStateText,
  runReportText,
};
if (typeof module !== 'undefined' && module.exports) module.exports = mailRulesModel;
