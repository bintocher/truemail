//! Списки заблокированных и доверенных отправителей: стадия разбора нового
//! письма, управление записями и уборка уже полученных писем
//! (specs/blocked-senders.md).
//!
//! Письма уходят только в корзину: безвозвратного удаления в этой задаче нет
//! (S-024), а письмо, уже лежащее в корзине, в корзину не переносится (S-005).

use super::Db;
use super::stages::{
    DEFERRED_MESSAGES, STAGE_MESSAGE_COLUMNS, StageCounters, StageMessage, WORKING_FOLDERS,
    needs_retry,
};
use crate::Result;
use crate::model::*;
use crate::storage::repo::{
    TakeawayActor, TakeawayOutcome, TakeawayTarget, queue_takeaway_operation,
};
use sqlx::AssertSqlSafe;

/// Вид снимка кандидатов уборки в общей таблице снимков.
const SNAPSHOT_KIND: &str = "sender_policy";

/// Аренда задания уборки: задание, не продлившее её, считается брошенным
/// (S-037). Пять минут покрывают самую долгую пачку и не держат задание
/// после закрытия программы.
const LEASE_SECONDS: i64 = 300;

/// Вид отложенных писем: у каждого рода заданий свой, номера заданий разных
/// родов совпадают.
const DEFERRAL_KIND: &str = "sender_policy";

/// Вид отложенных писем самой стадии: у стадии задания нет, поэтому её
/// отложенные письма хранятся отдельно от писем заданий уборки и не
/// снимаются вместе с закрытым заданием.
const STAGE_DEFERRAL_KIND: &str = "sender_policy_stage";

/// Номер задания у писем, отложенных стадией. Стадия идёт без задания, а
/// столбец в таблице общий, поэтому ей отведено значение, которого у
/// настоящих заданий не бывает.
const STAGE_DEFERRAL_JOB: i64 = 0;

/// Метка операции очереди, поставленной списком отправителей (S-047).
fn policy_marker(policy_id: i64) -> String {
    format!("sender_policy:{policy_id}")
}

/// Решение списков по одному письму (S-014 - S-018).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SenderDecision {
    /// Отправитель ни в одном списке: письмо идёт следующей стадии.
    Pass,
    /// Отправитель доверенный: письмо идёт следующей стадии и не блокируется.
    Trusted,
    /// Отправитель заблокирован записью с этим номером.
    Blocked(i64),
}

/// Списки, прочитанные в память каноническими значениями. Стадия сверяет с
/// ними каждое письмо, поэтому читать их из базы на письмо было бы расточительно.
#[derive(Debug, Clone, Default)]
pub(crate) struct SenderPolicySet {
    /// Канонический адрес в нижнем регистре - номер записи и её решение.
    addresses: std::collections::HashMap<String, (i64, String)>,
    domains: std::collections::HashMap<String, (i64, String)>,
    /// Собственные адреса подключённых ящиков в нижнем регистре (S-018).
    own: std::collections::HashSet<String>,
}

impl SenderPolicySet {
    pub fn new(rows: Vec<(i64, String, String, String)>, own_addresses: Vec<String>) -> Self {
        let mut set = Self::default();
        for (id, kind, value, decision) in rows {
            let key = value.to_lowercase();
            match kind.as_str() {
                POLICY_KIND_ADDRESS => {
                    set.addresses.insert(key, (id, decision));
                }
                POLICY_KIND_DOMAIN => {
                    set.domains.insert(key, (id, decision));
                }
                _ => {}
            }
        }
        for email in own_addresses {
            set.own
                .insert(canonical_sender_address(&email).to_lowercase());
        }
        set
    }

    pub fn is_empty(&self) -> bool {
        self.addresses.is_empty() && self.domains.is_empty()
    }

    /// Решение по адресу отправителя письма. Адрес письма приводится к общей
    /// форме тем же разбором, что и значение записи, и сравнивается без учёта
    /// регистра.
    pub fn decide(&self, from_addr: Option<&str>) -> SenderDecision {
        // S-026: у письма без разобранного адреса отправителя стадии сверять
        // нечего, и оно идёт дальше.
        let Some(raw) = from_addr.map(str::trim).filter(|value| !value.is_empty()) else {
            return SenderDecision::Pass;
        };
        let address = canonical_sender_address(raw).to_lowercase();
        // S-018: собственный адрес ящика доверенный независимо от содержимого
        // списков и проверяется до них.
        if self.own.contains(&address) {
            return SenderDecision::Trusted;
        }
        // S-014: запись адреса точнее записи домена, поэтому её решение
        // применяется даже при совпавшем домене.
        if let Some((id, decision)) = self.addresses.get(&address) {
            return match decision.as_str() {
                POLICY_DECISION_BLOCKED => SenderDecision::Blocked(*id),
                _ => SenderDecision::Trusted,
            };
        }
        let Some(domain) = address_domain(&address) else {
            return SenderDecision::Pass;
        };
        let mut blocked: Option<i64> = None;
        for parent in domain_lookup_chain(domain) {
            let Some((id, decision)) = self.domains.get(&parent) else {
                continue;
            };
            // S-015: при совпадении записей одного вида доверие важнее
            // блокировки, поэтому доверенный поддомен отменяет блокировку
            // родительского домена.
            if decision == POLICY_DECISION_TRUSTED {
                return SenderDecision::Trusted;
            }
            if blocked.is_none() {
                blocked = Some(*id);
            }
        }
        match blocked {
            Some(id) => SenderDecision::Blocked(id),
            None => SenderDecision::Pass,
        }
    }
}

