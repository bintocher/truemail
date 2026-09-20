'use strict';
(function(root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.flagDueDatesModel = api;
})(typeof globalThis !== 'undefined' ? globalThis : this, function() {
  const GROUPS = ['overdue', 'today', 'tomorrow', 'week', 'later', 'none', 'done'];

  function asDate(value) {
    if (value == null || value === '') return null;
    if (value instanceof Date) return Number.isNaN(value.getTime()) ? null : new Date(value);
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? null : date;
  }

  function dayStart(value) {
    const date = asDate(value);
    if (!date) return null;
    date.setHours(0, 0, 0, 0);
    return date;
  }

  function nextMonday(value) {
    const date = dayStart(value);
    const days = (8 - date.getDay()) % 7 || 7;
    date.setDate(date.getDate() + days);
    date.setHours(9, 0, 0, 0);
    return date;
  }

  // Сдвиг на месяц вперёд с удержанием в пределах месяца: 31 марта + месяц
  // даёт 30 апреля, а не 1 мая. Без удержания срок уезжал бы на день дальше
  // обещанного у всех длинных месяцев.
  function monthLater(value) {
    const date = dayStart(value);
    const day = date.getDate();
    date.setDate(1);
    date.setMonth(date.getMonth() + 1);
    const lastDay = new Date(date.getFullYear(), date.getMonth() + 1, 0).getDate();
    date.setDate(Math.min(day, lastDay));
    return date;
  }

  function duePresets(now = new Date(), lang = 'ru') {
    const base = asDate(now) || new Date();
    // Час отсчитывается от текущего момента, остальные значения - от начала
    // суток: "завтра в 09:00" не должно зависеть от времени нажатия.
    const hour = new Date(base.getTime() + 60 * 60 * 1000);
    const today = dayStart(base);
    const evening = dayStart(base);
    const tomorrow = dayStart(base);
    const monday = nextMonday(base);
    const nextWeek = dayStart(base);
    const nextMonth = monthLater(base);
    today.setHours(23, 59, 59, 999);
    evening.setHours(18, 0, 0, 0);
    tomorrow.setDate(tomorrow.getDate() + 1);
    tomorrow.setHours(9, 0, 0, 0);
    nextWeek.setDate(nextWeek.getDate() + 7);
    nextWeek.setHours(9, 0, 0, 0);
    nextMonth.setHours(9, 0, 0, 0);
    const en = lang === 'en';
    return [
      {id: 'hour', title: en ? 'In an hour' : 'Через час', value: hour},
      {id: 'today', title: en ? 'Today' : 'Сегодня', value: today},
      {id: 'evening', title: en ? 'This evening' : 'Сегодня вечером', value: evening},
      {id: 'tomorrow', title: en ? 'Tomorrow at 09:00' : 'Завтра в 09:00', value: tomorrow},
      {id: 'monday', title: en ? 'Monday at 09:00' : 'В понедельник в 09:00', value: monday},
      {id: 'next_week', title: en ? 'In a week' : 'Через неделю', value: nextWeek},
      {id: 'next_month', title: en ? 'In a month' : 'Через месяц', value: nextMonth},
      {id: 'none', title: en ? 'No due date' : 'Без срока', value: null},
      {id: 'custom', title: en ? 'Custom time' : 'Другие дата и время', value: null},
    ];
  }

  function dueQuickChoices(lang = 'ru', now = new Date()) {
    return duePresets(now, lang);
  }

  function validateTask(task, now = new Date()) {
    const start = task.start_at ? asDate(task.start_at) : null;
    const due = task.due_at ? asDate(task.due_at) : null;
    const reminder = task.reminder_at ? asDate(task.reminder_at) : null;
    if ((task.start_at && !start) || (task.due_at && !due) || (task.reminder_at && !reminder)) {
      return {ok: false, reason: 'invalid_time'};
    }
    if (start && due && start > due) return {ok: false, reason: 'start_after_due'};
    if (reminder && reminder < asDate(now)) return {ok: false, reason: 'reminder_past'};
    return {ok: true};
  }

  function taskValue(item) {
    return item?.task || item || {};
  }

  function taskGroup(item, now = new Date()) {
    const task = taskValue(item);
    if (task.state === 'done') return 'done';
    const due = asDate(task.due_at);
    if (!due) return 'none';
    if (task.state === 'active' && due < asDate(now)) return 'overdue';
    const tomorrow = dayStart(now);
    tomorrow.setDate(tomorrow.getDate() + 1);
    const afterTomorrow = dayStart(tomorrow);
    afterTomorrow.setDate(afterTomorrow.getDate() + 1);
    const week = dayStart(now);
    week.setDate(week.getDate() + 7);
    if (due < tomorrow) return 'today';
    if (due < afterTomorrow) return 'tomorrow';
    if (due <= week) return 'week';
    return 'later';
  }

  function taskField(item, key) {
    const task = taskValue(item);
    return task[key] ?? item?.[key] ?? null;
  }

  function sortTasks(items, now = new Date()) {
    return [...(items || [])].sort((left, right) => {
      const group = GROUPS.indexOf(taskGroup(left, now)) - GROUPS.indexOf(taskGroup(right, now));
      if (group) return group;
      const leftDue = String(taskField(left, 'due_at') || '');
      const rightDue = String(taskField(right, 'due_at') || '');
      const due = leftDue.localeCompare(rightDue);
      if (due) return due;
      const leftDate = String(taskField(left, 'date') || taskField(left, 'message_date') || '');
      const rightDate = String(taskField(right, 'date') || taskField(right, 'message_date') || '');
      const date = rightDate.localeCompare(leftDate);
      if (date) return date;
      return Number(taskField(right, 'message_id')) - Number(taskField(left, 'message_id'));
    });
  }

  function groupLabel(group, lang = 'ru') {
    const labels = {
      overdue: ['Просрочено', 'Overdue'],
      today: ['Сегодня', 'Today'],
      tomorrow: ['Завтра', 'Tomorrow'],
      week: ['На неделе', 'This week'],
      later: ['Позже', 'Later'],
      none: ['Без срока', 'No due date'],
      done: ['Выполнено', 'Done'],
    };
    return (labels[group] || [group, group])[lang === 'en' ? 1 : 0];
  }

  function formatDue(value, lang = 'ru', now = new Date()) {
    const date = asDate(value);
    if (!date) return '';
    const locale = lang === 'en' ? 'en-US' : 'ru-RU';
    const group = taskGroup({due_at: date, state: 'active'}, now);
    if (group === 'today') return date.toLocaleTimeString(locale, {hour: '2-digit', minute: '2-digit'});
    if (group === 'tomorrow') {
      return `${groupLabel('tomorrow', lang)}, ${date.toLocaleTimeString(locale, {hour: '2-digit', minute: '2-digit'})}`;
    }
    return date.toLocaleString(locale, {day: '2-digit', month: 'short', hour: '2-digit', minute: '2-digit'});
  }

  function senderText(item, lang) {
    const name = taskField(item, 'from_name') || taskField(item, 'sender_name');
    const address = taskField(item, 'from_addr') || taskField(item, 'sender_address');
    return name || address || (lang === 'en' ? 'Unknown sender' : 'Отправитель неизвестен');
  }

  function taskRowText(item, lang = 'ru') {
    const subject = taskField(item, 'subject') || (lang === 'en' ? 'No subject' : 'Без темы');
    const sender = senderText(item, lang);
    const account = taskField(item, 'account_email') || '';
    const due = formatDue(taskField(item, 'due_at'), lang);
    const state = taskField(item, 'state') === 'done' ? groupLabel('done', lang) : '';
    const snoozed = taskField(item, 'snoozed_until');
    const snoozeText = snoozed
      ? `${lang === 'en' ? 'Return' : 'Возврат после откладывания'}: ${formatDue(snoozed, lang)}`
      : '';
    return [subject, sender, account, due, state, snoozeText].filter(Boolean).join(' - ');
  }

  function overdueTaskCount(items, now = new Date()) {
    return (items || []).filter(item => taskGroup(item, now) === 'overdue').length;
  }

  function visibleTasks(items) {
    // Признак уводимого письма ядро называет has_takeaway: по прежнему имени
    // отбор не отсекал ничего вовсе (S-052).
    return (items || []).filter(item => !(item?.has_takeaway ?? taskField(item, 'takeaway_pending')));
  }

  function reminderCard(item, lang = 'ru') {
    const subject = taskField(item, 'subject') || (lang === 'en' ? 'No subject' : 'Без темы');
    const sender = senderText(item, lang);
    const dueDate = asDate(taskField(item, 'due_at'));
    const due = dueDate
      ? dueDate.toLocaleString(lang === 'en' ? 'en-US' : 'ru-RU', {
        day: '2-digit',
        month: 'short',
        hour: '2-digit',
        minute: '2-digit',
      })
      : '';
    const actions = lang === 'en'
      ? [['open', 'Open'], ['snooze', 'Snooze for 10 minutes'], ['done', 'Done']]
      : [['open', 'Открыть'], ['snooze', 'Отложить на 10 минут'], ['done', 'Выполнено']];
    return {
      title: lang === 'en' ? 'Task reminder' : 'Напоминание о деле',
      subject,
      preview: sender,
      details: due,
      body: [subject, sender, due].filter(Boolean).join(' - '),
      actions: actions.map(([id, title]) => ({id, title})),
    };
  }

  // Сводка пропущенных напоминаний: из ядра приходит только число, а текст
  // собирается по языку интерфейса.
  function missedSummary(count, lang = 'ru') {
    const total = Number(count) || 0;
    return {
      title: lang === 'en' ? 'Missed reminders' : 'Пропущенные напоминания',
      subject: lang === 'en' ? `Missed reminders: ${total}` : `Пропущено напоминаний: ${total}`,
      action: lang === 'en' ? 'Open tasks' : 'Открыть дела',
    };
  }

  function missedReminders(items, now = new Date(), lang = 'ru') {
    const current = asDate(now)?.getTime() || Date.now();
    const week = 7 * 24 * 60 * 60 * 1000;
    const marked = (items || []).map(item => Number(taskField(item, 'message_id')));
    const shown = (items || []).filter(item => {
      const reminder = asDate(taskField(item, 'reminder_at'));
      return reminder && current - reminder.getTime() <= week;
    });
    const text = lang === 'en'
      ? `Missed reminders: ${shown.length}`
      : `Пропущено напоминаний: ${shown.length}`;
    return {shown, marked, text};
  }

  // Письма, у которых задан хотя бы один из трёх сроков дела. Снятие флажка
  // удаляет дело целиком, поэтому спрашивать надо про любой срок, а не только
  // про срок исполнения.
  function datedTasks(items) {
    return (items || []).filter(item => ['start_at', 'due_at', 'reminder_at']
      .some(key => {
        const value = taskField(item, key) ?? taskField(item, `task_${key}`);
        return value != null && value !== '';
      }));
  }

  function flagClearWarning(count, lang = 'ru') {
    if (count <= 1) {
      return lang === 'en'
        ? 'Clear the flag and delete the reminder?'
        : 'Снять флажок и удалить напоминание?';
    }
    return lang === 'en'
      ? `Clear the flag? Reminders will be deleted for ${count} message(s).`
      : `Снять флажок? Напоминания будут удалены у писем: ${count}.`;
  }

  async function toggleFlag(bridge, ids, flagged, options = {}) {
    const messageIds = [...new Set(ids || [])];
    // Сроки собираются по всем выбранным письмам: вопрос по одному письму
    // молча стирал бы дела всех остальных.
    const dated = flagged ? [] : datedTasks(options.tasks || (options.task ? [options.task] : []));
    if (dated.length) {
      const text = flagClearWarning(dated.length, options.lang);
      if (options.confirm && !await options.confirm(text)) return 0;
    }
    options.onOptimistic?.(messageIds, Boolean(flagged));
    return bridge.markFlagged(messageIds, Boolean(flagged), options.reason || 'user');
  }

  function saveTask(bridge, messageId, times) {
    if (bridge.saveMessageTask.length >= 2) return bridge.saveMessageTask(messageId, times);
    return bridge.saveMessageTask({message_id: messageId, ...times});
  }

  function completeTask(bridge, messageId, state = 'active') {
    if (state === 'done' && bridge.reopenMessageTask) return bridge.reopenMessageTask(messageId);
    if (state === 'detached' && bridge.deleteMessageTask) return bridge.deleteMessageTask(messageId);
    return bridge.completeMessageTask(messageId);
  }

  function taskErrorText(reason, lang = 'ru') {
    const text = {
      invalid_time: ['Время дела указано неверно', 'Task time is invalid'],
      start_after_due: ['Срок начала не может быть позже срока исполнения', 'Start time cannot be after due time'],
      reminder_past: ['Время напоминания не может быть в прошлом', 'Reminder time cannot be in the past'],
    };
    return (text[reason] || [reason, reason])[lang === 'en' ? 1 : 0];
  }

  return {
    GROUPS,
    duePresets,
    dueQuickChoices,
    validateTask,
    taskGroup,
    sortTasks,
    groupLabel,
    formatDue,
    taskRowText,
    overdueTaskCount,
    visibleTasks,
    reminderCard,
    missedSummary,
    missedReminders,
    datedTasks,
    flagClearWarning,
    toggleFlag,
    saveTask,
    completeTask,
    taskErrorText,
  };
});
