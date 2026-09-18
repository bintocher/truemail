//! Списки заблокированных и доверенных отправителей: нормализация значений,
//! правила совпадения и типы записей (specs/blocked-senders.md).
//!
//! Нормализация живёт здесь, а не в хранилище: её же применяет автоочистка по
//! отправителю (specs/sweep-by-sender.md, S-046), и двух разных пониманий
//! одного адреса у них быть не должно.

use serde::{Deserialize, Serialize};

/// Название стадии списков отправителей в столбце результата стадии письма
/// (specs/mail-rules-conditions-and-actions.md, S-009).
pub const SENDER_POLICY_STAGE_NAME: &str = "sender_policy";

/// Пределы длины заданы почтовыми стандартами (S-011).
pub const MAX_ADDRESS_BYTES: usize = 254;
pub const MAX_DOMAIN_BYTES: usize = 253;
pub const MAX_DOMAIN_LABEL_BYTES: usize = 63;

/// Больше двадцати уровней домена не разбирается: это вдвое больше самой
/// длинной осмысленной цепочки поддоменов и держит проверку письма в пределах
/// двух десятков поисков по индексу.
pub const MAX_DOMAIN_LEVELS: usize = 20;

/// Вид записи списка: адрес целиком или домен (S-006).
pub const POLICY_KIND_ADDRESS: &str = "address";
pub const POLICY_KIND_DOMAIN: &str = "domain";

/// Решение записи: заблокирован или доверенный.
pub const POLICY_DECISION_BLOCKED: &str = "blocked";
pub const POLICY_DECISION_TRUSTED: &str = "trusted";

pub fn is_policy_kind(kind: &str) -> bool {
    matches!(kind, POLICY_KIND_ADDRESS | POLICY_KIND_DOMAIN)
}

pub fn is_policy_decision(decision: &str) -> bool {
    matches!(decision, POLICY_DECISION_BLOCKED | POLICY_DECISION_TRUSTED)
}

/// Запись списка отправителей в том виде, в каком её читает интерфейс.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SenderPolicy {
    pub id: i64,
    pub kind: String,
    pub value: String,
    pub decision: String,
    pub created_at: String,
    pub updated_at: String,
    /// Сколько писем уже убрано уборкой этой записи (S-045).
    #[sqlx(default)]
    pub swept: i64,
    /// Состояние незавершённой уборки, если она есть (S-045, S-048).
    #[sqlx(default)]
    pub job_state: Option<String>,
    /// Причина отказа последней уборки или стадии (S-003, S-048).
    #[sqlx(default)]
    pub last_error: Option<String>,
    /// Число писем, по которым операция дошла до отказа (S-048).
    #[sqlx(default)]
    pub failed: i64,
}

/// Предварительный просмотр блокировки: канонический вид значения, ключ
/// снимка и число уже полученных писем по каждому ящику (S-029, S-031).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SenderPolicyPreview {
    pub kind: String,
    pub value: String,
    /// Собственный адрес блокировать нельзя (S-019).
    pub own_address: bool,
    /// Домену принадлежит собственный адрес: нужно отдельное подтверждение
    /// (S-020).
    pub own_domain: bool,
    pub snapshot_key: String,
    pub total: i64,
    pub per_account: Vec<SenderPolicyAccountCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SenderPolicyAccountCount {
    pub account_id: i64,
    pub email: String,
    pub count: i64,
}

/// Отчёт о проходе уборки уже полученных писем (S-036, S-042).
#[derive(Debug, Clone, Default, Serialize, Deserialize, sqlx::FromRow)]
pub struct SenderPolicySweepReport {
    pub id: i64,
    pub state: String,
    pub queued: i64,
    pub skipped: i64,
    pub failed: i64,
    pub remaining: i64,
}

/// Отчёт о снятии блокировки (S-040 - S-042).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SenderPolicyReleaseReport {
    /// Отменённые операции в состояниях pending и retry (S-041).
    pub cancelled: i64,
    /// Письма, перемещение которых уже выполняется или выполнено (S-042).
    pub irreversible: i64,
    /// Уже убранные письма из корзины не возвращаются (S-040).
    pub kept_in_trash: i64,
}

