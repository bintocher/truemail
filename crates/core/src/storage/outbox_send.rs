//! Очередь отправки письма: приём письма в очередь, окно отмены, захват
//! операции работником, отмена, неопределённый итог и возврат отменённого
//! письма в композер (specs/undo-send.md).
//!
//! Тело и вложения лежат в хранилище больших объектов, а в данных операции
//! остаются только ссылки: очередь перечитывается на каждом проходе работника,
//! и письмо предельного размера давало бы порядка 35 МБ строки (S-006).

use super::Db;
use crate::Result;
use crate::backend::{OutgoingAttachment, OutgoingMessage};
use crate::model::*;
use crate::storage::repo::OutboxOperation;

/// Как долго операция отправки считается захваченной работником. То же число,
/// что и у остальных видов операций; в автоматический захват операция отправки
/// в состоянии передачи всё равно не попадает (S-026).
const SEND_LEASE: &str = "+2 minutes";

/// Отсрочка, которую получает операция, не дошедшая до обращения к серверу из-за
/// недоступности сети. Попытку такой отказ не расходует (S-028).
const NETWORK_RETRY_SECONDS: i64 = 60;

impl Db {
    /// Длительность окна отмены. Несохранённое значение означает 5 секунд
    /// (S-011).
    pub async fn undo_send_seconds(&self) -> Result<i64> {
        let stored = self.setting(UNDO_SEND_SETTING).await?;
        Ok(stored
            .and_then(|value| value.trim().parse::<i64>().ok())
            .filter(|value| validate_undo_seconds(*value).is_ok())
            .unwrap_or(DEFAULT_UNDO_SECONDS))
    }

    /// Сохранить длительность окна отмены. Границы проверяет ядро независимо от
    /// интерфейса (S-012, S-013).
    pub async fn set_undo_send_seconds(&self, value: i64) -> Result<i64> {
        let value = validate_undo_seconds(value).map_err(crate::Error::AccountConfig)?;
        self.set_setting(UNDO_SEND_SETTING, &value.to_string())
            .await?;
        Ok(value)
    }

    /// Принять письмо в очередь отправки. Запись операции, ключа запроса и
    /// ссылок на большие объекты выполняется одной неделимой операцией (S-008);
    /// при её отказе большие объекты стираются, чтобы не остаться мусором.
    pub async fn queue_outgoing_send(
        &self,
        account_id: i64,
        message: OutgoingMessage,
        origin: &str,
        request_key: Option<String>,
        undo_seconds: i64,
    ) -> Result<SendQueued> {
        // S-005: адресаты проверяются до записи операции, поэтому непригодный
        // адрес не создаёт ожидающего письма вовсе.
        crate::backend::validate_outgoing(&message)?;
        let undo_seconds = validate_undo_seconds(undo_seconds)
            .map_err(crate::Error::AccountConfig)?
            .max(0);
        if let Some(key) = request_key.as_deref()
            && let Some(existing) = self.existing_send_request(account_id, key).await?
        {
            return Ok(existing);
        }
        let message_id = message
            .message_id
            .clone()
            .unwrap_or_else(|| fixed_message_id(&message.from));
        let body = SendBody {
            body_text: message.body_text.clone(),
            body_html: message.body_html.clone(),
        };
        // Большие объекты пишутся до неделимой записи: файл писателя базы не
        // держит, а неудачная запись их стирает.
        let mut written = Vec::new();
        let queued = async {
            let body_ref = self.blobs.put(serde_json::to_string(&body)?.as_bytes())?;
            written.push(body_ref.clone());
            let mut attachments = Vec::new();
            for item in &message.attachments {
                let blob_ref = self.blobs.put(&item.data)?;
                written.push(blob_ref.clone());
                attachments.push(SendPayloadAttachment {
                    filename: item.filename.clone(),
                    mime_type: item.mime_type.clone(),
                    blob_ref,
                    size: item.data.len() as i64,
                });
            }
            let payload = SendPayload {
                version: SEND_PAYLOAD_VERSION,
                from: message.from.clone(),
                to: message.to.clone(),
                cc: message.cc.clone(),
                bcc: message.bcc.clone(),
                subject: message.subject.clone(),
                message_id: message_id.clone(),
                body_ref,
                attachments,
                headers: message.headers.clone(),
                raw_ref: None,
            };
            let mut tx = self.begin_write().await?;
            let (operation_id, cancel_until) = insert_send_operation(
                &mut tx,
                account_id,
                &payload,
                origin,
                request_key.as_deref(),
                undo_seconds,
            )
            .await?;
            tx.commit().await?;
            Ok::<SendQueued, crate::Error>(SendQueued {
                operation_id,
                account_id,
                status: SEND_STATUS_PENDING.to_owned(),
                cancel_until,
                undo_seconds,
                duplicate: false,
            })
        }
        .await;
        match queued {
            Ok(value) => Ok(value),
            Err(error) => {
                // S-003: письмо не принято, значит и большие объекты остаться
                // не должны - иначе каждая неудачная отправка оставляла бы в
                // хранилище тело письма без ссылки на него.
                for reference in written {
                    let _ = self.blobs.remove(&reference);
                }
                Err(error)
            }
        }
    }

