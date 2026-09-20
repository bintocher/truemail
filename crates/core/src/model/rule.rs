//! Правила обработки почты: группы условий, группы исключений и цепочка
//! действий. Словарь полей, операторов и действий живёт здесь и словарь умных
//! папок не переиспользует: добавленный там оператор молча изменил бы смысл
//! почтового правила (S-020 - S-022).

use super::{LIMIT_GROUP_CONDITIONS, LIMIT_RULE_ACTIONS, LIMIT_RULE_GROUPS, LimitSet};
use serde::{Deserialize, Serialize};

// Предельные размеры правила (S-028 - S-030) задаются настройками: ключи LIMIT_RULE_GROUPS,
// LIMIT_GROUP_CONDITIONS и LIMIT_RULE_ACTIONS (crates/core/src/model/limits.rs).
// Здесь чисел нет намеренно: прежде те же три числа лежали и в интерфейсе, и
// копии начали расходиться.

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
pub fn validate_rule_input(rule: &MailRuleInput, limits: &LimitSet) -> Result<(), String> {
    let max_groups = limits.count(LIMIT_RULE_GROUPS);
    if rule.id.trim().is_empty() || rule.name.trim().is_empty() {
        return Err("правилу нужны идентификатор и название".into());
    }
    if rule.groups.len() > max_groups || rule.exceptions.len() > max_groups {
        return Err(format!(
            "в правиле не больше {max_groups} групп условий и {max_groups} групп исключений"
        ));
    }
    // S-019: правило без единой обычной группы с условием применилось бы к
    // любому письму.
    if rule.groups.is_empty() || rule.groups.iter().all(|group| group.conditions.is_empty()) {
        return Err("в правиле нет ни одного условия".into());
    }
    for group in rule.groups.iter().chain(rule.exceptions.iter()) {
        validate_group(group, limits)?;
    }
    validate_actions(&rule.actions, limits)?;
    Ok(())
}

