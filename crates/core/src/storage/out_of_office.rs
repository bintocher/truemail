//! Автоответ "нет на месте": локальная настройка, память об уже отправленных
//! ответах и пятая стадия разбора нового письма (specs/out-of-office.md).
//!
//! Серверный автоответ Exchange хранится на сервере и живёт в модуле Exchange:
//! общий интерфейс почтового модуля операциями отсутствия не расширяется
//! (S-009).

use super::Db;
use super::stages::STAGE_BATCH;
use crate::Result;
use crate::model::*;

/// Стадия автоответа выполняется после списков отправителей, игнорируемых
/// переписок, автоочистки и правил обработки (S-030).
const STAGE_NAME: &str = OUT_OF_OFFICE_STAGE_NAME;

/// Письмо в том виде, в каком его читает стадия автоответа: к общему снимку
/// стадии добавлены поля правил молчания.
#[derive(Debug, Clone, sqlx::FromRow)]
struct ReplyRow {
    id: i64,
    account_id: i64,
    folder_role: Option<String>,
    backfilled: i64,
    closed_by_stage: Option<String>,
    date: Option<String>,
    from_addr: Option<String>,
    subject: String,
    rfc822_message_id: Option<String>,
    to_addrs: Option<String>,
    cc_addrs: Option<String>,
    reply_to_addrs: Option<String>,
    is_newsletter: i64,
    auto_submitted: Option<String>,
    precedence: Option<String>,
    return_path_empty: i64,
    auto_response_suppress: Option<String>,
    silence_headers_known: i64,
}

impl Db {
    /// Настройка отсутствия ящика. Режим выбирает программа по виду серверного
    /// модуля и по текущей сборке, выбора пользователю не предлагается (S-002).
    pub async fn out_of_office_settings(&self, account_id: i64) -> Result<OutOfOfficeSettings> {
        let account: Option<(String, String)> =
            sqlx::query_as("SELECT email, provider FROM accounts WHERE id=?")
                .bind(account_id)
                .fetch_optional(&self.pool)
                .await?;
        let Some((email, provider)) = account else {
            return Err(crate::Error::AccountConfig("ящик не найден".into()));
        };
        let exchange = provider == "exchange";
        let windows_build = cfg!(windows);
        let mode = if exchange {
            OUT_OF_OFFICE_MODE_SERVER
        } else {
            OUT_OF_OFFICE_MODE_LOCAL
        };
        let stored: Option<StoredSettingsRow> = sqlx::query_as(
            "SELECT mode, enabled, starts_at, ends_at, internal_text, external_text,
                    internal_domains, version, server_checked_at, last_error, skipped_old
               FROM out_of_office_settings WHERE account_id=?",
        )
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?;
        let domains = match stored.as_ref() {
            Some(row) => serde_json::from_str(&row.internal_domains).unwrap_or_default(),
            // S-025: перечень по умолчанию - домен самого ящика и домены
            // остальных подключённых ящиков.
            None => self.default_internal_domains(account_id).await?,
        };
        Ok(OutOfOfficeSettings {
            account_id,
            mode: mode.to_owned(),
            enabled: stored.as_ref().is_some_and(|row| row.enabled != 0),
            starts_at: stored.as_ref().and_then(|row| row.starts_at.clone()),
            ends_at: stored.as_ref().and_then(|row| row.ends_at.clone()),
            internal_text: stored
                .as_ref()
                .map(|row| row.internal_text.clone())
                .unwrap_or_default(),
            external_text: stored
                .as_ref()
                .map(|row| row.external_text.clone())
                .unwrap_or_default(),
            internal_domains: domains,
            version: stored.as_ref().map(|row| row.version).unwrap_or(1),
            server_checked_at: stored
                .as_ref()
                .and_then(|row| row.server_checked_at.clone()),
            last_error: stored.as_ref().and_then(|row| row.last_error.clone()),
            skipped_old: stored.as_ref().map(|row| row.skipped_old).unwrap_or(0),
            // S-039: правило молчания по заголовку не применяется к письму, у
            // которого этого заголовка нет вовсе. Сами заголовки отдают все
            // модули: письма IMAP и JMAP разбираются из сырого письма,
            // проекция Gmail несёт их перечнем, а облегчённый запрос Exchange -
            // расширенными свойствами.
            silence_headers_available: true,
            // S-004: вне сборки Windows все запросы Exchange отклоняются, и
            // ящик не синхронизируется вовсе - подменять серверный автоответ
            // локальным программа не вправе.
            available: !exchange || windows_build,
            unavailable_reason: (exchange && !windows_build).then(|| {
                "автоответ ящика Exchange доступен только в сборке для Windows".to_owned()
            }),
            account_email: email,
        })
    }

