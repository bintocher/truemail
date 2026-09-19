//! Автоочистка писем по отправителю: разовая уборка, постоянные записи режимов
//! "только последнее" и "старше N дней" и стадия разбора нового письма
//! (specs/sweep-by-sender.md).
//!
//! Письма всегда уходят в корзину и остаются восстановимыми: безвозвратного
//! удаления автоочистка не создаёт (S-014), а письмо, уже лежащее в корзине, в
//! корзину не переносится (S-005).

use super::Db;
use super::stages::{
    DEFERRED_MESSAGES, STAGE_BATCH, STAGE_MESSAGE_COLUMNS, StageCounters, StageMessage,
    WORKING_FOLDERS, WORKING_FOLDERS_WITH_ARCHIVE, needs_retry,
};
use crate::Result;
use crate::model::*;
use crate::storage::repo::{
    TakeawayActor, TakeawayOutcome, TakeawayTarget, queue_takeaway_operation, resolve_role_folder,
};
use sqlx::AssertSqlSafe;

/// Вид снимка кандидатов уборки в общей таблице снимков.
const SNAPSHOT_KIND: &str = "sender_sweep";

/// Вид отложенных писем: у каждого рода заданий свой, номера заданий разных
/// родов совпадают.
const DEFERRAL_KIND: &str = "sender_sweep";

/// Метка операции очереди, поставленной автоочисткой: по ней отменяются
/// неисполненные перемещения и считаются неотменимые (S-041, S-042).
fn sweep_marker(job_id: i64) -> String {
    format!("sender_sweep:{job_id}")
}

/// Состав уборки в ключе снимка: ключ, выданный для одной области, числа дней
/// и согласия на архив, к другой уборке не подходит (S-011).
fn sweep_payload(address: &str, input: &SenderSweepInput) -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}",
        address.to_lowercase(),
        input.mode,
        input.account_id.unwrap_or(-1),
        input.days.unwrap_or(0),
        input.sweep_archive as i64
    )
}

/// Граница возраста письма для режима "старше N дней": дата письма меньше
/// текущего времени по utc за вычетом N суток (S-033). Формат совпадает с тем,
/// в котором дата письма лежит в базе, поэтому сравнение идёт как у строк.
fn older_than_border(days: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::days(days))
        .format("%Y-%m-%dT%H:%M:%S+00:00")
        .to_string()
}

/// Проход уборки, прочитанный из базы: состав уборки, граница снимка, курсор и
/// число проверок ожидания (S-017 - S-021).
#[derive(Debug, sqlx::FromRow)]
struct SweepJobRow {
    rule_id: Option<i64>,
    account_id: Option<i64>,
    address: String,
    mode: String,
    days: Option<i64>,
    sweep_archive: i64,
    snapshot_key: String,
    max_message_id: i64,
    cursor_message_id: i64,
    state: String,
    waits: i64,
    /// Проход заведён приходом письма: полным проходом записи он не считается
    /// (S-035, S-036).
    triggered_by_message: i64,
}

/// Запись автоочистки в том объёме, который нужен стадии и полному проходу
/// (S-026, S-035).
#[derive(Debug, sqlx::FromRow)]
struct SweepRuleRow {
    id: i64,
    address: String,
    account_id: Option<i64>,
    mode: String,
    days: Option<i64>,
    sweep_archive: i64,
}

/// Условие отбора писем уборки и его связанные параметры. Значения в текст
/// запроса не попадают.
struct SweepScope {
    filter: String,
    /// Только условие рабочих папок: уборка по снимку отбирает письма по его
    /// строкам, но служебные папки не берёт и тогда.
    folders: &'static str,
    address: String,
    account_id: Option<i64>,
    border: Option<String>,
    keep_message_id: Option<i64>,
}

impl SweepScope {
    fn build(
        address: &str,
        account_id: Option<i64>,
        mode: &str,
        days: Option<i64>,
        sweep_archive: bool,
        keep_message_id: Option<i64>,
    ) -> Self {
        // S-016: архив берётся только по отдельному согласию пользователя.
        let folders = if sweep_archive {
            WORKING_FOLDERS_WITH_ARCHIVE
        } else {
            WORKING_FOLDERS
        };
        // S-009: адрес письма сравнивается целиком в нижнем регистре, поэтому
        // отбор опирается на индекс по выражению lower(from_addr).
        let mut filter = format!("lower(m.from_addr)=? AND {folders}");
        if account_id.is_some() {
            filter.push_str(" AND m.account_id=?");
        }
        let border = if mode == SWEEP_MODE_OLDER_THAN {
            // S-034: письмо без разобранной даты автоочистка не убирает.
            filter.push_str(" AND m.date IS NOT NULL AND m.date < ?");
            days.map(older_than_border)
        } else {
            None
        };
        if mode == SWEEP_MODE_ONLY_LAST && keep_message_id.is_some() {
            filter.push_str(" AND m.id<>?");
        }
        Self {
            filter,
            folders,
            address: address.to_lowercase(),
            account_id,
            border,
            keep_message_id: if mode == SWEEP_MODE_ONLY_LAST {
                keep_message_id
            } else {
                None
            },
        }
    }