/// Канонический домен записи: без ведущего знака "@", внешних пробелов и
/// завершающей точки, в нижнем регистре и в общей форме международного домена
/// (S-009 - S-011). Общую форму даёт разбор узла уже объявленной зависимостью
/// `url`: новой прямой зависимости ради этого не вводится (S-010).
pub fn normalize_policy_domain(value: &str) -> Result<String, String> {
    let cleaned = value
        .trim()
        .trim_start_matches('@')
        .trim()
        .trim_end_matches('.')
        .to_lowercase();
    if cleaned.is_empty() {
        return Err("домен не указан".into());
    }
    if cleaned.starts_with('[') || cleaned.ends_with(']') {
        return Err("адресный литерал в квадратных скобках не поддерживается".into());
    }
    if cleaned.contains(char::is_whitespace) || cleaned.contains('@') || cleaned.contains('/') {
        return Err("домен записывается одним словом без пробелов".into());
    }
    let host = url::Url::parse(&format!("https://{cleaned}/"))
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .ok_or_else(|| format!("домен {cleaned} не разбирается"))?;
    // Разбор узла вернул адрес в квадратных скобках: это адресный литерал,
    // а не доменное имя (S-008).
    if host.starts_with('[') || host.parse::<std::net::IpAddr>().is_ok() {
        return Err("адресный литерал в квадратных скобках не поддерживается".into());
    }
    if host.len() > MAX_DOMAIN_BYTES {
        return Err(format!("домен длиннее {MAX_DOMAIN_BYTES} байт"));
    }
    let labels: Vec<&str> = host.split('.').collect();
    // S-013: одна метка без точки доменом записи не считается - такая запись
    // накрыла бы целую доменную зону.
    if labels.len() < 2 {
        return Err("в домене нужна хотя бы одна точка".into());
    }
    for label in &labels {
        if label.is_empty() {
            return Err("в домене есть пустая метка".into());
        }
        if label.len() > MAX_DOMAIN_LABEL_BYTES {
            return Err(format!(
                "метка домена длиннее {MAX_DOMAIN_LABEL_BYTES} байт"
            ));
        }
    }
    Ok(host)
}

/// Канонический адрес записи: пробелы обрезаны, написание локальной части
/// сохранено, домен канонизирован (S-007 - S-011). Регистр локальной части
/// сохраняется намеренно: стандарт оставляет его на усмотрение сервера, а без
/// учёта регистра выполняется только сравнение.
pub fn normalize_policy_address(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("адрес не указан".into());
    }
    // S-007: из записи вида "Имя <адрес>" берётся сам адрес, но только если
    // угловые скобки в ней одни.
    let inner = match (trimmed.find('<'), trimmed.rfind('>')) {
        (Some(open), Some(close)) if close > open => {
            if trimmed[open + 1..close].contains('<') || trimmed[open + 1..close].contains('>') {
                return Err("в значении несколько адресов".into());
            }
            trimmed[open + 1..close].trim()
        }
        (None, None) => trimmed,
        _ => return Err("адрес записан неверно".into()),
    };
    if inner.is_empty() {
        return Err("адрес не указан".into());
    }
    // S-013: несколько адресов в одном значении не принимаются.
    if inner.contains(',') || inner.contains(';') || inner.contains(char::is_whitespace) {
        return Err("в значении несколько адресов".into());
    }
    if inner.len() > MAX_ADDRESS_BYTES {
        return Err(format!("адрес длиннее {MAX_ADDRESS_BYTES} байт"));
    }
    // S-008: ровно один знак "@" вне кавычек. Кавычки в локальной части
    // допускаются стандартом, но знак "@" внутри них адреса не разделяет.
    let mut quoted = false;
    let mut at_positions = Vec::new();
    for (index, symbol) in inner.char_indices() {
        match symbol {
            '"' => quoted = !quoted,
            '@' if !quoted => at_positions.push(index),
            _ => {}
        }
    }
    if quoted {
        return Err("в адресе незакрытая кавычка".into());
    }
    if at_positions.len() != 1 {
        return Err("в адресе нужен ровно один знак @".into());
    }
    let at = at_positions[0];
    let local = &inner[..at];
    let domain = &inner[at + 1..];
    if local.is_empty() {
        return Err("в адресе пустая локальная часть".into());
    }
    if local.chars().any(|symbol| symbol.is_control()) {
        return Err("в локальной части адреса есть управляющий символ".into());
    }
    let domain = normalize_policy_domain(domain)?;
    Ok(format!("{local}@{domain}"))
}

