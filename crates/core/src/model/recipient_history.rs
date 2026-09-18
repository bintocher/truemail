//! История получателей: ранг записи по распределению обращений во времени,
//! порядок кандидатов подсказки и границы хранения
//! (specs/recipient-history.md).
//!
//! Ранжирование - чистые функции: они считаются по отметкам обращений и
//! проверяются без базы.

use serde::{Deserialize, Serialize};

/// На запись хранится не больше 50 отметок обращений (S-027).
pub const MAX_TOUCHES: i64 = 50;
/// Видимых записей на ящик не больше 2000 (S-051).
pub const MAX_VISIBLE_ENTRIES: i64 = 2000;
/// Подсказка показывает не больше восьми адресов (S-032).
pub const MAX_SUGGESTIONS: usize = 8;
/// Отметка собственной отправки хранится не менее 30 суток (S-010).
pub const OWN_SEND_DAYS: i64 = 30;
/// Первичное заполнение читает письма пачками, чтобы не держать писателя.
pub const BACKFILL_BATCH: i64 = 500;
/// Раздел управления историей загружает записи страницами.
pub const HISTORY_PAGE: i64 = 100;
/// Предел длины адреса истории (S-023). Тот же предел, что и у записей списков
/// отправителей: имя взято другое, чтобы общая область имён модели не
/// сталкивалась.
pub const MAX_HISTORY_ADDRESS_BYTES: usize = 254;

/// Вес одной отметки обращения по её возрасту в сутках (S-028). Границы
/// закрыты явно: щели между группами нет, поэтому отметка возрастом ровно 30,
/// 90 или 365 суток попадает в следующую группу, а не теряется.
pub fn touch_weight(age_days: f64) -> i64 {
    if age_days < 30.0 {
        3
    } else if age_days < 90.0 {
        2
    } else if age_days < 365.0 {
        1
    } else {
        0
    }
}

/// Ранг записи - сумма весов её отметок обращений (S-028).
pub fn entry_rank(age_days: &[f64]) -> i64 {
    age_days.iter().copied().map(touch_weight).sum()
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

    /// S-028: границы весов закрыты явно. Отметка ровно на границе принадлежит
    /// следующей группе, иначе между группами оставалась бы щель и такое
    /// обращение не считалось бы вовсе.
    #[test]
    fn touch_weight_has_no_gaps_on_its_boundaries() {
        assert_eq!(touch_weight(0.0), 3);
        assert_eq!(touch_weight(29.999), 3);
        assert_eq!(touch_weight(30.0), 2);
        assert_eq!(touch_weight(89.999), 2);
        assert_eq!(touch_weight(90.0), 1);
        assert_eq!(touch_weight(364.999), 1);
        assert_eq!(touch_weight(365.0), 0);
        assert_eq!(entry_rank(&[1.0, 45.0, 120.0, 400.0]), 6);
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
