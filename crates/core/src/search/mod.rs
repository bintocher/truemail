//! Поиск. Трейт `SearchIndex`: старт на SQLite FTS5, миграция на Tantivy при росте.
//! Тот же индекс доступен внешнему API как источник поиска.

use crate::Result;
use crate::storage::Db;

#[async_trait::async_trait]
pub trait SearchIndex: Send + Sync {
    /// Проиндексировать тело письма (метаданные индексируются триггером БД).
    async fn index_body(&self, message_id: i64, body_text: &str) -> Result<()>;
    /// Полнотекстовый поиск, вернуть id писем.
    async fn search(&self, query: &str, limit: i64) -> Result<Vec<i64>>;
}

/// Реализация на SQLite FTS5.
pub struct Fts5Index {
    db: Db,
}

impl Fts5Index {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl SearchIndex for Fts5Index {
    async fn index_body(&self, message_id: i64, body_text: &str) -> Result<()> {
        sqlx::query("UPDATE messages_fts SET body = ? WHERE rowid = ?")
            .bind(body_text)
            .bind(message_id)
            .execute(&self.db.write_pool)
            .await?;
        Ok(())
    }

    async fn search(&self, query: &str, limit: i64) -> Result<Vec<i64>> {
        let rows: Vec<(i64,)> = sqlx::query_as(
            "SELECT rowid FROM messages_fts WHERE messages_fts MATCH ? ORDER BY rank LIMIT ?",
        )
        .bind(query)
        .bind(limit)
        .fetch_all(&self.db.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }
}

/// Раскладко-независимое сопоставление запроса (латиница <-> кириллица по позициям
/// клавиш): "as" находит "фы", "ыуе" находит "set". Используется в «Поиске и командах».
pub fn layout_variants(q: &str) -> Vec<String> {
    const RU: &str = "йцукенгшщзхъфывапролджэячсмитьбю";
    const EN: &str = "qwertyuiop[]asdfghjkl;'zxcvbnm,.";
    let conv = |s: &str, from: &str, to: &str| -> String {
        let to: Vec<char> = to.chars().collect();
        s.chars()
            .map(|c| match from.chars().position(|f| f == c) {
                Some(i) if i < to.len() => to[i],
                _ => c,
            })
            .collect()
    };
    let ql = q.to_lowercase();
    vec![ql.clone(), conv(&ql, RU, EN), conv(&ql, EN, RU)]
}

/// Безопасный префиксный запрос FTS5. Благодаря `*` поиск начинает находить
/// слова уже с двух введённых символов (`дро` -> `Дром`, `ошь` -> `Jimny`
/// после преобразования раскладки). Кавычки исключают влияние FTS-операторов,
/// введённых пользователем.
pub fn prefix_query(q: &str) -> Option<String> {
    let tokens = q
        .split_whitespace()
        .filter(|token| token.chars().count() >= 2)
        .map(|token| format!("\"{}\"*", token.replace('"', "\"\"")))
        .collect::<Vec<_>>();
    (!tokens.is_empty()).then(|| tokens.join(" AND "))
}

/// Префиксные варианты с коррекцией одной лишней/ошибочной клавиши для одного
/// слова длиной от четырёх символов. Например, `jimy` дополнительно проверяет
/// `jim*` и поэтому находит `Jimny`, не расширяя шумные двухбуквенные запросы.
pub fn typo_prefix_queries(q: &str) -> Vec<String> {
    let mut queries = prefix_query(q).into_iter().collect::<Vec<_>>();
    let tokens = q.split_whitespace().collect::<Vec<_>>();
    if tokens.len() != 1 || tokens[0].chars().count() < 4 {
        return queries;
    }
    let chars = tokens[0].chars().collect::<Vec<_>>();
    for skipped in 0..chars.len() {
        let candidate = chars
            .iter()
            .enumerate()
            .filter_map(|(index, ch)| (index != skipped).then_some(*ch))
            .collect::<String>();
        if let Some(query) = prefix_query(&candidate)
            && !queries.contains(&query)
        {
            queries.push(query);
        }
    }
    queries
}

#[cfg(test)]
mod tests {
    use super::{layout_variants, prefix_query, typo_prefix_queries};

    #[test]
    fn builds_safe_prefix_query_from_two_characters() {
        assert_eq!(prefix_query("др"), Some("\"др\"*".into()));
        assert_eq!(
            prefix_query("Дро Suzuki"),
            Some("\"Дро\"* AND \"Suzuki\"*".into())
        );
        assert_eq!(prefix_query("д"), None);
    }

    #[test]
    fn converts_physical_keyboard_layout() {
        assert!(layout_variants("ошь").contains(&"jim".to_owned()));
    }

    #[test]
    fn tolerates_one_wrong_key_in_longer_single_word() {
        assert!(typo_prefix_queries("jimy").contains(&"\"jim\"*".to_owned()));
        assert_eq!(typo_prefix_queries("др"), vec!["\"др\"*"]);
    }
}
