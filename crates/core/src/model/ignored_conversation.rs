//! Игнорирование переписки: опознание по идентификаторам писем, состояния
//! записи и возврата (specs/ignore-conversation.md).
//!
//! Тема переписки здесь не участвует ни в одном сравнении: она совпадает у
//! несвязанных писем, и одно действие уносило бы в корзину чужую почту (S-009).

use serde::{Deserialize, Serialize};

/// Название стадии игнорируемых переписок в столбце результата стадии письма
/// (specs/mail-rules-conditions-and-actions.md, S-009).
pub const IGNORED_CONVERSATION_STAGE_NAME: &str = "ignored_conversation";

/// Один нормализованный идентификатор письма не длиннее 998 байт: это предел
/// строки заголовка по почтовым стандартам.
pub const MAX_MESSAGE_ID_BYTES: usize = 998;
/// Снимки темы и участников хранятся усечёнными до 4096 байт.
pub const MAX_SNAPSHOT_BYTES: usize = 4096;
/// Из одного заголовка References берётся не более 1000 идентификаторов.
pub const MAX_REFERENCES: usize = 1000;
// Предел набора одной переписки (S-028), предел числа включённых переписок
// (S-029) и срок ожидания письма в корзине при возврате (S-042) задаются
// настройками: ключи LIMIT_CONVERSATION_IDS, LIMIT_IGNORED_CONVERSATIONS и
// LIMIT_IGNORE_RETURN_WAIT_DAYS (crates/core/src/model/limits.rs).

/// Состояния записи игнорируемой переписки (S-032).
pub const IGNORE_STATE_ENABLED: &str = "enabled";
pub const IGNORE_STATE_DISABLING: &str = "disabling";
pub const IGNORE_STATE_RETURNING: &str = "returning";
pub const IGNORE_STATE_DISABLED: &str = "disabled";
pub const IGNORE_STATE_RETURN_FAILED: &str = "return_failed";

/// Состояния возврата одного письма (S-038, S-041 - S-043).
pub const RETURN_STATE_PENDING: &str = "pending";
pub const RETURN_STATE_WAITING_SYNC: &str = "waiting_sync";
pub const RETURN_STATE_COMPLETED: &str = "completed";
pub const RETURN_STATE_SKIPPED: &str = "skipped";
pub const RETURN_STATE_FAILED: &str = "failed";

/// Запись игнорируемой переписки для списка в настройках (S-031).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct IgnoredConversation {
    pub id: i64,
    pub account_id: i64,
    pub account_email: String,
    pub subject: String,
    pub participants: String,
    pub state: String,
    /// Набор достиг предела идентификаторов (S-028).
    pub partial: bool,
    pub created_at: String,
    pub updated_at: String,
    /// Число идентификаторов набора.
    pub ids_count: i64,
    /// Убранные письма и разбивка их возвратов (S-031, S-038).
    pub moved: i64,
    pub returned: i64,
    pub skipped: i64,
    pub failed: i64,
    pub last_error: Option<String>,
    /// Письма, перемещение которых дошло до постоянного отказа очереди уже
    /// после успешного прохода (S-049).
    #[sqlx(default)]
    pub queue_failed: i64,
    /// Причина последнего постоянного отказа очереди (S-049).
    #[sqlx(default)]
    pub queue_error: Option<String>,
}

/// Предварительный просмотр включения игнорирования (S-013, S-014).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IgnoreConversationPreview {
    pub account_id: i64,
    pub account_email: String,
    pub subject: String,
    pub participants: String,
    pub snapshot_key: String,
    /// Число писем, которые уйдут в корзину.
    pub total: i64,
    /// Число идентификаторов набора переписки.
    pub ids_count: i64,
    /// Набор упёрся в предел идентификаторов (S-028).
    pub partial: bool,
    /// Переписка уже игнорируется: включение открывает существующую запись
    /// (S-044).
    pub existing_id: Option<i64>,
}

/// Отчёт прохода уборки или возврата (S-022, S-024, S-036, S-038).
#[derive(Debug, Clone, Default, Serialize, Deserialize, sqlx::FromRow)]
pub struct IgnoreJobReport {
    pub id: i64,
    pub conversation_id: i64,
    pub kind: String,
    pub state: String,
    pub queued: i64,
    pub skipped: i64,
    pub failed: i64,
    pub remaining: i64,
    /// Перемещения, которые уже выполняются и отменены быть не могут (S-036).
    pub irreversible: i64,
}

