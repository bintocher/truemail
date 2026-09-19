// Проверки сроков у флажка письма и списка дел: быстрые значения срока,
// согласованность сроков, группы и порядок списка дел, подписи строк, карточка
// напоминания, сводка пропущенных и настоящий путь установки флажка через мост
// команд. Разметка здесь не участвует: проверяются чистые модули и обращения к
// мосту.
// Спецификация: specs/flag-due-dates.md.
// Запуск: node --test apps/desktop/tests/js/flag-due-dates.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');

// Модуль сроков флажка появляется вместе с реализацией. Пока его нет, каждая
// проверка падает на своём месте и своими словами, а не одной общей ошибкой
// загрузки файла.
function loadFlagModule() {
  try {
    return require('../../ui/modules/flag-due-dates.js');
  } catch (error) {
    if (error && error.code === 'MODULE_NOT_FOUND') return null;
    throw error;
  }
}

const flag = loadFlagModule();

function fn(name, whatBreaks) {
  if (flag && typeof flag[name] === 'function') return flag[name];
  return () => {
    throw new Error(`${name}: ${whatBreaks}`);
  };
}

// Имена границы выбраны автором кода: модуль отдаёт duePresets, validateTask,
// taskGroup, groupLabel, formatDue и taskErrorText, а время напоминания зовёт
// reminder_at - тем же именем, что и ядро. Ниже только приведение обращений.
function taskTimesError(times, now, lang) {
  const check = fn('validateTask', 'сроки принимаются без проверки: дело получит срок начала позже срока исполнения')(times, now);
  if (check.ok) return null;
  return fn('taskErrorText', 'причина отказа не называется: сроки пропадают молча')(check.reason, lang);
}

// Подпись срока в строке списка писем собирается из текста срока и признака
// просрочки: их модуль отдаёт двумя своими функциями.
function dueBadge(item, now, lang) {
  const due = (item.task || item).due_at;
  if (!due) return null;
  const text = fn('formatDue', 'подписи срока в строке списка писем нет: срок виден только в списке дел')(due, lang, now);
  const group = fn('taskGroup', 'просроченный срок не отличается от обычного')(item, now);
  return {text, overdue: group === 'overdue'};
}

const NOW = Date.parse('2026-09-18T12:00:00Z');

function task(extra) {
  return Object.assign(
    {
      message_id: 1,
      subject: 'Договор',
      from_name: 'Начальник',
      from_addr: 'boss@example.test',
      account_email: 'me@example.test',
      folder_id: 10,
      state: 'active',
      due_at: null,
      start_at: null,
      reminder_at: null,
      completed_at: null,
      snoozed_until: null,
      takeaway_pending: false,
    },
    extra,
  );
}

test('S-016, S-100: быстрые значения срока повторяют окно откладывания и переводятся', () => {
  // Два соседних окна выбора времени предлагали бы пользователю разные наборы,
  // и одно и то же "Завтра в 09:00" означало бы в них разное.
  const choices = fn('dueQuickChoices', 'выбрать срок исполнения нечем: окно сроков пустое')('ru');
  const titles = choices.map(choice => choice.title);
  for (const wanted of ['Сегодня', 'Завтра в 09:00', 'В понедельник в 09:00', 'На следующей неделе', 'Без срока']) {
    assert.ok(titles.includes(wanted), `в наборе нет значения "${wanted}": ${titles.join(', ')}`);
  }
  assert.ok(
    choices.some(choice => choice.id === 'custom'),
    'произвольные дата и время обязаны остаться: иначе срок можно поставить только из готового набора',
  );
  const english = fn('dueQuickChoices', 'выбрать срок исполнения нечем')('en');
  assert.ok(
    english.every(choice => !/[А-Яа-я]/.test(choice.title)),
    'при английском языке подписи сроков остались русскими',
  );
});