fn validate_group(group: &MailRuleGroup, limits: &LimitSet) -> Result<(), String> {
    if group.conditions.is_empty() {
        return Err("группа без условий не сохраняется".into());
    }
    let max_conditions = limits.count(LIMIT_GROUP_CONDITIONS);
    if group.conditions.len() > max_conditions {
        return Err(format!("в группе не больше {max_conditions} условий"));
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

fn validate_actions(actions: &[MailRuleAction], limits: &LimitSet) -> Result<(), String> {
    if actions.is_empty() {
        return Err("в правиле нет ни одного действия".into());
    }
    let max_actions = limits.count(LIMIT_RULE_ACTIONS);
    if actions.len() > max_actions {
        return Err(format!("в правиле не больше {max_actions} действий"));
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

/// Проверка цепочки быстрого действия делит с правилами словарь и требования
/// к целям, но намеренно не вызывает validate_actions: правила допускают stop
/// и хвост из stop после увода, а кнопка быстрого действия - нет.
pub fn validate_quick_step_input(
    step: &super::QuickStepInput,
    limits: &LimitSet,
) -> Result<(), String> {
    let name = step.name.trim();
    if name.is_empty() || name.chars().count() > 40 {
        return Err("имя быстрого действия должно содержать от 1 до 40 символов".into());
    }
    if step.actions.is_empty() {
        return Err("в быстром действии нет ни одного действия".into());
    }
    let max_actions = limits.count(LIMIT_RULE_ACTIONS);
    if step.actions.len() > max_actions {
        return Err(format!(
            "в быстром действии не больше {max_actions} действий"
        ));
    }
    if step
        .hotkey_slot
        .is_some_and(|slot| !(1..=super::QUICK_STEP_SLOTS).contains(&slot))
    {
        return Err(format!(
            "номер слота горячей клавиши должен быть от 1 до {}",
            super::QUICK_STEP_SLOTS
        ));
    }
    let mut takeaway = None;
    for (index, action) in step.actions.iter().enumerate() {
        if !RULE_ACTIONS.contains(&action.kind.as_str()) || action.kind == "stop" {
            return Err(format!("действие {} не поддерживается", action.kind));
        }
        if action.kind == "move" && action.folder_id.is_none() && action.folder_role.is_none() {
            return Err("у перемещения не выбрана папка назначения".into());
        }
        if matches!(action.kind.as_str(), "label_add" | "label_remove") && action.label_id.is_none()
        {
            return Err("у действия с меткой не выбрана метка".into());
        }
        if is_takeaway_action(&action.kind) && takeaway.replace(index).is_some() {
            return Err(
                "письмо можно увести только один раз: оставьте одно уводящее действие".into(),
            );
        }
    }
    if let Some(index) = takeaway
        && index + 1 != step.actions.len()
    {
        return Err("после уводящего действия остальные действия не выполнятся".into());
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
    use crate::storage::Db;
    use crate::storage::repo::test_storage::{TestDb, open_test_db};

    /// Ящик проверок: тот же адрес нужен и условию по полю `account`.
    const ACCOUNT_EMAIL: &str = "rules@example.test";

    /// Проверка на значениях первого запуска: их же получает база, где
    /// настройку ещё не меняли.
    fn validate_rule_input_default(rule: &MailRuleInput) -> Result<(), String> {
        validate_rule_input(rule, &LimitSet::defaults())
    }

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

    /// S-018, S-019, S-022, S-025: правило, которым письмо разобрать нельзя,
    /// до записи не доходит. Причина сверяется дословно: без неё перестановка
    /// проверок местами осталась бы незамеченной, а отказ по другой причине
    /// выглядел бы успехом.
    #[test]
    fn rule_input_rejections_name_their_reason() {
        let ok_condition = || condition("subject", "ends_with", "счет");
        let cases: Vec<(&str, MailRuleInput, Option<&str>)> = vec![
            (
                "подходящая пара поля и оператора",
                rule(
                    vec![group(vec![ok_condition()])],
                    vec![action("archive")],
                ),
                None,
            ),
            (
                "оператор чужого вида поля",
                rule(
                    vec![group(vec![condition("read_state", "contains", "read")])],
                    vec![action("archive")],
                ),
                Some("оператор contains не подходит полю read_state"),
            ),
            (
                "текстовое условие из одних пробелов",
                rule(
                    vec![group(vec![condition("subject", "contains", "   ")])],
                    vec![action("archive")],
                ),
                Some("значение условия не может быть пустым"),
            ),
            (
                "единственная группа без условий",
                rule(vec![group(Vec::new())], vec![action("archive")]),
                Some("в правиле нет ни одного условия"),
            ),
            (
                "правило без групп",
                rule(Vec::new(), vec![action("archive")]),
                Some("в правиле нет ни одного условия"),
            ),
            (
                "пустая группа рядом с заполненной",
                rule(
                    vec![group(vec![ok_condition()]), group(Vec::new())],
                    vec![action("archive")],
                ),
                Some("группа без условий не сохраняется"),
            ),
            (
                "пустая группа исключений",
                {
                    let mut input =
                        rule(vec![group(vec![ok_condition()])], vec![action("archive")]);
                    input.exceptions = vec![group(Vec::new())];
                    input
                },
                Some("группа без условий не сохраняется"),
            ),
        ];
        for (name, input, expected) in cases {
            match (validate_rule_input_default(&input), expected) {
                (Ok(()), None) => {}
                (Err(reason), Some(expected)) => assert_eq!(reason, expected, "{name}"),
                (Ok(()), Some(expected)) => {
                    panic!("{name}: правило принято, ожидался отказ \"{expected}\"")
                }
                (Err(reason), None) => panic!("{name}: правило отклонено: {reason}"),
            }
        }
    }

    /// S-028 - S-030: пределы числа групп, условий и действий берутся из
    /// настроек, а не из числа в коде: проверка идёт по значению первого
    /// запуска, но через тот же снимок, которым пользуется рабочий путь.
    #[test]
    fn size_limits_are_enforced() {
        let limits = LimitSet::defaults();
        let many_groups = rule(
            (0..limits.count(LIMIT_RULE_GROUPS) + 1)
                .map(|_| group(vec![condition("subject", "contains", "счет")]))
                .collect(),
            vec![action("archive")],
        );
        assert!(validate_rule_input_default(&many_groups).is_err());
        let many_conditions = rule(
            vec![group(
                (0..limits.count(LIMIT_GROUP_CONDITIONS) + 1)
                    .map(|_| condition("subject", "contains", "счет"))
                    .collect(),
            )],
            vec![action("archive")],
        );
        assert!(validate_rule_input_default(&many_conditions).is_err());
        let many_actions = rule(
            vec![group(vec![condition("subject", "contains", "счет")])],
            (0..limits.count(LIMIT_RULE_ACTIONS) + 1)
                .map(|_| action("mark_read"))
                .collect(),
        );
        assert!(validate_rule_input_default(&many_actions).is_err());
    }

    /// S-039, S-040, S-035: одно уводящее действие, после него только
    /// остановка, а сама остановка стоит последней.
    #[test]
    fn action_chain_order_is_enforced() {
        let base = vec![group(vec![condition("subject", "contains", "счет")])];
        let two_takeaways = rule(base.clone(), vec![action("archive"), action("trash")]);
        assert!(validate_rule_input_default(&two_takeaways).is_err());
        let after_takeaway = rule(base.clone(), vec![action("archive"), action("mark_read")]);
        assert!(validate_rule_input_default(&after_takeaway).is_err());
        let stop_in_middle = rule(base.clone(), vec![action("stop"), action("mark_read")]);
        assert!(validate_rule_input_default(&stop_in_middle).is_err());
        let good = rule(
            base,
            vec![action("mark_read"), action("archive"), action("stop")],
        );
        assert!(validate_rule_input_default(&good).is_ok());
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

    /// Условие по полю словаря: значение, которое на письмо проверки
    /// подходит, и значение, которое подходить не должно.
    struct FieldCase {
        field: &'static str,
        op: &'static str,
        unit: Option<&'static str>,
        matching: &'static str,
        other: &'static str,
        /// Действие для несовпадающего случая, если тем же действием подобрать
        /// его нельзя. У даты так и есть: письмо проверки получено только что,
        /// и "получено за последние N" совпадает при любом допустимом N, а
        /// нулём или пустым значением условие просто не сохранится.
        other_op: Option<&'static str>,
    }

    /// Письмо проверки заполнено так, чтобы у каждого поля словаря было и
    /// совпадающее, и заведомо чужое значение.
    const FIELD_CASES: &[FieldCase] = &[
        FieldCase {
            field: "sender",
            op: "contains",
            unit: None,
            matching: "Иван Иванов",
            other: "Пётр Петров",
            other_op: None,
        },
        FieldCase {
            field: "sender_address",
            op: "equals",
            unit: None,
            matching: "ivan@example.test",
            other: "petr@example.test",
            other_op: None,
        },
        FieldCase {
            field: "recipient",
            op: "contains",
            unit: None,
            matching: "sales@example.test",
            other: "support@example.test",
            other_op: None,
        },
        FieldCase {
            field: "subject",
            op: "starts_with",
            unit: None,
            matching: "Счет",
            other: "Договор",
            other_op: None,
        },
        FieldCase {
            field: "body",
            op: "contains",
            unit: None,
            matching: "оплату",
            other: "доставку",
            other_op: None,
        },
        FieldCase {
            field: "account",
            op: "equals",
            unit: None,
            matching: ACCOUNT_EMAIL,
            other: "other@example.test",
            other_op: None,
        },
        FieldCase {
            field: "folder",
            op: "contains",
            unit: None,
            matching: "Входящие",
            other: "Архив",
            other_op: None,
        },
        FieldCase {
            field: "folder_role",
            op: "equals",
            unit: None,
            matching: "inbox",
            other: "archive",
            other_op: None,
        },
        FieldCase {
            field: "read_state",
            op: "equals",
            unit: None,
            matching: "unread",
            other: "read",
            other_op: None,
        },
        FieldCase {
            field: "importance",
            op: "equals",
            unit: None,
            matching: "normal",
            other: "flagged",
            other_op: None,
        },
        FieldCase {
            field: "reply_state",
            op: "equals",
            unit: None,
            matching: "unanswered",
            other: "answered",
            other_op: None,
        },
        FieldCase {
            field: "draft_state",
            op: "equals",
            unit: None,
            matching: "not_draft",
            other: "draft",
            other_op: None,
        },
        FieldCase {
            field: "attachment",
            op: "equals",
            unit: None,
            matching: "none",
            other: "has",
            other_op: None,
        },
        // Письмо проверки весит 2 кб.
        FieldCase {
            field: "size",
            op: "greater_than",
            unit: Some("kb"),
            matching: "1",
            other: "1024",
            other_op: None,
        },
        FieldCase {
            field: "label",
            op: "contains",
            unit: None,
            matching: "важное",
            other: "неважное",
            other_op: None,
        },
        // Письмо проверки только что получено.
        FieldCase {
            field: "date",
            op: "within_last",
            unit: Some("days"),
            matching: "1",
            other: "1",
            other_op: Some("older_than"),
        },
    ];

    async fn seed_account(db: &Db, email: &str) -> i64 {
        sqlx::query_as::<_, (i64,)>(
            "INSERT INTO accounts(uuid, email, provider, backend_kind, auth_kind)
             VALUES(?, ?, 'generic', 'imap', 'password') RETURNING id",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(email)
        .fetch_one(&db.write_pool)
        .await
        .expect("создать ящик")
        .0
    }

    async fn seed_folder(db: &Db, account_id: i64, path: &str, name: &str, role: &str) -> i64 {
        sqlx::query_as::<_, (i64,)>(
            "INSERT INTO folders(account_id, remote_path, display_name, role)
             VALUES(?, ?, ?, ?) RETURNING id",
        )
        .bind(account_id)
        .bind(path)
        .bind(name)
        .bind(role)
        .fetch_one(&db.write_pool)
        .await
        .expect("создать папку")
        .0
    }

    /// Письмо в том виде, в каком его оставляет синхронизация: непрочитанное,
    /// без важности, без вложений и с получателем в заголовке.
    async fn seed_message(db: &Db, account_id: i64, folder_id: i64) -> i64 {
        sqlx::query_as::<_, (i64,)>(
            "INSERT INTO messages(account_id, folder_id, uid, remote_id, from_name, from_addr,
                                  to_addrs, subject, preview, date, size, seen, flagged,
                                  answered, draft, has_attachments)
             VALUES(?, ?, 1, 'remote-1', 'Иван Иванов', 'ivan@example.test',
                    ?, 'Счет за сентябрь', 'просим оплату по счету', datetime('now'),
                    2048, 0, 0, 0, 0, 0)
             RETURNING id",
        )
        .bind(account_id)
        .bind(folder_id)
        .bind(r#"[{"name":"Отдел продаж","email":"sales@example.test"}]"#)
        .fetch_one(&db.write_pool)
        .await
        .expect("создать письмо")
        .0
    }

    async fn seed_label(db: &Db, name: &str) -> i64 {
        sqlx::query_as::<_, (i64,)>("INSERT INTO labels(name) VALUES(?) RETURNING id")
            .bind(name)
            .fetch_one(&db.write_pool)
            .await
            .expect("создать метку")
            .0
    }

    async fn message_labels(db: &Db, message_id: i64) -> Vec<String> {
        let mut names: Vec<String> = sqlx::query_as::<_, (String,)>(
            "SELECT l.name FROM message_labels ml JOIN labels l ON l.id=ml.label_id
              WHERE ml.message_id=?",
        )
        .bind(message_id)
        .fetch_all(&db.pool)
        .await
        .expect("метки письма")
        .into_iter()
        .map(|(name,)| name)
        .collect();
        names.sort();
        names
    }

    /// S-020 - S-022: словарь полей проверяется настоящим прогоном правил.
    /// На каждое поле словаря сохраняются два правила - с условием, которое
    /// письму подходит, и с условием, которое подходить не должно, - и метку
    /// обязано поставить только первое. Перечень, сверяющий сам себя,
    /// пропустил бы и поле, выпавшее из словаря, и поле, которого прогон не
    /// знает: у неизвестного поля значение пустое, и вхождение в него
    /// совпадает с чем угодно.
    #[tokio::test]
    async fn every_dictionary_field_decides_a_real_rule_run() {
        let db: TestDb = open_test_db("rule-dictionary").await;
        let account = seed_account(&db, ACCOUNT_EMAIL).await;
        let inbox = seed_folder(&db, account, "INBOX", "Входящие", "inbox").await;
        let message = seed_message(&db, account, inbox).await;
        let own_label = seed_label(&db, "важное").await;
        sqlx::query("INSERT INTO message_labels(message_id, label_id) VALUES(?, ?)")
            .bind(message)
            .bind(own_label)
            .execute(&db.write_pool)
            .await
            .expect("метка письма");

        let mut expected = vec!["важное".to_owned()];
        for case in FIELD_CASES {
            assert!(
                rule_field_kind(case.field).is_some(),
                "поле {} проверяется прогоном, но в словаре его нет",
                case.field
            );
            for (outcome, value) in [("подходит", case.matching), ("чужое", case.other)] {
                let label_name = format!("{}: {outcome}", case.field);
                let label = seed_label(&db, &label_name).await;
                let op = if outcome == "чужое" {
                    case.other_op.unwrap_or(case.op)
                } else {
                    case.op
                };
                let mut condition = condition(case.field, op, value);
                condition.unit = case.unit.map(str::to_owned);
                let mut input = rule(
                    vec![group(vec![condition])],
                    vec![MailRuleAction {
                        kind: "label_add".into(),
                        label_id: Some(label),
                        ..action("label_add")
                    }],
                );
                input.id = label_name.clone();
                input.name = label_name.clone();
                input.account_id = Some(account);
                db.save_mail_rule(&input, true, None)
                    .await
                    .unwrap_or_else(|error| panic!("сохранить правило {label_name}: {error}"));
                if outcome == "подходит" {
                    expected.push(label_name);
                }
            }
        }
        for (field, _) in RULE_FIELDS {
            assert!(
                FIELD_CASES.iter().any(|case| case.field == *field),
                "поле словаря {field} прогоном не проверено"
            );
        }

        db.process_mail_rules().await.expect("прогон правил");
        expected.sort();
        assert_eq!(
            message_labels(&db, message).await,
            expected,
            "метку ставит условие по полю словаря и только оно"
        );

        // Обратная сторона словаря: поле, которого в нём нет, правилом не
        // становится - иначе условие с пустым значением совпадало бы с любым
        // письмом.
        let mut unknown = rule(
            vec![group(vec![condition("вымышленное", "contains", "счет")])],
            vec![action("mark_read")],
        );
        unknown.id = "неизвестное поле".into();
        unknown.account_id = Some(account);
        let error = db
            .save_mail_rule(&unknown, true, None)
            .await
            .expect_err("правило с неизвестным полем сохранилось");
        assert!(
            error.to_string().contains("вымышленное"),
            "отказ должен называть поле: {error}"
        );
        let saved = db.list_mail_rules().await.expect("список правил");
        assert!(
            saved.iter().all(|rule| rule.id != "неизвестное поле"),
            "отклонённое правило не должно попадать в список"
        );
        db.close().await;
    }

    /// S-048, S-049: подтверждение удаления навсегда выдаётся на конкретный
    /// состав правила. Изменённое после выдачи правило сохраняться не должно,
    /// неизменённое - должно, и ровно один раз. Отпечаток проверяется через
    /// запись правила, а не сам по себе: его смысл в том, что запись с чужим
    /// подтверждением не проходит.
    #[tokio::test]
    async fn a_changed_rule_needs_a_fresh_delete_confirmation() {
        let db: TestDb = open_test_db("rule-delete-confirm").await;
        let account = seed_account(&db, ACCOUNT_EMAIL).await;
        seed_folder(&db, account, "INBOX", "Входящие", "inbox").await;
        let mut original = rule(
            vec![group(vec![condition("subject", "contains", "счет")])],
            vec![action("delete")],
        );
        original.account_id = Some(account);
        let key = db
            .issue_delete_confirmation(&original)
            .await
            .expect("выдать подтверждение");

        let mut changed = original.clone();
        changed.groups[0].conditions[0].value = "договор".into();
        changed.confirm_key = Some(key.clone());
        let error = db
            .save_mail_rule(&changed, true, None)
            .await
            .expect_err("изменённое правило прошло со старым подтверждением");
        assert!(
            error.to_string().contains("подтвердить заново"),
            "отказ должен звать подтвердить заново: {error}"
        );
        assert!(
            db.list_mail_rules()
                .await
                .expect("список правил")
                .is_empty(),
            "отклонённое правило не должно сохраняться"
        );

        original.confirm_key = Some(key.clone());
        db.save_mail_rule(&original, true, None)
            .await
            .expect("неизменённое правило с его подтверждением");
        let repeated = db
            .save_mail_rule(&original, true, None)
            .await
            .expect_err("подтверждение принято второй раз");
        assert!(
            repeated.to_string().contains("подтвердить заново"),
            "подтверждение одноразовое: {repeated}"
        );

        let fresh = db
            .issue_delete_confirmation(&changed)
            .await
            .expect("выдать подтверждение изменённому правилу");
        assert_ne!(fresh, key, "у изменённого правила своё подтверждение");
        changed.confirm_key = Some(fresh);
        db.save_mail_rule(&changed, true, None)
            .await
            .expect("изменённое правило со своим подтверждением");
        db.close().await;
    }
}
