//! Репозитории: типобезопасные запросы к таблицам.

use super::Db;
use crate::Result;
use crate::model::*;

#[derive(Debug, Clone, serde::Serialize)]
pub struct QueuedAction {
    pub operation_ids: Vec<i64>,
}

/// Результат сохранения календарей/контактов провайдера: счётчики плюс
/// смысловые изменения встреч (для будущих уведомлений - показ отдельным
/// этапом, здесь только вычисляем дельту).
#[derive(Debug, Clone, Default)]
pub struct AuxiliarySaveResult {
    pub calendars: usize,
    pub events: usize,
    pub contacts: usize,
    pub changes: Vec<CalendarChange>,
}

/// Смысловое изменение встречи, вычисленное сравнением со старым состоянием
/// в БД. При полном снимке (SyncScope::Full, первая синхронизация) не
/// формируется вовсе - иначе первый sync даст сотни "новых встреч".
#[derive(Debug, Clone)]
pub struct CalendarChange {
    pub kind: CalendarChangeKind,
    pub calendar_id: i64,
    pub event_id: i64,
    pub summary: String,
    pub start: Option<String>,
    pub previous_start: Option<String>,
    pub previous_summary: Option<String>,
    pub location: Option<String>,
    pub organizer: Option<String>,
    pub attendee_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalendarChangeKind {
    Created,
    Rescheduled,
    Cancelled,
    Renamed,
    LocationChanged,
    AttendeesChanged,
}

/// Контекст события для ответа на приглашение - см. Db::event_for_response.
#[derive(Debug, Clone)]
pub struct EventResponseContext {
    pub account_id: i64,
    pub calendar_source: String,
    pub remote_url: Option<String>,
    pub etag: Option<String>,
    pub event: Event,
}

/// Прежнее состояние строки events, нужное для сравнения при upsert.
/// Ключ в HashMap - (uid, recurrence_id.unwrap_or_default()), совпадает с
/// уникальным индексом uq_events_calendar_uid (migrations/0011).
#[derive(Debug, Clone, sqlx::FromRow)]
struct PreviousEventRow {
    id: i64,
    uid: Option<String>,
    recurrence_id: Option<String>,
    summary: String,
    dtstart: String,
    dtend: Option<String>,
    status: Option<String>,
    location: Option<String>,
}

fn is_cancelled_status(status: Option<&str>) -> bool {
    status.is_some_and(|value| value.eq_ignore_ascii_case("CANCELLED"))
}

/// Классифицировать изменение одной встречи по старому и новому состоянию.
/// Приоритет строгий для первых трёх видов: Created, затем Rescheduled,
/// затем Cancelled - они взаимоисключающие и обрывают дальнейшие проверки,
/// иначе на одну встречу пришло бы сразу несколько разнородных уведомлений.
/// Место и состав участников проверяются, только если ни один из первых
/// трёх не сработал, но независимо друг от друга - если поменялось и то,
/// и другое разом, в changes попадут оба. Изменение только description и
/// прочих полей изменением не считается.
#[allow(clippy::too_many_arguments)]
fn classify_event_change(
    calendar_id: i64,
    event_id: i64,
    new_summary: &str,
    new_dtstart: &str,
    new_dtend: Option<&str>,
    new_status: Option<&str>,
    new_location: Option<&str>,
    new_organizer: Option<&str>,
    new_attendees: &std::collections::HashSet<String>,
    previous: Option<&PreviousEventRow>,
    previous_attendees: Option<&std::collections::HashSet<String>>,
    changes: &mut Vec<CalendarChange>,
) {
    let Some(previous) = previous else {
        changes.push(CalendarChange {
            kind: CalendarChangeKind::Created,
            calendar_id,
            event_id,
            summary: new_summary.to_owned(),
            start: Some(new_dtstart.to_owned()),
            previous_start: None,
            previous_summary: None,
            location: new_location.map(str::to_owned),
            organizer: new_organizer.map(str::to_owned),
            attendee_count: new_attendees.len(),
        });
        return;
    };
    let time_changed = previous.dtstart != new_dtstart || previous.dtend.as_deref() != new_dtend;
    if time_changed && !is_cancelled_status(new_status) {
        changes.push(CalendarChange {
            kind: CalendarChangeKind::Rescheduled,
            calendar_id,
            event_id,
            summary: new_summary.to_owned(),
            start: Some(new_dtstart.to_owned()),
            previous_start: Some(previous.dtstart.clone()),
            previous_summary: None,
            location: new_location.map(str::to_owned),
            organizer: new_organizer.map(str::to_owned),
            attendee_count: new_attendees.len(),
        });
        return;
    }
    if is_cancelled_status(new_status) && !is_cancelled_status(previous.status.as_deref()) {
        changes.push(CalendarChange {
            kind: CalendarChangeKind::Cancelled,
            calendar_id,
            event_id,
            summary: new_summary.to_owned(),
            start: Some(new_dtstart.to_owned()),
            previous_start: None,
            previous_summary: None,
            location: new_location.map(str::to_owned),
            organizer: new_organizer.map(str::to_owned),
            attendee_count: new_attendees.len(),
        });
        return;
    }
    // Переименование сравнивается по значению из RETURNING, а не по тому, что
    // прислал сервер: у огрызка summary пустое, но COALESCE вернул прежнее -
    // значит здесь оно совпадёт с previous и ложного "переименована" не будет.
    if previous.summary != new_summary {
        changes.push(CalendarChange {
            kind: CalendarChangeKind::Renamed,
            calendar_id,
            event_id,
            summary: new_summary.to_owned(),
            start: Some(new_dtstart.to_owned()),
            previous_start: None,
            previous_summary: Some(previous.summary.clone()),
            location: new_location.map(str::to_owned),
            organizer: new_organizer.map(str::to_owned),
            attendee_count: new_attendees.len(),
        });
    }
    if previous.location.as_deref() != new_location {
        changes.push(CalendarChange {
            kind: CalendarChangeKind::LocationChanged,
            calendar_id,
            event_id,
            summary: new_summary.to_owned(),
            start: Some(new_dtstart.to_owned()),
            previous_start: None,
            previous_summary: None,
            location: new_location.map(str::to_owned),
            organizer: new_organizer.map(str::to_owned),
            attendee_count: new_attendees.len(),
        });
    }
    if let Some(previous_attendees) = previous_attendees
        && previous_attendees != new_attendees
    {
        changes.push(CalendarChange {
            kind: CalendarChangeKind::AttendeesChanged,
            calendar_id,
            event_id,
            summary: new_summary.to_owned(),
            start: Some(new_dtstart.to_owned()),
            previous_start: None,
            previous_summary: None,
            location: new_location.map(str::to_owned),
            organizer: new_organizer.map(str::to_owned),
            attendee_count: new_attendees.len(),
        });
    }
}

/// Строка локатора письма: account_id, remote_path папки, uid, remote_id, raw_blob_ref.
type MessageLocatorRow = (i64, String, i64, Option<String>, i64);

/// Строка метаданных письма для уведомления: id, from_name, from_addr, subject, preview.
type NotificationPreviewRow = (i64, Option<String>, Option<String>, String, Option<String>);

/// Строка удалённой на сервере встречи: id, summary, dtstart, location,
/// organizer, число участников. Читается до DELETE - после него участников
/// уже не сосчитать.
type DeletedEventRow = (i64, String, String, Option<String>, Option<String>, i64);

/// Что вернул upsert встречи (RETURNING): id, summary, dtstart, dtend, status,
/// location, organizer. Отсюда берётся и дельта изменений для уведомлений.
type SavedEventRow = (
    i64,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);
type FolderCursorRow = (
    String,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<String>,
);
type MessageContentCacheRow = (
    Option<String>,
    Option<String>,
    String,
    i64,
    i64,
    Option<String>,
);

#[derive(Debug, Clone)]
pub struct OutboxOperation {
    pub id: i64,
    pub account_id: i64,
    pub message_id: Option<i64>,
    pub op_kind: String,
    pub payload: String,
    pub attempts: i64,
}

impl Db {
    pub async fn list_keybindings(&self) -> Result<Vec<Keybinding>> {
        Ok(sqlx::query_as::<_, (String, String, String)>(
            "SELECT action, scope, combo FROM keybindings ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|(action, scope, combo)| Keybinding {
            action,
            scope,
            combo,
        })
        .collect())
    }

    pub async fn set_keybinding(&self, action: &str, combo: &str) -> Result<()> {
        let result = sqlx::query("UPDATE keybindings SET combo=? WHERE action=?")
            .bind(combo)
            .bind(action)
            .execute(&self.write_pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(crate::Error::Other(
                "неизвестное действие клавиатуры".into(),
            ));
        }
        Ok(())
    }