    /// Отложенная отправка по времени: передача начинается в заданное время, а
    /// окно отмены поверх него не добавляется - пользователь уже выбрал время
    /// сам (S-055). Отменить такую операцию можно из раздела "Исходящие", пока
    /// она не перешла в состояние передачи (S-056).
    pub async fn queue_scheduled_send(
        &self,
        account_id: i64,
        message: OutgoingMessage,
        send_at: &str,
    ) -> Result<SendQueued> {
        let queued = self
            .queue_outgoing_send(account_id, message, SEND_ORIGIN_SCHEDULED, None, 0)
            .await?;
        sqlx::query("UPDATE outbox_ops SET cancel_until=?, next_attempt_at=? WHERE id=?")
            .bind(send_at)
            .bind(send_at)
            .bind(queued.operation_id)
            .execute(&self.write_pool)
            .await?;
        Ok(SendQueued {
            cancel_until: send_at.to_owned(),
            ..queued
        })
    }

    /// Ранее созданная операция того же запроса (S-053).
    async fn existing_send_request(
        &self,
        account_id: i64,
        request_key: &str,
    ) -> Result<Option<SendQueued>> {
        let row: Option<(i64, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT k.operation_id, k.cancel_until, o.status
               FROM send_request_keys k
               LEFT JOIN outbox_ops o ON o.id = k.operation_id
              WHERE k.request_key=? AND k.account_id=?",
        )
        .bind(request_key)
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(operation_id, cancel_until, status)| SendQueued {
            operation_id,
            account_id,
            status: status.unwrap_or_else(|| SEND_STATUS_PENDING.to_owned()),
            cancel_until: cancel_until.unwrap_or_default(),
            undo_seconds: 0,
            duplicate: true,
        }))
    }

    /// Захватить одну готовую операцию отправки. Операции отправки берутся по
    /// одной и переводятся в состояние передачи непосредственно перед
    /// обращением к серверу: при захвате пачкой последнее письмо пачки
    /// показывалось бы начавшим передачу и его отмена отклонялась бы неверно
    /// (S-025). Состояние передачи в захват не попадает ни при каком сроке:
    /// иначе аварийное завершение во время передачи отправило бы письмо второй
    /// раз (S-026).
    pub async fn claim_send_operation(&self, account_id: i64) -> Result<Option<OutboxOperation>> {
        let row: Option<OutboxOperationRow> = sqlx::query_as(
            "UPDATE outbox_ops
                SET status='processing', next_attempt_at=datetime('now', ?)
              WHERE id = (
                SELECT id FROM outbox_ops
                 WHERE account_id=? AND op_kind='send' AND status IN ('pending','retry')
                   AND coalesce(cancel_until, created_at) <= datetime('now')
                   AND coalesce(next_attempt_at, created_at) <= datetime('now')
                 ORDER BY id LIMIT 1)
              RETURNING id, account_id, message_id, op_kind, payload, attempts",
        )
        .bind(SEND_LEASE)
        .bind(account_id)
        .fetch_optional(&self.write_pool)
        .await?;
        Ok(row.map(Into::into))
    }

    /// Перевести операции, застигнутые аварийным завершением в состоянии
    /// передачи, в неопределённый итог (S-027). Выполняется при запуске: решение
    /// о повторе остаётся за пользователем.
    pub async fn recover_sending_operations(&self) -> Result<i64> {
        let result = sqlx::query(
            "UPDATE outbox_ops
                SET status='uncertain', next_attempt_at=NULL,
                    last_error='программа завершилась во время передачи письма, итог неизвестен'
              WHERE op_kind='send' AND status='processing'",
        )
        .execute(&self.write_pool)
        .await?;
        Ok(result.rows_affected() as i64)
    }

    /// Отменить отправку. Отмена и захват работником спорят за одну строку:
    /// одно условное изменение решает спор без гонки (S-037, S-038).
    pub async fn cancel_send_operation(
        &self,
        account_id: i64,
        operation_id: i64,
    ) -> Result<CancelSendOutcome> {
        let mut tx = self.begin_write().await?;
        let changed = sqlx::query(
            "UPDATE outbox_ops SET status='cancelled', next_attempt_at=NULL
              WHERE id=? AND account_id=? AND op_kind='send'
                AND status IN ('pending','retry')",
        )
        .bind(operation_id)
        .bind(account_id)
        .execute(&mut *tx)
        .await?;
        if changed.rows_affected() == 1 {
            tx.commit().await?;
            return Ok(CancelSendOutcome::Cancelled);
        }
        let status: Option<(String,)> =
            sqlx::query_as("SELECT status FROM outbox_ops WHERE id=? AND account_id=?")
                .bind(operation_id)
                .bind(account_id)
                .fetch_optional(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok(match status.as_ref().map(|(value,)| value.as_str()) {
            Some(SEND_STATUS_PROCESSING) => CancelSendOutcome::AlreadySending,
            // Письма в очереди уже нет: сервер подтвердил принятие, и обещать
            // отзыв у получателей программа не вправе (S-044).
            _ => CancelSendOutcome::AlreadySent,
        })
    }

    /// Раздел "Исходящие": операции отправки выбранного ящика. Строится по
    /// данным очереди, строки в таблице папок для него не заводится (S-022).
    pub async fn list_outbox_sends(
        &self,
        account_id: Option<i64>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<OutboxSendEntry>> {
        let rows: Vec<OutboxSendListRow> = sqlx::query_as(
            "SELECT o.id, o.account_id, a.email, a.enabled, o.send_origin, o.status, o.payload,
                    o.cancel_until, o.next_attempt_at, o.attempts, o.last_error, o.created_at
               FROM outbox_ops o JOIN accounts a ON a.id=o.account_id
              WHERE o.op_kind IN ('send','append_sent')
                AND (? IS NULL OR o.account_id = ?)
              ORDER BY o.id DESC LIMIT ? OFFSET ?",
        )
        .bind(account_id)
        .bind(account_id)
        .bind(limit.clamp(1, 100))
        .bind(offset.max(0))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Отменённое письмо целиком: адресаты, тема, оформленное тело и все
    /// вложения (S-040). Операция при этом остаётся на месте: неудачный возврат
    /// в композер не должен потерять письмо.
    pub async fn cancelled_send_message(
        &self,
        account_id: i64,
        operation_id: i64,
    ) -> Result<CancelledSendMessage> {
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT payload, status FROM outbox_ops
              WHERE id=? AND account_id=? AND op_kind='send'",
        )
        .bind(operation_id)
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some((payload, status)) = row else {
            return Err(crate::Error::Other(
                "письмо уже не в очереди отправки".into(),
            ));
        };
        if status != SEND_STATUS_CANCELLED {
            return Err(crate::Error::Other(
                "вернуть в композер можно только отменённое письмо".into(),
            ));
        }
        let payload = self.read_send_payload(operation_id, &payload).await?;
        let body: SendBody = serde_json::from_slice(&self.blobs.get(&payload.body_ref)?)?;
        let mut attachments = Vec::new();
        for item in &payload.attachments {
            attachments.push(CancelledSendAttachment {
                filename: item.filename.clone(),
                mime_type: item.mime_type.clone(),
                data: self.blobs.get(&item.blob_ref)?,
            });
        }
        Ok(CancelledSendMessage {
            operation_id,
            account_id,
            from: payload.from,
            to: payload.to,
            cc: payload.cc,
            bcc: payload.bcc,
            subject: payload.subject,
            body_text: body.body_text,
            body_html: body.body_html,
            attachments,
        })
    }

    /// Удалить операцию отправки вместе с её большими объектами. Другой копии
    /// письма у программы нет, поэтому команду даёт только пользователь (S-043).
    pub async fn delete_send_operation(&self, account_id: i64, operation_id: i64) -> Result<()> {
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT payload, status FROM outbox_ops
              WHERE id=? AND account_id=? AND op_kind IN ('send','append_sent')",
        )
        .bind(operation_id)
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some((payload, status)) = row else {
            return Err(crate::Error::Other("операция отправки не найдена".into()));
        };
        if matches!(status.as_str(), SEND_STATUS_PROCESSING) {
            return Err(crate::Error::Other(
                "отправка уже началась, удалить письмо нельзя".into(),
            ));
        }
        let references = serde_json::from_str::<SendPayload>(&payload)
            .map(|payload| payload.blob_refs())
            .unwrap_or_default();
        let deleted = sqlx::query("DELETE FROM outbox_ops WHERE id=? AND account_id=?")
            .bind(operation_id)
            .bind(account_id)
            .execute(&self.write_pool)
            .await?;
        if deleted.rows_affected() == 1 {
            for reference in references {
                let _ = self.blobs.remove(&reference);
            }
        }
        Ok(())
    }

    /// Ручной повтор отправки с неопределённым итогом или с окончательным
    /// отказом. Закреплённый идентификатор письма сохраняется, ключ запроса
    /// выдаётся новый (S-051).
    pub async fn retry_send_operation(&self, account_id: i64, operation_id: i64) -> Result<()> {
        let changed = sqlx::query(
            "UPDATE outbox_ops
                SET status='pending', attempts=0, last_error=NULL,
                    request_key=NULL,
                    cancel_until=datetime('now'), next_attempt_at=datetime('now')
              WHERE id=? AND account_id=? AND op_kind IN ('send','append_sent')
                AND status IN ('uncertain','failed','cancelled')",
        )
        .bind(operation_id)
        .bind(account_id)
        .execute(&self.write_pool)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(crate::Error::Other(
                "повторить можно только отправку с неопределённым итогом или отказом".into(),
            ));
        }
        Ok(())
    }

    /// Отказ, случившийся до обращения к серверу из-за недоступности сети или
    /// учётных данных: операция остаётся ожидающей и попытку не расходует
    /// (S-028, S-048).
    pub async fn defer_send_operation(&self, operation_id: i64, error: &str) -> Result<()> {
        let reason = crate::logging::mask_error_text(error);
        sqlx::query(
            "UPDATE outbox_ops
                SET status='retry', last_error=?, next_attempt_at=datetime('now', ?)
              WHERE id=?",
        )
        .bind(reason.chars().take(1000).collect::<String>())
        .bind(format!("+{NETWORK_RETRY_SECONDS} seconds"))
        .bind(operation_id)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// Обращение к серверу началось и оборвалось: достоверно непринятым письмо
    /// считать нельзя, потому что общий интерфейс серверного модуля не
    /// различает отказ приёма и потерянный ответ (S-049).
    pub async fn mark_send_uncertain(&self, operation_id: i64, error: &str) -> Result<()> {
        let reason = crate::logging::mask_error_text(error);
        sqlx::query(
            "UPDATE outbox_ops
                SET status='uncertain', next_attempt_at=NULL, last_error=?
              WHERE id=? AND op_kind='send'",
        )
        .bind(reason.chars().take(1000).collect::<String>())
        .bind(operation_id)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// Превратить отправку в дозапись копии в папку с ролью `sent`. Ссылки на
    /// большие объекты переходят к новой операции в той же записи, а точные
    /// байты письма кладутся в хранилище и в данные операции строкой не
    /// попадают (S-009, S-052).
    pub async fn convert_send_to_sent_append(
        &self,
        operation_id: i64,
        raw: &[u8],
        error: &str,
    ) -> Result<()> {
        let (payload,): (String,) = sqlx::query_as("SELECT payload FROM outbox_ops WHERE id=?")
            .bind(operation_id)
            .fetch_one(&self.pool)
            .await?;
        let mut payload = self.read_send_payload(operation_id, &payload).await?;
        let raw_ref = self.blobs.put(raw)?;
        payload.raw_ref = Some(raw_ref.clone());
        let serialized = serde_json::to_string(&payload)?;
        let updated = sqlx::query(
            "UPDATE outbox_ops
                SET op_kind='append_sent', payload=?, status='retry', attempts=0,
                    next_attempt_at=datetime('now','+5 seconds'), cancel_until=NULL,
                    last_error=?
              WHERE id=?",
        )
        .bind(&serialized)
        .bind(
            crate::logging::mask_error_text(error)
                .chars()
                .take(1000)
                .collect::<String>(),
        )
        .bind(operation_id)
        .execute(&self.write_pool)
        .await?;
        if updated.rows_affected() != 1 {
            let _ = self.blobs.remove(&raw_ref);
        }
        Ok(())
    }

    /// Прочитать данные операции отправки. Данные старого формата - письмо
    /// строкой - переносятся в хранилище больших объектов при первом чтении и
    /// получают закреплённый идентификатор письма (S-046).
    pub async fn read_send_payload(&self, operation_id: i64, payload: &str) -> Result<SendPayload> {
        if let Ok(parsed) = serde_json::from_str::<SendPayload>(payload)
            && parsed.version >= SEND_PAYLOAD_VERSION
        {
            return Ok(parsed);
        }
        let legacy: OutgoingMessage = serde_json::from_str(payload)?;
        let message_id = legacy
            .message_id
            .clone()
            .unwrap_or_else(|| fixed_message_id(&legacy.from));
        let body_ref = self.blobs.put(
            serde_json::to_string(&SendBody {
                body_text: legacy.body_text.clone(),
                body_html: legacy.body_html.clone(),
            })?
            .as_bytes(),
        )?;
        let mut attachments = Vec::new();
        for item in &legacy.attachments {
            attachments.push(SendPayloadAttachment {
                filename: item.filename.clone(),
                mime_type: item.mime_type.clone(),
                blob_ref: self.blobs.put(&item.data)?,
                size: item.data.len() as i64,
            });
        }
        let payload = SendPayload {
            version: SEND_PAYLOAD_VERSION,
            from: legacy.from,
            to: legacy.to,
            cc: legacy.cc,
            bcc: legacy.bcc,
            subject: legacy.subject,
            message_id: message_id.clone(),
            body_ref,
            attachments,
            headers: legacy.headers,
            raw_ref: None,
        };
        sqlx::query(
            "UPDATE outbox_ops SET payload=?, fixed_message_id=coalesce(fixed_message_id, ?)
              WHERE id=?",
        )
        .bind(serde_json::to_string(&payload)?)
        .bind(&message_id)
        .bind(operation_id)
        .execute(&self.write_pool)
        .await?;
        Ok(payload)
    }

    /// Собрать письмо операции обратно из хранилища больших объектов.
    pub async fn outgoing_from_payload(&self, payload: &SendPayload) -> Result<OutgoingMessage> {
        let body: SendBody = serde_json::from_slice(&self.blobs.get(&payload.body_ref)?)?;
        let mut attachments = Vec::new();
        for item in &payload.attachments {
            attachments.push(OutgoingAttachment {
                filename: item.filename.clone(),
                mime_type: item.mime_type.clone(),
                data: self.blobs.get(&item.blob_ref)?,
            });
        }
        Ok(OutgoingMessage {
            from: payload.from.clone(),
            to: payload.to.clone(),
            cc: payload.cc.clone(),
            bcc: payload.bcc.clone(),
            subject: payload.subject.clone(),
            body_text: body.body_text,
            body_html: body.body_html,
            attachments,
            message_id: Some(payload.message_id.clone()),
            headers: payload.headers.clone(),
        })
    }

    /// Точные байты письма, уже переданные серверу: их дозаписывает в папку с
    /// ролью `sent` операция дозаписи копии.
    pub fn sent_append_bytes(&self, payload: &SendPayload) -> Result<Vec<u8>> {
        let reference = payload.raw_ref.as_deref().ok_or_else(|| {
            crate::Error::AccountConfig("у дозаписи копии нет сохранённых байтов письма".into())
        })?;
        self.blobs.get(reference)
    }

    /// Сколько секунд осталось до ближайшего срока отмены этого ящика. Работник
    /// ждёт этот срок и не опрашивает базу каждую секунду (S-024).
    pub async fn next_send_wakeup_seconds(&self, account_id: i64) -> Result<Option<i64>> {
        let row: Option<(Option<f64>,)> = sqlx::query_as(
            "SELECT min((julianday(coalesce(cancel_until, created_at)) - julianday('now')) * 86400.0)
               FROM outbox_ops
              WHERE account_id=? AND op_kind='send' AND status IN ('pending','retry')",
        )
        .bind(account_id)
        .fetch_optional(&self.write_pool)
        .await?;
        Ok(row
            .and_then(|(seconds,)| seconds)
            .map(|seconds| seconds.ceil().max(0.0) as i64))
    }

    /// Состояние очереди отправки на запуске программы: сколько писем дождалось
    /// конца окна отмены, пока программа не работала, и сколько операций ждёт
    /// решения пользователя (S-034 - S-036).
    pub async fn startup_send_state(&self) -> Result<StartupSendState> {
        let (expired, uncertain): (i64, i64) = sqlx::query_as(
            "SELECT
               sum(CASE WHEN status IN ('pending','retry')
                         AND coalesce(cancel_until, created_at) <= datetime('now')
                        THEN 1 ELSE 0 END),
               sum(CASE WHEN status='uncertain' THEN 1 ELSE 0 END)
             FROM outbox_ops WHERE op_kind='send'",
        )
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or((0, 0));
        // Незакончившиеся окна отмены восстанавливаются карточкой с фактически
        // оставшимся временем (S-034).
        let rows: Vec<(i64, i64, String, String)> = sqlx::query_as(
            "SELECT id, account_id, status, cancel_until
               FROM outbox_ops
              WHERE op_kind='send' AND status='pending' AND send_origin='ordinary'
                AND cancel_until > datetime('now')
              ORDER BY cancel_until",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(StartupSendState {
            expired,
            uncertain,
            pending: rows
                .into_iter()
                .map(
                    |(operation_id, account_id, status, cancel_until)| SendQueued {
                        operation_id,
                        account_id,
                        status,
                        cancel_until,
                        undo_seconds: 0,
                        duplicate: false,
                    },
                )
                .collect(),
        })
    }

    /// Немедленно отпустить письма, ждущие окна отмены: пользователь выбрал
    /// отправить их перед выходом из программы (S-031).
    pub async fn release_undo_windows(&self) -> Result<i64> {
        let result = sqlx::query(
            "UPDATE outbox_ops
                SET cancel_until=datetime('now'), next_attempt_at=datetime('now')
              WHERE op_kind='send' AND status='pending' AND cancel_until > datetime('now')",
        )
        .execute(&self.write_pool)
        .await?;
        Ok(result.rows_affected() as i64)
    }

    /// Удалить ключи запросов старше 30 суток пачками. Незавершённые операции
    /// это не трогает: удаляется только память о запросе (S-054).
    pub async fn purge_send_request_keys(&self) -> Result<i64> {
        let result = sqlx::query(
            "DELETE FROM send_request_keys
              WHERE request_key IN (
                SELECT request_key FROM send_request_keys
                 WHERE created_at < datetime('now', ?) LIMIT ?)",
        )
        .bind(format!("-{REQUEST_KEY_DAYS} days"))
        .bind(REQUEST_KEY_PURGE_BATCH)
        .execute(&self.write_pool)
        .await?;
        Ok(result.rows_affected() as i64)
    }

    /// Ссылки исходящих писем: сборка мусора считает их достижимыми наравне со
    /// ссылками писем, вложений, контактов и событий (S-007).
    pub(crate) async fn outgoing_blob_references(&self) -> Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT payload FROM outbox_ops WHERE op_kind IN ('send','append_sent')",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(payload,)| serde_json::from_str::<SendPayload>(&payload).ok())
            .flat_map(|payload| payload.blob_refs())
            .collect())
    }
}