    fn bind<'q, T>(
        &'q self,
        mut query: sqlx::query::QueryAs<'q, sqlx::Sqlite, T, sqlx::sqlite::SqliteArguments>,
    ) -> sqlx::query::QueryAs<'q, sqlx::Sqlite, T, sqlx::sqlite::SqliteArguments> {
        query = query.bind(&self.address);
        if let Some(account_id) = self.account_id {
            query = query.bind(account_id);
        }
        if let Some(border) = &self.border {
            query = query.bind(border);
        }
        if let Some(keep) = self.keep_message_id {
            query = query.bind(keep);
        }
        query
    }
}

impl Db {
    /// Последнее письмо отправителя: наибольшая дата, а при равных датах -
    /// наибольший локальный номер.
    async fn last_message_of_sender(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        address: &str,
        account_id: Option<i64>,
        sweep_archive: bool,
    ) -> Result<Option<i64>> {
        let folders = if sweep_archive {
            WORKING_FOLDERS_WITH_ARCHIVE
        } else {
            WORKING_FOLDERS
        };
        let account_filter = if account_id.is_some() {
            " AND m.account_id=?"
        } else {
            ""
        };
        let sql = format!(
            "SELECT m.id FROM messages m JOIN folders f ON f.id=m.folder_id
              WHERE lower(m.from_addr)=? AND {folders}{account_filter}
              ORDER BY m.date DESC, m.id DESC LIMIT 1"
        );
        let mut query =
            sqlx::query_as::<_, (i64,)>(AssertSqlSafe(sql)).bind(address.to_lowercase());
        if let Some(account_id) = account_id {
            query = query.bind(account_id);
        }
        Ok(query.fetch_optional(&mut **tx).await?.map(|(id,)| id))
    }

    /// Предварительный подсчёт: число писем, распределение по папкам и ключ
    /// снимка кандидатов (S-010 - S-012).
    pub async fn preview_sender_sweep(
        &self,
        input: SenderSweepInput,
    ) -> Result<SenderSweepPreview> {
        validate_sweep_input(&input).map_err(crate::Error::AccountConfig)?;
        // S-046: адрес нормализуется так же, как значение списков отправителей.
        let address =
            normalize_policy_address(&input.address).map_err(crate::Error::AccountConfig)?;
        let mut tx = self.begin_write().await?;
        let keep = if input.mode == SWEEP_MODE_ONLY_LAST {
            Self::last_message_of_sender(&mut tx, &address, input.account_id, input.sweep_archive)
                .await?
        } else {
            None
        };
        let scope = SweepScope::build(
            &address,
            input.account_id,
            &input.mode,
            input.days,
            input.sweep_archive,
            keep,
        );
        // S-010, S-011: все четыре режима идут общим путём просмотра, и снимок
        // хранит сами письма-кандидаты, а не только их число. Для режима
        // "новые сразу" это уже полученные письма: они останутся на месте, но
        // пользователь видит, скольких писем правило не коснётся.
        let sql = format!(
            "SELECT m.id, m.folder_id, m.account_id, f.display_name, f.role
               FROM messages m JOIN folders f ON f.id=m.folder_id
              WHERE {filter}
              ORDER BY m.id",
            filter = scope.filter
        );
        let rows = scope
            .bind(sqlx::query_as::<
                _,
                (i64, i64, i64, Option<String>, Option<String>),
            >(AssertSqlSafe(sql)))
            .fetch_all(&mut *tx)
            .await?;
        let mut folders: Vec<SenderSweepFolderCount> = Vec::new();
        let mut candidates: Vec<i64> = Vec::with_capacity(rows.len());
        for (id, folder_id, account_id, name, role) in rows {
            candidates.push(id);
            match folders.iter_mut().find(|item| item.folder_id == folder_id) {
                Some(found) => found.count += 1,
                None => folders.push(SenderSweepFolderCount {
                    folder_id,
                    account_id,
                    name: name.unwrap_or_default(),
                    role,
                    count: 1,
                }),
            }
        }
        folders.sort_by_key(|folder| std::cmp::Reverse(folder.count));
        let total = candidates.len() as i64;
        let max_message_id = Self::max_message_id(&mut tx).await?;
        let payload = sweep_payload(&address, &input);
        let snapshot_key = Self::save_stage_snapshot(
            &mut tx,
            SNAPSHOT_KIND,
            &payload,
            max_message_id,
            &candidates,
        )
        .await?;
        tx.commit().await?;
        Ok(SenderSweepPreview {
            address,
            mode: input.mode,
            account_id: input.account_id,
            days: input.days,
            sweep_archive: input.sweep_archive,
            snapshot_key,
            total,
            folders,
        })
    }