/// Нормализованный идентификатор письма: обрезаны внешние пробелы, угловые
/// скобки и переносы строк заголовка (S-008).
pub fn normalize_message_id(value: &str) -> Option<String> {
    let cleaned: String = value
        .chars()
        .filter(|symbol| !symbol.is_control())
        .collect::<String>()
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim()
        .to_owned();
    if cleaned.is_empty() || cleaned.len() > MAX_MESSAGE_ID_BYTES {
        return None;
    }
    Some(cleaned)
}

/// Разбор заголовка References на отдельные идентификаторы (S-006). Ошибка
/// разбора одного элемента не отбрасывает остальные.
pub fn parse_references(value: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = value;
    while let Some(open) = rest.find('<') {
        let Some(close) = rest[open..].find('>') else {
            break;
        };
        if let Some(id) = normalize_message_id(&rest[open + 1..open + close]) {
            found.push(id);
        }
        rest = &rest[open + close + 1..];
        if found.len() >= MAX_REFERENCES {
            return found;
        }
    }
    // Заголовок без угловых скобок встречается у самодельных отправителей:
    // тогда идентификаторы разделены пробелами.
    if found.is_empty() {
        for part in value.split_whitespace() {
            if let Some(id) = normalize_message_id(part) {
                found.push(id);
            }
            if found.len() >= MAX_REFERENCES {
                break;
            }
        }
    }
    found
}

/// Все идентификаторы письма: собственный, ссылка на родителя и элементы
/// References (S-006, S-027).
pub fn message_identifiers(
    message_id: Option<&str>,
    in_reply_to: Option<&str>,
    references: Option<&str>,
) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut push = |value: String| {
        if !found.contains(&value) {
            found.push(value);
        }
    };
    if let Some(id) = message_id.and_then(normalize_message_id) {
        push(id);
    }
    if let Some(id) = in_reply_to.and_then(normalize_message_id) {
        push(id);
    }
    for id in references.map(parse_references).unwrap_or_default() {
        push(id);
    }
    found
}

/// Усечь снимок темы или участников до предела хранения по границе символа.
pub fn truncate_snapshot(value: &str) -> String {
    if value.len() <= MAX_SNAPSHOT_BYTES {
        return value.to_owned();
    }
    let mut end = MAX_SNAPSHOT_BYTES;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

/// Состояние записи принимает команду включения только когда возврат уже
/// завершён или не начинался (S-035).
pub fn can_enable_again(state: &str) -> bool {
    matches!(state, IGNORE_STATE_ENABLED | IGNORE_STATE_DISABLED)
}

/// Состояние записи принимает команду прекращения (S-032). Пока идёт
/// прекращение или возврат, вторая такая команда завела бы второе задание
/// возврата на те же письма; после неполного возврата повтор разрешён -
/// письма могли появиться в корзине позже.
pub fn can_start_disable(state: &str) -> bool {
    matches!(state, IGNORE_STATE_ENABLED | IGNORE_STATE_RETURN_FAILED)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Опознание переписки держится на заголовках, которые приходят с сервера
    /// в произвольном виде: со скобками и без, с переносами строк, с битым
    /// элементом внутри References и в количестве, превышающем предел
    /// (S-006 - S-008, S-027). Ошибка разбора одного элемента не должна
    /// отбрасывать остальные, а слишком длинный заголовок - раздувать набор.
    #[test]
    fn message_identifiers_survive_broken_headers() {
        assert_eq!(
            normalize_message_id(
                " <abc@example.test>
 "
            )
            .as_deref(),
            Some("abc@example.test")
        );
        assert_eq!(normalize_message_id("   "), None);
        assert_eq!(normalize_message_id(&"x".repeat(1200)), None);
        assert_eq!(
            parse_references("<one@example.test> <broken <two@example.test>"),
            vec![
                "one@example.test".to_owned(),
                "broken <two@example.test".to_owned()
            ]
        );
        assert_eq!(
            parse_references("one@example.test two@example.test"),
            vec!["one@example.test".to_owned(), "two@example.test".to_owned()]
        );
        let huge = (0..1500)
            .map(|index| format!("<id{index}@example.test>"))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(parse_references(&huge).len(), MAX_REFERENCES);
        // Один и тот же идентификатор из разных заголовков в набор попадает
        // один раз.
        assert_eq!(
            message_identifiers(
                Some("<own@example.test>"),
                Some("<parent@example.test>"),
                Some("<parent@example.test> <root@example.test>"),
            ),
            vec![
                "own@example.test".to_owned(),
                "parent@example.test".to_owned(),
                "root@example.test".to_owned()
            ]
        );
        // Снимок темы режется по границе символа: срез посреди буквы уронил бы
        // сохранение записи.
        let long = "я".repeat(4000);
        let cut = truncate_snapshot(&long);
        assert!(cut.len() <= MAX_SNAPSHOT_BYTES && long.starts_with(&cut));
    }
}
