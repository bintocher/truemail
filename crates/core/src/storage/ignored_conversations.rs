//! Игнорирование переписки: опознание по идентификаторам писем, уборка
//! текущих писем в корзину, стадия разбора новых писем и возврат писем из
//! корзины по приметам (specs/ignore-conversation.md).
//!
//! Серверный интерфейс применения операции не меняется: нового расположения
//! письма он не сообщает, поэтому возврат ищет письмо в корзине по сохранённым
//! приметам и честно называет число ненайденных (S-037, S-038).

use super::Db;
use super::stages::{STAGE_BATCH, STAGE_MESSAGE_COLUMNS, StageCounters, StageMessage};
use crate::Result;
use crate::model::*;
use crate::storage::repo::{
    TakeawayActor, TakeawayOutcome, TakeawayTarget, queue_takeaway_operation, resolve_role_folder,
};
use sqlx::AssertSqlSafe;
use std::collections::{HashMap, HashSet};

/// Вид снимка кандидатов уборки в общей таблице снимков.
const SNAPSHOT_KIND: &str = "ignored_conversation";

/// Папки, из которых игнорирование убирает письма. Архив не берётся ни при
/// каком согласии пользователя: осознанно сохранённую почту игнорирование не
/// трогает (S-017).
const IGNORE_WORKING_FOLDERS: &str =
    "(f.role IS NULL OR f.role NOT IN ('sent','drafts','archive','spam','trash'))";

/// Набор переписки, построенный обходом ссылок (S-006, S-007).
#[derive(Debug, Default)]
pub(crate) struct ConversationSet {
    /// Все идентификаторы набора в порядке появления.
    pub ids: Vec<String>,
    /// Идентификаторы, подтверждённые локальным письмом: только они годятся
    /// для слияния наборов (S-045, S-046).
    pub local_ids: HashSet<String>,
    /// Номера локальных писем набора.
    pub messages: Vec<i64>,
    /// Набор упёрся в предел идентификаторов (S-028).
    pub partial: bool,
}

/// Заголовки опознания одного локального письма: обход переписки читает
/// только их (S-006).
#[derive(Debug, sqlx::FromRow)]
struct ConversationRow {
    id: i64,
    rfc822_message_id: Option<String>,
    in_reply_to: Option<String>,
    references_ids: Option<String>,
}

/// Письмо, с которого пользователь включает игнорирование: ящик, тема,
/// заголовки опознания и участники для снимка записи (S-013, S-047).
#[derive(Debug, sqlx::FromRow)]
struct IgnoreSourceRow {
    account_id: i64,
    #[sqlx(default)]
    email: String,
    subject: Option<String>,
    rfc822_message_id: Option<String>,
    in_reply_to: Option<String>,
    references_ids: Option<String>,
    from_addr: Option<String>,
    #[sqlx(default)]
    to_addrs: Option<String>,
}

/// Убранное письмо, ждущее возврата: сохранённые приметы и состояние (S-020,
/// S-037).
#[derive(Debug, sqlx::FromRow)]
struct PendingReturnRow {
    id: i64,
    source_folder_id: Option<i64>,
    from_addr: Option<String>,
    message_date: Option<String>,
    header_id: Option<String>,
    return_requested_at: Option<String>,
}

/// Метка операции очереди, поставленной игнорированием: по ней отменяются
/// неисполненные перемещения и считаются неотменимые (S-036).
fn ignore_marker(conversation_id: i64) -> String {
    format!("ignored_conversation:{conversation_id}")
}

impl Db {
    /// Обход связей до неподвижного результата в пределах одного ящика (S-007,
    /// S-010). Каждое локальное письмо посещается ровно один раз, обход
    /// завершается, когда новые идентификаторы перестают добавляться или набор
    /// достиг предела.
    pub(crate) async fn build_conversation_set(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        account_id: i64,
        seed: Vec<String>,
    ) -> Result<ConversationSet> {
        let mut set = ConversationSet::default();
        let mut known: HashSet<String> = HashSet::new();
        let mut visited: HashSet<i64> = HashSet::new();
        let mut frontier: Vec<String> = Vec::new();
        for id in seed {
            if known.insert(id.clone()) {
                set.ids.push(id.clone());
                frontier.push(id);
            }
        }
        while let Some(current) = frontier.pop() {
            if set.ids.len() >= MAX_CONVERSATION_IDS {
                set.partial = true;
                break;
            }
            // Заголовки приходят и в угловых скобках, и без них: сравнение
            // идёт с обоими написаниями, чтобы обход пользовался индексом по
            // Message-ID и всё же не терял ветвь переписки.
            let bracketed = format!("<{current}>");
            let rows: Vec<ConversationRow> = sqlx::query_as(
                "SELECT id, rfc822_message_id, in_reply_to, references_ids FROM messages
                  WHERE account_id=?
                    AND (rfc822_message_id=? OR rfc822_message_id=?
                         OR in_reply_to=? OR in_reply_to=?
                         OR (references_ids IS NOT NULL AND instr(references_ids, ?)>0))",
            )
            .bind(account_id)
            .bind(&current)
            .bind(&bracketed)
            .bind(&current)
            .bind(&bracketed)
            .bind(&current)
            .fetch_all(&mut **tx)
            .await?;
            for row in rows {
                if !visited.insert(row.id) {
                    continue;
                }
                set.messages.push(row.id);
                for id in message_identifiers(
                    row.rfc822_message_id.as_deref(),
                    row.in_reply_to.as_deref(),
                    row.references_ids.as_deref(),
                ) {
                    // Идентификатор, взятый из локального письма, подтверждает
                    // цепочку ссылок: по нему разрешено слияние наборов.
                    set.local_ids.insert(id.clone());
                    if set.ids.len() >= MAX_CONVERSATION_IDS {
                        set.partial = true;
                        break;
                    }
                    if known.insert(id.clone()) {
                        set.ids.push(id.clone());
                        frontier.push(id);
                    }
                }
            }
        }
        Ok(set)
    }