    /// Подтверждённая уборка: разовая, постоянная запись или обычное правило
    /// режима "новые сразу" (S-022 - S-026).
    pub async fn start_sender_sweep(
        &self,
        input: SenderSweepInput,
        snapshot_key: &str,
    ) -> Result<SenderSweepJobReport> {
        validate_sweep_input(&input).map_err(crate::Error::AccountConfig)?;
        let address =
            normalize_policy_address(&input.address).map_err(crate::Error::AccountConfig)?;
        let mut tx = self.begin_write().await?;
        // S-011: ключ снимка расходуется неделимо и для всех четырёх режимов,
        // поэтому подтверждение нельзя применить к другой области, другому
        // числу дней или другому согласию на архив.
        let (payload, max_message_id, total) =
            Self::consume_stage_snapshot(&mut tx, snapshot_key, SNAPSHOT_KIND).await?;
        if payload != sweep_payload(&address, &input) {
            return Err(crate::Error::AccountConfig(
                "список писем относится к другой уборке, откройте подтверждение заново".into(),
            ));
        }
        if input.mode == SWEEP_MODE_NEW_NOW {
            // S-023, S-024: режим "новые сразу" целиком выражается обычным
            // правилом и живёт в общем списке правил. Уже полученные письма он
            // не трогает, поэтому задание уборки не заводится.
            tx.commit().await?;
            self.create_new_now_rule(&address, input.account_id).await?;
            return Ok(SenderSweepJobReport {
                state: SWEEP_JOB_COMPLETED.into(),
                ..SenderSweepJobReport::default()
            });
        }
        // S-012: под уборку не подошло ни одного письма - разовая уборка не
        // создаётся.
        if total == 0 && input.mode == SWEEP_MODE_ONCE {
            return Err(crate::Error::AccountConfig(
                "писем этого отправителя не нашлось: убирать нечего".into(),
            ));
        }
        let rule_id = if is_persistent_mode(&input.mode) {
            // S-027: вторую включённую запись для того же адреса и области
            // отвергает уникальный ключ схемы.
            let inserted: std::result::Result<(i64,), sqlx::Error> = sqlx::query_as(
                "INSERT INTO sender_sweep_rules(address, account_id, mode, days, sweep_archive)
                 VALUES(?, ?, ?, ?, ?) RETURNING id",
            )
            .bind(&address)
            .bind(input.account_id)
            .bind(&input.mode)
            .bind(input.days)
            .bind(input.sweep_archive as i64)
            .fetch_one(&mut *tx)
            .await;
            match inserted {
                Ok((id,)) => Some(id),
                Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
                    return Err(crate::Error::AccountConfig(
                        "для этого отправителя уже есть включённая автоочистка: измените её".into(),
                    ));
                }
                Err(error) => return Err(error.into()),
            }
        } else {
            None
        };
        let job: (i64,) = sqlx::query_as(
            "INSERT INTO sender_sweep_jobs(rule_id, account_id, address, mode, days,
                                           sweep_archive, snapshot_key, max_message_id)
             VALUES(?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(rule_id)
        .bind(input.account_id)
        .bind(&address)
        .bind(&input.mode)
        .bind(input.days)
        .bind(input.sweep_archive as i64)
        .bind(snapshot_key)
        .bind(max_message_id)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        self.continue_sender_sweep_job(job.0).await
    }

    /// Правило режима "новые сразу": точное совпадение по адресу отправителя,
    /// перенос в корзину и остановка обработки (S-023).
    async fn create_new_now_rule(&self, address: &str, account_id: Option<i64>) -> Result<()> {
        let rule = MailRuleInput {
            id: uuid::Uuid::new_v4().to_string(),
            name: format!("Автоочистка: {address}"),
            account_id,
            enabled: true,
            groups: vec![MailRuleGroup {
                logic: "all".into(),
                conditions: vec![MailRuleCondition {
                    field: "sender_address".into(),
                    op: "equals".into(),
                    value: address.to_owned(),
                    unit: None,
                    value2: None,
                }],
            }],
            exceptions: Vec::new(),
            actions: vec![
                MailRuleAction {
                    kind: "trash".into(),
                    folder_id: None,
                    folder_role: None,
                    label_id: None,
                },
                MailRuleAction {
                    kind: "stop".into(),
                    folder_id: None,
                    folder_role: None,
                    label_id: None,
                },
            ],
            confirm_key: None,
        };
        self.save_mail_rule(&rule, false, None).await?;
        Ok(())
    }

