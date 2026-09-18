//! Правила обработки почты: группы условий, группы исключений и цепочка
//! действий. Словарь полей, операторов и действий живёт здесь и словарь умных
//! папок не переиспользует: добавленный там оператор молча изменил бы смысл
//! почтового правила (S-020 - S-022).

use serde::{Deserialize, Serialize};

/// Предельные размеры правила. Числа продуктовые: они держат проверку одного
/// письма в пределах сотни сравнений (S-028 - S-030).
pub const MAX_RULE_GROUPS: usize = 10;
pub const MAX_GROUP_CONDITIONS: usize = 10;
pub const MAX_RULE_ACTIONS: usize = 10;

/// Название стадии правил в столбце результата стадии письма (S-009).
pub const RULES_STAGE_NAME: &str = "rules";

/// Вид поля условия определяет набор операторов и разбор значения.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleFieldKind {
    Text,
    Enum,
    Date,
    Size,
}

/// Словарь полей условий (S-020). Идентификатор `sender` сохраняет прежний
/// смысл склейки имени и адреса, а точный адрес получает собственный
/// `sender_address` и в словарь умных папок не попадает (S-021).
pub const RULE_FIELDS: &[(&str, RuleFieldKind)] = &[
    ("sender", RuleFieldKind::Text),
    ("sender_address", RuleFieldKind::Text),
    ("recipient", RuleFieldKind::Text),
    ("subject", RuleFieldKind::Text),
    ("body", RuleFieldKind::Text),
    ("account", RuleFieldKind::Text),
    ("folder", RuleFieldKind::Text),
    ("folder_role", RuleFieldKind::Enum),
    ("read_state", RuleFieldKind::Enum),
    ("importance", RuleFieldKind::Enum),
    ("reply_state", RuleFieldKind::Enum),
    ("draft_state", RuleFieldKind::Enum),
    ("attachment", RuleFieldKind::Enum),
    ("size", RuleFieldKind::Size),
    ("label", RuleFieldKind::Text),
    ("date", RuleFieldKind::Date),
];

/// Допустимые значения перечислимых полей. Роли папок ограничены рабочими:
/// автоматический прогон не берёт письма из служебных папок, и условие по
/// `sent` или `trash` не сработало бы никогда (S-027).
pub fn rule_enum_values(field: &str) -> &'static [&'static str] {
    match field {
        "folder_role" => &["inbox", "archive", "other"],
        "read_state" => &["read", "unread"],
        "importance" => &["flagged", "normal"],
        "reply_state" => &["answered", "unanswered"],
        "draft_state" => &["draft", "not_draft"],
        "attachment" => &["has", "none"],
        _ => &[],
    }
}

pub fn rule_field_kind(field: &str) -> Option<RuleFieldKind> {
    RULE_FIELDS
        .iter()
        .find(|(id, _)| *id == field)
        .map(|(_, kind)| *kind)
}

/// Операторы, допустимые для вида поля (S-022).
pub fn rule_operators(kind: RuleFieldKind) -> &'static [&'static str] {
    match kind {
        RuleFieldKind::Text => &[
            "contains",
            "not_contains",
            "equals",
            "not_equals",
            "starts_with",
            "ends_with",
        ],
        RuleFieldKind::Enum => &["equals", "not_equals"],
        RuleFieldKind::Date => &["within_last", "older_than", "before", "after", "on"],
        RuleFieldKind::Size => &[
            "greater_than",
            "greater_or_equal",
            "less_than",
            "less_or_equal",
            "equals",
            "between",
        ],
    }
}

/// Единицы относительной даты и размера: те же, что понимает прогон.
pub const RULE_DATE_UNITS: &[&str] = &["minutes", "hours", "days", "weeks"];
pub const RULE_SIZE_UNITS: &[&str] = &["kb", "mb", "gb"];

/// Набор действий правила (S-038). Пересылки по адресу здесь нет намеренно:
/// она вынесена в отдельную задачу.
pub const RULE_ACTIONS: &[&str] = &[
    "move",
    "archive",
    "spam",
    "trash",
    "delete",
    "label_add",
    "label_remove",
    "mark_read",
    "mark_flagged",
    "stop",
];

