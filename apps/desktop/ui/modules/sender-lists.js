// Чистые функции без DOM и Tauri API: разбор адреса отправителя для диалогов,
// подписи списков отправителей, игнорируемых переписок и записей автоочистки,
// проверка числа дней и тексты отчётов. Подключается в index.html обычным
// скриптом и отдаёт всё через один глобальный объект.
// См. specs/blocked-senders.md, specs/ignore-conversation.md,
// specs/sweep-by-sender.md.

// Границы числа дней режима "старше N дней" (sweep-by-sender.md, S-032). Те же
// числа проверяет ядро: интерфейс объясняет отказ заранее, но решение всё равно
// принимает ядро.
const SWEEP_MIN_DAYS = 1;
const SWEEP_MAX_DAYS = 3650;

// Виды записи списка и решения (blocked-senders.md, S-006).
const POLICY_KINDS = [
  {id: 'address', ru: 'Адрес целиком', en: 'Whole address'},
  {id: 'domain', ru: 'Весь домен', en: 'Whole domain'},
];
const POLICY_DECISIONS = [
  {id: 'blocked', ru: 'Заблокирован', en: 'Blocked'},
  {id: 'trusted', ru: 'Доверенный', en: 'Trusted'},
];

// Виды уборки в диалоге автоочистки (sweep-by-sender.md, S-008).
const SWEEP_MODES = [
  {id: 'once', ru: 'Убрать все письма этого отправителя', en: 'Clean up all messages from this sender'},
  {id: 'new_now', ru: 'Новые сразу в корзину', en: 'New messages straight to trash'},
  {id: 'only_last', ru: 'Хранить только последнее письмо', en: 'Keep only the latest message'},
  {id: 'older_than', ru: 'Убирать письма старше N дней', en: 'Clean up messages older than N days'},
];

// Состояния игнорируемой переписки (ignore-conversation.md, S-032).
const IGNORE_STATES = {
  enabled: ['Игнорируется', 'Ignored'],
  disabling: ['Прекращается', 'Stopping'],
  returning: ['Идёт возврат писем', 'Returning messages'],
  disabled: ['Игнорирование снято', 'Ignoring is off'],
  return_failed: ['Возврат неполный', 'Return incomplete'],
};

const senderText = (item, lang) => (lang === 'en' ? item.en : item.ru);

function policyKindText(kind, lang) {
  const found = POLICY_KINDS.find(item => item.id === kind) || POLICY_KINDS[0];
  return senderText(found, lang);
}

function policyDecisionText(decision, lang) {
  const found = POLICY_DECISIONS.find(item => item.id === decision) || POLICY_DECISIONS[0];
  return senderText(found, lang);
}

function sweepModeText(mode, days, lang) {
  const found = SWEEP_MODES.find(item => item.id === mode);
  if (!found) return mode || '';
  if (mode !== 'older_than') return senderText(found, lang);
  const number = Number(days) || SWEEP_MIN_DAYS;
  return lang === 'en'
    ? `Clean up messages older than ${number} days`
    : `Убирать письма старше ${number} дней`;
}

function ignoreStateText(state, lang) {
  const found = IGNORE_STATES[state] || IGNORE_STATES.enabled;
  return lang === 'en' ? found[1] : found[0];
}

// Адрес отправителя открытого письма: диалог блокировки предлагает адрес и
// домен, а по умолчанию выбирает адрес (blocked-senders.md, S-027, S-028).
// Отображаемое имя в значение не попадает: оно подделывается и меняется.
function senderChoice(message) {
  const raw = String(message?.from?.email || message?.from_addr || '').trim();
  const inner = raw.includes('<') && raw.includes('>')
    ? raw.slice(raw.indexOf('<') + 1, raw.lastIndexOf('>')).trim()
    : raw;
  const at = inner.lastIndexOf('@');
  if (at <= 0 || at === inner.length - 1) return null;
  const domain = inner.slice(at + 1).replace(/\.+$/, '').toLowerCase();
  // Домен без точки записью не становится: такая запись накрыла бы целую
  // доменную зону (S-013).
  if (!domain.includes('.')) return null;
  return {address: inner, domain, kind: 'address'};
}

// Число дней режима "старше N дней": целое от 1 до 3650 (S-032).
function validSweepDays(value) {
  const number = Number(value);
  return Number.isInteger(number) && number >= SWEEP_MIN_DAYS && number <= SWEEP_MAX_DAYS;
}

// Состав уборки, который уходит в ядро. Адрес берётся из письма и в диалоге не
// правится: уборка по домену здесь не предлагается (S-046, "Интерфейсы и
// данные").
function sweepInput(choice, form) {
  return {
    address: choice.address,
    mode: form.mode,
    account_id: form.accountId ?? null,
    days: form.mode === 'older_than' ? Number(form.days) : null,
    sweep_archive: Boolean(form.sweepArchive),
  };
}