    /// Записи автоочистки для общего списка правил (S-037).
    pub async fn list_sender_sweep_rules(&self) -> Result<Vec<SenderSweepRule>> {
        let rows: Vec<SenderSweepRule> = sqlx::query_as(
            "SELECT r.id, r.address, r.account_id, r.mode, r.days, r.sweep_archive, r.enabled,
                    r.last_full_pass_at, r.next_check_at, r.created_at, r.updated_at,
                    coalesce((SELECT sum(j.queued) FROM sender_sweep_jobs j WHERE j.rule_id=r.id), 0) AS queued,
                    coalesce((SELECT sum(j.skipped) FROM sender_sweep_jobs j WHERE j.rule_id=r.id), 0) AS skipped,
                    coalesce((SELECT sum(j.failed) FROM sender_sweep_jobs j WHERE j.rule_id=r.id), 0) AS failed,
                    (SELECT j.state FROM sender_sweep_jobs j
                      WHERE j.rule_id=r.id ORDER BY j.id DESC LIMIT 1) AS job_state,
                    r.last_error,
                    (SELECT count(*) FROM outbox_ops o
                      WHERE o.status='failed'
                        AND (json_extract(o.payload, '$.rule_id')='sender_sweep_rule:' || r.id
                             OR json_extract(o.payload, '$.rule_id') IN
                                (SELECT 'sender_sweep:' || j.id FROM sender_sweep_jobs j
                                  WHERE j.rule_id=r.id))) AS queue_failed,
                    (SELECT o.last_error FROM outbox_ops o
                      WHERE o.status='failed'
                        AND (json_extract(o.payload, '$.rule_id')='sender_sweep_rule:' || r.id
                             OR json_extract(o.payload, '$.rule_id') IN
                                (SELECT 'sender_sweep:' || j.id FROM sender_sweep_jobs j
                                  WHERE j.rule_id=r.id))
                      ORDER BY o.id DESC LIMIT 1) AS queue_error
               FROM sender_sweep_rules r
              ORDER BY r.created_at, r.id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Выключить или включить запись: выключенная запись новых операций не
    /// создаёт (S-038).
    pub async fn set_sender_sweep_enabled(&self, id: i64, enabled: bool) -> Result<()> {
        let changed = sqlx::query(
            "UPDATE sender_sweep_rules SET enabled=?, updated_at=datetime('now') WHERE id=?",
        )
        .bind(enabled as i64)
        .bind(id)
        .execute(&self.write_pool)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(crate::Error::AccountConfig(
                "запись автоочистки не найдена".into(),
            ));
        }
        Ok(())
    }

    /// Сменить режим записи одной неделимой операцией: состояния с двумя
    /// включёнными режимами не возникает (S-028).
    pub async fn update_sender_sweep_mode(
        &self,
        id: i64,
        mode: &str,
        days: Option<i64>,
        sweep_archive: bool,
    ) -> Result<()> {
        if !is_persistent_mode(mode) {
            return Err(crate::Error::AccountConfig(
                "у записи автоочистки бывает только режим \"только последнее\" или \"старше N дней\"".into(),
            ));
        }
        validate_sweep_input(&SenderSweepInput {
            address: String::new(),
            mode: mode.to_owned(),
            account_id: None,
            days,
            sweep_archive,
        })
        .map_err(crate::Error::AccountConfig)?;
        let changed = sqlx::query(
            "UPDATE sender_sweep_rules
                SET mode=?, days=?, sweep_archive=?, next_check_at=NULL,
                    updated_at=datetime('now')
              WHERE id=?",
        )
        .bind(mode)
        .bind(days)
        .bind(sweep_archive as i64)
        .bind(id)
        .execute(&self.write_pool)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(crate::Error::AccountConfig(
                "запись автоочистки не найдена".into(),
            ));
        }
        Ok(())
    }

    /// Удалить запись автоочистки: уже убранные письма остаются в корзине и не
    /// возвращаются (S-039).
    pub async fn delete_sender_sweep_rule(&self, id: i64) -> Result<()> {
        sqlx::query("DELETE FROM sender_sweep_rules WHERE id=?")
            .bind(id)
            .execute(&self.write_pool)
            .await?;
        Ok(())
    }

