// truemail UI module: limits.js
// Реестр настраиваемых пределов интерфейса. Значения, границы и подписи
// приходят из ядра командой limit_settings; своих чисел здесь нет намеренно.
// Прежде каждый такой предел был записан дважды - в ядре и в интерфейсе, - и
// копии разошлись: ядро разрешало закрепить двадцать писем, а список показывал
// место под пятьдесят, и отказ никто не объяснял.
// См. crates/core/src/model/limits.rs.
'use strict';
(function(root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.limitsModel = api;
})(typeof globalThis !== 'undefined' ? globalThis : this, function() {
  // Имена ключей настроек. Строка, набранная руками во втором месте, молча
  // разошлась бы с ядром, поэтому обращаться к пределу нужно только отсюда.
  const KEYS = {
    pinnedPerAccount: 'limit_pinned_per_account',
    pinnedVisible: 'limit_pinned_visible',
    messageFirstPage: 'limit_message_first_page',
    messagePage: 'limit_message_page',
    smartMessagePage: 'limit_smart_message_page',
    messageMemory: 'limit_message_memory',
    syncFailuresBeforeToast: 'limit_sync_failures_before_toast',
    backgroundSyncMinutes: 'limit_background_sync_minutes',
    ruleGroups: 'limit_rule_groups',
    groupConditions: 'limit_group_conditions',
    ruleActions: 'limit_rule_actions',
    quickSteps: 'limit_quick_steps',
    quickStepMessages: 'limit_quick_step_messages',
    manualRunMessages: 'limit_manual_run_messages',
    manualRunBatch: 'limit_manual_run_batch',
    undoSendDefault: 'limit_undo_send_default',
    undoSendMin: 'limit_undo_send_min',
    undoSendMax: 'limit_undo_send_max',
    recipientEntries: 'limit_recipient_entries',
    recipientTouches: 'limit_recipient_touches',
    recipientSuggestions: 'limit_recipient_suggestions',
    oofSilenceDays: 'limit_oof_silence_days',
    oofMemoryDays: 'limit_oof_memory_days',
    oofMessageAgeHours: 'limit_oof_message_age_hours',
    oofTextChars: 'limit_oof_text_chars',
    oofPeriodDays: 'limit_oof_period_days',
    oofInternalDomains: 'limit_oof_internal_domains',
    doneTaskDays: 'limit_done_task_days',
    ignoredConversations: 'limit_ignored_conversations',
    conversationIds: 'limit_conversation_ids',
    ignoreReturnWaitDays: 'limit_ignore_return_wait_days',
    sweepMinDays: 'limit_sweep_min_days',
    sweepMaxDays: 'limit_sweep_max_days',
    purgeBatch: 'limit_purge_batch',
    requestKeyDays: 'limit_request_key_days',
    recipientOwnSendDays: 'limit_recipient_own_send_days',
    messageTraitsDays: 'limit_message_traits_days',
    operationAttempts: 'limit_operation_attempts',
  };

  let sections = [];
  const specs = new Map();

  // Принять перечень из ядра. Вызывается при старте до первой отрисовки списка
  // и после каждой записи значения.
  function applyLimits(payload) {
    sections = Array.isArray(payload?.sections) ? [...payload.sections] : [];
    specs.clear();
    (payload?.limits || []).forEach(item => {
      if (item && typeof item.key === 'string') specs.set(item.key, {...item});
    });
    return specs.size;
  }

  function limitSpec(key) {
    return specs.get(key) || null;
  }

  // Рабочее значение предела. Пока перечень не загружен, значения нет: числа
  // "на всякий случай" здесь и были той самой второй копией.
  function limitValue(key) {
    const spec = specs.get(key);
    return spec && Number.isFinite(Number(spec.value)) ? Number(spec.value) : null;
  }

  // Значение с запасным числом на случай незагруженного перечня. Нужно там,
  // где без числа нечего показать вовсе - например, сколько строк списка
  // держать в памяти: запас берётся из того же описания ядра.
  function limitValueOr(key, fallback) {
    const value = limitValue(key);
    return value === null ? fallback : value;
  }

  function limitsLoaded() {
    return specs.size > 0;
  }

  // Проверить значение по границам ядра. Незагруженный перечень означает, что
  // решение принимает ядро: интерфейс объясняет отказ заранее, но не выдумывает
  // границы сам.
  function withinLimit(key, value) {
    const spec = specs.get(key);
    if (!spec) return true;
    const number = Number(value);
    if (!Number.isInteger(number)) return false;
    return number >= Number(spec.min) && number <= Number(spec.max);
  }

  // Поля раздела настроек, разложенные по разделам. Подписи не строки, а
  // ключи каталога локализации: перевод берётся тем же способом, что и у
  // остальной разметки.
  function limitSectionRows() {
    return sections.map(section => ({
      id: section.id,
      titleKey: section.title_key,
      hintKey: section.hint_key,
      fields: [...specs.values()].filter(spec => spec.section === section.id),
    })).filter(section => section.fields.length);
  }

  // Текст отказа со всеми числами из описания ядра.
  function limitErrorText(key, lang = 'ru', translate = value => value) {
    const spec = specs.get(key);
    if (!spec) return '';
    const title = translate(spec.title_key);
    const unit = translate(spec.unit_key);
    return lang === 'en'
      ? `"${title}": allowed values are from ${spec.min} to ${spec.max} (${unit})`
      : `"${title}": допустимы значения от ${spec.min} до ${spec.max} (${unit})`;
  }

  // Записать значение через мост и обновить реестр ответом ядра: принятое
  // значение всегда приходит от ядра, а не додумывается интерфейсом.
  async function saveLimit(bridge, key, value) {
    const saved = await bridge.setLimitSetting(key, Number(value));
    const spec = specs.get(key);
    if (spec) spec.value = Number(saved);
    return Number(saved);
  }

  return {
    KEYS,
    applyLimits,
    limitSpec,
    limitValue,
    limitValueOr,
    limitsLoaded,
    withinLimit,
    limitSectionRows,
    limitErrorText,
    saveLimit,
  };
});