    /// Домены подключённых ящиков: сам ящик первым, остальные по алфавиту, не
    /// больше предела (S-025).
    pub async fn default_internal_domains(&self, account_id: i64) -> Result<Vec<String>> {
        let rows: Vec<(i64, String)> = sqlx::query_as("SELECT id, email FROM accounts")
            .fetch_all(&self.pool)
            .await?;
        let mut own = Vec::new();
        let mut others = Vec::new();
        for (id, email) in rows {
            let Some(domain) =
                address_domain(&canonical_sender_address(&email).to_lowercase()).map(str::to_owned)
            else {
                continue;
            };
            if id == account_id {
                own.push(domain);
            } else {
                others.push(domain);
            }
        }
        others.sort();
        others.dedup();
        let mut domains = own;
        domains.dedup();
        for domain in others {
            if domains.len() >= MAX_INTERNAL_DOMAINS {
                break;
            }
            if !domains.contains(&domain) {
                domains.push(domain);
            }
        }
        Ok(domains)
    }

    /// Сохранить локальную настройку отсутствия. Границы периода, длины текстов
    /// и перечня доменов проверяет ядро (S-021 - S-027).
    pub async fn save_local_out_of_office(
        &self,
        input: &OutOfOfficeInput,
    ) -> Result<OutOfOfficeSettings> {
        let domains = validate_local_input(input).map_err(crate::Error::AccountConfig)?;
        let previous: Option<(i64,)> =
            sqlx::query_as("SELECT enabled FROM out_of_office_settings WHERE account_id=?")
                .bind(input.account_id)
                .fetch_optional(&self.pool)
                .await?;
        let mut tx = self.begin_write().await?;
        sqlx::query(
            "INSERT INTO out_of_office_settings(account_id, mode, enabled, starts_at, ends_at,
                                                internal_text, external_text, internal_domains,
                                                version, updated_at)
             VALUES(?, 'local', ?, ?, ?, ?, ?, ?, 1, datetime('now'))
             ON CONFLICT(account_id) DO UPDATE SET
                mode='local', enabled=excluded.enabled, starts_at=excluded.starts_at,
                ends_at=excluded.ends_at, internal_text=excluded.internal_text,
                external_text=excluded.external_text,
                internal_domains=excluded.internal_domains,
                version=out_of_office_settings.version+1,
                last_error=NULL, updated_at=datetime('now')",
        )
        .bind(input.account_id)
        .bind(input.enabled as i64)
        .bind(input.starts_at.trim())
        .bind(input.ends_at.trim())
        .bind(input.internal_text.trim())
        .bind(input.external_text.trim())
        .bind(serde_json::to_string(&domains)?)
        .execute(&mut *tx)
        .await?;
        // S-051: новое включение начинает период без унаследованного молчания,
        // иначе адресат, которому ответили в прошлый отпуск, остался бы без
        // ответа в новый.
        let was_enabled = previous.is_some_and(|(enabled,)| enabled != 0);
        let enabling = input.enabled && !was_enabled;
        if enabling {
            sqlx::query("UPDATE out_of_office_settings SET skipped_old=0 WHERE account_id=?")
                .bind(input.account_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM out_of_office_replies WHERE account_id=?")
                .bind(input.account_id)
                .execute(&mut *tx)
                .await?;
        }
        if !input.enabled {
            // S-067: отключение отменяет ещё не начатые автоответы этого ящика;
            // уже начатый доводится до итога и показывается отдельно (S-068).
            sqlx::query(
                "UPDATE outbox_ops SET status='cancelled', next_attempt_at=NULL
                  WHERE op_kind='send' AND send_origin='automatic' AND account_id=?
                    AND status IN ('pending','retry')
                    AND request_key LIKE 'oof:%'",
            )
            .bind(input.account_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.out_of_office_settings(input.account_id).await
    }

    /// Запомнить состояние, подтверждённое сервером Exchange. Показывается
    /// всегда оно, а не введённое пользователем (S-016, S-018).
    pub async fn save_server_out_of_office_state(
        &self,
        account_id: i64,
        state: &OutOfOfficeInput,
        error: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO out_of_office_settings(account_id, mode, enabled, starts_at, ends_at,
                                                internal_text, external_text, internal_domains,
                                                version, server_checked_at, last_error, updated_at)
             VALUES(?, 'server', ?, ?, ?, ?, ?, '[]', 1, datetime('now'), ?, datetime('now'))
             ON CONFLICT(account_id) DO UPDATE SET
                mode='server', enabled=excluded.enabled, starts_at=excluded.starts_at,
                ends_at=excluded.ends_at, internal_text=excluded.internal_text,
                external_text=excluded.external_text,
                server_checked_at=excluded.server_checked_at,
                last_error=excluded.last_error,
                version=out_of_office_settings.version+1,
                updated_at=datetime('now')",
        )
        .bind(account_id)
        .bind(state.enabled as i64)
        .bind(state.starts_at.trim())
        .bind(state.ends_at.trim())
        .bind(state.internal_text.trim())
        .bind(state.external_text.trim())
        .bind(error)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// Записать вид последней ошибки серверного режима, не трогая подтверждённое
    /// сервером состояние (S-018).
    pub async fn save_out_of_office_error(&self, account_id: i64, error: &str) -> Result<()> {
        sqlx::query(
            "UPDATE out_of_office_settings SET last_error=?, updated_at=datetime('now')
              WHERE account_id=?",
        )
        .bind(crate::logging::mask_error_text(error))
        .bind(account_id)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// Последние ответы ящика: их показывает раздел автоответа.
    pub async fn list_out_of_office_replies(
        &self,
        account_id: i64,
        limit: i64,
    ) -> Result<Vec<OutOfOfficeReply>> {
        let rows: Vec<(i64, String, String, Option<i64>, String, String)> = sqlx::query_as(
            "SELECT id, recipient_key, source_message_key, operation_id, state, replied_at
               FROM out_of_office_replies WHERE account_id=?
              ORDER BY replied_at DESC LIMIT ?",
        )
        .bind(account_id)
        .bind(limit.clamp(1, 200))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, recipient_key, source_message_key, operation_id, state, replied_at)| {
                    OutOfOfficeReply {
                        id,
                        recipient_key,
                        source_message_key,
                        operation_id,
                        state,
                        replied_at,
                    }
                },
            )
            .collect())
    }

