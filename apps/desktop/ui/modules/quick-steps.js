'use strict';
(function(root, factory) {
  const rules = typeof module === 'object' && module.exports ? require('./mail-rules.js') : root.mailRulesModel;
  const api = factory(rules);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.quickStepsModel = api;
})(typeof globalThis !== 'undefined' ? globalThis : this, function(rules) {
  const MAX_STEPS = 20;
  const MAX_ACTIONS = 10;
  const MAX_MESSAGES = 500;
  const QUICK_STEP_SLOT_ACTIONS = Array.from({length: 10}, (_, index) => `quick_step_${index + 1}`);
  // Закрытый перечень значков. Свободная строка значка подставляется в
  // разметку окна, у которого есть весь мост команд, поэтому выбирать можно
  // только из известных имён набора значков; тем же перечнем пользуется выбор
  // значка умной папки.
  const ICONS = [
    'star', 'flag', 'inbox', 'archive', 'trash', 'spam', 'draft', 'send', 'sync',
    'cal', 'people', 'sun', 'compose', 'filter', 'sort', 'search', 'reply',
    'replyall', 'forward', 'snooze', 'paperclip', 'shield', 'settings', 'palette',
    'user', 'globe', 'keyboard', 'copy', 'lock', 'key', 'server', 'edit', 'list',
    'link', 'at', 'upload', 'download', 'image', 'storage', 'unsub', 'print',
    'translate', 'pin', 'openext',
  ];
  // Роли папки назначения: тот же закрытый перечень, что у действий правил.
  const FOLDER_ROLES = ['inbox', 'archive', 'spam', 'trash'];

  function normalizeIcon(value) {
    const icon = String(value || '');
    return ICONS.includes(icon) ? icon : ICONS[0];
  }

  const availableActions = () => rules.RULE_ACTIONS
    .filter(action => action.id !== 'stop')
    .map(action => ({...action, folder: action.needs === 'folder', label: action.needs === 'label'}));

  function normalizeQuickStep(source = {}) {
    return {
      id: source.id ?? null,
      name: String(source.name || ''),
      icon: normalizeIcon(source.icon),
      sort_order: Number(source.sort_order ?? source.position) || 0,
      hotkey_slot: source.hotkey_slot ?? null,
      state: source.state || 'ok',
      actions: (source.actions || []).map(action => Object.assign({}, action)),
    };
  }

  // Цель действия проверяется по настоящим папкам и меткам программы: номер,
  // которого нет, унёс бы письма в чужую папку одним нажатием. Контекст не
  // задан - проверяется только заполненность цели.
  function targetIssue(action, definition, context) {
    if (definition.needs === 'folder') {
      if (!action.folder_id && !action.folder_role) return 'action_folder';
      if (action.folder_role && !FOLDER_ROLES.includes(String(action.folder_role))) {
        return 'unknown_role';
      }
      if (action.folder_id && context?.folders
        && !context.folders.some(folder => Number(folder.id) === Number(action.folder_id))) {
        return 'unknown_folder';
      }
    }
    if (definition.needs === 'label') {
      if (!action.label_id) return 'action_label';
      if (context?.labels
        && !context.labels.some(label => Number(label.id) === Number(action.label_id))) {
        return 'unknown_label';
      }
    }
    return null;
  }

  function validateQuickStep(source, context = null) {
    const step = normalizeQuickStep(source);
    if (!step.name.trim() || [...step.name.trim()].length > 40) return {ok: false, reason: 'name'};
    if (!step.actions.length) return {ok: false, reason: 'empty'};
    if (step.actions.length > MAX_ACTIONS) return {ok: false, reason: 'actions_limit'};
    let takeaway = -1;
    for (let index = 0; index < step.actions.length; index++) {
      const action = step.actions[index];
      const definition = availableActions().find(item => item.id === action.kind);
      if (!definition) return {ok: false, reason: 'unknown_action'};
      const issue = targetIssue(action, definition, context);
      if (issue) return {ok: false, reason: issue, index};
      if (definition.takeaway) {
        if (takeaway >= 0) return {ok: false, reason: 'two_takeaways'};
        takeaway = index;
      }
    }
    if (takeaway >= 0 && takeaway !== step.actions.length - 1) return {ok: false, reason: 'after_takeaway'};
    return {ok: true};
  }

  function normalizeCombo(combo) {
    const parts = String(combo || '').split('+').map(part => part.trim()).filter(Boolean);
    const modifiers = new Set();
    const keys = [];
    parts.forEach(part => {
      const lower = part.toLowerCase();
      if (['ctrl', 'control'].includes(lower)) modifiers.add('Ctrl');
      else if (lower === 'alt') modifiers.add('Alt');
      else if (lower === 'shift') modifiers.add('Shift');
      else if (['meta', 'win', 'cmd'].includes(lower)) modifiers.add('Meta');
      else keys.push(part.length === 1 ? part.toUpperCase() : part);
    });
    if (keys.length !== 1) return '';
    return ['Ctrl', 'Alt', 'Shift', 'Meta'].filter(modifier => modifiers.has(modifier)).concat(keys).join('+');
  }

  function eventCombo(event) {
    const parts = [];
    if (event.ctrlKey) parts.push('Ctrl');
    if (event.altKey) parts.push('Alt');
    if (event.shiftKey) parts.push('Shift');
    if (event.metaKey) parts.push('Meta');
    let key = /^Digit[0-9]$/.test(event.code) ? event.code.slice(5) : event.key;
    if (['Control', 'Alt', 'Shift', 'Meta'].includes(key)) return '';
    if (key === 'Delete') key = 'Del';
    else if (key === ' ') key = 'Space';
    else if (key.length === 1) key = key.toUpperCase();
    parts.push(key);
    return normalizeCombo(parts.join('+'));
  }

  function proposedSlotCombo(slot) {
    const value = Number(slot);
    return value >= 1 && value <= 10 ? `Ctrl+Shift+${value === 10 ? 0 : value}` : '';
  }

  function toolbarKey(step) {
    return `quick_step:${step.id}`;
  }

  function toolbarSet(builtin, steps, layout) {
    const quick = (steps || [])
      .map(normalizeQuickStep)
      .sort((a, b) => a.sort_order - b.sort_order || Number(a.id) - Number(b.id))
      .map(step => ({
        id: toolbarKey(step),
        key: toolbarKey(step),
        quickStepId: step.id,
        name: step.name,
        icon: step.icon,
        visible: false,
        labels: 'text',
      }));
    const base = (builtin || []).map(item => ({
      ...item,
      id: item.id || item.key || item.k,
      key: item.key || item.id || item.k,
      visible: item.visible ?? item.on ?? false,
      labels: item.labels || 'text',
    }));
    const all = base.concat(quick);
    if (!Array.isArray(layout)) return all;
    const byId = new Map(all.map(item => [item.id, item]));
    const restored = [];
    layout.forEach(entry => {
      const id = entry.id || entry.key;
      const item = byId.get(id);
      if (!item) return;
      restored.push({...item, visible: entry.visible ?? item.visible, labels: entry.labels || item.labels});
      byId.delete(id);
    });
    return restored.concat([...byId.values()]);
  }

  function toolbarMoreMenu(items) {
    return (items || []).filter(item => !item.visible);
  }

  function quickStepTargets(context = {}) {
    const selection = [...new Set((context.selection || []).map(Number).filter(Number.isFinite))];
    if (selection.length) return {ids: selection};
    const source = context.collapsedThread || context.activeMessage;
    if (!source) return {ids: []};
    if (!context.collapsedThread || source.thread_id == null) return {ids: [Number(source.id)]};
    const ids = (context.loaded || [])
      .filter(message => message.folder_id === source.folder_id && message.thread_id === source.thread_id)
      .map(message => Number(message.id));
    return {ids: [...new Set(ids.length ? ids : [Number(source.id)])]};
  }

  function quickStepLimitError(count, lang = 'ru') {
    if (Number(count) <= MAX_MESSAGES) return null;
    return lang === 'en'
      ? `Select no more than ${MAX_MESSAGES} messages`
      : `Выберите не больше ${MAX_MESSAGES} писем`;
  }

  function attentionText(lang) {
    return lang === 'en'
      ? 'This quick step needs a folder or label target'
      : 'Быстрое действие требует выбрать папку или метку';
  }

  function noMessageText(lang) {
    return lang === 'en' ? 'Select a message first' : 'Сначала выберите письмо';
  }

  async function runQuickStep(step, context = {}) {
    if (!step || context.inTextField) return null;
    if (step.state === 'needs_attention') {
      context.notify?.(attentionText(context.lang));
      return null;
    }
    const targets = quickStepTargets(context);
    if (!targets.ids.length) {
      context.notify?.(noMessageText(context.lang));
      return null;
    }
    const limit = quickStepLimitError(targets.ids.length, context.lang);
    if (limit) {
      context.notify?.(limit);
      return null;
    }
    if ((step.actions || []).some(action => action.kind === 'delete')) {
      const text = context.lang === 'en'
        ? `Permanently delete messages: ${targets.ids.length}?`
        : `Удалить навсегда писем: ${targets.ids.length}?`;
      if (context.confirm && !await context.confirm(text)) return null;
    }
    const command = context.bridge?.runQuickStep || context.bridge?.applyQuickStep;
    if (!command) throw new Error('Команда быстрого действия недоступна');
    const report = await command.call(context.bridge, step.id, targets.ids);
    await context.onApplied?.(report, targets.ids, step);
    return report;
  }

  async function handleSlotPress(action, context = {}) {
    if (context.inTextField || !QUICK_STEP_SLOT_ACTIONS.includes(action)) return null;
    const slot = Number(String(action).slice('quick_step_'.length));
    const step = (context.steps || []).find(item => Number(item.hotkey_slot) === slot);
    if (!step) return null;
    return runQuickStep(step, context);
  }

  function reportText(report, lang = 'ru') {
    const reasons = [];
    if (report.skipped_busy) reasons.push(lang === 'en' ? `busy: ${report.skipped_busy}` : `занято: ${report.skipped_busy}`);
    if (report.skipped_failed) reasons.push(lang === 'en' ? `awaiting decision: ${report.skipped_failed}` : `ждет решения: ${report.skipped_failed}`);
    if (report.skipped_no_folder) reasons.push(lang === 'en' ? `folder not found: ${report.skipped_no_folder}` : `папка не найдена: ${report.skipped_no_folder}`);
    if (report.skipped_foreign_account) reasons.push(lang === 'en' ? `another mailbox: ${report.skipped_foreign_account}` : `другой ящик: ${report.skipped_foreign_account}`);
    if (report.traits_at_risk) reasons.push(lang === 'en' ? `task or pin may be lost: ${report.traits_at_risk}` : `дело или закрепление может потеряться: ${report.traits_at_risk}`);
    return lang === 'en'
      ? `Applied: ${report.applied || 0}, skipped: ${report.skipped || 0}${reasons.length ? ` (${reasons.join(', ')})` : ''}`
      : `Выполнено: ${report.applied || 0}, пропущено: ${report.skipped || 0}${reasons.length ? ` (${reasons.join(', ')})` : ''}`;
  }

  function quickStepErrorText(reason, lang = 'ru') {
    const text = {
      name: ['Имя быстрого действия - от одного до сорока знаков', 'Quick step name must be 1 to 40 characters'],
      empty: ['В цепочке нет ни одного действия', 'The chain has no actions'],
      actions_limit: ['В цепочке больше десяти действий', 'The chain has more than ten actions'],
      unknown_action: ['Такого действия нет в словаре', 'This action is not in the dictionary'],
      action_folder: ['У перемещения не выбрана папка', 'The move action has no folder'],
      action_label: ['У действия с меткой не выбрана метка', 'The label action has no label'],
      unknown_folder: ['Выбранной папки больше нет', 'The selected folder no longer exists'],
      unknown_label: ['Выбранной метки больше нет', 'The selected label no longer exists'],
      unknown_role: ['Такого типа папки нет', 'There is no such folder type'],
      two_takeaways: ['Письмо можно увести только один раз', 'A message can be taken away only once'],
      after_takeaway: ['После ухода письма остальные действия не выполнятся', 'Actions after the takeaway will not run'],
    };
    return (text[reason] || [reason, reason])[lang === 'en' ? 1 : 0];
  }

  return {
    MAX_STEPS,
    MAX_ACTIONS,
    MAX_MESSAGES,
    QUICK_STEP_SLOT_ACTIONS,
    ICONS,
    FOLDER_ROLES,
    normalizeIcon,
    quickStepErrorText,
    availableActions,
    normalizeQuickStep,
    validateQuickStep,
    normalizeCombo,
    eventCombo,
    proposedSlotCombo,
    toolbarKey,
    toolbarSet,
    toolbarMoreMenu,
    quickStepTargets,
    quickStepLimitError,
    runQuickStep,
    handleSlotPress,
    reportText,
  };
});
