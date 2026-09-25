// truemail UI module: activity-log.js
// Журнал событий строки статуса: уведомления, ошибки и действия программы.
// Всплывающих карточек больше нет - последняя запись всегда видна в нижней
// строке окна, остальные открываются по щелчку на ней (issue #120).
// Здесь только чистые функции над состоянием журнала; отрисовка - в composer.js.
// Размер журнала задаёт предел limit_activity_log_entries, своего числа тут нет.
// См. specs/status-activity-log.md.
'use strict';
(function(root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.activityLogModel = api;
})(typeof globalThis !== 'undefined' ? globalThis : this, function() {
  function createLog() {
    return {entries: [], sequence: 0, unseenError: false};
  }

  // Одинаковой считается запись того же вида, того же ящика, с тем же текстом
  // и тем же действием - повтор такой записи новой строки не заводит. Ключ key
  // различает записи с одинаковым текстом, но разным предметом действия: две
  // отправки с одной темой - это две отмены, а не одна.
  function fingerprint(item) {
    return JSON.stringify([
      item.level || 'info',
      item.kind || 'notice',
      item.accountId ?? null,
      item.text || '',
      item.action || '',
      item.key ?? null,
    ]);
  }

  // Действие повторной записи. Пока прежнее действие выполняется, его не
  // трогаем: иначе повтор сообщения снял бы ожидание и разрешил второе нажатие.
  // Иначе действие берётся из нового события - в том числе заново появляется у
  // записи, чьё прежнее действие уже выполнено.
  function repeatAction(entry, item, callbacks) {
    if (entry.actionState === 'pending') return {};
    return {
      level: item.level || entry.level,
      hasAction: Boolean(item.hasAction),
      action: item.action,
      actionLabel: item.actionLabel,
      retryAt: item.retryAt ?? null,
      actionUntil: item.actionUntil ?? null,
      callbacks,
      actionState: '',
      actionStatus: '',
    };
  }

  function trim(entries, capacity) {
    const limit = Number(capacity);
    if (!Number.isFinite(limit) || limit < 1) return entries;
    return entries.slice(0, limit);
  }

  // Добавить запись. Новая запись встаёт первой. Повтор уже записанной
  // поднимает её наверх и увеличивает счётчик; ошибка синхронизации ещё одного
  // ящика того же вида дописывается в уже висящую запись (G3 из
  // error-kinds-and-messages.md), а не заводит вторую.
  function addEntry(log, item, now, capacity, formatAccounts) {
    const entries = (log?.entries || []).map(entry => ({...entry}));
    const unseenError = Boolean(log?.unseenError) || item.level === 'error';
    let sequence = log?.sequence || 0;
    const grouped = item.groupByKind && item.accountId != null
      ? entries.find(entry => entry.groupByKind && entry.kind === item.kind)
      : null;
    if (grouped) {
      const accounts = [...(grouped.accounts || [])];
      const included = (grouped.accountIds || []).some(id => Number(id) === Number(item.accountId));
      if (!included && item.accounts?.[0]) accounts.push(item.accounts[0]);
      const merged = {
        ...grouped,
        accounts,
        accountIds: included ? grouped.accountIds : [...grouped.accountIds, item.accountId],
        // После завершённого действия прежние обработчики своё отработали:
        // новое событие приносит свой. Иначе - обработчик нового ящика
        // дописывается, а повтор уже учтённого ящика второй не заводит (G3).
        callbacks: grouped.actionState && grouped.actionState !== 'pending'
          ? (item.callbacks || [item.callback]).filter(Boolean)
          : included
            ? grouped.callbacks
            : [...grouped.callbacks, ...(item.callbacks || [item.callback]).filter(Boolean)],
        // Подробности относились к первому ящику и вводили бы в заблуждение
        // насчёт второго - у объединённой записи их нет (G4).
        details: included ? grouped.details : '',
        text: typeof formatAccounts === 'function'
          ? formatAccounts(grouped.baseText || item.baseText, accounts, item)
          : grouped.text,
        count: included ? grouped.count + 1 : grouped.count,
        time: now,
      };
      Object.assign(merged, repeatAction(grouped, item, merged.callbacks));
      const rest = entries.filter(entry => entry.id !== grouped.id);
      return {log: {entries: trim([merged, ...rest], capacity), sequence, unseenError}, id: merged.id, merged: true};
    }
    const print = fingerprint(item);
    const repeated = entries.find(entry => entry.fingerprint === print);
    if (repeated) {
      const merged = {
        ...repeated,
        details: item.details || repeated.details,
        count: repeated.count + 1,
        time: now,
        ...repeatAction(repeated, item, (item.callbacks || [item.callback]).filter(Boolean)),
      };
      const rest = entries.filter(entry => entry.id !== repeated.id);
      return {log: {entries: trim([merged, ...rest], capacity), sequence, unseenError}, id: merged.id, merged: true};
    }
    sequence += 1;
    const entry = {
      ...item,
      id: `activity-${sequence}`,
      level: item.level || 'info',
      fingerprint: print,
      count: 1,
      time: now,
      accountIds: item.accountId == null ? [] : [item.accountId],
      callbacks: (item.callbacks || [item.callback]).filter(Boolean),
      actionUntil: item.actionUntil ?? null,
      actionState: '',
      actionStatus: '',
    };
    delete entry.callback;
    return {log: {entries: trim([entry, ...entries], capacity), sequence, unseenError}, id: entry.id, merged: false};
  }

  function mapEntry(log, id, change) {
    return {...log, entries: (log?.entries || []).map(entry => entry.id === id ? change({...entry}) : {...entry})};
  }

  // Поправить запись на месте, не поднимая её: так идёт обратный отсчёт окна
  // отмены отправки.
  function updateEntry(log, id, patch) {
    return mapEntry(log, id, entry => ({...entry, ...patch}));
  }

  // Действие доступно, пока оно есть у записи и не вышел его срок. Истёкшая
  // отмена остаётся в журнале просто строкой истории.
  function actionAvailable(entry, now) {
    if (!entry?.hasAction || !(entry.callbacks || []).length) return false;
    if (entry.actionUntil != null && Number(entry.actionUntil) <= now) return false;
    if (entry.action === 'wait') {
      const until = entry.retryAt ? Date.parse(entry.retryAt) : 0;
      if (!until || until > now) return false;
    }
    return entry.actionState !== 'pending';
  }

  function beginAction(log, id, pendingText) {
    return mapEntry(log, id, entry => ({...entry, actionState: 'pending', actionStatus: pendingText}));
  }

  // Итог действия. Успех снимает действие и дописывает итог, сама запись
  // остаётся в истории как была; отказ заменяет запись новой ошибкой с её
  // собственным действием.
  function finishAction(log, id, outcome, now) {
    const next = mapEntry(log, id, entry => {
      if (outcome.ok) {
        return {...entry, level: 'info', details: '', hasAction: false,
          actionState: 'success', actionStatus: outcome.text, time: now};
      }
      const item = outcome.item || {};
      // Ключ предмета и сроки относились к прежнему действию: у новой причины
      // свои, иначе она не склеилась бы со своим повтором и получила бы чужое
      // истёкшее окно отмены.
      const failed = {...entry, ...item, id: entry.id, level: item.level || 'error',
        key: item.key ?? null, actionUntil: item.actionUntil ?? null, retryAt: item.retryAt ?? null,
        callbacks: (item.callbacks || [item.callback]).filter(Boolean),
        actionState: 'failed', actionStatus: '', time: now};
      delete failed.callback;
      // Запись сменила текст и вид - ключ повтора считается заново, иначе
      // повтор прежней причины склеился бы с записью уже другой причины.
      failed.fingerprint = fingerprint(failed);
      return failed;
    });
    if (!outcome.ok) next.unseenError = true;
    return next;
  }

  function markSeen(log) {
    return {...log, unseenError: false};
  }

  function clearLog(log) {
    return {...createLog(), sequence: log?.sequence || 0};
  }

  function latestEntry(log) {
    return log?.entries?.[0] || null;
  }

  return {
    createLog,
    fingerprint,
    addEntry,
    updateEntry,
    actionAvailable,
    beginAction,
    finishAction,
    markSeen,
    clearLog,
    latestEntry,
  };
});
