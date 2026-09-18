//! Автоответ "нет на месте": режимы, границы настройки и правила молчания
//! (specs/out-of-office.md).
//!
//! Правила молчания - чистые функции: ответ рассылке, чужому автоответу или
//! отчёту о недоставке запускает бесконечную переписку двух программ, поэтому
//! их решение проверяется без базы и без сети.

use super::Addr;
use serde::{Deserialize, Serialize};

/// Название стадии автоответа в столбце результата стадии письма. Автоответ -
/// пятая стадия, после правил обработки (S-030).
pub const OUT_OF_OFFICE_STAGE_NAME: &str = "out_of_office";

/// Режим автоответа. Серверный - настройка живёт на сервере Exchange и
/// работает при выключенном компьютере; локальный - отвечает сама программа.
pub const OUT_OF_OFFICE_MODE_SERVER: &str = "server";
pub const OUT_OF_OFFICE_MODE_LOCAL: &str = "local";

/// Окно молчания: семь суток одному ключу адресата (S-048).
pub const SILENCE_WINDOW_DAYS: i64 = 7;
/// Срок памяти о записях ответов (S-050).
pub const REPLY_MEMORY_DAYS: i64 = 60;
/// Записи ответов удаляются пачками не более 500 строк.
pub const REPLY_PURGE_BATCH: i64 = 500;
/// Письмо старше суток остаётся без ответа: иначе программа разослала бы
/// запоздалые ответы после долгого перерыва (S-035).
pub const MAX_MESSAGE_AGE_HOURS: i64 = 24;
/// Границы перечня внутренних доменов (S-025, S-026).
pub const MAX_INTERNAL_DOMAINS: usize = 20;
/// Границы длины текста отсутствия (S-023).
pub const MIN_TEXT_CHARS: usize = 1;
pub const MAX_TEXT_CHARS: usize = 10000;
/// Границы периода отсутствия (S-021).
pub const MIN_PERIOD_MINUTES: i64 = 1;
pub const MAX_PERIOD_DAYS: i64 = 366;
/// Тема ответа на письмо без темы (S-060).
pub const EMPTY_SUBJECT_REPLY: &str = "Автоответ";

/// Локальные части служебных адресов: ответ на них бессмыслен (S-044).
pub const SERVICE_LOCAL_PARTS: [&str; 4] = ["mailer-daemon", "postmaster", "no-reply", "noreply"];

/// Настройка отсутствия одного ящика.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutOfOfficeSettings {
    pub account_id: i64,
    pub account_email: String,
    pub mode: String,
    pub enabled: bool,
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
    pub internal_text: String,
    pub external_text: String,
    pub internal_domains: Vec<String>,
    pub version: i64,
    /// Для серверного режима - время последнего подтверждённого чтения с
    /// сервера: показывается всегда подтверждённое сервером состояние (S-018).
    pub server_checked_at: Option<String>,
    pub last_error: Option<String>,
    /// Сколько писем периода отсутствия программа пропустила по возрасту: они
    /// пришли, пока программа не работала, и отвечать на них уже поздно
    /// (S-064, S-065).
    #[serde(default)]
    pub skipped_old: i64,
    /// Заголовки правил молчания доступны не во всех серверных модулях, и об
    /// этом сказано прямо в разделе автоответа ящика (S-039).
    #[serde(default)]
    pub silence_headers_available: bool,
    /// Вне сборки Windows ящик Exchange не синхронизируется вовсе, поэтому
    /// автоответ для него недоступен (S-004).
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub unavailable_reason: Option<String>,
}

/// Состав настройки, приходящий из интерфейса.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OutOfOfficeInput {
    pub account_id: i64,
    pub enabled: bool,
    pub starts_at: String,
    pub ends_at: String,
    pub internal_text: String,
    pub external_text: String,
    #[serde(default)]
    pub internal_domains: Vec<String>,
}

