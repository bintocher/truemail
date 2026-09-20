//! История получателей: порядок кандидатов подсказки, разбор имени адресата и
//! границы хранения (specs/recipient-history.md).
//!
//! Ранг записи считает запрос базы: отметки обращений на каждую видимую
//! запись в память не поднимаются.

use serde::{Deserialize, Serialize};

// Число отметок на запись (S-027), число видимых записей на ящик (S-051),
// длина подсказки (S-032), срок памяти о собственных отправках (S-010) и
// размер пачки первичного заполнения задаются настройками: ключи
// LIMIT_RECIPIENT_TOUCHES, LIMIT_RECIPIENT_ENTRIES, LIMIT_RECIPIENT_SUGGESTIONS,
// LIMIT_RECIPIENT_OWN_SEND_DAYS и LIMIT_PURGE_BATCH
// (crates/core/src/model/limits.rs).

/// Раздел управления историей загружает записи страницами.
pub const HISTORY_PAGE: i64 = 100;
/// Предел длины адреса истории (S-023). Тот же предел, что и у записей списков
/// отправителей: имя взято другое, чтобы общая область имён модели не
/// сталкивалась.
pub const MAX_HISTORY_ADDRESS_BYTES: usize = 254;

/// Имя и адрес из строки адресата вида `Имя <user@example.test>`. Собственная
/// отправка хранит адресатов такими строками целиком: без разбора запись
/// истории получила бы пустое имя и адрес вместе с именем внутри (S-024,
/// S-038).
pub fn split_display_address(value: &str) -> (String, String) {
    let trimmed = value.trim();
    let angle = trimmed
        .rfind('<')
        .filter(|_| trimmed.ends_with('>'))
        .map(|start| (start, trimmed.len() - 1));
    let Some((start, end)) = angle else {
        return (String::new(), trimmed.to_owned());
    };
    let email = trimmed[start + 1..end].trim().to_owned();
    let name = trimmed[..start].trim().trim_matches('"').trim();
    // Строка вида "<user@example.test>" имени не несёт, а адрес именем не
    // считается: подпись кандидата показывает его и так (S-038).
    if name.is_empty() || name.contains('@') {
        return (String::new(), email);
    }
    (name.to_owned(), email)
}

/// Источник кандидата подсказки: кандидат только из истории помечается в
/// подсказке как взятый из переписки (S-039).
pub const CANDIDATE_SOURCE_CONTACT: &str = "contact";
pub const CANDIDATE_SOURCE_HISTORY: &str = "history";
pub const CANDIDATE_SOURCE_BOTH: &str = "both";

/// Объединённый кандидат подсказки: один адрес, собранный из контакта, из
/// истории или из обоих источников (S-035).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipientCandidate {
    pub email: String,
    pub name: String,
    pub source: String,
    pub favorite: bool,
    pub rank: i64,
    pub touches: i64,
    pub last_used_at: Option<String>,
}

/// Упорядочить кандидатов (S-029, S-030). Кандидат без обращений стоит ниже
/// любого кандидата с обращениями независимо от ранга: ранг ноль бывает и у
/// записи, все отметки которой старше года, и такая запись всё равно говорит о
/// состоявшейся переписке.
pub fn order_candidates(candidates: &mut [RecipientCandidate]) {
    candidates.sort_by(|left, right| {
        let left_used = left.touches > 0;
        let right_used = right.touches > 0;
        right_used
            .cmp(&left_used)
            .then_with(|| {
                if left_used {
                    right
                        .rank
                        .cmp(&left.rank)
                        .then_with(|| right.last_used_at.cmp(&left.last_used_at))
                } else {
                    right.favorite.cmp(&left.favorite)
                }
            })
            .then_with(|| {
                left.name
                    .to_lowercase()
                    .cmp(&right.name.to_lowercase())
                    .then_with(|| left.email.to_lowercase().cmp(&right.email.to_lowercase()))
            })
    });
}