test('S-019 - S-023: срок начала не позже исполнения, напоминание не в прошлом, пустой срок исполнения допустим', () => {
  // Отклонённое сохранение обязано назвать причину: молчаливый отказ выглядит
  // потерей введённых сроков.
  const validate = taskTimesError;
  assert.equal(validate({start_at: '2026-09-19T09:00:00Z', due_at: '2026-09-20T09:00:00Z', reminder_at: null}, NOW, 'ru'), null);
  assert.equal(validate({start_at: '2026-09-20T09:00:00Z', due_at: '2026-09-20T09:00:00Z', reminder_at: null}, NOW, 'ru'), null,
    'равные сроки начала и исполнения допустимы: запрещено только "позже"');
  const reversed = validate({start_at: '2026-09-21T09:00:00Z', due_at: '2026-09-20T09:00:00Z', reminder_at: null}, NOW, 'ru');
  assert.ok(reversed, 'срок начала позже срока исполнения принят');
  assert.ok(/срок/i.test(reversed), `отказ обязан объяснять причину: ${reversed}`);
  const past = validate({start_at: null, due_at: null, reminder_at: '2026-09-18T11:59:00Z'}, NOW, 'ru');
  assert.ok(past, 'время напоминания в прошлом принято: напоминание не пришло бы никогда');
  // Напоминание без срока исполнения и позже него - обычные случаи, а не ошибка.
  assert.equal(validate({start_at: null, due_at: null, reminder_at: '2026-09-19T09:00:00Z'}, NOW, 'ru'), null);
  assert.equal(validate({start_at: null, due_at: '2026-09-19T09:00:00Z', reminder_at: '2026-09-25T09:00:00Z'}, NOW, 'ru'), null);
});

// Границы групп проходят по суткам часового пояса компьютера, а не по суткам
// всемирного времени: иначе вечернее дело попадало бы в "Завтра" у всех, кто
// живёт восточнее Гринвича.
function localDue(dayShift, hours, minutes) {
  const value = new Date(NOW);
  value.setDate(value.getDate() + dayShift);
  value.setHours(hours, minutes, 0, 0);
  return value.toISOString();
}

test('S-046: список дел разбит на группы по границам суток', () => {
  const group = fn('taskGroup', 'список дел не разделяется на группы: просроченное смешано с сегодняшним');
  assert.equal(group(task({due_at: localDue(-1, 9, 0)}), NOW), 'overdue');
  assert.equal(group(task({due_at: localDue(0, 23, 59)}), NOW), 'today');
  assert.equal(group(task({due_at: localDue(1, 0, 1)}), NOW), 'tomorrow');
  // "На неделе" - ближайшие 7 суток после завтрашнего дня.
  assert.equal(group(task({due_at: localDue(3, 10, 0)}), NOW), 'week');
  assert.equal(group(task({due_at: localDue(42, 10, 0)}), NOW), 'later');
  assert.equal(group(task({due_at: null}), NOW), 'none');
  assert.equal(group(task({state: 'done', due_at: localDue(-1, 9, 0), completed_at: localDue(0, 8, 0)}), NOW), 'done',
    'выполненное дело уходит в свою группу, а не остаётся просроченным');
});

test('S-047: список дел выстроен полным порядком', () => {
  // Полный порядок: группа, срок по возрастанию, дата письма по убыванию,
  // номер письма по убыванию. Неполный порядок дал бы повтор и потерю строк
  // между страницами.
  const sorted = fn('sortTasks', 'порядок списка дел не задан: страницы будут повторять и терять строки')([
    task({message_id: 3, due_at: '2026-09-19T09:00:00Z', date: '2026-09-10T10:00:00Z'}),
    task({message_id: 1, due_at: '2026-09-17T09:00:00Z', date: '2026-09-10T10:00:00Z'}),
    task({message_id: 5, due_at: '2026-09-19T09:00:00Z', date: '2026-09-12T10:00:00Z'}),
    task({message_id: 4, due_at: '2026-09-19T09:00:00Z', date: '2026-09-10T10:00:00Z'}),
  ], NOW);
  assert.deepEqual(sorted.map(item => item.message_id), [1, 5, 4, 3]);
});