/// Нормализовать значение записи по её виду (S-013).
pub fn normalize_policy_value(kind: &str, value: &str) -> Result<String, String> {
    match kind {
        POLICY_KIND_ADDRESS => normalize_policy_address(value),
        POLICY_KIND_DOMAIN => normalize_policy_domain(value),
        _ => Err(format!("вид записи {kind} не поддерживается")),
    }
}

/// Домен канонического адреса.
pub fn address_domain(address: &str) -> Option<&str> {
    address.rsplit_once('@').map(|(_, domain)| domain)
}

/// Домен отправителя и все его родительские домены: по ним идёт поиск записи
/// домена. Совпадением считается равенство или окончание на точку и значение
/// записи (S-012), поэтому перебор родителей даёт тот же ответ, что и поиск
/// по суффиксу, но пользуется индексом.
pub fn domain_lookup_chain(domain: &str) -> Vec<String> {
    let mut chain = Vec::new();
    let mut current = domain;
    while !current.is_empty() && chain.len() < MAX_DOMAIN_LEVELS {
        chain.push(current.to_owned());
        match current.split_once('.') {
            Some((_, rest)) if !rest.is_empty() => current = rest,
            _ => break,
        }
    }
    chain
}

/// Совпадение домена отправителя с записью домена (S-012): равенство значений
/// либо окончание домена отправителя на точку и значение записи. Значение
/// "mail.ru" не совпадает с доменом "notmail.ru".
pub fn domain_matches(sender_domain: &str, entry: &str) -> bool {
    if sender_domain == entry {
        return true;
    }
    sender_domain
        .strip_suffix(entry)
        .is_some_and(|head| head.ends_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Нормализация значения записи: написание локальной части, канонизация
    /// домена, общая форма международного домена и все отказы грамматики и
    /// пределов длины разом (S-007 - S-013). Эти границы задают почтовые
    /// стандарты, и ошибка в любой из них пустила бы в список значение, по
    /// которому письма отбираются неверно.
    #[test]
    fn value_normalization_and_limits() {
        assert_eq!(
            normalize_policy_address("  Ivan.Petrov@Example.COM. ").expect("адрес"),
            "Ivan.Petrov@example.com"
        );
        assert_eq!(
            normalize_policy_address("Имя <boss@Example.test>").expect("адрес"),
            "boss@example.test"
        );
        assert_eq!(
            normalize_policy_address("почта@пример.рф").expect("адрес"),
            "почта@xn--e1afmkfd.xn--p1ai"
        );
        // Знак "@" внутри кавычек адрес не разделяет.
        assert_eq!(
            normalize_policy_address("\"odd@name\"@example.test").expect("адрес"),
            "\"odd@name\"@example.test"
        );
        for broken in [
            "a@b@example.test",
            "@example.test",
            "user@[192.168.0.1]",
            "one@example.test, two@example.test",
            "user@localhost",
            "\"open@example.test",
            "",
        ] {
            assert!(
                normalize_policy_address(broken).is_err(),
                "значение {broken} должно быть отклонено"
            );
        }
        let long_local = "a".repeat(250);
        assert!(normalize_policy_address(&format!("{long_local}@example.test")).is_err());
        assert!(normalize_policy_domain(&format!("{}.test", "b".repeat(64))).is_err());
        // Одна метка без точки записью домена не становится.
        assert!(normalize_policy_domain("test").is_err());
    }

    /// Домен совпадает целиком или по границе точки, а цепочка родителей не
    /// уходит глубже предела (S-012). Совпадение по простому окончанию строки
    /// увело бы в корзину почту чужого домена.
    #[test]
    fn domain_matching_respects_dot_boundary() {
        assert!(domain_matches("mail.ru", "mail.ru"));
        assert!(domain_matches("news.mail.ru", "mail.ru"));
        assert!(!domain_matches("notmail.ru", "mail.ru"));
        assert!(!domain_matches("mail.ru.evil.test", "mail.ru"));
        assert_eq!(
            domain_lookup_chain("a.b.example.test"),
            vec![
                "a.b.example.test".to_owned(),
                "b.example.test".to_owned(),
                "example.test".to_owned(),
                "test".to_owned(),
            ]
        );
        let deep = (0..40)
            .map(|i| format!("l{i}"))
            .collect::<Vec<_>>()
            .join(".");
        assert_eq!(domain_lookup_chain(&deep).len(), MAX_DOMAIN_LEVELS);
    }
}
