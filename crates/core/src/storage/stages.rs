//! Общая обвязка стадий разбора письма: состав рабочих папок, курсор стадии,
//! снимки кандидатов уборки и снимок письма, которого хватает стадии.
//!
//! Сквозной порядок стадий - списки отправителей, игнорируемые переписки,
//! автоочистка по отправителю и только потом правила обработки
//! (specs/blocked-senders.md, specs/ignore-conversation.md,
//! specs/sweep-by-sender.md, specs/mail-rules-conditions-and-actions.md).

use super::Db;
use crate::Result;
use crate::storage::repo::{TakeawayMessage, TakeawayOutcome};

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

/// Размер пачки стадии: то же число, что уже выбирает один проход правил.
pub(crate) const STAGE_BATCH: i64 = 500;

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

    /// Зафиксировать набор писем-кандидатов и вернуть ключ снимка. Границей
    /// набора служит наибольший номер письма на момент подсчёта: письма,
    /// пришедшие позже, получают больший номер и достаются стадии как новые.
    pub(crate) async fn save_stage_snapshot(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        kind: &str,
        payload: &str,
        max_message_id: i64,
        total: i64,
    ) -> Result<String> {
        let key = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO stage_snapshots(key, kind, payload, max_message_id, total)
             VALUES(?, ?, ?, ?, ?)",
        )
        .bind(&key)
        .bind(kind)
        .bind(payload)
        .bind(max_message_id)
        .bind(total)
        .execute(&mut **tx)
        .await?;
        Ok(key)
    }

    /// Прочитать снимок по ключу. Подтверждение без снимка не принимается:
    /// иначе в корзину ушло бы больше писем, чем показано пользователю.
    pub(crate) async fn take_stage_snapshot(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        key: &str,
        kind: &str,
    ) -> Result<(String, i64, i64)> {
        let row: Option<(String, i64, i64)> = sqlx::query_as(
            "SELECT payload, max_message_id, total FROM stage_snapshots WHERE key=? AND kind=?",
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

    /// Снимок израсходован подтверждением: второй уборки по тому же ключу не
    /// бывает, иначе одно подтверждение пользователя запускало бы её дважды.
    pub(crate) async fn drop_stage_snapshot(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        key: &str,
    ) -> Result<()> {
        sqlx::query("DELETE FROM stage_snapshots WHERE key=?")
            .bind(key)
            .execute(&mut **tx)
            .await?;
        Ok(())
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