    /// Показать тему, ящик и число писем, которые уйдут в корзину, и вернуть
    /// ключ снимка кандидатов (S-013, S-014).
    pub async fn preview_ignore_conversation(
        &self,
        message_id: i64,
    ) -> Result<IgnoreConversationPreview> {
        let mut tx = self.begin_write().await?;
        let row: Option<IgnoreSourceRow> = sqlx::query_as(
            "SELECT m.account_id, a.email, m.subject, m.rfc822_message_id, m.in_reply_to,
                    m.references_ids, m.from_addr, m.to_addrs
               FROM messages m JOIN accounts a ON a.id=m.account_id
              WHERE m.id=?",
        )
        .bind(message_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(source) = row else {
            return Err(crate::Error::AccountConfig("письмо не найдено".into()));
        };
        let account_id = source.account_id;
        let seed = message_identifiers(
            source.rfc822_message_id.as_deref(),
            source.in_reply_to.as_deref(),
            source.references_ids.as_deref(),
        );
        // S-011: переписку без единого идентификатора опознать нечем.
        if seed.is_empty() {
            return Err(crate::Error::AccountConfig(
                "эту переписку нечем опознать: у письма нет заголовков Message-ID, In-Reply-To и References".into(),
            ));
        }
        let set = Self::build_conversation_set(&mut tx, account_id, seed).await?;
        let existing = Self::conversation_owner(&mut tx, account_id, &set.ids).await?;
        let total = Self::count_conversation_messages(&mut tx, account_id, &set.messages).await?;
        let max_message_id = Self::max_message_id(&mut tx).await?;
        let payload = format!("{account_id}\n{message_id}");
        let snapshot_key =
            Self::save_stage_snapshot(&mut tx, SNAPSHOT_KIND, &payload, max_message_id, total)
                .await?;
        tx.commit().await?;
        Ok(IgnoreConversationPreview {
            account_id,
            account_email: source.email,
            subject: truncate_snapshot(source.subject.as_deref().unwrap_or("")),
            participants: truncate_snapshot(source.from_addr.as_deref().unwrap_or("")),
            snapshot_key,
            total,
            ids_count: set.ids.len() as i64,
            partial: set.partial,
            existing_id: existing.first().copied(),
        })
    }

    /// Записи, которым уже принадлежит хотя бы один идентификатор набора.
    async fn conversation_owner(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        account_id: i64,
        ids: &[String],
    ) -> Result<Vec<i64>> {
        let mut owners: Vec<i64> = Vec::new();
        for chunk in ids.chunks(200) {
            let placeholders = vec!["?"; chunk.len()].join(",");
            let sql = format!(
                "SELECT DISTINCT conversation_id FROM ignored_conversation_ids
                  WHERE account_id=? AND message_id IN ({placeholders})"
            );
            let mut query = sqlx::query_as::<_, (i64,)>(AssertSqlSafe(sql)).bind(account_id);
            for id in chunk {
                query = query.bind(id);
            }
            for (owner,) in query.fetch_all(&mut **tx).await? {
                if !owners.contains(&owner) {
                    owners.push(owner);
                }
            }
        }
        Ok(owners)
    }

    /// Число локальных писем набора в рабочих папках.
    async fn count_conversation_messages(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        account_id: i64,
        messages: &[i64],
    ) -> Result<i64> {
        if messages.is_empty() {
            return Ok(0);
        }
        let mut total = 0;
        for chunk in messages.chunks(400) {
            let placeholders = vec!["?"; chunk.len()].join(",");
            let sql = format!(
                "SELECT count(*) FROM messages m JOIN folders f ON f.id=m.folder_id
                  WHERE m.account_id=? AND m.id IN ({placeholders}) AND {IGNORE_WORKING_FOLDERS}"
            );
            let mut query = sqlx::query_as::<_, (i64,)>(AssertSqlSafe(sql)).bind(account_id);
            for id in chunk {
                query = query.bind(id);
            }
            total += query.fetch_one(&mut **tx).await?.0;
        }
        Ok(total)
    }

    /// Включить игнорирование переписки: запись создаётся до постановки
    /// первого перемещения, и в той же неделимой операции заводится задание
    /// уборки (S-015, S-016, S-022).
    pub async fn enable_ignore_conversation(
        &self,
        message_id: i64,
        snapshot_key: &str,
        confirmed: bool,
    ) -> Result<IgnoredConversation> {
        if !confirmed {
            return Err(crate::Error::AccountConfig(
                "игнорирование переписки включается только после подтверждения".into(),
            ));
        }
        let mut tx = self.begin_write().await?;
        let row: Option<IgnoreSourceRow> = sqlx::query_as(
            "SELECT m.account_id, a.email, m.subject, m.rfc822_message_id, m.in_reply_to,
                    m.references_ids, m.from_addr, m.to_addrs
               FROM messages m JOIN accounts a ON a.id=m.account_id
              WHERE m.id=?",
        )
        .bind(message_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(source) = row else {
            return Err(crate::Error::AccountConfig("письмо не найдено".into()));
        };
        let account_id = source.account_id;
        let (payload, max_message_id, _) =
            Self::take_stage_snapshot(&mut tx, snapshot_key, SNAPSHOT_KIND).await?;
        if payload != format!("{account_id}\n{message_id}") {
            return Err(crate::Error::AccountConfig(
                "список писем относится к другой переписке, откройте подтверждение заново".into(),
            ));
        }
        let seed = message_identifiers(
            source.rfc822_message_id.as_deref(),
            source.in_reply_to.as_deref(),
            source.references_ids.as_deref(),
        );
        if seed.is_empty() {
            return Err(crate::Error::AccountConfig(
                "эту переписку нечем опознать: у письма нет заголовков Message-ID, In-Reply-To и References".into(),
            ));
        }
        let set = Self::build_conversation_set(&mut tx, account_id, seed).await?;
        let owners = Self::conversation_owner(&mut tx, account_id, &set.ids).await?;
        let conversation_id = match owners.split_first() {
            Some((first, rest)) => {
                let state: (String,) =
                    sqlx::query_as("SELECT state FROM ignored_conversations WHERE id=?")
                        .bind(first)
                        .fetch_one(&mut *tx)
                        .await?;
                // S-035: пока идёт прекращение или возврат, включить заново
                // нельзя.
                if !can_enable_again(&state.0) {
                    return Err(crate::Error::AccountConfig(
                        "по этой переписке идёт возврат писем: дождитесь его завершения или отмените".into(),
                    ));
                }
                // S-046: совпадение идентификатора без подтверждённой цепочки
                // ссылок наборы не объединяет.
                let confirmed_chain = set
                    .ids
                    .iter()
                    .any(|id| set.local_ids.contains(id) && Self::id_belongs(&set, id));
                if !rest.is_empty() && !confirmed_chain {
                    sqlx::query(
                        "UPDATE ignored_conversations SET last_error=?, updated_at=datetime('now')
                          WHERE id=?",
                    )
                    .bind("совпал идентификатор письма без подтверждённой цепочки ссылок: наборы не объединены")
                    .bind(first)
                    .execute(&mut *tx)
                    .await?;
                    return Err(crate::Error::AccountConfig(
                        "идентификатор письма совпал с другой перепиской без подтверждённой цепочки ссылок".into(),
                    ));
                }
                // S-045: наборы пересеклись по подтверждённой цепочке - они
                // объединяются одной неделимой операцией.
                for other in rest {
                    Self::merge_conversations(&mut tx, *first, *other).await?;
                }
                sqlx::query(
                    "UPDATE ignored_conversations SET state='enabled', updated_at=datetime('now')
                      WHERE id=?",
                )
                .bind(first)
                .execute(&mut *tx)
                .await?;
                *first
            }
            None => {
                // S-029: предел числа включённых переписок.
                let enabled: (i64,) = sqlx::query_as(
                    "SELECT count(*) FROM ignored_conversations WHERE state='enabled'",
                )
                .fetch_one(&mut *tx)
                .await?;
                if enabled.0 >= MAX_IGNORED_CONVERSATIONS {
                    return Err(crate::Error::AccountConfig(format!(
                        "уже игнорируется {MAX_IGNORED_CONVERSATIONS} переписок: снимите игнорирование с ненужных"
                    )));
                }
                // S-047: снимок темы и участников остаётся читаемым после того,
                // как успешное перемещение удалит локальные строки писем.
                let participants = truncate_snapshot(&format!(
                    "{} {}",
                    source.from_addr.as_deref().unwrap_or(""),
                    source.to_addrs.as_deref().unwrap_or("")
                ));
                let inserted: (i64,) = sqlx::query_as(
                    "INSERT INTO ignored_conversations(account_id, subject, participants, partial)
                     VALUES(?, ?, ?, ?) RETURNING id",
                )
                .bind(account_id)
                .bind(truncate_snapshot(source.subject.as_deref().unwrap_or("")))
                .bind(participants.trim())
                .bind(set.partial as i64)
                .fetch_one(&mut *tx)
                .await?;
                inserted.0
            }
        };
        Self::store_conversation_ids(&mut tx, conversation_id, account_id, &set.ids).await?;
        if set.partial {
            sqlx::query("UPDATE ignored_conversations SET partial=1 WHERE id=?")
                .bind(conversation_id)
                .execute(&mut *tx)
                .await?;
        }
        // S-022: задание создаётся до первой пачки и хранит ключ снимка.
        sqlx::query(
            "INSERT INTO ignored_conversation_jobs(conversation_id, kind, snapshot_key, max_message_id)
             VALUES(?, 'sweep', ?, ?)",
        )
        .bind(conversation_id)
        .bind(snapshot_key)
        .bind(max_message_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.advance_ignored_conversation_jobs().await?;
        self.ignored_conversation(conversation_id).await
    }

    /// Идентификатор относится к набору, построенному обходом локальных писем.
    fn id_belongs(set: &ConversationSet, id: &str) -> bool {
        set.local_ids.contains(id)
    }

    /// Перенести идентификаторы, перемещения и задания второй записи в первую
    /// и удалить вторую (S-045).
    async fn merge_conversations(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        keep: i64,
        drop: i64,
    ) -> Result<()> {
        sqlx::query("UPDATE OR IGNORE ignored_conversation_ids SET conversation_id=? WHERE conversation_id=?")
            .bind(keep)
            .bind(drop)
            .execute(&mut **tx)
            .await?;
        sqlx::query("UPDATE OR IGNORE ignored_conversation_moves SET conversation_id=? WHERE conversation_id=?")
            .bind(keep)
            .bind(drop)
            .execute(&mut **tx)
            .await?;
        sqlx::query(
            "UPDATE ignored_conversation_jobs SET conversation_id=? WHERE conversation_id=?",
        )
        .bind(keep)
        .bind(drop)
        .execute(&mut **tx)
        .await?;
        sqlx::query("DELETE FROM ignored_conversations WHERE id=?")
            .bind(drop)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    /// Сохранить идентификаторы набора. Уникальный ключ ящика и идентификатора
    /// не даёт двум записям владеть одним идентификатором.
    async fn store_conversation_ids(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        conversation_id: i64,
        account_id: i64,
        ids: &[String],
    ) -> Result<()> {
        for id in ids.iter().take(MAX_CONVERSATION_IDS) {
            sqlx::query(
                "INSERT OR IGNORE INTO ignored_conversation_ids(conversation_id, account_id, message_id)
                 VALUES(?, ?, ?)",
            )
            .bind(conversation_id)
            .bind(account_id)
            .bind(id)
            .execute(&mut **tx)
            .await?;
        }
        Ok(())
    }

    /// Список игнорируемых переписок для раздела настроек (S-031).
    pub async fn list_ignored_conversations(&self) -> Result<Vec<IgnoredConversation>> {
        let rows: Vec<IgnoredConversation> = sqlx::query_as(LIST_SQL).fetch_all(&self.pool).await?;
        Ok(rows)
    }

    pub async fn ignored_conversation(&self, id: i64) -> Result<IgnoredConversation> {
        let sql = format!("{LIST_SQL} AND c.id=?");
        let row: IgnoredConversation = sqlx::query_as(AssertSqlSafe(sql))
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        Ok(row)
    }

    /// Прекратить игнорирование с возвратом писем или без него (S-033, S-034,
    /// S-036).
    pub async fn disable_ignore_conversation(
        &self,
        conversation_id: i64,
        return_messages: bool,
    ) -> Result<IgnoreJobReport> {
        let mut tx = self.begin_write().await?;
        let found: Option<(String,)> =
            sqlx::query_as("SELECT state FROM ignored_conversations WHERE id=?")
                .bind(conversation_id)
                .fetch_optional(&mut *tx)
                .await?;
        if found.is_none() {
            return Err(crate::Error::AccountConfig(
                "игнорируемая переписка не найдена".into(),
            ));
        }
        sqlx::query(
            "UPDATE ignored_conversations SET state='disabling', updated_at=datetime('now')
              WHERE id=?",
        )
        .bind(conversation_id)
        .execute(&mut *tx)
        .await?;
        let marker = ignore_marker(conversation_id);
        // S-036: уже выполняющиеся перемещения отменить нельзя, их число
        // называется пользователю.
        let irreversible: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM outbox_ops
              WHERE json_extract(payload, '$.rule_id')=? AND status NOT IN ('pending','retry')",
        )
        .bind(&marker)
        .fetch_one(&mut *tx)
        .await?;
        let cancelled = if return_messages {
            sqlx::query(
                "DELETE FROM outbox_ops
                  WHERE json_extract(payload, '$.rule_id')=? AND status IN ('pending','retry')",
            )
            .bind(&marker)
            .execute(&mut *tx)
            .await?
            .rows_affected() as i64
        } else {
            0
        };
        if !return_messages {
            sqlx::query(
                "UPDATE ignored_conversations SET state='disabled', updated_at=datetime('now')
                  WHERE id=?",
            )
            .bind(conversation_id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok(IgnoreJobReport {
                conversation_id,
                kind: "return".into(),
                state: IGNORE_STATE_DISABLED.into(),
                irreversible: irreversible.0,
                ..IgnoreJobReport::default()
            });
        }
        // S-021: письмо без заголовка Message-ID опознать в корзине нечем,
        // поэтому его возврат помечается пропущенным заранее.
        sqlx::query(
            "UPDATE ignored_conversation_moves
                SET return_state='skipped', updated_at=datetime('now')
              WHERE conversation_id=? AND (header_id IS NULL OR header_id='')",
        )
        .bind(conversation_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE ignored_conversation_moves
                SET return_state='pending', return_requested_at=datetime('now'),
                    updated_at=datetime('now')
              WHERE conversation_id=? AND header_id IS NOT NULL AND header_id<>''
                AND return_state NOT IN ('completed','skipped')",
        )
        .bind(conversation_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE ignored_conversations SET state='returning', updated_at=datetime('now')
              WHERE id=?",
        )
        .bind(conversation_id)
        .execute(&mut *tx)
        .await?;
        let job: (i64,) = sqlx::query_as(
            "INSERT INTO ignored_conversation_jobs(conversation_id, kind) VALUES(?, 'return')
             RETURNING id",
        )
        .bind(conversation_id)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        let mut report = self.continue_ignored_conversation_job(job.0).await?;
        report.irreversible = irreversible.0;
        // Отменённые перемещения возвращать уже не нужно: письмо осталось на
        // месте, поэтому в счётчик возвращённых они не попадают (S-036).
        tracing::info!(
            conversation_id,
            cancelled,
            "прекращение игнорирования: неисполненные перемещения отменены"
        );
        Ok(report)
    }

    /// Один проход задания: уборка текущих писем или возврат убранных.
    pub async fn continue_ignored_conversation_job(&self, job_id: i64) -> Result<IgnoreJobReport> {
        let kind: Option<(String,)> =
            sqlx::query_as("SELECT kind FROM ignored_conversation_jobs WHERE id=?")
                .bind(job_id)
                .fetch_optional(&self.pool)
                .await?;
        let Some((kind,)) = kind else {
            return Err(crate::Error::AccountConfig("задание не найдено".into()));
        };
        if kind == "return" {
            self.run_return_batch(job_id).await
        } else {
            self.run_ignore_sweep_batch(job_id).await
        }
    }

    /// Пачка уборки текущих писем переписки: не больше 500 писем за проход
    /// (S-016, S-023, S-024).
    async fn run_ignore_sweep_batch(&self, job_id: i64) -> Result<IgnoreJobReport> {
        let mut tx = self.begin_write().await?;
        let job: Option<(i64, i64, i64, String)> = sqlx::query_as(
            "SELECT conversation_id, max_message_id, cursor_message_id, state
               FROM ignored_conversation_jobs WHERE id=?",
        )
        .bind(job_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((conversation_id, max_message_id, cursor, state)) = job else {
            return Err(crate::Error::AccountConfig("задание не найдено".into()));
        };
        if matches!(state.as_str(), "completed" | "cancelled" | "failed") {
            return self.ignore_job_report(job_id).await;
        }
        let account: (i64,) =
            sqlx::query_as("SELECT account_id FROM ignored_conversations WHERE id=?")
                .bind(conversation_id)
                .fetch_one(&mut *tx)
                .await?;
        let account_id = account.0;
        sqlx::query(
            "UPDATE ignored_conversation_jobs SET state='running', updated_at=datetime('now')
              WHERE id=?",
        )
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
        let trash = resolve_role_folder(&mut tx, account_id, "trash").await?;
        let sql = format!(
            "SELECT {STAGE_MESSAGE_COLUMNS}
               FROM messages m JOIN folders f ON f.id=m.folder_id
              WHERE m.account_id=? AND m.id>? AND m.id<=? AND {IGNORE_WORKING_FOLDERS}
                AND {CONVERSATION_MEMBERSHIP}
              ORDER BY m.id LIMIT ?"
        );
        let batch = sqlx::query_as::<_, StageMessage>(AssertSqlSafe(sql))
            .bind(account_id)
            .bind(cursor)
            .bind(max_message_id)
            .bind(conversation_id)
            .bind(conversation_id)
            .bind(conversation_id)
            .bind(STAGE_BATCH)
            .fetch_all(&mut *tx)
            .await?;
        let mut counters = StageCounters::default();
        let mut last_id = cursor;
        for message in &batch {
            last_id = message.id;
            let Some((folder_id, path)) = trash.clone() else {
                // S-003: ящик без корзины оставляет письмо на месте, а причина
                // видна в списке игнорируемых переписок.
                counters.failed += 1;
                sqlx::query(
                    "UPDATE ignored_conversations SET last_error=?, updated_at=datetime('now')
                      WHERE id=?",
                )
                .bind("в ящике нет папки с ролью корзины")
                .bind(conversation_id)
                .execute(&mut *tx)
                .await?;
                sqlx::query("UPDATE messages SET closed_by_stage=? WHERE id=?")
                    .bind(IGNORED_CONVERSATION_STAGE_NAME)
                    .bind(message.id)
                    .execute(&mut *tx)
                    .await?;
                continue;
            };
            let outcome = queue_takeaway_operation(
                &mut tx,
                &message.takeaway(),
                &TakeawayTarget::Folder {
                    id: folder_id,
                    path,
                },
                TakeawayActor::Stage,
                Some(ignore_marker(conversation_id).as_str()),
                0,
            )
            .await?;
            if counters.account(&outcome) {
                Self::remember_move(&mut tx, conversation_id, message, &outcome).await?;
                sqlx::query("UPDATE messages SET closed_by_stage=? WHERE id=?")
                    .bind(IGNORED_CONVERSATION_STAGE_NAME)
                    .bind(message.id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        let remaining_sql = format!(
            "SELECT count(*) FROM messages m JOIN folders f ON f.id=m.folder_id
              WHERE m.account_id=? AND m.id>? AND m.id<=? AND {IGNORE_WORKING_FOLDERS}
                AND {CONVERSATION_MEMBERSHIP}"
        );
        let remaining: (i64,) = sqlx::query_as(AssertSqlSafe(remaining_sql))
            .bind(account_id)
            .bind(last_id)
            .bind(max_message_id)
            .bind(conversation_id)
            .bind(conversation_id)
            .bind(conversation_id)
            .fetch_one(&mut *tx)
            .await?;
        let next_state = if remaining.0 > 0 {
            "pending"
        } else {
            "completed"
        };
        sqlx::query(
            "UPDATE ignored_conversation_jobs
                SET state=?, cursor_message_id=?, queued=queued+?, skipped=skipped+?,
                    failed=failed+?, remaining=?, updated_at=datetime('now')
              WHERE id=?",
        )
        .bind(next_state)
        .bind(last_id)
        .bind(counters.queued)
        .bind(counters.skipped)
        .bind(counters.failed)
        .bind(remaining.0)
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.ignore_job_report(job_id).await
    }

    /// Запомнить исходную папку и приметы письма до перемещения (S-020).
    async fn remember_move(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        conversation_id: i64,
        message: &StageMessage,
        outcome: &TakeawayOutcome,
    ) -> Result<()> {
        let operation_id = match outcome {
            TakeawayOutcome::Queued(id) => Some(*id),
            _ => None,
        };
        let header_id = message
            .rfc822_message_id
            .as_deref()
            .and_then(normalize_message_id);
        sqlx::query(
            "INSERT OR IGNORE INTO ignored_conversation_moves(
                conversation_id, message_id, source_folder_id, from_addr, message_date,
                header_id, operation_id, move_state,
                return_state)
             VALUES(?, ?, ?, ?, ?, ?, ?, 'queued', ?)",
        )
        .bind(conversation_id)
        .bind(message.id)
        .bind(message.folder_id)
        .bind(message.from_addr.as_deref())
        .bind(message.date.as_deref())
        .bind(header_id.as_deref())
        .bind(operation_id)
        // S-021: без заголовка Message-ID возврат обещать нечем.
        .bind(if header_id.is_some() {
            RETURN_STATE_PENDING
        } else {
            RETURN_STATE_SKIPPED
        })
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Пачка возврата: письмо ищется в корзине того же ящика по приметам и
    /// ставится в очередь на перемещение в свою сохранённую исходную папку
    /// (S-037 - S-041).
    async fn run_return_batch(&self, job_id: i64) -> Result<IgnoreJobReport> {
        let mut tx = self.begin_write().await?;
        let job: Option<(i64, String)> = sqlx::query_as(
            "SELECT conversation_id, state FROM ignored_conversation_jobs WHERE id=?",
        )
        .bind(job_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((conversation_id, state)) = job else {
            return Err(crate::Error::AccountConfig("задание не найдено".into()));
        };
        if matches!(state.as_str(), "completed" | "cancelled" | "failed") {
            return self.ignore_job_report(job_id).await;
        }
        let account: (i64,) =
            sqlx::query_as("SELECT account_id FROM ignored_conversations WHERE id=?")
                .bind(conversation_id)
                .fetch_one(&mut *tx)
                .await?;
        let account_id = account.0;
        sqlx::query(
            "UPDATE ignored_conversation_jobs SET state='running', updated_at=datetime('now')
              WHERE id=?",
        )
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
        let inbox = resolve_role_folder(&mut tx, account_id, "inbox").await?;
        let pending: Vec<PendingReturnRow> = sqlx::query_as(
            "SELECT id, source_folder_id, from_addr, message_date, header_id,
                    return_requested_at
               FROM ignored_conversation_moves
              WHERE conversation_id=? AND return_state IN ('pending','waiting_sync')
              ORDER BY id LIMIT ?",
        )
        .bind(conversation_id)
        .bind(STAGE_BATCH)
        .fetch_all(&mut *tx)
        .await?;
        let mut counters = StageCounters::default();
        let mut waiting = 0i64;
        for row in pending {
            let (move_id, source_folder_id, header_id) =
                (row.id, row.source_folder_id, row.header_id.clone());
            let (from_addr, message_date, requested_at) =
                (row.from_addr, row.message_date, row.return_requested_at);
            let found: Option<(i64, i64, i64, String, Option<String>)> = sqlx::query_as(
                "SELECT m.id, m.folder_id, m.uid, f.remote_path, m.remote_id
                   FROM messages m JOIN folders f ON f.id=m.folder_id
                  WHERE m.account_id=? AND f.role='trash'
                    AND trim(coalesce(m.rfc822_message_id,''), '<> ')=?
                    AND coalesce(m.from_addr,'')=coalesce(?,'')
                    AND coalesce(m.date,'')=coalesce(?,'')
                  ORDER BY m.id LIMIT 1",
            )
            .bind(account_id)
            .bind(header_id.as_deref())
            .bind(from_addr.as_deref())
            .bind(message_date.as_deref())
            .fetch_optional(&mut *tx)
            .await?;
            let Some((message_id, folder_id, uid, remote_path, remote_id)) = found else {
                // Письмо не опознано в корзине. Пока идёт ожидание, поиск
                // повторяется после следующей синхронизации; по истечении
                // срока возврат считается неполным (S-041 - S-043).
                let expired: (i64,) = sqlx::query_as(
                    "SELECT CASE WHEN ? IS NOT NULL
                                  AND datetime(?) <= datetime('now', ?) THEN 1 ELSE 0 END",
                )
                .bind(requested_at.as_deref())
                .bind(requested_at.as_deref())
                .bind(format!("-{RETURN_WAIT_DAYS} days"))
                .fetch_one(&mut *tx)
                .await?;
                // S-038: письмо, которое так и осталось в своей папке,
                // возвращать не нужно - его возврат пропускается со счётчиком.
                let still_in_place: (i64,) = sqlx::query_as(
                    "SELECT count(*) FROM messages
                      WHERE account_id=?
                        AND trim(coalesce(rfc822_message_id,''), '<> ')=?
                        AND folder_id=coalesce(?, folder_id)",
                )
                .bind(account_id)
                .bind(header_id.as_deref())
                .bind(source_folder_id)
                .fetch_one(&mut *tx)
                .await?;
                let next = if still_in_place.0 > 0 {
                    counters.skipped += 1;
                    RETURN_STATE_SKIPPED
                } else if expired.0 == 1 {
                    counters.failed += 1;
                    RETURN_STATE_FAILED
                } else {
                    waiting += 1;
                    RETURN_STATE_WAITING_SYNC
                };
                sqlx::query(
                    "UPDATE ignored_conversation_moves SET return_state=?,
                            updated_at=datetime('now') WHERE id=?",
                )
                .bind(next)
                .bind(move_id)
                .execute(&mut *tx)
                .await?;
                continue;
            };
            // S-039, S-040: письмо возвращается в свою исходную папку, а если
            // её больше нет - во входящие того же ящика.
            let target: Option<(i64, String)> = match source_folder_id {
                Some(id) => {
                    sqlx::query_as(
                        "SELECT id, remote_path FROM folders WHERE id=? AND account_id=?",
                    )
                    .bind(id)
                    .bind(account_id)
                    .fetch_optional(&mut *tx)
                    .await?
                }
                None => None,
            };
            let target = target.or_else(|| inbox.clone());
            let Some((target_id, target_path)) = target else {
                counters.failed += 1;
                sqlx::query(
                    "UPDATE ignored_conversation_moves SET return_state='failed',
                            updated_at=datetime('now') WHERE id=?",
                )
                .bind(move_id)
                .execute(&mut *tx)
                .await?;
                continue;
            };
            let outcome = queue_takeaway_operation(
                &mut tx,
                &crate::storage::repo::TakeawayMessage {
                    id: message_id,
                    account_id,
                    folder_id,
                    uid,
                    remote_path,
                    remote_id,
                },
                &TakeawayTarget::Folder {
                    id: target_id,
                    path: target_path,
                },
                TakeawayActor::Stage,
                Some(ignore_marker(conversation_id).as_str()),
                0,
            )
            .await?;
            let returned = counters.account(&outcome);
            sqlx::query(
                "UPDATE ignored_conversation_moves SET return_state=?, updated_at=datetime('now')
                  WHERE id=?",
            )
            .bind(if returned {
                RETURN_STATE_COMPLETED
            } else {
                RETURN_STATE_WAITING_SYNC
            })
            .bind(move_id)
            .execute(&mut *tx)
            .await?;
            if returned {
                // Возвращённое письмо снова доступно стадиям: признак закрывшей
                // стадии с него снимается.
                sqlx::query("UPDATE messages SET closed_by_stage=NULL WHERE id=?")
                    .bind(message_id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        let failed: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM ignored_conversation_moves
              WHERE conversation_id=? AND return_state='failed'",
        )
        .bind(conversation_id)
        .fetch_one(&mut *tx)
        .await?;
        let next_state = if waiting > 0 { "pending" } else { "completed" };
        sqlx::query(
            "UPDATE ignored_conversation_jobs
                SET state=?, queued=queued+?, skipped=skipped+?, failed=failed+?, remaining=?,
                    updated_at=datetime('now')
              WHERE id=?",
        )
        .bind(next_state)
        .bind(counters.queued)
        .bind(counters.skipped)
        .bind(counters.failed)
        .bind(waiting)
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
        // S-043: неполный возврат оставляет запись в состоянии return_failed и
        // включить игнорирование заново не даёт.
        let record_state = if failed.0 > 0 {
            IGNORE_STATE_RETURN_FAILED
        } else if waiting > 0 {
            IGNORE_STATE_RETURNING
        } else {
            IGNORE_STATE_DISABLED
        };
        sqlx::query(
            "UPDATE ignored_conversations SET state=?, updated_at=datetime('now') WHERE id=?",
        )
        .bind(record_state)
        .bind(conversation_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.ignore_job_report(job_id).await
    }

    pub async fn ignore_job_report(&self, job_id: i64) -> Result<IgnoreJobReport> {
        let report: IgnoreJobReport = sqlx::query_as(
            "SELECT id, conversation_id, kind, state, queued, skipped, failed, remaining,
                    0 AS irreversible
               FROM ignored_conversation_jobs WHERE id=?",
        )
        .bind(job_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(report)
    }

    /// Незавершённые задания игнорирования для раздела настроек.
    pub async fn pending_ignore_jobs(&self) -> Result<Vec<IgnoreJobReport>> {
        let rows: Vec<IgnoreJobReport> = sqlx::query_as(
            "SELECT id, conversation_id, kind, state, queued, skipped, failed, remaining,
                    0 AS irreversible
               FROM ignored_conversation_jobs WHERE state IN ('pending','running') ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Вернуть задания, брошенные закрытием программы, в состояние ожидания
    /// (S-048).
    pub(crate) async fn restore_ignore_jobs(&self) -> Result<()> {
        sqlx::query(
            "UPDATE ignored_conversation_jobs SET state='pending', updated_at=datetime('now')
              WHERE state='running'",
        )
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// Продвинуть незавершённые задания на одну пачку.
    pub(crate) async fn advance_ignored_conversation_jobs(&self) -> Result<()> {
        let jobs: Vec<(i64,)> = sqlx::query_as(
            "SELECT id FROM ignored_conversation_jobs WHERE state='pending' ORDER BY id LIMIT 4",
        )
        .fetch_all(&self.pool)
        .await?;
        for (job_id,) in jobs {
            self.continue_ignored_conversation_job(job_id).await?;
        }
        Ok(())
    }

    /// Стадия игнорируемых переписок: вторая в сквозном порядке стадий и
    /// первая на пути догрузки писем прокруткой (S-001, S-025, S-026).
    pub(crate) async fn process_ignored_conversation_stage(&self) -> Result<usize> {
        let mut tx = self.begin_write().await?;
        let enabled: (i64,) =
            sqlx::query_as("SELECT count(*) FROM ignored_conversations WHERE state='enabled'")
                .fetch_one(&mut *tx)
                .await?;
        let cursor = Self::stage_cursor(&mut tx, IGNORED_CONVERSATION_STAGE_NAME).await?;
        if enabled.0 == 0 {
            let newest = Self::max_message_id(&mut tx).await?;
            Self::set_stage_cursor(&mut tx, IGNORED_CONVERSATION_STAGE_NAME, newest).await?;
            tx.commit().await?;
            return Ok(0);
        }
        // Папка отправленных берётся вместе с рабочими: собственное письмо в
        // ней снимает игнорирование переписки (S-030).
        let sql = format!(
            "SELECT {STAGE_MESSAGE_COLUMNS}
               FROM messages m JOIN folders f ON f.id=m.folder_id
              WHERE m.id>? AND m.closed_by_stage IS NULL
                AND (f.role IS NULL OR f.role NOT IN ('drafts','archive','spam','trash'))
              ORDER BY m.id LIMIT ?"
        );
        let batch = sqlx::query_as::<_, StageMessage>(AssertSqlSafe(sql))
            .bind(cursor)
            .bind(STAGE_BATCH)
            .fetch_all(&mut *tx)
            .await?;
        let mut queued = 0usize;
        let mut last_id = cursor;
        let mut trash_cache: HashMap<i64, Option<(i64, String)>> = HashMap::new();
        for message in &batch {
            last_id = message.id;
            let ids = message_identifiers(
                message.rfc822_message_id.as_deref(),
                message.in_reply_to.as_deref(),
                message.references_ids.as_deref(),
            );
            if ids.is_empty() {
                continue;
            }
            let Some((conversation_id, state)) =
                Self::conversation_for_ids(&mut tx, message.account_id, &ids).await?
            else {
                continue;
            };
            if state != IGNORE_STATE_ENABLED {
                continue;
            }
            // S-030: письмо в папке отправленных выключает игнорирование
            // независимо от того, с какого устройства оно отправлено.
            if message.folder_role.as_deref() == Some("sent") {
                sqlx::query(
                    "UPDATE ignored_conversations SET state='disabled', last_error=?,
                            updated_at=datetime('now') WHERE id=?",
                )
                .bind("игнорирование снято: в переписке появилось собственное письмо")
                .bind(conversation_id)
                .execute(&mut *tx)
                .await?;
                continue;
            }
            // S-027: набор расширяется идентификаторами нового письма.
            Self::store_conversation_ids(&mut tx, conversation_id, message.account_id, &ids)
                .await?;
            Self::mark_partial_if_full(&mut tx, conversation_id).await?;
            let trash = match trash_cache.entry(message.account_id) {
                std::collections::hash_map::Entry::Occupied(found) => found.get().clone(),
                std::collections::hash_map::Entry::Vacant(empty) => {
                    let found = resolve_role_folder(&mut tx, message.account_id, "trash").await?;
                    empty.insert(found.clone());
                    found
                }
            };
            let Some((folder_id, path)) = trash else {
                // S-003: письмо остаётся на месте, следующим стадиям не
                // передаётся и в уведомление не попадает.
                sqlx::query("UPDATE messages SET closed_by_stage=? WHERE id=?")
                    .bind(IGNORED_CONVERSATION_STAGE_NAME)
                    .bind(message.id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(
                    "UPDATE ignored_conversations SET last_error=?, updated_at=datetime('now')
                      WHERE id=?",
                )
                .bind("в ящике нет папки с ролью корзины")
                .bind(conversation_id)
                .execute(&mut *tx)
                .await?;
                continue;
            };
            let outcome = queue_takeaway_operation(
                &mut tx,
                &message.takeaway(),
                &TakeawayTarget::Folder {
                    id: folder_id,
                    path,
                },
                TakeawayActor::Stage,
                Some(ignore_marker(conversation_id).as_str()),
                0,
            )
            .await?;
            if matches!(outcome, TakeawayOutcome::Queued(_)) {
                Self::remember_move(&mut tx, conversation_id, message, &outcome).await?;
                sqlx::query("UPDATE messages SET closed_by_stage=? WHERE id=?")
                    .bind(IGNORED_CONVERSATION_STAGE_NAME)
                    .bind(message.id)
                    .execute(&mut *tx)
                    .await?;
                queued += 1;
            }
        }
        Self::set_stage_cursor(&mut tx, IGNORED_CONVERSATION_STAGE_NAME, last_id).await?;
        tx.commit().await?;
        Ok(queued)
    }

    /// Запись, которой принадлежит хотя бы один идентификатор письма.
    async fn conversation_for_ids(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        account_id: i64,
        ids: &[String],
    ) -> Result<Option<(i64, String)>> {
        for chunk in ids.chunks(200) {
            let placeholders = vec!["?"; chunk.len()].join(",");
            let sql = format!(
                "SELECT c.id, c.state FROM ignored_conversations c
                   JOIN ignored_conversation_ids i ON i.conversation_id=c.id
                  WHERE i.account_id=? AND i.message_id IN ({placeholders})
                  ORDER BY c.id LIMIT 1"
            );
            let mut query = sqlx::query_as::<_, (i64, String)>(AssertSqlSafe(sql)).bind(account_id);
            for id in chunk {
                query = query.bind(id);
            }
            if let Some(found) = query.fetch_optional(&mut **tx).await? {
                return Ok(Some(found));
            }
        }
        Ok(None)
    }

    /// Набор, добравший предел идентификаторов, переводится в состояние
    /// частичного покрытия (S-028).
    async fn mark_partial_if_full(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        conversation_id: i64,
    ) -> Result<()> {
        let count: (i64,) =
            sqlx::query_as("SELECT count(*) FROM ignored_conversation_ids WHERE conversation_id=?")
                .bind(conversation_id)
                .fetch_one(&mut **tx)
                .await?;
        if count.0 >= MAX_CONVERSATION_IDS as i64 {
            sqlx::query(
                "UPDATE ignored_conversations SET partial=1, updated_at=datetime('now') WHERE id=?",
            )
            .bind(conversation_id)
            .execute(&mut **tx)
            .await?;
        }
        Ok(())
    }
}

/// Принадлежность письма набору переписки: собственный идентификатор, ссылка
/// на родителя или элемент References. `thread_id` служит только ускорению
/// поиска кандидатов и принадлежности не определяет (S-012).
const CONVERSATION_MEMBERSHIP: &str = "(EXISTS(SELECT 1 FROM ignored_conversation_ids c
              WHERE c.conversation_id=?
                AND c.message_id=trim(coalesce(m.rfc822_message_id,''), '<> '))
      OR EXISTS(SELECT 1 FROM ignored_conversation_ids c
                 WHERE c.conversation_id=?
                   AND c.message_id=trim(coalesce(m.in_reply_to,''), '<> '))
      OR EXISTS(SELECT 1 FROM ignored_conversation_ids c
                 WHERE c.conversation_id=? AND m.references_ids IS NOT NULL
                   AND instr(m.references_ids, c.message_id)>0))";

/// Список игнорируемых переписок со сводкой перемещений и возвратов (S-031).
const LIST_SQL: &str = "SELECT c.id, c.account_id, a.email AS account_email, c.subject,
            c.participants, c.state, c.partial, c.created_at, c.updated_at,
            (SELECT count(*) FROM ignored_conversation_ids i WHERE i.conversation_id=c.id) AS ids_count,
            (SELECT count(*) FROM ignored_conversation_moves m WHERE m.conversation_id=c.id) AS moved,
            (SELECT count(*) FROM ignored_conversation_moves m
              WHERE m.conversation_id=c.id AND m.return_state='completed') AS returned,
            (SELECT count(*) FROM ignored_conversation_moves m
              WHERE m.conversation_id=c.id AND m.return_state='skipped') AS skipped,
            (SELECT count(*) FROM ignored_conversation_moves m
              WHERE m.conversation_id=c.id AND m.return_state='failed') AS failed,
            c.last_error
       FROM ignored_conversations c JOIN accounts a ON a.id=c.account_id
      WHERE 1=1";