    /// Записи ответов старше 60 суток удаляются пачками (S-050).
    pub async fn purge_out_of_office_replies(&self) -> Result<i64> {
        let result = sqlx::query(
            "DELETE FROM out_of_office_replies
              WHERE id IN (SELECT id FROM out_of_office_replies
                            WHERE replied_at < datetime('now', ?) LIMIT ?)",
        )
        .bind(format!("-{REPLY_MEMORY_DAYS} days"))
        .bind(REPLY_PURGE_BATCH)
        .execute(&self.write_pool)
        .await?;
        Ok(result.rows_affected() as i64)
    }

    /// Отметить итог передачи автоответа (S-421 одновременной работы: успешная
    /// передача переводит запись ответа в состояние отправленного).
    pub async fn mark_out_of_office_reply_state(
        &self,
        operation_id: i64,
        state: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE out_of_office_replies SET state=? WHERE operation_id=?")
            .bind(state)
            .bind(operation_id)
            .execute(&self.write_pool)
            .await?;
        Ok(())
    }

    /// Пятая стадия разбора нового письма: локальный автоответ. Письмо, закрытое
    /// предшествующей стадией, до неё не доходит вовсе (S-030, S-031).
    pub async fn process_out_of_office_stage(&self) -> Result<usize> {
        let settings: Vec<(i64, String, String, String, String, String, String)> = sqlx::query_as(
            "SELECT s.account_id, a.email, s.starts_at, s.ends_at, s.internal_text,
                    s.external_text, s.internal_domains
               FROM out_of_office_settings s JOIN accounts a ON a.id=s.account_id
              WHERE s.mode='local' AND s.enabled=1",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut tx = self.begin_write().await?;
        if settings.is_empty() {
            // Курсор двигается и без включённых настроек: иначе первое же
            // включение разобрало бы всю историю писем разом.
            let newest = Self::max_message_id(&mut tx).await?;
            Self::set_stage_cursor(&mut tx, STAGE_NAME, newest).await?;
            tx.commit().await?;
            return Ok(0);
        }
        let cursor = Self::stage_cursor(&mut tx, STAGE_NAME).await?;
        let batch: Vec<ReplyRow> = sqlx::query_as(
            "SELECT m.id, m.account_id, f.role AS folder_role, m.backfilled, m.closed_by_stage,
                    m.date, m.from_addr, m.subject, m.rfc822_message_id, m.to_addrs, m.cc_addrs,
                    m.reply_to_addrs, m.is_newsletter, m.auto_submitted, m.precedence,
                    m.return_path_empty, m.auto_response_suppress, m.silence_headers_known
               FROM messages m JOIN folders f ON f.id=m.folder_id
              WHERE m.id>? ORDER BY m.id LIMIT ?",
        )
        .bind(cursor)
        .bind(STAGE_BATCH)
        .fetch_all(&mut *tx)
        .await?;
        let own: Vec<String> = sqlx::query_as::<_, (String,)>("SELECT email FROM accounts")
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|(email,)| recipient_key(&email))
            .collect();
        let now = chrono::Utc::now();
        let mut queued = 0usize;
        let mut last_id = cursor;
        for row in &batch {
            last_id = row.id;
            let Some((_, email, starts_at, ends_at, internal_text, external_text, domains)) =
                settings
                    .iter()
                    .find(|item| item.0 == row.account_id)
                    .cloned()
            else {
                continue;
            };
            let candidate = row.to_candidate();
            let period_start = parse_time(&starts_at);
            let period_end = parse_time(&ends_at);
            match silence_reason(&candidate, &email, &own, period_start, period_end, now) {
                None => {}
                // S-065: письмо периода отсутствия, пришедшее пока программа не
                // работала, остаётся без ответа, и его число показывается
                // отдельным сообщением раздела.
                Some(SilenceReason::TooOld) => {
                    sqlx::query(
                        "UPDATE out_of_office_settings SET skipped_old=skipped_old+1
                          WHERE account_id=?",
                    )
                    .bind(row.account_id)
                    .execute(&mut *tx)
                    .await?;
                    continue;
                }
                Some(_) => continue,
            }
            let Some(destination) = reply_destination(&candidate) else {
                continue;
            };
            let key = recipient_key(&destination);
            let source_key = row
                .rfc822_message_id
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| format!("local:{}", row.id));
            // S-053: повторный разбор того же письма пользуется уже созданной
            // записью ответа и второй отправки не ставит.
            let inserted = sqlx::query(
                "INSERT OR IGNORE INTO out_of_office_replies(account_id, recipient_key,
                                                             source_message_key, state)
                 VALUES(?, ?, ?, 'queued')",
            )
            .bind(row.account_id)
            .bind(&key)
            .bind(&source_key)
            .execute(&mut *tx)
            .await?;
            if inserted.rows_affected() == 0 {
                continue;
            }
            let reply_id = inserted.last_insert_rowid();
            // S-048, S-052: окно молчания проверяется запросом внутри той же
            // неделимой операции - ограничение схемы не может зависеть от
            // текущего времени.
            let (recent,): (i64,) = sqlx::query_as(
                "SELECT count(*) FROM out_of_office_replies
                  WHERE account_id=? AND recipient_key=? AND id<>?
                    AND replied_at > datetime('now', ?)",
            )
            .bind(row.account_id)
            .bind(&key)
            .bind(reply_id)
            .bind(format!("-{SILENCE_WINDOW_DAYS} days"))
            .fetch_one(&mut *tx)
            .await?;
            if recent > 0 {
                sqlx::query("DELETE FROM out_of_office_replies WHERE id=?")
                    .bind(reply_id)
                    .execute(&mut *tx)
                    .await?;
                continue;
            }
            let domains: Vec<String> = serde_json::from_str(&domains).unwrap_or_default();
            // S-028, S-029: текст выбирается по домену адреса назначения ответа.
            let text = if is_internal_address(&destination, &domains) {
                internal_text
            } else {
                external_text
            };
            let message_id = crate::storage::outbox_send::fixed_message_id(&email);
            let body_ref = self.blobs.put(
                serde_json::to_string(&SendBody {
                    body_text: text,
                    body_html: None,
                })?
                .as_bytes(),
            )?;
            let payload = SendPayload {
                version: SEND_PAYLOAD_VERSION,
                from: email.clone(),
                to: vec![destination.clone()],
                cc: Vec::new(),
                bcc: Vec::new(),
                subject: reply_subject(&row.subject),
                message_id,
                body_ref,
                attachments: Vec::new(),
                headers: reply_headers(row.rfc822_message_id.as_deref()),
                raw_ref: None,
            };
            // S-054: ответ уходит общей очередью отправки с происхождением
            // automatic и нулевым окном отмены.
            let (operation_id, _) = crate::storage::outbox_send::insert_send_operation(
                &mut tx,
                row.account_id,
                &payload,
                SEND_ORIGIN_AUTOMATIC,
                Some(&format!("oof:{}:{}", row.account_id, row.id)),
                0,
            )
            .await?;
            sqlx::query("UPDATE out_of_office_replies SET operation_id=? WHERE id=?")
                .bind(operation_id)
                .bind(reply_id)
                .execute(&mut *tx)
                .await?;
            queued += 1;
        }
        if last_id > cursor {
            Self::set_stage_cursor(&mut tx, STAGE_NAME, last_id).await?;
        }
        tx.commit().await?;
        Ok(queued)
    }
}

