// truemail UI module: recipient-history.js
// Чистые функции подсказки получателей и раздела управления историей без DOM
// и Tauri API. Порядок кандидатов задаёт ядро, интерфейс его только сохраняет
// и прекращает отбор на пределе подсказок (S-031, S-032).
// См. specs/recipient-history.md.

// Длина подсказки (S-032) - настройка ядра, а не число здесь.
const historyLimits = typeof module === 'object' && module.exports
  ? require('./limits.js')
  : globalThis.limitsModel;
// Пока перечень пределов не загружен, отбор не обрезается: придуманное здесь
// число и было бы второй копией предела.
const suggestionLimit = () => historyLimits.limitValue(historyLimits.KEYS.recipientSuggestions) ?? Infinity;

const historyText = (pair, lang) => (lang === 'en' ? pair[1] : pair[0]);

// Пометка кандидата, взятого только из истории переписки (S-039).
function historyCandidateBadge(candidate, lang) {
  return candidate?.source === 'history'
    ? historyText(['из переписки', 'from correspondence'], lang)
    : '';
}

// Отбор подсказок по кандидатам ядра. Порядок входного перечня сохраняется:
// упорядочивание в интерфейсе зависело бы от поверхности, а отбор прекращается
// на пределе по этому же порядку (S-031).
function historySuggestions(candidates, query, used, keysFor, search) {
  const engine = search || (typeof personSearch !== 'undefined' ? personSearch : null);
  if (!engine) return [];
  return engine.suggestRecipients(
    candidates || [],
    query,
    used,
    suggestionLimit(),
    keysFor,
  );
}

// Строка раздела управления историей: имя, адрес, число сохранённых отметок и
// дата последнего обращения. Число названо числом отметок намеренно: отметок
// хранится не больше настроенного предела, а писем могло быть больше (S-042).
function historyRowText(entry, lang) {
  const name = String(entry?.name || '').trim();
  const address = String(entry?.address || '').trim();
  const head = name && name.toLowerCase() !== address.toLowerCase()
    ? `${name} <${address}>`
    : address;
  const parts = [head];
  parts.push(lang === 'en'
    ? `saved marks: ${entry?.saved_touches ?? 0}`
    : `сохранённых отметок: ${entry?.saved_touches ?? 0}`);
  if (entry?.last_used_at) {
    parts.push(lang === 'en'
      ? `last message: ${entry.last_used_at}`
      : `последнее письмо: ${entry.last_used_at}`);
  }
  if (entry?.hidden_by_user) parts.push(historyText(['убрано вами', 'removed by you'], lang));
  else if (entry?.evicted) parts.push(historyText(['вытеснено пределом', 'pushed out by the limit'], lang));
  return parts.join(' - ');
}

// Подпись кандидата в подсказке: имя контакта, изменённое пользователем имя
// записи или сам адрес (S-036 - S-038).
function historyCandidateLabel(candidate) {
  const name = String(candidate?.name || '').trim();
  const email = String(candidate?.email || '').trim();
  return name && name.toLowerCase() !== email.toLowerCase() ? name : email;
}

const recipientHistoryModel = {
  suggestionLimit,
  historyCandidateBadge,
  historySuggestions,
  historyRowText,
  historyCandidateLabel,
};
if (typeof module !== 'undefined' && module.exports) module.exports = recipientHistoryModel;
if (typeof window !== 'undefined') window.recipientHistoryModel = recipientHistoryModel;
