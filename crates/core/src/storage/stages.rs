//! Общая обвязка стадий разбора письма: состав рабочих папок, курсор стадии,
//! снимки кандидатов уборки и снимок письма, которого хватает стадии.
//!
//! Сквозной порядок стадий - списки отправителей, игнорируемые переписки,
//! автоочистка по отправителю и только потом правила обработки
//! (specs/blocked-senders.md, specs/ignore-conversation.md,
//! specs/sweep-by-sender.md, specs/mail-rules-conditions-and-actions.md).

use super::Db;
use crate::Result;
use crate::model::*;
use crate::storage::repo::{TakeawayMessage, TakeawayOutcome};
use sqlx::AssertSqlSafe;

/// Рабочие папки: папка с ролью `inbox` и папка без роли. Папки с ролями
/// `sent`, `drafts`, `spam` и `trash` рабочими не считаются ни в одной стадии,
/// а `archive` берётся только по отдельному согласию пользователя и только
/// автоочисткой.
pub(crate) const WORKING_FOLDERS: &str =
    "(f.role IS NULL OR f.role NOT IN ('sent','drafts','archive','spam','trash'))";

/// Рабочие папки вместе с архивом: согласие на архив даёт только автоочистка
/// по отправителю (specs/sweep-by-sender.md, S-016).
pub(crate) const WORKING_FOLDERS_WITH_ARCHIVE: &str =
    "(f.role IS NULL OR f.role NOT IN ('sent','drafts','spam','trash'))";

/// Условие отбора писем, отложенных этим заданием: они повторяются наравне с
/// письмами после курсора, поэтому проход к ним возвращается.
pub(crate) const DEFERRED_MESSAGES: &str =
    "m.id IN (SELECT message_id FROM stage_job_deferrals WHERE kind=? AND job_id=?)";

/// Исход постановки увода требует повторить письмо следующим проходом: чужая
/// незавершённая операция и отказ очереди сами по себе не значат, что письмо
/// убирать не нужно.
pub(crate) fn needs_retry(outcome: &TakeawayOutcome) -> bool {
    matches!(
        outcome,
        TakeawayOutcome::Conflict | TakeawayOutcome::Busy | TakeawayOutcome::Failed
    )
}

/// Данные письма, которых хватает стадии: постановка увода, сверка отправителя
/// и опознание переписки. Тело письма при этом не загружается.
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct StageMessage {
    pub id: i64,
    pub account_id: i64,
    pub folder_id: i64,
    pub uid: i64,
    pub remote_path: String,
    pub remote_id: Option<String>,
    pub folder_role: Option<String>,
    pub from_addr: Option<String>,
    pub date: Option<String>,
    pub rfc822_message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references_ids: Option<String>,
}

impl StageMessage {
    pub fn takeaway(&self) -> TakeawayMessage {
        TakeawayMessage {
            id: self.id,
            account_id: self.account_id,
            folder_id: self.folder_id,
            uid: self.uid,
            remote_path: self.remote_path.clone(),
            remote_id: self.remote_id.clone(),
        }
    }
}

/// Столбцы снимка письма: перечислены один раз, чтобы три стадии читали письмо
/// одинаково.
pub(crate) const STAGE_MESSAGE_COLUMNS: &str =
    "m.id, m.account_id, m.folder_id, m.uid, f.remote_path, m.remote_id, f.role AS folder_role,
     m.from_addr, m.date, m.rfc822_message_id, m.in_reply_to, m.references_ids";

/// Счётчики одного прохода стадии или уборки.
#[derive(Debug, Clone, Default)]
pub(crate) struct StageCounters {
    pub queued: i64,
    pub skipped: i64,
    pub failed: i64,
}

impl StageCounters {
    /// Учесть исход постановки увода. Ни один исход не прерывает проход: одно
    /// занятое письмо не отменяет уборку остальных
    /// (specs/mail-rules-conditions-and-actions.md, S-005).
    pub fn account(&mut self, outcome: &TakeawayOutcome) -> bool {
        match outcome {
            TakeawayOutcome::Queued(_) => {
                self.queued += 1;
                true
            }
            TakeawayOutcome::Failed => {
                self.failed += 1;
                false
            }
            TakeawayOutcome::Conflict | TakeawayOutcome::Busy | TakeawayOutcome::Unchanged => {
                self.skipped += 1;
                false
            }
            TakeawayOutcome::NeedsAttention => {
                self.skipped += 1;
                false
            }
        }
    }
}

