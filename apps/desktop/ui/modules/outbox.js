// truemail UI module: outbox.js
// Чистые функции очереди отправки без DOM и Tauri API: обратный отсчёт окна
// отмены, подписи состояний раздела "Исходящие", итог отмены и состав
// композера для возвращённого письма. Подключается в index.html обычным
// скриптом и отдаёт всё через один глобальный объект.
// См. specs/undo-send.md.

// Границы длительности окна отмены (S-012). Те же числа проверяет ядро:
// интерфейс объясняет отказ заранее, но решение принимает ядро.
const UNDO_MIN_SECONDS = 0;
const UNDO_MAX_SECONDS = 60;

// Состояния операции отправки (S-087 - S-091 словаря спецификации).
const SEND_STATUS_TEXT = {
  pending: ['Ожидает отправки', 'Waiting to be sent'],
  processing: ['Передаётся серверу', 'Being sent'],
  retry: ['Ждёт следующей попытки', 'Waiting for the next attempt'],
  failed: ['Отправить не удалось', 'Sending failed'],
  cancelled: ['Отменено', 'Cancelled'],
  // S-064: неопределённый итог называется неопределённым и не выдаётся ни за
  // отказ, ни за успешную отправку.
  uncertain: ['Итог неизвестен', 'Outcome unknown'],
};

const SEND_ORIGIN_TEXT = {
  ordinary: ['Письмо', 'Message'],
  scheduled: ['Отправка по времени', 'Scheduled send'],
  automatic: ['Служебное письмо', 'Automatic message'],
  external: ['Письмо внешней программы', 'Message from an external app'],
};

const outboxText = (pair, lang) => (lang === 'en' ? pair[1] : pair[0]);

// Время из базы приходит без обозначения зоны ("2026-09-18 12:00:00") и всегда
// во всемирном времени. Без явной метки браузер прочитал бы его как местное, и
// обратный отсчёт ошибся бы на смещение часового пояса.
function parseQueueTime(value) {
  const text = String(value || '').trim();
  if (!text) return NaN;
  if (/[zZ]$|[+-]\d{2}:?\d{2}$/.test(text)) return Date.parse(text);
  return Date.parse(`${text.replace(' ', 'T')}Z`);
}

// Целое число оставшихся секунд окна отмены (S-017, S-018).
function remainingUndoSeconds(cancelUntil, now) {
  const until = parseQueueTime(cancelUntil);
  if (!Number.isFinite(until)) return 0;
  return Math.max(0, Math.ceil((until - Number(now)) / 1000));
}

// Показывать ли действие "Отменить". Нулевое окно отмены его не показывает, а
// служебное письмо и письмо внешней программы карточки не получают вовсе
// (S-019, S-058, S-059).
function showsUndoAction(queued, now) {
  if (!queued || queued.duplicate) return false;
  const origin = queued.origin || 'ordinary';
  if (origin !== 'ordinary') return false;
  return remainingUndoSeconds(queued.cancel_until, now) > 0;
}

// Подпись карточки окна отмены: тема, адресаты поля "Кому" и оставшиеся
// секунды (S-017).
function undoCardText(request, secondsLeft, lang) {
  const subject = String(request?.subject || '').trim()
    || outboxText(['без темы', 'no subject'], lang);
  const to = (request?.to || []).join(', ');
  const seconds = Math.max(0, Math.trunc(secondsLeft));
  return lang === 'en'
    ? `Sending "${subject}" to ${to} in ${seconds} s`
    : `Отправка "${subject}" для ${to} через ${seconds} с`;
}

function sendStatusText(status, lang) {
  const pair = SEND_STATUS_TEXT[status] || SEND_STATUS_TEXT.pending;
  return outboxText(pair, lang);
}

function sendOriginText(origin, lang) {
  const pair = SEND_ORIGIN_TEXT[origin] || SEND_ORIGIN_TEXT.ordinary;
  return outboxText(pair, lang);
}

// Строка раздела "Исходящие": тема, адресаты, состояние и пояснение про
// выключенный ящик (S-020, S-029). Скрытые копии в строке не показываются.
function outboxRowText(entry, lang) {
  const subject = String(entry?.subject || '').trim()
    || outboxText(['без темы', 'no subject'], lang);
  const parts = [subject];
  const to = (entry?.to || []).join(', ');
  if (to) parts.push(to);
  parts.push(sendStatusText(entry?.status, lang));
  if (entry && entry.account_enabled === false) {
    parts.push(outboxText(['ящик выключен, письмо ждёт его включения', 'the mailbox is off, the message waits for it'], lang));
  }
  if (entry?.attempts > 0) {
    parts.push(lang === 'en' ? `attempts: ${entry.attempts}` : `попыток: ${entry.attempts}`);
  }
  if (entry?.last_error) parts.push(entry.last_error);
  return parts.join(' - ');
}

// Итог отмены: отказ после начала передачи называется прямо, а уже принятое
// сервером письмо не сопровождается обещанием отзыва (S-038, S-044).
function cancelOutcomeText(outcome, lang) {
  if (outcome === 'cancelled') {
    return outboxText(['Отправка отменена, письмо возвращено', 'Sending cancelled, the message is back'], lang);
  }
  if (outcome === 'already_sending') {
    return outboxText(['Отправка уже началась, отменить нельзя', 'Sending has already started and cannot be cancelled'], lang);
  }
  return outboxText(['Письмо отправлено, отозвать его нельзя', 'The message is sent and cannot be recalled'], lang);
}

// Одно сообщение про письма, чьё окно отмены истекло, пока программа не
// работала (S-036).
function expiredWindowsText(count, lang) {
  const number = Math.max(0, Math.trunc(Number(count) || 0));
  return lang === 'en'
    ? `Messages waiting to be sent: ${number}`
    : `Писем ожидает отправки: ${number}`;
}

// Состав композера для возвращённого письма (S-040): адресаты, тема,
// оформленное тело и все вложения без потерь.
function composerDraftFromCancelled(message) {
  return {
    account_id: message?.account_id ?? null,
    operation_id: message?.operation_id ?? null,
    to: message?.to || [],
    cc: message?.cc || [],
    bcc: message?.bcc || [],
    subject: message?.subject || '',
    body_html: message?.body_html || '',
    body_text: message?.body_text || '',
    attachments: (message?.attachments || []).map(item => ({
      filename: item.filename,
      mime_type: item.mime_type,
      data: Array.isArray(item.data) ? item.data : Array.from(item.data || []),
    })),
  };
}

// Границы длительности окна отмены проверяются и здесь: пользователю не нужно
// ждать ответа ядра, чтобы увидеть отказ (S-012).
function validUndoSeconds(value) {
  // Пустое поле ввода даёт пустую строку, а она превращается в ноль: без этой
  // проверки очищенное поле молча выключило бы окно отмены.
  if (typeof value === 'string' && !value.trim()) return false;
  const number = Number(value);
  return Number.isInteger(number) && number >= UNDO_MIN_SECONDS && number <= UNDO_MAX_SECONDS;
}

const outboxModel = {
  UNDO_MIN_SECONDS,
  UNDO_MAX_SECONDS,
  parseQueueTime,
  remainingUndoSeconds,
  showsUndoAction,
  undoCardText,
  sendStatusText,
  sendOriginText,
  outboxRowText,
  cancelOutcomeText,
  expiredWindowsText,
  composerDraftFromCancelled,
  validUndoSeconds,
};
if (typeof module !== 'undefined' && module.exports) module.exports = outboxModel;
if (typeof window !== 'undefined') window.outboxModel = outboxModel;