/// Уводящее действие забирает письмо из папки, поэтому выполняется через
/// очередь операций и допускается по письму только один раз.
pub fn is_takeaway_action(kind: &str) -> bool {
    matches!(kind, "move" | "archive" | "spam" | "trash" | "delete")
}

/// Местное действие меняет локальную базу без ожидания сервера (S-041).
pub fn is_local_action(kind: &str) -> bool {
    matches!(
        kind,
        "label_add" | "label_remove" | "mark_read" | "mark_flagged"
    )
}

/// Роль папки назначения уводящего действия. `delete` роли не имеет: оно
/// удаляет письмо без промежуточного переноса в корзину (S-047).
pub fn takeaway_target_role(kind: &str) -> Option<&'static str> {
    match kind {
        "archive" => Some("archive"),
        "spam" => Some("spam"),
        "trash" => Some("trash"),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailRuleCondition {
    pub field: String,
    pub op: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub value2: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailRuleGroup {
    #[serde(default = "all_logic")]
    pub logic: String,
    #[serde(default)]
    pub conditions: Vec<MailRuleCondition>,
}

fn all_logic() -> String {
    "all".to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailRuleAction {
    pub kind: String,
    #[serde(default)]
    pub folder_id: Option<i64>,
    #[serde(default)]
    pub folder_role: Option<String>,
    #[serde(default)]
    pub label_id: Option<i64>,
}

/// Правило целиком: то, что читает интерфейс и чем пользуется прогон.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailRule {
    pub id: String,
    pub name: String,
    pub account_id: Option<i64>,
    pub enabled: bool,
    pub progress_message_id: i64,
    pub sort_order: i64,
    pub version: i64,
    pub groups: Vec<MailRuleGroup>,
    pub exceptions: Vec<MailRuleGroup>,
    pub actions: Vec<MailRuleAction>,
    /// `ok` или `needs_attention`: правило не выполняется, пока его папка или
    /// метка не выбрана заново (S-054).
    pub state: String,
    pub attention_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailRuleInput {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub account_id: Option<i64>,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    #[serde(default)]
    pub groups: Vec<MailRuleGroup>,
    #[serde(default)]
    pub exceptions: Vec<MailRuleGroup>,
    #[serde(default)]
    pub actions: Vec<MailRuleAction>,
    /// Одноразовый ключ подтверждения удаления навсегда (S-048).
    #[serde(default)]
    pub confirm_key: Option<String>,
}

fn enabled_by_default() -> bool {
    true
}

/// Отчёт ручного прогона (S-073).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MailRuleRunReport {
    pub run_id: i64,
    pub state: String,
    pub scanned: i64,
    pub applied: i64,
    pub queued: i64,
    pub skipped: i64,
    pub remaining: i64,
}

/// Канонический адрес отправителя: пробелы обрезаны, написание локальной части
/// сохранено, домен приведён к нижнему регистру и к общей форме международного
/// домена (S-023, `specs/blocked-senders.md`).
pub fn canonical_sender_address(value: &str) -> String {
    let trimmed = value.trim().trim_start_matches('@');
    let Some((local, domain)) = trimmed.rsplit_once('@') else {
        return trimmed.to_owned();
    };
    format!("{local}@{}", canonical_domain(domain))
}

/// Домен без завершающей точки, в нижнем регистре и в общей форме
/// международного домена. Общую форму даёт разбор адреса: он приводит
/// национальные буквы к тому же виду, в котором домен приходит в письме.
pub fn canonical_domain(domain: &str) -> String {
    let cleaned = domain
        .trim()
        .trim_start_matches('@')
        .trim_end_matches('.')
        .to_lowercase();
    if cleaned.is_empty() {
        return cleaned;
    }
    url::Url::parse(&format!("https://{cleaned}/"))
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or(cleaned)
}

/// Проверка состава правила перед записью. Отказ ничего не создаёт и не
/// изменяет: проверки выполняются до открытия транзакции.
pub fn validate_rule_input(rule: &MailRuleInput) -> Result<(), String> {
    if rule.id.trim().is_empty() || rule.name.trim().is_empty() {
        return Err("правилу нужны идентификатор и название".into());
    }
    if rule.groups.len() > MAX_RULE_GROUPS || rule.exceptions.len() > MAX_RULE_GROUPS {
        return Err(format!(
            "в правиле не больше {MAX_RULE_GROUPS} групп условий и {MAX_RULE_GROUPS} групп исключений"
        ));
    }
    // S-019: правило без единой обычной группы с условием применилось бы к
    // любому письму.
    if rule.groups.is_empty() || rule.groups.iter().all(|group| group.conditions.is_empty()) {
        return Err("в правиле нет ни одного условия".into());
    }
    for group in rule.groups.iter().chain(rule.exceptions.iter()) {
        validate_group(group)?;
    }
    validate_actions(&rule.actions)?;
    Ok(())
}

fn validate_group(group: &MailRuleGroup) -> Result<(), String> {
    if group.conditions.is_empty() {
        return Err("группа без условий не сохраняется".into());
    }
    if group.conditions.len() > MAX_GROUP_CONDITIONS {
        return Err(format!("в группе не больше {MAX_GROUP_CONDITIONS} условий"));
    }
    if !matches!(group.logic.as_str(), "all" | "any") {
        return Err("логика группы бывает только \"все\" или \"любое\"".into());
    }
    for condition in &group.conditions {
        validate_condition(condition)?;
    }
    Ok(())
}

fn validate_condition(condition: &MailRuleCondition) -> Result<(), String> {
    let Some(kind) = rule_field_kind(&condition.field) else {
        return Err(format!(
            "поле условия {} не поддерживается",
            condition.field
        ));
    };
    if !rule_operators(kind).contains(&condition.op.as_str()) {
        return Err(format!(
            "оператор {} не подходит полю {}",
            condition.op, condition.field
        ));
    }
    match kind {
        RuleFieldKind::Text => {
            // S-025: пустое значение текстового условия совпало бы со всем
            // подряд у операторов вхождения.
            if condition.value.trim().is_empty() {
                return Err("значение условия не может быть пустым".into());
            }
        }
        RuleFieldKind::Enum => {
            if !rule_enum_values(&condition.field).contains(&condition.value.as_str()) {
                return Err(format!(
                    "значение {} не подходит полю {}",
                    condition.value, condition.field
                ));
            }
        }
        RuleFieldKind::Date => {
            if matches!(condition.op.as_str(), "within_last" | "older_than") {
                let amount = condition.value.trim().parse::<i64>().unwrap_or(0);
                let unit = condition.unit.as_deref().unwrap_or("");
                if amount <= 0 || !RULE_DATE_UNITS.contains(&unit) {
                    return Err("у условия по дате нужны число и единица времени".into());
                }
            } else if chrono::NaiveDate::parse_from_str(condition.value.trim(), "%Y-%m-%d").is_err()
            {
                return Err("дата условия записывается как ГГГГ-ММ-ДД".into());
            }
        }
        RuleFieldKind::Size => {
            let unit = condition.unit.as_deref().unwrap_or("");
            // S-025: пустое поле размера прежде приводилось к нулю, и условие
            // "размер больше" совпадало с каждым письмом. Значение, которое не
            // является конечным неотрицательным числом, условием не считается.
            let amount = parse_size_amount(&condition.value);
            if amount.is_none() || !RULE_SIZE_UNITS.contains(&unit) {
                return Err("у условия по размеру нужны число и единица размера".into());
            }
            if condition.op == "between" {
                let maximum = condition.value2.as_deref().map(parse_size_amount);
                match (maximum, amount) {
                    (Some(Some(maximum)), Some(amount)) if maximum > amount => {}
                    _ => {
                        return Err("верхняя граница размера должна быть больше нижней".into());
                    }
                }
            }
        }
    }
    Ok(())
}

/// Размер условия: конечное неотрицательное число. Пустая строка, текст и
/// нечисловые значения вида "nan" условием не считаются.
fn parse_size_amount(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed
        .parse::<f64>()
        .ok()
        .filter(|amount| amount.is_finite() && *amount >= 0.0)
}

fn validate_actions(actions: &[MailRuleAction]) -> Result<(), String> {
    if actions.is_empty() {
        return Err("в правиле нет ни одного действия".into());
    }
    if actions.len() > MAX_RULE_ACTIONS {
        return Err(format!("в правиле не больше {MAX_RULE_ACTIONS} действий"));
    }
    for action in actions {
        if !RULE_ACTIONS.contains(&action.kind.as_str()) {
            return Err(format!("действие {} не поддерживается", action.kind));
        }
        if action.kind == "move" && action.folder_id.is_none() && action.folder_role.is_none() {
            return Err("у перемещения не выбрана папка назначения".into());
        }
        if matches!(action.kind.as_str(), "label_add" | "label_remove") && action.label_id.is_none()
        {
            return Err("у действия с меткой не выбрана метка".into());
        }
    }
    if actions
        .iter()
        .filter(|a| is_takeaway_action(&a.kind))
        .count()
        > 1
    {
        return Err("письмо можно увести только один раз: оставьте одно уводящее действие".into());
    }
    // S-035, S-040: после ухода письма выполнить осталось бы нечего, поэтому
    // хвост правила после уводящего действия допускает только остановку.
    if let Some(index) = actions.iter().position(|a| is_takeaway_action(&a.kind))
        && actions[index + 1..].iter().any(|a| a.kind != "stop")
    {
        return Err("после уводящего действия остальные действия не выполнятся".into());
    }
    if let Some(index) = actions.iter().position(|a| a.kind == "stop")
        && index + 1 != actions.len()
    {
        return Err("остановка обработки ставится последним действием".into());
    }
    Ok(())
}

/// Отпечаток правила для подтверждения удаления навсегда: в него входят
/// область, условия и состав действий, поэтому любое изменение правила делает
/// прежнее подтверждение недействительным (S-048, S-049).
pub fn delete_forever_fingerprint(rule: &MailRuleInput) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(rule.id.trim().as_bytes());
    hasher.update([0]);
    hasher.update(rule.name.trim().as_bytes());
    hasher.update([0]);
    hasher.update(
        rule.account_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "all".into())
            .as_bytes(),
    );
    for (marker, groups) in [(b'g', &rule.groups), (b'e', &rule.exceptions)] {
        for group in groups {
            hasher.update([0, marker]);
            hasher.update(group.logic.as_bytes());
            for condition in &group.conditions {
                hasher.update([0]);
                hasher.update(condition.field.as_bytes());
                hasher.update([1]);
                hasher.update(condition.op.as_bytes());
                hasher.update([1]);
                hasher.update(condition.value.as_bytes());
                hasher.update([1]);
                hasher.update(condition.unit.as_deref().unwrap_or("").as_bytes());
                hasher.update([1]);
                hasher.update(condition.value2.as_deref().unwrap_or("").as_bytes());
            }
        }
    }
    for action in &rule.actions {
        hasher.update([0, b'a']);
        hasher.update(action.kind.as_bytes());
        hasher.update([1]);
        hasher.update(action.folder_id.unwrap_or(0).to_le_bytes());
        hasher.update([1]);
        hasher.update(action.folder_role.as_deref().unwrap_or("").as_bytes());
        hasher.update([1]);
        hasher.update(action.label_id.unwrap_or(0).to_le_bytes());
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn condition(field: &str, op: &str, value: &str) -> MailRuleCondition {
        MailRuleCondition {
            field: field.into(),
            op: op.into(),
            value: value.into(),
            unit: None,
            value2: None,
        }
    }

    fn action(kind: &str) -> MailRuleAction {
        MailRuleAction {
            kind: kind.into(),
            folder_id: None,
            folder_role: None,
            label_id: None,
        }
    }

    fn rule(groups: Vec<MailRuleGroup>, actions: Vec<MailRuleAction>) -> MailRuleInput {
        MailRuleInput {
            id: "rule-1".into(),
            name: "Правило".into(),
            account_id: None,
            enabled: true,
            groups,
            exceptions: Vec::new(),
            actions,
            confirm_key: None,
        }
    }

    fn group(conditions: Vec<MailRuleCondition>) -> MailRuleGroup {
        MailRuleGroup {
            logic: "all".into(),
            conditions,
        }
    }

    /// S-020, S-021: словарь полей перечислен целиком, а точный адрес
    /// отправителя носит собственный идентификатор.
    #[test]
    fn field_dictionary_matches_specification() {
        assert_eq!(RULE_FIELDS.len(), 16);
        assert_eq!(rule_field_kind("sender_address"), Some(RuleFieldKind::Text));
        assert_eq!(rule_field_kind("sender"), Some(RuleFieldKind::Text));
        assert_eq!(rule_field_kind("вымышленное"), None);
    }

    /// S-022: пара поля и оператора проверяется по виду поля.
    #[test]
    fn operator_must_match_field_kind() {
        let ok = rule(
            vec![group(vec![condition("subject", "ends_with", "счет")])],
            vec![action("archive")],
        );
        assert!(validate_rule_input(&ok).is_ok());
        let bad = rule(
            vec![group(vec![condition("read_state", "contains", "read")])],
            vec![action("archive")],
        );
        assert!(validate_rule_input(&bad).is_err());
    }

    /// S-025: текстовое условие без значения не сохраняется.
    #[test]
    fn empty_text_value_is_rejected() {
        let rule = rule(
            vec![group(vec![condition("subject", "contains", "   ")])],
            vec![action("archive")],
        );
        assert!(validate_rule_input(&rule).is_err());
    }

    /// S-018, S-019: пустая группа и правило без условий отклоняются.
    #[test]
    fn empty_groups_are_rejected() {
        let empty_group = rule(vec![group(Vec::new())], vec![action("archive")]);
        assert!(validate_rule_input(&empty_group).is_err());
        let no_groups = rule(Vec::new(), vec![action("archive")]);
        assert!(validate_rule_input(&no_groups).is_err());
    }

    /// S-028 - S-030: пределы числа групп, условий и действий.
    #[test]
    fn size_limits_are_enforced() {
        let many_groups = rule(
            (0..MAX_RULE_GROUPS + 1)
                .map(|_| group(vec![condition("subject", "contains", "счет")]))
                .collect(),
            vec![action("archive")],
        );
        assert!(validate_rule_input(&many_groups).is_err());
        let many_conditions = rule(
            vec![group(
                (0..MAX_GROUP_CONDITIONS + 1)
                    .map(|_| condition("subject", "contains", "счет"))
                    .collect(),
            )],
            vec![action("archive")],
        );
        assert!(validate_rule_input(&many_conditions).is_err());
        let many_actions = rule(
            vec![group(vec![condition("subject", "contains", "счет")])],
            (0..MAX_RULE_ACTIONS + 1)
                .map(|_| action("mark_read"))
                .collect(),
        );
        assert!(validate_rule_input(&many_actions).is_err());
    }

    /// S-039, S-040, S-035: одно уводящее действие, после него только
    /// остановка, а сама остановка стоит последней.
    #[test]
    fn action_chain_order_is_enforced() {
        let base = vec![group(vec![condition("subject", "contains", "счет")])];
        let two_takeaways = rule(base.clone(), vec![action("archive"), action("trash")]);
        assert!(validate_rule_input(&two_takeaways).is_err());
        let after_takeaway = rule(base.clone(), vec![action("archive"), action("mark_read")]);
        assert!(validate_rule_input(&after_takeaway).is_err());
        let stop_in_middle = rule(base.clone(), vec![action("stop"), action("mark_read")]);
        assert!(validate_rule_input(&stop_in_middle).is_err());
        let good = rule(
            base,
            vec![action("mark_read"), action("archive"), action("stop")],
        );
        assert!(validate_rule_input(&good).is_ok());
    }

    /// S-023: домен приводится к нижнему регистру и к общей форме, написание
    /// локальной части сохраняется.
    #[test]
    fn sender_address_is_canonical() {
        assert_eq!(
            canonical_sender_address("  Ivan.Petrov@Example.COM. "),
            "Ivan.Petrov@example.com"
        );
        assert_eq!(
            canonical_sender_address("почта@пример.рф"),
            "почта@xn--e1afmkfd.xn--p1ai"
        );
    }

    /// S-049: изменение любой части правила меняет отпечаток подтверждения.
    #[test]
    fn fingerprint_changes_with_the_rule() {
        let base = rule(
            vec![group(vec![condition("subject", "contains", "счет")])],
            vec![action("delete")],
        );
        let first = delete_forever_fingerprint(&base);
        let mut changed = base.clone();
        changed.groups[0].conditions[0].value = "договор".into();
        assert_ne!(first, delete_forever_fingerprint(&changed));
        let mut scoped = base.clone();
        scoped.account_id = Some(7);
        assert_ne!(first, delete_forever_fingerprint(&scoped));
        assert_eq!(first, delete_forever_fingerprint(&base));
    }
}