impl ReplyRow {
    fn to_candidate(&self) -> ReplyCandidate {
        let addresses = |json: Option<&String>| {
            json.map(|value| serde_json::from_str::<Vec<Addr>>(value).unwrap_or_default())
                .unwrap_or_default()
        };
        ReplyCandidate {
            folder_role: self.folder_role.clone(),
            backfilled: self.backfilled != 0,
            closed_by_stage: self.closed_by_stage.clone(),
            received_at: self.date.as_deref().and_then(parse_time),
            from: self.from_addr.clone(),
            reply_to: addresses(self.reply_to_addrs.as_ref()),
            to: addresses(self.to_addrs.as_ref()),
            cc: addresses(self.cc_addrs.as_ref()),
            is_newsletter: self.is_newsletter != 0,
            auto_submitted: self.auto_submitted.clone(),
            precedence: self.precedence.clone(),
            return_path_empty: self.return_path_empty != 0,
            auto_response_suppress: self.auto_response_suppress.clone(),
            silence_headers_known: self.silence_headers_known != 0,
        }
    }
}

/// Строка сохранённой настройки.
#[derive(sqlx::FromRow)]
struct StoredSettingsRow {
    #[allow(dead_code)]
    mode: String,
    enabled: i64,
    starts_at: Option<String>,
    ends_at: Option<String>,
    internal_text: String,
    external_text: String,
    internal_domains: String,
    version: i64,
    server_checked_at: Option<String>,
    last_error: Option<String>,
    skipped_old: i64,
}

fn parse_time(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(value) {
        return Some(parsed.with_timezone(&chrono::Utc));
    }
    // Время, записанное самой базой ("2026-09-18 12:00:00"), зоны не несёт и
    // всегда означает всемирное: без этого разбора письмо выглядело бы письмом
    // без даты.
    chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|naive| naive.and_utc())
}

/// Запись ответа в разделе автоответа.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OutOfOfficeReply {
    pub id: i64,
    pub recipient_key: String,
    pub source_message_key: String,
    pub operation_id: Option<i64>,
    pub state: String,
    pub replied_at: String,
}