// Строка отчёта прохода уборки: поставлено, пропущено, отказов и остаток
// (blocked-senders.md S-036, sweep-by-sender.md S-040).
function sweepReportText(report, lang) {
  if (!report) return '';
  const parts = lang === 'en'
    ? [`queued ${report.queued || 0}`, `skipped ${report.skipped || 0}`, `failed ${report.failed || 0}`]
    : [`поставлено ${report.queued || 0}`, `пропущено ${report.skipped || 0}`, `отказов ${report.failed || 0}`];
  if (report.remaining) {
    parts.push(lang === 'en' ? `left ${report.remaining}` : `осталось ${report.remaining}`);
  }
  if (report.irreversible) {
    parts.push(lang === 'en'
      ? `already running ${report.irreversible}`
      : `уже выполняется ${report.irreversible}`);
  }
  return parts.join(', ');
}

// Отчёт о возврате писем игнорируемой переписки: обещание сформулировано
// честно, число непрошедших возврат называется (ignore-conversation.md, S-034,
// S-038).
function returnReportText(report, lang) {
  if (!report) return '';
  const parts = lang === 'en'
    ? [`returned ${report.queued || 0}`, `not found in trash ${report.skipped || 0}`]
    : [`возвращено ${report.queued || 0}`, `не найдено в корзине ${report.skipped || 0}`];
  if (report.failed) {
    parts.push(lang === 'en' ? `failed ${report.failed}` : `отказов ${report.failed}`);
  }
  if (report.irreversible) {
    parts.push(lang === 'en'
      ? `already running ${report.irreversible}`
      : `уже выполняется ${report.irreversible}`);
  }
  return parts.join(', ');
}

// Подпись записи списка отправителей для раздела настроек (S-045).
function policyRowText(policy, lang) {
  const kind = policyKindText(policy.kind, lang);
  const decision = policyDecisionText(policy.decision, lang);
  const swept = policy.swept
    ? (lang === 'en' ? `, cleaned up ${policy.swept}` : `, убрано писем: ${policy.swept}`)
    : '';
  return `${kind}: ${policy.value} - ${decision}${swept}`;
}

// Поздний отказ очереди: письмо осталось на месте уже после того, как проход
// отчитался об успехе (blocked-senders.md S-048, ignore-conversation.md S-049,
// sweep-by-sender.md S-044).
function queueFailureText(record, lang) {
  if (!record || !record.queue_failed) return '';
  const head = lang === 'en'
    ? `queue failures ${record.queue_failed}`
    : `отказов очереди: ${record.queue_failed}`;
  return record.queue_error ? `${head} - ${record.queue_error}` : head;
}

// Подпись игнорируемой переписки: тема, ящик, участники, дата включения,
// состояние, признак частичного покрытия и счётчики (ignore-conversation.md,
// S-031).
function ignoreRowText(record, lang) {
  const subject = record.subject || (lang === 'en' ? 'no subject' : 'без темы');
  const parts = [`${subject} (${record.account_email})`];
  if (record.participants) parts.push(record.participants);
  parts.push(lang === 'en' ? `since ${record.created_at}` : `включено ${record.created_at}`);
  parts.push(ignoreStateText(record.state, lang));
  if (record.partial) parts.push(lang === 'en' ? 'partial coverage' : 'частичное покрытие');
  parts.push(lang === 'en'
    ? `messages moved ${record.moved || 0}`
    : `убрано писем: ${record.moved || 0}`);
  if (record.returned) {
    parts.push(lang === 'en' ? `returned ${record.returned}` : `возвращено: ${record.returned}`);
  }
  if (record.skipped) {
    parts.push(lang === 'en' ? `skipped ${record.skipped}` : `пропущено: ${record.skipped}`);
  }
  if (record.failed) {
    parts.push(lang === 'en' ? `not returned ${record.failed}` : `не вернулось: ${record.failed}`);
  }
  const queue = queueFailureText(record, lang);
  if (queue) parts.push(queue);
  if (record.last_error) parts.push(record.last_error);
  return parts.join(' - ');
}

// Область записи автоочистки: один ящик либо все ящики (S-026, S-037).
function sweepScopeText(rule, accounts, lang) {
  if (!rule.account_id) return lang === 'en' ? 'All mailboxes' : 'Все ящики';
  const account = (accounts || []).find(item => item.id === rule.account_id);
  return account ? account.email : (lang === 'en' ? 'Mailbox' : 'Ящик');
}

const senderListsModel = {
  SWEEP_MIN_DAYS,
  SWEEP_MAX_DAYS,
  POLICY_KINDS,
  POLICY_DECISIONS,
  SWEEP_MODES,
  policyKindText,
  policyDecisionText,
  sweepModeText,
  ignoreStateText,
  senderChoice,
  validSweepDays,
  sweepInput,
  sweepReportText,
  returnReportText,
  policyRowText,
  sweepScopeText,
  queueFailureText,
  ignoreRowText,
};
if (typeof module !== 'undefined' && module.exports) module.exports = senderListsModel;