test('S-048, S-049, S-054: строка списка дел называет письмо и ящик, пустые значения подписаны, счётчик считает просроченные', () => {
  const rowText = fn('taskRowText', 'строка списка дел пуста: письмо в ней не опознать');
  const full = rowText(task({due_at: '2026-09-19T09:00:00Z'}), 'ru');
  for (const piece of ['Договор', 'Начальник', 'me@example.test']) {
    assert.ok(full.includes(piece), `в строке нет "${piece}": ${full}`);
  }
  const empty = rowText(task({subject: '', from_name: '', from_addr: ''}), 'ru');
  assert.ok(empty.includes('Без темы'), `пустая тема не подписана: ${empty}`);
  assert.ok(empty.includes('Отправитель неизвестен'), `неизвестный отправитель не подписан: ${empty}`);

  // Счётчик рядом с разделом "Дела" считает только просроченные дела в работе.
  const overdue = fn('overdueTaskCount', 'счётчика просроченных дел нет: пользователь не увидит, что сроки горят');
  assert.equal(overdue([
    task({due_at: '2026-09-17T09:00:00Z'}),
    task({message_id: 2, due_at: '2026-09-19T09:00:00Z'}),
    task({message_id: 3, due_at: '2026-09-17T09:00:00Z', state: 'done', completed_at: '2026-09-18T08:00:00Z'}),
    task({message_id: 4, due_at: '2026-09-17T09:00:00Z', state: 'detached'}),
  ], NOW), 1);
});

test('S-052, S-053: уводимое письмо в списке дел не показывается, отложенное показывается с временем возврата', () => {
  const visible = fn('visibleTasks', 'список дел не отбирает письма: уводимое письмо покажется там, где его уже нет');
  const rows = visible([
    task({message_id: 1}),
    task({message_id: 2, takeaway_pending: true}),
    task({message_id: 3, snoozed_until: '2026-09-20T09:00:00Z'}),
  ], NOW);
  assert.deepEqual(rows.map(item => item.message_id), [1, 3],
    'уводимое письмо обязано исчезнуть, а отложенное - остаться: в списках писем его не видно, а срок по нему горит');
  const text = fn('taskRowText', 'строка списка дел пуста')(
    task({message_id: 3, snoozed_until: '2026-09-20T09:00:00Z'}), 'ru');
  assert.ok(/отложено|возврат/i.test(text), `отложенное письмо не подписано временем возврата: ${text}`);
});

test('S-055, S-056, S-018: строка списка писем несёт краткую подпись срока, просроченный срок выделен', () => {
  const badge = dueBadge;
  const soon = badge(task({due_at: '2026-09-19T09:00:00Z'}), NOW, 'ru');
  assert.ok(soon && soon.text, 'у письма со сроком нет подписи');
  assert.equal(soon.overdue, false);
  const late = badge(task({due_at: '2026-09-17T09:00:00Z'}), NOW, 'ru');
  assert.equal(late.overdue, true, 'просроченный срок не отличается от обычного');
  assert.equal(badge(task({due_at: null}), NOW, 'ru'), null, 'письмо без срока подписи не получает');
  // Время показывается в часовом поясе компьютера, а не во всемирном.
  const local = badge(task({due_at: '2026-09-19T21:30:00Z'}), NOW, 'ru');
  const expected = new Date('2026-09-19T21:30:00Z').toLocaleTimeString('ru', {hour: '2-digit', minute: '2-digit'});
  assert.ok(local.text.includes(expected), `срок показан не в часовом поясе компьютера: ${local.text}`);
});

test('S-063, S-064: карточка напоминания называет письмо и предлагает три действия', () => {
  const card = fn('reminderCard', 'карточка напоминания не собирается: напоминание о сроке не покажется')(
    task({due_at: '2026-09-19T09:00:00Z', reminder_at: '2026-09-18T12:00:00Z'}), 'ru');
  assert.ok(card.title.includes('Договор') || card.body.includes('Договор'), 'в карточке нет темы письма');
  assert.ok(`${card.title} ${card.body}`.includes('Начальник'), 'в карточке нет отправителя');
  assert.ok(`${card.title} ${card.body}`.includes('19'), 'в карточке нет срока исполнения');
  assert.deepEqual(card.actions.map(action => action.id), ['open', 'snooze', 'done']);
  const empty = fn('reminderCard', 'карточка напоминания не собирается')(
    task({subject: '', reminder_at: '2026-09-18T12:00:00Z'}), 'ru');
  assert.ok(`${empty.title} ${empty.body}`.includes('Без темы'), 'пустая тема в карточке не подписана');
});

