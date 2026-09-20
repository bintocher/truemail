//! История получателей по каждому ящику: запись обращений, первичное
//! заполнение по папкам с ролью `sent`, объединение с контактами и раздел
//! управления (specs/recipient-history.md).
//!
//! Прежний сбор корреспондентов заводил контакт на каждый встреченный адрес и
//! перечитывал всю почту ящика после каждой записи писем. Здесь адрес попадает
//! в историю только тогда, когда пользователь действительно писал на него.

use super::Db;
use crate::Result;
use crate::model::*;
use sqlx::AssertSqlSafe;

/// Время истории в том же виде, в каком его пишет само обращение: с буквой T и
/// смещением. Время базы ("2026-09-18 12:00:00") сравнивалось бы с ним как
/// строка неверно, и убранный адрес возвращался бы в видимые записи тем же
/// днём (S-046 - S-048).
const HISTORY_NOW_SQL: &str = "strftime('%Y-%m-%dT%H:%M:%S+00:00','now')";

/// Ранг записи выражением базы: веса считаются на стороне SQLite, чтобы не
/// поднимать в память до 50 отметок на каждую из 2000 записей (S-028).
const RANK_SQL: &str = "(SELECT coalesce(sum(CASE
            WHEN julianday('now') - julianday(t.used_at) < 30 THEN 3
            WHEN julianday('now') - julianday(t.used_at) < 90 THEN 2
            WHEN julianday('now') - julianday(t.used_at) < 365 THEN 1
            ELSE 0 END), 0)
      FROM recipient_history_touches t WHERE t.history_id = h.id)";

/// Строка отправленного письма при пополнении истории: номер, ключ письма,
/// адресаты полей "Кому" и "Копия" и дата.
type SentMessageRow = (
    i64,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// Строка раздела управления историей в том виде, в каком её отдаёт база.
type HistoryEntryRow = (i64, i64, String, String, i64, i64, Option<String>, i64, i64);

/// Почтовый контакт прежнего сбора: номер, ящик, имя, опознаватель, признак
/// избранного, признак скрытия и единственный адрес.
type MailContactRow = (i64, Option<i64>, String, Option<String>, i64, i64, String);

/// Откуда пришло обращение. Собственная подтверждённая отправка снимает
/// скрытие записи пользователем (S-049), а письмо, вычитанное из папки с ролью
/// `sent`, подчиняется времени скрытия и границе очистки (S-048).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchOrigin {
    OwnSend,
    SentFolder,
}

/// Одно обращение: адрес, показываемое имя, ключ письма и время.
#[derive(Debug, Clone)]
pub struct RecipientTouch {
    pub email: String,
    pub name: String,
    pub message_key: String,
    pub used_at: String,
}