impl Db {
    /// Номер письма, на котором стадия остановилась в прошлый раз. Курсор
    /// заведён по той же причине, что и прогресс правила: без него стадия
    /// заново разбирала бы всю историю писем на каждой синхронизации.
    pub(crate) async fn stage_cursor(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        stage: &str,
    ) -> Result<i64> {
        let stored: Option<(i64,)> =
            sqlx::query_as("SELECT message_id FROM stage_progress WHERE stage=?")
                .bind(stage)
                .fetch_optional(&mut **tx)
                .await?;
        Ok(stored.map(|(value,)| value).unwrap_or(0))
    }

    /// Двинуть курсор стадии вперёд. Назад он не откатывается: письмо, уже
    /// разобранное стадией, второй раз ей не достаётся.
    pub(crate) async fn set_stage_cursor(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        stage: &str,
        message_id: i64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO stage_progress(stage, message_id, updated_at)
             VALUES(?, ?, datetime('now'))
             ON CONFLICT(stage) DO UPDATE SET
                message_id=max(message_id, excluded.message_id),
                updated_at=datetime('now')",
        )
        .bind(stage)
        .bind(message_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Начальное значение курсора каждой стадии - наибольший номер письма в
    /// базе. Без него запись, заведённая до первой успешной синхронизации,
    /// досталась бы стадии вместе со всей историей писем и увела бы в корзину
    /// письма, на уборку которых пользователь согласия не давал. Шаг
    /// прикладной, а не частью миграции: уже применённая миграция не меняется.
    pub(crate) async fn seed_stage_cursors(&self) -> Result<()> {
        let mut tx = self.begin_write().await?;
        for stage in [
            SENDER_POLICY_STAGE_NAME,
            IGNORED_CONVERSATION_STAGE_NAME,
            SENDER_SWEEP_STAGE_NAME,
            // Стадия автоответа заводит курсор по той же причине: настройка
            // отсутствия, включённая до первой синхронизации, иначе получила бы
            // на разбор всю историю писем (specs/out-of-office.md, S-030).
            OUT_OF_OFFICE_STAGE_NAME,
        ] {
            // Строка заводится один раз: у работающей стадии значение своё, и
            // назад курсор не двигается.
            sqlx::query(
                "INSERT OR IGNORE INTO stage_progress(stage, message_id)
                 SELECT ?, coalesce(max(id), 0) FROM messages",
            )
            .bind(stage)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Зафиксировать набор писем-кандидатов и вернуть ключ снимка. Хранятся
    /// сами номера писем, а не только их граница: по подтверждению в корзину
    /// уходят ровно те письма, которые были показаны пользователю.
    pub(crate) async fn save_stage_snapshot(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        kind: &str,
        payload: &str,
        max_message_id: i64,
        candidates: &[i64],
        snapshot_hours: i64,
    ) -> Result<String> {
        Self::purge_stale_snapshots_in_tx(tx, Some(kind), snapshot_hours).await?;
        let key = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO stage_snapshots(key, kind, payload, max_message_id, total)
             VALUES(?, ?, ?, ?, ?)",
        )
        .bind(&key)
        .bind(kind)
        .bind(payload)
        .bind(max_message_id)
        .bind(candidates.len() as i64)
        .execute(&mut **tx)
        .await?;
        for chunk in candidates.chunks(200) {
            let values = vec!["(?, ?)"; chunk.len()].join(",");
            let sql = format!(
                "INSERT OR IGNORE INTO stage_snapshot_messages(key, message_id) VALUES {values}"
            );
            let mut query = sqlx::query(AssertSqlSafe(sql));
            for id in chunk {
                query = query.bind(&key).bind(id);
            }
            query.execute(&mut **tx).await?;
        }
        Ok(key)
    }

    /// Израсходовать снимок: строка снимка удаляется тем же запросом, которым
    /// читается. Одно подтверждение пользователя запускает уборку один раз,
    /// даже если команда пришла дважды. Строки кандидатов остаются заданию.
    pub(crate) async fn consume_stage_snapshot(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        key: &str,
        kind: &str,
    ) -> Result<(String, i64, i64)> {
        let row: Option<(String, i64, i64)> = sqlx::query_as(
            "DELETE FROM stage_snapshots WHERE key=? AND kind=?
             RETURNING payload, max_message_id, total",
        )
        .bind(key)
        .bind(kind)
        .fetch_optional(&mut **tx)
        .await?;
        row.ok_or_else(|| {
            crate::Error::AccountConfig(
                "список писем устарел, откройте подтверждение заново".into(),
            )
        })
    }

    /// Убрать снимки, которых никто не подтвердил, и осиротевшие строки
    /// кандидатов: диалог открывают часто, а подтверждают редко, и без уборки
    /// строки копились бы навсегда.
    pub(crate) async fn purge_stale_stage_snapshots(&self) -> Result<()> {
        let mut tx = self.begin_write().await?;
        Self::purge_stale_snapshots_in_tx(&mut tx, None, self.limit(LIMIT_STAGE_SNAPSHOT_HOURS))
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Срок жизни снимка приходит значением, а не читается здесь: обе ветки
    /// уборки работают внутри чужой транзакции, а база настроек за ней не
    /// видна.
    async fn purge_stale_snapshots_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        kind: Option<&str>,
        snapshot_hours: i64,
    ) -> Result<()> {
        let border = format!("-{snapshot_hours} hours");
        match kind {
            Some(kind) => {
                sqlx::query(
                    "DELETE FROM stage_snapshots
                      WHERE kind=? AND datetime(created_at) <= datetime('now', ?)",
                )
                .bind(kind)
                .bind(&border)
                .execute(&mut **tx)
                .await?;
            }
            None => {
                sqlx::query(
                    "DELETE FROM stage_snapshots
                      WHERE datetime(created_at) <= datetime('now', ?)",
                )
                .bind(&border)
                .execute(&mut **tx)
                .await?;
            }
        }
        // Строки кандидатов нужны, пока жив снимок или незавершённое задание,
        // которое по нему убирает письма.
        sqlx::query(
            "DELETE FROM stage_snapshot_messages
              WHERE key NOT IN (SELECT key FROM stage_snapshots)
                AND key NOT IN (
                    SELECT snapshot_key FROM sender_policy_jobs
                     WHERE state IN ('pending','running')
                    UNION SELECT snapshot_key FROM ignored_conversation_jobs
                     WHERE state IN ('pending','running')
                    UNION SELECT snapshot_key FROM sender_sweep_jobs
                     WHERE state IN ('pending','running','waiting_operation'))",
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Отложить письмо до следующего прохода: его повторяют по номеру, поэтому
    /// движение общего курсора не выводит его из остатка навсегда (S-019).
    pub(crate) async fn defer_stage_message(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        kind: &str,
        job_id: i64,
        message_id: i64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT OR IGNORE INTO stage_job_deferrals(kind, job_id, message_id)
             VALUES(?, ?, ?)",
        )
        .bind(kind)
        .bind(job_id)
        .bind(message_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Письмо обработано: откладывать его больше не нужно.
    pub(crate) async fn clear_stage_deferral(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        kind: &str,
        job_id: i64,
        message_id: i64,
    ) -> Result<()> {
        sqlx::query("DELETE FROM stage_job_deferrals WHERE kind=? AND job_id=? AND message_id=?")
            .bind(kind)
            .bind(job_id)
            .bind(message_id)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    /// Задание закрыто: его отложенные письма больше никого не ждут.
    pub(crate) async fn clear_stage_deferrals(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        kind: &str,
        job_id: i64,
    ) -> Result<()> {
        sqlx::query("DELETE FROM stage_job_deferrals WHERE kind=? AND job_id=?")
            .bind(kind)
            .bind(job_id)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    /// Отменить неисполненные перемещения по метке. Признак закрывшей стадии
    /// снимается тем же запросом: без этого письмо оставалось бы закрытым
    /// навсегда - правила его больше не разбирают, а в уведомление о новой
    /// почте оно не попадает, хотя лежит во входящих.
    pub(crate) async fn cancel_marked_operations(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        marker: &str,
    ) -> Result<i64> {
        sqlx::query(
            "UPDATE messages SET closed_by_stage=NULL
              WHERE id IN (SELECT message_id FROM outbox_ops
                            WHERE json_extract(payload, '$.rule_id')=?
                              AND status IN ('pending','retry'))",
        )
        .bind(marker)
        .execute(&mut **tx)
        .await?;
        let cancelled = sqlx::query(
            "DELETE FROM outbox_ops
              WHERE json_extract(payload, '$.rule_id')=? AND status IN ('pending','retry')",
        )
        .bind(marker)
        .execute(&mut **tx)
        .await?;
        Ok(cancelled.rows_affected() as i64)
    }

    /// Число перемещений по метке, которые отменить уже нельзя (S-042).
    pub(crate) async fn count_irreversible_operations(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        marker: &str,
    ) -> Result<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM outbox_ops
              WHERE json_extract(payload, '$.rule_id')=? AND status NOT IN ('pending','retry')",
        )
        .bind(marker)
        .fetch_one(&mut **tx)
        .await?;
        Ok(row.0)
    }

    /// Наибольший номер письма: граница снимка кандидатов.
    pub(crate) async fn max_message_id(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ) -> Result<i64> {
        let row: (Option<i64>,) = sqlx::query_as("SELECT max(id) FROM messages")
            .fetch_one(&mut **tx)
            .await?;
        Ok(row.0.unwrap_or(0))
    }
}