test('S-071, S-073 - S-076: пропущенные напоминания сводятся в одно сообщение, старше 7 суток только отмечаются', () => {
  const summary = fn('missedReminders', 'сводки пропущенных напоминаний нет: неделя отсутствия даст поток карточек')([
    task({message_id: 1, reminder_at: '2026-09-17T09:00:00Z'}),
    task({message_id: 2, reminder_at: '2026-09-16T09:00:00Z'}),
    task({message_id: 3, reminder_at: '2026-09-01T09:00:00Z'}),
  ], NOW, 'ru');
  assert.equal(summary.shown.length, 2, 'в сводку вошли не только свежие пропущенные напоминания');
  assert.ok(summary.text.includes('2'), `сводка не называет число напоминаний: ${summary.text}`);
  assert.deepEqual(summary.marked.sort((a, b) => a - b), [1, 2, 3],
    'отметку о показе получают все разобранные напоминания, иначе та же сводка появится при следующем запуске');
  assert.ok(
    !summary.shown.some(item => item.message_id === 3),
    'напоминание старше 7 суток попало в сводку',
  );
});

test('S-001, S-004, S-005, S-008, S-024 - S-027, S-102: путь флажка и дела идёт одной командой на пачку писем', async () => {
  // Сквозная проверка настоящего пути: поверхности разные, а команда одна.
  const calls = [];
  const bridge = {
    markFlagged: async (ids, flagged, reason) => {
      calls.push({command: 'markFlagged', ids, flagged, reason});
      return ids.length;
    },
    saveMessageTask: async (messageId, times) => {
      calls.push({command: 'saveMessageTask', messageId, times});
      return task({message_id: messageId, due_at: times.due_at});
    },
    completeMessageTask: async messageId => {
      calls.push({command: 'completeMessageTask', messageId});
      return task({message_id: messageId, state: 'done', completed_at: '2026-09-18T12:00:00Z'});
    },
    deleteMessageTask: async messageId => {
      calls.push({command: 'deleteMessageTask', messageId});
      return true;
    },
  };
  const toggle = fn('toggleFlag', 'флажок из окна программы не ставится: мост markFlagged по-прежнему никто не вызывает');
  const shown = [];
  const applied = await toggle(bridge, [11, 12, 13], true, {
    reason: 'user',
    onOptimistic: ids => shown.push(...ids),
    confirm: async () => true,
  });
  assert.equal(applied, 3);
  assert.deepEqual(shown, [11, 12, 13], 'значок строки не показал новое состояние до ответа сервера');
  const flagCalls = calls.filter(call => call.command === 'markFlagged');
  assert.equal(flagCalls.length, 1, 'выделение обработано по письму на вызов, а не одной неделимой операцией');
  assert.deepEqual(flagCalls[0].ids, [11, 12, 13]);
  assert.equal(flagCalls[0].reason, 'user', 'причина изменения признака в ядро не передана');

  // Сроки сохраняются отдельной командой, а отметка о выполнении - своей:
  // ручное снятие флажка удалило бы только что созданное дело.
  await fn('saveTask', 'сроки дела сохранить нечем')(bridge, 11, {start_at: null, due_at: '2026-09-19T09:00:00Z', reminder_at: null});
  await fn('completeTask', 'отметить дело выполненным нечем')(bridge, 11);
  const kinds = calls.map(call => call.command);
  assert.deepEqual(kinds, ['markFlagged', 'saveMessageTask', 'completeMessageTask']);
  assert.ok(
    !calls.some(call => call.command === 'markFlagged' && call.flagged === false),
    'отметка о выполнении сняла флажок командой ручного снятия и стёрла собственное дело',
  );

  // Снятие флажка у дела со сроком спрашивает подтверждение до удаления.
  const asked = [];
  await toggle(bridge, [11], false, {
    reason: 'user',
    task: task({message_id: 11, due_at: '2026-09-19T09:00:00Z'}),
    confirm: async text => {
      asked.push(text);
      return false;
    },
  });
  assert.equal(asked.length, 1, 'сроки удалены без подтверждения');
  assert.ok(
    !calls.some(call => call.command === 'markFlagged' && call.flagged === false),
    'отказ от подтверждения всё равно снял флажок',
  );
});