impl Db {
    /// Записать обращения к адресатам одного письма. Отметка добавляется
    /// выражением базы в одной неделимой операции с обновлением времени
    /// последнего обращения, а ключ письма не даёт учесть одно письмо дважды
    /// (S-016, S-017, S-055).
    pub async fn record_recipient_touches(
        &self,
        account_id: i64,
        touches: &[RecipientTouch],
        origin: TouchOrigin,
    ) -> Result<i64> {
        if touches.is_empty() {
            return Ok(0);
        }
        let own = self.own_account_addresses().await?;
        let mut tx = self.begin_write().await?;
        let cleared_at: Option<String> = sqlx::query_as::<_, (Option<String>,)>(
            "SELECT cleared_at FROM recipient_history_state WHERE account_id=?",
        )
        .bind(account_id)
        .fetch_optional(&mut *tx)
        .await?
        .and_then(|(value,)| value);
        let mut recorded = 0;
        // Один адрес одного письма даёт ровно одно обращение, даже если он
        // встретился и в поле "Кому", и в поле "Копия" (S-016). Ключ пары -
        // письмо и адрес: в одной пачке приходят адресаты разных писем, и
        // общий ключ по адресу потерял бы все письма, кроме первого.
        let mut seen = std::collections::HashSet::new();
        for touch in touches {
            let key = canonical_sender_address(&touch.email).to_lowercase();
            if key.is_empty() || !key.contains('@') || key.len() > MAX_HISTORY_ADDRESS_BYTES {
                continue;
            }
            // S-019: собственный адрес подключённого ящика в историю не идёт.
            if own.contains(&key) || !seen.insert((touch.message_key.clone(), key.clone())) {
                continue;
            }
            // S-048: граница очистки защищает и те адреса, которые ещё не
            // прочитаны из старых отправленных писем.
            if origin == TouchOrigin::SentFolder
                && cleared_at
                    .as_deref()
                    .is_some_and(|cleared| touch.used_at.as_str() <= cleared)
            {
                continue;
            }
            let address = canonical_sender_address(touch.email.trim());
            let row: (i64, i64, Option<String>) = sqlx::query_as(
                "INSERT INTO recipient_history(account_id, address_key, display_address,
                                               display_name, last_used_at)
                 VALUES(?, ?, ?, ?, ?)
                 ON CONFLICT(account_id, address_key) DO UPDATE SET
                    display_address=excluded.display_address,
                    display_name=CASE
                        WHEN recipient_history.name_edited=1 THEN recipient_history.display_name
                        WHEN excluded.display_name<>'' THEN excluded.display_name
                        ELSE recipient_history.display_name END,
                    last_used_at=CASE
                        WHEN excluded.last_used_at > coalesce(recipient_history.last_used_at,'')
                        THEN excluded.last_used_at ELSE recipient_history.last_used_at END,
                    -- S-052: вытеснение пределом снимается любым новым
                    -- обращением: запись убрал предел, а не решение человека.
                    evicted=0, evicted_at=NULL
                 RETURNING id, hidden_by_user, hidden_at",
            )
            .bind(account_id)
            .bind(&key)
            .bind(&address)
            .bind(touch.name.trim())
            .bind(&touch.used_at)
            .fetch_one(&mut *tx)
            .await?;
            let (history_id, hidden, hidden_at) = row;
            if hidden != 0 {
                let unhide = origin == TouchOrigin::OwnSend
                    || (touch.used_at.as_str() > hidden_at.as_deref().unwrap_or("")
                        && touch.used_at.as_str() > cleared_at.as_deref().unwrap_or(""));
                if unhide {
                    sqlx::query(
                        "UPDATE recipient_history SET hidden_by_user=0, hidden_at=NULL WHERE id=?",
                    )
                    .bind(history_id)
                    .execute(&mut *tx)
                    .await?;
                }
            }
            let inserted = sqlx::query(
                "INSERT OR IGNORE INTO recipient_history_touches(history_id, message_key, used_at)
                 VALUES(?, ?, ?)",
            )
            .bind(history_id)
            .bind(&touch.message_key)
            .bind(&touch.used_at)
            .execute(&mut *tx)
            .await?;
            if inserted.rows_affected() == 0 {
                continue;
            }
            recorded += 1;
            // S-027: у записи остаются только последние 50 отметок.
            sqlx::query(
                "DELETE FROM recipient_history_touches
                  WHERE history_id=? AND id NOT IN (
                    SELECT id FROM recipient_history_touches
                     WHERE history_id=? ORDER BY used_at DESC, id DESC LIMIT ?)",
            )
            .bind(history_id)
            .bind(history_id)
            .bind(self.limit(LIMIT_RECIPIENT_TOUCHES))
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        if recorded > 0 {
            self.evict_recipient_history_overflow(account_id).await?;
        }
        Ok(recorded)
    }

