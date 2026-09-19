'use strict';
(function(root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.pinMessageModel = api;
})(typeof globalThis !== 'undefined' ? globalThis : this, function() {
  const PINNED_VISIBLE_LIMIT = 50;

  function messageValue(message, sort) {
    if (sort === 'sender') return String(message.from?.name || message.from?.email || '');
    if (sort === 'subject') return String(message.subject || '');
    return String(message.date || '');
  }

  function compareMessages(a, b, sort = 'date-desc', locale = 'ru') {
    if (sort === 'newest') sort = 'date-desc';
    if (sort === 'oldest') sort = 'date-asc';
    if (sort === 'date-desc' || sort === 'date-asc') {
      const direction = sort === 'date-asc' ? 1 : -1;
      const difference = String(a.date || '').localeCompare(String(b.date || ''));
      return difference ? direction * difference : Number(b.id) - Number(a.id);
    }
    const difference = messageValue(a, sort).localeCompare(messageValue(b, sort), locale, {sensitivity: 'base'});
    return difference || Number(b.id) - Number(a.id);
  }

  function filterMessages(rows, options = {}) {
    let result = [...(rows || [])];
    if (options.unread) result = result.filter(message => !message.flags?.seen);
    if (options.attachments) result = result.filter(message => message.has_attachments);
    if (options.flagged) result = result.filter(message => message.flags?.flagged);
    if (options.text) {
      const query = String(options.text).toLocaleLowerCase();
      result = result.filter(message =>
        `${message.from?.name || ''} ${message.from?.email || ''} ${message.subject || ''} ${message.preview || ''}`
          .toLocaleLowerCase()
          .includes(query));
    }
    return result.sort((a, b) => compareMessages(a, b, options.sort, options.locale));
  }

  function conversationKey(message) {
    return message.thread_id != null
      ? `${message.account_id}|t:${message.thread_id}`
      : `${message.account_id}|m:${message.id}`;
  }

  function collapsedRows(rows, options) {
    if (options.collapseThreads === false) return rows;
    const expanded = options.expandedThreads instanceof Set
      ? options.expandedThreads
      : new Set(options.expandedThreads || []);
    const groups = new Map();
    rows.forEach(message => {
      const key = conversationKey(message);
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key).push(message);
    });
    const result = [];
    for (const items of groups.values()) {
      const ordered = [...items].sort((a, b) => compareMessages(a, b, options.sort, options.locale));
      const key = conversationKey(ordered[0]);
      result.push({...ordered[0], _convKey: key, _convCount: ordered.length, threadCount: ordered.length});
      if (ordered.length > 1 && expanded.has(key)) {
        ordered.slice(1).forEach(message => result.push({...message, _convKey: key, _convChild: true}));
      }
    }
    return result.sort((a, b) => compareMessages(a, b, options.sort, options.locale));
  }

  function buildRows(pinned, ordinary, options = {}) {
    const filteredPinned = filterMessages(pinned || [], options);
    const pinnedIds = new Set(filteredPinned.map(message => message.id));
    const filteredOrdinary = filterMessages(
      (ordinary || []).filter(message => !pinnedIds.has(message.id)),
      options,
    );
    const ordinaryRows = collapsedRows(filteredOrdinary, options);
    const shown = filteredPinned.slice(0, PINNED_VISIBLE_LIMIT);
    const hidden = Math.max(0, filteredPinned.length - shown.length);
    return {
      pinned: shown,
      ordinary: ordinaryRows,
      hidden,
      rows: shown.length
        ? [...shown, {kind: 'pinned_separator', hidden}, ...ordinaryRows]
        : ordinaryRows,
    };
  }

  function messageRows(rows) {
    return (rows || []).filter(row => !row?.kind);
  }

  function ordinaryCursor(ordinary) {
    if (!ordinary?.length) return null;
    return ordinary.reduce((oldest, message) => {
      const left = String(message.date || '');
      const right = String(oldest.date || '');
      return left < right || (left === right && Number(message.id) < Number(oldest.id)) ? message : oldest;
    });
  }

  function uniqueMessages(rows) {
    const byId = new Map();
    (rows || []).forEach(message => byId.set(message.id, message));
    return [...byId.values()];
  }

  function mergeReloadedPages(state, options = {}) {
    const oldPinned = uniqueMessages(state?.pinned);
    const oldNormal = uniqueMessages(state?.normal);
    const fresh = uniqueMessages(options.fresh || []);
    const freshIds = new Set(options.freshIds || fresh.map(message => message.id));
    const pageSize = Number(options.pageSize) || 0;
    const counts = new Map();
    const edges = new Map();
    fresh.forEach(message => {
      counts.set(message.folder_id, (counts.get(message.folder_id) || 0) + 1);
      const edge = edges.get(message.folder_id);
      if (!edge || compareMessages(message, edge, 'date-asc') < 0) edges.set(message.folder_id, message);
    });
    const survived = oldNormal.filter(message => {
      if (freshIds.has(message.id)) return true;
      const edge = edges.get(message.folder_id);
      if (edge) {
        if (pageSize && (counts.get(message.folder_id) || 0) < pageSize) return false;
        return compareMessages(message, edge, 'date-asc') < 0;
      }
      if (options.fullPageEdge != null) return String(message.date || '') < String(options.fullPageEdge);
      return !freshIds.size;
    });
    const pinned = uniqueMessages(oldPinned.concat(fresh.filter(message => message.pinned_at)));
    const pinnedIds = new Set(pinned.map(message => message.id));
    const normal = uniqueMessages(survived.concat(fresh.filter(message => !message.pinned_at)))
      .filter(message => !pinnedIds.has(message.id));
    return {pinned, normal};
  }

  function trimToMemoryLimit(state, options = {}) {
    const limit = Math.max(0, Number(options.limit) || 0);
    const keepIds = new Set(options.keepIds || []);
    const pinned = uniqueMessages(state?.pinned);
    const normal = uniqueMessages(state?.normal);
    if (!limit || normal.length <= limit) return {pinned, normal};
    const kept = normal.filter(message => keepIds.has(message.id));
    const keptIds = new Set(kept.map(message => message.id));
    const rest = normal
      .filter(message => !keptIds.has(message.id))
      .sort((a, b) => compareMessages(a, b, 'date-desc'));
    return {pinned, normal: kept.concat(rest.slice(0, Math.max(0, limit - kept.length)))};
  }

  async function togglePin(bridge, ids, pinned) {
    const result = await bridge.setMessagesPinned([...new Set(ids || [])], Boolean(pinned));
    const rejected = rejectedCount(result);
    return {...(result || {}), rejected, rejected_limit: rejected};
  }

  function unpinInPlace(state, id, options = {}) {
    const ids = new Set(Array.isArray(id) ? id : [id]);
    const moved = (state?.pinned || [])
      .filter(message => ids.has(message.id))
      .map(message => ({...message, pinned_at: null}));
    return {
      pinned: (state?.pinned || []).filter(message => !ids.has(message.id)),
      normal: uniqueMessages((state?.normal || []).concat(moved))
        .sort((a, b) => compareMessages(a, b, options.sort, options.locale)),
    };
  }

  function separatorText(hidden, lang = 'ru') {
    const base = lang === 'en' ? 'Pinned' : 'Закрепленные';
    if (!(hidden > 0)) return base;
    return lang === 'en' ? `${base} - ${hidden} more not shown` : `${base} - не показано еще ${hidden}`;
  }

  // Ядро отвечает полем rejected_limit, а ранние ответы моста несли rejected:
  // обе подписи считает одним и тем же числом отклонённых пределом писем.
  function rejectedCount(result) {
    return Number(result?.rejected ?? result?.rejected_limit ?? 0);
  }

  function pinLimitText(result, lang = 'ru') {
    const rejected = rejectedCount(result);
    if (!rejected) return '';
    return lang === 'en'
      ? `Could not pin: ${rejected}. The mailbox pin limit has been reached`
      : `Не удалось закрепить: ${rejected}. Достигнут предел закреплений ящика`;
  }

  return {
    PINNED_VISIBLE_LIMIT,
    compareMessages,
    conversationKey,
    filterMessages,
    buildRows,
    messageRows,
    ordinaryCursor,
    mergeReloadedPages,
    trimToMemoryLimit,
    togglePin,
    unpinInPlace,
    separatorText,
    pinLimitText,
  };
});