/// Причина молчания: её называет раздел автоответа и проверки стадии.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SilenceReason {
    ClosedByStage,
    NotInbox,
    Backfilled,
    OutsidePeriod,
    TooOld,
    UnparsedSender,
    MultipleReplyTo,
    Newsletter,
    AutoSubmitted,
    BulkPrecedence,
    EmptyReturnPath,
    SuppressedBySender,
    ServiceAddress,
    OwnAddress,
    NotAddressedToMailbox,
    UnrecognizedHeader,
}

/// Письмо в том виде, в каком его видит стадия автоответа.
#[derive(Debug, Clone, Default)]
pub struct ReplyCandidate {
    pub folder_role: Option<String>,
    pub backfilled: bool,
    pub closed_by_stage: Option<String>,
    pub received_at: Option<chrono::DateTime<chrono::Utc>>,
    pub from: Option<String>,
    pub reply_to: Vec<Addr>,
    pub to: Vec<Addr>,
    pub cc: Vec<Addr>,
    pub is_newsletter: bool,
    pub auto_submitted: Option<String>,
    pub precedence: Option<String>,
    pub return_path_empty: bool,
    pub auto_response_suppress: Option<String>,
    /// Заголовки правил молчания у этого письма читались. Если нет, правила по
    /// заголовкам не применяются вовсе (S-039).
    pub silence_headers_known: bool,
}