    /// Собственные адреса подключённых ящиков в каноническом виде.
    pub(crate) async fn own_account_addresses(&self) -> Result<std::collections::HashSet<String>> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT email FROM accounts")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|(email,)| canonical_sender_address(&email).to_lowercase())
            .collect())
    }

    /// Убрать лишние записи сверх предела видимых. Вытеснение отмечается
    /// отдельным признаком: пользователь не должен путать его со своим
    /// решением, а вытесненная запись возвращается новым обращением (S-051).
    async fn evict_recipient_history_overflow(&self, account_id: i64) -> Result<i64> {
        let (visible,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM recipient_history
              WHERE account_id=? AND hidden_by_user=0 AND evicted=0",
        )
        .bind(account_id)
        .fetch_one(&self.pool)
        .await?;
        let max_visible = self.limit(LIMIT_RECIPIENT_ENTRIES);
        if visible <= max_visible {
            return Ok(0);
        }
        let extra = visible - max_visible;
        let sql = format!(
            "UPDATE recipient_history SET evicted=1, evicted_at=datetime('now')
              WHERE id IN (
                SELECT h.id FROM recipient_history h
                 WHERE h.account_id=? AND h.hidden_by_user=0 AND h.evicted=0
                 ORDER BY {RANK_SQL} ASC, coalesce(h.last_used_at,'') ASC, h.id ASC
                 LIMIT ?)"
        );
        let result = sqlx::query(AssertSqlSafe(sql))
            .bind(account_id)
            .bind(extra)
            .execute(&self.write_pool)
            .await?;
        Ok(result.rows_affected() as i64)
    }

    /// Запомнить закреплённый идентификатор своего отправленного письма: по нему
    /// его копия, появившаяся в папке с ролью `sent`, узнаётся и второго
    /// обращения не добавляет (S-010, S-011).
    pub async fn record_own_send(&self, account_id: i64, fixed_message_id: &str) -> Result<()> {
        if fixed_message_id.trim().is_empty() {
            return Ok(());
        }
        sqlx::query(
            "INSERT OR IGNORE INTO recipient_own_sends(account_id, fixed_message_id)
             VALUES(?, ?)",
        )
        .bind(account_id)
        .bind(fixed_message_id.trim())
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// Отметки собственных отправок старше 30 суток удаляются фоновым
    /// обслуживанием (S-010).
    pub async fn purge_recipient_own_sends(&self) -> Result<i64> {
        let result =
            sqlx::query("DELETE FROM recipient_own_sends WHERE sent_at < datetime('now', ?)")
                .bind(format!(
                    "-{} days",
                    self.limit(LIMIT_RECIPIENT_OWN_SEND_DAYS)
                ))
                .execute(&self.write_pool)
                .await?;
        Ok(result.rows_affected() as i64)
    }

    /// Пополнить историю из папок с ролью `sent`. Курсор пополнения только
    /// ускоряет проход: защиту от повторного счёта даёт ключ письма (S-018).
    /// Проход идёт пачками до исчерпания писем: на ящике с тысячами отправленных
    /// писем остановка после первой пачки оставила бы историю неполной, объявив
    /// первичное заполнение законченным (S-008).
    pub async fn advance_recipient_history(&self, account_id: i64) -> Result<i64> {
        let mut cursor: i64 = sqlx::query_as::<_, (i64,)>(
            "SELECT cursor_message_id FROM recipient_history_state WHERE account_id=?",
        )
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?
        .map(|(value,)| value)
        .unwrap_or(0);
        let mut recorded = 0;
        loop {
            let batch = self.recipient_history_batch(account_id, cursor).await?;
            if batch.is_empty() {
                break;
            }
            let (last_id, added) = self.record_sent_batch(account_id, batch).await?;
            recorded += added;
            cursor = last_id;
            self.advance_recipient_history_cursor(account_id, cursor, false)
                .await?;
        }
        self.advance_recipient_history_cursor(account_id, cursor, true)
            .await?;
        Ok(recorded)
    }

    /// Очередная пачка уже сохранённых отправленных писем ящика.
    async fn recipient_history_batch(
        &self,
        account_id: i64,
        cursor: i64,
    ) -> Result<Vec<SentMessageRow>> {
        let rows: Vec<SentMessageRow> = sqlx::query_as(
            "SELECT m.id, m.rfc822_message_id, m.to_addrs, m.cc_addrs, m.date
                   FROM messages m JOIN folders f ON f.id=m.folder_id
                  WHERE m.account_id=? AND f.role='sent' AND m.id>?
                  ORDER BY m.id LIMIT ?",
        )
        .bind(account_id)
        .bind(cursor)
        .bind(self.limit(LIMIT_PURGE_BATCH))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Записать обращения целой пачки писем одной неделимой операцией. Прежде
    /// каждое письмо открывало свою запись и перечитывало адреса ящиков, и
    /// первичное заполнение давало сотни записей на проход (S-008).
    async fn record_sent_batch(
        &self,
        account_id: i64,
        rows: Vec<SentMessageRow>,
    ) -> Result<(i64, i64)> {
        let own_sends: std::collections::HashSet<String> = sqlx::query_as::<_, (String,)>(
            "SELECT fixed_message_id FROM recipient_own_sends WHERE account_id=?",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|(value,)| value)
        .collect();
        let mut last_id = 0;
        let mut touches = Vec::new();
        for (message_id, rfc_id, to_json, cc_json, date) in rows {
            last_id = message_id;
            // S-011: своя копия обращения не удваивает - оно уже записано в
            // момент подтверждения сервером.
            if rfc_id
                .as_deref()
                .is_some_and(|value| own_sends.contains(value))
            {
                continue;
            }
            // S-017: ключ письма - Message-ID, а при его отсутствии локальный
            // номер письма; пересборка папки меняет номера, поэтому именно ключ
            // защищает от повторного счёта.
            let message_key = rfc_id
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| format!("local:{message_id}"));
            // S-026: у письма без разобранной даты берётся время его первой
            // локальной вставки, поэтому обращение не оказывается в 1970 году.
            let used_at = normalize_touch_time(date.as_deref());
            for json in [to_json, cc_json] {
                let Some(json) = json else { continue };
                for address in serde_json::from_str::<Vec<Addr>>(&json).unwrap_or_default() {
                    touches.push(RecipientTouch {
                        email: address.email,
                        name: address.name.unwrap_or_default(),
                        message_key: message_key.clone(),
                        used_at: used_at.clone(),
                    });
                }
            }
        }
        let recorded = self
            .record_recipient_touches(account_id, &touches, TouchOrigin::SentFolder)
            .await?;
        Ok((last_id, recorded))
    }

    /// Двинуть курсор пополнения вперёд. Признак завершённого первичного
    /// заполнения ставится только по исчерпании писем (S-008, S-018).
    async fn advance_recipient_history_cursor(
        &self,
        account_id: i64,
        cursor: i64,
        done: bool,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO recipient_history_state(account_id, cursor_message_id, initial_done,
                                                 updated_at)
             VALUES(?, ?, ?, datetime('now'))
             ON CONFLICT(account_id) DO UPDATE SET
                cursor_message_id=max(recipient_history_state.cursor_message_id,
                                      excluded.cursor_message_id),
                initial_done=max(recipient_history_state.initial_done, excluded.initial_done),
                updated_at=datetime('now')",
        )
        .bind(account_id)
        .bind(cursor)
        .bind(done as i64)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// Кандидаты подсказки: история выбранного ящика, объединённая с контактами
    /// по ключу адреса, уже упорядоченная ядром (S-031, S-035).
    pub async fn recipient_candidates(&self, account_id: i64) -> Result<Vec<RecipientCandidate>> {
        let sql = format!(
            "SELECT h.display_address, h.display_name, {RANK_SQL},
                    (SELECT count(*) FROM recipient_history_touches t WHERE t.history_id=h.id),
                    h.last_used_at
               FROM recipient_history h
              WHERE h.account_id=? AND h.hidden_by_user=0 AND h.evicted=0"
        );
        let history: Vec<(String, String, i64, i64, Option<String>)> =
            sqlx::query_as(AssertSqlSafe(sql))
                .bind(account_id)
                .fetch_all(&self.pool)
                .await?;
        let mut merged: std::collections::HashMap<String, RecipientCandidate> =
            std::collections::HashMap::new();
        for (address, name, rank, touches, last_used_at) in history {
            let key = address.to_lowercase();
            merged.insert(
                key,
                RecipientCandidate {
                    email: address,
                    name,
                    source: CANDIDATE_SOURCE_HISTORY.to_owned(),
                    favorite: false,
                    rank,
                    touches,
                    last_used_at,
                },
            );
        }
        // S-040: один адрес в нескольких контактах даёт один кандидат -
        // избранный контакт, затем контакт выбранного ящика, затем контакт с
        // наименьшим номером.
        let contacts: Vec<(String, String, i64, Option<i64>, i64)> = sqlx::query_as(
            "SELECT ce.email, c.display_name, c.is_favorite, c.account_id, c.id
               FROM contact_emails ce JOIN contacts c ON c.id=ce.contact_id
              WHERE c.hidden=0
              ORDER BY c.is_favorite DESC,
                       CASE WHEN c.account_id=? THEN 0 ELSE 1 END, c.id",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        let mut taken = std::collections::HashSet::new();
        for (email, display_name, favorite, _, _) in contacts {
            let key = email.trim().to_lowercase();
            if key.is_empty() || !taken.insert(key.clone()) {
                continue;
            }
            match merged.get_mut(&key) {
                Some(existing) => {
                    existing.source = CANDIDATE_SOURCE_BOTH.to_owned();
                    existing.favorite = existing.favorite || favorite != 0;
                    // S-036: у объединённого кандидата с непустым именем
                    // контакта показывается имя контакта.
                    if !display_name.trim().is_empty() {
                        existing.name = display_name;
                    }
                }
                None => {
                    merged.insert(
                        key,
                        RecipientCandidate {
                            email: email.trim().to_owned(),
                            name: display_name,
                            source: CANDIDATE_SOURCE_CONTACT.to_owned(),
                            favorite: favorite != 0,
                            rank: 0,
                            touches: 0,
                            last_used_at: None,
                        },
                    );
                }
            }
        }
        let mut candidates: Vec<RecipientCandidate> = merged
            .into_values()
            .map(|mut candidate| {
                // S-038: имени нет ни у контакта, ни у записи - показывается сам
                // адрес.
                if candidate.name.trim().is_empty() {
                    candidate.name = candidate.email.clone();
                }
                candidate
            })
            .collect();
        order_candidates(&mut candidates);
        Ok(candidates)
    }

    /// Раздел управления историей: записи выбранного ящика страницами (S-042).
    pub async fn list_recipient_history(
        &self,
        account_id: i64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<RecipientHistoryEntry>> {
        let rows: Vec<HistoryEntryRow> = sqlx::query_as(
            "SELECT h.id, h.account_id, h.display_address, h.display_name, h.name_edited,
                        (SELECT count(*) FROM recipient_history_touches t WHERE t.history_id=h.id),
                        h.last_used_at, h.hidden_by_user, h.evicted
                   FROM recipient_history h
                  WHERE h.account_id=?
                  ORDER BY coalesce(h.last_used_at,'') DESC, h.id DESC
                  LIMIT ? OFFSET ?",
        )
        .bind(account_id)
        .bind(limit.clamp(1, HISTORY_PAGE))
        .bind(offset.max(0))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    account_id,
                    address,
                    name,
                    name_edited,
                    saved_touches,
                    last_used_at,
                    hidden,
                    evicted,
                )| RecipientHistoryEntry {
                    id,
                    account_id,
                    address,
                    name,
                    name_edited: name_edited != 0,
                    saved_touches,
                    last_used_at,
                    hidden_by_user: hidden != 0,
                    evicted: evicted != 0,
                },
            )
            .collect())
    }

    /// Изменить имя и адрес записи. Адрес и его ключ меняются одной неделимой
    /// операцией, а занятый ключ не сливает записи молча (S-043 - S-045).
    pub async fn update_recipient_history_entry(
        &self,
        account_id: i64,
        entry_id: i64,
        name: Option<String>,
        address: Option<String>,
    ) -> Result<()> {
        let mut tx = self.begin_write().await?;
        if let Some(address) = address.as_deref() {
            let normalized =
                normalize_policy_address(address).map_err(crate::Error::AccountConfig)?;
            if normalized.len() > MAX_HISTORY_ADDRESS_BYTES {
                return Err(crate::Error::AccountConfig(format!(
                    "адрес длиннее {MAX_HISTORY_ADDRESS_BYTES} байт"
                )));
            }
            let key = normalized.to_lowercase();
            let taken: Option<(i64, String)> = sqlx::query_as(
                "SELECT id, display_address FROM recipient_history
                  WHERE account_id=? AND address_key=? AND id<>?",
            )
            .bind(account_id)
            .bind(&key)
            .bind(entry_id)
            .fetch_optional(&mut *tx)
            .await?;
            if let Some((_, existing)) = taken {
                return Err(crate::Error::AccountConfig(format!(
                    "адрес уже есть в истории этого ящика: {existing}"
                )));
            }
            sqlx::query(
                "UPDATE recipient_history SET address_key=?, display_address=?
                  WHERE id=? AND account_id=?",
            )
            .bind(&key)
            .bind(&normalized)
            .bind(entry_id)
            .bind(account_id)
            .execute(&mut *tx)
            .await?;
        }
        if let Some(name) = name {
            sqlx::query(
                "UPDATE recipient_history SET display_name=?, name_edited=1
                  WHERE id=? AND account_id=?",
            )
            .bind(name.trim())
            .bind(entry_id)
            .bind(account_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Убрать адрес из истории. Строка сохраняется скрытой вместе со временем
    /// скрытия: иначе старое отправленное письмо вернуло бы её при следующей
    /// синхронизации (S-046).
    pub async fn hide_recipient_history_entry(&self, account_id: i64, entry_id: i64) -> Result<()> {
        sqlx::query(AssertSqlSafe(format!(
            "UPDATE recipient_history
                SET hidden_by_user=1, hidden_at={HISTORY_NOW_SQL}, evicted=0, evicted_at=NULL
              WHERE id=? AND account_id=?"
        )))
        .bind(entry_id)
        .bind(account_id)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// Очистить историю ящика: граница очистки сохраняется, а видимые записи
    /// становятся скрытыми пользователем (S-047).
    pub async fn clear_recipient_history(&self, account_id: i64) -> Result<i64> {
        let mut tx = self.begin_write().await?;
        let hidden = sqlx::query(AssertSqlSafe(format!(
            "UPDATE recipient_history
                SET hidden_by_user=1, hidden_at={HISTORY_NOW_SQL}
              WHERE account_id=? AND hidden_by_user=0"
        )))
        .bind(account_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(AssertSqlSafe(format!(
            "INSERT INTO recipient_history_state(account_id, cleared_at, updated_at)
             VALUES(?, {HISTORY_NOW_SQL}, datetime('now'))
             ON CONFLICT(account_id) DO UPDATE SET
                cleared_at={HISTORY_NOW_SQL}, updated_at=datetime('now')"
        )))
        .bind(account_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(hidden.rows_affected() as i64)
    }

    /// Перенести почтовые контакты прежнего сбора в историю получателей.
    /// Контакт, который пользователь дополнил телефоном, почтовым адресом,
    /// фотографией, признаком избранного или вторым адресом, остаётся в
    /// адресной книге как есть: история хранит только имя и один адрес, и
    /// перенос стёр бы остальные данные (S-003, S-004).
    pub async fn migrate_mail_contacts_to_history(&self) -> Result<ContactMigrationReport> {
        let rows: Vec<MailContactRow> = sqlx::query_as(
            "SELECT c.id, c.account_id, c.display_name, c.uid, c.is_favorite, c.hidden,
                        (SELECT ce.email FROM contact_emails ce WHERE ce.contact_id=c.id
                          ORDER BY ce.id LIMIT 1)
                   FROM contacts c
                  WHERE c.account_id IS NOT NULL
                    AND c.uid LIKE 'mail:%'
                    AND c.vcard_ref IS NULL
                    AND c.etag IS NULL
                    AND c.photo_blob_ref IS NULL
                    AND c.is_favorite=0
                    AND (SELECT count(*) FROM contact_emails ce WHERE ce.contact_id=c.id)=1
                    AND (SELECT count(*) FROM contact_phones p WHERE p.contact_id=c.id)=0
                    AND (SELECT count(*) FROM contact_addresses a WHERE a.contact_id=c.id)=0",
        )
        .fetch_all(&self.pool)
        .await?;
        if rows.is_empty() {
            return Ok(ContactMigrationReport::default());
        }
        let mut report = ContactMigrationReport::default();
        // S-054 ошибок и частичных отказов: перенос откатывается целиком, чтобы
        // адрес не исчез из адресной книги, не появившись в истории.
        let mut tx = self.begin_write().await?;
        for (contact_id, account_id, display_name, _, _, hidden, email) in rows {
            let Some(account_id) = account_id else {
                report.kept += 1;
                continue;
            };
            let key = canonical_sender_address(&email).to_lowercase();
            if key.is_empty() || !key.contains('@') {
                report.kept += 1;
                continue;
            }
            sqlx::query(
                "INSERT INTO recipient_history(account_id, address_key, display_address,
                                               display_name, hidden_by_user, hidden_at)
                 VALUES(?, ?, ?, ?, ?,
                        CASE WHEN ?=1 THEN strftime('%Y-%m-%dT%H:%M:%S+00:00','now')
                             ELSE NULL END)
                 ON CONFLICT(account_id, address_key) DO UPDATE SET
                    display_name=CASE WHEN recipient_history.display_name=''
                                      THEN excluded.display_name
                                      ELSE recipient_history.display_name END",
            )
            .bind(account_id)
            .bind(&key)
            .bind(canonical_sender_address(email.trim()))
            .bind(if display_name.contains('@') {
                String::new()
            } else {
                display_name
            })
            // S-005: скрытый почтовый контакт становится записью, скрытой
            // пользователем, иначе убранный ранее адрес всплыл бы после
            // обновления.
            .bind(hidden)
            .bind(hidden)
            .execute(&mut *tx)
            .await?;
            sqlx::query("DELETE FROM contacts WHERE id=?")
                .bind(contact_id)
                .execute(&mut *tx)
                .await?;
            report.moved += 1;
        }
        tx.commit().await?;
        tracing::info!(
            moved = report.moved,
            kept = report.kept,
            "почтовые контакты прежнего сбора перенесены в историю получателей"
        );
        Ok(report)
    }
}

/// Время обращения: время письма в будущем заменяется текущим, а письмо без
/// разобранной даты получает текущее время своей первой локальной вставки
/// (S-025, S-026).
fn normalize_touch_time(date: Option<&str>) -> String {
    let now = chrono::Utc::now();
    let formatted =
        |value: chrono::DateTime<chrono::Utc>| value.format("%Y-%m-%dT%H:%M:%S+00:00").to_string();
    let Some(date) = date.map(str::trim).filter(|value| !value.is_empty()) else {
        return formatted(now);
    };
    match chrono::DateTime::parse_from_rfc3339(date) {
        Ok(parsed) if parsed.with_timezone(&chrono::Utc) <= now => date.to_owned(),
        Ok(_) => formatted(now),
        Err(_) => formatted(now),
    }
}
