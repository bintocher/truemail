// truemail UI module: out-of-office.js
// Чистые функции автоответа "нет на месте" без DOM и Tauri API: объяснение
// выбранного режима, проверка периода и текстов, разбор перечня внутренних
// доменов. Подключается в index.html обычным скриптом.
// См. specs/out-of-office.md.

// Границы настройки (S-021, S-023, S-026). Те же числа проверяет ядро.
const OOF_MIN_PERIOD_MINUTES = 1;
const OOF_MAX_PERIOD_DAYS = 366;
const OOF_MIN_TEXT_CHARS = 1;
const OOF_MAX_TEXT_CHARS = 10000;
const OOF_MAX_DOMAINS = 20;

const oofText = (pair, lang) => (lang === 'en' ? pair[1] : pair[0]);

// Что программа обещает пользователю в выбранном режиме. Режим выбирается сам,
// поэтому объяснение - единственное место, где пользователь узнаёт разницу
// (S-002, S-004, S-007, S-008, S-020, S-039).
function oofModeExplanation(settings, lang) {
  if (!settings) return '';
  if (settings.available === false) {
    return settings.unavailable_reason
      || oofText(['Автоответ для этого ящика недоступен.', 'Auto-reply is not available for this mailbox.'], lang);
  }
  const lines = [];
  if (settings.mode === 'server') {
    lines.push(oofText([
      'Автоответ хранится на сервере и работает даже при закрытой программе.',
      'The auto-reply is stored on the server and works even when the app is closed.',
    ], lang));
    lines.push(oofText([
      'Защиту от рассылок, частоту ответов и деление отправителей на внутренних и внешних определяет сервер.',
      'The server decides bulk-mail protection, reply frequency and who counts as internal or external.',
    ], lang));
    return lines.join(' ');
  }
  lines.push(oofText([
    'Ответы уходят только пока программа запущена и этот ящик синхронизируется.',
    'Replies are sent only while the app is running and this mailbox is syncing.',
  ], lang));
  if (settings.silence_headers_available === false) {
    lines.push(oofText([
      'Служебные заголовки писем этого ящика недоступны, поэтому часть правил молчания к нему не применяется.',
      'Service headers are not available for this mailbox, so some silence rules do not apply to it.',
    ], lang));
  }
  return lines.join(' ');
}

// Перечень внутренних доменов из поля ввода: значения через запятую, перевод
// строки или точку с запятой, без ведущего знака "@" (S-027).
function oofDomainsFromText(value) {
  return String(value || '')
    .split(/[\s,;]+/)
    .map(item => item.trim().replace(/^@/, '').replace(/\.$/, '').toLowerCase())
    .filter(Boolean)
    .filter((item, index, list) => list.indexOf(item) === index);
}

// Проверка настройки до обращения к ядру: отказ объясняется сразу, но решение
// всё равно принимает ядро (S-021, S-023, S-026).
function oofValidationError(input, lang) {
  if (!input || !input.enabled) return null;
  const start = Date.parse(input.starts_at);
  const end = Date.parse(input.ends_at);
  if (!Number.isFinite(start) || !Number.isFinite(end)) {
    return oofText(['Укажите начало и окончание периода.', 'Set the start and the end of the period.'], lang);
  }
  const minutes = (end - start) / 60000;
  if (minutes < OOF_MIN_PERIOD_MINUTES) {
    return oofText([
      'Окончание периода должно быть позже начала не менее чем на 1 минуту.',
      'The end of the period must be at least 1 minute after its start.',
    ], lang);
  }
  if (minutes > OOF_MAX_PERIOD_DAYS * 24 * 60) {
    return oofText([
      'Период отсутствия не длиннее 366 суток.',
      'The absence period cannot be longer than 366 days.',
    ], lang);
  }
  for (const text of [input.internal_text, input.external_text]) {
    const length = String(text || '').trim().length;
    if (length < OOF_MIN_TEXT_CHARS || length > OOF_MAX_TEXT_CHARS) {
      return oofText([
        'Текст автоответа задаётся длиной от 1 до 10000 символов.',
        'The auto-reply text must be between 1 and 10000 characters.',
      ], lang);
    }
  }
  const domains = input.internal_domains || [];
  if (!domains.length) {
    return oofText(['Нужен хотя бы один внутренний домен.', 'At least one internal domain is required.'], lang);
  }
  if (domains.length > OOF_MAX_DOMAINS) {
    return oofText([
      'Внутренних доменов не больше 20.',
      'No more than 20 internal domains are allowed.',
    ], lang);
  }
  if (domains.some(domain => !domain.includes('.'))) {
    return oofText([
      'Домен без точки записью не считается.',
      'A domain without a dot is not accepted.',
    ], lang);
  }
  return null;
}

// Состав, уходящий в ядро. Локальное время поля ввода переводится во
// всемирное: период сравнивается с временем получения письма (S-013, S-034).
function oofInput(form) {
  const stamp = value => {
    const parsed = Date.parse(value);
    return Number.isFinite(parsed) ? new Date(parsed).toISOString() : '';
  };
  return {
    account_id: Number(form?.accountId) || 0,
    enabled: Boolean(form?.enabled),
    starts_at: stamp(form?.startsAt),
    ends_at: stamp(form?.endsAt),
    internal_text: String(form?.internalText || '').trim(),
    external_text: String(form?.externalText || '').trim(),
    internal_domains: oofDomainsFromText(form?.domains),
  };
}

// Подпись состояния автоответа для списка ящиков.
function oofStateText(settings, lang) {
  if (!settings || settings.available === false) {
    return oofText(['недоступен', 'unavailable'], lang);
  }
  if (!settings.enabled) return oofText(['выключен', 'off'], lang);
  return settings.mode === 'server'
    ? oofText(['включён на сервере', 'on, stored on the server'], lang)
    : oofText(['включён в программе', 'on, handled by the app'], lang);
}

const outOfOfficeModel = {
  OOF_MIN_PERIOD_MINUTES,
  OOF_MAX_PERIOD_DAYS,
  OOF_MIN_TEXT_CHARS,
  OOF_MAX_TEXT_CHARS,
  OOF_MAX_DOMAINS,
  oofModeExplanation,
  oofDomainsFromText,
  oofValidationError,
  oofInput,
  oofStateText,
};
if (typeof module !== 'undefined' && module.exports) module.exports = outOfOfficeModel;
if (typeof window !== 'undefined') window.outOfOfficeModel = outOfOfficeModel;