/// Адрес назначения ответа: единственный адрес из `Reply-To`, а при его
/// отсутствии адрес отправителя. Проверка правил молчания и отправка пользуются
/// одним и тем же ключом (S-036).
pub fn reply_destination(candidate: &ReplyCandidate) -> Option<String> {
    if candidate.reply_to.len() > 1 {
        return None;
    }
    if let Some(first) = candidate.reply_to.first()
        && !first.email.trim().is_empty()
    {
        return Some(first.email.trim().to_owned());
    }
    candidate
        .from
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// Ключ адресата: тот же ключ адреса, что и у списков отправителей и у истории
/// получателей (S-036).
pub fn recipient_key(address: &str) -> String {
    super::canonical_sender_address(address).to_lowercase()
}

/// Внутренний отправитель: домен равен внутреннему домену или оканчивается на
/// точку и этот домен. Сравнение окончанием строки без точки молча захватило бы
/// независимый домен `notexample.com` записью `example.com` (S-028).
pub fn is_internal_address(address: &str, domains: &[String]) -> bool {
    let key = recipient_key(address);
    let Some(domain) = super::address_domain(&key) else {
        return false;
    };
    domains
        .iter()
        .any(|entry| super::domain_matches(domain, entry))
}

/// Канонический внутренний домен (S-027).
pub fn normalize_internal_domain(value: &str) -> Result<String, String> {
    let domain = super::canonical_domain(value);
    if domain.is_empty() {
        return Err("домен не указан".into());
    }
    if !domain.contains('.') {
        return Err(format!("домен {domain} не содержит точки"));
    }
    Ok(domain)
}

/// Проверить состав локальной настройки до записи (S-021, S-023, S-026).
pub fn validate_local_input(input: &OutOfOfficeInput) -> Result<Vec<String>, String> {
    if !input.enabled {
        return Ok(Vec::new());
    }
    let start = chrono::DateTime::parse_from_rfc3339(input.starts_at.trim())
        .map_err(|_| "начало периода задано не датой".to_owned())?;
    let end = chrono::DateTime::parse_from_rfc3339(input.ends_at.trim())
        .map_err(|_| "окончание периода задано не датой".to_owned())?;
    let minutes = (end - start).num_minutes();
    if minutes < MIN_PERIOD_MINUTES {
        return Err(format!(
            "окончание периода должно быть позже начала не менее чем на {MIN_PERIOD_MINUTES} минуту"
        ));
    }
    if (end - start).num_days() > MAX_PERIOD_DAYS {
        return Err(format!(
            "период отсутствия не длиннее {MAX_PERIOD_DAYS} суток"
        ));
    }
    for text in [&input.internal_text, &input.external_text] {
        let length = text.trim().chars().count();
        if !(MIN_TEXT_CHARS..=MAX_TEXT_CHARS).contains(&length) {
            return Err(format!(
                "текст автоответа задаётся длиной от {MIN_TEXT_CHARS} до {MAX_TEXT_CHARS} символов"
            ));
        }
    }
    if input.internal_domains.is_empty() {
        return Err("нужен хотя бы один внутренний домен".into());
    }
    if input.internal_domains.len() > MAX_INTERNAL_DOMAINS {
        return Err(format!(
            "внутренних доменов не больше {MAX_INTERNAL_DOMAINS}"
        ));
    }
    let mut domains = Vec::new();
    for value in &input.internal_domains {
        let domain = normalize_internal_domain(value)?;
        if !domains.contains(&domain) {
            domains.push(domain);
        }
    }
    Ok(domains)
}

/// Значение заголовка `Auto-Submitted` разбирается в ключевое слово: значение
/// `no` ответу не мешает, любое другое - мешает, а неразборное значение
/// вызывает молчание, потому что молчание безопаснее ответа рассылке (S-040,
/// S-049).
fn auto_submitted_silences(value: &str) -> Option<SilenceReason> {
    let cleaned = value.trim().to_ascii_lowercase();
    if cleaned.is_empty() {
        return Some(SilenceReason::UnrecognizedHeader);
    }
    // Значение стандарта состоит из ключевого слова и необязательных
    // параметров через точку с запятой.
    let keyword = cleaned
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned();
    match keyword.as_str() {
        "no" => None,
        "auto-replied" | "auto-generated" | "auto-notified" => Some(SilenceReason::AutoSubmitted),
        _ => Some(SilenceReason::UnrecognizedHeader),
    }
}

/// Решение правил молчания. `None` означает кандидата на ответ (S-031 - S-049).
pub fn silence_reason(
    candidate: &ReplyCandidate,
    account_email: &str,
    own_addresses: &[String],
    period_start: Option<chrono::DateTime<chrono::Utc>>,
    period_end: Option<chrono::DateTime<chrono::Utc>>,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<SilenceReason> {
    if candidate.closed_by_stage.is_some() {
        return Some(SilenceReason::ClosedByStage);
    }
    if candidate.folder_role.as_deref() != Some("inbox") {
        return Some(SilenceReason::NotInbox);
    }
    if candidate.backfilled {
        return Some(SilenceReason::Backfilled);
    }
    // Без разобранного времени получения период проверить нечем, а молчание
    // безопаснее ответа: неизвестная дата не должна открывать дорогу ответу
    // письму, пришедшему вне периода отсутствия.
    let Some(received) = candidate.received_at else {
        return Some(SilenceReason::OutsidePeriod);
    };
    // S-022, S-034: период - от начала включительно до окончания не включая.
    if period_start.is_some_and(|start| received < start)
        || period_end.is_some_and(|end| received >= end)
    {
        return Some(SilenceReason::OutsidePeriod);
    }
    if (now - received).num_hours() > MAX_MESSAGE_AGE_HOURS {
        return Some(SilenceReason::TooOld);
    }
    // S-047: у письма без ровно одного разобранного адреса отправителя сверять
    // нечего.
    let Some(from) = candidate
        .from
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.contains('@'))
    else {
        return Some(SilenceReason::UnparsedSender);
    };
    // S-037: несколько адресов для ответа раскрыли бы отсутствие тем, кто
    // письма не писал.
    if candidate.reply_to.len() > 1 {
        return Some(SilenceReason::MultipleReplyTo);
    }
    let Some(destination) = reply_destination(candidate) else {
        return Some(SilenceReason::UnparsedSender);
    };
    if candidate.silence_headers_known {
        if candidate.is_newsletter {
            return Some(SilenceReason::Newsletter);
        }
        if let Some(value) = candidate.auto_submitted.as_deref()
            && let Some(reason) = auto_submitted_silences(value)
        {
            return Some(reason);
        }
        if let Some(value) = candidate.precedence.as_deref() {
            // S-049: значение вне перечня нераспознанным не считается и
            // молчания не вызывает.
            let cleaned = value.trim().to_ascii_lowercase();
            if matches!(cleaned.as_str(), "bulk" | "list" | "junk") {
                return Some(SilenceReason::BulkPrecedence);
            }
        }
        if candidate.return_path_empty {
            return Some(SilenceReason::EmptyReturnPath);
        }
        if let Some(value) = candidate.auto_response_suppress.as_deref() {
            let cleaned = value.trim().to_ascii_lowercase();
            if cleaned.is_empty() {
                return Some(SilenceReason::UnrecognizedHeader);
            }
            if cleaned.contains("all") || cleaned.contains("oof") {
                return Some(SilenceReason::SuppressedBySender);
            }
        }
    }
    for address in [from, destination.as_str()] {
        let key = recipient_key(address);
        let local = key.split('@').next().unwrap_or_default();
        if SERVICE_LOCAL_PARTS.contains(&local) {
            return Some(SilenceReason::ServiceAddress);
        }
        // S-045: собственный адрес любого подключённого ящика ответа не
        // получает - иначе два ящика пользователя отвечали бы друг другу.
        if own_addresses.contains(&key) {
            return Some(SilenceReason::OwnAddress);
        }
    }
    // S-046: письмо, в котором адреса ящика нет ни в поле "Кому", ни в поле
    // "Копия", получено по скрытой копии или через рассылку.
    let mailbox_key = recipient_key(account_email);
    let addressed = candidate
        .to
        .iter()
        .chain(&candidate.cc)
        .any(|addr| recipient_key(&addr.email) == mailbox_key);
    if !addressed {
        return Some(SilenceReason::NotAddressedToMailbox);
    }
    None
}

/// Тема ответа: `Re: ` добавляется один раз (S-059, S-060).
pub fn reply_subject(original: &str) -> String {
    let trimmed = original.trim();
    if trimmed.is_empty() {
        return EMPTY_SUBJECT_REPLY.to_owned();
    }
    // Срез по байтам здесь не годится: тема начинается с многобайтных букв
    // чаще, чем с ASCII, и рубил бы символ пополам.
    let head: String = trimmed.chars().take(3).collect();
    if head.eq_ignore_ascii_case("re:") {
        return trimmed.to_owned();
    }
    format!("Re: {trimmed}")
}

/// Заголовки автоответа: признак автоматического ответа, запрет ответов на него
/// и связь с исходным письмом (S-056 - S-058).
pub fn reply_headers(source_message_id: Option<&str>) -> Vec<(String, String)> {
    let mut headers = vec![
        ("Auto-Submitted".to_owned(), "auto-replied".to_owned()),
        ("X-Auto-Response-Suppress".to_owned(), "All".to_owned()),
    ];
    if let Some(value) = source_message_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        headers.push(("In-Reply-To".to_owned(), value.to_owned()));
        headers.push(("References".to_owned(), value.to_owned()));
    }
    headers
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339("2026-09-18T12:00:00+00:00")
            .expect("время")
            .with_timezone(&chrono::Utc)
    }

    fn inbox_letter() -> ReplyCandidate {
        ReplyCandidate {
            folder_role: Some("inbox".into()),
            received_at: Some(now() - chrono::Duration::minutes(5)),
            from: Some("boss@partner.test".into()),
            to: vec![Addr::new("me@example.test")],
            silence_headers_known: true,
            ..Default::default()
        }
    }

    fn decide(candidate: &ReplyCandidate) -> Option<SilenceReason> {
        silence_reason(
            candidate,
            "me@example.test",
            &[recipient_key("me@example.test")],
            Some(now() - chrono::Duration::days(1)),
            Some(now() + chrono::Duration::days(1)),
            now(),
        )
    }

    /// S-040 - S-049: письмо рассылки, чужого автоответа, отчёта о недоставке и
    /// письма с нераспознанным заголовком остаётся без ответа. Ответ на такое
    /// письмо запускает бесконечную переписку двух программ.
    #[test]
    fn automatic_mail_never_gets_an_answer() {
        assert_eq!(decide(&inbox_letter()), None, "обычное письмо - кандидат");
        let cases: Vec<(ReplyCandidate, SilenceReason)> = vec![
            (
                ReplyCandidate {
                    is_newsletter: true,
                    ..inbox_letter()
                },
                SilenceReason::Newsletter,
            ),
            (
                ReplyCandidate {
                    auto_submitted: Some("auto-generated".into()),
                    ..inbox_letter()
                },
                SilenceReason::AutoSubmitted,
            ),
            (
                ReplyCandidate {
                    auto_submitted: Some("совершенно непонятно".into()),
                    ..inbox_letter()
                },
                SilenceReason::UnrecognizedHeader,
            ),
            (
                ReplyCandidate {
                    precedence: Some("Bulk".into()),
                    ..inbox_letter()
                },
                SilenceReason::BulkPrecedence,
            ),
            (
                ReplyCandidate {
                    return_path_empty: true,
                    ..inbox_letter()
                },
                SilenceReason::EmptyReturnPath,
            ),
            (
                ReplyCandidate {
                    auto_response_suppress: Some("DR, OOF, AutoReply".into()),
                    ..inbox_letter()
                },
                SilenceReason::SuppressedBySender,
            ),
            (
                ReplyCandidate {
                    from: Some("MAILER-DAEMON@partner.test".into()),
                    ..inbox_letter()
                },
                SilenceReason::ServiceAddress,
            ),
            (
                ReplyCandidate {
                    to: vec![Addr::new("team@partner.test")],
                    ..inbox_letter()
                },
                SilenceReason::NotAddressedToMailbox,
            ),
            (
                ReplyCandidate {
                    reply_to: vec![Addr::new("one@partner.test"), Addr::new("two@partner.test")],
                    ..inbox_letter()
                },
                SilenceReason::MultipleReplyTo,
            ),
        ];
        for (candidate, expected) in cases {
            assert_eq!(decide(&candidate), Some(expected), "{expected:?}");
        }
        // S-041: значение Precedence вне перечня нераспознанным не считается.
        assert_eq!(
            decide(&ReplyCandidate {
                precedence: Some("first-class".into()),
                ..inbox_letter()
            }),
            None
        );
        // S-039: у письма, чьи заголовки серверный модуль не отдаёт, правила по
        // заголовкам не применяются, а остальные продолжают работать.
        assert_eq!(
            decide(&ReplyCandidate {
                silence_headers_known: false,
                is_newsletter: true,
                auto_submitted: Some("auto-replied".into()),
                ..inbox_letter()
            }),
            None
        );
    }

    /// S-022, S-034, S-035: граница окончания периода не включается, а письмо
    /// старше суток остаётся без ответа и внутри периода.
    #[test]
    fn period_and_age_are_checked_on_their_boundaries() {
        let start = now() - chrono::Duration::days(2);
        let end = now();
        let at_end = ReplyCandidate {
            received_at: Some(end),
            ..inbox_letter()
        };
        assert_eq!(
            silence_reason(
                &at_end,
                "me@example.test",
                &[],
                Some(start),
                Some(end),
                now()
            ),
            Some(SilenceReason::OutsidePeriod)
        );
        let old = ReplyCandidate {
            received_at: Some(now() - chrono::Duration::hours(25)),
            ..inbox_letter()
        };
        assert_eq!(
            silence_reason(
                &old,
                "me@example.test",
                &[],
                Some(start),
                Some(now() + chrono::Duration::days(1)),
                now()
            ),
            Some(SilenceReason::TooOld)
        );
    }

    /// S-028: внутренним считается только домен целиком или его поддомен.
    /// Сравнение окончанием строки захватило бы независимый домен.
    #[test]
    fn internal_domain_never_swallows_a_similar_one() {
        let domains = vec!["example.test".to_owned()];
        assert!(is_internal_address("user@example.test", &domains));
        assert!(is_internal_address("user@sub.example.test", &domains));
        assert!(!is_internal_address("user@notexample.test", &domains));
    }

    /// S-059, S-060: тема ответа получает `Re: ` один раз, а пустая исходная
    /// тема заменяется словом автоответа.
    #[test]
    fn reply_subject_is_prefixed_once() {
        assert_eq!(reply_subject("Отчёт"), "Re: Отчёт");
        assert_eq!(reply_subject("re: Отчёт"), "re: Отчёт");
        assert_eq!(reply_subject("   "), EMPTY_SUBJECT_REPLY);
    }
}