/// Экранирование значения для оператора LIKE: домен пользователя может
/// содержать знаки подчёркивания, а они в шаблоне значат "любой символ".
fn like_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// Условие отбора писем по значению записи и его связанные параметры. Значения
/// в текст запроса не попадают: они идут связанными параметрами.
fn sender_match_filter(kind: &str, value: &str) -> (String, Vec<String>) {
    if kind == POLICY_KIND_DOMAIN {
        // S-012: домен совпадает целиком или по границе точки, поэтому
        // окончание "notmail.ru" под запись "mail.ru" не подходит.
        let escaped = like_escape(value);
        (
            "(lower(m.from_addr) LIKE '%@' || ? ESCAPE '\\'
              OR lower(m.from_addr) LIKE '%.' || ? ESCAPE '\\')"
                .to_owned(),
            vec![escaped.clone(), escaped],
        )
    } else {
        // S-035: сравнение адреса письма в нижнем регистре опирается на индекс
        // по выражению lower(from_addr) из миграции 0040.
        (
            "lower(m.from_addr) = ?".to_owned(),
            vec![value.to_lowercase()],
        )
    }
}

impl Db {
    /// Списки для раздела настроек вместе со счётчиками уборки (S-045).
    pub async fn list_sender_policies(&self) -> Result<Vec<SenderPolicy>> {
        // S-048: поздний отказ очереди виден в разделе отправителей. Счётчика
        // отказов постановки для этого мало: письмо остаётся на месте уже
        // после того, как проход отчитался об успехе.
        let rows: Vec<SenderPolicy> = sqlx::query_as(
            "SELECT p.id, p.kind, p.value, p.decision, p.created_at, p.updated_at, p.swept,
                    (SELECT j.state FROM sender_policy_jobs j
                      WHERE j.policy_id=p.id AND j.state IN ('pending','running')
                      ORDER BY j.id LIMIT 1) AS job_state,
                    p.last_error,
                    coalesce((SELECT sum(j.failed) FROM sender_policy_jobs j
                               WHERE j.policy_id=p.id), 0) AS failed,
                    (SELECT count(*) FROM outbox_ops o
                      WHERE o.status='failed'
                        AND json_extract(o.payload, '$.rule_id')='sender_policy:' || p.id)
                      AS queue_failed,
                    (SELECT o.last_error FROM outbox_ops o
                      WHERE o.status='failed'
                        AND json_extract(o.payload, '$.rule_id')='sender_policy:' || p.id
                      ORDER BY o.id DESC LIMIT 1) AS queue_error
               FROM sender_policies p
              ORDER BY p.decision, p.kind, p.value",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Прочитать списки и собственные адреса ящиков в память.
    async fn load_policy_set(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ) -> Result<SenderPolicySet> {
        let rows: Vec<(i64, String, String, String)> =
            sqlx::query_as("SELECT id, kind, value, decision FROM sender_policies")
                .fetch_all(&mut **tx)
                .await?;
        let own: Vec<(String,)> = sqlx::query_as("SELECT email FROM accounts")
            .fetch_all(&mut **tx)
            .await?;
        Ok(SenderPolicySet::new(
            rows,
            own.into_iter().map(|(email,)| email).collect(),
        ))
    }

    /// Предварительный просмотр блокировки: канонический вид значения, защита
    /// собственного адреса, число уже полученных писем по каждому ящику и ключ
    /// снимка кандидатов (S-019, S-020, S-029, S-031).
    pub async fn preview_sender_policy(
        &self,
        kind: &str,
        value: &str,
    ) -> Result<SenderPolicyPreview> {
        if !is_policy_kind(kind) {
            return Err(crate::Error::AccountConfig(format!(
                "вид записи {kind} не поддерживается"
            )));
        }
        let canonical = normalize_policy_value(kind, value).map_err(crate::Error::AccountConfig)?;
        let mut tx = self.begin_write().await?;
        let own: Vec<(i64, String)> = sqlx::query_as("SELECT id, email FROM accounts ORDER BY id")
            .fetch_all(&mut *tx)
            .await?;
        let lowered = canonical.to_lowercase();
        let mut own_address = false;
        let mut own_domain = false;
        for (_, email) in &own {
            let address = canonical_sender_address(email).to_lowercase();
            if kind == POLICY_KIND_ADDRESS && address == lowered {
                own_address = true;
            }
            if kind == POLICY_KIND_DOMAIN
                && address_domain(&address).is_some_and(|domain| domain_matches(domain, &lowered))
            {
                own_domain = true;
            }
        }
        let (filter, binds) = sender_match_filter(kind, &canonical);
        // Кандидаты читаются номерами, а не одним лишь количеством: снимок
        // хранит сами письма, поэтому в корзину уходят ровно показанные
        // пользователю письма (S-031).
        let sql = format!(
            "SELECT m.id, m.account_id FROM messages m
               JOIN folders f ON f.id=m.folder_id
              WHERE {filter} AND {WORKING_FOLDERS}
              ORDER BY m.id"
        );
        let mut query = sqlx::query_as::<_, (i64, i64)>(AssertSqlSafe(sql));
        for bind in &binds {
            query = query.bind(bind);
        }
        let candidates = query.fetch_all(&mut *tx).await?;
        let mut per_account = Vec::new();
        let mut total = 0;
        for (account_id, email) in &own {
            let count = candidates
                .iter()
                .filter(|(_, owner)| owner == account_id)
                .count() as i64;
            total += count;
            per_account.push(SenderPolicyAccountCount {
                account_id: *account_id,
                email: email.clone(),
                count,
            });
        }
        let ids: Vec<i64> = candidates.iter().map(|(id, _)| *id).collect();
        let max_message_id = Self::max_message_id(&mut tx).await?;
        let payload = format!("{kind}\n{canonical}");
        let snapshot_key = Self::save_stage_snapshot(
            &mut tx,
            SNAPSHOT_KIND,
            &payload,
            max_message_id,
            &ids,
            self.limit(LIMIT_STAGE_SNAPSHOT_HOURS),
        )
        .await?;
        tx.commit().await?;
        Ok(SenderPolicyPreview {
            kind: kind.to_owned(),
            value: canonical,
            own_address,
            own_domain,
            snapshot_key,
            total,
            per_account,
        })
    }

    /// Добавить запись списка или сменить решение существующей (S-039,
    /// S-043, S-044). Собственный адрес заблокировать нельзя, а блокировка
    /// собственного домена требует отдельного подтверждения (S-019, S-020).
    pub async fn save_sender_policy(
        &self,
        kind: &str,
        value: &str,
        decision: &str,
        confirm_own_domain: bool,
    ) -> Result<SenderPolicy> {
        if !is_policy_kind(kind) || !is_policy_decision(decision) {
            return Err(crate::Error::AccountConfig(
                "вид записи или решение не поддерживаются".into(),
            ));
        }
        let canonical = normalize_policy_value(kind, value).map_err(crate::Error::AccountConfig)?;
        let lowered = canonical.to_lowercase();
        let mut tx = self.begin_write().await?;
        if decision == POLICY_DECISION_BLOCKED {
            let own: Vec<(String,)> = sqlx::query_as("SELECT email FROM accounts")
                .fetch_all(&mut *tx)
                .await?;
            for (email,) in &own {
                let address = canonical_sender_address(email).to_lowercase();
                // S-019: адрес подключённого ящика заблокировать нельзя ни при
                // каком подтверждении.
                if kind == POLICY_KIND_ADDRESS && address == lowered {
                    return Err(crate::Error::AccountConfig(
                        "адрес подключённого ящика заблокировать нельзя".into(),
                    ));
                }
                // S-020: домен собственного ящика блокируется только с
                // отдельным подтверждением, собственные адреса при этом
                // остаются доверенными.
                if kind == POLICY_KIND_DOMAIN
                    && !confirm_own_domain
                    && address_domain(&address)
                        .is_some_and(|domain| domain_matches(domain, &lowered))
                {
                    return Err(crate::Error::AccountConfig(
                        "этому домену принадлежит адрес подключённого ящика: подтвердите блокировку, собственные адреса останутся доверенными".into(),
                    ));
                }
            }
        }
        // S-043, S-044: повтор возвращает существующую запись, а смена решения
        // выполняется одной неделимой операцией.
        sqlx::query(
            "INSERT INTO sender_policies(kind, value, decision)
             VALUES(?, ?, ?)
             ON CONFLICT(kind, lower(value)) DO UPDATE SET
                decision=excluded.decision,
                value=excluded.value,
                updated_at=datetime('now')",
        )
        .bind(kind)
        .bind(&canonical)
        .bind(decision)
        .execute(&mut *tx)
        .await?;
        let saved: SenderPolicy = sqlx::query_as(
            "SELECT id, kind, value, decision, created_at, updated_at, swept,
                    NULL AS job_state, last_error, 0 AS failed, 0 AS queue_failed,
                    NULL AS queue_error
               FROM sender_policies WHERE kind=? AND lower(value)=?",
        )
        .bind(kind)
        .bind(&lowered)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        tracing::info!(
            policy_id = saved.id,
            kind,
            decision,
            sender = %crate::logging::mask_email(&canonical),
            "запись списка отправителей сохранена"
        );
        Ok(saved)
    }

    /// Снять блокировку: запись удаляется, неисполненные перемещения по ней
    /// отменяются, а уже убранные письма остаются в корзине (S-039 - S-042).
    pub async fn delete_sender_policy(&self, id: i64) -> Result<SenderPolicyReleaseReport> {
        let mut tx = self.begin_write().await?;
        let marker = policy_marker(id);
        let swept: Option<(i64,)> = sqlx::query_as("SELECT swept FROM sender_policies WHERE id=?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
        // Удаление уже удалённой записи ошибкой не считается.
        let kept_in_trash = swept.map(|(value,)| value).unwrap_or(0);
        // S-042: операции, которые уже выполняются или выполнены, отменить
        // нельзя - их число называется пользователю.
        let irreversible = Self::count_irreversible_operations(&mut tx, &marker).await?;
        // S-041: отменяются только неисполненные операции, и письма при этом
        // возвращаются стадиям.
        let cancelled = Self::cancel_marked_operations(&mut tx, &marker).await?;
        sqlx::query("DELETE FROM sender_policies WHERE id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(SenderPolicyReleaseReport {
            cancelled,
            irreversible,
            kept_in_trash,
        })
    }

    /// Запустить уборку уже полученных писем по согласию пользователя и по
    /// ключу снимка (S-030 - S-033). Без согласия и снимка уборка не
    /// начинается.
    pub async fn start_sender_policy_sweep(
        &self,
        policy_id: i64,
        snapshot_key: &str,
        consent: bool,
    ) -> Result<Vec<SenderPolicySweepReport>> {
        if !consent {
            return Err(crate::Error::AccountConfig(
                "уборка уже полученных писем выполняется только по согласию".into(),
            ));
        }
        let mut tx = self.begin_write().await?;
        let policy: Option<(String, String)> =
            sqlx::query_as("SELECT kind, value FROM sender_policies WHERE id=?")
                .bind(policy_id)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((kind, value)) = policy else {
            return Err(crate::Error::AccountConfig(
                "запись списка не найдена".into(),
            ));
        };
        // Снимок расходуется неделимо: повторная команда с тем же ключом
        // второй уборки не запускает.
        let (payload, max_message_id, _) = Self::consume_stage_snapshot(
            &mut tx,
            snapshot_key,
            SNAPSHOT_KIND,
            self.limit(LIMIT_STAGE_SNAPSHOT_HOURS),
        )
        .await?;
        if payload != format!("{kind}\n{value}") {
            return Err(crate::Error::AccountConfig(
                "список писем относится к другой записи, откройте подтверждение заново".into(),
            ));
        }
        // S-033: уборка берёт совпавшие письма всех ящиков, поэтому задание
        // заводится на каждый ящик отдельно и отказ одного не отменяет
        // остальные.
        let accounts: Vec<(i64,)> = sqlx::query_as("SELECT id FROM accounts ORDER BY id")
            .fetch_all(&mut *tx)
            .await?;
        let mut created = Vec::new();
        for (account_id,) in accounts {
            let inserted: (i64,) = sqlx::query_as(
                "INSERT INTO sender_policy_jobs(policy_id, account_id, snapshot_key, max_message_id)
                 VALUES(?, ?, ?, ?) RETURNING id",
            )
            .bind(policy_id)
            .bind(account_id)
            .bind(snapshot_key)
            .bind(max_message_id)
            .fetch_one(&mut *tx)
            .await?;
            created.push(inserted.0);
        }
        tx.commit().await?;
        let mut reports = Vec::new();
        for job_id in created {
            reports.push(self.continue_sender_policy_sweep(job_id).await?);
        }
        Ok(reports)
    }

    /// Один проход уборки: не больше 500 перемещений за раз, остаток виден в
    /// отчёте (S-032, S-036).
    pub async fn continue_sender_policy_sweep(
        &self,
        job_id: i64,
    ) -> Result<SenderPolicySweepReport> {
        let mut tx = self.begin_write().await?;
        let job: Option<(i64, i64, String, i64, String)> = sqlx::query_as(
            "SELECT policy_id, account_id, snapshot_key, cursor_message_id, state
               FROM sender_policy_jobs WHERE id=?",
        )
        .bind(job_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((policy_id, account_id, snapshot_key, cursor, state)) = job else {
            return Err(crate::Error::AccountConfig(
                "задание уборки не найдено".into(),
            ));
        };
        if matches!(state.as_str(), "completed" | "cancelled" | "failed") {
            return self.sender_policy_job_report(job_id).await;
        }
        let policy: Option<(String, String)> =
            sqlx::query_as("SELECT kind, value FROM sender_policies WHERE id=?")
                .bind(policy_id)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((_, _)) = policy else {
            // Запись удалена во время уборки: задание закрывается, а уже
            // поставленные перемещения остаются на совести пользователя.
            sqlx::query(
                "UPDATE sender_policy_jobs SET state='cancelled', updated_at=datetime('now')
                  WHERE id=?",
            )
            .bind(job_id)
            .execute(&mut *tx)
            .await?;
            Self::clear_stage_deferrals(&mut tx, DEFERRAL_KIND, job_id).await?;
            tx.commit().await?;
            return self.sender_policy_job_report(job_id).await;
        };
        // S-037: аренда продлевается на время прохода.
        sqlx::query(
            "UPDATE sender_policy_jobs
                SET state='running', lease_expires_at=datetime('now', ?), updated_at=datetime('now')
              WHERE id=?",
        )
        .bind(format!("+{LEASE_SECONDS} seconds"))
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
        let trash = crate::storage::repo::resolve_role_folder(&mut tx, account_id, "trash").await?;
        // S-031, S-034: пачка берётся из строк снимка, поэтому отбор не зависит
        // ни от записи списка, ни от границы номеров. Письма, отложенные чужой
        // операцией, повторяются наравне с письмами после курсора.
        let sql = format!(
            "SELECT {STAGE_MESSAGE_COLUMNS}
               FROM messages m JOIN folders f ON f.id=m.folder_id
               JOIN stage_snapshot_messages s ON s.message_id=m.id AND s.key=?
              WHERE m.account_id=? AND {WORKING_FOLDERS}
                AND (m.id>? OR {DEFERRED_MESSAGES})
              ORDER BY m.id LIMIT ?"
        );
        let batch = sqlx::query_as::<_, StageMessage>(AssertSqlSafe(sql))
            .bind(&snapshot_key)
            .bind(account_id)
            .bind(cursor)
            .bind(DEFERRAL_KIND)
            .bind(job_id)
            .bind(self.limit(LIMIT_STAGE_BATCH))
            .fetch_all(&mut *tx)
            .await?;
        let mut counters = StageCounters::default();
        let mut last_id = cursor;
        let marker = policy_marker(policy_id);
        if let Some((folder_id, path)) = trash.clone() {
            for message in &batch {
                let outcome = queue_takeaway_operation(
                    &mut tx,
                    &message.takeaway(),
                    &TakeawayTarget::Folder {
                        id: folder_id,
                        path: path.clone(),
                    },
                    TakeawayActor::Stage,
                    Some(marker.as_str()),
                    0,
                )
                .await?;
                if counters.account(&outcome) {
                    sqlx::query("UPDATE messages SET closed_by_stage=? WHERE id=?")
                        .bind(SENDER_POLICY_STAGE_NAME)
                        .bind(message.id)
                        .execute(&mut *tx)
                        .await?;
                }
                if needs_retry(&outcome) {
                    Self::defer_stage_message(&mut tx, DEFERRAL_KIND, job_id, message.id).await?;
                } else {
                    Self::clear_stage_deferral(&mut tx, DEFERRAL_KIND, job_id, message.id).await?;
                }
                last_id = last_id.max(message.id);
            }
        } else {
            // S-003: ящик без корзины оставляет письма на месте, а причина
            // показывается в разделе отправителей. Откладывать их незачем:
            // без назначенной корзины повтор дал бы тот же отказ бесконечно.
            counters.failed += batch.len() as i64;
            for message in &batch {
                Self::clear_stage_deferral(&mut tx, DEFERRAL_KIND, job_id, message.id).await?;
                last_id = last_id.max(message.id);
            }
            sqlx::query(
                "UPDATE sender_policies SET last_error=?, updated_at=datetime('now') WHERE id=?",
            )
            .bind("в ящике нет папки с ролью корзины")
            .bind(policy_id)
            .execute(&mut *tx)
            .await?;
        }
        // S-036: остаток - письма снимка после курсора вместе с отложенными.
        let remaining_sql = format!(
            "SELECT count(*) FROM messages m JOIN folders f ON f.id=m.folder_id
               JOIN stage_snapshot_messages s ON s.message_id=m.id AND s.key=?
              WHERE m.account_id=? AND {WORKING_FOLDERS}
                AND (m.id>? OR {DEFERRED_MESSAGES})"
        );
        let remaining = sqlx::query_as::<_, (i64,)>(AssertSqlSafe(remaining_sql))
            .bind(&snapshot_key)
            .bind(account_id)
            .bind(last_id)
            .bind(DEFERRAL_KIND)
            .bind(job_id)
            .fetch_one(&mut *tx)
            .await?
            .0;
        let next_state = if remaining > 0 {
            "pending"
        } else {
            "completed"
        };
        if next_state == "completed" {
            Self::clear_stage_deferrals(&mut tx, DEFERRAL_KIND, job_id).await?;
        }
        sqlx::query(
            "UPDATE sender_policy_jobs
                SET state=?, cursor_message_id=?, queued=queued+?, skipped=skipped+?,
                    failed=failed+?, remaining=?, lease_expires_at=NULL,
                    updated_at=datetime('now')
              WHERE id=?",
        )
        .bind(next_state)
        .bind(last_id)
        .bind(counters.queued)
        .bind(counters.skipped)
        .bind(counters.failed)
        .bind(remaining)
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE sender_policies SET swept=swept+? WHERE id=?")
            .bind(counters.queued)
            .bind(policy_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.sender_policy_job_report(job_id).await
    }

    /// Отменить незавершённую уборку: новые перемещения не создаются, а уже
    /// неисполненные операции удаляются (S-041, S-042).
    pub async fn cancel_sender_policy_sweep(
        &self,
        job_id: i64,
    ) -> Result<SenderPolicyReleaseReport> {
        let mut tx = self.begin_write().await?;
        let job: Option<(i64,)> =
            sqlx::query_as("SELECT policy_id FROM sender_policy_jobs WHERE id=?")
                .bind(job_id)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((policy_id,)) = job else {
            return Err(crate::Error::AccountConfig(
                "задание уборки не найдено".into(),
            ));
        };
        let marker = policy_marker(policy_id);
        let irreversible = Self::count_irreversible_operations(&mut tx, &marker).await?;
        let cancelled = Self::cancel_marked_operations(&mut tx, &marker).await?;
        sqlx::query(
            "UPDATE sender_policy_jobs SET state='cancelled', lease_expires_at=NULL,
                    updated_at=datetime('now') WHERE id=?",
        )
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
        Self::clear_stage_deferrals(&mut tx, DEFERRAL_KIND, job_id).await?;
        tx.commit().await?;
        Ok(SenderPolicyReleaseReport {
            cancelled,
            irreversible,
            kept_in_trash: 0,
        })
    }

    pub async fn sender_policy_job_report(&self, job_id: i64) -> Result<SenderPolicySweepReport> {
        let report: SenderPolicySweepReport = sqlx::query_as(
            "SELECT id, state, queued, skipped, failed, remaining
               FROM sender_policy_jobs WHERE id=?",
        )
        .bind(job_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(report)
    }

    /// Незавершённые задания уборки для раздела настроек (S-045).
    pub async fn pending_sender_policy_jobs(&self) -> Result<Vec<SenderPolicySweepReport>> {
        let rows: Vec<SenderPolicySweepReport> = sqlx::query_as(
            "SELECT id, state, queued, skipped, failed, remaining
               FROM sender_policy_jobs WHERE state IN ('pending','running') ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Вернуть брошенные задания уборки в состояние ожидания (S-037, S-038).
    pub(crate) async fn restore_sender_policy_jobs(&self) -> Result<()> {
        sqlx::query(
            "UPDATE sender_policy_jobs
                SET state='pending', lease_expires_at=NULL, updated_at=datetime('now')
              WHERE state='running'",
        )
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// Продвинуть незавершённые задания уборки на одну пачку. Вызывается из
    /// конвейера стадий, поэтому уборка продолжается сама, без участия
    /// интерфейса.
    pub(crate) async fn advance_sender_policy_jobs(&self) -> Result<()> {
        // S-037: задание с истёкшей арендой считается брошенным и достаётся
        // следующему проходу наравне с ожидающими.
        let jobs: Vec<(i64,)> = sqlx::query_as(
            "SELECT id FROM sender_policy_jobs
              WHERE state='pending'
                 OR (state='running' AND (lease_expires_at IS NULL
                     OR datetime(lease_expires_at) <= datetime('now')))
              ORDER BY id LIMIT 4",
        )
        .fetch_all(&self.pool)
        .await?;
        for (job_id,) in jobs {
            self.continue_sender_policy_sweep(job_id).await?;
        }
        Ok(())
    }

    /// Стадия списков отправителей: первая в сквозном порядке стадий (S-001).
    /// Возвращает число писем, уведённых стадией.
    pub(crate) async fn process_sender_policy_stage(&self) -> Result<usize> {
        let mut tx = self.begin_write().await?;
        let policies = Self::load_policy_set(&mut tx).await?;
        if policies.is_empty() {
            // Курсор всё равно двигается: иначе первая же запись списка
            // разобрала бы всю историю писем разом.
            let newest = Self::max_message_id(&mut tx).await?;
            Self::set_stage_cursor(&mut tx, SENDER_POLICY_STAGE_NAME, newest).await?;
            tx.commit().await?;
            return Ok(0);
        }
        let cursor = Self::stage_cursor(&mut tx, SENDER_POLICY_STAGE_NAME).await?;
        // S-066: письма, отложенные прошлым проходом, повторяются наравне с
        // письмами после курсора. Их номера читаются заранее одним запросом:
        // снимать откладывание с каждого письма пачки означало бы пятьсот
        // лишних запросов на каждый приход почты, а отложенных писем обычно
        // нет вовсе.
        let deferred: Vec<i64> = sqlx::query_as::<_, (i64,)>(
            "SELECT message_id FROM stage_job_deferrals WHERE kind=? AND job_id=?",
        )
        .bind(STAGE_DEFERRAL_KIND)
        .bind(STAGE_DEFERRAL_JOB)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(|(id,)| id)
        .collect();
        // S-022, S-025: догруженные прокруткой письма стадия списков не берёт,
        // как и письма служебных папок и архива.
        let sql = format!(
            "SELECT {STAGE_MESSAGE_COLUMNS}
               FROM messages m JOIN folders f ON f.id=m.folder_id
              WHERE (m.id>? OR {DEFERRED_MESSAGES})
                AND m.backfilled=0 AND m.closed_by_stage IS NULL AND {WORKING_FOLDERS}
              ORDER BY m.id LIMIT ?"
        );
        let batch = sqlx::query_as::<_, StageMessage>(AssertSqlSafe(sql))
            .bind(cursor)
            .bind(STAGE_DEFERRAL_KIND)
            .bind(STAGE_DEFERRAL_JOB)
            .bind(self.limit(LIMIT_STAGE_BATCH))
            .fetch_all(&mut *tx)
            .await?;
        let mut queued = 0usize;
        let mut last_id = cursor;
        for message in &batch {
            // Отложенное письмо лежит до курсора, поэтому курсор от него назад
            // не двигается.
            last_id = last_id.max(message.id);
            let was_deferred = deferred.contains(&message.id);
            let SenderDecision::Blocked(policy_id) = policies.decide(message.from_addr.as_deref())
            else {
                // Отправителя могли убрать из списка, пока письмо ждало
                // повтора: тогда ждать больше нечего.
                if was_deferred {
                    Self::clear_stage_deferral(
                        &mut tx,
                        STAGE_DEFERRAL_KIND,
                        STAGE_DEFERRAL_JOB,
                        message.id,
                    )
                    .await?;
                }
                continue;
            };
            let trash =
                crate::storage::repo::resolve_role_folder(&mut tx, message.account_id, "trash")
                    .await?;
            let Some((folder_id, path)) = trash else {
                // S-003: письмо остаётся на месте, следующим стадиям не
                // передаётся и в уведомление о новой почте не попадает.
                sqlx::query("UPDATE messages SET closed_by_stage=? WHERE id=?")
                    .bind(SENDER_POLICY_STAGE_NAME)
                    .bind(message.id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(
                    "UPDATE sender_policies SET last_error=?, updated_at=datetime('now')
                      WHERE id=?",
                )
                .bind("в ящике нет папки с ролью корзины")
                .bind(policy_id)
                .execute(&mut *tx)
                .await?;
                // Повторять нечего: без назначенной корзины следующий проход
                // получил бы тот же отказ.
                if was_deferred {
                    Self::clear_stage_deferral(
                        &mut tx,
                        STAGE_DEFERRAL_KIND,
                        STAGE_DEFERRAL_JOB,
                        message.id,
                    )
                    .await?;
                }
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
                Some(policy_marker(policy_id).as_str()),
                0,
            )
            .await?;
            if let TakeawayOutcome::Queued(_) = outcome {
                // S-002, S-009: письмо закрыто стадией списков и дальше не
                // передаётся.
                sqlx::query("UPDATE messages SET closed_by_stage=? WHERE id=?")
                    .bind(SENDER_POLICY_STAGE_NAME)
                    .bind(message.id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("UPDATE sender_policies SET swept=swept+1 WHERE id=?")
                    .bind(policy_id)
                    .execute(&mut *tx)
                    .await?;
                queued += 1;
            }
            // S-004: второй операции увода по письму не создаётся. S-066:
            // чужая незавершённая операция, занятое письмо и отказ очереди
            // сами по себе не значат, что письмо убирать не нужно, поэтому
            // оно откладывается до следующего прохода. Без этого письмо
            // отправителя, прошлая операция которого ждёт решения
            // пользователя, оставалось бы во входящих навсегда: курсор уже
            // сдвинут, и стадия к нему не возвращается.
            if needs_retry(&outcome) {
                Self::defer_stage_message(
                    &mut tx,
                    STAGE_DEFERRAL_KIND,
                    STAGE_DEFERRAL_JOB,
                    message.id,
                )
                .await?;
            } else if was_deferred {
                Self::clear_stage_deferral(
                    &mut tx,
                    STAGE_DEFERRAL_KIND,
                    STAGE_DEFERRAL_JOB,
                    message.id,
                )
                .await?;
            }
        }
        if !deferred.is_empty() {
            // Отложенное письмо могло уйти совсем - его удалили или закрыла
            // другая стадия. Такие строки в остаток больше не приводят, и
            // копиться им незачем.
            sqlx::query(
                "DELETE FROM stage_job_deferrals
                  WHERE kind=? AND job_id=?
                    AND message_id NOT IN (SELECT id FROM messages WHERE closed_by_stage IS NULL)",
            )
            .bind(STAGE_DEFERRAL_KIND)
            .bind(STAGE_DEFERRAL_JOB)
            .execute(&mut *tx)
            .await?;
        }
        Self::set_stage_cursor(&mut tx, SENDER_POLICY_STAGE_NAME, last_id).await?;
        tx.commit().await?;
        Ok(queued)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::repo::test_storage::{TestDb, open_test_db};

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

    async fn seed_folder(db: &Db, account_id: i64, path: &str, role: Option<&str>) -> i64 {
        sqlx::query_as::<_, (i64,)>(
            "INSERT INTO folders(account_id, remote_path, display_name, role)
             VALUES(?, ?, ?, ?) RETURNING id",
        )
        .bind(account_id)
        .bind(path)
        .bind(path)
        .bind(role)
        .fetch_one(&db.write_pool)
        .await
        .expect("создать папку")
        .0
    }

    async fn seed_message(db: &Db, account_id: i64, folder_id: i64, uid: i64, from: &str) -> i64 {
        sqlx::query_as::<_, (i64,)>(
            "INSERT INTO messages(account_id, folder_id, uid, from_name, from_addr, subject,
                                  preview, date, remote_id, size)
             VALUES(?, ?, ?, 'Отправитель', ?, 'письмо', 'предпросмотр',
                    '2026-01-01T00:00:00+00:00', ?, 2048)
             RETURNING id",
        )
        .bind(account_id)
        .bind(folder_id)
        .bind(uid)
        .bind(from)
        .bind(format!("remote-{folder_id}-{uid}"))
        .fetch_one(&db.write_pool)
        .await
        .expect("сохранить письмо")
        .0
    }

    /// Операции увода по письму: вид, состояние и папка назначения.
    async fn takeaways(db: &Db, message_id: i64) -> Vec<(String, String, Option<i64>)> {
        sqlx::query_as(
            "SELECT op_kind, status, CAST(json_extract(payload,'$.target_folder_id') AS INTEGER)
               FROM outbox_ops WHERE message_id=? AND op_kind IN ('move','delete') ORDER BY id",
        )
        .bind(message_id)
        .fetch_all(&db.pool)
        .await
        .expect("прочитать очередь")
    }

    /// Довести операцию до состояния отказа настоящим путём: работник забирает
    /// её и сообщает об ошибке, пока не исчерпает разрешённое число попыток.
    /// Число попыток - настройка, поэтому берётся из реестра, а не из числа в
    /// проверке. Срок следующей попытки сдвигается запросом: ждать нарастающую
    /// паузу проверке незачем.
    async fn fail_until_refused(db: &TestDb, account_id: i64, operation_id: i64) {
        for _ in 0..=db.limit(LIMIT_OPERATION_ATTEMPTS) {
            sqlx::query(
                "UPDATE outbox_ops SET next_attempt_at=datetime('now','-1 hour') WHERE id=?",
            )
            .bind(operation_id)
            .execute(&db.write_pool)
            .await
            .expect("вернуть срок попытки");
            let claimed = db
                .claim_outbox_operations(account_id, 10)
                .await
                .expect("забрать операции");
            let Some(operation) = claimed.iter().find(|item| item.id == operation_id) else {
                break;
            };
            db.fail_outbox_operation(operation.id, "сервер отверг перемещение")
                .await
                .expect("сообщить об ошибке");
        }
        let status: (String,) = sqlx::query_as("SELECT status FROM outbox_ops WHERE id=?")
            .bind(operation_id)
            .fetch_one(&db.pool)
            .await
            .expect("состояние операции");
        assert_eq!(status.0, "failed", "операция должна дойти до отказа");
    }

    /// Письмо, прошлая операция которого ждёт решения пользователя, стадия
    /// списков не бросает: она откладывает его и возвращается к нему следующим
    /// проходом, хотя курсор уже ушёл вперёд (S-064, S-066, issue #114).
    /// Прежде такое письмо оставалось во входящих навсегда: стадия видела
    /// чужую незавершённую операцию, двигала курсор дальше и больше к письму
    /// не возвращалась.
    #[tokio::test]
    async fn a_message_with_a_refused_operation_waits_and_is_taken_after_the_user_decides() {
        let db = test_db("policy-stage-deferral").await;
        let account = seed_account(&db, "me@example.test").await;
        let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
        let archive = seed_folder(&db, account, "Archive", Some("archive")).await;
        let trash = seed_folder(&db, account, "Trash", Some("trash")).await;

        let blocked = seed_message(&db, account, inbox, 1, "spam@example.test").await;
        // Пользователь сам отправил письмо в архив, а сервер перемещение
        // отверг: операция дошла до отказа и ждёт решения.
        let queued = db
            .queue_message_action(&[blocked], "archive")
            .await
            .expect("перенос в архив");
        let refused = queued.operation_ids[0];
        fail_until_refused(&db, account, refused).await;

        db.save_sender_policy(
            POLICY_KIND_ADDRESS,
            "spam@example.test",
            POLICY_DECISION_BLOCKED,
            false,
        )
        .await
        .expect("запись блокировки");

        db.process_sync_batch_stages().await.expect("первый проход");
        let after_first = takeaways(&db, blocked).await;
        assert_eq!(
            after_first,
            vec![("move".to_string(), "failed".to_string(), Some(archive))],
            "пока пользователь не решил, второй операции по письму не ставится"
        );
        let closed: (Option<String>,) =
            sqlx::query_as("SELECT closed_by_stage FROM messages WHERE id=?")
                .bind(blocked)
                .fetch_one(&db.pool)
                .await
                .expect("признак стадии");
        assert!(
            closed.0.is_none(),
            "письмо осталось во входящих и закрытым стадией не считается"
        );

        // Курсор уходит вперёд на обычной почте: письмо ждёт решения, а работа
        // стадии не останавливается.
        let next = seed_message(&db, account, inbox, 2, "friend@example.test").await;
        db.process_sync_batch_stages().await.expect("второй проход");
        assert!(
            takeaways(&db, next).await.is_empty(),
            "письмо обычного отправителя стадия не трогает"
        );

        // Пользователь отказался от застрявшей операции - путь к письму открыт.
        db.discard_failed_operation(refused)
            .await
            .expect("отказ от операции");
        db.process_sync_batch_stages().await.expect("третий проход");
        let after_decision = takeaways(&db, blocked).await;
        assert_eq!(
            after_decision,
            vec![("move".to_string(), "pending".to_string(), Some(trash))],
            "после решения пользователя стадия возвращается к письму и уводит его в корзину"
        );
        let closed: (Option<String>,) =
            sqlx::query_as("SELECT closed_by_stage FROM messages WHERE id=?")
                .bind(blocked)
                .fetch_one(&db.pool)
                .await
                .expect("признак стадии");
        assert_eq!(
            closed.0.as_deref(),
            Some(SENDER_POLICY_STAGE_NAME),
            "письмо закрыто стадией списков"
        );
        let waiting: (i64,) =
            sqlx::query_as("SELECT count(*) FROM stage_job_deferrals WHERE kind=?")
                .bind(STAGE_DEFERRAL_KIND)
                .fetch_one(&db.pool)
                .await
                .expect("счёт отложенных");
        assert_eq!(
            waiting.0, 0,
            "обработанное письмо в остатке не задерживается"
        );
        db.close().await;
    }

    async fn test_db(prefix: &str) -> TestDb {
        open_test_db(prefix).await
    }

    /// Смешанные случаи адреса и домена разом: запись адреса точнее записи
    /// домена, доверие важнее блокировки одного вида, собственный адрес
    /// доверенный до всяких списков, а письмо без адреса отправителя идёт
    /// дальше (S-014 - S-018, S-026). Ошибка в любом из этих приоритетов
    /// уносит в корзину нужную почту или оставляет во входящих нежеланную.
    #[test]
    fn decision_priority_between_lists() {
        let set = SenderPolicySet::new(
            vec![
                (1, "domain".into(), "spam.test".into(), "blocked".into()),
                (
                    2,
                    "address".into(),
                    "friend@spam.test".into(),
                    "trusted".into(),
                ),
                (3, "domain".into(), "ok.spam.test".into(), "trusted".into()),
                (4, "domain".into(), "trusted.test".into(), "trusted".into()),
                (
                    5,
                    "address".into(),
                    "bad@trusted.test".into(),
                    "blocked".into(),
                ),
                (6, "domain".into(), "example.test".into(), "blocked".into()),
            ],
            vec!["me@example.test".into()],
        );
        assert_eq!(
            set.decide(Some("friend@spam.test")),
            SenderDecision::Trusted
        );
        assert_eq!(
            set.decide(Some("bad@trusted.test")),
            SenderDecision::Blocked(5)
        );
        assert_eq!(
            set.decide(Some("news@ok.spam.test")),
            SenderDecision::Trusted
        );
        assert_eq!(
            set.decide(Some("news@bad.spam.test")),
            SenderDecision::Blocked(1)
        );
        assert_eq!(set.decide(Some("ME@Example.test")), SenderDecision::Trusted);
        assert_eq!(
            set.decide(Some("other@example.test")),
            SenderDecision::Blocked(6)
        );
        assert_eq!(set.decide(Some("news@notspam.test")), SenderDecision::Pass);
        assert_eq!(set.decide(None), SenderDecision::Pass);
        assert_eq!(set.decide(Some("   ")), SenderDecision::Pass);
    }
}