/// Запись раздела управления историей (S-042).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipientHistoryEntry {
    pub id: i64,
    pub account_id: i64,
    pub address: String,
    pub name: String,
    pub name_edited: bool,
    /// Число сохранённых отметок, а не число писем: отметок хранится не больше
    /// 50, и настоящих писем могло быть больше (S-042).
    pub saved_touches: i64,
    pub last_used_at: Option<String>,
    pub hidden_by_user: bool,
    pub evicted: bool,
}

/// Итог переноса почтовых контактов прежнего сбора в историю (S-002, S-004).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContactMigrationReport {
    pub moved: i64,
    /// Дополненный пользователем контакт остаётся в адресной книге как есть.
    pub kept: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// S-024, S-038: имя и адрес берутся из строки адресата собственной
    /// отправки. Адрес без имени и адрес, записанный именем, имени не дают, но
    /// сам адрес возвращается всегда без угловых скобок.
    #[test]
    fn display_name_and_address_come_from_the_address_string() {
        assert_eq!(
            split_display_address("Иванов Иван <ivanov@example.test>"),
            ("Иванов Иван".to_owned(), "ivanov@example.test".to_owned())
        );
        assert_eq!(
            split_display_address("\"Иванов, Иван\" <ivanov@example.test>"),
            ("Иванов, Иван".to_owned(), "ivanov@example.test".to_owned())
        );
        assert_eq!(
            split_display_address("ivanov@example.test"),
            (String::new(), "ivanov@example.test".to_owned())
        );
        assert_eq!(
            split_display_address("<ivanov@example.test>"),
            (String::new(), "ivanov@example.test".to_owned())
        );
        assert_eq!(
            split_display_address("ivanov@example.test <ivanov@example.test>"),
            (String::new(), "ivanov@example.test".to_owned())
        );
    }

    fn candidate(
        email: &str,
        name: &str,
        rank: i64,
        touches: i64,
        last: Option<&str>,
    ) -> RecipientCandidate {
        RecipientCandidate {
            email: email.into(),
            name: name.into(),
            source: CANDIDATE_SOURCE_HISTORY.into(),
            favorite: false,
            rank,
            touches,
            last_used_at: last.map(str::to_owned),
        }
    }

    /// S-029, S-030: переписка важнее алфавита, а кандидат без обращений стоит
    /// ниже любого, кому пользователь писал, - даже если по алфавиту он первый.
    #[test]
    fn correspondence_outranks_the_alphabet() {
        let mut candidates = vec![
            candidate("aaa@example.test", "Аверин", 0, 0, None),
            candidate(
                "zzz@example.test",
                "Яковлев",
                3,
                1,
                Some("2026-09-01T10:00:00+00:00"),
            ),
            candidate(
                "mmm@example.test",
                "Морозов",
                3,
                2,
                Some("2026-09-10T10:00:00+00:00"),
            ),
        ];
        order_candidates(&mut candidates);
        assert_eq!(candidates[0].email, "mmm@example.test");
        assert_eq!(candidates[1].email, "zzz@example.test");
        assert_eq!(candidates[2].email, "aaa@example.test");
    }

    /// S-030: среди кандидатов без обращений избранный контакт выше остальных,
    /// а дальше идёт возрастание показываемого имени.
    #[test]
    fn favorite_contact_leads_among_candidates_without_correspondence() {
        let mut candidates = vec![
            candidate("b@example.test", "Борисов", 0, 0, None),
            RecipientCandidate {
                favorite: true,
                source: CANDIDATE_SOURCE_CONTACT.into(),
                ..candidate("ya@example.test", "Яшин", 0, 0, None)
            },
            candidate("a@example.test", "Антонов", 0, 0, None),
        ];
        order_candidates(&mut candidates);
        assert_eq!(candidates[0].email, "ya@example.test");
        assert_eq!(candidates[1].email, "a@example.test");
        assert_eq!(candidates[2].email, "b@example.test");
    }
}