    /// Один проход уборки: не больше 500 писем, письмо с чужой незавершённой
    /// операцией переводит проход в состояние ожидания (S-017 - S-021).
    pub async fn continue_sender_sweep_job(&self, job_id: i64) -> Result<SenderSweepJobReport> {
        let mut tx = self.begin_write().await?;
        let job: Option<SweepJobRow> = sqlx::query_as(
            "SELECT rule_id, account_id, address, mode, days, sweep_archive, snapshot_key,
                    max_message_id, cursor_message_id, state, waits, triggered_by_message
               FROM sender_sweep_jobs WHERE id=?",
        )
        .bind(job_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(job) = job else {
            return Err(crate::Error::AccountConfig(
                "проход уборки не найден".into(),
            ));
        };
        let SweepJobRow {
            rule_id,
            account_id,
            address,
            mode,
            days,
            sweep_archive,
            snapshot_key,
            max_message_id,
            cursor_message_id: cursor,
            state,
            waits,
            triggered_by_message,
        } = job;
        if matches!(state.as_str(), "completed" | "cancelled" | "failed") {
            return self.sender_sweep_job_report(job_id).await;
        }
        // S-038: выключенная запись новых операций не создаёт.
        if let Some(rule_id) = rule_id {
            let enabled: Option<(i64,)> =
                sqlx::query_as("SELECT enabled FROM sender_sweep_rules WHERE id=?")
                    .bind(rule_id)
                    .fetch_optional(&mut *tx)
                    .await?;
            if enabled.map(|(value,)| value).unwrap_or(0) == 0 {
                sqlx::query(
                    "UPDATE sender_sweep_jobs SET state='cancelled', updated_at=datetime('now')
                      WHERE id=?",
                )
                .bind(job_id)
                .execute(&mut *tx)
                .await?;
                tx.commit().await?;
                return self.sender_sweep_job_report(job_id).await;
            }
        }
        sqlx::query(
            "UPDATE sender_sweep_jobs SET state='running', updated_at=datetime('now') WHERE id=?",
        )
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
        let sweep_archive = sweep_archive != 0;
        // Уборка по подтверждению идёт по строкам снимка: в корзину уходят
        // ровно показанные пользователю письма. У полного прохода записи
        // снимка нет, и он отбирает письма условием самой записи.
        let uses_snapshot = !snapshot_key.is_empty();
        let keep = if mode == SWEEP_MODE_ONLY_LAST && !uses_snapshot {
            // Последнее письмо вычисляется внутри неделимой операции: приход
            // двух писем подряд заново определяет его до постановки
            // перемещений. Снимку это не нужно: последнее письмо в него не
            // попало ещё при подсчёте.
            Self::last_message_of_sender(&mut tx, &address, account_id, sweep_archive).await?
        } else {
            None
        };
        let scope = SweepScope::build(&address, account_id, &mode, days, sweep_archive, keep);
        let (join, filter) = if uses_snapshot {
            (
                "JOIN stage_snapshot_messages s ON s.message_id=m.id AND s.key=?",
                scope.folders.to_owned(),
            )
        } else {
            ("", scope.filter.clone())
        };
        // S-019: письмо, отложенное чужой незавершённой операцией, читается
        // наравне с письмами после курсора, поэтому движение курсора не выводит
        // его из остатка навсегда.
        let sql = format!(
            "SELECT {STAGE_MESSAGE_COLUMNS}
               FROM messages m JOIN folders f ON f.id=m.folder_id {join}
              WHERE {filter} AND m.id<=? AND (m.id>? OR {DEFERRED_MESSAGES})
              ORDER BY m.id LIMIT ?"
        );
        let mut query = sqlx::query_as::<_, StageMessage>(AssertSqlSafe(sql));
        query = if uses_snapshot {
            query.bind(&snapshot_key)
        } else {
            scope.bind(query)
        };
        let batch = query
            .bind(max_message_id)
            .bind(cursor)
            .bind(DEFERRAL_KIND)
            .bind(job_id)
            .bind(STAGE_BATCH)
            .fetch_all(&mut *tx)
            .await?;
        let mut counters = StageCounters::default();
        let mut busy = 0i64;
        let mut last_id = cursor;
        let mut trash_missing = false;
        let marker = sweep_marker(job_id);
        for message in &batch {
            last_id = last_id.max(message.id);
            let trash = resolve_role_folder(&mut tx, message.account_id, "trash").await?;
            let Some((folder_id, path)) = trash else {
                // S-003: ящик без корзины даёт отказ прохода для этого ящика,
                // а письмо продолжает путь к правилам обработки. Повторять его
                // незачем: без назначенной корзины отказ повторится тот же.
                trash_missing = true;
                counters.failed += 1;
                Self::clear_stage_deferral(&mut tx, DEFERRAL_KIND, job_id, message.id).await?;
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
                Some(marker.as_str()),
                0,
            )
            .await?;
            // S-019: письмо с чужой незавершённой операцией пропускается в
            // текущем проходе и повторяется следующим.
            if needs_retry(&outcome) {
                busy += 1;
                Self::defer_stage_message(&mut tx, DEFERRAL_KIND, job_id, message.id).await?;
            } else {
                Self::clear_stage_deferral(&mut tx, DEFERRAL_KIND, job_id, message.id).await?;
            }
            if counters.account(&outcome) {
                sqlx::query("UPDATE messages SET closed_by_stage=? WHERE id=?")
                    .bind(SENDER_SWEEP_STAGE_NAME)
                    .bind(message.id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        let remaining_sql = format!(
            "SELECT count(*) FROM messages m JOIN folders f ON f.id=m.folder_id {join}
              WHERE {filter} AND m.id<=? AND (m.id>? OR {DEFERRED_MESSAGES})"
        );
        let mut remaining_query = sqlx::query_as::<_, (i64,)>(AssertSqlSafe(remaining_sql));
        remaining_query = if uses_snapshot {
            remaining_query.bind(&snapshot_key)
        } else {
            scope.bind(remaining_query)
        };
        let remaining: (i64,) = remaining_query
            .bind(max_message_id)
            .bind(last_id)
            .bind(DEFERRAL_KIND)
            .bind(job_id)
            .fetch_one(&mut *tx)
            .await?;
        // S-020, S-021: ожидание чужой операции назначается не раньше чем через
        // минуту и повторяется не более восьми раз.
        let (next_state, next_waits, next_check) = if busy > 0 {
            if waits + 1 >= SWEEP_MAX_WAITS {
                (SWEEP_JOB_FAILED, waits + 1, None)
            } else {
                (
                    SWEEP_JOB_WAITING,
                    waits + 1,
                    Some(format!("+{SWEEP_WAIT_SECONDS} seconds")),
                )
            }
        } else if remaining.0 > 0 {
            (SWEEP_JOB_PENDING, waits, None)
        } else {
            (SWEEP_JOB_COMPLETED, waits, None)
        };
        sqlx::query(
            "UPDATE sender_sweep_jobs
                SET state=?, waits=?, next_check_at=CASE WHEN ? IS NULL THEN NULL
                                                        ELSE datetime('now', ?) END,
                    cursor_message_id=?, found=found+?, queued=queued+?, skipped=skipped+?,
                    failed=failed+?, remaining=?, last_error=?, updated_at=datetime('now')
              WHERE id=?",
        )
        .bind(next_state)
        .bind(next_waits)
        .bind(next_check.as_deref())
        .bind(next_check.as_deref())
        .bind(last_id)
        .bind(batch.len() as i64)
        .bind(counters.queued)
        .bind(counters.skipped)
        .bind(counters.failed)
        // Остаток уже включает отложенные письма: второй раз их считать
        // нельзя, иначе пользователю показывается несуществующая работа.
        .bind(remaining.0)
        .bind(if trash_missing {
            Some("в ящике нет папки с ролью корзины")
        } else {
            None
        })
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
        if matches!(
            next_state,
            SWEEP_JOB_COMPLETED | SWEEP_JOB_FAILED | SWEEP_JOB_CANCELLED
        ) {
            Self::clear_stage_deferrals(&mut tx, DEFERRAL_KIND, job_id).await?;
        }
        if let Some(rule_id) = rule_id
            && next_state == SWEEP_JOB_COMPLETED
            // S-036: проход, заведённый приходом письма, полным не считается и
            // суточный срок записи не сбрасывает.
            && triggered_by_message == 0
        {
            // S-035: полный проход записи отмечается временем завершения.
            sqlx::query(
                "UPDATE sender_sweep_rules
                    SET last_full_pass_at=datetime('now'), last_error=?, updated_at=datetime('now')
                  WHERE id=?",
            )
            .bind(if trash_missing {
                Some("в ящике нет папки с ролью корзины")
            } else {
                None
            })
            .bind(rule_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.sender_sweep_job_report(job_id).await
    }

    /// Отменить незавершённую уборку: новые операции после текущего прохода не
    /// создаются, а неисполненные удаляются (S-041, S-042).
    pub async fn cancel_sender_sweep_job(&self, job_id: i64) -> Result<SenderSweepJobReport> {
        let mut tx = self.begin_write().await?;
        let marker = sweep_marker(job_id);
        let irreversible = Self::count_irreversible_operations(&mut tx, &marker).await?;
        // Признак закрывшей стадии снимается вместе с отменой перемещения:
        // иначе письмо осталось бы закрытым навсегда.
        Self::cancel_marked_operations(&mut tx, &marker).await?;
        let changed = sqlx::query(
            "UPDATE sender_sweep_jobs SET state='cancelled', updated_at=datetime('now')
              WHERE id=?",
        )
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(crate::Error::AccountConfig(
                "проход уборки не найден".into(),
            ));
        }
        Self::clear_stage_deferrals(&mut tx, DEFERRAL_KIND, job_id).await?;
        tx.commit().await?;
        let mut report = self.sender_sweep_job_report(job_id).await?;
        report.irreversible = irreversible;
        Ok(report)
    }

    pub async fn sender_sweep_job_report(&self, job_id: i64) -> Result<SenderSweepJobReport> {
        let report: SenderSweepJobReport = sqlx::query_as(
            "SELECT id, rule_id, state, found, queued, skipped, failed, remaining,
                    0 AS irreversible
               FROM sender_sweep_jobs WHERE id=?",
        )
        .bind(job_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(report)
    }

    /// Незавершённые проходы для интерфейса (S-018, S-040).
    pub async fn pending_sender_sweep_jobs(&self) -> Result<Vec<SenderSweepJobReport>> {
        let rows: Vec<SenderSweepJobReport> = sqlx::query_as(
            "SELECT id, rule_id, state, found, queued, skipped, failed, remaining,
                    0 AS irreversible
               FROM sender_sweep_jobs
              WHERE state IN ('pending','running','waiting_operation') ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Вернуть проходы, брошенные закрытием программы, в состояние ожидания
    /// (S-043).
    pub(crate) async fn restore_sender_sweep_jobs(&self) -> Result<()> {
        sqlx::query(
            "UPDATE sender_sweep_jobs
                SET state='pending', next_check_at=NULL, updated_at=datetime('now')
              WHERE state IN ('running','waiting_operation')",
        )
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// Полный проход каждой включённой записи при запуске программы (S-035).
    /// По одному лишь суточному сроку он бы не начался, и запись, заведённая
    /// без работающей синхронизации, не срабатывала бы вовсе.
    pub(crate) async fn start_sender_sweep_full_passes(&self) -> Result<()> {
        let mut tx = self.begin_write().await?;
        let max_message_id = Self::max_message_id(&mut tx).await?;
        sqlx::query(
            "INSERT INTO sender_sweep_jobs(rule_id, account_id, address, mode, days,
                                           sweep_archive, max_message_id)
             SELECT r.id, r.account_id, r.address, r.mode, r.days, r.sweep_archive, ?
               FROM sender_sweep_rules r
              WHERE r.enabled=1
                AND NOT EXISTS (SELECT 1 FROM sender_sweep_jobs j
                                 WHERE j.rule_id=r.id
                                   AND j.state IN ('pending','running','waiting_operation'))",
        )
        .bind(max_message_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Завести полный проход включённым записям, у которых его давно не было
    /// (S-035), и продвинуть незавершённые проходы на одну пачку.
    pub(crate) async fn advance_sender_sweep_jobs(&self) -> Result<()> {
        let mut tx = self.begin_write().await?;
        let due: Vec<SweepRuleRow> = sqlx::query_as(
            "SELECT id, address, account_id, mode, days, sweep_archive
               FROM sender_sweep_rules
              WHERE enabled=1
                AND (last_full_pass_at IS NULL
                     OR datetime(last_full_pass_at) <= datetime('now', ?))
                AND NOT EXISTS (SELECT 1 FROM sender_sweep_jobs j
                                 WHERE j.rule_id=sender_sweep_rules.id
                                   AND j.state IN ('pending','running','waiting_operation'))",
        )
        .bind(format!("-{SWEEP_FULL_PASS_HOURS} hours"))
        .fetch_all(&mut *tx)
        .await?;
        let max_message_id = Self::max_message_id(&mut tx).await?;
        for rule in due {
            sqlx::query(
                "INSERT INTO sender_sweep_jobs(rule_id, account_id, address, mode, days,
                                               sweep_archive, max_message_id)
                 VALUES(?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(rule.id)
            .bind(rule.account_id)
            .bind(&rule.address)
            .bind(&rule.mode)
            .bind(rule.days)
            .bind(rule.sweep_archive)
            .bind(max_message_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        let jobs: Vec<(i64,)> = sqlx::query_as(
            "SELECT id FROM sender_sweep_jobs
              WHERE state='pending'
                 OR (state='waiting_operation' AND (next_check_at IS NULL
                     OR datetime(next_check_at) <= datetime('now')))
              ORDER BY id LIMIT 4",
        )
        .fetch_all(&self.pool)
        .await?;
        for (job_id,) in jobs {
            self.continue_sender_sweep_job(job_id).await?;
        }
        Ok(())
    }

    /// Стадия автоочистки: третья в сквозном порядке стадий и вторая на пути
    /// догрузки писем прокруткой (S-001, S-031, S-036).
    pub(crate) async fn process_sender_sweep_stage(&self) -> Result<usize> {
        let mut tx = self.begin_write().await?;
        let rules: Vec<SweepRuleRow> = sqlx::query_as(
            "SELECT id, address, account_id, mode, days, sweep_archive
               FROM sender_sweep_rules WHERE enabled=1",
        )
        .fetch_all(&mut *tx)
        .await?;
        let cursor = Self::stage_cursor(&mut tx, SENDER_SWEEP_STAGE_NAME).await?;
        if rules.is_empty() {
            let newest = Self::max_message_id(&mut tx).await?;
            Self::set_stage_cursor(&mut tx, SENDER_SWEEP_STAGE_NAME, newest).await?;
            tx.commit().await?;
            return Ok(0);
        }
        // Архив берётся только у записей с согласием пользователя, поэтому
        // пачка читается вместе с архивом, а решение принимается по записи.
        let sql = format!(
            "SELECT {STAGE_MESSAGE_COLUMNS}
               FROM messages m JOIN folders f ON f.id=m.folder_id
              WHERE m.id>? AND m.closed_by_stage IS NULL AND {WORKING_FOLDERS_WITH_ARCHIVE}
              ORDER BY m.id LIMIT ?"
        );
        let batch = sqlx::query_as::<_, StageMessage>(AssertSqlSafe(sql))
            .bind(cursor)
            .bind(STAGE_BATCH)
            .fetch_all(&mut *tx)
            .await?;
        let mut queued = 0usize;
        let mut last_id = cursor;
        let mut touched: Vec<i64> = Vec::new();
        // Последнее письмо отправителя читается один раз на пачку: запрос на
        // каждое письмо превратил бы разбор пачки в пятьсот поисков подряд.
        let mut last_message: std::collections::HashMap<(String, Option<i64>), Option<i64>> =
            std::collections::HashMap::new();
        for message in &batch {
            last_id = message.id;
            let Some(address) = message.from_addr.as_deref() else {
                continue;
            };
            let address = canonical_sender_address(address).to_lowercase();
            // S-026: запись своего ящика точнее записи всех ящиков, поэтому
            // пересекающиеся области применяются в явном порядке, а не в
            // случайном порядке чтения базы.
            let same_address = |rule: &&SweepRuleRow| rule.address.to_lowercase() == address;
            let Some(rule) = rules
                .iter()
                .find(|rule| same_address(rule) && rule.account_id == Some(message.account_id))
                .or_else(|| {
                    rules
                        .iter()
                        .find(|rule| same_address(rule) && rule.account_id.is_none())
                })
            else {
                continue;
            };
            let (rule_id, account_id, mode, days) =
                (&rule.id, &rule.account_id, &rule.mode, &rule.days);
            let sweep_archive = rule.sweep_archive != 0;
            if message.folder_role.as_deref() == Some("archive") && !sweep_archive {
                continue;
            }
            if !touched.contains(rule_id) {
                touched.push(*rule_id);
            }
            let take = match mode.as_str() {
                SWEEP_MODE_OLDER_THAN => {
                    // S-033, S-034: письмо берётся только при разобранной дате
                    // старше границы.
                    let Some(days) = days else { continue };
                    message
                        .date
                        .as_deref()
                        .is_some_and(|date| date < older_than_border(*days).as_str())
                }
                SWEEP_MODE_ONLY_LAST => {
                    // S-030: новое письмо остаётся последним, а прежнее
                    // последнее убирает проход записи.
                    let key = (address.clone(), *account_id);
                    let last = match last_message.get(&key) {
                        Some(found) => *found,
                        None => {
                            let found = Self::last_message_of_sender(
                                &mut tx,
                                &address,
                                *account_id,
                                sweep_archive,
                            )
                            .await?;
                            last_message.insert(key, found);
                            found
                        }
                    };
                    last != Some(message.id)
                }
                _ => false,
            };
            if !take {
                continue;
            }
            let trash = resolve_role_folder(&mut tx, message.account_id, "trash").await?;
            let Some((folder_id, path)) = trash else {
                // S-003: результат стадии failed_open - письмо остаётся на
                // месте и передаётся правилам обработки, поэтому признак
                // закрывшей стадии ему не ставится.
                sqlx::query(
                    "UPDATE sender_sweep_rules SET last_error=?, updated_at=datetime('now')
                      WHERE id=?",
                )
                .bind("в ящике нет папки с ролью корзины")
                .bind(rule_id)
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
                Some(format!("sender_sweep_rule:{rule_id}").as_str()),
                0,
            )
            .await?;
            if matches!(outcome, TakeawayOutcome::Queued(_)) {
                sqlx::query("UPDATE messages SET closed_by_stage=? WHERE id=?")
                    .bind(SENDER_SWEEP_STAGE_NAME)
                    .bind(message.id)
                    .execute(&mut *tx)
                    .await?;
                queued += 1;
            }
        }
        // S-036: приход письма проверяет запись, но полным проходом не
        // считается и срок суточного прохода не сбрасывает.
        let max_message_id = Self::max_message_id(&mut tx).await?;
        for rule_id in touched {
            let busy: (i64,) = sqlx::query_as(
                "SELECT count(*) FROM sender_sweep_jobs
                  WHERE rule_id=? AND state IN ('pending','running','waiting_operation')",
            )
            .bind(rule_id)
            .fetch_one(&mut *tx)
            .await?;
            if busy.0 > 0 {
                continue;
            }
            sqlx::query(
                "INSERT INTO sender_sweep_jobs(rule_id, account_id, address, mode, days,
                                               sweep_archive, max_message_id,
                                               triggered_by_message)
                 SELECT id, account_id, address, mode, days, sweep_archive, ?, 1
                   FROM sender_sweep_rules WHERE id=? AND enabled=1",
            )
            .bind(max_message_id)
            .bind(rule_id)
            .execute(&mut *tx)
            .await?;
        }
        Self::set_stage_cursor(&mut tx, SENDER_SWEEP_STAGE_NAME, last_id).await?;
        tx.commit().await?;
        Ok(queued)
    }
}