    pub async fn image_sender_trusted(&self, sender: &str) -> Result<bool> {
        let row: Option<(i64,)> =
            sqlx::query_as("SELECT allow FROM image_trust WHERE sender=lower(?)")
                .bind(sender.trim())
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.is_some_and(|(allow,)| allow != 0))
    }

    pub async fn set_image_sender_trusted(&self, sender: &str, allow: bool) -> Result<()> {
        let sender = sender.trim().to_lowercase();
        if sender.is_empty() {
            return Err(crate::Error::Other("отправитель не указан".into()));
        }
        sqlx::query(
            "INSERT INTO image_trust(sender, allow) VALUES(?, ?)
             ON CONFLICT(sender) DO UPDATE SET allow=excluded.allow",
        )
        .bind(sender)
        .bind(allow as i64)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// Remove files no longer reachable from SQLite and report broken links.
    /// Run before background synchronization starts, so the reference snapshot
    /// cannot race a writer.
    pub async fn garbage_collect_blobs(&self) -> Result<(usize, Vec<String>)> {
        let mut referenced = std::collections::HashSet::new();
        for query in [
            "SELECT raw_blob_ref FROM messages WHERE raw_blob_ref IS NOT NULL",
            "SELECT blob_ref FROM attachments WHERE blob_ref IS NOT NULL",
            "SELECT vcard_ref FROM contacts WHERE vcard_ref IS NOT NULL",
            "SELECT ical_ref FROM events WHERE ical_ref IS NOT NULL",
        ] {
            let rows: Vec<(String,)> = sqlx::query_as(query).fetch_all(&self.pool).await?;
            referenced.extend(rows.into_iter().map(|row| row.0));
        }
        let missing = referenced
            .iter()
            .filter(|reference| !self.blobs.exists(reference))
            .cloned()
            .collect::<Vec<_>>();
        let mut removed = 0;
        for reference in self.blobs.references()? {
            if !referenced.contains(&reference) {
                self.blobs.remove(&reference)?;
                removed += 1;
            }
        }
        Ok((removed, missing))
    }

    // ---------- Аккаунты ----------

    pub async fn save_account(&self, input: &NewAccount) -> Result<Account> {
        let uuid = uuid::Uuid::new_v4().to_string();
        let provider = match input.provider {
            Provider::Yandex => "yandex",
            Provider::Mailru => "mailru",
            Provider::Icloud => "icloud",
            Provider::Exchange => "exchange",
            Provider::Gmail => "gmail",
            Provider::Outlook => "outlook",
            Provider::Generic => "generic",
        };
        let backend = match input.backend_kind {
            BackendKind::Imap => "imap",
            BackendKind::Ews => "ews",
            BackendKind::Jmap => "jmap",
        };
        let auth = match input.auth_kind {
            AuthKind::Oauth2 => "oauth2",
            AuthKind::AppPassword => "app_password",
            AuthKind::Password => "password",
            AuthKind::Ntlm => "ntlm",
        };
        let security = |value: Option<&ServerConfig>| {
            value.map(|server| match server.security {
                Security::Ssl => "ssl",
                Security::Starttls => "starttls",
                Security::None => "none",
            })
        };
        sqlx::query(
            "INSERT INTO accounts(
                uuid, email, display_name, provider, backend_kind, auth_kind,
                imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security,
                ews_url, jmap_url, caldav_url, carddav_url, username, secret_ref, color
             ) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(email) DO UPDATE SET
                display_name = excluded.display_name,
                provider = excluded.provider,
                backend_kind = excluded.backend_kind,
                auth_kind = excluded.auth_kind,
                imap_host = excluded.imap_host,
                imap_port = excluded.imap_port,
                imap_security = excluded.imap_security,
                smtp_host = excluded.smtp_host,
                smtp_port = excluded.smtp_port,
                smtp_security = excluded.smtp_security,
                ews_url = excluded.ews_url,
                jmap_url = excluded.jmap_url,
                username = excluded.username,
                secret_ref = excluded.secret_ref,
                enabled = 1,
                updated_at = datetime('now')",
        )
        .bind(uuid)
        .bind(&input.email)
        .bind(&input.display_name)
        .bind(provider)
        .bind(backend)
        .bind(auth)
        .bind(input.imap.as_ref().map(|server| &server.host))
        .bind(input.imap.as_ref().map(|server| server.port as i64))
        .bind(security(input.imap.as_ref()))
        .bind(input.smtp.as_ref().map(|server| &server.host))
        .bind(input.smtp.as_ref().map(|server| server.port as i64))
        .bind(security(input.smtp.as_ref()))
        .bind(input.ews_url.as_deref())
        .bind(input.jmap_url.as_deref())
        // caldav_url/carddav_url нарочно не входят в ON CONFLICT UPDATE SET
        // (как и color ниже): переподключение того же email не должно
        // затирать уже обнаруженные DAV-адреса значением NULL из NewAccount.
        .bind(input.caldav_url.as_deref())
        .bind(input.carddav_url.as_deref())
        .bind(input.username.as_deref())
        .bind(&input.secret_ref)
        .bind(input.color.as_deref())
        .execute(&self.write_pool)
        .await?;

        self.list_accounts()
            .await?
            .into_iter()
            .find(|account| account.email.eq_ignore_ascii_case(&input.email))
            .ok_or_else(|| crate::Error::Other("аккаунт не найден после сохранения".into()))
    }

    pub async fn list_accounts(&self) -> Result<Vec<Account>> {
        let rows = sqlx::query_as::<_, AccountRow>(
            "SELECT id, uuid, email, display_name, provider, backend_kind, auth_kind,
                    imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security,
                ews_url, jmap_url, caldav_url, carddav_url, username, secret_ref, include_in_unified, color, retention_days, enabled
             FROM accounts WHERE enabled = 1 ORDER BY sort_order, id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn rename_account(&self, account_id: i64, display_name: &str) -> Result<()> {
        let name = display_name.trim();
        if name.is_empty() {
            return Err(crate::Error::Other(
                "имя аккаунта не может быть пустым".into(),
            ));
        }
        let changed = sqlx::query(
            "UPDATE accounts SET display_name=?, updated_at=datetime('now') WHERE id=? AND enabled=1",
        )
        .bind(name)
        .bind(account_id)
        .execute(&self.write_pool)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(crate::Error::Other("аккаунт не найден".into()));
        }
        Ok(())
    }

    /// Все пользовательские метки (флажки): (id, имя, цвет).
    pub async fn list_labels(&self) -> Result<Vec<(i64, String, Option<String>)>> {
        Ok(
            sqlx::query_as("SELECT id, name, color FROM labels ORDER BY name COLLATE NOCASE")
                .fetch_all(&self.pool)
                .await?,
        )
    }

    /// Создать метку, вернуть её id (или id существующей с тем же именем).
    pub async fn create_label(&self, name: &str, color: &str) -> Result<i64> {
        let name = name.trim();
        if name.is_empty() {
            return Err(crate::Error::Other("имя метки не может быть пустым".into()));
        }
        sqlx::query("INSERT OR IGNORE INTO labels(name, color) VALUES(?, ?)")
            .bind(name)
            .bind(color)
            .execute(&self.write_pool)
            .await?;
        let (id,): (i64,) = sqlx::query_as("SELECT id FROM labels WHERE name = ?")
            .bind(name)
            .fetch_one(&self.pool)
            .await?;
        sqlx::query("UPDATE labels SET color = ? WHERE id = ?")
            .bind(color)
            .bind(id)
            .execute(&self.write_pool)
            .await?;
        Ok(id)
    }

    /// Обновить имя и цвет метки.
    pub async fn update_label(&self, id: i64, name: &str, color: &str) -> Result<()> {
        sqlx::query("UPDATE labels SET name = ?, color = ? WHERE id = ?")
            .bind(name.trim())
            .bind(color)
            .bind(id)
            .execute(&self.write_pool)
            .await?;
        Ok(())
    }

    /// Удалить метку (и её связи с письмами каскадно).
    pub async fn delete_label(&self, id: i64) -> Result<()> {
        sqlx::query("DELETE FROM labels WHERE id = ?")
            .bind(id)
            .execute(&self.write_pool)
            .await?;
        Ok(())
    }

    /// Поставить/снять метку на письмо.
    pub async fn toggle_message_label(
        &self,
        message_id: i64,
        label_id: i64,
        on: bool,
    ) -> Result<()> {
        if on {
            sqlx::query("INSERT OR IGNORE INTO message_labels(message_id, label_id) VALUES(?, ?)")
                .bind(message_id)
                .bind(label_id)
                .execute(&self.write_pool)
                .await?;
        } else {
            sqlx::query("DELETE FROM message_labels WHERE message_id = ? AND label_id = ?")
                .bind(message_id)
                .bind(label_id)
                .execute(&self.write_pool)
                .await?;
        }
        Ok(())
    }

    /// id меток, назначенных письму.
    pub async fn message_label_ids(&self, message_id: i64) -> Result<Vec<i64>> {
        let rows: Vec<(i64,)> =
            sqlx::query_as("SELECT label_id FROM message_labels WHERE message_id = ?")
                .bind(message_id)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(|row| row.0).collect())
    }

    /// Задать глубину локального кэша аккаунта в днях (0 - без ограничений).
    pub async fn set_account_retention(&self, account_id: i64, days: i64) -> Result<()> {
        sqlx::query(
            "UPDATE accounts SET retention_days=?, updated_at=datetime('now') WHERE id=? AND enabled=1",
        )
        .bind(days.max(0))
        .bind(account_id)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// Автоочистка кэша: удалить письма аккаунта старше retention_days вместе с
    /// их raw и blob-вложениями. days=0 - без ограничений (ничего не чистим).
    /// Возвращает число удалённых писем.
    pub async fn prune_cached_messages(&self, account_id: i64, days: i64) -> Result<usize> {
        if days <= 0 {
            return Ok(0);
        }
        let cutoff = format!("-{days} days");
        // Чистим только входящие/архив/спам/корзину и папки без роли. Отправленные,
        // черновики и исходящие - пользовательский контент, их не трогаем никогда.
        let old: Vec<(i64, Option<String>)> = sqlx::query_as(
            "SELECT m.id, m.raw_blob_ref FROM messages m \
             JOIN folders f ON f.id = m.folder_id \
             WHERE m.account_id = ? AND m.date IS NOT NULL AND m.date < datetime('now', ?) \
             AND (f.role IS NULL OR f.role NOT IN ('sent','drafts','outbox'))",
        )
        .bind(account_id)
        .bind(&cutoff)
        .fetch_all(&self.pool)
        .await?;
        if old.is_empty() {
            return Ok(0);
        }
        let mut tx = self.begin_write().await?;
        for (id, raw_ref) in &old {
            let atts: Vec<(Option<String>,)> =
                sqlx::query_as("SELECT blob_ref FROM attachments WHERE message_id = ?")
                    .bind(id)
                    .fetch_all(&mut *tx)
                    .await?;
            for (blob,) in atts {
                if let Some(reference) = blob {
                    let _ = self.blobs.remove(&reference);
                }
            }
            if let Some(reference) = raw_ref {
                let _ = self.blobs.remove(reference);
            }
            // Удаляем запись письма (attachments/labels уйдут по ON DELETE CASCADE).
            sqlx::query("DELETE FROM messages WHERE id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(old.len())
    }

    /// Сохранить обнаруженные (или заданные вручную) базовые адреса
    /// CalDAV/CardDAV, чтобы не искать их заново при каждой синхронизации.
    /// None в аргументе означает "не найдено" и пишет NULL - это осознанно:
    /// вызывающая сторона (AccountManager::resolve_dav_bases) передаёт сюда
    /// уже смешанный результат "было задано ИЛИ обнаружено".
    pub async fn set_dav_urls(
        &self,
        account_id: i64,
        caldav_url: Option<&str>,
        carddav_url: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE accounts SET caldav_url=?, carddav_url=?, updated_at=datetime('now') WHERE id=? AND enabled=1",
        )
        .bind(caldav_url)
        .bind(carddav_url)
        .bind(account_id)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// Задать цвет аккаунта (для аватаров писем и сайдбара).
    pub async fn set_account_color(&self, account_id: i64, color: &str) -> Result<()> {
        sqlx::query(
            "UPDATE accounts SET color=?, updated_at=datetime('now') WHERE id=? AND enabled=1",
        )
        .bind(color)
        .bind(account_id)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    // ---------- Папки ----------

    pub async fn list_folders(&self, account_id: i64) -> Result<Vec<Folder>> {
        let rows = sqlx::query_as::<_, FolderRow>(
            "SELECT id, account_id, remote_path, display_name, role, unread_count, total_count
             FROM folders WHERE account_id = ? ORDER BY id",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn folder(&self, folder_id: i64) -> Result<Folder> {
        let row = sqlx::query_as::<_, FolderRow>(
            "SELECT id, account_id, remote_path, display_name, role, unread_count, total_count FROM folders WHERE id=?",
        )
        .bind(folder_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.into())
    }

    pub async fn rename_folder_local(
        &self,
        folder_id: i64,
        remote_path: &str,
        display_name: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE folders SET remote_path=?, display_name=?, last_synced=datetime('now') WHERE id=?")
            .bind(remote_path)
            .bind(display_name)
            .bind(folder_id)
            .execute(&self.write_pool)
            .await?;
        Ok(())
    }

    pub async fn delete_folder_local(&self, folder_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM folders WHERE id=?")
            .bind(folder_id)
            .execute(&self.write_pool)
            .await?;
        Ok(())
    }

    pub async fn set_folder_role(
        &self,
        account_id: i64,
        role: &str,
        folder_id: Option<i64>,
    ) -> Result<()> {
        const ROLES: &[&str] = &["inbox", "sent", "drafts", "archive", "spam", "trash"];
        if !ROLES.contains(&role) {
            return Err(crate::Error::Other("неизвестная роль папки".into()));
        }
        let mut tx = self.begin_write().await?;
        sqlx::query("UPDATE folders SET role=NULL WHERE account_id=? AND role=?")
            .bind(account_id)
            .bind(role)
            .execute(&mut *tx)
            .await?;
        if let Some(folder_id) = folder_id {
            let updated = sqlx::query("UPDATE folders SET role=? WHERE id=? AND account_id=?")
                .bind(role)
                .bind(folder_id)
                .bind(account_id)
                .execute(&mut *tx)
                .await?;
            if updated.rows_affected() != 1 {
                return Err(crate::Error::Other("папка аккаунта не найдена".into()));
            }
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn save_discovered_folders(
        &self,
        account_id: i64,
        folders: &[crate::backend::DiscoveredFolder],
    ) -> Result<()> {
        let mut tx = self.begin_write().await?;
        for folder in folders {
            let role = folder.role.map(FolderRole::as_str);
            sqlx::query(
                "INSERT INTO folders(account_id, remote_path, display_name, role, unread_count,
                                     total_count, uidvalidity, uidnext, highestmodseq)
                 VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(account_id, remote_path) DO UPDATE SET
                    display_name = excluded.display_name, role = coalesce(excluded.role, folders.role),
                    unread_count = excluded.unread_count, total_count = excluded.total_count,
                    uidvalidity = coalesce(excluded.uidvalidity, folders.uidvalidity),
                    uidnext = coalesce(excluded.uidnext, folders.uidnext),
                    last_synced = datetime('now')",
            )
            .bind(account_id)
            .bind(&folder.remote_path)
            .bind(&folder.display_name)
            .bind(role)
            .bind(folder.unread_count)
            .bind(folder.total_count)
            .bind(folder.uidvalidity.map(i64::from))
            .bind(folder.uidnext.map(i64::from))
            .bind(folder.highestmodseq.map(|value| value as i64))
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Remove remote folders absent from a complete provider discovery.
    pub async fn reconcile_discovered_folders(
        &self,
        account_id: i64,
        folders: &[crate::backend::DiscoveredFolder],
    ) -> Result<usize> {
        if folders.is_empty() {
            return Ok(0);
        }
        let active = folders
            .iter()
            .map(|folder| folder.remote_path.as_str())
            .collect::<std::collections::HashSet<_>>();
        let rows: Vec<(i64, String, Option<String>)> = sqlx::query_as(
            "SELECT m.id, f.remote_path, m.raw_blob_ref
             FROM messages m JOIN folders f ON f.id=m.folder_id
             WHERE f.account_id=?",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        let mut blob_refs = Vec::new();
        for (message_id, path, raw_ref) in rows {
            if active.contains(path.as_str()) {
                continue;
            }
            if let Some(reference) = raw_ref {
                blob_refs.push(reference);
            }
            let attachment_refs: Vec<(Option<String>,)> =
                sqlx::query_as("SELECT blob_ref FROM attachments WHERE message_id=?")
                    .bind(message_id)
                    .fetch_all(&self.pool)
                    .await?;
            blob_refs.extend(attachment_refs.into_iter().filter_map(|row| row.0));
        }
        let existing: Vec<(i64, String)> =
            sqlx::query_as("SELECT id, remote_path FROM folders WHERE account_id=?")
                .bind(account_id)
                .fetch_all(&self.pool)
                .await?;
        let stale_ids = existing
            .into_iter()
            .filter_map(|(id, path)| (!active.contains(path.as_str())).then_some(id))
            .collect::<Vec<_>>();
        if stale_ids.is_empty() {
            return Ok(0);
        }
        let mut tx = self.begin_write().await?;
        for id in &stale_ids {
            sqlx::query("DELETE FROM folders WHERE id=? AND account_id=?")
                .bind(id)
                .bind(account_id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        for reference in blob_refs {
            let _ = self.blobs.remove(&reference);
        }
        Ok(stale_ids.len())
    }

    /// Commit opaque provider cursors only after messages and projections were
    /// stored successfully. If an earlier step fails, the same delta is safely
    /// requested again on the next cycle.
    pub async fn save_folder_sync_tokens(
        &self,
        account_id: i64,
        folders: &[crate::backend::DiscoveredFolder],
    ) -> Result<()> {
        let mut tx = self.begin_write().await?;
        for folder in folders {
            let Some(sync_token) = folder.sync_token.as_deref() else {
                continue;
            };
            sqlx::query(
                "UPDATE folders SET sync_token=?, uidvalidity=coalesce(?, uidvalidity),
                    uidnext=coalesce(?, uidnext),
                    highestmodseq=coalesce(?, highestmodseq), last_synced=datetime('now')
                 WHERE account_id=? AND remote_path=?",
            )
            .bind(sync_token)
            .bind(folder.uidvalidity.map(i64::from))
            .bind(folder.uidnext.map(i64::from))
            .bind(
                folder
                    .highestmodseq
                    .and_then(|value| i64::try_from(value).ok()),
            )
            .bind(account_id)
            .bind(&folder.remote_path)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn folder_sync_cursors(
        &self,
        account_id: i64,
    ) -> Result<std::collections::HashMap<String, crate::backend::FolderSyncCursor>> {
        let rows: Vec<FolderCursorRow> = sqlx::query_as(
            "SELECT f.remote_path, f.uidvalidity, min(m.uid), max(m.uid),
                    f.highestmodseq, f.sync_token
             FROM folders f LEFT JOIN messages m ON m.folder_id=f.id
             WHERE f.account_id=?
             GROUP BY f.id, f.remote_path, f.uidvalidity, f.highestmodseq, f.sync_token",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        let mut cursors = rows
            .into_iter()
            .map(
                |(path, uidvalidity, first_uid, last_uid, highestmodseq, sync_token)| {
                    (
                        path,
                        crate::backend::FolderSyncCursor {
                            uidvalidity: uidvalidity.and_then(|value| u32::try_from(value).ok()),
                            first_uid: first_uid.and_then(|value| u32::try_from(value).ok()),
                            last_uid: last_uid.and_then(|value| u32::try_from(value).ok()),
                            known_uids: Vec::new(),
                            highestmodseq: highestmodseq
                                .and_then(|value| u64::try_from(value).ok()),
                            sync_token,
                        },
                    )
                },
            )
            .collect::<std::collections::HashMap<_, _>>();
        let known_uid_rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT f.remote_path, m.uid
             FROM messages m JOIN folders f ON f.id=m.folder_id
             WHERE f.account_id=? ORDER BY f.remote_path, m.uid",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        for (path, uid) in known_uid_rows {
            if let Some(cursor) = cursors.get_mut(&path)
                && let Ok(uid) = u32::try_from(uid)
            {
                cursor.known_uids.push(uid);
            }
        }
        Ok(cursors)
    }

    /// Reconcile provider projections keyed by a stable remote ID. Gmail can
    /// expose one message in several label-backed folders; a history delta may
    /// remove one projection, add another, or delete the message completely.
    ///
    /// For a complete snapshot, IDs absent from `remote_snapshot` are removed.
    /// IDs that were listed but whose body failed to load are retained, so a
    /// transient API failure never destroys an otherwise valid local copy.
    pub async fn reconcile_remote_projections(
        &self,
        account_id: i64,
        messages: &[crate::backend::DiscoveredMessage],
        changed_remote_ids: &[String],
        remote_snapshot: Option<&[String]>,
    ) -> Result<usize> {
        use std::collections::{HashMap, HashSet};

        let mut desired: HashMap<&str, HashSet<&str>> = HashMap::new();
        for message in messages {
            if let Some(remote_id) = message.remote_id.as_deref() {
                desired
                    .entry(remote_id)
                    .or_default()
                    .insert(message.folder_path.as_str());
            }
        }
        let changed: HashSet<&str> = changed_remote_ids.iter().map(String::as_str).collect();
        let snapshot: Option<HashSet<&str>> =
            remote_snapshot.map(|ids| ids.iter().map(String::as_str).collect());
        if changed.is_empty() && snapshot.is_none() {
            return Ok(0);
        }

        let rows: Vec<(i64, String, String, Option<String>)> = sqlx::query_as(
            "SELECT m.id, m.remote_id, f.remote_path, m.raw_blob_ref
             FROM messages m JOIN folders f ON f.id=m.folder_id
             WHERE m.account_id=? AND m.remote_id IS NOT NULL",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        let mut delete_rows = Vec::new();
        for (id, remote_id, folder_path, raw_ref) in rows {
            let should_check = snapshot.is_some() || changed.contains(remote_id.as_str());
            if !should_check {
                continue;
            }
            let absent_from_server = snapshot
                .as_ref()
                .is_some_and(|ids| !ids.contains(remote_id.as_str()));
            let stale_projection = desired
                .get(remote_id.as_str())
                .is_some_and(|paths| !paths.contains(folder_path.as_str()));
            let confirmed_deleted = snapshot.is_none()
                && changed.contains(remote_id.as_str())
                && !desired.contains_key(remote_id.as_str());
            if absent_from_server || stale_projection || confirmed_deleted {
                delete_rows.push((id, raw_ref));
            }
        }

        let mut tx = self.begin_write().await?;
        let mut blob_refs = Vec::new();
        for (id, raw_ref) in &delete_rows {
            let attachment_refs: Vec<(Option<String>,)> =
                sqlx::query_as("SELECT blob_ref FROM attachments WHERE message_id=?")
                    .bind(id)
                    .fetch_all(&mut *tx)
                    .await?;
            blob_refs.extend(attachment_refs.into_iter().filter_map(|row| row.0));
            if let Some(reference) = raw_ref {
                blob_refs.push(reference.clone());
            }
            sqlx::query("DELETE FROM messages WHERE id=?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        for reference in blob_refs {
            let _ = self.blobs.remove(&reference);
        }
        Ok(delete_rows.len())
    }

    /// Удалить локальные письма, которых больше нет на сервере, и полностью
    /// сбросить mailbox при смене UIDVALIDITY. Blobs удаляются после COMMIT.
    pub async fn reconcile_imap_snapshot(
        &self,
        account_id: i64,
        snapshots: &[(String, Vec<u32>)],
        reset_folders: &[String],
    ) -> Result<usize> {
        use std::collections::HashSet;
        let reset: HashSet<&str> = reset_folders.iter().map(String::as_str).collect();
        let mut tx = self.begin_write().await?;
        let mut delete_ids = Vec::new();
        let mut blob_refs = Vec::new();
        for (path, server_uids) in snapshots {
            let rows: Vec<(i64, i64, Option<String>)> = sqlx::query_as(
                "SELECT m.id, m.uid, m.raw_blob_ref FROM messages m
                 JOIN folders f ON f.id=m.folder_id
                 WHERE m.account_id=? AND f.remote_path=?",
            )
            .bind(account_id)
            .bind(path)
            .fetch_all(&mut *tx)
            .await?;
            let server: HashSet<i64> = server_uids.iter().map(|uid| i64::from(*uid)).collect();
            for (id, uid, reference) in rows {
                if reset.contains(path.as_str()) || !server.contains(&uid) {
                    delete_ids.push(id);
                    if let Some(reference) = reference {
                        blob_refs.push(reference);
                    }
                }
            }
        }
        for id in &delete_ids {
            let attachment_refs: Vec<(Option<String>,)> =
                sqlx::query_as("SELECT blob_ref FROM attachments WHERE message_id=?")
                    .bind(id)
                    .fetch_all(&mut *tx)
                    .await?;
            blob_refs.extend(attachment_refs.into_iter().filter_map(|row| row.0));
            sqlx::query("DELETE FROM messages WHERE id=?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        for reference in blob_refs {
            let _ = self.blobs.remove(&reference);
        }
        Ok(delete_ids.len())
    }

    /// Apply CONDSTORE flag deltas without downloading message bodies again.
    pub async fn apply_imap_flag_updates(
        &self,
        account_id: i64,
        updates: &[crate::backend::DiscoveredFlagUpdate],
    ) -> Result<usize> {
        if updates.is_empty() {
            return Ok(0);
        }
        let started = std::time::Instant::now();
        let mut tx = self.begin_write().await?;
        let mut changed = 0usize;
        for update in updates {
            let result = sqlx::query(
                "UPDATE messages SET seen=?, flagged=?, answered=?, draft=?
                 WHERE account_id=? AND uid=? AND folder_id=(
                    SELECT id FROM folders WHERE account_id=? AND remote_path=?
                 )",
            )
            .bind(update.seen)
            .bind(update.flagged)
            .bind(update.answered)
            .bind(update.draft)
            .bind(account_id)
            .bind(i64::from(update.uid))
            .bind(account_id)
            .bind(&update.folder_path)
            .execute(&mut *tx)
            .await?;
            changed += result.rows_affected() as usize;
        }
        tx.commit().await?;
        tracing::info!(
            account_id,
            received = updates.len(),
            changed,
            tx_ms = started.elapsed().as_millis() as u64,
            "IMAP flag delta applied"
        );
        Ok(changed)
    }

    /// Delete exact QRESYNC VANISHED UIDs without enumerating the whole mailbox.
    pub async fn apply_imap_vanished(
        &self,
        account_id: i64,
        vanished: &[(String, Vec<u32>)],
    ) -> Result<usize> {
        if vanished.is_empty() {
            return Ok(0);
        }
        let started = std::time::Instant::now();
        let mut tx = self.begin_write().await?;
        let mut delete_ids = Vec::new();
        let mut blob_refs = Vec::new();
        for (path, uids) in vanished {
            for uid in uids {
                let row: Option<(i64, Option<String>)> = sqlx::query_as(
                    "SELECT m.id, m.raw_blob_ref FROM messages m
                     JOIN folders f ON f.id=m.folder_id
                     WHERE m.account_id=? AND f.remote_path=? AND m.uid=?",
                )
                .bind(account_id)
                .bind(path)
                .bind(i64::from(*uid))
                .fetch_optional(&mut *tx)
                .await?;
                if let Some((id, raw_ref)) = row {
                    let attachment_refs: Vec<(Option<String>,)> =
                        sqlx::query_as("SELECT blob_ref FROM attachments WHERE message_id=?")
                            .bind(id)
                            .fetch_all(&mut *tx)
                            .await?;
                    blob_refs.extend(attachment_refs.into_iter().filter_map(|row| row.0));
                    if let Some(reference) = raw_ref {
                        blob_refs.push(reference);
                    }
                    delete_ids.push(id);
                }
            }
        }
        for id in &delete_ids {
            sqlx::query("DELETE FROM messages WHERE id=?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        for reference in blob_refs {
            let _ = self.blobs.remove(&reference);
        }
        tracing::info!(
            account_id,
            deleted = delete_ids.len(),
            tx_ms = started.elapsed().as_millis() as u64,
            "IMAP QRESYNC tombstones applied"
        );
        Ok(delete_ids.len())
    }

    pub async fn auxiliary_sync_cursors(
        &self,
        account_id: i64,
    ) -> Result<crate::account::AuxiliarySyncCursors> {
        use crate::account::{AuxiliarySyncCursors, CollectionCursor};

        let calendar_rows: Vec<(Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT url, ctag, sync_token FROM calendars WHERE account_id=? AND url IS NOT NULL",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        let event_etag_rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT c.url, e.remote_url, e.etag
             FROM events e JOIN calendars c ON c.id=e.calendar_id
             WHERE c.account_id=? AND c.url IS NOT NULL
               AND e.remote_url IS NOT NULL AND e.etag IS NOT NULL",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        let collection_rows: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT url, ctag, sync_token FROM auxiliary_collections WHERE account_id=?",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        let contacts_sync_token: Option<(String,)> = sqlx::query_as(
            "SELECT sync_token FROM auxiliary_sync_state WHERE account_id=? AND kind='google-contacts'",
        )
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?;
        let contact_etag_rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT remote_url, etag FROM contacts
             WHERE account_id=? AND remote_url IS NOT NULL AND etag IS NOT NULL",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        let mut calendar_etags: std::collections::HashMap<
            String,
            std::collections::HashMap<String, String>,
        > = std::collections::HashMap::new();
        for (collection_url, resource_url, etag) in event_etag_rows {
            calendar_etags
                .entry(collection_url)
                .or_default()
                .insert(resource_url, etag);
        }
        let mut contact_collections = collection_rows
            .into_iter()
            .map(|(url, ctag, sync_token)| {
                (
                    url,
                    CollectionCursor {
                        ctag,
                        sync_token,
                        resource_etags: std::collections::HashMap::new(),
                    },
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        for (resource_url, etag) in contact_etag_rows {
            if let Some((_, cursor)) = contact_collections
                .iter_mut()
                .filter(|(collection_url, _)| resource_url.starts_with(collection_url.as_str()))
                .max_by_key(|(collection_url, _)| collection_url.len())
            {
                cursor.resource_etags.insert(resource_url, etag);
            }
        }
        Ok(AuxiliarySyncCursors {
            calendars: calendar_rows
                .into_iter()
                .filter_map(|(url, ctag, sync_token)| {
                    url.map(|url| {
                        let resource_etags = calendar_etags.remove(&url).unwrap_or_default();
                        (
                            url,
                            CollectionCursor {
                                ctag,
                                sync_token,
                                resource_etags,
                            },
                        )
                    })
                })
                .collect(),
            contact_collections,
            contacts_sync_token: contacts_sync_token.map(|row| row.0),
        })
    }

    /// Сохранить результат CalDAV/CardDAV-синхронизации - для любого DAV-
    /// провайдера (Яндекс и все остальные), не только для Яндекса, как раньше.
    pub async fn save_dav(
        &self,
        account_id: i64,
        data: &crate::account::DavSyncResult,
    ) -> Result<AuxiliarySaveResult> {
        self.save_auxiliary_data(account_id, "caldav", data).await
    }

    pub async fn save_google_services(
        &self,
        account_id: i64,
        data: &crate::account::DavSyncResult,
    ) -> Result<AuxiliarySaveResult> {
        self.save_auxiliary_data(account_id, "google", data).await
    }

    /// Сохранить календарные источники и контакты конкретного провайдера.
    pub async fn save_auxiliary_data(
        &self,
        account_id: i64,
        source_kind: &str,
        data: &crate::account::DavSyncResult,
    ) -> Result<AuxiliarySaveResult> {
        use std::collections::{HashMap, HashSet};

        // Файловая система не участвует в SQLite-транзакции. Поэтому новые
        // blobs учитываем отдельно: при любой ошибке удаляем их, а старые
        // ссылки удаляем только после успешного COMMIT.
        let old_event_refs: Vec<(Option<String>,)> = sqlx::query_as(
            "SELECT e.ical_ref FROM events e JOIN calendars c ON c.id=e.calendar_id
             WHERE c.account_id=? AND e.ical_ref IS NOT NULL",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        let old_contact_refs: Vec<(Option<String>,)> = sqlx::query_as(
            "SELECT vcard_ref FROM contacts WHERE account_id=? AND vcard_ref IS NOT NULL",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;

        let mut created_refs: Vec<String> = Vec::new();
        let mut calendar_rows = Vec::new();
        if data.calendars_available {
            for calendar in &data.calendars {
                let mut events = Vec::new();
                for event in &calendar.events {
                    let reference = self.blobs.put(event.raw.as_bytes())?;
                    created_refs.push(reference.clone());
                    events.push((event, reference));
                }
                calendar_rows.push((calendar, events));
            }
        }
        let mut contact_rows = Vec::new();
        if data.contacts_available {
            for contact in &data.contacts {
                let reference = self.blobs.put(contact.raw.as_bytes())?;
                created_refs.push(reference.clone());
                contact_rows.push((contact, reference));
            }
        }

        let save_result: Result<AuxiliarySaveResult> = async {
            let writer_wait_started = std::time::Instant::now();
            let mut tx = self.begin_write().await?;
            let writer_wait = writer_wait_started.elapsed();
            let tx_started = std::time::Instant::now();
            let existing_calendars: Vec<(i64,)> = if data.calendars_available {
                sqlx::query_as("SELECT id FROM calendars WHERE account_id=? AND kind=?")
                    .bind(account_id)
                    .bind(source_kind)
                    .fetch_all(&mut *tx)
                    .await?
            } else {
                Vec::new()
            };
            let mut active_calendars = HashSet::new();
            let mut event_count = 0;
            let mut changes: Vec<CalendarChange> = Vec::new();
            for (calendar, events) in calendar_rows {
                let (calendar_id,): (i64,) = sqlx::query_as(
                    "INSERT INTO calendars(account_id, uid, name, kind, url, ctag, sync_token)
                     VALUES(?, ?, ?, ?, ?, ?, ?)
                     ON CONFLICT DO UPDATE SET name=excluded.name, url=excluded.url,
                         kind=excluded.kind, ctag=excluded.ctag,
                         sync_token=excluded.sync_token
                     RETURNING id",
                )
                .bind(account_id)
                .bind(&calendar.url)
                .bind(&calendar.name)
                .bind(source_kind)
                .bind(&calendar.url)
                .bind(&calendar.ctag)
                .bind(&calendar.sync_token)
                .fetch_one(&mut *tx)
                .await?;
                active_calendars.insert(calendar_id);

                // Расширенная выборка (не дополнительный запрос - раньше тут
                // читали только id) даёт прежнее состояние каждой встречи:
                // нужно для дельты изменений при upsert ниже. Ключ совпадает
                // с уникальным индексом uq_events_calendar_uid.
                let existing_event_rows: Vec<PreviousEventRow> = sqlx::query_as(
                    "SELECT id, uid, recurrence_id, summary, dtstart, dtend, status, location
                     FROM events WHERE calendar_id=?",
                )
                .bind(calendar_id)
                .fetch_all(&mut *tx)
                .await?;
                let mut previous_by_key: HashMap<(String, String), &PreviousEventRow> =
                    HashMap::new();
                for row in &existing_event_rows {
                    if let Some(uid) = &row.uid {
                        previous_by_key.insert(
                            (uid.clone(), row.recurrence_id.clone().unwrap_or_default()),
                            row,
                        );
                    }
                }
                let track_changes = calendar.sync_scope != crate::account::SyncScope::Full;
                for remote_url in &calendar.deleted_event_urls {
                    let deleted: Option<DeletedEventRow> = sqlx::query_as(
                            "SELECT e.id, e.summary, e.dtstart, e.location, e.organizer,
                                    (SELECT COUNT(*) FROM event_attendees a WHERE a.event_id = e.id)
                             FROM events e
                             WHERE e.calendar_id=? AND e.remote_url=?",
                        )
                    .bind(calendar_id)
                    .bind(remote_url)
                    .fetch_optional(&mut *tx)
                    .await?;
                    sqlx::query("DELETE FROM events WHERE calendar_id=? AND remote_url=?")
                        .bind(calendar_id)
                        .bind(remote_url)
                        .execute(&mut *tx)
                        .await?;
                    // Ресурс реально пропал на сервере (не просто помечен
                    // CANCELLED - см. задачу B, где Google больше так не
                    // делает). С точки зрения пользователя это та же отмена.
                    if track_changes
                        && let Some((
                            event_id,
                            summary,
                            dtstart,
                            location,
                            organizer,
                            attendee_count,
                        )) = deleted
                    {
                        changes.push(CalendarChange {
                            kind: CalendarChangeKind::Cancelled,
                            calendar_id,
                            event_id,
                            summary,
                            start: Some(dtstart),
                            previous_start: None,
                            previous_summary: None,
                            location,
                            organizer,
                            attendee_count: attendee_count.max(0) as usize,
                        });
                    }
                }
                let mut active_events = HashSet::new();
                for (event, blob_ref) in events {
                    // Google в delta-синхронизации присылает отменённые
                    // события огрызками без summary/dtstart (задача B).
                    // COALESCE(NULLIF(...)) сохраняет прежнее значение
                    // колонки, если новое - пустая строка, а не перезаписывает
                    // встречу пустым названием и датой. all_day пересчитан от
                    // того же эффективного (после coalesce) dtstart, иначе
                    // такой огрызок сбросил бы флаг "весь день" на false.
                    let previous = previous_by_key
                        .get(&(
                            event.uid.clone(),
                            event.recurrence_id.clone().unwrap_or_default(),
                        ))
                        .copied();
                    // У огрызка заполнен только идентификатор и статус. Полный upsert
                    // затёр бы место, описание, организатора и категории пустыми, а ниже
                    // снёс бы и всех участников - от отменённой встречи не осталось бы
                    // ничего, кроме названия. Поэтому такому событию меняем только статус.
                    let cancel_snippet = event.dtstart.trim().is_empty();
                    let saved: Option<SavedEventRow> = if cancel_snippet {
                        sqlx::query_as(
                            "UPDATE events SET status=?, etag=COALESCE(?, etag)
                             WHERE calendar_id=? AND uid=?
                                AND COALESCE(recurrence_id,'') = COALESCE(?,'')
                             RETURNING id, summary, dtstart, dtend, status, location, organizer",
                        )
                        .bind(&event.status)
                        .bind(&event.etag)
                        .bind(calendar_id)
                        .bind(&event.uid)
                        .bind(&event.recurrence_id)
                        .fetch_optional(&mut *tx)
                        .await?
                    } else {
                        Some(sqlx::query_as(
                        "INSERT INTO events(calendar_id, uid, summary, description, location,
                                            dtstart, dtend, all_day, rrule, recurrence_id, exdates, rdates,
                                            status, ical_ref, etag, remote_url, timezone, transp, class,
                                            categories, url, organizer, sequence)
                         VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                         ON CONFLICT DO UPDATE SET
                            summary=COALESCE(NULLIF(excluded.summary, ''), summary),
                            description=excluded.description, location=excluded.location,
                            dtstart=COALESCE(NULLIF(excluded.dtstart, ''), dtstart),
                            dtend=excluded.dtend,
                            all_day=CASE
                                WHEN length(COALESCE(NULLIF(excluded.dtstart, ''), dtstart)) = 8
                                    THEN 1
                                WHEN length(COALESCE(NULLIF(excluded.dtstart, ''), dtstart)) = 10
                                    AND instr(COALESCE(NULLIF(excluded.dtstart, ''), dtstart), '-') > 0
                                    THEN 1
                                ELSE 0
                            END,
                            rrule=excluded.rrule, recurrence_id=excluded.recurrence_id,
                            exdates=excluded.exdates, rdates=excluded.rdates,
                            status=excluded.status,
                            ical_ref=excluded.ical_ref, etag=excluded.etag,
                            remote_url=excluded.remote_url, timezone=excluded.timezone,
                            transp=excluded.transp, class=excluded.class,
                            categories=excluded.categories, url=excluded.url,
                            organizer=excluded.organizer, sequence=excluded.sequence
                         RETURNING id, summary, dtstart, dtend, status, location, organizer",
                    )
                    .bind(calendar_id)
                    .bind(&event.uid)
                    .bind(&event.summary)
                    .bind(&event.description)
                    .bind(&event.location)
                    .bind(&event.dtstart)
                    .bind(&event.dtend)
                    .bind(
                        event.dtstart.len() == 8
                            || (event.dtstart.len() == 10 && event.dtstart.contains('-')),
                    )
                    .bind(&event.rrule)
                    .bind(&event.recurrence_id)
                    .bind(&event.exdates)
                    .bind(&event.rdates)
                    .bind(&event.status)
                    .bind(blob_ref)
                    .bind(&event.etag)
                    .bind(&event.remote_url)
                    .bind(&event.timezone)
                    .bind(&event.transp)
                    .bind(&event.class)
                    .bind(event.categories.join(","))
                    .bind(&event.url)
                    .bind(&event.organizer)
                    .bind(event.sequence)
                    .fetch_one(&mut *tx)
                    .await?)
                    };
                    // Огрызок про событие, которого у нас нет: отменять нечего.
                    let Some((
                        event_id,
                        new_summary,
                        new_dtstart,
                        new_dtend,
                        new_status,
                        new_location,
                        new_organizer,
                    )) = saved
                    else {
                        continue;
                    };
                    if track_changes {
                        let previous_attendees: HashSet<String> = sqlx::query_as::<_, (String,)>(
                            "SELECT email FROM event_attendees WHERE event_id=?",
                        )
                        .bind(event_id)
                        .fetch_all(&mut *tx)
                        .await?
                        .into_iter()
                        .map(|(email,)| email.to_ascii_lowercase())
                        .collect();
                        // У огрызка списка участников нет, и мы его не перезаписываем -
                        // значит состав не менялся, иначе выдали бы ложное
                        // "изменился состав участников" на каждой отмене.
                        let new_attendees: HashSet<String> = if cancel_snippet {
                            previous_attendees.clone()
                        } else {
                            event
                                .attendees
                                .iter()
                                .map(|attendee| attendee.email.to_ascii_lowercase())
                                .collect()
                        };
                        classify_event_change(
                            calendar_id,
                            event_id,
                            &new_summary,
                            &new_dtstart,
                            new_dtend.as_deref(),
                            new_status.as_deref(),
                            new_location.as_deref(),
                            new_organizer.as_deref(),
                            &new_attendees,
                            previous,
                            Some(&previous_attendees),
                            &mut changes,
                        );
                    }
                    // Огрызок не несёт ни участников, ни напоминаний - перезапись
                    // стёрла бы уже сохранённые. Обновление статуса выше уже выполнено.
                    if cancel_snippet {
                        active_events.insert(event_id);
                        event_count += 1;
                        continue;
                    }
                    sqlx::query("DELETE FROM event_attendees WHERE event_id=?")
                        .bind(event_id)
                        .execute(&mut *tx)
                        .await?;
                    for attendee in &event.attendees {
                        sqlx::query(
                            "INSERT OR IGNORE INTO event_attendees(
                                event_id, email, name, role, partstat, rsvp
                             ) VALUES(?, ?, ?, ?, ?, ?)",
                        )
                        .bind(event_id)
                        .bind(&attendee.email)
                        .bind(&attendee.name)
                        .bind(&attendee.role)
                        .bind(&attendee.partstat)
                        .bind(attendee.rsvp)
                        .execute(&mut *tx)
                        .await?;
                    }
                    sqlx::query("DELETE FROM event_alarms WHERE event_id=?")
                        .bind(event_id)
                        .execute(&mut *tx)
                        .await?;
                    for alarm in &event.alarms {
                        sqlx::query(
                            "INSERT OR IGNORE INTO event_alarms(
                                event_id, trigger_minutes, action
                             ) VALUES(?, ?, ?)",
                        )
                        .bind(event_id)
                        .bind(alarm.trigger_minutes)
                        .bind(&alarm.action)
                        .execute(&mut *tx)
                        .await?;
                    }
                    active_events.insert(event_id);
                    event_count += 1;
                }
                if calendar.sync_scope == crate::account::SyncScope::Full {
                    for row in &existing_event_rows {
                        if !active_events.contains(&row.id) {
                            sqlx::query("DELETE FROM events WHERE id=?")
                                .bind(row.id)
                                .execute(&mut *tx)
                                .await?;
                        }
                    }
                }
            }
            for (calendar_id,) in existing_calendars {
                if !active_calendars.contains(&calendar_id) {
                    sqlx::query("DELETE FROM calendars WHERE id=?")
                        .bind(calendar_id)
                        .execute(&mut *tx)
                        .await?;
                }
            }

            if data.contacts_available {
                let collection_kind = if source_kind == "caldav" { "carddav" } else { source_kind };
                sqlx::query("DELETE FROM auxiliary_collections WHERE account_id=? AND kind=?")
                    .bind(account_id)
                    .bind(collection_kind)
                    .execute(&mut *tx)
                    .await?;
                for collection in &data.contact_collections {
                    sqlx::query(
                        "INSERT INTO auxiliary_collections(account_id, kind, url, ctag, sync_token)
                         VALUES(?, ?, ?, ?, ?)
                         ON CONFLICT(account_id, kind, url) DO UPDATE SET
                            ctag=excluded.ctag, sync_token=excluded.sync_token",
                    )
                    .bind(account_id)
                    .bind(collection_kind)
                    .bind(&collection.url)
                    .bind(&collection.ctag)
                    .bind(&collection.sync_token)
                    .execute(&mut *tx)
                    .await?;
                }
                let existing_contacts: Vec<(i64,)> = sqlx::query_as(
                    "SELECT id FROM contacts
                     WHERE account_id=? AND uid NOT LIKE 'mail:%' AND uid NOT LIKE 'local:%'",
                )
                .bind(account_id)
                .fetch_all(&mut *tx)
                .await?;
                let mut active_contacts = HashSet::new();
                for remote_url in &data.deleted_contact_urls {
                    sqlx::query("DELETE FROM contacts WHERE account_id=? AND remote_url=?")
                        .bind(account_id)
                        .bind(remote_url)
                        .execute(&mut *tx)
                        .await?;
                }
                for (contact, blob_ref) in contact_rows {
                    let (contact_id,): (i64,) = sqlx::query_as(
                        "INSERT INTO contacts(account_id, uid, display_name, first_name,
                                               last_name, organization, vcard_ref, etag, remote_url)
                         VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?)
                         ON CONFLICT DO UPDATE SET display_name=excluded.display_name,
                            first_name=excluded.first_name, last_name=excluded.last_name,
                            organization=excluded.organization, vcard_ref=excluded.vcard_ref,
                            etag=excluded.etag, remote_url=excluded.remote_url
                         RETURNING id",
                    )
                    .bind(account_id)
                    .bind(&contact.uid)
                    .bind(clean_contact_name(&contact.display_name))
                    .bind(&contact.first_name)
                    .bind(&contact.last_name)
                    .bind(&contact.organization)
                    .bind(blob_ref)
                    .bind(&contact.etag)
                    .bind(&contact.remote_url)
                    .fetch_one(&mut *tx)
                    .await?;
                    active_contacts.insert(contact_id);
                    sqlx::query("DELETE FROM contact_emails WHERE contact_id=?")
                        .bind(contact_id)
                        .execute(&mut *tx)
                        .await?;
                    for email in &contact.emails {
                        sqlx::query(
                            "INSERT OR IGNORE INTO contact_emails(contact_id, email) VALUES(?, ?)",
                        )
                        .bind(contact_id)
                        .bind(email)
                        .execute(&mut *tx)
                        .await?;
                    }
                    sqlx::query("DELETE FROM contact_phones WHERE contact_id=?")
                        .bind(contact_id)
                        .execute(&mut *tx)
                        .await?;
                    for phone in &contact.phones {
                        sqlx::query(
                            "INSERT INTO contact_phones(contact_id, number, kind, extension)
                             VALUES(?, ?, ?, ?)",
                        )
                        .bind(contact_id)
                        .bind(&phone.number)
                        .bind(&phone.kind)
                        .bind(&phone.extension)
                        .execute(&mut *tx)
                        .await?;
                    }
                }
                if data.contacts_scope == crate::account::SyncScope::Full {
                    for (contact_id,) in existing_contacts {
                        if !active_contacts.contains(&contact_id) {
                            sqlx::query("DELETE FROM contacts WHERE id=?")
                                .bind(contact_id)
                                .execute(&mut *tx)
                                .await?;
                        }
                    }
                }
                if let Some(sync_token) = &data.contacts_sync_token {
                    sqlx::query(
                        "INSERT INTO auxiliary_sync_state(account_id, kind, sync_token)
                         VALUES(?, 'google-contacts', ?)
                         ON CONFLICT(account_id, kind) DO UPDATE SET
                            sync_token=excluded.sync_token, updated_at=datetime('now')",
                    )
                    .bind(account_id)
                    .bind(sync_token)
                    .execute(&mut *tx)
                    .await?;
                }
            }

            tx.commit().await?;
            tracing::info!(
                account_id,
                source_kind,
                calendars = data.calendars.len(),
                events = event_count,
                contacts = data.contacts.len(),
                changes = changes.len(),
                writer_wait_ms = writer_wait.as_millis() as u64,
                tx_ms = tx_started.elapsed().as_millis() as u64,
                "auxiliary delta applied"
            );
            Ok(AuxiliarySaveResult {
                calendars: data.calendars.len(),
                events: event_count,
                contacts: data.contacts.len(),
                changes,
            })
        }
        .await;

        let counts = match save_result {
            Ok(counts) => counts,
            Err(error) => {
                for reference in &created_refs {
                    let _ = self.blobs.remove(reference);
                }
                return Err(error);
            }
        };

        let current_event_refs: HashSet<String> = sqlx::query_as::<_, (Option<String>,)>(
            "SELECT e.ical_ref FROM events e JOIN calendars c ON c.id=e.calendar_id
             WHERE c.account_id=? AND e.ical_ref IS NOT NULL",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .filter_map(|row| row.0)
        .collect();
        let current_contact_refs: HashSet<String> = sqlx::query_as::<_, (Option<String>,)>(
            "SELECT vcard_ref FROM contacts WHERE account_id=? AND vcard_ref IS NOT NULL",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .filter_map(|row| row.0)
        .collect();
        for (reference,) in old_event_refs {
            if let Some(reference) = reference
                && !current_event_refs.contains(&reference)
            {
                let _ = self.blobs.remove(&reference);
            }
        }
        for (reference,) in old_contact_refs {
            if let Some(reference) = reference
                && !current_contact_refs.contains(&reference)
            {
                let _ = self.blobs.remove(&reference);
            }
        }
        self.sync_contacts_from_messages(account_id).await?;
        Ok(counts)
    }

    pub async fn save_discovered_messages(
        &self,
        account_id: i64,
        messages: &[crate::backend::DiscoveredMessage],
    ) -> Result<()> {
        use mail_parser::{MessageParser, MimeHeaders};
        use std::collections::HashSet;
        let mut rows = Vec::new();
        let mut created_refs: Vec<String> = Vec::new();
        for source in messages {
            let Some(message) = MessageParser::default().parse(&source.raw) else {
                continue;
            };
            let from = message.from().and_then(|value| value.first());
            let addresses = |value: Option<&mail_parser::Address<'_>>| {
                value
                    .map(|value| {
                        value
                            .iter()
                            .map(|addr| Addr {
                                name: addr.name.as_deref().map(str::to_owned),
                                email: addr.address.as_deref().unwrap_or_default().to_owned(),
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            };
            let preview: String = message
                .body_text(0)
                .map(|body| body.chars().take(240).collect())
                .unwrap_or_default();
            let body_text = message
                .body_text(0)
                .map(|body| body.into_owned())
                .unwrap_or_default();
            let auth_header = message
                .header("Authentication-Results")
                .and_then(|value| value.as_text())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let auth = |name: &str| {
                if auth_header.contains(&format!("{name}=pass")) {
                    Some(1_i64)
                } else if auth_header.contains(&format!("{name}=fail")) {
                    Some(0_i64)
                } else {
                    None
                }
            };
            let attachment_rows = message
                .attachments()
                .enumerate()
                .map(|(index, part)| {
                    let mime_type = part.content_type().map(|content_type| {
                        format!(
                            "{}/{}",
                            content_type.c_type,
                            content_type.c_subtype.as_deref().unwrap_or("octet-stream")
                        )
                    });
                    let size = match &part.body {
                        mail_parser::PartType::Binary(bytes)
                        | mail_parser::PartType::InlineBinary(bytes) => Some(bytes.len() as i64),
                        mail_parser::PartType::Text(text) | mail_parser::PartType::Html(text) => {
                            Some(text.len() as i64)
                        }
                        _ => None,
                    };
                    (
                        part.attachment_name()
                            .map(str::to_owned)
                            .unwrap_or_else(|| format!("attachment-{}", index + 1)),
                        mime_type,
                        size,
                        part.content_id().map(|value| {
                            value
                                .trim()
                                .trim_start_matches('<')
                                .trim_end_matches('>')
                                .to_owned()
                        }),
                    )
                })
                .collect::<Vec<_>>();
            let to_json = match serde_json::to_string(&addresses(message.to())) {
                Ok(value) => value,
                Err(error) => {
                    for reference in &created_refs {
                        let _ = self.blobs.remove(reference);
                    }
                    return Err(error.into());
                }
            };
            let cc_json = match serde_json::to_string(&addresses(message.cc())) {
                Ok(value) => value,
                Err(error) => {
                    for reference in &created_refs {
                        let _ = self.blobs.remove(reference);
                    }
                    return Err(error.into());
                }
            };
            let raw_ref = match self.blobs.put(&source.raw) {
                Ok(reference) => reference,
                Err(error) => {
                    for reference in &created_refs {
                        let _ = self.blobs.remove(reference);
                    }
                    return Err(error);
                }
            };
            created_refs.push(raw_ref.clone());
            rows.push((
                source,
                message.message_id().map(str::to_owned),
                message
                    .header("In-Reply-To")
                    .and_then(|value| value.as_text())
                    .map(str::to_owned),
                message
                    .header("References")
                    .and_then(|value| value.as_text())
                    .map(str::to_owned),
                from.and_then(|a| a.name.as_deref()).map(str::to_owned),
                from.and_then(|a| a.address.as_deref()).map(str::to_owned),
                to_json,
                cc_json,
                message.subject().unwrap_or_default().to_owned(),
                preview,
                body_text,
                message.date().map(|date| date.to_rfc3339()),
                attachment_rows,
                auth("dkim"),
                auth("spf"),
                auth("dmarc"),
                raw_ref,
            ));
        }
        let mut active_refs = HashSet::new();
        let mut stale_refs = Vec::new();
        let mut indexed_bodies = Vec::new();
        let save_result: Result<()> = async {
            let mut remaining = rows.into_iter();
            loop {
                let batch = remaining.by_ref().take(100).collect::<Vec<_>>();
                if batch.is_empty() {
                    break;
                }
                let batch_size = batch.len();
                let writer_wait_started = std::time::Instant::now();
                let mut tx = self.begin_write().await?;
                let writer_wait = writer_wait_started.elapsed();
                let tx_started = std::time::Instant::now();
                for (
                source,
                message_id,
                in_reply_to,
                references,
                from_name,
                from_addr,
                to,
                cc,
                subject,
                preview,
                body_text,
                date,
                attachments,
                dkim,
                spf,
                dmarc,
                raw_ref,
                ) in batch
                {
                let folder: Option<(i64,)> = sqlx::query_as(
                    "SELECT id FROM folders WHERE account_id = ? AND remote_path = ? LIMIT 1",
                )
                .bind(account_id)
                .bind(&source.folder_path)
                .fetch_optional(&mut *tx)
                .await?;
                let Some((folder_id,)) = folder else { continue };
                let old_ref: Option<(Option<String>, i64)> = sqlx::query_as(
                    "SELECT raw_blob_ref, body_fetched FROM messages WHERE folder_id=? AND uid=?",
                )
                .bind(folder_id)
                .bind(source.uid as i64)
                .fetch_optional(&mut *tx)
                .await?;
                let is_new_message = old_ref.is_none();
                let preserve_full_body = !source.body_fetched
                    && old_ref
                        .as_ref()
                        .is_some_and(|(_, body_fetched)| *body_fetched != 0);
                let effective_ref = if preserve_full_body {
                    old_ref
                        .as_ref()
                        .and_then(|(reference, _)| reference.clone())
                        .unwrap_or_else(|| raw_ref.clone())
                } else {
                    raw_ref.clone()
                };
                sqlx::query(
                    "INSERT INTO messages(account_id, folder_id, uid, remote_id, rfc822_message_id, in_reply_to, references_ids, from_name, from_addr, to_addrs, cc_addrs, subject, preview, date, size, seen, flagged, answered, draft, has_attachments, dkim_pass, spf_pass, dmarc_pass, raw_blob_ref, body_fetched)
                     VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                     ON CONFLICT(folder_id, uid) DO UPDATE SET
                        remote_id=coalesce(excluded.remote_id,messages.remote_id), rfc822_message_id=excluded.rfc822_message_id, in_reply_to=excluded.in_reply_to,
                        references_ids=excluded.references_ids, from_name=excluded.from_name,
                        from_addr=excluded.from_addr, to_addrs=excluded.to_addrs, cc_addrs=excluded.cc_addrs,
                        subject=excluded.subject, preview=excluded.preview, date=excluded.date, size=excluded.size,
                        seen=excluded.seen, flagged=excluded.flagged, answered=excluded.answered,
                        draft=excluded.draft,
                        has_attachments=CASE WHEN messages.body_fetched=1 AND excluded.body_fetched=0 THEN messages.has_attachments ELSE excluded.has_attachments END,
                        dkim_pass=excluded.dkim_pass, spf_pass=excluded.spf_pass,
                        dmarc_pass=excluded.dmarc_pass, raw_blob_ref=excluded.raw_blob_ref,
                        body_fetched=CASE WHEN messages.body_fetched=1 THEN 1 ELSE excluded.body_fetched END",
                )
                .bind(account_id).bind(folder_id).bind(source.uid as i64).bind(&source.remote_id).bind(&message_id)
                .bind(&in_reply_to).bind(&references).bind(from_name).bind(from_addr).bind(to).bind(cc)
                .bind(&subject).bind(&preview).bind(&date).bind(source.size.map(i64::from))
                .bind(source.seen as i64).bind(source.flagged as i64).bind(source.answered as i64)
                .bind(source.draft as i64).bind(!attachments.is_empty() as i64).bind(dkim).bind(spf)
                .bind(dmarc).bind(&effective_ref).bind(source.body_fetched as i64).execute(&mut *tx).await?;
                active_refs.insert(effective_ref.clone());
                if preserve_full_body {
                    if raw_ref != effective_ref {
                        stale_refs.push(raw_ref);
                    }
                } else if let Some((Some(reference), _)) = old_ref
                    && reference != effective_ref
                {
                    stale_refs.push(reference);
                }
                let (message_row_id,): (i64,) =
                    sqlx::query_as("SELECT id FROM messages WHERE folder_id = ? AND uid = ?")
                        .bind(folder_id)
                        .bind(source.uid as i64)
                        .fetch_one(&mut *tx)
                        .await?;
                if !preserve_full_body {
                    sqlx::query("DELETE FROM attachments WHERE message_id=?")
                        .bind(message_row_id)
                        .execute(&mut *tx)
                        .await?;
                    for (filename, mime_type, size, content_id) in attachments {
                        sqlx::query("INSERT INTO attachments(message_id, filename, mime_type, size, content_id, is_inline, fetched) VALUES(?, ?, ?, ?, ?, ?, 0)")
                            .bind(message_row_id).bind(filename).bind(mime_type).bind(size)
                            .bind(&content_id).bind(content_id.is_some() as i64).execute(&mut *tx).await?;
                    }
                }
                if let Some(parent_id) = in_reply_to.as_deref() {
                    let parent: Option<(i64, Option<i64>, Option<String>)> = sqlx::query_as(
                        "SELECT id, thread_id, rfc822_message_id FROM messages WHERE account_id=? AND rfc822_message_id=? LIMIT 1",
                    )
                    .bind(account_id).bind(parent_id).fetch_optional(&mut *tx).await?;
                    if let Some((parent_row_id, parent_thread, root_id)) = parent {
                        let thread_id = if let Some(thread_id) = parent_thread { thread_id } else {
                            let (thread_id,): (i64,) = sqlx::query_as(
                                "INSERT INTO threads(account_id, root_message_id, subject_norm, last_date, message_count) VALUES(?, ?, lower(?), ?, 1) ON CONFLICT DO UPDATE SET last_date=excluded.last_date RETURNING id",
                            ).bind(account_id).bind(root_id.or_else(||message_id.clone())).bind(&subject).bind(&date).fetch_one(&mut *tx).await?;
                            sqlx::query("UPDATE messages SET thread_id=? WHERE id=?").bind(thread_id).bind(parent_row_id).execute(&mut *tx).await?;
                            thread_id
                        };
                        sqlx::query("UPDATE messages SET thread_id=? WHERE id=?").bind(thread_id).bind(message_row_id).execute(&mut *tx).await?;
                        if is_new_message {
                            sqlx::query("UPDATE threads SET last_date=?, message_count=message_count+1 WHERE id=?").bind(&date).bind(thread_id).execute(&mut *tx).await?;
                        }
                    }
                }
                if !preserve_full_body {
                    indexed_bodies.push((message_row_id, body_text));
                }
                }
                tx.commit().await?;
                tracing::info!(
                    account_id,
                    rows = batch_size,
                    writer_wait_ms = writer_wait.as_millis() as u64,
                    tx_ms = tx_started.elapsed().as_millis() as u64,
                    "mail delta batch applied"
                );
                tokio::task::yield_now().await;
            }
            Ok(())
        }
        .await;
        if let Err(error) = save_result {
            for reference in &created_refs {
                let _ = self.blobs.remove(reference);
            }
            return Err(error);
        }
        for reference in created_refs
            .iter()
            .filter(|reference| !active_refs.contains(*reference))
        {
            let _ = self.blobs.remove(reference);
        }
        for reference in stale_refs {
            let _ = self.blobs.remove(&reference);
        }
        // Импорт не знает, какая реализация поиска выбрана приложением. Даже
        // SQLite FTS обновляется через единый контракт SearchIndex, поэтому
        // переход на другой индекс не потребует искать прямые SQL-записи.
        use crate::search::{Fts5Index, SearchIndex};
        let index = Fts5Index::new(self.clone());
        for (message_id, body_text) in indexed_bodies {
            index.index_body(message_id, &body_text).await?;
        }
        self.sync_contacts_from_messages(account_id).await?;
        Ok(())
    }

    /// Дополнить адресную книгу реальными участниками переписки. Это особенно
    /// важно для личного Яндекс-аккаунта: его CardDAV содержит только явно
    /// синхронизируемую книгу и часто пуст, а адреса писем уже доступны локально.
    pub async fn sync_contacts_from_messages(&self, account_id: i64) -> Result<usize> {
        use std::collections::{HashMap, HashSet};

        let (own_email,): (String,) = sqlx::query_as("SELECT email FROM accounts WHERE id = ?")
            .bind(account_id)
            .fetch_one(&self.pool)
            .await?;
        let rows: Vec<(Option<String>, Option<String>, String, String)> = sqlx::query_as(
            "SELECT from_name, from_addr, coalesce(to_addrs, '[]'), coalesce(cc_addrs, '[]')
             FROM messages WHERE account_id = ?",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        let mut candidates = HashMap::<String, String>::new();
        let mut add = |email: &str, name: Option<&str>| {
            let normalized = email.trim().to_lowercase();
            if normalized.is_empty()
                || normalized == own_email.to_lowercase()
                || !normalized.contains('@')
            {
                return;
            }
            let display = clean_contact_name(
                name.map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(&normalized),
            );
            candidates
                .entry(normalized)
                .and_modify(|current| {
                    if current.contains('@') && !display.contains('@') {
                        *current = display.clone();
                    }
                })
                .or_insert(display);
        };
        for (from_name, from_addr, to_json, cc_json) in rows {
            if let Some(email) = from_addr.as_deref() {
                add(email, from_name.as_deref());
            }
            for json in [to_json, cc_json] {
                for address in serde_json::from_str::<Vec<Addr>>(&json).unwrap_or_default() {
                    add(&address.email, address.name.as_deref());
                }
            }
        }
        let existing: HashSet<String> = sqlx::query_as::<_, (String,)>(
            "SELECT lower(ce.email) FROM contact_emails ce
             JOIN contacts c ON c.id = ce.contact_id WHERE c.account_id = ?",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| row.0)
        .collect();
        let mut tx = self.begin_write().await?;
        let mut inserted = 0;
        for (email, display_name) in candidates {
            if existing.contains(&email) {
                continue;
            }
            let result = sqlx::query(
                "INSERT INTO contacts(account_id, uid, display_name)
                 VALUES(?, ?, ?)",
            )
            .bind(account_id)
            .bind(format!("mail:{email}"))
            .bind(display_name)
            .execute(&mut *tx)
            .await?;
            sqlx::query("INSERT INTO contact_emails(contact_id, email, kind) VALUES(?, ?, 'mail')")
                .bind(result.last_insert_rowid())
                .bind(email)
                .execute(&mut *tx)
                .await?;
            inserted += 1;
        }
        tx.commit().await?;
        Ok(inserted)
    }

    // ---------- Письма ----------

    /// Список писем папки (метаданные), новые сверху.
    pub async fn list_messages(&self, folder_id: i64, limit: i64) -> Result<Vec<MessageMeta>> {
        let rows = sqlx::query_as::<_, MessageRow>(
            "SELECT id, account_id, folder_id, thread_id, uid, rfc822_message_id,
                    from_name, from_addr, to_addrs, cc_addrs, subject, preview, date, size,
                    seen, flagged, answered, draft, has_attachments, dkim_pass, spf_pass, dmarc_pass
             FROM messages
             WHERE folder_id = ?
               AND (snoozed_until IS NULL OR snoozed_until <= datetime('now'))
               AND NOT EXISTS (
                 SELECT 1 FROM outbox_ops o
                  WHERE o.message_id=messages.id AND o.op_kind IN ('move','delete')
                    AND o.status IN ('pending','processing','retry')
               )
             ORDER BY date DESC LIMIT ?",
        )
        .bind(folder_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Cursor page ordered by `(date DESC, id DESC)`. Unlike OFFSET this stays
    /// stable while IMAP inserts new mail at the top of the folder.
    pub async fn list_messages_page(
        &self,
        folder_id: i64,
        before_date: Option<&str>,
        before_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<MessageMeta>> {
        let limit = limit.clamp(1, 500);
        if before_date.is_none() || before_id.is_none() {
            return self.list_messages(folder_id, limit).await;
        }
        let date = before_date.unwrap_or_default();
        let id = before_id.unwrap_or(i64::MAX);
        let rows = sqlx::query_as::<_, MessageRow>(
            "SELECT id, account_id, folder_id, thread_id, uid, rfc822_message_id,
                    from_name, from_addr, to_addrs, cc_addrs, subject, preview, date, size,
                    seen, flagged, answered, draft, has_attachments, dkim_pass, spf_pass, dmarc_pass
             FROM messages
             WHERE folder_id = ?
               AND (COALESCE(date, '') < ? OR (COALESCE(date, '') = ? AND id < ?))
               AND (snoozed_until IS NULL OR snoozed_until <= datetime('now'))
               AND NOT EXISTS (
                 SELECT 1 FROM outbox_ops o
                  WHERE o.message_id=messages.id AND o.op_kind IN ('move','delete')
                    AND o.status IN ('pending','processing','retry')
               )
             ORDER BY date DESC, id DESC LIMIT ?",
        )
        .bind(folder_id)
        .bind(date)
        .bind(date)
        .bind(id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn list_recent_messages(&self, limit: i64) -> Result<Vec<MessageMeta>> {
        let ids: Vec<(i64,)> = sqlx::query_as(
            "SELECT id FROM messages
             WHERE (snoozed_until IS NULL OR snoozed_until <= datetime('now'))
               AND NOT EXISTS (
               SELECT 1 FROM outbox_ops o WHERE o.message_id=messages.id
                 AND o.op_kind IN ('move','delete') AND o.status IN ('pending','processing','retry')
             )
             ORDER BY date DESC, id DESC LIMIT ?",
        )
        .bind(limit.clamp(1, 200))
        .fetch_all(&self.pool)
        .await?;
        self.list_messages_by_ids(&ids.into_iter().map(|row| row.0).collect::<Vec<_>>())
            .await
    }

    pub async fn set_messages_snoozed(&self, ids: &[i64], until: Option<&str>) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }
        let mut query =
            sqlx::QueryBuilder::<sqlx::Sqlite>::new("UPDATE messages SET snoozed_until = ");
        query.push_bind(until);
        query.push(" WHERE id IN (");
        let mut separated = query.separated(",");
        for id in ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");
        Ok(query
            .build()
            .execute(&self.write_pool)
            .await?
            .rows_affected() as usize)
    }

    pub async fn release_due_snoozes(&self) -> Result<usize> {
        Ok(sqlx::query(
            "UPDATE messages SET snoozed_until = NULL
             WHERE snoozed_until IS NOT NULL AND snoozed_until <= datetime('now')",
        )
        .execute(&self.write_pool)
        .await?
        .rows_affected() as usize)
    }

    pub async fn list_signatures(&self, account_id: i64) -> Result<Vec<Signature>> {
        let rows: Vec<(String, String, bool)> = sqlx::query_as(
            "SELECT kind, body_html, enabled FROM signatures
             WHERE account_id = ? ORDER BY CASE kind WHEN 'new' THEN 0 ELSE 1 END",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(kind, body_html, enabled)| Signature {
                kind,
                body_html,
                enabled,
            })
            .collect())
    }

    pub async fn upsert_signature(
        &self,
        account_id: i64,
        kind: &str,
        body_html: &str,
        enabled: bool,
    ) -> Result<()> {
        if !matches!(kind, "new" | "reply") {
            return Err(crate::Error::AccountConfig(
                "вид подписи должен быть new или reply".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO signatures(account_id, kind, body_html, enabled)
             VALUES(?, ?, ?, ?)
             ON CONFLICT(account_id, kind) DO UPDATE SET
               body_html=excluded.body_html, enabled=excluded.enabled",
        )
        .bind(account_id)
        .bind(kind)
        .bind(body_html)
        .bind(enabled)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    pub async fn list_message_templates(&self, account_id: i64) -> Result<Vec<MessageTemplate>> {
        let rows: Vec<(i64, i64, String, String, String)> = sqlx::query_as(
            "SELECT id, account_id, name, subject, body_html FROM message_templates
             WHERE account_id = ? ORDER BY name COLLATE NOCASE, id",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, account_id, name, subject, body_html)| MessageTemplate {
                    id,
                    account_id,
                    name,
                    subject,
                    body_html,
                },
            )
            .collect())
    }

    pub async fn save_message_template(
        &self,
        id: Option<i64>,
        account_id: i64,
        name: &str,
        subject: &str,
        body_html: &str,
    ) -> Result<i64> {
        let name = name.trim();
        if name.is_empty() {
            return Err(crate::Error::AccountConfig(
                "название шаблона не указано".into(),
            ));
        }
        if let Some(id) = id {
            let result = sqlx::query(
                "UPDATE message_templates SET name=?, subject=?, body_html=?, updated_at=datetime('now')
                 WHERE id=? AND account_id=?",
            )
            .bind(name)
            .bind(subject)
            .bind(body_html)
            .bind(id)
            .bind(account_id)
            .execute(&self.write_pool)
            .await?;
            if result.rows_affected() == 0 {
                return Err(crate::Error::Other("шаблон не найден".into()));
            }
            return Ok(id);
        }
        Ok(sqlx::query(
            "INSERT INTO message_templates(account_id, name, subject, body_html)
             VALUES(?, ?, ?, ?)",
        )
        .bind(account_id)
        .bind(name)
        .bind(subject)
        .bind(body_html)
        .execute(&self.write_pool)
        .await?
        .last_insert_rowid())
    }

    pub async fn delete_message_template(&self, id: i64, account_id: i64) -> Result<bool> {
        Ok(
            sqlx::query("DELETE FROM message_templates WHERE id=? AND account_id=?")
                .bind(id)
                .bind(account_id)
                .execute(&self.write_pool)
                .await?
                .rows_affected()
                > 0,
        )
    }

    pub async fn list_messages_by_ids(&self, ids: &[i64]) -> Result<Vec<MessageMeta>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut query = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "SELECT id, account_id, folder_id, thread_id, uid, rfc822_message_id,
                    from_name, from_addr, to_addrs, cc_addrs, subject, preview, date, size,
                    seen, flagged, answered, draft, has_attachments, dkim_pass, spf_pass, dmarc_pass
             FROM messages WHERE id IN (",
        );
        let mut separated = query.separated(",");
        for id in ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");
        let rows = query
            .build_query_as::<MessageRow>()
            .fetch_all(&self.pool)
            .await?;
        let mut by_id = rows
            .into_iter()
            .map(|row| (row.id, MessageMeta::from(row)))
            .collect::<std::collections::HashMap<_, _>>();
        Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
    }

    /// Данные для докачки письма с сервера, когда полный MIME ещё не загружен:
    /// (account_id, remote_path папки, uid, remote_id, загружено ли тело).
    pub async fn message_fetch_locator(
        &self,
        message_id: i64,
    ) -> Result<Option<(i64, String, i64, Option<String>, bool)>> {
        let row: Option<MessageLocatorRow> = sqlx::query_as(
            "SELECT m.account_id, f.remote_path, m.uid, m.remote_id, m.body_fetched \
             FROM messages m JOIN folders f ON f.id = m.folder_id WHERE m.id = ?",
        )
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(account_id, path, uid, remote_id, body_fetched)| {
            (account_id, path, uid, remote_id, body_fetched != 0)
        }))
    }

    /// Сохранить докачанный с сервера сырой MIME и пометить тело загруженным.
    /// Свежий blob не удаляется прунингом текущей сессии (prune только на старте).
    pub async fn store_fetched_raw(&self, message_id: i64, raw: &[u8]) -> Result<()> {
        let reference = self.blobs.put(raw)?;
        sqlx::query("UPDATE messages SET raw_blob_ref = ?, body_fetched = 1 WHERE id = ?")
            .bind(&reference)
            .bind(message_id)
            .execute(&self.write_pool)
            .await?;
        Ok(())
    }

    pub async fn get_message(&self, message_id: i64) -> Result<MessageFull> {
        use base64::Engine as _;
        use mail_parser::{MessageParser, MimeHeaders, PartType};
        let meta = self
            .list_messages_by_ids(&[message_id])
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| crate::Error::Other("письмо не найдено".into()))?;
        let (raw_ref,): (Option<String>,) =
            sqlx::query_as("SELECT raw_blob_ref FROM messages WHERE id = ?")
                .bind(message_id)
                .fetch_one(&self.pool)
                .await?;
        if let Some(reference) = raw_ref.as_deref() {
            let cached: Option<MessageContentCacheRow> = sqlx::query_as(
                "SELECT body_html, body_text, attachments_json, has_remote_content,
                        is_newsletter, unsubscribe_json
                   FROM message_content_cache
                  WHERE message_id = ? AND raw_blob_ref = ?",
            )
            .bind(message_id)
            .bind(reference)
            .fetch_optional(&self.pool)
            .await?;
            if let Some((body_html, body_text, attachments, remote, newsletter, unsubscribe)) =
                cached
            {
                return Ok(MessageFull {
                    meta,
                    body_html,
                    body_text,
                    attachments: serde_json::from_str(&attachments)?,
                    has_remote_content: remote != 0,
                    is_newsletter: newsletter != 0,
                    unsubscribe: unsubscribe
                        .as_deref()
                        .map(serde_json::from_str)
                        .transpose()?,
                });
            }
        }
        let raw = raw_ref
            .as_deref()
            .map(|reference| self.blobs.get(reference))
            .transpose()?;
        let parsed = raw
            .as_deref()
            .and_then(|bytes| MessageParser::default().parse(bytes));
        let body_text = parsed
            .as_ref()
            .and_then(|message| message.body_text(0).map(|v| v.into_owned()));
        let mut body_html = parsed
            .as_ref()
            .and_then(|message| message.body_html(0).map(|v| v.into_owned()));
        let mut attachments = Vec::new();
        if let Some(message) = parsed.as_ref() {
            for (index, part) in message.attachments().enumerate() {
                let content_id = part.content_id().map(|value| {
                    value
                        .trim()
                        .trim_start_matches('<')
                        .trim_end_matches('>')
                        .to_owned()
                });
                let mime_type = part.content_type().map(|content_type| {
                    format!(
                        "{}/{}",
                        content_type.c_type,
                        content_type.c_subtype.as_deref().unwrap_or("octet-stream")
                    )
                });
                let bytes = match &part.body {
                    PartType::Binary(bytes) | PartType::InlineBinary(bytes) => Some(bytes.as_ref()),
                    PartType::Text(text) | PartType::Html(text) => Some(text.as_bytes()),
                    _ => None,
                };
                if let (Some(html), Some(id), Some(bytes)) =
                    (body_html.as_mut(), content_id.as_deref(), bytes)
                {
                    let data = format!(
                        "data:{};base64,{}",
                        mime_type.as_deref().unwrap_or("application/octet-stream"),
                        base64::engine::general_purpose::STANDARD.encode(bytes)
                    );
                    *html = html
                        .replace(&format!("cid:{id}"), &data)
                        .replace(&format!("cid:<{id}>"), &data);
                }
                attachments.push(Attachment {
                    id: index as i64,
                    filename: part
                        .attachment_name()
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("attachment-{}", index + 1)),
                    mime_type,
                    size: bytes.map(|value| value.len() as i64),
                    is_inline: content_id.is_some(),
                    content_id,
                    fetched: bytes.is_some(),
                });
            }
        }
        let is_newsletter = parsed
            .as_ref()
            .is_some_and(|message| message.header("List-Unsubscribe").is_some());
        let unsubscribe = parsed.as_ref().and_then(|message| {
            // Берём сырое значение заголовка: mail_parser типизирует List-Unsubscribe
            // (в нём есть mailto:), из-за чего as_text() возвращает None и стандартная
            // ссылка отписки терялась. RFC 2369: несколько <URL> через запятую.
            let value = message.header_raw("List-Unsubscribe")?;
            let targets = value
                .split(',')
                .map(|item| item.trim().trim_start_matches('<').trim_end_matches('>'))
                .collect::<Vec<_>>();
            let http = targets
                .iter()
                .find(|item| item.starts_with("https://") || item.starts_with("http://"))
                .map(|item| (*item).to_owned());
            let mailto = targets
                .iter()
                .find(|item| item.starts_with("mailto:"))
                .map(|item| (*item).to_owned());
            // RFC 8058: одношаговая отписка, если сервер прислал этот заголовок.
            let one_click = message
                .header_raw("List-Unsubscribe-Post")
                .is_some_and(|value| value.to_ascii_lowercase().contains("one-click"));
            Some(Unsubscribe {
                one_click_url: one_click.then(|| http.clone()).flatten(),
                mailto,
                http,
            })
        });
        let has_remote_content = body_html
            .as_deref()
            .is_some_and(|html| html.contains("http://") || html.contains("https://"));
        if let Some(reference) = raw_ref.as_deref() {
            sqlx::query(
                "INSERT INTO message_content_cache(
                    message_id, raw_blob_ref, body_html, body_text, attachments_json,
                    has_remote_content, is_newsletter, unsubscribe_json, parsed_at
                 ) VALUES(?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
                 ON CONFLICT(message_id) DO UPDATE SET
                    raw_blob_ref=excluded.raw_blob_ref, body_html=excluded.body_html,
                    body_text=excluded.body_text, attachments_json=excluded.attachments_json,
                    has_remote_content=excluded.has_remote_content,
                    is_newsletter=excluded.is_newsletter,
                    unsubscribe_json=excluded.unsubscribe_json, parsed_at=datetime('now')",
            )
            .bind(message_id)
            .bind(reference)
            .bind(&body_html)
            .bind(&body_text)
            .bind(serde_json::to_string(&attachments)?)
            .bind(has_remote_content as i64)
            .bind(is_newsletter as i64)
            .bind(
                unsubscribe
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
            )
            .execute(&self.write_pool)
            .await?;
        }
        Ok(MessageFull {
            meta,
            has_remote_content,
            body_html,
            body_text,
            attachments,
            is_newsletter,
            unsubscribe,
        })
    }

    /// Метаданные письма по локальному id для карточки уведомления:
    /// (id, отправитель, тема, превью). Тело и вложения сюда не тянем -
    /// уведомлению нужен только короткий текст.
    pub async fn message_notification_preview(
        &self,
        message_id: i64,
    ) -> Result<Option<(i64, String, String, String)>> {
        let row: Option<NotificationPreviewRow> = sqlx::query_as(
            "SELECT id, from_name, from_addr, subject, preview FROM messages WHERE id = ?",
        )
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(id, name, addr, subject, preview)| {
            let from = name
                .filter(|value| !value.trim().is_empty())
                .or(addr)
                .unwrap_or_default();
            (id, from, subject, preview.unwrap_or_default())
        }))
    }

    /// Локальные id писем аккаунта из Входящих (роль строго 'inbox', без
    /// "OR role IS NULL" - уведомлению не нужны письма из папок без роли),
    /// у которых remote_id входит в переданный список. Сортировка по дате
    /// по возрастанию: последний элемент результата - самое свежее письмо.
    pub async fn inbox_message_ids_by_remote_ids(
        &self,
        account_id: i64,
        remote_ids: &[String],
    ) -> Result<Vec<i64>> {
        if remote_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; remote_ids.len()].join(",");
        let sql = format!(
            "SELECT m.id FROM messages m JOIN folders f ON f.id = m.folder_id \
                 WHERE m.account_id = ? AND f.role = 'inbox' AND m.remote_id IN ({placeholders}) \
                 ORDER BY m.date ASC"
        );
        // Плейсхолдеры формируются только из числа id (не из пользовательских
        // данных), сами значения передаются через bind - инъекция невозможна.
        let mut query = sqlx::query_as::<_, (i64,)>(sqlx::AssertSqlSafe(sql)).bind(account_id);
        for id in remote_ids {
            query = query.bind(id);
        }
        let rows = query.fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Из набора remote_id вернуть те, которых ещё нет в БД (новые письма).
    pub async fn unknown_remote_ids(&self, account_id: i64, ids: &[String]) -> Result<Vec<String>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; ids.len()].join(",");
        let sql = format!(
            "SELECT remote_id FROM messages WHERE account_id = ? AND remote_id IN ({placeholders})"
        );
        // Плейсхолдеры формируются только из числа id (не из пользовательских
        // данных), сами значения передаются через bind - инъекция невозможна.
        let mut query =
            sqlx::query_as::<_, (Option<String>,)>(sqlx::AssertSqlSafe(sql)).bind(account_id);
        for id in ids {
            query = query.bind(id);
        }
        let existing = query.fetch_all(&self.pool).await?;
        let known: std::collections::HashSet<String> =
            existing.into_iter().filter_map(|row| row.0).collect();
        Ok(ids
            .iter()
            .filter(|id| !known.contains(*id))
            .cloned()
            .collect())
    }

    /// Сырой MIME-исходник письма (для просмотра "как есть" и диагностики).
    pub async fn message_raw(&self, message_id: i64) -> Result<String> {
        Ok(String::from_utf8_lossy(&self.message_raw_bytes(message_id).await?).into_owned())
    }

    /// Исходные байты RFC 5322/MIME без перекодирования — для экспорта `.eml`.
    pub async fn message_raw_bytes(&self, message_id: i64) -> Result<Vec<u8>> {
        let (raw_ref,): (Option<String>,) =
            sqlx::query_as("SELECT raw_blob_ref FROM messages WHERE id = ?")
                .bind(message_id)
                .fetch_one(&self.pool)
                .await?;
        let raw = raw_ref
            .as_deref()
            .map(|reference| self.blobs.get(reference))
            .transpose()?
            .ok_or_else(|| crate::Error::Other("исходник письма недоступен".into()))?;
        Ok(raw)
    }

    /// Извлечь содержимое вложения по индексу (Attachment.id) из raw-MIME письма.
    /// Возвращает (имя файла, mime-тип, байты).
    pub async fn attachment_bytes(
        &self,
        message_id: i64,
        attachment_id: i64,
    ) -> Result<(String, Option<String>, Vec<u8>)> {
        use mail_parser::{MessageParser, MimeHeaders, PartType};
        let (raw_ref,): (Option<String>,) =
            sqlx::query_as("SELECT raw_blob_ref FROM messages WHERE id = ?")
                .bind(message_id)
                .fetch_one(&self.pool)
                .await?;
        let raw = raw_ref
            .as_deref()
            .map(|reference| self.blobs.get(reference))
            .transpose()?
            .ok_or_else(|| crate::Error::Other("raw письма недоступно".into()))?;
        let parsed = MessageParser::default()
            .parse(&raw)
            .ok_or_else(|| crate::Error::Other("не удалось разобрать письмо".into()))?;
        let part = parsed
            .attachments()
            .nth(attachment_id as usize)
            .ok_or_else(|| crate::Error::Other("вложение не найдено".into()))?;
        let bytes = match &part.body {
            PartType::Binary(value) | PartType::InlineBinary(value) => value.to_vec(),
            PartType::Text(value) | PartType::Html(value) => value.as_bytes().to_vec(),
            _ => return Err(crate::Error::Other("вложение без содержимого".into())),
        };
        let mime_type = part.content_type().map(|content_type| {
            format!(
                "{}/{}",
                content_type.c_type,
                content_type.c_subtype.as_deref().unwrap_or("octet-stream")
            )
        });
        let filename = part
            .attachment_name()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("attachment-{}", attachment_id + 1));
        Ok((filename, mime_type, bytes))
    }

    pub async fn list_calendars_and_events(&self) -> Result<(Vec<CalendarSummary>, Vec<Event>)> {
        let calendar_rows: Vec<(i64, i64, String, Option<String>, i64, i64)> = sqlx::query_as(
            "SELECT id, account_id, name, color, visible, read_only FROM calendars ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?;
        let calendars: Vec<CalendarSummary> = calendar_rows
            .into_iter()
            .map(|row| CalendarSummary {
                id: row.0,
                account_id: row.1,
                name: row.2,
                color: row.3,
                visible: row.4 != 0,
                read_only: row.5 != 0,
            })
            .collect();
        let rows: Vec<EventRow> = sqlx::query_as(
            "SELECT id, calendar_id, uid, summary, description, location, dtstart, dtend,
                    all_day, rrule, recurrence_id, exdates, rdates, timezone, status, transp, class,
                    categories, url, organizer, sequence
             FROM events ORDER BY dtstart",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut events: Vec<Event> = rows.into_iter().map(Into::into).collect();
        let indexes: std::collections::HashMap<i64, usize> = events
            .iter()
            .enumerate()
            .filter_map(|(index, event)| event.id.map(|id| (id, index)))
            .collect();
        let attendee_rows: Vec<EventAttendeeRow> = sqlx::query_as(
            "SELECT event_id, email, name, role, partstat, rsvp
             FROM event_attendees ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?;
        for attendee in attendee_rows {
            if let Some(index) = indexes.get(&attendee.event_id) {
                events[*index].attendees.push(crate::model::Attendee {
                    email: attendee.email,
                    name: attendee.name,
                    role: attendee.role,
                    partstat: attendee.partstat,
                    rsvp: attendee.rsvp != 0,
                });
            }
        }
        let alarm_rows: Vec<EventAlarmRow> = sqlx::query_as(
            "SELECT event_id, trigger_minutes, action FROM event_alarms ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?;
        for alarm in alarm_rows {
            if let Some(index) = indexes.get(&alarm.event_id) {
                events[*index].alarms.push(crate::model::Alarm {
                    trigger_minutes: alarm.trigger_minutes,
                    action: alarm.action,
                });
            }
        }
        // Свой статус участия - удобное представление для UI (см. resolve_my_attendance):
        // сопоставляем участников события с email аккаунта, которому принадлежит
        // календарь события (через calendar_id -> account_id -> accounts.email).
        let account_emails: std::collections::HashMap<i64, String> =
            sqlx::query_as::<_, (i64, String)>("SELECT id, email FROM accounts")
                .fetch_all(&self.pool)
                .await?
                .into_iter()
                .collect();
        let calendar_accounts: std::collections::HashMap<i64, i64> =
            calendars.iter().map(|c| (c.id, c.account_id)).collect();
        for event in events.iter_mut() {
            if let Some(email) = calendar_accounts
                .get(&event.calendar_id)
                .and_then(|account_id| account_emails.get(account_id))
            {
                let (my_partstat, needs_response) = crate::model::resolve_my_attendance(
                    &event.attendees,
                    event.organizer.as_deref(),
                    email,
                );
                event.my_partstat = my_partstat;
                event.needs_response = needs_response;
            }
        }
        Ok((calendars, events))
    }

    /// Контекст события для отправки ответа на приглашение: серверные
    /// идентификаторы календаря/события и модель события с участниками -
    /// без my_partstat/needs_response, их выше по стеку пересчитывает
    /// вызывающий код (respond_to_event в commands.rs), уже имея полный
    /// Account, а не только email.
    pub async fn event_for_response(&self, event_id: i64) -> Result<Option<EventResponseContext>> {
        let Some(row): Option<EventRow> = sqlx::query_as(
            "SELECT id, calendar_id, uid, summary, description, location, dtstart, dtend,
                    all_day, rrule, recurrence_id, exdates, rdates, timezone, status, transp, class,
                    categories, url, organizer, sequence
             FROM events WHERE id=?",
        )
        .bind(event_id)
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };
        let mut event: Event = row.into();
        let attendee_rows: Vec<EventAttendeeRow> = sqlx::query_as(
            "SELECT event_id, email, name, role, partstat, rsvp FROM event_attendees WHERE event_id=?",
        )
        .bind(event_id)
        .fetch_all(&self.pool)
        .await?;
        event.attendees = attendee_rows
            .into_iter()
            .map(|row| crate::model::Attendee {
                email: row.email,
                name: row.name,
                role: row.role,
                partstat: row.partstat,
                rsvp: row.rsvp != 0,
            })
            .collect();
        let calendar: (i64, String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT c.account_id, c.url, e.remote_url, e.etag
             FROM events e JOIN calendars c ON c.id=e.calendar_id WHERE e.id=?",
        )
        .bind(event_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(Some(EventResponseContext {
            account_id: calendar.0,
            calendar_source: calendar.1,
            remote_url: calendar.2,
            etag: calendar.3,
            event,
        }))
    }

    /// После успешной отправки ответа обновить свою запись в event_attendees
    /// локально, не дожидаясь следующей синхронизации - иначе UI показывал бы
    /// старый PARTSTAT до следующего прохода sync_auxiliary_account.
    pub async fn update_own_partstat(
        &self,
        event_id: i64,
        account_email: &str,
        partstat: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE event_attendees SET partstat=? WHERE event_id=? AND lower(email)=lower(?)",
        )
        .bind(partstat)
        .bind(event_id)
        .bind(account_email)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// Нужен ли ответ на приглашение для этого события и адреса аккаунта -
    /// используется при формировании карточки уведомления об изменении
    /// календаря (см. notify_calendar_changes в commands.rs), где под рукой
    /// только event_id, а не полный Event.
    pub async fn event_needs_response(&self, event_id: i64, account_email: &str) -> Result<bool> {
        let organizer: Option<(Option<String>,)> =
            sqlx::query_as("SELECT organizer FROM events WHERE id=?")
                .bind(event_id)
                .fetch_optional(&self.pool)
                .await?;
        let Some((organizer,)) = organizer else {
            return Ok(false);
        };
        let attendee_rows: Vec<EventAttendeeRow> = sqlx::query_as(
            "SELECT event_id, email, name, role, partstat, rsvp FROM event_attendees WHERE event_id=?",
        )
        .bind(event_id)
        .fetch_all(&self.pool)
        .await?;
        let attendees: Vec<crate::model::Attendee> = attendee_rows
            .into_iter()
            .map(|row| crate::model::Attendee {
                email: row.email,
                name: row.name,
                role: row.role,
                partstat: row.partstat,
                rsvp: row.rsvp != 0,
            })
            .collect();
        Ok(crate::model::resolve_my_attendance(&attendees, organizer.as_deref(), account_email).1)
    }

    pub async fn set_calendar_visible(&self, calendar_id: i64, visible: bool) -> Result<()> {
        sqlx::query("UPDATE calendars SET visible=? WHERE id=?")
            .bind(visible)
            .bind(calendar_id)
            .execute(&self.write_pool)
            .await?;
        Ok(())
    }

    /// Отметить письмо прочитанным (локально; в outbox уйдёт синхронизация флага).
    pub async fn mark_seen(&self, message_id: i64, seen: bool) -> Result<()> {
        let mut tx = self.begin_write().await?;
        let locator: (i64, i64, String, Option<String>, i64) = sqlx::query_as(
            "SELECT m.account_id, m.uid, f.remote_path, m.remote_id, m.flagged FROM messages m
             JOIN folders f ON f.id=m.folder_id WHERE m.id=?",
        )
        .bind(message_id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("UPDATE messages SET seen = ? WHERE id = ?")
            .bind(seen as i64)
            .bind(message_id)
            .execute(&mut *tx)
            .await?;
        Self::queue_flag_sync(
            &mut tx,
            message_id,
            locator.0,
            &locator.2,
            locator.1,
            locator.3.as_deref(),
            seen,
            locator.4 != 0,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Отметить/снять звёздочку (\Flagged; локально, в outbox уйдёт
    /// синхронизация). Устроено симметрично mark_seen - seen и flagged
    /// синхронизируются независимо и не затирают друг друга, потому что
    /// payload каждый раз несёт оба текущих значения, а не только изменённое
    /// (см. queue_flag_sync).
    pub async fn mark_flagged(&self, message_id: i64, flagged: bool) -> Result<()> {
        let mut tx = self.begin_write().await?;
        let locator: (i64, i64, String, Option<String>, i64) = sqlx::query_as(
            "SELECT m.account_id, m.uid, f.remote_path, m.remote_id, m.seen FROM messages m
             JOIN folders f ON f.id=m.folder_id WHERE m.id=?",
        )
        .bind(message_id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("UPDATE messages SET flagged = ? WHERE id = ?")
            .bind(flagged as i64)
            .bind(message_id)
            .execute(&mut *tx)
            .await?;
        Self::queue_flag_sync(
            &mut tx,
            message_id,
            locator.0,
            &locator.2,
            locator.1,
            locator.3.as_deref(),
            locator.4 != 0,
            flagged,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Общая часть mark_seen/mark_flagged: заменить незавершённую
    /// 'flag'-операцию в outbox на новую с актуальным состоянием обоих
    /// флагов сразу. Пересборка целиком (а не только изменённого поля)
    /// нужна, чтобы более ранняя ещё не отправленная пометка не потерялась,
    /// когда пользователь быстро меняет seen и flagged подряд.
    async fn queue_flag_sync(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        message_id: i64,
        account_id: i64,
        folder_path: &str,
        uid: i64,
        remote_id: Option<&str>,
        seen: bool,
        flagged: bool,
    ) -> Result<()> {
        let payload = serde_json::json!({
            "message_id": message_id,
            "folder_path": folder_path,
            "uid": uid,
            "remote_id": remote_id,
            "seen": seen,
            "flagged": flagged,
        });
        sqlx::query("DELETE FROM outbox_ops WHERE message_id=? AND op_kind='flag' AND status IN ('pending','retry')")
            .bind(message_id)
            .execute(&mut **tx)
            .await?;
        sqlx::query(
            "INSERT INTO outbox_ops(account_id, message_id, op_kind, payload, status, next_attempt_at)
             VALUES(?, ?, 'flag', ?, 'pending', datetime('now'))",
        )
        .bind(account_id)
        .bind(message_id)
        .bind(payload.to_string())
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Поставить реальное перемещение/удаление на сервере в очередь. Локальный
    /// список скрывает такие письма сразу, но undo может отменить pending op.
    pub async fn queue_message_action(
        &self,
        message_ids: &[i64],
        target_role: &str,
    ) -> Result<QueuedAction> {
        let mut tx = self.begin_write().await?;
        let mut operation_ids = Vec::new();
        for message_id in message_ids {
            let locator: (i64, i64, i64, String, Option<String>, Option<String>) = sqlx::query_as(
                "SELECT m.account_id, m.folder_id, m.uid, f.remote_path, f.role, m.remote_id
                 FROM messages m JOIN folders f ON f.id=m.folder_id WHERE m.id=?",
            )
            .bind(message_id)
            .fetch_one(&mut *tx)
            .await?;
            let permanently_delete =
                target_role == "trash" && locator.4.as_deref() == Some("trash");
            let (kind, target): (&str, Option<(i64, String)>) = if permanently_delete {
                ("delete", None)
            } else {
                let mut target = sqlx::query_as::<_, (i64, String)>(
                    "SELECT id, remote_path FROM folders WHERE account_id=? AND role=? LIMIT 1",
                )
                .bind(locator.0)
                .bind(target_role)
                .fetch_optional(&mut *tx)
                .await?;
                if target.is_none() {
                    let expected = FolderRole::parse(target_role);
                    let folders = sqlx::query_as::<_, (i64, String, String)>(
                        "SELECT id, remote_path, display_name FROM folders WHERE account_id=?",
                    )
                    .bind(locator.0)
                    .fetch_all(&mut *tx)
                    .await?;
                    if let Some((id, path, _)) = folders.into_iter().find(|(_, path, name)| {
                        crate::model::infer_folder_role(path, name) == expected
                    }) {
                        sqlx::query("UPDATE folders SET role=? WHERE id=?")
                            .bind(target_role)
                            .bind(id)
                            .execute(&mut *tx)
                            .await?;
                        target = Some((id, path));
                    }
                }
                let target = target.ok_or_else(|| {
                    crate::Error::AccountConfig(format!(
                        "для аккаунта не назначена папка {target_role}"
                    ))
                })?;
                ("move", Some(target))
            };
            let payload = serde_json::json!({
                "message_id": message_id,
                "folder_id": locator.1,
                "folder_path": locator.3,
                "uid": locator.2,
                "remote_id": locator.5,
                "target_folder_id": target.as_ref().map(|value| value.0),
                "target_folder_path": target.as_ref().map(|value| value.1.as_str()),
            });
            let result = sqlx::query(
                "INSERT INTO outbox_ops(account_id, message_id, op_kind, payload, status, next_attempt_at)
                 VALUES(?, ?, ?, ?, 'pending', datetime('now','+10 seconds'))",
            )
            .bind(locator.0)
            .bind(message_id)
            .bind(kind)
            .bind(payload.to_string())
            .execute(&mut *tx)
            .await?;
            operation_ids.push(result.last_insert_rowid());
        }
        tx.commit().await?;
        Ok(QueuedAction { operation_ids })
    }

    /// Queue moving messages to an explicitly selected folder. Rules use this
    /// for user folders which do not have a system role.
    pub async fn queue_message_move(
        &self,
        message_ids: &[i64],
        target_folder_id: i64,
    ) -> Result<QueuedAction> {
        let mut tx = self.begin_write().await?;
        let target: (i64, String) =
            sqlx::query_as("SELECT account_id, remote_path FROM folders WHERE id=?")
                .bind(target_folder_id)
                .fetch_one(&mut *tx)
                .await?;
        let mut operation_ids = Vec::new();
        for message_id in message_ids {
            let locator: (i64, i64, i64, String, Option<String>) = sqlx::query_as(
                "SELECT m.account_id, m.folder_id, m.uid, f.remote_path, m.remote_id
                 FROM messages m JOIN folders f ON f.id=m.folder_id WHERE m.id=?",
            )
            .bind(message_id)
            .fetch_one(&mut *tx)
            .await?;
            if locator.0 != target.0 {
                return Err(crate::Error::AccountConfig(
                    "нельзя переместить письмо в папку другого аккаунта".into(),
                ));
            }
            if locator.1 == target_folder_id {
                continue;
            }
            let payload = serde_json::json!({
                "message_id": message_id,
                "folder_id": locator.1,
                "folder_path": locator.3,
                "uid": locator.2,
                "remote_id": locator.4,
                "target_folder_id": target_folder_id,
                "target_folder_path": target.1.as_str(),
            });
            let result = sqlx::query(
                "INSERT INTO outbox_ops(account_id, message_id, op_kind, payload, status, next_attempt_at)
                 VALUES(?, ?, 'move', ?, 'pending', datetime('now','+10 seconds'))",
            )
            .bind(locator.0)
            .bind(message_id)
            .bind(payload.to_string())
            .execute(&mut *tx)
            .await?;
            operation_ids.push(result.last_insert_rowid());
        }
        tx.commit().await?;
        Ok(QueuedAction { operation_ids })
    }

    pub async fn cancel_outbox_operations(&self, operation_ids: &[i64]) -> Result<usize> {
        let mut removed = 0;
        for id in operation_ids {
            removed +=
                sqlx::query("DELETE FROM outbox_ops WHERE id=? AND status IN ('pending','retry')")
                    .bind(id)
                    .execute(&self.write_pool)
                    .await?
                    .rows_affected() as usize;
        }
        Ok(removed)
    }

    pub async fn claim_outbox_operations(
        &self,
        account_id: i64,
        limit: i64,
    ) -> Result<Vec<OutboxOperation>> {
        let rows: Vec<OutboxRow> = sqlx::query_as(
            "UPDATE outbox_ops SET status='processing', next_attempt_at=datetime('now','+2 minutes')
             WHERE id IN (
               SELECT id FROM outbox_ops
                WHERE account_id=? AND status IN ('pending','retry','processing')
                  AND coalesce(next_attempt_at, created_at) <= datetime('now')
                ORDER BY id LIMIT ?
             )
             RETURNING id, account_id, message_id, op_kind, payload, attempts",
        )
        .bind(account_id)
        .bind(limit)
        // UPDATE ... RETURNING - это запись, несмотря на fetch_all: только через
        // очередь записи, иначе конкурирует с ней за блокировку писателя.
        .fetch_all(&self.write_pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Вернуть в очередь операции Exchange, отложенные старой реализацией,
    /// которая отправляла UpdateItem без обязательного ChangeKey.
    pub async fn requeue_exchange_change_key_operations(&self, account_id: i64) -> Result<usize> {
        let result = sqlx::query(
            "UPDATE outbox_ops SET status='retry', attempts=0, next_attempt_at=datetime('now')
             WHERE account_id=? AND status IN ('retry','failed')
               AND attempts >= 7
               AND last_error LIKE '%ChangeKey is required%'",
        )
        .bind(account_id)
        .execute(&self.write_pool)
        .await?;
        Ok(result.rows_affected() as usize)
    }

    pub async fn queue_scheduled_send(
        &self,
        account_id: i64,
        payload: &str,
        send_at: &str,
    ) -> Result<i64> {
        let result = sqlx::query(
            "INSERT INTO outbox_ops(account_id, op_kind, payload, status, next_attempt_at)
             VALUES(?, 'send', ?, 'pending', ?)",
        )
        .bind(account_id)
        .bind(payload)
        .bind(send_at)
        .execute(&self.write_pool)
        .await?;
        Ok(result.last_insert_rowid())
    }

    /// SMTP уже принял письмо, но IMAP APPEND серверной копии не завершился.
    /// Повторяем только APPEND: повторная SMTP-отправка создала бы дубль у адресата.
    pub async fn queue_sent_append(
        &self,
        account_id: i64,
        payload: &str,
        error: &str,
    ) -> Result<i64> {
        let result = sqlx::query(
            "INSERT INTO outbox_ops(account_id, op_kind, payload, status, attempts,
                                    next_attempt_at, last_error)
             VALUES(?, 'append_sent', ?, 'retry', 0, datetime('now','+5 seconds'), ?)",
        )
        .bind(account_id)
        .bind(payload)
        .bind(error.chars().take(1000).collect::<String>())
        .execute(&self.write_pool)
        .await?;
        Ok(result.last_insert_rowid())
    }

    /// Превратить scheduled `send` в append-only retry после успешного SMTP DATA.
    pub async fn convert_outbox_to_sent_append(
        &self,
        id: i64,
        payload: &str,
        error: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE outbox_ops
                SET op_kind='append_sent', payload=?, status='retry', attempts=0,
                    next_attempt_at=datetime('now','+5 seconds'), last_error=?
              WHERE id=?",
        )
        .bind(payload)
        .bind(error.chars().take(1000).collect::<String>())
        .bind(id)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    pub async fn complete_outbox_operation(&self, operation: &OutboxOperation) -> Result<()> {
        let mut tx = self.begin_write().await?;
        if matches!(operation.op_kind.as_str(), "move" | "delete")
            && let Some(message_id) = operation.message_id
        {
            sqlx::query("DELETE FROM messages WHERE id=?")
                .bind(message_id)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("DELETE FROM outbox_ops WHERE id=?")
            .bind(operation.id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn fail_outbox_operation(&self, id: i64, error: &str) -> Result<()> {
        let attempts: (i64,) = sqlx::query_as("SELECT attempts+1 FROM outbox_ops WHERE id=?")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        let delay = (5_i64.saturating_mul(1_i64 << attempts.0.min(9))).min(3600);
        let status = if attempts.0 >= 8 { "failed" } else { "retry" };
        sqlx::query(
            "UPDATE outbox_ops SET attempts=?, last_error=?, status=?,
                    next_attempt_at=datetime('now', ?)
             WHERE id=?",
        )
        .bind(attempts.0)
        .bind(error.chars().take(1000).collect::<String>())
        .bind(status)
        .bind(format!("+{delay} seconds"))
        .bind(id)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    // ---------- Правила обработки почты ----------

    pub async fn import_legacy_mail_rules(&self) -> Result<()> {
        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM mail_rules")
            .fetch_one(&self.pool)
            .await?;
        if count > 0 {
            return Ok(());
        }
        let Some(serialized) = self.setting("mail_rules_ui").await? else {
            return Ok(());
        };
        let Ok(rules) = serde_json::from_str::<Vec<serde_json::Value>>(&serialized) else {
            tracing::warn!("старые правила UI не импортированы: JSON повреждён");
            return Ok(());
        };
        let mut tx = self.begin_write().await?;
        for (sort_order, rule) in rules.into_iter().enumerate() {
            let string = |key: &str| rule.get(key).and_then(|value| value.as_str());
            let Some(id) = string("id") else { continue };
            let Some(name) = string("name") else { continue };
            let Some(field) = string("field") else {
                continue;
            };
            let Some(operator) = string("operator") else {
                continue;
            };
            let Some(value) = string("value") else {
                continue;
            };
            let Some(action) = string("action") else {
                continue;
            };
            if !matches!(field, "sender" | "subject")
                || !matches!(operator, "contains" | "equals")
                || !matches!(action, "move" | "archive" | "spam" | "trash")
            {
                continue;
            }
            sqlx::query(
                "INSERT OR IGNORE INTO mail_rules(
                    id, name, field, operator, value, account_id, action, folder_id,
                    enabled, progress_message_id, sort_order
                 ) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(name)
            .bind(field)
            .bind(operator)
            .bind(value)
            .bind(rule.get("account_id").and_then(|value| value.as_i64()))
            .bind(action)
            .bind(rule.get("folder_id").and_then(|value| value.as_i64()))
            .bind(
                rule.get("enabled")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(true),
            )
            .bind(
                rule.get("last_id")
                    .and_then(|value| value.as_i64())
                    .unwrap_or(0),
            )
            .bind(sort_order as i64)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query("DELETE FROM settings WHERE key='mail_rules_ui'")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn list_mail_rules(&self) -> Result<Vec<MailRule>> {
        let rows: Vec<MailRuleRow> = sqlx::query_as(
            "SELECT id, name, field, operator, value, account_id, action, folder_id,
                    enabled, progress_message_id, sort_order
             FROM mail_rules ORDER BY sort_order, created_at, id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn save_mail_rule(
        &self,
        rule: &MailRuleInput,
        apply_existing: bool,
    ) -> Result<MailRule> {
        if rule.id.trim().is_empty() || rule.name.trim().is_empty() || rule.value.trim().is_empty()
        {
            return Err(crate::Error::AccountConfig(
                "правилу нужны id, название и значение".into(),
            ));
        }
        if !matches!(rule.field.as_str(), "sender" | "subject")
            || !matches!(rule.operator.as_str(), "contains" | "equals")
            || !matches!(rule.action.as_str(), "move" | "archive" | "spam" | "trash")
        {
            return Err(crate::Error::AccountConfig(
                "правило содержит неподдерживаемое условие или действие".into(),
            ));
        }
        let mut tx = self.begin_write().await?;
        if let Some(account_id) = rule.account_id {
            let exists: Option<(i64,)> =
                sqlx::query_as("SELECT id FROM accounts WHERE id=? AND enabled=1")
                    .bind(account_id)
                    .fetch_optional(&mut *tx)
                    .await?;
            if exists.is_none() {
                return Err(crate::Error::AccountConfig(
                    "аккаунт правила не найден".into(),
                ));
            }
        }
        if rule.action == "move" {
            let account_id = rule.account_id.ok_or_else(|| {
                crate::Error::AccountConfig("для перемещения выберите конкретный аккаунт".into())
            })?;
            let folder_id = rule
                .folder_id
                .ok_or_else(|| crate::Error::AccountConfig("папка назначения не выбрана".into()))?;
            let target: Option<(i64,)> =
                sqlx::query_as("SELECT id FROM folders WHERE id=? AND account_id=?")
                    .bind(folder_id)
                    .bind(account_id)
                    .fetch_optional(&mut *tx)
                    .await?;
            if target.is_none() {
                return Err(crate::Error::AccountConfig(
                    "папка назначения не принадлежит аккаунту правила".into(),
                ));
            }
        }
        let existing: Option<(i64, i64)> =
            sqlx::query_as("SELECT progress_message_id, sort_order FROM mail_rules WHERE id=?")
                .bind(&rule.id)
                .fetch_optional(&mut *tx)
                .await?;
        let progress = if apply_existing {
            0
        } else if let Some((progress, _)) = existing {
            progress
        } else {
            sqlx::query_as::<_, (i64,)>("SELECT coalesce(max(id), 0) FROM messages")
                .fetch_one(&mut *tx)
                .await?
                .0
        };
        let sort_order = if let Some((_, sort_order)) = existing {
            sort_order
        } else {
            sqlx::query_as::<_, (i64,)>("SELECT coalesce(max(sort_order), -1)+1 FROM mail_rules")
                .fetch_one(&mut *tx)
                .await?
                .0
        };
        sqlx::query(
            "INSERT INTO mail_rules(
                id, name, field, operator, value, account_id, action, folder_id,
                enabled, progress_message_id, sort_order
             ) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                name=excluded.name, field=excluded.field, operator=excluded.operator,
                value=excluded.value, account_id=excluded.account_id,
                action=excluded.action, folder_id=excluded.folder_id,
                enabled=excluded.enabled, progress_message_id=excluded.progress_message_id,
                updated_at=datetime('now')",
        )
        .bind(&rule.id)
        .bind(rule.name.trim())
        .bind(&rule.field)
        .bind(&rule.operator)
        .bind(rule.value.trim())
        .bind(rule.account_id)
        .bind(&rule.action)
        .bind((rule.action == "move").then_some(rule.folder_id).flatten())
        .bind(rule.enabled)
        .bind(progress)
        .bind(sort_order)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.list_mail_rules()
            .await?
            .into_iter()
            .find(|saved| saved.id == rule.id)
            .ok_or_else(|| crate::Error::Other("сохранённое правило не найдено".into()))
    }

    pub async fn set_mail_rule_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        let changed =
            sqlx::query("UPDATE mail_rules SET enabled=?, updated_at=datetime('now') WHERE id=?")
                .bind(enabled)
                .bind(id)
                .execute(&self.write_pool)
                .await?;
        if changed.rows_affected() != 1 {
            return Err(crate::Error::Other("правило не найдено".into()));
        }
        Ok(())
    }

    pub async fn delete_mail_rule(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM mail_rules WHERE id=?")
            .bind(id)
            .execute(&self.write_pool)
            .await?;
        Ok(())
    }

    /// Match new messages and atomically queue server actions together with
    /// rule progress. A crash can therefore cause neither a skipped message nor
    /// a duplicate operation.
    pub async fn process_mail_rules(&self) -> Result<usize> {
        let mut tx = self.begin_write().await?;
        let mut rules: Vec<MailRuleRow> = sqlx::query_as(
            "SELECT id, name, field, operator, value, account_id, action, folder_id,
                    enabled, progress_message_id, sort_order
             FROM mail_rules WHERE enabled=1 ORDER BY sort_order, created_at, id",
        )
        .fetch_all(&mut *tx)
        .await?;
        if rules.is_empty() {
            tx.commit().await?;
            return Ok(0);
        }
        let min_progress = rules
            .iter()
            .map(|rule| rule.progress_message_id)
            .min()
            .unwrap_or(0);
        let messages: Vec<RuleMessageRow> = sqlx::query_as(
            "SELECT m.id, m.account_id, m.folder_id, m.uid, f.remote_path,
                    m.remote_id, m.from_name, m.from_addr, m.subject
             FROM messages m JOIN folders f ON f.id=m.folder_id
             WHERE m.id>? AND (f.role IS NULL OR f.role NOT IN
                    ('sent','drafts','archive','spam','trash'))
               AND NOT EXISTS (
                    SELECT 1 FROM outbox_ops o WHERE o.message_id=m.id
                      AND o.op_kind IN ('move','delete')
                      AND o.status IN ('pending','processing','retry')
               )
             ORDER BY m.id LIMIT 500",
        )
        .bind(min_progress)
        .fetch_all(&mut *tx)
        .await?;
        let mut queued = 0;
        for message in messages {
            let matching = rules.iter().position(|rule| {
                if message.id <= rule.progress_message_id
                    || rule.account_id.is_some_and(|id| id != message.account_id)
                {
                    return false;
                }
                let source = if rule.field == "subject" {
                    message.subject.clone()
                } else {
                    format!(
                        "{} {}",
                        message.from_name.as_deref().unwrap_or(""),
                        message.from_addr.as_deref().unwrap_or("")
                    )
                };
                let source = source.to_lowercase();
                let value = rule.value.to_lowercase();
                if rule.operator == "equals" {
                    source == value
                } else {
                    source.contains(&value)
                }
            });
            if let Some(index) = matching {
                let rule = &rules[index];
                let target = if rule.action == "move" {
                    let folder_id = rule.folder_id.ok_or_else(|| {
                        crate::Error::AccountConfig(format!(
                            "у правила {} нет папки назначения",
                            rule.name
                        ))
                    })?;
                    sqlx::query_as::<_, (i64, String)>(
                        "SELECT id, remote_path FROM folders WHERE id=? AND account_id=?",
                    )
                    .bind(folder_id)
                    .bind(message.account_id)
                    .fetch_optional(&mut *tx)
                    .await?
                } else {
                    sqlx::query_as::<_, (i64, String)>(
                        "SELECT id, remote_path FROM folders WHERE account_id=? AND role=? LIMIT 1",
                    )
                    .bind(message.account_id)
                    .bind(&rule.action)
                    .fetch_optional(&mut *tx)
                    .await?
                }
                .ok_or_else(|| {
                    crate::Error::AccountConfig(format!(
                        "для правила {} не найдена папка назначения",
                        rule.name
                    ))
                })?;
                if target.0 != message.folder_id {
                    let payload = serde_json::json!({
                        "message_id": message.id,
                        "folder_id": message.folder_id,
                        "folder_path": message.remote_path,
                        "uid": message.uid,
                        "remote_id": message.remote_id,
                        "target_folder_id": target.0,
                        "target_folder_path": target.1,
                        "rule_id": rule.id,
                    });
                    sqlx::query(
                        "INSERT INTO outbox_ops(
                            account_id, message_id, op_kind, payload, status, next_attempt_at
                         ) VALUES(?, ?, 'move', ?, 'pending', datetime('now'))",
                    )
                    .bind(message.account_id)
                    .bind(message.id)
                    .bind(payload.to_string())
                    .execute(&mut *tx)
                    .await?;
                    queued += 1;
                }
            }
            for rule in &mut rules {
                if message.id > rule.progress_message_id {
                    rule.progress_message_id = message.id;
                }
            }
        }
        for rule in &rules {
            sqlx::query(
                "UPDATE mail_rules SET progress_message_id=?, updated_at=datetime('now') WHERE id=?",
            )
            .bind(rule.progress_message_id)
            .bind(&rule.id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(queued)
    }

    // ---------- Умные папки ----------

    pub async fn list_smart_folders(&self) -> Result<Vec<SmartFolder>> {
        let rows = sqlx::query_as::<_, SmartRow>(
            "SELECT id, stable_id, name, icon, is_builtin, enabled, sort_order
             FROM smart_folders ORDER BY sort_order, id",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::new();
        for r in rows {
            let condition_rows = sqlx::query_as::<_, CondRow>(
                "SELECT field, op, value, group_index, group_logic, unit, value2
                 FROM smart_conditions WHERE smart_folder_id = ? ORDER BY group_index, id",
            )
            .bind(r.id)
            .fetch_all(&self.pool)
            .await?;
            let mut groups = Vec::<SmartConditionGroup>::new();
            for condition in condition_rows {
                let group_index = condition.group_index.max(0) as usize;
                while groups.len() <= group_index {
                    groups.push(SmartConditionGroup {
                        logic: "all".into(),
                        conditions: Vec::new(),
                    });
                }
                groups[group_index].logic = condition.group_logic;
                groups[group_index].conditions.push(SmartCondition {
                    field: condition.field,
                    op: condition.op,
                    value: condition.value,
                    unit: condition.unit,
                    value2: condition.value2,
                });
            }
            out.push(SmartFolder {
                id: r.stable_id,
                name: r.name,
                icon: r.icon,
                is_builtin: r.is_builtin != 0,
                enabled: r.enabled != 0,
                sort_order: r.sort_order,
                groups,
            });
        }
        Ok(out)
    }

    pub async fn save_smart_folders(&self, folders: &[SmartFolder]) -> Result<()> {
        let mut tx = self.begin_write().await?;
        let mut stable_ids = Vec::new();
        for (index, folder) in folders.iter().enumerate() {
            let stable_id = folder.id.trim();
            if stable_id.is_empty()
                || !stable_id
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character))
            {
                return Err(crate::Error::AccountConfig(
                    "некорректный идентификатор умной папки".into(),
                ));
            }
            if folder.name.trim().is_empty() {
                return Err(crate::Error::AccountConfig(
                    "название умной папки не указано".into(),
                ));
            }
            stable_ids.push(stable_id.to_owned());
            let existing: Option<(i64, i64)> =
                sqlx::query_as("SELECT id, is_builtin FROM smart_folders WHERE stable_id=?")
                    .bind(stable_id)
                    .fetch_optional(&mut *tx)
                    .await?;
            let database_id = if let Some((id, _)) = existing {
                sqlx::query(
                    "UPDATE smart_folders SET name=?, icon=?, enabled=?, sort_order=? WHERE id=?",
                )
                .bind(folder.name.trim())
                .bind(&folder.icon)
                .bind(folder.enabled)
                .bind(index as i64)
                .bind(id)
                .execute(&mut *tx)
                .await?;
                id
            } else {
                sqlx::query(
                    "INSERT INTO smart_folders(stable_id, name, icon, is_builtin, enabled, sort_order)
                     VALUES(?, ?, ?, 0, ?, ?)",
                )
                .bind(stable_id)
                .bind(folder.name.trim())
                .bind(&folder.icon)
                .bind(folder.enabled)
                .bind(index as i64)
                .execute(&mut *tx)
                .await?
                .last_insert_rowid()
            };
            sqlx::query("DELETE FROM smart_conditions WHERE smart_folder_id=?")
                .bind(database_id)
                .execute(&mut *tx)
                .await?;
            for (group_index, group) in folder.groups.iter().enumerate() {
                let logic = if group.logic == "any" { "any" } else { "all" };
                for condition in &group.conditions {
                    sqlx::query(
                        "INSERT INTO smart_conditions(smart_folder_id, field, op, value, group_index, group_logic, unit, value2)
                         VALUES(?, ?, ?, ?, ?, ?, ?, ?)",
                    )
                    .bind(database_id)
                    .bind(&condition.field)
                    .bind(&condition.op)
                    .bind(&condition.value)
                    .bind(group_index as i64)
                    .bind(logic)
                    .bind(&condition.unit)
                    .bind(&condition.value2)
                    .execute(&mut *tx)
                    .await?;
                }
            }
        }
        let custom_rows: Vec<(i64, String)> =
            sqlx::query_as("SELECT id, stable_id FROM smart_folders WHERE is_builtin=0")
                .fetch_all(&mut *tx)
                .await?;
        for (id, stable_id) in custom_rows {
            if !stable_ids.iter().any(|value| value == &stable_id) {
                sqlx::query("DELETE FROM smart_folders WHERE id=?")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn list_unified_sources(&self) -> Result<Vec<UnifiedSource>> {
        let (unified_id,): (i64,) =
            sqlx::query_as("SELECT id FROM unified_folders WHERE role='all'")
                .fetch_one(&self.pool)
                .await?;
        sqlx::query(
            "INSERT OR IGNORE INTO unified_sources(unified_id, folder_id, included)
             SELECT ?, id, 1 FROM folders",
        )
        .bind(unified_id)
        .execute(&self.write_pool)
        .await?;
        let rows: Vec<(i64, bool)> = sqlx::query_as(
            "SELECT folder_id, included FROM unified_sources WHERE unified_id=? ORDER BY folder_id",
        )
        .bind(unified_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(folder_id, included)| UnifiedSource {
                folder_id,
                included,
            })
            .collect())
    }

    pub async fn set_unified_source(&self, folder_id: i64, included: bool) -> Result<()> {
        sqlx::query(
            "INSERT INTO unified_sources(unified_id, folder_id, included)
             SELECT id, ?, ? FROM unified_folders WHERE role='all'
             ON CONFLICT(unified_id, folder_id) DO UPDATE SET included=excluded.included",
        )
        .bind(folder_id)
        .bind(included)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    pub async fn list_smart_folder_messages(
        &self,
        stable_id: &str,
        limit: usize,
    ) -> Result<Vec<MessageMeta>> {
        self.list_smart_folder_messages_page(stable_id, None, None, limit)
            .await
    }

    pub async fn list_smart_folder_messages_page(
        &self,
        stable_id: &str,
        before_date: Option<&str>,
        before_id: Option<i64>,
        limit: usize,
    ) -> Result<Vec<MessageMeta>> {
        let folder = self
            .list_smart_folders()
            .await?
            .into_iter()
            .find(|folder| folder.id == stable_id)
            .ok_or_else(|| crate::Error::Other("умная папка не найдена".into()))?;
        let included = sqlx::query_as::<_, (i64,)>(
            "SELECT f.id FROM folders f
             LEFT JOIN unified_sources us ON us.folder_id=f.id
               AND us.unified_id=(SELECT id FROM unified_folders WHERE role='all')
             WHERE COALESCE(us.included, 1)=1",
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| row.0)
        .collect::<std::collections::HashSet<_>>();
        let accounts = self
            .list_accounts()
            .await?
            .into_iter()
            .map(|account| (account.id, account.email))
            .collect::<std::collections::HashMap<_, _>>();
        let mut folders = std::collections::HashMap::new();
        for account_id in accounts.keys() {
            for folder in self.list_folders(*account_id).await? {
                folders.insert(folder.id, folder);
            }
        }
        let label_rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT ml.message_id, l.name FROM message_labels ml JOIN labels l ON l.id=ml.label_id",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut labels = std::collections::HashMap::<i64, Vec<String>>::new();
        for (message_id, name) in label_rows {
            labels.entry(message_id).or_default().push(name);
        }
        const SCAN_PAGE_SIZE: i64 = 1_000;
        let page_size = limit.clamp(1, 500);
        let mut cursor = before_date
            .zip(before_id)
            .map(|(date, id)| (date.to_owned(), id));
        let mut result = Vec::new();
        loop {
            let rows = if let Some((date, id)) = &cursor {
                sqlx::query_as::<_, MessageRow>(
                    "SELECT id, account_id, folder_id, thread_id, uid, rfc822_message_id,
                            from_name, from_addr, to_addrs, cc_addrs, subject, preview, date, size,
                            seen, flagged, answered, draft, has_attachments, dkim_pass, spf_pass, dmarc_pass
                     FROM messages
                     WHERE (COALESCE(date, '') < ? OR (COALESCE(date, '') = ? AND id < ?))
                       AND (snoozed_until IS NULL OR snoozed_until <= datetime('now'))
                       AND NOT EXISTS (
                         SELECT 1 FROM outbox_ops o WHERE o.message_id=messages.id
                           AND o.op_kind IN ('move','delete') AND o.status IN ('pending','processing','retry')
                       )
                     ORDER BY COALESCE(date, '') DESC, id DESC LIMIT ?",
                )
                .bind(date)
                .bind(date)
                .bind(id)
                .bind(SCAN_PAGE_SIZE)
                .fetch_all(&self.pool)
                .await?
            } else {
                sqlx::query_as::<_, MessageRow>(
                    "SELECT id, account_id, folder_id, thread_id, uid, rfc822_message_id,
                            from_name, from_addr, to_addrs, cc_addrs, subject, preview, date, size,
                            seen, flagged, answered, draft, has_attachments, dkim_pass, spf_pass, dmarc_pass
                     FROM messages
                     WHERE (snoozed_until IS NULL OR snoozed_until <= datetime('now'))
                       AND NOT EXISTS (
                         SELECT 1 FROM outbox_ops o WHERE o.message_id=messages.id
                           AND o.op_kind IN ('move','delete') AND o.status IN ('pending','processing','retry')
                       )
                     ORDER BY COALESCE(date, '') DESC, id DESC LIMIT ?",
                )
                .bind(SCAN_PAGE_SIZE)
                .fetch_all(&self.pool)
                .await?
            };
            let row_count = rows.len();
            let Some(last) = rows.last() else {
                break;
            };
            let next_cursor = (last.date.clone().unwrap_or_default(), last.id);
            for row in rows {
                let mut message = MessageMeta::from(row);
                if !included.contains(&message.folder_id) {
                    continue;
                }
                message.labels = labels.remove(&message.id).unwrap_or_default();
                if smart_folder_matches(
                    &folder,
                    &message,
                    accounts.get(&message.account_id).map(String::as_str),
                    folders.get(&message.folder_id),
                ) {
                    result.push(message);
                    if result.len() >= page_size {
                        return Ok(result);
                    }
                }
            }
            if row_count < SCAN_PAGE_SIZE as usize {
                break;
            }
            cursor = Some(next_cursor);
        }
        Ok(result)
    }

    // ---------- Контакты ----------

    pub async fn save_local_contact(
        &self,
        account_id: i64,
        contact_id: Option<i64>,
        input: &crate::account::ContactInput,
    ) -> Result<i64> {
        let mut tx = self.begin_write().await?;
        let id = if let Some(contact_id) = contact_id {
            sqlx::query(
                "UPDATE contacts SET display_name=?, first_name=?, last_name=?, organization=?, hidden=0
                 WHERE id=? AND account_id=?",
            )
            .bind(clean_contact_name(&input.display_name))
            .bind(&input.first_name)
            .bind(&input.last_name)
            .bind(&input.organization)
            .bind(contact_id)
            .bind(account_id)
            .execute(&mut *tx)
            .await?;
            contact_id
        } else {
            let result = sqlx::query(
                "INSERT INTO contacts(account_id, uid, display_name, first_name, last_name, organization)
                 VALUES(?, ?, ?, ?, ?, ?)",
            )
            .bind(account_id)
            .bind(format!("local:{}", uuid::Uuid::new_v4()))
            .bind(clean_contact_name(&input.display_name))
            .bind(&input.first_name)
            .bind(&input.last_name)
            .bind(&input.organization)
            .execute(&mut *tx)
            .await?;
            result.last_insert_rowid()
        };
        sqlx::query("DELETE FROM contact_emails WHERE contact_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        for email in &input.emails {
            sqlx::query(
                "INSERT OR IGNORE INTO contact_emails(contact_id, email, kind) VALUES(?, ?, 'other')",
            )
            .bind(id)
            .bind(email.trim())
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query("DELETE FROM contact_phones WHERE contact_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        for phone in &input.phones {
            sqlx::query(
                "INSERT INTO contact_phones(contact_id, number, kind, extension) VALUES(?, ?, ?, ?)",
            )
            .bind(id)
            .bind(phone.number.trim())
            .bind(&phone.kind)
            .bind(phone.extension.as_deref().map(str::trim))
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(id)
    }

    pub async fn hide_local_contact(&self, contact_id: i64) -> Result<()> {
        sqlx::query("UPDATE contacts SET hidden=1 WHERE id=?")
            .bind(contact_id)
            .execute(&self.write_pool)
            .await?;
        Ok(())
    }

    /// Все контакты (по умолчанию) или подходящие под подстроку поиска.
    ///
    /// Раньше здесь стоял LIMIT 500, и адресная книга просто обрывалась на
    /// пятисотом контакте по алфавиту - остальных в программе не было видно
    /// вообще. Лимит был вынужденным: почта и телефоны читались отдельными
    /// запросами на каждый контакт, то есть 1 + 2N обращений к зашифрованной
    /// базе, и на большой книге это заметно подвисало. Теперь всё читается
    /// тремя запросами независимо от числа контактов, и ограничивать выдачу
    /// больше незачем.
    pub async fn list_contacts(&self, query: Option<&str>) -> Result<Vec<Contact>> {
        use std::collections::HashMap;

        let like = format!("%{}%", query.unwrap_or(""));
        let rows = sqlx::query_as::<_, ContactRow>(
            "SELECT id, account_id, uid, display_name, first_name, last_name, organization, is_favorite, remote_url
             FROM contacts WHERE hidden=0 AND display_name LIKE ? ORDER BY display_name",
        )
        .bind(&like)
        .fetch_all(&self.pool)
        .await?;
        let mut contacts: Vec<Contact> = rows.into_iter().map(Into::into).collect();
        // Позиция контакта по его id: раскладывать почты и телефоны по владельцам
        // приходится в памяти, зато вместо запроса на каждый контакт остаётся
        // один на всю таблицу.
        let positions: HashMap<i64, usize> = contacts
            .iter()
            .enumerate()
            .filter_map(|(index, contact)| contact.id.map(|id| (id, index)))
            .collect();
        if positions.is_empty() {
            return Ok(contacts);
        }
        // Условие по hidden повторяет выборку выше: без него в выдачу попали бы
        // адреса скрытых контактов, а с JOIN по contacts это дешевле, чем
        // перечислять сотни id в IN (...).
        let email_rows = sqlx::query_as::<_, ContactEmailRow>(
            "SELECT e.contact_id, e.email, e.kind
             FROM contact_emails e JOIN contacts c ON c.id = e.contact_id
             WHERE c.hidden=0 AND c.display_name LIKE ?
             ORDER BY e.id",
        )
        .bind(&like)
        .fetch_all(&self.pool)
        .await?;
        for row in email_rows {
            if let Some(index) = positions.get(&row.contact_id) {
                contacts[*index].emails.push(ContactEmail {
                    email: row.email,
                    kind: row.kind,
                });
            }
        }
        let phone_rows = sqlx::query_as::<_, ContactPhoneRow>(
            "SELECT p.contact_id, p.number, p.kind, p.extension
             FROM contact_phones p JOIN contacts c ON c.id = p.contact_id
             WHERE c.hidden=0 AND c.display_name LIKE ?
             ORDER BY p.id",
        )
        .bind(&like)
        .fetch_all(&self.pool)
        .await?;
        for row in phone_rows {
            if let Some(index) = positions.get(&row.contact_id) {
                contacts[*index].phones.push(ContactPhone {
                    number: row.number,
                    kind: row.kind,
                    extension: row.extension,
                });
            }
        }
        Ok(contacts)
    }
}

fn smart_folder_matches(
    folder: &SmartFolder,
    message: &MessageMeta,
    account_email: Option<&str>,
    source_folder: Option<&Folder>,
) -> bool {
    folder.groups.iter().any(|group| {
        let matches = |condition: &SmartCondition| {
            smart_condition_matches(condition, message, account_email, source_folder)
        };
        !group.conditions.is_empty()
            && if group.logic == "any" {
                group.conditions.iter().any(matches)
            } else {
                group.conditions.iter().all(matches)
            }
    })
}

fn smart_condition_matches(
    condition: &SmartCondition,
    message: &MessageMeta,
    account_email: Option<&str>,
    source_folder: Option<&Folder>,
) -> bool {
    if condition.field == "date" {
        let Some(raw) = message.date.as_deref() else {
            return false;
        };
        let timestamp = chrono::DateTime::parse_from_rfc3339(raw)
            .map(|value| value.with_timezone(&chrono::Utc))
            .or_else(|_| {
                chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
                    .map(|value| value.and_utc())
            });
        let Ok(timestamp) = timestamp else {
            return false;
        };
        if matches!(condition.op.as_str(), "within_last" | "older_than") {
            let Ok(amount) = condition.value.parse::<i64>() else {
                return false;
            };
            let seconds = match condition.unit.as_deref().unwrap_or("hours") {
                "minutes" => 60,
                "days" => 86_400,
                "weeks" => 604_800,
                _ => 3_600,
            };
            let threshold = chrono::Utc::now() - chrono::Duration::seconds(amount * seconds);
            return if condition.op == "within_last" {
                timestamp >= threshold
            } else {
                timestamp < threshold
            };
        }
        let Ok(target) = chrono::NaiveDate::parse_from_str(&condition.value, "%Y-%m-%d") else {
            return false;
        };
        let actual = timestamp.date_naive();
        return match condition.op.as_str() {
            "before" => actual < target,
            "after" => actual > target,
            _ => actual == target,
        };
    }

    if condition.field == "size" {
        let Some(bytes) = message.size else {
            return false;
        };
        let factor = match condition.unit.as_deref().unwrap_or("mb") {
            "kb" => 1_024_f64,
            "gb" => 1_073_741_824_f64,
            _ => 1_048_576_f64,
        };
        let Ok(value) = condition.value.parse::<f64>() else {
            return false;
        };
        let minimum = value * factor;
        let bytes = bytes as f64;
        return match condition.op.as_str() {
            "greater_than" => bytes > minimum,
            "greater_or_equal" => bytes >= minimum,
            "less_than" => bytes < minimum,
            "less_or_equal" => bytes <= minimum,
            "between" => condition
                .value2
                .as_deref()
                .and_then(|value| value.parse::<f64>().ok())
                .is_some_and(|maximum| bytes >= minimum && bytes <= maximum * factor),
            _ => (bytes - minimum).abs() < f64::EPSILON,
        };
    }

    let value = match condition.field.as_str() {
        "sender" => format!(
            "{} {}",
            message.from.name.as_deref().unwrap_or(""),
            message.from.email
        ),
        "recipient" => message
            .to
            .iter()
            .chain(message.cc.iter())
            .map(|address| {
                format!(
                    "{} {}",
                    address.name.as_deref().unwrap_or(""),
                    address.email
                )
            })
            .collect::<Vec<_>>()
            .join(" "),
        "subject" => message.subject.clone(),
        "body" => message.preview.clone(),
        "account" => account_email.unwrap_or_default().to_owned(),
        "folder" => source_folder
            .map(|folder| format!("{} {}", folder.display_name, folder.remote_path))
            .unwrap_or_default(),
        "folder_role" => source_folder
            .and_then(|folder| folder.role)
            .map(|role| role.as_str().to_owned())
            .unwrap_or_else(|| "other".into()),
        "read_state" => if message.flags.seen { "read" } else { "unread" }.into(),
        "importance" => if message.flags.flagged {
            "flagged"
        } else {
            "normal"
        }
        .into(),
        "reply_state" => if message.flags.answered {
            "answered"
        } else {
            "unanswered"
        }
        .into(),
        "draft_state" => if message.flags.draft {
            "draft"
        } else {
            "not_draft"
        }
        .into(),
        "attachment" => if message.has_attachments {
            "has"
        } else {
            "none"
        }
        .into(),
        "label" => message.labels.join(" "),
        _ => String::new(),
    };
    let left = value.to_lowercase();
    let right = condition.value.to_lowercase();
    match condition.op.as_str() {
        "not_contains" => !left.contains(&right),
        "equals" => left == right,
        "not_equals" => left != right,
        "starts_with" => left.starts_with(&right),
        "ends_with" => left.ends_with(&right),
        _ => left.contains(&right),
    }
}

// ---- Промежуточные строки sqlx (FromRow) ----

#[derive(sqlx::FromRow)]
struct OutboxRow {
    id: i64,
    account_id: i64,
    message_id: Option<i64>,
    op_kind: String,
    payload: String,
    attempts: i64,
}

impl From<OutboxRow> for OutboxOperation {
    fn from(row: OutboxRow) -> Self {
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

#[derive(sqlx::FromRow)]
struct AccountRow {
    id: i64,
    uuid: String,
    email: String,
    display_name: String,
    provider: String,
    backend_kind: String,
    auth_kind: String,
    imap_host: Option<String>,
    imap_port: Option<i64>,
    imap_security: Option<String>,
    smtp_host: Option<String>,
    smtp_port: Option<i64>,
    smtp_security: Option<String>,
    ews_url: Option<String>,
    jmap_url: Option<String>,
    caldav_url: Option<String>,
    carddav_url: Option<String>,
    username: Option<String>,
    secret_ref: Option<String>,
    include_in_unified: i64,
    color: Option<String>,
    retention_days: i64,
    enabled: i64,
}

impl From<AccountRow> for Account {
    fn from(r: AccountRow) -> Self {
        let sec = |s: Option<String>| match s.as_deref() {
            Some("starttls") => Security::Starttls,
            Some("none") => Security::None,
            _ => Security::Ssl,
        };
        let imap = match (r.imap_host, r.imap_port) {
            (Some(h), Some(p)) => Some(ServerConfig {
                host: h,
                port: p as u16,
                security: sec(r.imap_security),
            }),
            _ => None,
        };
        let smtp = match (r.smtp_host, r.smtp_port) {
            (Some(h), Some(p)) => Some(ServerConfig {
                host: h,
                port: p as u16,
                security: sec(r.smtp_security),
            }),
            _ => None,
        };
        Account {
            id: r.id,
            uuid: r.uuid,
            email: r.email,
            display_name: r.display_name,
            provider: parse_provider(&r.provider),
            backend_kind: match r.backend_kind.as_str() {
                "ews" => BackendKind::Ews,
                "jmap" => BackendKind::Jmap,
                _ => BackendKind::Imap,
            },
            auth_kind: match r.auth_kind.as_str() {
                "oauth2" => AuthKind::Oauth2,
                "ntlm" => AuthKind::Ntlm,
                "password" => AuthKind::Password,
                _ => AuthKind::AppPassword,
            },
            imap,
            smtp,
            ews_url: r.ews_url,
            jmap_url: r.jmap_url,
            caldav_url: r.caldav_url,
            carddav_url: r.carddav_url,
            username: r.username,
            secret_ref: r.secret_ref,
            include_in_unified: r.include_in_unified != 0,
            color: r.color,
            retention_days: r.retention_days,
            enabled: r.enabled != 0,
        }
    }
}

fn parse_provider(s: &str) -> Provider {
    match s {
        "yandex" => Provider::Yandex,
        "mailru" => Provider::Mailru,
        "icloud" => Provider::Icloud,
        "exchange" => Provider::Exchange,
        "gmail" => Provider::Gmail,
        "outlook" => Provider::Outlook,
        _ => Provider::Generic,
    }
}

#[derive(sqlx::FromRow)]
struct FolderRow {
    id: i64,
    account_id: i64,
    remote_path: String,
    display_name: String,
    role: Option<String>,
    unread_count: i64,
    total_count: i64,
}
impl From<FolderRow> for Folder {
    fn from(r: FolderRow) -> Self {
        let role = match r.role.as_deref() {
            Some("inbox") => Some(FolderRole::Inbox),
            Some("sent") => Some(FolderRole::Sent),
            Some("drafts") => Some(FolderRole::Drafts),
            Some("spam") => Some(FolderRole::Spam),
            Some("trash") => Some(FolderRole::Trash),
            Some("archive") => Some(FolderRole::Archive),
            _ => None,
        };
        Folder {
            id: r.id,
            account_id: r.account_id,
            remote_path: r.remote_path,
            display_name: r.display_name,
            role,
            unread_count: r.unread_count,
            total_count: r.total_count,
        }
    }
}

#[derive(sqlx::FromRow)]
struct MessageRow {
    id: i64,
    account_id: i64,
    folder_id: i64,
    thread_id: Option<i64>,
    uid: i64,
    rfc822_message_id: Option<String>,
    from_name: Option<String>,
    from_addr: Option<String>,
    to_addrs: Option<String>,
    cc_addrs: Option<String>,
    subject: String,
    preview: String,
    date: Option<String>,
    size: Option<i64>,
    seen: i64,
    flagged: i64,
    answered: i64,
    draft: i64,
    has_attachments: i64,
    dkim_pass: Option<i64>,
    spf_pass: Option<i64>,
    dmarc_pass: Option<i64>,
}
impl From<MessageRow> for MessageMeta {
    fn from(r: MessageRow) -> Self {
        let parse_addrs = |s: Option<String>| -> Vec<Addr> {
            s.and_then(|v| serde_json::from_str(&v).ok())
                .unwrap_or_default()
        };
        MessageMeta {
            id: r.id,
            account_id: r.account_id,
            folder_id: r.folder_id,
            thread_id: r.thread_id,
            uid: r.uid as u32,
            message_id: r.rfc822_message_id,
            from: Addr {
                name: r.from_name,
                email: r.from_addr.unwrap_or_default(),
            },
            to: parse_addrs(r.to_addrs),
            cc: parse_addrs(r.cc_addrs),
            subject: r.subject,
            preview: r.preview,
            date: r.date,
            size: r.size,
            flags: Flags {
                seen: r.seen != 0,
                flagged: r.flagged != 0,
                answered: r.answered != 0,
                draft: r.draft != 0,
            },
            has_attachments: r.has_attachments != 0,
            auth: AuthResults {
                spf: r.spf_pass.map(|v| v != 0),
                dkim: r.dkim_pass.map(|v| v != 0),
                dmarc: r.dmarc_pass.map(|v| v != 0),
            },
            labels: Vec::new(),
        }
    }
}

#[derive(sqlx::FromRow)]
struct MailRuleRow {
    id: String,
    name: String,
    field: String,
    operator: String,
    value: String,
    account_id: Option<i64>,
    action: String,
    folder_id: Option<i64>,
    enabled: i64,
    progress_message_id: i64,
    sort_order: i64,
}

impl From<MailRuleRow> for MailRule {
    fn from(rule: MailRuleRow) -> Self {
        Self {
            id: rule.id,
            name: rule.name,
            field: rule.field,
            operator: rule.operator,
            value: rule.value,
            account_id: rule.account_id,
            action: rule.action,
            folder_id: rule.folder_id,
            enabled: rule.enabled != 0,
            progress_message_id: rule.progress_message_id,
            sort_order: rule.sort_order,
        }
    }
}

#[derive(sqlx::FromRow)]
struct RuleMessageRow {
    id: i64,
    account_id: i64,
    folder_id: i64,
    uid: i64,
    remote_path: String,
    remote_id: Option<String>,
    from_name: Option<String>,
    from_addr: Option<String>,
    subject: String,
}

#[derive(sqlx::FromRow)]
struct SmartRow {
    id: i64,
    stable_id: String,
    name: String,
    icon: Option<String>,
    is_builtin: i64,
    enabled: i64,
    sort_order: i64,
}
#[derive(sqlx::FromRow)]
struct CondRow {
    field: String,
    op: String,
    value: String,
    group_index: i64,
    group_logic: String,
    unit: Option<String>,
    value2: Option<String>,
}

#[derive(sqlx::FromRow)]
struct ContactRow {
    id: i64,
    account_id: Option<i64>,
    uid: Option<String>,
    display_name: String,
    first_name: Option<String>,
    last_name: Option<String>,
    organization: Option<String>,
    is_favorite: i64,
    remote_url: Option<String>,
}

/// Почта контакта вместе с владельцем: адреса всей книги читаются одним
/// запросом и раскладываются по контактам в памяти (см. list_contacts).
#[derive(sqlx::FromRow)]
struct ContactEmailRow {
    contact_id: i64,
    email: String,
    kind: Option<String>,
}

#[derive(sqlx::FromRow)]
struct ContactPhoneRow {
    contact_id: i64,
    number: String,
    kind: Option<String>,
    extension: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CalendarSummary {
    pub id: i64,
    pub account_id: i64,
    pub name: String,
    pub color: Option<String>,
    pub visible: bool,
    pub read_only: bool,
}

#[derive(sqlx::FromRow)]
struct EventRow {
    id: i64,
    calendar_id: i64,
    uid: Option<String>,
    summary: String,
    description: Option<String>,
    location: Option<String>,
    dtstart: String,
    dtend: Option<String>,
    all_day: i64,
    rrule: Option<String>,
    recurrence_id: Option<String>,
    exdates: Option<String>,
    rdates: Option<String>,
    timezone: Option<String>,
    status: Option<String>,
    transp: Option<String>,
    class: Option<String>,
    categories: Option<String>,
    url: Option<String>,
    organizer: Option<String>,
    sequence: i64,
}

#[derive(sqlx::FromRow)]
struct EventAttendeeRow {
    event_id: i64,
    email: String,
    name: Option<String>,
    role: Option<String>,
    partstat: Option<String>,
    rsvp: i64,
}

#[derive(sqlx::FromRow)]
struct EventAlarmRow {
    event_id: i64,
    trigger_minutes: i32,
    action: String,
}
impl From<EventRow> for Event {
    fn from(row: EventRow) -> Self {
        Event {
            id: Some(row.id),
            calendar_id: row.calendar_id,
            uid: row.uid,
            summary: row.summary,
            description: row.description,
            location: row.location,
            dtstart: row.dtstart,
            dtend: row.dtend,
            all_day: row.all_day != 0,
            attendees: Vec::new(),
            alarms: Vec::new(),
            rrule: row.rrule,
            recurrence_id: row.recurrence_id,
            exdates: row.exdates,
            rdates: row.rdates,
            timezone: row.timezone,
            status: row.status.as_deref().and_then(EventStatus::parse),
            transp: match row.transp.as_deref() {
                Some("TRANSPARENT") => Some(Transp::Transparent),
                Some(_) => Some(Transp::Opaque),
                None => None,
            },
            class: match row.class.as_deref() {
                Some("PRIVATE") => Some(EventClass::Private),
                Some("CONFIDENTIAL") => Some(EventClass::Confidential),
                Some(_) => Some(EventClass::Public),
                None => None,
            },
            categories: row
                .categories
                .unwrap_or_default()
                .split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
            url: row.url,
            organizer: row.organizer,
            sequence: row.sequence,
            // Заполняется отдельно, уже с участниками - см.
            // list_calendars_and_events/event_for_response ниже.
            my_partstat: None,
            needs_response: false,
        }
    }
}
impl From<ContactRow> for Contact {
    fn from(r: ContactRow) -> Self {
        Contact {
            id: Some(r.id),
            account_id: r.account_id,
            uid: r.uid,
            display_name: clean_contact_name(&r.display_name),
            first_name: r.first_name,
            last_name: r.last_name,
            organization: r.organization,
            emails: Vec::new(),
            phones: Vec::new(),
            is_favorite: r.is_favorite != 0,
            is_local_only: r.remote_url.is_none(),
        }
    }
}

#[cfg(test)]
mod notification_lookup_tests {
    use super::*;
    use crate::crypto::{DatabaseKey, StorageCrypto};
    use rand::Rng as _;
    use std::sync::Arc;

    fn random_key() -> [u8; 32] {
        let mut key = [0_u8; 32];
        rand::rng().fill_bytes(&mut key);
        key
    }

    /// Тестовое хранилище с применёнными миграциями - тот же паттерн, что в
    /// storage::tests (storage/mod.rs).
    async fn test_db() -> Db {
        let root =
            std::env::temp_dir().join(format!("truemail-repo-notify-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create temp data dir");
        let crypto = Arc::new(StorageCrypto::from_key(random_key()));
        let database_key = DatabaseKey::from_key(random_key());
        let db = Db::open_with_database_key(&root, crypto, &database_key)
            .await
            .expect("open database");
        db.migrate().await.expect("migrate database");
        db
    }

    /// Заводит аккаунт, возвращает его id.
    async fn seed_account(db: &Db) -> i64 {
        let (account_id,): (i64,) = sqlx::query_as(
            "INSERT INTO accounts(uuid, email, provider, backend_kind, auth_kind) \
                 VALUES (?, ?, 'generic', 'imap', 'password') RETURNING id",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(format!("{}@example.test", uuid::Uuid::new_v4()))
        .fetch_one(&db.write_pool)
        .await
        .expect("insert account");
        account_id
    }

    /// Заводит папку заданной роли в аккаунте, возвращает id папки.
    async fn seed_folder(db: &Db, account_id: i64, remote_path: &str, role: Option<&str>) -> i64 {
        let (folder_id,): (i64,) = sqlx::query_as(
            "INSERT INTO folders(account_id, remote_path, display_name, role) \
                 VALUES (?, ?, ?, ?) RETURNING id",
        )
        .bind(account_id)
        .bind(remote_path)
        .bind(remote_path)
        .bind(role)
        .fetch_one(&db.write_pool)
        .await
        .expect("insert folder");
        folder_id
    }

    /// Вставляет письмо с заданным remote_id в указанную папку, возвращает id письма.
    async fn seed_message(
        db: &Db,
        account_id: i64,
        folder_id: i64,
        uid: i64,
        remote_id: &str,
    ) -> i64 {
        let (message_id,): (i64,) = sqlx::query_as(
            "INSERT INTO messages(account_id, folder_id, uid, remote_id, subject, date) \
                 VALUES (?, ?, ?, ?, 'тест', datetime('now')) RETURNING id",
        )
        .bind(account_id)
        .bind(folder_id)
        .bind(uid)
        .bind(remote_id)
        .fetch_one(&db.write_pool)
        .await
        .expect("insert message");
        message_id
    }

    #[tokio::test]
    async fn returns_ids_for_inbox_messages_only() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let inbox_id = seed_folder(&db, account_id, "INBOX", Some("inbox")).await;
        let archive_id = seed_folder(&db, account_id, "Archive", Some("archive")).await;

        let inbox_message_id = seed_message(&db, account_id, inbox_id, 1, "remote-inbox").await;
        seed_message(&db, account_id, archive_id, 2, "remote-archive").await;

        let ids = db
            .inbox_message_ids_by_remote_ids(
                account_id,
                &["remote-inbox".to_owned(), "remote-archive".to_owned()],
            )
            .await
            .expect("query inbox ids");

        assert_eq!(
            ids,
            vec![inbox_message_id],
            "письмо из архива не должно попасть в выборку"
        );
    }

    #[tokio::test]
    async fn empty_input_returns_empty_result_without_querying() {
        let db = test_db().await;
        let ids = db
            .inbox_message_ids_by_remote_ids(1, &[])
            .await
            .expect("query with empty input");
        assert!(ids.is_empty());
    }

    /// Читает payload единственной pending-операции 'flag' письма из outbox.
    async fn pending_flag_payload(db: &Db, message_id: i64) -> serde_json::Value {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT payload FROM outbox_ops WHERE message_id=? AND op_kind='flag' AND status IN ('pending','retry')",
        )
        .bind(message_id)
        .fetch_all(&db.write_pool)
        .await
        .expect("query outbox_ops");
        assert_eq!(
            rows.len(),
            1,
            "должна остаться ровно одна pending 'flag'-операция"
        );
        serde_json::from_str(&rows[0].0).expect("payload must be valid JSON")
    }

    /// seen и flagged синхронизируются независимо: смена одного не должна
    /// затирать ещё не отправленное значение другого в outbox-payload -
    /// раньше payload нёс только изменённое поле, и mark_flagged вообще не
    /// существовал (звёздочка не уходила на сервер).
    #[tokio::test]
    async fn mark_seen_and_mark_flagged_do_not_clobber_each_other_in_outbox_payload() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let folder_id = seed_folder(&db, account_id, "INBOX", Some("inbox")).await;
        let message_id = seed_message(&db, account_id, folder_id, 1, "remote-1").await;

        db.mark_seen(message_id, true).await.expect("mark_seen");
        let payload = pending_flag_payload(&db, message_id).await;
        assert_eq!(payload["seen"], serde_json::json!(true));
        assert_eq!(payload["flagged"], serde_json::json!(false));

        db.mark_flagged(message_id, true)
            .await
            .expect("mark_flagged");
        let payload = pending_flag_payload(&db, message_id).await;
        assert_eq!(
            payload["seen"],
            serde_json::json!(true),
            "flagged не должен сбрасывать ранее выставленный seen"
        );
        assert_eq!(payload["flagged"], serde_json::json!(true));

        db.mark_seen(message_id, false).await.expect("mark_seen");
        let payload = pending_flag_payload(&db, message_id).await;
        assert_eq!(payload["seen"], serde_json::json!(false));
        assert_eq!(
            payload["flagged"],
            serde_json::json!(true),
            "seen не должен сбрасывать ранее выставленный flagged"
        );

        let (seen, flagged): (i64, i64) =
            sqlx::query_as("SELECT seen, flagged FROM messages WHERE id=?")
                .bind(message_id)
                .fetch_one(&db.write_pool)
                .await
                .expect("query message flags");
        assert_eq!(seen, 0);
        assert_eq!(flagged, 1);
    }
}