/// Записать операцию отправки и ключ её запроса внутри уже открытой неделимой
/// операции. Отдельная функция нужна автоответу: он проверяет окно молчания,
/// записывает запись ответа и ставит отправку одной неделимой операцией
/// (specs/out-of-office.md, S-052).
pub(crate) async fn insert_send_operation(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    account_id: i64,
    payload: &SendPayload,
    origin: &str,
    request_key: Option<&str>,
    undo_seconds: i64,
) -> Result<(i64, String)> {
    let serialized = serde_json::to_string(payload)?;
    // S-015: срок отмены считается по длительности, действующей в момент
    // приёма, и хранится абсолютным временем. Изменение настройки после этого
    // уже назначенный срок не двигает.
    let (cancel_until,): (String,) = sqlx::query_as("SELECT datetime('now', ?)")
        .bind(format!("+{undo_seconds} seconds"))
        .fetch_one(&mut **tx)
        .await?;
    let (operation_id,): (i64,) = sqlx::query_as(
        "INSERT INTO outbox_ops(account_id, op_kind, payload, status, attempts,
                                next_attempt_at, request_key, fixed_message_id,
                                send_origin, cancel_until)
         VALUES(?, 'send', ?, 'pending', 0, ?, ?, ?, ?, ?)
         RETURNING id",
    )
    .bind(account_id)
    .bind(&serialized)
    .bind(&cancel_until)
    .bind(request_key)
    .bind(&payload.message_id)
    .bind(origin)
    .bind(&cancel_until)
    .fetch_one(&mut **tx)
    .await?;
    if let Some(key) = request_key {
        sqlx::query(
            "INSERT INTO send_request_keys(request_key, account_id, operation_id,
                                           outcome, cancel_until)
             VALUES(?, ?, ?, 'queued', ?)",
        )
        .bind(key)
        .bind(account_id)
        .bind(operation_id)
        .bind(&cancel_until)
        .execute(&mut **tx)
        .await?;
    }
    Ok((operation_id, cancel_until))
}

/// Закреплённый идентификатор письма. Домен берётся из адреса отправителя:
/// идентификатор с чужим доменом отдельные серверы считают подозрительным.
pub fn fixed_message_id(from: &str) -> String {
    let domain = from
        .rsplit_once('@')
        .map(|(_, domain)| domain.trim_end_matches('>').trim())
        .filter(|domain| !domain.is_empty())
        .unwrap_or("truemail.local");
    format!("<{}@{domain}>", uuid::Uuid::new_v4())
}

/// Строка списка "Исходящие" в том виде, в каком её отдаёт база.
#[derive(sqlx::FromRow)]
struct OutboxSendListRow {
    id: i64,
    account_id: i64,
    email: String,
    enabled: i64,
    send_origin: Option<String>,
    status: String,
    payload: String,
    cancel_until: Option<String>,
    next_attempt_at: Option<String>,
    attempts: i64,
    last_error: Option<String>,
    created_at: String,
}

impl From<OutboxSendListRow> for OutboxSendEntry {
    fn from(row: OutboxSendListRow) -> Self {
        // Данные операции старого формата тоже показываются в списке: письмо
        // отложенной отправки лежит в них целиком.
        let (subject, to, attachments) = match serde_json::from_str::<SendPayload>(&row.payload) {
            Ok(payload) => (
                payload.subject,
                payload.to,
                payload.attachments.len() as i64,
            ),
            Err(_) => match serde_json::from_str::<OutgoingMessage>(&row.payload) {
                Ok(message) => (
                    message.subject,
                    message.to,
                    message.attachments.len() as i64,
                ),
                Err(_) => (String::new(), Vec::new(), 0),
            },
        };
        Self {
            id: row.id,
            account_id: row.account_id,
            account_email: row.email,
            account_enabled: row.enabled != 0,
            origin: row
                .send_origin
                .unwrap_or_else(|| SEND_ORIGIN_ORDINARY.to_owned()),
            status: row.status,
            subject,
            to,
            cancel_until: row.cancel_until,
            next_attempt_at: row.next_attempt_at,
            attempts: row.attempts,
            last_error: row.last_error,
            created_at: row.created_at,
            attachments,
        }
    }
}

/// Строка захваченной операции: те же столбцы, что и у пачки остальных видов.
#[derive(sqlx::FromRow)]
struct OutboxOperationRow {
    id: i64,
    account_id: i64,
    message_id: Option<i64>,
    op_kind: String,
    payload: String,
    attempts: i64,
}

impl From<OutboxOperationRow> for OutboxOperation {
    fn from(row: OutboxOperationRow) -> Self {
        Self {
            id: row.id,
            account_id: row.account_id,
            message_id: row.message_id,
            op_kind: row.op_kind,
            payload: row.payload,
            attempts: row.attempts,
        }
    }
}
