//! Репозитории: типобезопасные запросы к таблицам.

use super::Db;
use super::encoded_words;
use crate::Result;
use crate::model::*;
use futures::TryStreamExt;
use sqlx::AssertSqlSafe;

/// Состояния операции признака важности, при которых значение сервера к письму
/// не применяется. Состояние отказа входит сюда наравне с остальными: после
/// восьми попыток операция повтору не подлежит и лежит в очереди до решения
/// пользователя, а значение сервера тем временем вернуло бы прежний признак и
/// воскресило бы выполненное дело (flag-due-dates.md, S-039 и S-040).
/// Макрос, а не константа: строки запросов собираются на компиляции
/// через `concat!`.
macro_rules! unfinished_flag_op_sql {
    () => {
        "('pending','processing','retry','failed')"
    };
}

/// Операция вида `flag` несёт оба признака сразу и ставится в том числе
/// отметкой о прочтении. Придержать значение сервера должна только та, что
/// действительно меняет важность: иначе неотправленная отметка о прочтении
/// молча запрещала бы принимать важность с сервера. Записи прежних выпусков
/// поля не имеют и считаются меняющими важность, как было до правки.
macro_rules! flag_op_changes_flag_sql {
    ($alias:literal) => {
        concat!(
            " AND COALESCE(json_extract(",
            $alias,
            "payload,'$.sets_flagged'),1)=1"
        )
    };
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct QueuedAction {
    pub operation_ids: Vec<i64>,
    /// Письма, пропущенные из-за уже стоящей по ним операции увода (S-005).
    pub skipped: usize,
    /// Из них письма, которые прямо сейчас переносятся на сервере (S-006).
    pub skipped_busy: usize,
    /// Из них письма с прошлой операцией в состоянии отказа: по ним нужно
    /// решение пользователя - повторить или отказаться (S-052, S-053).
    pub skipped_failed: usize,
    /// Из них письма, для которых не нашлось единственной папки нужной роли
    /// (S-046).
    pub skipped_no_folder: usize,
    /// Закрепление или незавершенное дело нельзя гарантированно вернуть после
    /// переноса, потому что у письма нет единственного Message-ID.
    pub traits_at_risk: usize,
}

impl QueuedAction {
    /// Учесть исход постановки по одному письму. Ни один исход не прерывает
    /// цикл: одно занятое письмо не отменяет действие над остальными (S-005).
    fn account(&mut self, outcome: TakeawayOutcome) {
        match outcome {
            TakeawayOutcome::Queued(id) => self.operation_ids.push(id),
            TakeawayOutcome::Conflict => self.skipped += 1,
            TakeawayOutcome::Busy => {
                self.skipped += 1;
                self.skipped_busy += 1;
            }
            TakeawayOutcome::Failed => {
                self.skipped += 1;
                self.skipped_failed += 1;
            }
            TakeawayOutcome::NeedsAttention => {
                self.skipped += 1;
                self.skipped_no_folder += 1;
            }
            // Письмо уже лежит в папке назначения: делать нечего и жаловаться
            // не на что (S-012).
            TakeawayOutcome::Unchanged => {}
        }
    }
}

/// Операция увода, дошедшая до состояния отказа (S-052).
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct FailedOperation {
    pub id: i64,
    pub message_id: Option<i64>,
    pub op_kind: String,
    pub last_error: Option<String>,
}

/// Результат сохранения календарей/контактов провайдера: счётчики плюс
/// смысловые изменения встреч (для будущих уведомлений - показ отдельным
/// этапом, здесь только вычисляем дельту).
/// Состояние папки для слепка синхронизации: по нему решается, менялось ли что-то
/// в дереве папок и счётчиках писем.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct FolderState {
    pub remote_path: String,
    pub display_name: Option<String>,
    pub role: Option<String>,
    pub parent_id: Option<i64>,
    pub unread_count: i64,
    pub total_count: i64,
}

/// Итог одного прохода синхронизации почты аккаунта - то, что попадает в
/// постоянное состояние (mail-sync-visible-state.md). Строится по фактическому
/// Ok/Err вызова прохода (sync_mail_account, sync_mail_inbox/_delta), а не по
/// тексту опубликованного события truemail-sync-state: они могут расходиться
/// при частичном исходе цикла sync_accounts (S-013).
#[derive(Debug, Clone)]
pub enum MailSyncOutcome {
    Success,
    Failure {
        /// Безопасный текст ошибки (секреты уже вырезаны).
        message: String,
        /// Устойчивый вид ошибки, error-kinds-and-messages.md.
        kind: String,
        /// Вид ошибки входит в список требующих повторного входа (S-016 там).
        needs_reauth: bool,
    },
}

impl MailSyncOutcome {
    /// Строит исход по результату прохода, каким бы ни было содержимое Ok.
    pub fn from_result<T>(result: &std::result::Result<T, crate::Error>) -> Self {
        match result {
            Ok(_) => Self::Success,
            Err(error) => Self::Failure {
                message: crate::error::sanitize_error_message(&error.to_string()),
                kind: error.code().to_owned(),
                needs_reauth: error.requires_reauth(),
            },
        }
    }
}

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

/// Заголовки правил молчания одного письма: их читает стадия автоответа
/// (specs/out-of-office.md, S-063).
struct SilenceHeaders {
    is_newsletter: bool,
    auto_submitted: Option<String>,
    precedence: Option<String>,
    return_path_empty: bool,
    auto_response_suppress: Option<String>,
    reply_to_json: String,
    known: bool,
}

#[derive(Debug, Clone)]
pub struct OutboxOperation {
    pub id: i64,
    pub account_id: i64,
    pub message_id: Option<i64>,
    pub op_kind: String,
    pub payload: String,
    pub attempts: i64,
}

/// Строка исчезнувшего с сервера письма: номер, ссылка на хранилище больших
/// объектов, заголовок Message-ID и время закрепления. Нужна отдельным именем,
/// потому что кортеж из четырёх значений в теле функции нечитаем.
type VanishedMessageRow = (i64, Option<String>);

/// Закрепление и сроки дела, дочитываемые к странице списка писем.
type MessageDetailsRow = (
    i64,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

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
        // Пустая строка снимает клавишу с действия и доходит сюда с настоящего
        // пути снятия. Разбирать её нечем, а отказ означал бы, что назначенное
        // сочетание освободить нечем вовсе (issue #104).
        let combo = if combo.trim().is_empty() {
            String::new()
        } else {
            normalize_key_combo(combo)
                .ok_or_else(|| crate::Error::Other("неверное сочетание клавиш".into()))?
        };
        let result = if is_quick_step_key_action(action) {
            sqlx::query(
                "INSERT INTO keybindings(action,scope,combo) VALUES(?,'local',?)
                 ON CONFLICT(action) DO UPDATE SET combo=excluded.combo",
            )
            .bind(action)
            .bind(&combo)
            .execute(&self.write_pool)
            .await?
        } else {
            sqlx::query("UPDATE keybindings SET combo=? WHERE action=?")
                .bind(&combo)
                .bind(action)
                .execute(&self.write_pool)
                .await?
        };
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
        // undo-send.md S-007: тело и вложения ожидающего письма лежат в том же
        // хранилище, а ссылки на них - в данных операции очереди. Без этого
        // первый же запуск стёр бы тело, и письмо ушло бы пустым.
        referenced.extend(self.outgoing_blob_references().await?);
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
                ews_url, jmap_url, caldav_url, carddav_url, username, secret_ref, include_in_unified, color, retention_days, enabled,
                last_sync_at, last_sync_error, last_sync_error_kind, needs_reauth
             FROM accounts WHERE enabled = 1 ORDER BY sort_order, id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Записывает исход прохода синхронизации почты аккаунта
    /// (mail-sync-visible-state.md, S-002 - S-005). Успех обновляет
    /// last_sync_at и снимает сохранённую ошибку и признак "нужен повторный
    /// вход"; неудача сохраняет текст, вид ошибки и признак, не трогая
    /// last_sync_at (S-003, S-004) - время последнего успеха должно
    /// оставаться прежним. Ошибка самой этой записи (например, база временно
    /// заблокирована) не должна прерывать проход синхронизации - вызывающая
    /// сторона теряет её без повтора (см. mail-sync-visible-state.md,
    /// "Ошибки и частичные отказы").
    pub async fn record_mail_sync_outcome(
        &self,
        account_id: i64,
        outcome: &MailSyncOutcome,
    ) -> Result<()> {
        match outcome {
            MailSyncOutcome::Success => {
                sqlx::query(
                    "UPDATE accounts SET last_sync_at = datetime('now'), last_sync_error = NULL,
                            last_sync_error_kind = NULL, needs_reauth = 0
                     WHERE id = ?",
                )
                .bind(account_id)
                .execute(&self.write_pool)
                .await?;
            }
            MailSyncOutcome::Failure {
                message,
                kind,
                needs_reauth,
            } => {
                sqlx::query(
                    "UPDATE accounts SET last_sync_error = ?, last_sync_error_kind = ?, needs_reauth = ?
                     WHERE id = ?",
                )
                .bind(message)
                .bind(kind)
                .bind(*needs_reauth as i64)
                .bind(account_id)
                .execute(&self.write_pool)
                .await?;
            }
        }
        Ok(())
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
             AND m.pinned_at IS NULL \
             AND NOT EXISTS(SELECT 1 FROM message_tasks t WHERE t.message_id=m.id \
                            AND t.state IN ('active','detached')) \
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
            "SELECT id, account_id, remote_path, display_name, role, parent_id, unread_count, total_count
             FROM folders WHERE account_id = ? ORDER BY id",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn folder(&self, folder_id: i64) -> Result<Folder> {
        let row = sqlx::query_as::<_, FolderRow>(
            "SELECT id, account_id, remote_path, display_name, role, parent_id, unread_count, total_count FROM folders WHERE id=?",
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
        // Второй проход: разрешаем parent_id по remote_path родителя (родитель
        // мог быть вставлен позже в этом же батче). Родитель верхнего уровня
        // (msgfolderroot и т.п.) среди папок отсутствует - parent_id остаётся NULL.
        for folder in folders {
            if let Some(parent) = folder.parent_remote_path.as_deref() {
                sqlx::query(
                    "UPDATE folders SET parent_id = (
                         SELECT id FROM folders WHERE account_id=? AND remote_path=?
                     ) WHERE account_id=? AND remote_path=?",
                )
                .bind(account_id)
                .bind(parent)
                .bind(account_id)
                .bind(&folder.remote_path)
                .execute(&mut *tx)
                .await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    /// Слепок состояния папок аккаунта - по записи на папку: путь, имя, роль,
    /// родитель и счётчики писем. Синхронизация сравнивает слепок до и после
    /// сохранения папок: появившаяся, переименованная, переехавшая папка или
    /// сдвинувшиеся счётчики означают, что интерфейсу есть что перечитать.
    /// Именно по папкам, а не суммами: +1 в одной папке и -1 в другой дали бы
    /// одинаковую сумму, и изменение осталось бы незамеченным. Сравниваем
    /// типизированные записи, а не склеенную строку - разделитель мог бы
    /// встретиться в имени папки и спрятать изменение.
    pub async fn folder_state_signature(&self, account_id: i64) -> Result<Vec<FolderState>> {
        Ok(sqlx::query_as::<_, FolderState>(
            "SELECT remote_path, display_name, role, parent_id,
                    COALESCE(unread_count, 0) AS unread_count,
                    COALESCE(total_count, 0) AS total_count
               FROM folders WHERE account_id = ? ORDER BY remote_path",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?)
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
        // Снимок пределов берётся до цикла: клонировать его на каждое
        // письмо незачем, значение за один проход не меняется.
        let limits = self.limit_set();
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
            // Проекция письма убрана провайдером - строка уходит, а сроки дела
            // и закрепление остаются: письмо то же самое.
            preserve_message_traits(&mut tx, *id, &limits).await?;
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
        // Снимок пределов берётся до цикла: клонировать его на каждое
        // письмо незачем, значение за один проход не меняется.
        let limits = self.limit_set();
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
            // Сверка снимка и смена признака действительности папки удаляют
            // строку письма, которое на сервере осталось: приметы сохраняются
            // ровно как при своём переносе.
            preserve_message_traits(&mut tx, *id, &limits).await?;
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
            let result = sqlx::query(concat!(
                "UPDATE messages SET seen=?,
                    flagged=CASE WHEN EXISTS(
                        SELECT 1 FROM outbox_ops o WHERE o.message_id=messages.id
                         AND o.op_kind='flag' AND o.status IN ",
                unfinished_flag_op_sql!(),
                flag_op_changes_flag_sql!("o."),
                "
                    ) THEN flagged ELSE ? END,
                    answered=?, draft=?
                 WHERE account_id=? AND uid=? AND folder_id=(
                    SELECT id FROM folders WHERE account_id=? AND remote_path=?
                 )"
            ))
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
            sqlx::query(AssertSqlSafe(format!(
                concat!(
                    "UPDATE message_tasks SET {transition}
                  WHERE state IN ('active','detached','done') AND message_id IN (
                    SELECT m.id FROM messages m WHERE m.account_id=? AND m.uid=?
                     AND m.folder_id=(SELECT id FROM folders WHERE account_id=? AND remote_path=?)
                     AND NOT EXISTS(SELECT 1 FROM outbox_ops o WHERE o.message_id=m.id
                         AND o.op_kind='flag' AND o.status IN ",
                    unfinished_flag_op_sql!(),
                    flag_op_changes_flag_sql!("o."),
                    ")
                  )"
                ),
                transition = TASK_SYNC_FLAG_TRANSITION_SQL
            )))
            .bind(update.flagged)
            .bind(update.flagged)
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
        // Снимок пределов берётся до цикла: клонировать его на каждое
        // письмо незачем, значение за один проход не меняется.
        let limits = self.limit_set();
        if vanished.is_empty() {
            return Ok(0);
        }
        let started = std::time::Instant::now();
        let mut tx = self.begin_write().await?;
        let mut delete_ids = Vec::new();
        let mut blob_refs = Vec::new();
        for (path, uids) in vanished {
            for uid in uids {
                let row: Option<VanishedMessageRow> = sqlx::query_as(
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
            // Письмо исчезло из этой папки, но в ящике оно чаще всего уже
            // лежит на новом месте: приметы уходят туда, а не пропадают.
            preserve_message_traits(&mut tx, *id, &limits).await?;
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
                    sqlx::query("DELETE FROM contact_addresses WHERE contact_id=?")
                        .bind(contact_id)
                        .execute(&mut *tx)
                        .await?;
                    for address in &contact.addresses {
                        sqlx::query(
                            "INSERT INTO contact_addresses(contact_id, kind, street, city,
                                                           region, postal_code, country)
                             VALUES(?, ?, ?, ?, ?, ?, ?)",
                        )
                        .bind(contact_id)
                        .bind(&address.kind)
                        .bind(&address.street)
                        .bind(&address.city)
                        .bind(&address.region)
                        .bind(&address.postal_code)
                        .bind(&address.country)
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
        self.advance_recipient_history(account_id).await?;
        Ok(counts)
    }

    /// `backfilled` - письма подняты прокруткой вглубь папки. Пометка ставится в
    /// той же транзакции, что и вставка: у старой переписки новые rowid, и пока
    /// флага нет, подоспевшая проверка правил разослала бы её по папкам на
    /// сервере как только что пришедшую.
    pub async fn save_discovered_messages(
        &self,
        account_id: i64,
        messages: &[crate::backend::DiscoveredMessage],
        backfilled: bool,
    ) -> Result<()> {
        // Снимок пределов берётся до цикла: клонировать его на каждое
        // письмо незачем, значение за один проход не меняется.
        let limits = self.limit_set();
        use mail_parser::{MessageParser, MimeHeaders};
        use std::collections::HashSet;
        let mut rows = Vec::new();
        let mut created_refs: Vec<String> = Vec::new();
        for source in messages {
            let normalized = encoded_words::join_split_encoded_words(&source.raw);
            let Some(message) = MessageParser::default().parse(normalized.as_ref()) else {
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
            // out-of-office.md S-063: заголовки правил молчания читаются при
            // сохранении письма. Без них стадия автоответа загружала бы тело
            // письма из сети на каждое новое письмо.
            let raw_header = |name: &str| {
                message
                    .header_raw(name)
                    .map(|value| value.trim().to_owned())
                    .filter(|value| !value.is_empty())
            };
            let is_newsletter = message.header("List-Unsubscribe").is_some();
            let auto_submitted = raw_header("Auto-Submitted");
            let precedence = raw_header("Precedence");
            let return_path = message.header_raw("Return-Path").map(str::trim);
            let return_path_empty = return_path.is_some_and(|value| {
                let inner = value.trim_start_matches('<').trim_end_matches('>').trim();
                inner.is_empty()
            });
            let auto_response_suppress = raw_header("X-Auto-Response-Suppress");
            // S-039: у письма, чьи заголовки серверный модуль не отдаёт,
            // правила молчания по заголовкам не применяются вовсе. Полное
            // письмо их даёт всегда, облегчённая проекция - только если модуль
            // включил их в перечень.
            let silence_headers_known = source.body_fetched
                || is_newsletter
                || auto_submitted.is_some()
                || precedence.is_some()
                || return_path.is_some()
                || auto_response_suppress.is_some();
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
            let reply_to_json = match serde_json::to_string(&addresses(message.reply_to())) {
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
                // Дату держим в UTC: курсор страниц и слияние списка сравнивают
                // её как строку, а при смешанных смещениях ("+03:00" и "Z")
                // такое сравнение путало порядок - письма пропадали между
                // страницами или возвращались повторно.
                message.date().map(Self::normalized_date),
                attachment_rows,
                auth("dkim"),
                auth("spf"),
                auth("dmarc"),
                raw_ref,
                SilenceHeaders {
                    is_newsletter,
                    auto_submitted,
                    precedence,
                    return_path_empty,
                    auto_response_suppress,
                    reply_to_json,
                    known: silence_headers_known,
                },
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
                silence,
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
                sqlx::query(concat!(
                    // backfilled намеренно не входит в DO UPDATE SET: письмо,
                    // уже лежавшее в базе, не должно стать "догруженным" из-за
                    // перекрытия страниц - иначе правила его больше не увидят.
                    "INSERT INTO messages(account_id, folder_id, uid, remote_id, rfc822_message_id, in_reply_to, references_ids, from_name, from_addr, to_addrs, cc_addrs, subject, preview, date, size, seen, flagged, answered, draft, has_attachments, dkim_pass, spf_pass, dmarc_pass, raw_blob_ref, body_fetched, backfilled, is_newsletter, auto_submitted, precedence, return_path_empty, auto_response_suppress, reply_to_addrs, silence_headers_known)
                     VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                     ON CONFLICT(folder_id, uid) DO UPDATE SET
                        remote_id=coalesce(excluded.remote_id,messages.remote_id), rfc822_message_id=excluded.rfc822_message_id, in_reply_to=excluded.in_reply_to,
                        references_ids=excluded.references_ids, from_name=excluded.from_name,
                        from_addr=excluded.from_addr, to_addrs=excluded.to_addrs, cc_addrs=excluded.cc_addrs,
                        subject=excluded.subject, preview=excluded.preview, date=excluded.date, size=excluded.size,
                        seen=excluded.seen,
                        flagged=CASE WHEN EXISTS(
                            SELECT 1 FROM outbox_ops o WHERE o.message_id=messages.id
                             AND o.op_kind='flag' AND o.status IN ",
                        unfinished_flag_op_sql!(),
                        flag_op_changes_flag_sql!("o."),
                        "
                        ) THEN messages.flagged ELSE excluded.flagged END,
                        answered=excluded.answered,
                        draft=excluded.draft,
                        has_attachments=CASE WHEN messages.body_fetched=1 AND excluded.body_fetched=0 THEN messages.has_attachments ELSE excluded.has_attachments END,
                        dkim_pass=excluded.dkim_pass, spf_pass=excluded.spf_pass,
                        dmarc_pass=excluded.dmarc_pass, raw_blob_ref=excluded.raw_blob_ref,
                        body_fetched=CASE WHEN messages.body_fetched=1 THEN 1 ELSE excluded.body_fetched END,
                        -- Заголовки правил молчания обновляются только тогда,
                        -- когда новая проекция их действительно читала: иначе
                        -- облегчённый повтор стёр бы значения полного письма.
                        is_newsletter=CASE WHEN excluded.silence_headers_known=1 THEN excluded.is_newsletter ELSE messages.is_newsletter END,
                        auto_submitted=CASE WHEN excluded.silence_headers_known=1 THEN excluded.auto_submitted ELSE messages.auto_submitted END,
                        precedence=CASE WHEN excluded.silence_headers_known=1 THEN excluded.precedence ELSE messages.precedence END,
                        return_path_empty=CASE WHEN excluded.silence_headers_known=1 THEN excluded.return_path_empty ELSE messages.return_path_empty END,
                        auto_response_suppress=CASE WHEN excluded.silence_headers_known=1 THEN excluded.auto_response_suppress ELSE messages.auto_response_suppress END,
                        reply_to_addrs=excluded.reply_to_addrs,
                        silence_headers_known=CASE WHEN messages.silence_headers_known=1 THEN 1 ELSE excluded.silence_headers_known END"
                ))
                .bind(account_id).bind(folder_id).bind(source.uid as i64).bind(&source.remote_id).bind(&message_id)
                .bind(&in_reply_to).bind(&references).bind(from_name).bind(from_addr).bind(to).bind(cc)
                .bind(&subject).bind(&preview).bind(&date).bind(source.size.map(i64::from))
                .bind(source.seen as i64).bind(source.flagged as i64).bind(source.answered as i64)
                .bind(source.draft as i64)
                // Признак вложений от сервера важнее разбора: у лёгкой проекции
                // сырого письма нет, и по нему вложения не видны
                // (ews-lightweight-message-fetch.md, S-007).
                .bind(source.has_attachments.unwrap_or(!attachments.is_empty()) as i64).bind(dkim).bind(spf)
                .bind(dmarc).bind(&effective_ref).bind(source.body_fetched as i64)
                .bind(backfilled as i64)
                .bind(silence.is_newsletter as i64).bind(&silence.auto_submitted)
                .bind(&silence.precedence).bind(silence.return_path_empty as i64)
                .bind(&silence.auto_response_suppress).bind(&silence.reply_to_json)
                .bind(silence.known as i64)
                .execute(&mut *tx).await?;
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
                if let Some(fixed_id) = message_id.as_deref() {
                    restore_transferred_message_traits(
                        &mut tx,
                        account_id,
                        fixed_id,
                        message_row_id,
                        &limits,
                    )
                    .await?;
                }
                let pending_flag: (i64,) = sqlx::query_as(concat!(
                    "SELECT count(*) FROM outbox_ops o WHERE o.message_id=? AND o.op_kind='flag'
                      AND o.status IN ",
                    unfinished_flag_op_sql!(),
                    flag_op_changes_flag_sql!("o.")
                ))
                .bind(message_row_id)
                .fetch_one(&mut *tx)
                .await?;
                if pending_flag.0 == 0 {
                    sqlx::query(AssertSqlSafe(format!(
                        "UPDATE message_tasks SET {TASK_SYNC_FLAG_TRANSITION_SQL}
                          WHERE message_id=? AND state IN ('active','detached','done')",
                    )))
                    .bind(source.flagged)
                    .bind(source.flagged)
                    .bind(message_row_id)
                    .execute(&mut *tx)
                    .await?;
                }
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
        // recipient-history.md S-001, S-006: адресная книга письмами больше не
        // пополняется. Адреса, которым пользователь действительно писал,
        // попадают в историю получателей из папок с ролью sent.
        self.advance_recipient_history(account_id).await?;
        Ok(())
    }

    // ---------- Письма ----------

    /// Список писем папки (метаданные), новые сверху.
    /// Дата письма в едином виде - UTC со смещением "+00:00". Заголовок Date
    /// приходит в зоне отправителя, а страницы писем и слияние списка сравнивают
    /// дату как строку: смешанные смещения ломали порядок.
    fn normalized_date(date: &mail_parser::DateTime) -> String {
        chrono::DateTime::from_timestamp(date.to_timestamp(), 0)
            .map(|value| value.format("%Y-%m-%dT%H:%M:%S+00:00").to_string())
            .unwrap_or_else(|| date.to_rfc3339())
    }

    /// Дописать письмам их метки: строка `messages` меток не содержит, они лежат
    /// в отдельной таблице. Без этого бейджи в списке, счётчики и фильтр по метке
    /// видели метки только в умных папках, где выборка заполняет их сама.
    async fn attach_labels(&self, messages: &mut [MessageMeta]) -> Result<()> {
        if messages.is_empty() {
            return Ok(());
        }
        let mut query = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "SELECT ml.message_id, l.name FROM message_labels ml
             JOIN labels l ON l.id = ml.label_id WHERE ml.message_id IN (",
        );
        let mut separated = query.separated(",");
        for message in messages.iter() {
            separated.push_bind(message.id);
        }
        separated.push_unseparated(")");
        let rows = query
            .build_query_as::<(i64, String)>()
            .fetch_all(&self.pool)
            .await?;
        let mut by_message = std::collections::HashMap::<i64, Vec<String>>::new();
        for (message_id, name) in rows {
            by_message.entry(message_id).or_default().push(name);
        }
        for message in messages.iter_mut() {
            if let Some(names) = by_message.remove(&message.id) {
                message.labels = names;
            }
        }
        let mut details = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "SELECT m.id, m.pinned_at, t.due_at,
                    CASE WHEN t.state IS NOT NULL THEN t.state
                         WHEN m.flagged=1 THEN 'active' END,
                    t.start_at, t.reminder_at
               FROM messages m LEFT JOIN message_tasks t ON t.message_id=m.id
              WHERE m.id IN (",
        );
        let mut separated = details.separated(",");
        for message in messages.iter() {
            separated.push_bind(message.id);
        }
        separated.push_unseparated(")");
        let details = details
            .build_query_as::<MessageDetailsRow>()
            .fetch_all(&self.pool)
            .await?;
        let mut details = details
            .into_iter()
            .map(|row| (row.0, (row.1, row.2, row.3, row.4, row.5)))
            .collect::<std::collections::HashMap<_, _>>();
        for message in messages.iter_mut() {
            if let Some((pinned_at, due_at, state, start_at, reminder_at)) =
                details.remove(&message.id)
            {
                message.pinned_at = pinned_at;
                message.task_due_at = due_at;
                message.task_state = state;
                message.task_start_at = start_at;
                message.task_reminder_at = reminder_at;
            }
        }
        Ok(())
    }

    /// Письма папки для внешнего интерфейса приложений: закреплённые письма
    /// отсюда не исключаются. Исключение существует ради того, чтобы письмо не
    /// показывалось дважды в окне программы, и стоит только в запросах страниц
    /// списка (pin-message.md, S-012).
    pub async fn list_messages(&self, folder_id: i64, limit: i64) -> Result<Vec<MessageMeta>> {
        self.list_folder_messages(folder_id, limit, false).await
    }

    /// Первая страница списка папки: закреплённые письма приходят отдельным
    /// перечнем и из страницы исключаются (S-011).
    pub async fn list_folder_first_page(
        &self,
        folder_id: i64,
        limit: i64,
    ) -> Result<Vec<MessageMeta>> {
        self.list_folder_messages(folder_id, limit, true).await
    }

    async fn list_folder_messages(
        &self,
        folder_id: i64,
        limit: i64,
        skip_pinned: bool,
    ) -> Result<Vec<MessageMeta>> {
        let rows = sqlx::query_as::<_, MessageRow>(
            "SELECT id, account_id, folder_id, thread_id, uid, rfc822_message_id,
                    from_name, from_addr, to_addrs, cc_addrs, subject, preview, date, size,
                    seen, flagged, answered, draft, has_attachments, dkim_pass, spf_pass, dmarc_pass
             FROM messages
             WHERE folder_id = ?
               AND (? = 0 OR pinned_at IS NULL)
               AND (snoozed_until IS NULL OR snoozed_until <= datetime('now'))
               AND NOT EXISTS (
                 SELECT 1 FROM outbox_ops o
                  WHERE o.message_id=messages.id AND o.op_kind IN ('move','delete')
                    AND o.status IN ('pending','processing','retry')
               )
             ORDER BY date DESC, id DESC LIMIT ?",
        )
        .bind(folder_id)
        .bind(i64::from(skip_pinned))
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        // Порядок тот же, что у курсорных страниц: (date DESC, id DESC). Сортировка
        // только по дате рассогласовывала первую страницу с курсором, и письма с
        // одинаковой датой могли не попасть ни в одну страницу.
        let mut messages = rows.into_iter().map(MessageMeta::from).collect::<Vec<_>>();
        self.attach_labels(&mut messages).await?;
        Ok(messages)
    }

    /// Дата самого старого письма папки. Нужна догрузке как курсор: фронтенд
    /// вёл его по своему списку писем, но письма, не попавшие в фильтр открытой
    /// умной папки, туда не добавлялись, курсор не двигался, и следующий проход
    /// запрашивал у сервера ту же самую страницу.
    pub async fn folder_oldest_message_date(&self, folder_id: i64) -> Result<Option<String>> {
        let value: Option<Option<String>> =
            sqlx::query_scalar("SELECT MIN(date) FROM messages WHERE folder_id = ?")
                .bind(folder_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(value.flatten().filter(|date| !date.is_empty()))
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
            return self.list_folder_first_page(folder_id, limit).await;
        }
        let date = before_date.unwrap_or_default();
        let id = before_id.unwrap_or(i64::MAX);
        let rows = sqlx::query_as::<_, MessageRow>(
            "SELECT id, account_id, folder_id, thread_id, uid, rfc822_message_id,
                    from_name, from_addr, to_addrs, cc_addrs, subject, preview, date, size,
                    seen, flagged, answered, draft, has_attachments, dkim_pass, spf_pass, dmarc_pass
             FROM messages
             WHERE folder_id = ?
               AND pinned_at IS NULL
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
        let mut messages = rows.into_iter().map(MessageMeta::from).collect::<Vec<_>>();
        self.attach_labels(&mut messages).await?;
        Ok(messages)
    }

    /// Курсорная страница писем с меткой. Раздел меток раньше показывал только
    /// то, что уже загружено в память по папкам, и письмо с меткой за пределами
    /// загруженных страниц не появлялось вовсе.
    pub async fn list_label_messages_page(
        &self,
        label: &str,
        before_date: Option<&str>,
        before_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<MessageMeta>> {
        let limit = limit.clamp(1, 500);
        let rows = match (before_date, before_id) {
            (Some(date), Some(id)) => {
                sqlx::query_as::<_, MessageRow>(
                    "SELECT id, account_id, folder_id, thread_id, uid, rfc822_message_id,
                            from_name, from_addr, to_addrs, cc_addrs, subject, preview, date, size,
                            seen, flagged, answered, draft, has_attachments, dkim_pass, spf_pass, dmarc_pass
                     FROM messages
                     WHERE id IN (SELECT ml.message_id FROM message_labels ml
                                  JOIN labels l ON l.id = ml.label_id WHERE l.name = ?)
                       AND pinned_at IS NULL
                       AND (COALESCE(date, '') < ? OR (COALESCE(date, '') = ? AND id < ?))
                       AND (snoozed_until IS NULL OR snoozed_until <= datetime('now'))
                       AND NOT EXISTS (
                         SELECT 1 FROM outbox_ops o
                          WHERE o.message_id=messages.id AND o.op_kind IN ('move','delete')
                            AND o.status IN ('pending','processing','retry')
                       )
                     ORDER BY date DESC, id DESC LIMIT ?",
                )
                .bind(label)
                .bind(date)
                .bind(date)
                .bind(id)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            _ => {
                sqlx::query_as::<_, MessageRow>(
                    "SELECT id, account_id, folder_id, thread_id, uid, rfc822_message_id,
                            from_name, from_addr, to_addrs, cc_addrs, subject, preview, date, size,
                            seen, flagged, answered, draft, has_attachments, dkim_pass, spf_pass, dmarc_pass
                     FROM messages
                     WHERE id IN (SELECT ml.message_id FROM message_labels ml
                                  JOIN labels l ON l.id = ml.label_id WHERE l.name = ?)
                       AND pinned_at IS NULL
                       AND (snoozed_until IS NULL OR snoozed_until <= datetime('now'))
                       AND NOT EXISTS (
                         SELECT 1 FROM outbox_ops o
                          WHERE o.message_id=messages.id AND o.op_kind IN ('move','delete')
                            AND o.status IN ('pending','processing','retry')
                       )
                     ORDER BY date DESC, id DESC LIMIT ?",
                )
                .bind(label)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
        };
        let mut messages = rows.into_iter().map(MessageMeta::from).collect::<Vec<_>>();
        self.attach_labels(&mut messages).await?;
        Ok(messages)
    }

    /// Сколько писем у каждой метки - для счётчика в разделе меток: считать по
    /// загруженным в память письмам значило показывать заниженное число.
    pub async fn label_message_counts(&self) -> Result<Vec<(String, i64)>> {
        // Условия те же, что в list_label_messages_page, иначе счётчик обещал бы
        // больше писем, чем раздел метки показывает.
        Ok(sqlx::query_as(
            "SELECT l.name, COUNT(m.id) FROM labels l
             LEFT JOIN message_labels ml ON ml.label_id = l.id
             LEFT JOIN messages m ON m.id = ml.message_id
                  AND (m.snoozed_until IS NULL OR m.snoozed_until <= datetime('now'))
                  AND NOT EXISTS (
                    SELECT 1 FROM outbox_ops o
                     WHERE o.message_id = m.id AND o.op_kind IN ('move','delete')
                       AND o.status IN ('pending','processing','retry')
                  )
             GROUP BY l.id ORDER BY l.name COLLATE NOCASE",
        )
        .fetch_all(&self.pool)
        .await?)
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
        let mut messages = ids
            .iter()
            .filter_map(|id| by_id.remove(id))
            .collect::<Vec<_>>();
        self.attach_labels(&mut messages).await?;
        Ok(messages)
    }

    /// Закрепить или открепить пачку писем. Предел считается отдельно для
    /// каждого ящика внутри одной транзакции, включая отложенные и уводимые
    /// письма: они остаются живыми и позднее вернутся в список.
    pub async fn set_messages_pinned(
        &self,
        message_ids: &[i64],
        pinned: bool,
    ) -> Result<PinMessagesResult> {
        let mut tx = self.begin_write().await?;
        let mut result = PinMessagesResult::default();
        for message_id in message_ids.iter().copied() {
            let row: Option<(i64, Option<String>)> =
                sqlx::query_as("SELECT account_id, pinned_at FROM messages WHERE id=?")
                    .bind(message_id)
                    .fetch_optional(&mut *tx)
                    .await?;
            let Some((account_id, current)) = row else {
                return Err(crate::Error::Other("письмо не найдено".into()));
            };
            if pinned == current.is_some() {
                continue;
            }
            if pinned {
                let count: (i64,) = sqlx::query_as(
                    "SELECT count(*) FROM messages WHERE account_id=? AND pinned_at IS NOT NULL",
                )
                .bind(account_id)
                .fetch_one(&mut *tx)
                .await?;
                if count.0 >= self.limit(LIMIT_PINNED_PER_ACCOUNT) {
                    result.rejected_limit += 1;
                    continue;
                }
                sqlx::query("UPDATE messages SET pinned_at=datetime('now') WHERE id=?")
                    .bind(message_id)
                    .execute(&mut *tx)
                    .await?;
            } else {
                sqlx::query("UPDATE messages SET pinned_at=NULL WHERE id=?")
                    .bind(message_id)
                    .execute(&mut *tx)
                    .await?;
            }
            result.changed += 1;
        }
        tx.commit().await?;
        Ok(result)
    }

    /// Закреплённая часть читается целиком и отдельна от курсора обычных
    /// страниц. Вид представления: folder, label, smart или unified.
    pub async fn list_pinned_messages(
        &self,
        view_kind: &str,
        view_value: Option<&str>,
    ) -> Result<PinnedMessageList> {
        if !matches!(view_kind, "folder" | "label" | "smart" | "unified") {
            return Err(crate::Error::Other(
                "неизвестный вид списка закрепленных писем".into(),
            ));
        }
        // Снимок закреплённых писем берётся отдельной неделимой операцией и
        // сразу закрывается: единственное соединение записи иначе держалось бы
        // на весь обход базы, и очередь операций, синхронизация и прогон
        // правил вставали бы вместе с ним. Обычная часть собирается после
        // снимка, а письма снимка из неё отсеиваются, поэтому письмо,
        // откреплённое между двумя чтениями, не приходит дважды.
        let mut snapshot = self.begin_write().await?;
        let rows = sqlx::query_as::<_, MessageRow>(
            "SELECT id, account_id, folder_id, thread_id, uid, rfc822_message_id,
                    from_name, from_addr, to_addrs, cc_addrs, subject, preview, date, size,
                    seen, flagged, answered, draft, has_attachments, dkim_pass, spf_pass, dmarc_pass
               FROM messages WHERE pinned_at IS NOT NULL
                AND (snoozed_until IS NULL OR snoozed_until<=datetime('now'))
                AND NOT EXISTS(SELECT 1 FROM outbox_ops o WHERE o.message_id=messages.id
                    AND o.op_kind IN ('move','delete')
                    AND o.status IN ('pending','processing','retry'))
              ORDER BY date DESC,id DESC",
        )
        .fetch_all(&mut *snapshot)
        .await?;
        let mut messages = self
            .pinned_messages_for_view(&mut snapshot, view_kind, view_value, rows)
            .await?;
        snapshot.commit().await?;
        self.attach_labels(&mut messages).await?;
        let total = messages.len();
        let mut ordinary = self.ordinary_page_for_view(view_kind, view_value).await?;
        self.attach_labels(&mut ordinary).await?;
        let shown = messages
            .iter()
            .map(|message| message.id)
            .collect::<std::collections::HashSet<_>>();
        ordinary.retain(|message| !shown.contains(&message.id));
        Ok(PinnedMessageList {
            messages,
            total,
            ordinary,
        })
    }

    /// Оставить из закреплённых писем те, что попадают в представление. Вид
    /// без своего значения - ошибка вызова, а не пустой список: пустой ответ
    /// выглядел бы как "закреплённых писем нет" и прятал бы ошибку.
    async fn pinned_messages_for_view(
        &self,
        snapshot: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        view_kind: &str,
        view_value: Option<&str>,
        rows: Vec<MessageRow>,
    ) -> Result<Vec<MessageMeta>> {
        // Умная папка и объединённый список отбирают письма тем же разбором
        // видимости, каким идёт и обычная страница.
        let mut context = if matches!(view_kind, "smart" | "unified") {
            Some(self.smart_selection_context(true).await?)
        } else {
            None
        };
        // Отбор умной папки читается своими запросами мимо снимка: под писателем
        // держится только чтение самих закреплённых писем.
        let smart = if view_kind == "smart" {
            let id =
                view_value.ok_or_else(|| crate::Error::Other("умная папка не указана".into()))?;
            Some(
                self.list_smart_folders()
                    .await?
                    .into_iter()
                    .find(|folder| folder.id == id)
                    .ok_or_else(|| crate::Error::Other("умная папка не найдена".into()))?,
            )
        } else {
            None
        };
        let label_ids = if view_kind == "label" {
            let label = view_value.ok_or_else(|| crate::Error::Other("метка не указана".into()))?;
            sqlx::query_as::<_, (i64,)>(
                "SELECT ml.message_id FROM message_labels ml JOIN labels l ON l.id=ml.label_id
                  WHERE l.name=?",
            )
            .bind(label)
            .fetch_all(&mut **snapshot)
            .await?
            .into_iter()
            .map(|row| row.0)
            .collect::<std::collections::HashSet<_>>()
        } else {
            std::collections::HashSet::new()
        };
        let folder_id = if view_kind == "folder" {
            Some(
                view_value
                    .and_then(|value| value.parse::<i64>().ok())
                    .ok_or_else(|| crate::Error::Other("папка не указана".into()))?,
            )
        } else {
            None
        };
        let mut messages = Vec::new();
        for row in rows {
            let message = if let Some(context) = context.as_mut() {
                let Some(message) = context.prepare(row, true) else {
                    continue;
                };
                if let Some(smart) = smart.as_ref()
                    && !context.matches(smart, &message)
                {
                    continue;
                }
                message
            } else {
                MessageMeta::from(row)
            };
            if folder_id.is_some_and(|id| message.folder_id != id)
                || (view_kind == "label" && !label_ids.contains(&message.id))
            {
                continue;
            }
            messages.push(message);
        }
        Ok(messages)
    }

    /// Первая страница писем представления без закрепления: та же выборка, в
    /// которую список идёт и без закреплённой части.
    async fn ordinary_page_for_view(
        &self,
        view_kind: &str,
        view_value: Option<&str>,
    ) -> Result<Vec<MessageMeta>> {
        const PAGE: i64 = 100;
        Ok(match view_kind {
            "folder" => {
                let folder_id = view_value
                    .and_then(|value| value.parse::<i64>().ok())
                    .unwrap_or_default();
                self.list_folder_first_page(folder_id, PAGE).await?
            }
            "label" => {
                self.list_label_messages_page(view_value.unwrap_or_default(), None, None, PAGE)
                    .await?
            }
            "smart" => {
                self.list_smart_folder_messages_page(
                    view_value.unwrap_or_default(),
                    None,
                    None,
                    PAGE as usize,
                )
                .await?
            }
            // У объединённого списка своей страницы нет: читаем общий поток
            // писем и берём видимые, пока не наберётся страница.
            _ => {
                let rows = sqlx::query_as::<_, MessageRow>(SMART_PAGE_FIRST_SQL)
                    .bind(1_000_i64)
                    .fetch_all(&self.pool)
                    .await?;
                let mut context = self.smart_selection_context(true).await?;
                rows.into_iter()
                    .filter_map(|row| context.prepare(row, true))
                    .take(PAGE as usize)
                    .collect()
            }
        })
    }

    pub async fn save_message_task(&self, input: &MessageTaskInput) -> Result<MessageTask> {
        // Время интерфейса приходит в другом виде, а сравнения сроков идут
        // строками: без приведения к виду базы просроченных дел не бывает,
        // а напоминание ждёт полуночи (S-017).
        let times = validate_task_times(input)?;
        let mut tx = self.begin_write().await?;
        let locator: (i64, i64, String, Option<String>, i64, i64) = sqlx::query_as(
            "SELECT m.account_id, m.uid, f.remote_path, m.remote_id, m.seen, m.flagged
               FROM messages m JOIN folders f ON f.id=m.folder_id
               JOIN accounts a ON a.id=m.account_id AND a.enabled=1 WHERE m.id=?",
        )
        .bind(input.message_id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO message_tasks(message_id,start_at,due_at,reminder_at,state,completed_at,
                                       reminder_shown_at,updated_at)
             VALUES(?,?,?,?,'active',NULL,NULL,datetime('now'))
             ON CONFLICT(message_id) DO UPDATE SET start_at=excluded.start_at,
                 due_at=excluded.due_at, reminder_at=excluded.reminder_at, state='active',
                 completed_at=NULL, reminder_shown_at=NULL, updated_at=datetime('now')",
        )
        .bind(input.message_id)
        .bind(&times.start_at)
        .bind(&times.due_at)
        .bind(&times.reminder_at)
        .execute(&mut *tx)
        .await?;
        if locator.5 == 0 {
            sqlx::query("UPDATE messages SET flagged=1 WHERE id=?")
                .bind(input.message_id)
                .execute(&mut *tx)
                .await?;
            Self::queue_flag_sync(
                &mut tx,
                input.message_id,
                locator.0,
                &locator.2,
                locator.1,
                locator.3.as_deref(),
                locator.4 != 0,
                true,
                true,
            )
            .await?;
        }
        let task = read_message_task(&mut tx, input.message_id).await?;
        tx.commit().await?;
        Ok(task)
    }

    pub async fn get_message_task(&self, message_id: i64) -> Result<Option<MessageTask>> {
        let row = sqlx::query_as::<_, MessageTaskRow>(MESSAGE_TASK_BY_ID_SQL)
            .bind(message_id)
            .fetch_optional(&self.pool)
            .await?;
        if let Some(row) = row {
            return Ok(Some(row.into()));
        }
        let flagged: Option<(i64,)> = sqlx::query_as("SELECT flagged FROM messages WHERE id=?")
            .bind(message_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(flagged.filter(|row| row.0 != 0).map(|_| MessageTask {
            message_id,
            start_at: None,
            due_at: None,
            reminder_at: None,
            state: "active".into(),
            completed_at: None,
            reminder_shown_at: None,
            created_at: None,
            updated_at: None,
        }))
    }

    pub async fn complete_message_task(&self, message_id: i64) -> Result<MessageTask> {
        self.set_message_task_completed(message_id, true).await
    }

    pub async fn reopen_message_task(&self, message_id: i64) -> Result<MessageTask> {
        self.set_message_task_completed(message_id, false).await
    }

    async fn set_message_task_completed(
        &self,
        message_id: i64,
        completed: bool,
    ) -> Result<MessageTask> {
        let mut tx = self.begin_write().await?;
        let locator: (i64, i64, String, Option<String>, i64) = sqlx::query_as(
            "SELECT m.account_id, m.uid, f.remote_path, m.remote_id, m.seen
               FROM messages m JOIN folders f ON f.id=m.folder_id WHERE m.id=?",
        )
        .bind(message_id)
        .fetch_one(&mut *tx)
        .await?;
        if completed {
            sqlx::query(
                "INSERT INTO message_tasks(message_id,state,completed_at,updated_at)
                 VALUES(?,'done',datetime('now'),datetime('now'))
                 ON CONFLICT(message_id) DO UPDATE SET state='done',
                     completed_at=datetime('now'), updated_at=datetime('now')",
            )
            .bind(message_id)
            .execute(&mut *tx)
            .await?;
        } else {
            let changed = sqlx::query(
                "UPDATE message_tasks SET state='active', completed_at=NULL,
                        updated_at=datetime('now') WHERE message_id=? AND state='done'",
            )
            .bind(message_id)
            .execute(&mut *tx)
            .await?;
            if changed.rows_affected() != 1 {
                return Err(crate::Error::Other("выполненное дело не найдено".into()));
            }
        }
        sqlx::query("UPDATE messages SET flagged=? WHERE id=?")
            .bind((!completed) as i64)
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
            !completed,
            true,
        )
        .await?;
        let task = read_message_task(&mut tx, message_id).await?;
        tx.commit().await?;
        Ok(task)
    }

    pub async fn delete_message_task(&self, message_id: i64) -> Result<bool> {
        Ok(sqlx::query("DELETE FROM message_tasks WHERE message_id=?")
            .bind(message_id)
            .execute(&self.write_pool)
            .await?
            .rows_affected()
            != 0)
    }

    pub async fn purge_completed_message_tasks(&self) -> Result<usize> {
        // Срок хранения выполненного дела - настройка, а не число в запросе:
        // сроки у всех разные, а прежде дело исчезало ровно через месяц.
        let removed = sqlx::query(
            "DELETE FROM message_tasks WHERE state='done'
              AND completed_at < datetime('now', ?)",
        )
        .bind(format!("-{} days", self.limit(LIMIT_DONE_TASK_DAYS)))
        .execute(&self.write_pool)
        .await?
        .rows_affected() as usize;
        // Тем же проходом уходят приметы писем, которые так и не вернулись:
        // запись без срока годности лежала бы вечно и однажды досталась бы
        // письму с тем же заголовком.
        sqlx::query(AssertSqlSafe(format!(
            "DELETE FROM storage_meta WHERE key LIKE 'message_traits:%'
               AND COALESCE(json_extract(value,'$.saved_at'),'9999-12-31 23:59:59')
                   < datetime('now','-{} days')",
            self.limit(LIMIT_MESSAGE_TRAITS_DAYS)
        )))
        .execute(&self.write_pool)
        .await?;
        Ok(removed)
    }

    pub async fn overdue_message_task_count(&self) -> Result<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM message_tasks
              WHERE state='active' AND due_at IS NOT NULL AND due_at < datetime('now')",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    pub async fn list_message_tasks(
        &self,
        limit: i64,
        cursor: Option<&str>,
    ) -> Result<TaskListPage> {
        let cursor = cursor.and_then(|value| serde_json::from_str::<TaskPageCursor>(value).ok());
        let base = "WITH task_rows AS (
             SELECT m.id AS message_id, t.start_at, t.due_at, t.reminder_at,
                    COALESCE(t.state,'active') AS state, t.completed_at, t.reminder_shown_at,
                    t.created_at, t.updated_at, m.account_id, a.email AS account_email,
                    m.folder_id, m.subject, m.from_name AS sender_name,
                    m.from_addr AS sender_address, m.date AS message_date, m.snoozed_until,
                    EXISTS(SELECT 1 FROM outbox_ops o WHERE o.message_id=m.id
                           AND o.op_kind IN ('move','delete')
                           AND o.status IN ('pending','processing','retry')) AS has_takeaway,
                    CASE
                      WHEN COALESCE(t.state,'active')='done' THEN 6
                      WHEN COALESCE(t.state,'active')='active' AND t.due_at < datetime('now') THEN 0
                      WHEN date(t.due_at,'localtime')=date('now','localtime') THEN 1
                      WHEN date(t.due_at,'localtime')=date('now','localtime','+1 day') THEN 2
                      WHEN t.due_at IS NOT NULL AND date(t.due_at,'localtime')<=date('now','localtime','+7 days') THEN 3
                      WHEN t.due_at IS NOT NULL THEN 4 ELSE 5 END AS sort_group,
                    COALESCE(t.due_at,'9999-12-31 23:59:59') AS due_key,
                    COALESCE(m.date,'') AS date_key
               FROM messages m
               JOIN accounts a ON a.id=m.account_id AND a.enabled=1
               LEFT JOIN message_tasks t ON t.message_id=m.id
              WHERE (m.flagged=1 OR t.message_id IS NOT NULL)
                AND NOT EXISTS(SELECT 1 FROM outbox_ops o WHERE o.message_id=m.id
                               AND o.op_kind IN ('move','delete')
                               AND o.status IN ('pending','processing','retry'))
            ) SELECT * FROM task_rows";
        let rows = if let Some(cursor) = cursor {
            sqlx::query_as::<_, MessageTaskListRow>(AssertSqlSafe(format!(
                "{base} WHERE sort_group>? OR (sort_group=? AND (
                    due_key>? OR (due_key=? AND (
                      date_key<? OR (date_key=? AND message_id<?)
                    ))
                 )) ORDER BY sort_group,due_key,date_key DESC,message_id DESC LIMIT ?"
            )))
            .bind(cursor.group)
            .bind(cursor.group)
            .bind(&cursor.due)
            .bind(&cursor.due)
            .bind(&cursor.date)
            .bind(&cursor.date)
            .bind(cursor.id)
            .bind(limit.clamp(1, 100))
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, MessageTaskListRow>(AssertSqlSafe(format!(
                "{base} ORDER BY sort_group,due_key,date_key DESC,message_id DESC LIMIT ?"
            )))
            .bind(limit.clamp(1, 100))
            .fetch_all(&self.pool)
            .await?
        };
        let count = rows.len() as i64;
        let next_cursor = rows.last().map(|row| {
            serde_json::to_string(&TaskPageCursor {
                group: row.sort_group,
                due: row.due_key.clone(),
                date: row.date_key.clone(),
                id: row.message_id,
            })
            .unwrap_or_default()
        });
        Ok(TaskListPage {
            items: rows.into_iter().map(Into::into).collect(),
            next_cursor: (count == limit.clamp(1, 100))
                .then_some(next_cursor)
                .flatten(),
        })
    }

    pub async fn due_task_reminders(&self, limit: i64) -> Result<Vec<TaskReminder>> {
        Ok(sqlx::query_as::<_, TaskReminderRow>(
            "SELECT t.message_id, m.from_name AS sender_name, m.from_addr AS sender_address,
                    m.subject, t.due_at, t.reminder_at
               FROM message_tasks t JOIN messages m ON m.id=t.message_id
              WHERE t.state='active' AND t.reminder_shown_at IS NULL
                AND t.reminder_at<=datetime('now') ORDER BY t.reminder_at,t.message_id LIMIT ?",
        )
        .bind(limit.clamp(1, 50))
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(Into::into)
        .collect())
    }

    pub async fn mark_task_reminders_shown(&self, message_ids: &[i64]) -> Result<usize> {
        if message_ids.is_empty() {
            return Ok(0);
        }
        let mut query = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "UPDATE message_tasks SET reminder_shown_at=datetime('now'), updated_at=datetime('now')
              WHERE message_id IN (",
        );
        let mut separated = query.separated(",");
        for id in message_ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");
        Ok(query
            .build()
            .execute(&self.write_pool)
            .await?
            .rows_affected() as usize)
    }

    pub async fn snooze_task_reminder(&self, message_id: i64) -> Result<MessageTask> {
        let mut tx = self.begin_write().await?;
        let changed = sqlx::query(
            "UPDATE message_tasks SET reminder_at=datetime('now','+10 minutes'),
                    reminder_shown_at=NULL, updated_at=datetime('now')
              WHERE message_id=? AND state='active'",
        )
        .bind(message_id)
        .execute(&mut *tx)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(crate::Error::Other("активное дело не найдено".into()));
        }
        let task = read_message_task(&mut tx, message_id).await?;
        tx.commit().await?;
        Ok(task)
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

    /// Письма аккаунта, у которых нет тела: их надо докачать заранее, чтобы
    /// письмо открывалось локально (gmail-local-body-prefetch.md, S-002 - S-006).
    /// `retention_days` = 0 - без ограничения по времени. Письма больше
    /// `max_size_bytes` пропускаем: их качают по открытию.
    /// Порядок - от новых к старым.
    pub async fn messages_missing_body(
        &self,
        account_id: i64,
        retention_days: i64,
        max_size_bytes: i64,
        limit: i64,
    ) -> Result<Vec<(i64, String, i64, Option<String>)>> {
        let cutoff = if retention_days > 0 {
            Some(format!("-{retention_days} days"))
        } else {
            None
        };
        let rows: Vec<(i64, String, i64, Option<String>)> = sqlx::query_as(
            "SELECT m.id, f.remote_path, m.uid, m.remote_id              FROM messages m JOIN folders f ON f.id = m.folder_id              WHERE m.account_id = ? AND m.body_fetched = 0                AND (m.size IS NULL OR m.size <= ?)                AND (? IS NULL OR (m.date IS NOT NULL AND m.date >= datetime('now', ?)))              ORDER BY m.date DESC, m.id DESC LIMIT ?",
        )
        .bind(account_id)
        .bind(max_size_bytes)
        .bind(cutoff.as_deref())
        .bind(cutoff.as_deref())
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
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
        let normalized = raw.as_deref().map(encoded_words::join_split_encoded_words);
        let parsed = normalized
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
    /// `not_before` и `not_after` - границы даты письма в том же виде, в каком
    /// дата лежит в базе (UTC, "%Y-%m-%dT%H:%M:%S+00:00"). За границами письма в
    /// выборку не попадают: локально новый id бывает и у догруженной истории, а
    /// уведомлять о письме позапрошлого года незачем. Верхняя граница нужна для
    /// писем с испорченной датой далеко в будущем - без неё такое письмо всегда
    /// считалось бы свежим. Письма без даты остаются: заголовок Date не
    /// обязателен, и терять из-за него уведомление нельзя.
    /// Письмо всё ещё доступно уведомлению: стадии его не закрыли и
    /// незавершённой операции увода по нему нет. Проверяется непосредственно
    /// перед показом, уже по зафиксированной базе (S-010).
    pub async fn message_is_notifiable(&self, message_id: i64) -> Result<bool> {
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT m.id FROM messages m
              WHERE m.id = ? AND m.closed_by_stage IS NULL
                AND NOT EXISTS (
                     SELECT 1 FROM outbox_ops o WHERE o.message_id = m.id
                       AND o.op_kind IN ('move','delete')
                       AND o.status IN ('pending','processing','retry')
                )",
        )
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.is_some())
    }

    pub async fn inbox_message_ids_by_remote_ids(
        &self,
        account_id: i64,
        remote_ids: &[String],
        not_before: Option<&str>,
        not_after: Option<&str>,
    ) -> Result<Vec<i64>> {
        if remote_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; remote_ids.len()].join(",");
        let date_filter = match (not_before.is_some(), not_after.is_some()) {
            (true, true) => " AND (m.date IS NULL OR (m.date >= ? AND m.date <= ?))",
            (true, false) => " AND (m.date IS NULL OR m.date >= ?)",
            (false, true) => " AND (m.date IS NULL OR m.date <= ?)",
            (false, false) => "",
        };
        // S-010: письмо, закрытое стадией обработки или уже стоящее в очереди
        // на увод, в уведомление не попадает - иначе программа зовёт читать
        // почту, которой в Входящих через мгновение не будет.
        let sql = format!(
            "SELECT m.id FROM messages m JOIN folders f ON f.id = m.folder_id \
                 WHERE m.account_id = ? AND f.role = 'inbox' AND m.remote_id IN ({placeholders}){date_filter} \
                   AND m.closed_by_stage IS NULL \
                   AND NOT EXISTS ( \
                        SELECT 1 FROM outbox_ops o WHERE o.message_id = m.id \
                          AND o.op_kind IN ('move','delete') \
                          AND o.status IN ('pending','processing','retry') \
                   ) \
                 ORDER BY m.date ASC"
        );
        // Плейсхолдеры формируются только из числа id (не из пользовательских
        // данных), сами значения передаются через bind - инъекция невозможна.
        let mut query = sqlx::query_as::<_, (i64,)>(sqlx::AssertSqlSafe(sql)).bind(account_id);
        for id in remote_ids {
            query = query.bind(id);
        }
        if let Some(border) = not_before {
            query = query.bind(border.to_owned());
        }
        if let Some(border) = not_after {
            query = query.bind(border.to_owned());
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
            false,
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
        self.mark_flagged_many(&[message_id], flagged, FlagChangeReason::User)
            .await?;
        Ok(())
    }

    /// Единый путь записи признака важности. Причина нужна переходам дела, а
    /// пачка выполняется одной транзакцией, поэтому групповое действие не
    /// оставляет половину выбранных писем в новом состоянии.
    pub async fn mark_flagged_many(
        &self,
        message_ids: &[i64],
        flagged: bool,
        reason: FlagChangeReason,
    ) -> Result<usize> {
        if message_ids.is_empty() {
            return Ok(0);
        }
        let mut tx = self.begin_write().await?;
        for message_id in message_ids.iter().copied() {
            let locator: (i64, i64, String, Option<String>, i64) = sqlx::query_as(
                "SELECT m.account_id, m.uid, f.remote_path, m.remote_id, m.seen FROM messages m
                 JOIN folders f ON f.id=m.folder_id WHERE m.id=?",
            )
            .bind(message_id)
            .fetch_one(&mut *tx)
            .await?;
            if reason == FlagChangeReason::Sync {
                let pending: (i64,) = sqlx::query_as(concat!(
                    "SELECT count(*) FROM outbox_ops o WHERE o.message_id=? AND o.op_kind='flag'
                      AND o.status IN ",
                    unfinished_flag_op_sql!(),
                    flag_op_changes_flag_sql!("o.")
                ))
                .bind(message_id)
                .fetch_one(&mut *tx)
                .await?;
                if pending.0 != 0 {
                    continue;
                }
            }
            sqlx::query("UPDATE messages SET flagged = ? WHERE id = ?")
                .bind(flagged as i64)
                .bind(message_id)
                .execute(&mut *tx)
                .await?;
            apply_task_flag_transition(&mut tx, message_id, reason, flagged).await?;
            if reason != FlagChangeReason::Sync {
                Self::queue_flag_sync(
                    &mut tx,
                    message_id,
                    locator.0,
                    &locator.2,
                    locator.1,
                    locator.3.as_deref(),
                    locator.4 != 0,
                    flagged,
                    true,
                )
                .await?;
            }
        }
        tx.commit().await?;
        Ok(message_ids.len())
    }

    /// Общая часть mark_seen/mark_flagged: заменить незавершённую
    /// 'flag'-операцию в outbox на новую с актуальным состоянием обоих
    /// флагов сразу. Пересборка целиком (а не только изменённого поля)
    /// нужна, чтобы более ранняя ещё не отправленная пометка не потерялась,
    /// когда пользователь быстро меняет seen и flagged подряд.
    /// `sets_flagged` говорит, меняет ли эта постановка важность письма.
    /// Отметка о прочтении ставит ту же операцию и важности не касается, а
    /// значение сервера придерживает только настоящее изменение важности.
    async fn queue_flag_sync(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        message_id: i64,
        account_id: i64,
        folder_path: &str,
        uid: i64,
        remote_id: Option<&str>,
        seen: bool,
        flagged: bool,
        sets_flagged: bool,
    ) -> Result<()> {
        // Прежняя незавершённая операция пересобирается целиком, и её обещание
        // об изменении важности наследуется: иначе отметка о прочтении
        // отменила бы придержку значения сервера для ещё не отправленного
        // флажка.
        let previous: Option<(String,)> = sqlx::query_as(
            "SELECT payload FROM outbox_ops WHERE message_id=? AND op_kind='flag'
              AND status IN ('pending','retry')",
        )
        .bind(message_id)
        .fetch_optional(&mut **tx)
        .await?;
        let inherited = previous
            .and_then(|(payload,)| serde_json::from_str::<serde_json::Value>(&payload).ok())
            .and_then(|payload| {
                payload
                    .get("sets_flagged")
                    .and_then(serde_json::Value::as_bool)
            })
            .unwrap_or(false);
        let payload = serde_json::json!({
            "message_id": message_id,
            "folder_path": folder_path,
            "uid": uid,
            "remote_id": remote_id,
            "seen": seen,
            "flagged": flagged,
            "sets_flagged": sets_flagged || inherited,
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

    /// Поставить перемещение или безвозвратное удаление на сервере в очередь.
    /// Единственная точка постановки для действий пользователя: роль `delete`
    /// означает явное удаление навсегда и из переноса в корзину не получается
    /// (S-012, S-013).
    pub async fn queue_message_action(
        &self,
        message_ids: &[i64],
        target_role: &str,
    ) -> Result<QueuedAction> {
        let mut tx = self.begin_write().await?;
        let mut queued = QueuedAction::default();
        for message_id in message_ids {
            let locator: (i64, i64, i64, String, Option<String>) = sqlx::query_as(
                "SELECT m.account_id, m.folder_id, m.uid, f.remote_path, m.remote_id
                 FROM messages m JOIN folders f ON f.id=m.folder_id WHERE m.id=?",
            )
            .bind(message_id)
            .fetch_one(&mut *tx)
            .await?;
            let message = TakeawayMessage {
                id: *message_id,
                account_id: locator.0,
                folder_id: locator.1,
                uid: locator.2,
                remote_path: locator.3,
                remote_id: locator.4,
            };
            if target_role != "delete"
                && message_traits_at_risk(&mut tx, message.id, message.account_id).await?
            {
                queued.traits_at_risk += 1;
            }
            let target = if target_role == "delete" {
                TakeawayTarget::Delete
            } else {
                // Роль могла быть не проставлена при первом обходе папок:
                // выводим её из имени и пути, как это делалось и раньше.
                infer_missing_folder_role(&mut tx, message.account_id, target_role).await?;
                // S-046: папка роли должна быть ровно одна - при двух папках
                // одной роли письмо ушло бы в произвольную из них.
                match resolve_role_folder(&mut tx, message.account_id, target_role).await? {
                    Some((id, path)) => TakeawayTarget::Folder { id, path },
                    None => {
                        queued.skipped += 1;
                        queued.skipped_no_folder += 1;
                        continue;
                    }
                }
            };
            // Ручное перемещение доступно очереди через 10 секунд: это окно
            // отмены в интерфейсе.
            let outcome =
                queue_takeaway_operation(&mut tx, &message, &target, TakeawayActor::User, None, 10)
                    .await?;
            queued.account(outcome);
        }
        tx.commit().await?;
        Ok(queued)
    }

    /// Поставить перемещение в явно выбранную папку.
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
        let mut queued = QueuedAction::default();
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
            let message = TakeawayMessage {
                id: *message_id,
                account_id: locator.0,
                folder_id: locator.1,
                uid: locator.2,
                remote_path: locator.3,
                remote_id: locator.4,
            };
            if message_traits_at_risk(&mut tx, message.id, message.account_id).await? {
                queued.traits_at_risk += 1;
            }
            let folder = TakeawayTarget::Folder {
                id: target_folder_id,
                path: target.1.clone(),
            };
            let outcome =
                queue_takeaway_operation(&mut tx, &message, &folder, TakeawayActor::User, None, 10)
                    .await?;
            queued.account(outcome);
        }
        tx.commit().await?;
        Ok(queued)
    }

    /// Повторить операцию, дошедшую до состояния отказа (S-053).
    pub async fn retry_failed_operation(&self, operation_id: i64) -> Result<()> {
        let mut tx = self.begin_write().await?;
        // Команды выхода из состояния отказа касаются только увода: операцию
        // отметки признаков или дозаписи копии через них трогать нельзя.
        let changed = sqlx::query(
            "UPDATE outbox_ops SET status='retry', attempts=0, last_error=NULL,
                    next_attempt_at=datetime('now')
              WHERE id=? AND status='failed' AND op_kind IN ('move','delete')",
        )
        .bind(operation_id)
        .execute(&mut *tx)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(crate::Error::Other(
                "операция не найдена или не в состоянии отказа".into(),
            ));
        }
        clear_stage_result_of_operation(&mut tx, operation_id).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Отказаться от операции в состоянии отказа: только после этого по письму
    /// можно поставить новую операцию увода (S-053).
    pub async fn discard_failed_operation(&self, operation_id: i64) -> Result<()> {
        let mut tx = self.begin_write().await?;
        // Признак стадии снимается до удаления записи: после удаления по ней
        // уже не найти письмо.
        clear_stage_result_of_operation(&mut tx, operation_id).await?;
        let changed = sqlx::query(
            "DELETE FROM outbox_ops
              WHERE id=? AND status='failed' AND op_kind IN ('move','delete')",
        )
        .bind(operation_id)
        .execute(&mut *tx)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(crate::Error::Other(
                "операция не найдена или не в состоянии отказа".into(),
            ));
        }
        tx.commit().await?;
        Ok(())
    }

    /// Операции писем, дошедшие до состояния отказа: их показывает отчёт
    /// правила и список правил (S-052).
    pub async fn failed_takeaway_operations(&self) -> Result<Vec<FailedOperation>> {
        let rows: Vec<FailedOperation> = sqlx::query_as(
            "SELECT o.id, o.message_id, o.op_kind, o.last_error
               FROM outbox_ops o
              WHERE o.status='failed' AND o.op_kind IN ('move','delete')
              ORDER BY o.id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
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
            // undo-send.md S-025: операции отправки в эту пачку не входят - их
            // работник берёт по одной и переводит в состояние передачи прямо
            // перед обращением к серверу, иначе последнее письмо пачки
            // показывалось бы начавшим передачу и его отмена отклонялась бы
            // сообщением, не соответствующим действительности.
            "UPDATE outbox_ops SET status='processing', next_attempt_at=datetime('now','+2 minutes')
             WHERE id IN (
               SELECT id FROM outbox_ops
                WHERE account_id=? AND op_kind<>'send'
                  AND status IN ('pending','retry','processing')
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

    pub async fn complete_outbox_operation(&self, operation: &OutboxOperation) -> Result<()> {
        // undo-send.md S-010: большие объекты письма освобождаются только после
        // успеха операции. До него они нужны и самой отправке, и возврату
        // отменённого письма в композер.
        let released = if matches!(operation.op_kind.as_str(), "send" | "append_sent") {
            serde_json::from_str::<crate::model::SendPayload>(&operation.payload)
                .map(|payload| payload.blob_refs())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let mut tx = self.begin_write().await?;
        if matches!(operation.op_kind.as_str(), "move" | "delete")
            && let Some(message_id) = operation.message_id
        {
            // Безвозвратное удаление уносит дело вместе со строкой письма
            // (flag-due-dates.md, S-088), а перенос обязан его сохранить.
            // Отказ переноса примет не отменяет завершения операции: письмо на
            // сервере уже переехало, и откат вернул бы строку письма на старое
            // место, а операцию оставил бы в состоянии выполнения навсегда.
            if operation.op_kind == "move"
                && let Err(error) =
                    preserve_message_traits(&mut tx, message_id, &self.limit_set()).await
            {
                tracing::warn!(
                    message_id,
                    error = %crate::logging::mask_error_text(&error.to_string()),
                    "приметы письма не перенесены на новое место"
                );
            }
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
        for reference in released {
            let _ = self.blobs.remove(&reference);
        }
        Ok(())
    }

    pub async fn fail_outbox_operation(&self, id: i64, error: &str) -> Result<()> {
        let attempts: (i64,) = sqlx::query_as("SELECT attempts+1 FROM outbox_ops WHERE id=?")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        let delay = (5_i64.saturating_mul(1_i64 << attempts.0.min(9))).min(3600);
        // Сколько раз повторять действие - настройка: на неустойчивой связи
        // восьми попыток мало, а на устойчивой сбой означает отказ сервера, и
        // повторять его восемь раз незачем.
        let status = if attempts.0 >= self.limit(LIMIT_OPERATION_ATTEMPTS) {
            "failed"
        } else {
            "retry"
        };
        // S-084: поле последней ошибки названо среди маскируемого, а сервер
        // охотно повторяет в ней адрес письма.
        let reason = crate::logging::mask_error_text(error);
        let mut tx = self.begin_write().await?;
        sqlx::query(
            "UPDATE outbox_ops SET attempts=?, last_error=?, status=?,
                    next_attempt_at=datetime('now', ?)
             WHERE id=?",
        )
        .bind(attempts.0)
        .bind(reason.chars().take(1000).collect::<String>())
        .bind(status)
        .bind(format!("+{delay} seconds"))
        .bind(id)
        .execute(&mut *tx)
        .await?;
        if status == "failed" {
            // out-of-office.md S-048: окончательно отказавший автоответ
            // отправленным не считается. Иначе окно молчания на семь суток
            // закрывало бы адресата, так и не получившего ни одного ответа.
            sqlx::query("UPDATE out_of_office_replies SET state='failed' WHERE operation_id=?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn list_quick_steps(&self) -> Result<Vec<QuickStep>> {
        let rows: Vec<QuickStepRow> = sqlx::query_as(
            "SELECT id,name,icon,sort_order,hotkey_slot,state
               FROM quick_steps ORDER BY sort_order,id",
        )
        .fetch_all(&self.pool)
        .await?;
        let action_rows: Vec<QuickStepActionRow> = sqlx::query_as(
            "SELECT quick_step_id,kind,folder_id,folder_role,label_id
               FROM quick_step_actions ORDER BY quick_step_id,sort_order,id",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut actions = std::collections::HashMap::<i64, Vec<MailRuleAction>>::new();
        for row in action_rows {
            actions
                .entry(row.quick_step_id)
                .or_default()
                .push(MailRuleAction {
                    kind: row.kind,
                    folder_id: row.folder_id,
                    folder_role: row.folder_role,
                    label_id: row.label_id,
                });
        }
        Ok(rows
            .into_iter()
            .map(|row| QuickStep {
                id: row.id,
                name: row.name,
                icon: row.icon,
                sort_order: row.sort_order,
                hotkey_slot: row.hotkey_slot,
                state: row.state,
                actions: actions.remove(&row.id).unwrap_or_default(),
            })
            .collect())
    }

    pub async fn save_quick_step(&self, input: &QuickStepInput) -> Result<i64> {
        validate_quick_step_input(input, &self.limit_set()).map_err(crate::Error::Other)?;
        let mut tx = self.begin_write().await?;
        if input.id.is_none() {
            let count: (i64,) = sqlx::query_as("SELECT count(*) FROM quick_steps")
                .fetch_one(&mut *tx)
                .await?;
            let max_steps = self.limit(LIMIT_QUICK_STEPS);
            if count.0 >= max_steps {
                return Err(crate::Error::Other(format!(
                    "достигнут предел в {max_steps} быстрых действий"
                )));
            }
        }
        // Слот пишется здесь же, и без проверки занятости пользователь получал
        // бы сырую ошибку уникального индекса базы (S-066).
        if let Some(slot) = input.hotkey_slot {
            if !(1..=10).contains(&slot) {
                return Err(crate::Error::Other(
                    "номер слота должен быть от 1 до 10".into(),
                ));
            }
            let occupied: Option<(String,)> =
                sqlx::query_as("SELECT name FROM quick_steps WHERE hotkey_slot=? AND id IS NOT ?")
                    .bind(slot)
                    .bind(input.id)
                    .fetch_optional(&mut *tx)
                    .await?;
            if let Some((name,)) = occupied {
                return Err(crate::Error::Other(format!(
                    "слот занят быстрым действием \"{name}\""
                )));
            }
        }
        // Цель действия сверяется с настоящими папками и метками: номер, к
        // которому ничего не ведёт, иначе унёс бы письма неизвестно куда
        // (S-012, S-043).
        for action in input.actions.iter() {
            if let Some(folder_id) = action.folder_id {
                let known: Option<(i64,)> = sqlx::query_as("SELECT id FROM folders WHERE id=?")
                    .bind(folder_id)
                    .fetch_optional(&mut *tx)
                    .await?;
                if known.is_none() {
                    return Err(crate::Error::Other(
                        "папка действия цепочки не найдена".into(),
                    ));
                }
            }
            if let Some(label_id) = action.label_id {
                let known: Option<(i64,)> = sqlx::query_as("SELECT id FROM labels WHERE id=?")
                    .bind(label_id)
                    .fetch_optional(&mut *tx)
                    .await?;
                if known.is_none() {
                    return Err(crate::Error::Other(
                        "метка действия цепочки не найдена".into(),
                    ));
                }
            }
        }
        let id = if let Some(id) = input.id {
            let changed = sqlx::query(
                "UPDATE quick_steps SET name=?,icon=?,sort_order=?,hotkey_slot=?,state='ok',
                        updated_at=datetime('now') WHERE id=?",
            )
            .bind(input.name.trim())
            .bind(input.icon.trim())
            .bind(input.sort_order)
            .bind(input.hotkey_slot)
            .bind(id)
            .execute(&mut *tx)
            .await?;
            if changed.rows_affected() != 1 {
                return Err(crate::Error::Other("быстрое действие не найдено".into()));
            }
            sqlx::query("DELETE FROM quick_step_actions WHERE quick_step_id=?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            id
        } else {
            sqlx::query(
                "INSERT INTO quick_steps(name,icon,sort_order,hotkey_slot,state)
                 VALUES(?,?,?,?,'ok')",
            )
            .bind(input.name.trim())
            .bind(input.icon.trim())
            .bind(input.sort_order)
            .bind(input.hotkey_slot)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid()
        };
        for (position, action) in input.actions.iter().enumerate() {
            sqlx::query(
                "INSERT INTO quick_step_actions(quick_step_id,sort_order,kind,folder_id,folder_role,label_id)
                 VALUES(?,?,?,?,?,?)",
            )
            .bind(id)
            .bind(position as i64)
            .bind(&action.kind)
            .bind(action.folder_id)
            .bind(&action.folder_role)
            .bind(action.label_id)
            .execute(&mut *tx)
            .await?;
        }
        // Состояние считается по фактическим целям той же неделимой операцией:
        // безусловное "в порядке" обещало бы запуск цепочке, которой некуда
        // уводить письма (S-047).
        sqlx::query(
            "UPDATE quick_steps SET state=CASE WHEN EXISTS(
                 SELECT 1 FROM quick_step_actions a
                  WHERE a.quick_step_id=quick_steps.id
                    AND ((a.kind='move' AND a.folder_id IS NULL AND a.folder_role IS NULL)
                      OR (a.kind IN ('label_add','label_remove') AND a.label_id IS NULL))
             ) THEN 'needs_attention' ELSE 'ok' END,
             updated_at=datetime('now') WHERE id=?",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(id)
    }

    pub async fn delete_quick_step(&self, id: i64) -> Result<bool> {
        Ok(sqlx::query("DELETE FROM quick_steps WHERE id=?")
            .bind(id)
            .execute(&self.write_pool)
            .await?
            .rows_affected()
            != 0)
    }

    pub async fn reorder_quick_steps(&self, ids: &[i64]) -> Result<()> {
        let mut tx = self.begin_write().await?;
        for (position, id) in ids.iter().enumerate() {
            sqlx::query(
                "UPDATE quick_steps SET sort_order=?,updated_at=datetime('now') WHERE id=?",
            )
            .bind(position as i64)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn bind_quick_step_slot(&self, id: i64, slot: Option<i64>) -> Result<()> {
        if slot.is_some_and(|value| !(1..=QUICK_STEP_SLOTS).contains(&value)) {
            return Err(crate::Error::Other(format!(
                "номер слота должен быть от 1 до {QUICK_STEP_SLOTS}"
            )));
        }
        let mut tx = self.begin_write().await?;
        if let Some(slot) = slot {
            let occupied: Option<(String,)> =
                sqlx::query_as("SELECT name FROM quick_steps WHERE hotkey_slot=? AND id<>?")
                    .bind(slot)
                    .bind(id)
                    .fetch_optional(&mut *tx)
                    .await?;
            if occupied.is_some() {
                return Err(crate::Error::Other(
                    "слот занят другим быстрым действием".into(),
                ));
            }
        }
        let changed = sqlx::query(
            "UPDATE quick_steps SET hotkey_slot=?,updated_at=datetime('now') WHERE id=?",
        )
        .bind(slot)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(crate::Error::Other("быстрое действие не найдено".into()));
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn apply_quick_step(
        &self,
        quick_step_id: i64,
        message_ids: &[i64],
    ) -> Result<QuickStepReport> {
        if message_ids.is_empty() {
            return Err(crate::Error::Other("письмо не выбрано".into()));
        }
        let max_messages = self.limit(LIMIT_QUICK_STEP_MESSAGES);
        if message_ids.len() as i64 > max_messages {
            return Err(crate::Error::Other(format!(
                "выберите не больше {max_messages} писем для одного запуска"
            )));
        }
        let step = self
            .list_quick_steps()
            .await?
            .into_iter()
            .find(|step| step.id == quick_step_id)
            .ok_or_else(|| crate::Error::Other("быстрое действие не найдено".into()))?;
        if step.state != "ok" {
            return Err(crate::Error::Other(
                "быстрое действие требует выбрать удалённую цель заново".into(),
            ));
        }
        let mut tx = self.begin_write().await?;
        let snapshots = load_rule_snapshots(
            &mut tx,
            &format!("m.id IN ({})", id_list(message_ids)),
            Vec::new(),
            max_messages,
        )
        .await?;
        if snapshots.len() != message_ids.len() {
            return Err(crate::Error::Other(
                "одно из выбранных писем не найдено".into(),
            ));
        }
        let mut report = QuickStepReport::default();
        for snapshot in &snapshots {
            if self
                .run_quick_step_chain(&mut tx, &step, snapshot, &mut report)
                .await?
            {
                report.skipped += 1;
            } else {
                report.applied += 1;
            }
        }
        tx.commit().await?;
        Ok(report)
    }

    /// Выполнить цепочку быстрого действия над одним письмом. Возвращает
    /// признак пропуска: письмо считается пропущенным один раз, каким бы
    /// действием цепочка ни оборвалась, а причина уходит в отчёт отдельным
    /// счётчиком.
    async fn run_quick_step_chain(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        step: &QuickStep,
        snapshot: &RuleMessageSnapshot,
        report: &mut QuickStepReport,
    ) -> Result<bool> {
        let mut flags = MessageFlags {
            seen: snapshot.seen,
            flagged: snapshot.flagged,
        };
        let mut skipped = false;
        for action in &step.actions {
            if is_local_action(&action.kind) {
                self.apply_local_action(tx, action, snapshot, &mut flags)
                    .await?;
                continue;
            }
            let target = match resolve_quick_step_target(tx, action, snapshot).await? {
                QuickStepTarget::Takeaway(target) => target,
                QuickStepTarget::ForeignAccount => {
                    report.skipped_foreign_account += 1;
                    return Ok(true);
                }
                QuickStepTarget::NoFolder => {
                    report.skipped_no_folder += 1;
                    return Ok(true);
                }
            };
            if !matches!(target, TakeawayTarget::Delete)
                && message_traits_at_risk(tx, snapshot.id, snapshot.account_id).await?
            {
                report.traits_at_risk += 1;
            }
            match queue_takeaway_operation(
                tx,
                &snapshot.takeaway_message(),
                &target,
                TakeawayActor::User,
                None,
                10,
            )
            .await?
            {
                TakeawayOutcome::Queued(id) => report.operation_ids.push(id),
                TakeawayOutcome::Busy | TakeawayOutcome::Conflict => {
                    report.skipped_busy += 1;
                    skipped = true;
                }
                TakeawayOutcome::Failed => {
                    report.skipped_failed += 1;
                    skipped = true;
                }
                TakeawayOutcome::NeedsAttention => {
                    report.skipped_no_folder += 1;
                    skipped = true;
                }
                TakeawayOutcome::Unchanged => {}
            }
        }
        Ok(skipped)
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

    /// Перенести правила прежней схемы в группы и действия (S-074 - S-077).
    /// Прикладной шаг, а не часть структурной миграции: он идёт после неё и
    /// повторный запуск ничего не меняет, потому что переносятся только
    /// правила, у которых ещё нет ни одного условия.
    pub async fn migrate_mail_rules_to_groups(&self) -> Result<usize> {
        let legacy: Vec<LegacyMailRuleRow> = sqlx::query_as(
            "SELECT id, field, operator, value, action, folder_id, label_id
                 FROM mail_rules r
                 WHERE NOT EXISTS (SELECT 1 FROM mail_rule_conditions c WHERE c.rule_id=r.id)
                   AND NOT EXISTS (SELECT 1 FROM mail_rule_actions a WHERE a.rule_id=r.id)",
        )
        .fetch_all(&self.pool)
        .await?;
        if legacy.is_empty() {
            return Ok(0);
        }
        let mut tx = self.begin_write().await?;
        let moved = legacy.len();
        for rule in legacy {
            let id = rule.id;
            sqlx::query(
                "INSERT INTO mail_rule_conditions(
                    rule_id, is_exception, group_index, group_logic, position, field, op, value
                 ) VALUES(?, 0, 0, 'all', 0, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&rule.field)
            .bind(&rule.operator)
            .bind(&rule.value)
            .execute(&mut *tx)
            .await?;
            let kind = if rule.action == "label" {
                "label_add"
            } else {
                rule.action.as_str()
            };
            sqlx::query(
                "INSERT INTO mail_rule_actions(rule_id, position, kind, folder_id, label_id)
                 VALUES(?, 0, ?, ?, ?)",
            )
            .bind(&id)
            .bind(kind)
            .bind(rule.folder_id)
            .bind(rule.label_id)
            .execute(&mut *tx)
            .await?;
            // S-076: прежняя схема применяла к письму только первое подходящее
            // правило, поэтому перенесённое правило заканчивается остановкой -
            // иначе почта после обновления разложилась бы иначе.
            sqlx::query(
                "INSERT INTO mail_rule_actions(rule_id, position, kind) VALUES(?, 1, 'stop')",
            )
            .bind(&id)
            .execute(&mut *tx)
            .await?;
            // Прежний столбец папки объявлен с каскадным удалением, поэтому
            // удаление папки снесло бы всё правило вместо перевода его в
            // состояние внимания. Номер папки уже перенесён в действие.
            sqlx::query("UPDATE mail_rules SET folder_id=NULL WHERE id=?")
                .bind(&id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(moved)
    }

    /// Правила с группами, исключениями, действиями и состоянием.
    pub async fn list_mail_rules(&self) -> Result<Vec<MailRule>> {
        let rows: Vec<MailRuleRow> = sqlx::query_as(MAIL_RULES_SQL).fetch_all(&self.pool).await?;
        let conditions: Vec<MailRuleConditionRow> = sqlx::query_as(MAIL_RULE_CONDITIONS_SQL)
            .fetch_all(&self.pool)
            .await?;
        let actions: Vec<MailRuleActionRow> = sqlx::query_as(MAIL_RULE_ACTIONS_SQL)
            .fetch_all(&self.pool)
            .await?;
        // Роли папок читаем один раз: состояние правила с ролью зависит от
        // того, ровно ли одна такая папка в ящике (S-046).
        let folder_roles: Vec<(i64, Option<String>)> = sqlx::query_as(FOLDER_ROLES_SQL)
            .fetch_all(&self.pool)
            .await?;
        Ok(assemble_mail_rules(
            rows,
            conditions,
            actions,
            &folder_roles,
        ))
    }

    /// Сохранить правило целиком. Проверки состава выполняются до открытия
    /// транзакции, поэтому отказ не создаёт и не изменяет ни одной записи.
    pub async fn save_mail_rule(
        &self,
        rule: &MailRuleInput,
        apply_existing: bool,
        known_rule_ids: Option<&[String]>,
    ) -> Result<MailRule> {
        validate_rule_input(rule, &self.limit_set()).map_err(crate::Error::AccountConfig)?;
        if rule.actions.iter().any(|action| action.kind == "delete") {
            // S-048: удаление навсегда не имеет отмены, поэтому подтверждение
            // привязано к составу правила и принимается ровно один раз.
            let expected = delete_forever_fingerprint(rule);
            let provided = rule.confirm_key.clone().unwrap_or_default();
            if provided != expected || !self.take_delete_confirmation(&expected).await {
                return Err(crate::Error::AccountConfig(
                    "удаление навсегда нужно подтвердить заново".into(),
                ));
            }
        }
        let mut tx = self.begin_write().await?;
        if let Some(known) = known_rule_ids {
            let current: Vec<(String,)> =
                sqlx::query_as("SELECT id FROM mail_rules ORDER BY sort_order, created_at, id")
                    .fetch_all(&mut *tx)
                    .await?;
            let current: Vec<String> = current.into_iter().map(|(id,)| id).collect();
            let expected: Vec<&String> = known.iter().filter(|id| **id != rule.id).collect();
            let actual: Vec<&String> = current.iter().filter(|id| **id != rule.id).collect();
            if expected != actual {
                return Err(crate::Error::AccountConfig(
                    "список правил изменился, откройте его заново".into(),
                ));
            }
        }
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
        for action in &rule.actions {
            self.check_rule_action_targets(&mut tx, rule, action)
                .await?;
        }
        let existing: Option<(i64, i64, i64)> = sqlx::query_as(
            "SELECT progress_message_id, sort_order, rule_version FROM mail_rules WHERE id=?",
        )
        .bind(&rule.id)
        .fetch_optional(&mut *tx)
        .await?;
        let progress = if apply_existing {
            0
        } else if let Some((progress, _, _)) = existing {
            progress
        } else {
            sqlx::query_as::<_, (i64,)>("SELECT coalesce(max(id), 0) FROM messages")
                .fetch_one(&mut *tx)
                .await?
                .0
        };
        let sort_order = if let Some((_, sort_order, _)) = existing {
            sort_order
        } else {
            sqlx::query_as::<_, (i64,)>("SELECT coalesce(max(sort_order), -1)+1 FROM mail_rules")
                .fetch_one(&mut *tx)
                .await?
                .0
        };
        // Версия растёт при каждом сохранении: по ней задание ручного прогона
        // узнаёт, что правило изменилось уже после его начала (S-071).
        let version = existing.map(|(_, _, version)| version + 1).unwrap_or(1);
        let legacy = legacy_rule_columns(rule);
        sqlx::query(
            "INSERT INTO mail_rules(
                id, name, field, operator, value, account_id, action, folder_id, label_id,
                enabled, progress_message_id, sort_order, rule_version
             ) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                name=excluded.name, field=excluded.field, operator=excluded.operator,
                value=excluded.value, account_id=excluded.account_id,
                action=excluded.action, folder_id=excluded.folder_id, label_id=excluded.label_id,
                enabled=excluded.enabled, progress_message_id=excluded.progress_message_id,
                rule_version=excluded.rule_version, updated_at=datetime('now')",
        )
        .bind(&rule.id)
        .bind(rule.name.trim())
        .bind(legacy.field)
        .bind(legacy.operator)
        .bind(legacy.value)
        .bind(rule.account_id)
        .bind(legacy.action)
        .bind(legacy.folder_id)
        .bind(legacy.label_id)
        .bind(rule.enabled)
        .bind(progress)
        .bind(sort_order)
        .bind(version)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM mail_rule_conditions WHERE rule_id=?")
            .bind(&rule.id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM mail_rule_actions WHERE rule_id=?")
            .bind(&rule.id)
            .execute(&mut *tx)
            .await?;
        for (is_exception, groups) in [(0_i64, &rule.groups), (1_i64, &rule.exceptions)] {
            for (group_index, group) in groups.iter().enumerate() {
                for (position, condition) in group.conditions.iter().enumerate() {
                    sqlx::query(
                        "INSERT INTO mail_rule_conditions(
                            rule_id, is_exception, group_index, group_logic, position,
                            field, op, value, unit, value2
                         ) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    )
                    .bind(&rule.id)
                    .bind(is_exception)
                    .bind(group_index as i64)
                    .bind(&group.logic)
                    .bind(position as i64)
                    .bind(&condition.field)
                    .bind(&condition.op)
                    .bind(condition.value.trim())
                    .bind(condition.unit.as_deref())
                    .bind(condition.value2.as_deref())
                    .execute(&mut *tx)
                    .await?;
                }
            }
        }
        for (position, action) in rule.actions.iter().enumerate() {
            sqlx::query(
                "INSERT INTO mail_rule_actions(rule_id, position, kind, folder_id, folder_role, label_id)
                 VALUES(?, ?, ?, ?, ?, ?)",
            )
            .bind(&rule.id)
            .bind(position as i64)
            .bind(&action.kind)
            .bind(action.folder_id)
            .bind(action.folder_role.as_deref())
            .bind(action.label_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.list_mail_rules()
            .await?
            .into_iter()
            .find(|saved| saved.id == rule.id)
            .ok_or_else(|| crate::Error::Other("сохранённое правило не найдено".into()))
    }

    /// Папка и метка действия должны существовать и принадлежать области
    /// правила: иначе правило сразу оказалось бы в состоянии внимания.
    async fn check_rule_action_targets(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        rule: &MailRuleInput,
        action: &MailRuleAction,
    ) -> Result<()> {
        if let Some(label_id) = action.label_id {
            let exists: Option<(i64,)> = sqlx::query_as("SELECT id FROM labels WHERE id=?")
                .bind(label_id)
                .fetch_optional(&mut **tx)
                .await?;
            if exists.is_none() {
                return Err(crate::Error::AccountConfig(
                    "метка правила не найдена".into(),
                ));
            }
        }
        if action.kind != "move" {
            return Ok(());
        }
        if let Some(folder_id) = action.folder_id {
            // S-045: номер папки допустим только у правила одного ящика, иначе
            // правило переносило бы письма между ящиками.
            let account_id = rule.account_id.ok_or_else(|| {
                crate::Error::AccountConfig(
                    "для перемещения в конкретную папку выберите ящик или укажите тип папки".into(),
                )
            })?;
            let target: Option<(i64,)> =
                sqlx::query_as("SELECT id FROM folders WHERE id=? AND account_id=?")
                    .bind(folder_id)
                    .bind(account_id)
                    .fetch_optional(&mut **tx)
                    .await?;
            if target.is_none() {
                return Err(crate::Error::AccountConfig(
                    "папка назначения не принадлежит аккаунту правила".into(),
                ));
            }
        } else if let Some(role) = action.folder_role.as_deref()
            && !matches!(role, "inbox" | "archive" | "spam" | "trash")
        {
            return Err(crate::Error::AccountConfig(
                "тип папки назначения не поддерживается".into(),
            ));
        }
        Ok(())
    }

    /// Выдать одноразовый ключ подтверждения удаления навсегда (S-048).
    pub async fn issue_delete_confirmation(&self, rule: &MailRuleInput) -> Result<String> {
        let key = delete_forever_fingerprint(rule);
        self.delete_confirmations.lock().await.insert(key.clone());
        Ok(key)
    }

    async fn take_delete_confirmation(&self, key: &str) -> bool {
        self.delete_confirmations.lock().await.remove(key)
    }

    pub async fn set_mail_rule_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        // S-060: переключатель списка меняет только признак включения.
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

    /// Переставить правила: перечень приходит целиком в новом порядке, а
    /// `sort_order` становится последовательными числами от нуля (S-059).
    pub async fn reorder_mail_rules(&self, ids: &[String]) -> Result<()> {
        let mut tx = self.begin_write().await?;
        let current: Vec<(String,)> = sqlx::query_as("SELECT id FROM mail_rules")
            .fetch_all(&mut *tx)
            .await?;
        let mut current: Vec<String> = current.into_iter().map(|(id,)| id).collect();
        let mut wanted = ids.to_vec();
        current.sort();
        wanted.sort();
        if current != wanted {
            return Err(crate::Error::AccountConfig(
                "список правил изменился, откройте его заново".into(),
            ));
        }
        for (position, id) in ids.iter().enumerate() {
            sqlx::query(
                "UPDATE mail_rules SET sort_order=?, updated_at=datetime('now') WHERE id=?",
            )
            .bind(position as i64)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn delete_mail_rule(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM mail_rules WHERE id=?")
            .bind(id)
            .execute(&self.write_pool)
            .await?;
        Ok(())
    }

    /// Ящики, у которых не назначена папка с ролью корзины (S-014).
    pub async fn accounts_without_trash(&self) -> Result<Vec<i64>> {
        let rows: Vec<(i64,)> = sqlx::query_as(
            "SELECT a.id FROM accounts a
              WHERE a.enabled=1
                AND NOT EXISTS (SELECT 1 FROM folders f WHERE f.account_id=a.id AND f.role='trash')
              ORDER BY a.id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Единая точка стадий обработки новых писем (S-001). Сквозной порядок
    /// стадий - списки отправителей, игнорируемые переписки, автоочистка по
    /// отправителю и только потом правила. Письмо, уведённое любой стадией,
    /// в следующие стадии не идёт: признак закрывшей стадии стоит в его строке
    /// и отбор каждой следующей стадии его не берёт (S-002, S-009).
    pub async fn process_sync_batch_stages(&self) -> Result<usize> {
        let mut taken = self.process_sender_policy_stage().await?;
        taken += self.process_ignored_conversation_stage().await?;
        taken += self.process_sender_sweep_stage().await?;
        taken += self.process_mail_rules().await?;
        // out-of-office.md S-030: локальный автоответ - пятая стадия, после
        // правил обработки. Письмо, закрытое предшествующей стадией, до неё не
        // доходит вовсе.
        self.process_out_of_office_stage().await?;
        // Незавершённые уборки продвигаются тем же конвейером: иначе они
        // стояли бы до следующего действия пользователя.
        self.advance_sender_policy_jobs().await?;
        self.advance_ignored_conversation_jobs().await?;
        self.advance_sender_sweep_jobs().await?;
        Ok(taken)
    }

    /// Стадии пути догрузки прокруткой (S-063): игнорируемые переписки и
    /// автоочистка по отправителю. Списки отправителей и правила по таким
    /// письмам не выполняются намеренно - правила не должны срабатывать на
    /// старую переписку, поднятую прокруткой, а списки отправителей заданы
    /// адресом, а не конкретной перепиской.
    pub async fn process_backfill_stages(&self) -> Result<usize> {
        let mut taken = self.process_ignored_conversation_stage().await?;
        taken += self.process_sender_sweep_stage().await?;
        Ok(taken)
    }

    /// Автоматический прогон правил по новым письмам. Поставленные операции,
    /// изменения локальной базы и новый прогресс правил записываются одной
    /// неделимой операцией (S-057).
    pub async fn process_mail_rules(&self) -> Result<usize> {
        // S-057: правила и граница пачки читаются уже внутри неделимой
        // операции. Прочитанные до неё, они дали бы двум одновременным
        // прогонам одну пачку писем и повтор местных действий.
        let mut tx = self.begin_write().await?;
        let rules: Vec<MailRule> = load_mail_rules_in_tx(&mut tx)
            .await?
            .into_iter()
            .filter(|rule| rule.enabled && rule.state == "ok")
            .collect();
        let Some(min_progress) = rules.iter().map(|rule| rule.progress_message_id).min() else {
            return Ok(0);
        };
        // S-061 - S-064: пачка не больше 500 писем, только рабочие папки, без
        // догруженных прокруткой писем и без писем, закрытых прежней стадией.
        let snapshots = load_rule_snapshots(
            &mut tx,
            "m.id>? AND m.backfilled=0 AND m.closed_by_stage IS NULL
               AND (f.role IS NULL OR f.role NOT IN ('sent','drafts','archive','spam','trash'))",
            vec![min_progress],
            500,
        )
        .await?;
        let mut progress: Vec<i64> = rules.iter().map(|rule| rule.progress_message_id).collect();
        let mut counters = RuleRunCounters::default();
        for snapshot in &snapshots {
            self.apply_rules_to_message(
                &mut tx,
                &rules,
                snapshot,
                RuleRunMode::Automatic,
                &mut counters,
            )
            .await?;
            for slot in progress.iter_mut() {
                if snapshot.id > *slot {
                    *slot = snapshot.id;
                }
            }
        }
        for (index, rule) in rules.iter().enumerate() {
            sqlx::query(
                "UPDATE mail_rules SET progress_message_id=?, updated_at=datetime('now') WHERE id=?",
            )
            .bind(progress[index])
            .bind(&rule.id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(counters.applied as usize)
    }

    /// Применить все подходящие правила к одному письму по его снимку
    /// (S-031 - S-034).
    async fn apply_rules_to_message(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        rules: &[MailRule],
        snapshot: &RuleMessageSnapshot,
        mode: RuleRunMode,
        counters: &mut RuleRunCounters,
    ) -> Result<()> {
        // S-002: письмо с уже поставленной операцией увода получает только
        // местные действия последующих правил.
        let mut taken = snapshot.has_takeaway;
        // S-031: снимок остаётся неизменным и служит только сверке условий, а
        // действия ведут собственное состояние признаков. Иначе вторая отметка
        // цепочки брала бы первый признак из снимка и отменяла бы первую.
        let mut flags = MessageFlags {
            seen: snapshot.seen,
            flagged: snapshot.flagged,
        };
        // S-073: письмо считается пропущенным один раз, даже если уводящих
        // действий у подходящих правил несколько.
        let mut skipped_counted = false;
        for rule in rules {
            if rule.account_id.is_some_and(|id| id != snapshot.account_id) {
                continue;
            }
            // Каждое правило двигает собственный прогресс. Без этой проверки
            // сохранение одного правила с отметкой "применить к уже
            // загруженным" опускает границу пачки, и остальные правила заново
            // разбирают всю историю писем.
            if mode == RuleRunMode::Automatic && snapshot.id <= rule.progress_message_id {
                continue;
            }
            if !rule_matches(rule, snapshot) {
                continue;
            }
            counters.applied += 1;
            let mut stop = false;
            for action in &rule.actions {
                if action.kind == "stop" {
                    stop = true;
                    break;
                }
                if is_takeaway_action(&action.kind) {
                    if taken {
                        // S-073: уводящее действие не выполнено, потому что
                        // письмо уже уведено - это пропуск, а не тишина.
                        if !skipped_counted {
                            counters.skipped += 1;
                            skipped_counted = true;
                        }
                        continue;
                    }
                    match self.apply_takeaway(tx, rule, action, snapshot).await? {
                        TakeawayOutcome::Queued(_) => {
                            taken = true;
                            counters.queued += 1;
                        }
                        // S-005: занятое письмо пропускается со счётчиком,
                        // прогон при этом не останавливается.
                        TakeawayOutcome::Conflict
                        | TakeawayOutcome::Busy
                        | TakeawayOutcome::Failed => {
                            taken = true;
                            if !skipped_counted {
                                counters.skipped += 1;
                                skipped_counted = true;
                            }
                        }
                        TakeawayOutcome::Unchanged => taken = true,
                        TakeawayOutcome::NeedsAttention => {}
                    }
                } else if is_local_action(&action.kind) {
                    self.apply_local_action(tx, action, snapshot, &mut flags)
                        .await?;
                }
            }
            if stop {
                break;
            }
        }
        Ok(())
    }

    /// Местные действия меняют локальную базу сразу и повторяются без вреда
    /// (S-041 - S-043).
    async fn apply_local_action(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        action: &MailRuleAction,
        snapshot: &RuleMessageSnapshot,
        flags: &mut MessageFlags,
    ) -> Result<()> {
        match action.kind.as_str() {
            "label_add" => {
                let Some(label_id) = action.label_id else {
                    return Ok(());
                };
                sqlx::query(
                    "INSERT OR IGNORE INTO message_labels(message_id, label_id) VALUES(?, ?)",
                )
                .bind(snapshot.id)
                .bind(label_id)
                .execute(&mut **tx)
                .await?;
            }
            "label_remove" => {
                let Some(label_id) = action.label_id else {
                    return Ok(());
                };
                sqlx::query("DELETE FROM message_labels WHERE message_id=? AND label_id=?")
                    .bind(snapshot.id)
                    .bind(label_id)
                    .execute(&mut **tx)
                    .await?;
            }
            "mark_read" | "mark_flagged" => {
                // S-042: оба признака пишутся присвоением, поэтому повтор
                // действия ничего не меняет. Второй признак берётся из
                // текущего состояния письма, а не из снимка: иначе цепочка
                // "пометить прочитанным, затем важным" сняла бы прочтение.
                if action.kind == "mark_read" {
                    flags.seen = true;
                } else {
                    flags.flagged = true;
                    // И правило, и быстрое действие используют этот путь:
                    // прежнее выполненное или отсоединённое дело возвращается
                    // в работу теми же переходами, что и ручная отметка.
                    apply_task_flag_transition(tx, snapshot.id, FlagChangeReason::Rule, true)
                        .await?;
                }
                let (seen, flagged) = (flags.seen, flags.flagged);
                sqlx::query("UPDATE messages SET seen=?, flagged=? WHERE id=?")
                    .bind(seen as i64)
                    .bind(flagged as i64)
                    .bind(snapshot.id)
                    .execute(&mut **tx)
                    .await?;
                // S-043: признак нужно перенести и на сервер, поэтому ставим
                // ту же операцию очереди, что и ручная отметка.
                Self::queue_flag_sync(
                    tx,
                    snapshot.id,
                    snapshot.account_id,
                    &snapshot.remote_path,
                    snapshot.uid,
                    snapshot.remote_id.as_deref(),
                    seen,
                    flagged,
                    // Правило и быстрое действие меняют именно важность.
                    true,
                )
                .await?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Уводящее действие всегда идёт через общую точку постановки операции.
    async fn apply_takeaway(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        rule: &MailRule,
        action: &MailRuleAction,
        snapshot: &RuleMessageSnapshot,
    ) -> Result<TakeawayOutcome> {
        let target = if action.kind == "delete" {
            TakeawayTarget::Delete
        } else if action.kind == "move" {
            match action.folder_id {
                Some(folder_id) => {
                    let folder = sqlx::query_as::<_, (i64, String)>(
                        "SELECT id, remote_path FROM folders WHERE id=? AND account_id=?",
                    )
                    .bind(folder_id)
                    .bind(snapshot.account_id)
                    .fetch_optional(&mut **tx)
                    .await?;
                    match folder {
                        Some((id, path)) => TakeawayTarget::Folder { id, path },
                        None => return Ok(TakeawayOutcome::NeedsAttention),
                    }
                }
                None => {
                    let Some(role) = action.folder_role.as_deref() else {
                        return Ok(TakeawayOutcome::NeedsAttention);
                    };
                    match resolve_role_folder(tx, snapshot.account_id, role).await? {
                        Some((id, path)) => TakeawayTarget::Folder { id, path },
                        None => return Ok(TakeawayOutcome::NeedsAttention),
                    }
                }
            }
        } else {
            let Some(role) = takeaway_target_role(&action.kind) else {
                return Ok(TakeawayOutcome::NeedsAttention);
            };
            match resolve_role_folder(tx, snapshot.account_id, role).await? {
                Some((id, path)) => TakeawayTarget::Folder { id, path },
                // S-046: роль без единственной папки останавливает только это
                // правило, а не разбор всей пачки.
                None => return Ok(TakeawayOutcome::NeedsAttention),
            }
        };
        let outcome = queue_takeaway_operation(
            tx,
            &snapshot.takeaway_message(),
            &target,
            TakeawayActor::Stage,
            Some(rule.id.as_str()),
            0,
        )
        .await?;
        if matches!(outcome, TakeawayOutcome::Queued(_)) {
            // S-009: письмо закрыто стадией правил и дальше не передаётся.
            sqlx::query("UPDATE messages SET closed_by_stage=? WHERE id=?")
                .bind(RULES_STAGE_NAME)
                .bind(snapshot.id)
                .execute(&mut **tx)
                .await?;
        }
        Ok(outcome)
    }

    /// Начать ручной прогон по выбранным папкам (S-065, S-066).
    pub async fn start_mail_rule_run(
        &self,
        account_id: Option<i64>,
        folder_ids: &[i64],
        rule_ids: Option<&[String]>,
    ) -> Result<MailRuleRunReport> {
        if folder_ids.is_empty() {
            return Err(crate::Error::AccountConfig(
                "для ручного запуска выберите хотя бы одну папку".into(),
            ));
        }
        for folder_id in folder_ids {
            let folder: Option<(i64, Option<String>)> =
                sqlx::query_as("SELECT account_id, role FROM folders WHERE id=?")
                    .bind(folder_id)
                    .fetch_optional(&self.pool)
                    .await?;
            let Some((folder_account, role)) = folder else {
                return Err(crate::Error::AccountConfig("папка не найдена".into()));
            };
            if account_id.is_some_and(|id| id != folder_account) {
                return Err(crate::Error::AccountConfig(
                    "папка принадлежит другому аккаунту".into(),
                ));
            }
            // Служебные папки не считаются рабочими ни в одной стадии: прогон
            // по отправленным или черновикам увёл бы почту, которую
            // пользователь никуда не отправлял.
            if role
                .as_deref()
                .is_some_and(|role| matches!(role, "sent" | "drafts" | "spam" | "trash"))
            {
                return Err(crate::Error::AccountConfig(
                    "правила не запускаются по отправленным, черновикам, спаму и корзине".into(),
                ));
            }
        }
        let rules: Vec<MailRule> = self
            .list_mail_rules()
            .await?
            .into_iter()
            .filter(|rule| rule.enabled && rule.state == "ok")
            .filter(|rule| rule_ids.is_none_or(|ids| ids.contains(&rule.id)))
            .collect();
        if rules.is_empty() {
            return Err(crate::Error::AccountConfig(
                "нет включённых правил, готовых к прогону".into(),
            ));
        }
        let versions: Vec<serde_json::Value> = rules
            .iter()
            .map(|rule| serde_json::json!({"id": rule.id, "version": rule.version}))
            .collect();
        let border: (i64,) = sqlx::query_as("SELECT coalesce(max(id), 0) FROM messages")
            .fetch_one(&self.pool)
            .await?;
        let run_id: (i64,) = sqlx::query_as(
            "INSERT INTO mail_rule_runs(folder_ids, rule_versions, max_message_id, state)
             VALUES(?, ?, ?, 'running') RETURNING id",
        )
        .bind(serde_json::to_string(folder_ids).unwrap_or_else(|_| "[]".into()))
        .bind(serde_json::to_string(&versions).unwrap_or_else(|_| "[]".into()))
        .bind(border.0)
        .fetch_one(&self.write_pool)
        .await?;
        self.run_mail_rule_job(run_id.0).await
    }

    /// Продолжить задание с сохранённого курсора (S-070).
    pub async fn continue_mail_rule_run(&self, run_id: i64) -> Result<MailRuleRunReport> {
        sqlx::query("UPDATE mail_rule_runs SET state='running', updated_at=datetime('now') WHERE id=? AND state IN ('pending','running')")
            .bind(run_id)
            .execute(&self.write_pool)
            .await?;
        self.run_mail_rule_job(run_id).await
    }

    /// Последнее задание ручного прогона: сводка для списка правил.
    pub async fn last_mail_rule_run(&self) -> Result<Option<MailRuleRunReport>> {
        let row: Option<MailRuleRunRow> = sqlx::query_as(
            "SELECT id, state, scanned, applied, queued, skipped, remaining
             FROM mail_rule_runs ORDER BY id DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    /// Незавершённые задания ручного прогона: их продолжение не должно
    /// зависеть от отчёта текущей сессии, поэтому раздел правил показывает их
    /// отдельно и после перезапуска программы (S-070, S-072).
    pub async fn pending_mail_rule_runs(&self) -> Result<Vec<MailRuleRunReport>> {
        let rows: Vec<MailRuleRunRow> = sqlx::query_as(
            "SELECT id, state, scanned, applied, queued, skipped, remaining
             FROM mail_rule_runs WHERE state IN ('pending','running') ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Вернуть задания из состояния выполнения в ожидание после запуска
    /// программы: прогон, прерванный закрытием окна, продолжается с курсора
    /// (S-072).
    pub async fn restore_mail_rule_runs(&self) -> Result<usize> {
        let restored = sqlx::query(
            "UPDATE mail_rule_runs SET state='pending', updated_at=datetime('now')
             WHERE state='running'",
        )
        .execute(&self.write_pool)
        .await?;
        Ok(restored.rows_affected() as usize)
    }

    /// Разбор пачками по 500 писем с пределом 5000 писем за запуск
    /// (S-068, S-069). Каждая пачка - отдельная неделимая операция вместе с
    /// обновлением задания: писатель в базе один, и длинный прогон задержал бы
    /// всю запись программы.
    async fn run_mail_rule_job(&self, run_id: i64) -> Result<MailRuleRunReport> {
        match self.run_mail_rule_job_batches(run_id).await {
            Ok(report) => Ok(report),
            Err(error) => {
                // Иначе задание осталось бы в состоянии выполнения до
                // перезапуска программы, и причину отказа никто бы не увидел.
                let reason = crate::logging::mask_error_text(&error.to_string());
                sqlx::query(
                    "UPDATE mail_rule_runs SET state='failed', last_error=?,
                            updated_at=datetime('now')
                      WHERE id=?",
                )
                .bind(reason)
                .bind(run_id)
                .execute(&self.write_pool)
                .await?;
                Err(error)
            }
        }
    }

    async fn run_mail_rule_job_batches(&self, run_id: i64) -> Result<MailRuleRunReport> {
        let job: MailRuleJobRow = sqlx::query_as(
            "SELECT folder_ids, rule_versions, max_message_id, cursor_message_id,
                    scanned, applied, queued, skipped
             FROM mail_rule_runs WHERE id=?",
        )
        .bind(run_id)
        .fetch_one(&self.pool)
        .await?;
        let folder_ids: Vec<i64> = serde_json::from_str(&job.folder_ids).unwrap_or_default();
        let versions: Vec<RuleVersion> =
            serde_json::from_str(&job.rule_versions).unwrap_or_default();
        // S-071: задание работает с тем перечнем правил и с теми версиями,
        // которые записаны в нём. Правило, изменённое во время прогона, из
        // начатого задания выбывает.
        let rules: Vec<MailRule> = self
            .list_mail_rules()
            .await?
            .into_iter()
            .filter(|rule| {
                versions
                    .iter()
                    .any(|saved| saved.id == rule.id && saved.version == rule.version)
            })
            .filter(|rule| rule.state == "ok")
            .collect();
        let folder_list = id_list(&folder_ids);
        let mut counters = RuleRunCounters {
            scanned: job.scanned,
            applied: job.applied,
            queued: job.queued,
            skipped: job.skipped,
        };
        let mut cursor = job.cursor_message_id;
        let mut processed_now = 0_i64;
        let run_limit = self.limit(LIMIT_MANUAL_RUN_MESSAGES);
        let run_batch = self.limit(LIMIT_MANUAL_RUN_BATCH);
        while processed_now < run_limit {
            let batch = run_batch.min(run_limit - processed_now);
            let mut tx = self.begin_write().await?;
            // S-065: ручной прогон берёт письма и с признаком backfilled, но
            // только из выбранных папок и только до границы задания.
            let snapshots = load_rule_snapshots(
                &mut tx,
                &format!("m.id>? AND m.id<=? AND m.folder_id IN ({folder_list})"),
                vec![cursor, job.max_message_id],
                batch,
            )
            .await?;
            if snapshots.is_empty() {
                tx.commit().await?;
                break;
            }
            for snapshot in &snapshots {
                self.apply_rules_to_message(
                    &mut tx,
                    &rules,
                    snapshot,
                    RuleRunMode::Manual,
                    &mut counters,
                )
                .await?;
                counters.scanned += 1;
                processed_now += 1;
                cursor = snapshot.id;
            }
            // S-067: прогресс автоматического прогона ручной запуск не двигает,
            // курсор живёт в задании.
            sqlx::query(
                "UPDATE mail_rule_runs SET cursor_message_id=?, scanned=?, applied=?, queued=?,
                        skipped=?, updated_at=datetime('now')
                 WHERE id=?",
            )
            .bind(cursor)
            .bind(counters.scanned)
            .bind(counters.applied)
            .bind(counters.queued)
            .bind(counters.skipped)
            .bind(run_id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
        }
        let remaining: (i64,) = sqlx::query_as(AssertSqlSafe(format!(
            "SELECT count(*) FROM messages m
              WHERE m.id>? AND m.id<=? AND m.folder_id IN ({folder_list})"
        )))
        .bind(cursor)
        .bind(job.max_message_id)
        .fetch_one(&self.pool)
        .await?;
        let state = if remaining.0 > 0 { "pending" } else { "done" };
        sqlx::query(
            "UPDATE mail_rule_runs SET state=?, remaining=?, updated_at=datetime('now') WHERE id=?",
        )
        .bind(state)
        .bind(remaining.0)
        .bind(run_id)
        .execute(&self.write_pool)
        .await?;
        Ok(MailRuleRunReport {
            run_id,
            state: state.to_owned(),
            scanned: counters.scanned,
            applied: counters.applied,
            queued: counters.queued,
            skipped: counters.skipped,
            remaining: remaining.0,
        })
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
                // Условия ранних версий лежат в базе в старом словаре. Миграция
                // 0036 переписывает известные варианты, но выборка не должна
                // зависеть от того, прошла ли она: приводим на чтении.
                let (field, op, value) =
                    normalize_smart_condition(condition.field, condition.op, condition.value);
                groups[group_index].conditions.push(SmartCondition {
                    field,
                    op,
                    value,
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
            stable_ids.push(stable_id.to_owned());
            let existing: Option<(i64, i64)> =
                sqlx::query_as("SELECT id, is_builtin FROM smart_folders WHERE stable_id=?")
                    .bind(stable_id)
                    .fetch_optional(&mut *tx)
                    .await?;
            // Пустое имя допустимо только для встроенной папки: там оно значит
            // "подпись даёт локализация интерфейса". Встроенность берём из базы,
            // а не из присланного флага - иначе безымянной можно было бы
            // объявить любую папку. Для пользовательской папки и для незнакомого
            // идентификатора имя по-прежнему обязательно.
            let is_builtin = matches!(existing, Some((_, builtin)) if builtin != 0);
            if folder.name.trim().is_empty() && !is_builtin {
                return Err(crate::Error::AccountConfig(
                    "название умной папки не указано".into(),
                ));
            }
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

    /// Окружение отбора писем умной папки: набор включённых папок, аккаунты,
    /// папки и (по требованию) метки. Одно на список и на счётчик, чтобы
    /// правило вхождения письма не разъезжалось по двум копиям
    /// (smart-folder-selection-shared.md, S-001).
    async fn smart_selection_context(&self, with_labels: bool) -> Result<SmartSelectionContext> {
        let included = sqlx::query_as::<_, (i64,)>(SMART_INCLUDED_FOLDERS_SQL)
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
        let mut labels = std::collections::HashMap::<i64, Vec<String>>::new();
        if with_labels {
            let label_rows: Vec<(i64, String)> = sqlx::query_as(
                "SELECT ml.message_id, l.name FROM message_labels ml JOIN labels l ON l.id=ml.label_id",
            )
            .fetch_all(&self.pool)
            .await?;
            for (message_id, name) in label_rows {
                labels.entry(message_id).or_default().push(name);
            }
        }
        Ok(SmartSelectionContext {
            included,
            accounts,
            folders,
            labels,
        })
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
        let mut context = self.smart_selection_context(true).await?;
        const SCAN_PAGE_SIZE: i64 = 1_000;
        let page_size = limit.clamp(1, 500);
        let mut cursor = before_date
            .zip(before_id)
            .map(|(date, id)| (date.to_owned(), id));
        let mut result = Vec::new();
        loop {
            let rows = if let Some((date, id)) = &cursor {
                sqlx::query_as::<_, MessageRow>(SMART_PAGE_AFTER_CURSOR_SQL)
                    .bind(date)
                    .bind(date)
                    .bind(id)
                    .bind(SCAN_PAGE_SIZE)
                    .fetch_all(&self.pool)
                    .await?
            } else {
                sqlx::query_as::<_, MessageRow>(SMART_PAGE_FIRST_SQL)
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
                let Some(message) = context.prepare(row, true) else {
                    continue;
                };
                if context.matches(&folder, &message) {
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

    /// Счётчики писем умных папок для боковой панели. Считаем сразу пачкой:
    /// у умной папки нет своей таблицы, каждое число - это проход по всем
    /// письмам, и отдельный запрос на папку означал бы столько же полных
    /// сканов, сколько включено счётчиков.
    pub async fn count_smart_folder_messages(
        &self,
        stable_ids: &[String],
    ) -> Result<Vec<SmartFolderCount>> {
        if stable_ids.is_empty() {
            return Ok(Vec::new());
        }
        let wanted = self
            .list_smart_folders()
            .await?
            .into_iter()
            .filter(|folder| stable_ids.iter().any(|id| id == &folder.id))
            .collect::<Vec<_>>();
        if wanted.is_empty() {
            return Ok(Vec::new());
        }
        // Метки читаем, только если хоть одна папка по ним отбирает: иначе это
        // лишний проход по всей таблице связей ради данных, которые никто не
        // спросит (S-006).
        let needs_labels = wanted.iter().any(|folder| {
            folder.groups.iter().any(|group| {
                group
                    .conditions
                    .iter()
                    .any(|condition| condition.field == "label")
            })
        });
        let mut context = self.smart_selection_context(needs_labels).await?;
        // Читаем таблицу одним потоковым проходом. Постраничный курсор, как в
        // выборке писем, здесь дал бы квадратичную работу: условие по
        // COALESCE(date, '') под индекс не подводится, и каждая следующая
        // страница заново перебирала бы все предыдущие. Строки обрабатываются по
        // мере поступления, поэтому в памяти не накапливаются.
        let mut counts = vec![(0_i64, 0_i64); wanted.len()];
        let mut rows = sqlx::query_as::<_, MessageRow>(SMART_STREAM_SQL).fetch(&self.pool);
        while let Some(row) = rows.try_next().await? {
            let Some(message) = context.prepare(row, needs_labels) else {
                continue;
            };
            for (index, folder) in wanted.iter().enumerate() {
                if context.matches(folder, &message) {
                    counts[index].0 += 1;
                    if !message.flags.seen {
                        counts[index].1 += 1;
                    }
                }
            }
        }
        drop(rows);
        Ok(wanted
            .into_iter()
            .zip(counts)
            .map(|(folder, (total, unread))| SmartFolderCount {
                id: folder.id,
                total,
                unread,
            })
            .collect())
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
        sqlx::query("DELETE FROM contact_addresses WHERE contact_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        for address in input.addresses.iter().filter(|value| !value.is_empty()) {
            sqlx::query(
                "INSERT INTO contact_addresses(contact_id, kind, street, city,
                                               region, postal_code, country)
                 VALUES(?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(&address.kind)
            .bind(address.street.as_deref().map(str::trim))
            .bind(address.city.as_deref().map(str::trim))
            .bind(address.region.as_deref().map(str::trim))
            .bind(address.postal_code.as_deref().map(str::trim))
            .bind(address.country.as_deref().map(str::trim))
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
        let address_rows = sqlx::query_as::<_, ContactAddressRow>(
            "SELECT a.contact_id, a.kind, a.street, a.city, a.region, a.postal_code, a.country
             FROM contact_addresses a JOIN contacts c ON c.id = a.contact_id
             WHERE c.hidden=0 AND c.display_name LIKE ?
             ORDER BY a.id",
        )
        .bind(&like)
        .fetch_all(&self.pool)
        .await?;
        for row in address_rows {
            if let Some(index) = positions.get(&row.contact_id) {
                contacts[*index].addresses.push(ContactAddress {
                    kind: row.kind,
                    street: row.street,
                    city: row.city,
                    region: row.region,
                    postal_code: row.postal_code,
                    country: row.country,
                });
            }
        }
        Ok(contacts)
    }

    // ---------- Починка кодировок (issue #41, specs/message-charset-decoding.md) ----------

    /// Разовая фоновая починка уже сохранённой почты с символами замены в теме
    /// и в именах. Возвращает число исправленных писем за проход.
    ///
    /// Номер в имени флага растёт вместе с правками разбора: письма, испорченные
    /// прежним разбором, чинятся новым проходом. Проход `v2` перечитывает почту
    /// после склейки разрезанных закодированных слов (issue #62).
    ///
    /// Флаг `charset_repair_v2` в `storage_meta` защищает от повторного
    /// полного скана (S-008): при уже выставленном флаге функция сразу
    /// возвращает 0, не трогая `messages`. Ошибка на отдельном письме не
    /// прерывает проход и не пробрасывается наружу (S-007): письмо
    /// пропускается, в журнал уходит предупреждение с его id. Ошибка выборки
    /// страницы, фазы починки контактов или записи флага, наоборот,
    /// останавливает весь проход без выставления флага - починка повторится
    /// при следующем запуске (S-011).
    pub async fn repair_broken_charset_messages(&self) -> Result<usize> {
        let already_done: Option<(String,)> =
            sqlx::query_as("SELECT value FROM storage_meta WHERE key = 'charset_repair_v2'")
                .fetch_optional(&self.pool)
                .await?;
        if already_done.is_some() {
            return Ok(0);
        }

        tracing::info!("починка кодировок писем: старт прохода");

        const PAGE_SIZE: i64 = 200;
        let mut fixed = 0_usize;
        let mut skipped = 0_usize;
        let mut after_id = 0_i64;
        loop {
            let page = self
                .broken_charset_message_page(after_id, PAGE_SIZE)
                .await?;
            if page.is_empty() {
                break;
            }
            let page_len = page.len();
            after_id = page.last().map(|(id, _)| *id).unwrap_or(after_id);
            for (message_id, raw_blob_ref) in page {
                match self
                    .repair_one_charset_message(message_id, &raw_blob_ref)
                    .await
                {
                    Ok(true) => fixed += 1,
                    Ok(false) => skipped += 1,
                    Err(error) => {
                        tracing::warn!(
                            message_id,
                            error = %error,
                            "починка кодировок писем: письмо пропущено"
                        );
                        skipped += 1;
                    }
                }
            }
            if page_len < PAGE_SIZE as usize {
                break;
            }
            // Между письмами и страницами блокировка записи не удерживается -
            // список, поиск и синхронизация продолжают работать (S-016).
            tokio::task::yield_now().await;
        }

        // Фаза контактов - после починки писем: иначе кандидатов с чистым
        // именем ещё не будет и испорченные имена останутся навсегда (S-006).
        self.repair_contact_names_from_messages().await?;

        sqlx::query(
            "INSERT INTO storage_meta(key, value) VALUES('charset_repair_v2', '1')
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .execute(&self.write_pool)
        .await?;

        tracing::info!(fixed, skipped, "починка кодировок писем: проход завершён");
        Ok(fixed)
    }

    /// Страница id битых писем и их `raw_blob_ref` на момент выборки, по
    /// возрастанию id (keyset, не OFFSET - иначе исправленные письма сдвигали
    /// бы окно и часть писем терялась бы навсегда, S-016). Источники битости
    /// сведены через UNION по различным id письма (S-003): письмо, испорченное
    /// сразу в нескольких местах, попадёт в страницу один раз.
    async fn broken_charset_message_page(
        &self,
        after_id: i64,
        page_size: i64,
    ) -> Result<Vec<(i64, String)>> {
        Ok(sqlx::query_as(
            "WITH broken AS (
                SELECT id FROM messages
                 WHERE id > ? AND raw_blob_ref IS NOT NULL
                   AND (
                        subject LIKE '%' || char(0x1B) || '%' OR subject LIKE '%' || char(0xFFFD) || '%'
                     OR coalesce(from_name,'') LIKE '%' || char(0x1B) || '%' OR coalesce(from_name,'') LIKE '%' || char(0xFFFD) || '%'
                     OR coalesce(from_addr,'') LIKE '%' || char(0x1B) || '%' OR coalesce(from_addr,'') LIKE '%' || char(0xFFFD) || '%'
                     OR preview LIKE '%' || char(0x1B) || '%' OR preview LIKE '%' || char(0xFFFD) || '%'
                     OR coalesce(to_addrs,'') LIKE '%' || char(0x1B) || '%' OR coalesce(to_addrs,'') LIKE '%' || char(0xFFFD) || '%'
                     OR coalesce(cc_addrs,'') LIKE '%' || char(0x1B) || '%' OR coalesce(cc_addrs,'') LIKE '%' || char(0xFFFD) || '%'
                   )
                UNION
                SELECT m.id FROM messages m
                 JOIN attachments a ON a.message_id = m.id
                 WHERE m.id > ? AND m.raw_blob_ref IS NOT NULL
                   AND (a.filename LIKE '%' || char(0x1B) || '%' OR a.filename LIKE '%' || char(0xFFFD) || '%')
                UNION
                SELECT m.id FROM messages m
                 JOIN message_content_cache c ON c.message_id = m.id
                 WHERE m.id > ? AND m.raw_blob_ref IS NOT NULL
                   AND (
                        coalesce(c.body_text,'') LIKE '%' || char(0x1B) || '%' OR coalesce(c.body_text,'') LIKE '%' || char(0xFFFD) || '%'
                     OR coalesce(c.body_html,'') LIKE '%' || char(0x1B) || '%' OR coalesce(c.body_html,'') LIKE '%' || char(0xFFFD) || '%'
                   )
                UNION
                SELECT m.id FROM messages m
                 JOIN messages_fts f ON f.rowid = m.id
                 WHERE m.id > ? AND m.raw_blob_ref IS NOT NULL
                   AND (coalesce(f.body,'') LIKE '%' || char(0x1B) || '%' OR coalesce(f.body,'') LIKE '%' || char(0xFFFD) || '%')
             )
             SELECT b.id, m.raw_blob_ref FROM broken b
              JOIN messages m ON m.id = b.id
              ORDER BY b.id ASC LIMIT ?",
        )
        .bind(after_id)
        .bind(after_id)
        .bind(after_id)
        .bind(after_id)
        .bind(page_size)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Починить одно письмо повторным разбором исходника. `true` - письмо
    /// исправлено, `false` - пропущено, потому что `raw_blob_ref` уже сменился
    /// с момента выборки (S-014, гонка с докачкой тела или пересинхронизацией):
    /// в этом случае поля, кэш и индекс не трогаются вовсе. Ошибка (недоступен
    /// blob, не разобрался MIME, не сериализовались адреса) уходит наружу -
    /// вызывающий код превращает её в пропуск с предупреждением (S-007).
    async fn repair_one_charset_message(
        &self,
        message_id: i64,
        raw_blob_ref: &str,
    ) -> Result<bool> {
        use mail_parser::{MessageParser, MimeHeaders};
        let raw = self.blobs.get(raw_blob_ref)?;
        let normalized = encoded_words::join_split_encoded_words(&raw);
        let message = MessageParser::default()
            .parse(normalized.as_ref())
            .ok_or_else(|| crate::Error::Other("не удалось разобрать письмо повторно".into()))?;
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
        let to_json = serde_json::to_string(&addresses(message.to()))?;
        let cc_json = serde_json::to_string(&addresses(message.cc()))?;
        let subject = message.subject().unwrap_or_default().to_owned();
        let from_name = from.and_then(|a| a.name.as_deref()).map(str::to_owned);
        let from_addr = from.and_then(|a| a.address.as_deref()).map(str::to_owned);
        let preview: String = message
            .body_text(0)
            .map(|body| body.chars().take(240).collect())
            .unwrap_or_default();
        let body_text = message
            .body_text(0)
            .map(|body| body.into_owned())
            .unwrap_or_default();
        // Тот же порядок разбора, что при первичном сохранении письма
        // (save_discovered_messages) - имена вложений совпадут построчно со
        // строками attachments, вставленными в том же порядке.
        let attachment_names: Vec<String> = message
            .attachments()
            .enumerate()
            .map(|(index, part)| {
                part.attachment_name()
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("attachment-{}", index + 1))
            })
            .collect();

        let mut tx = self.begin_write().await?;
        let updated = sqlx::query(
            "UPDATE messages SET subject=?, from_name=?, from_addr=?, to_addrs=?, cc_addrs=?, preview=?
              WHERE id=? AND raw_blob_ref=?",
        )
        .bind(&subject)
        .bind(&from_name)
        .bind(&from_addr)
        .bind(&to_json)
        .bind(&cc_json)
        .bind(&preview)
        .bind(message_id)
        .bind(raw_blob_ref)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() == 0 {
            // Исходник сменился между выборкой и записью: поля уже разобраны
            // новым парсером, транзакция просто не коммитится (S-014).
            return Ok(false);
        }
        sqlx::query("DELETE FROM message_content_cache WHERE message_id = ?")
            .bind(message_id)
            .execute(&mut *tx)
            .await?;
        let attachment_ids: Vec<(i64,)> =
            sqlx::query_as("SELECT id FROM attachments WHERE message_id = ? ORDER BY id ASC")
                .bind(message_id)
                .fetch_all(&mut *tx)
                .await?;
        if attachment_ids.len() != attachment_names.len() {
            // Строки вложений разошлись с разбором исходника: имена ставим
            // только там, где соответствие однозначно, остальные не трогаем.
            tracing::warn!(
                message_id,
                in_database = attachment_ids.len(),
                in_source = attachment_names.len(),
                "починка кодировок: число вложений не совпало с разбором"
            );
        }
        for ((attachment_id,), filename) in attachment_ids.into_iter().zip(attachment_names) {
            sqlx::query("UPDATE attachments SET filename = ? WHERE id = ?")
                .bind(filename)
                .bind(attachment_id)
                .execute(&mut *tx)
                .await?;
        }
        // Тело - прямым запросом в этой же транзакции, а не вызовом
        // SearchIndex::index_body: тот пишет отдельным соединением write_pool,
        // а busy_timeout в проекте нулевой - вызов из открытой транзакции
        // немедленно упёрся бы в занятую базу (S-004).
        sqlx::query("UPDATE messages_fts SET body = ? WHERE rowid = ?")
            .bind(&body_text)
            .bind(message_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Починить имена контактов, заведённых из писем (S-006). Только контакты
    /// с `uid` вида `mail:<адрес>` и известным `account_id` - контакты CardDAV
    /// не трогаем, их имя принадлежит серверу. Кандидат на новое имя - самое
    /// свежее письмо того же аккаунта с совпадающим (без учёта регистра и
    /// крайних пробелов) `from_addr` и непустым `from_name` без маркера порчи;
    /// если такого письма нет, имя контакта остаётся прежним.
    async fn repair_contact_names_from_messages(&self) -> Result<()> {
        let broken: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT id, account_id FROM contacts
              WHERE account_id IS NOT NULL AND uid LIKE 'mail:%'
                AND (display_name LIKE '%' || char(0x1B) || '%'
                     OR display_name LIKE '%' || char(0xFFFD) || '%')",
        )
        .fetch_all(&self.pool)
        .await?;
        for (contact_id, account_id) in broken {
            let emails: Vec<(String,)> = sqlx::query_as(
                "SELECT lower(trim(email)) FROM contact_emails WHERE contact_id = ?",
            )
            .bind(contact_id)
            .fetch_all(&self.pool)
            .await?;
            if emails.is_empty() {
                continue;
            }
            // Порядок кандидатов (S-006): сначала письма с непустой датой по
            // убыванию даты, письма без даты считаются самыми старыми; при
            // равных датах побеждает письмо с наибольшим id.
            let mut query = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
                "SELECT from_name FROM messages WHERE account_id = ",
            );
            query.push_bind(account_id);
            query.push(" AND lower(trim(from_addr)) IN (");
            let mut separated = query.separated(",");
            for (email,) in &emails {
                separated.push_bind(email);
            }
            separated.push_unseparated(")");
            query.push(
                " AND from_name IS NOT NULL AND trim(from_name) <> ''
                   AND from_name NOT LIKE '%' || char(0x1B) || '%'
                   AND from_name NOT LIKE '%' || char(0xFFFD) || '%'
                 ORDER BY CASE WHEN date IS NULL OR date = '' THEN 0 ELSE 1 END DESC, date DESC, id DESC
                 LIMIT 1",
            );
            let candidate: Option<(String,)> = query
                .build_query_as::<(String,)>()
                .fetch_optional(&self.pool)
                .await?;
            if let Some((from_name,)) = candidate {
                sqlx::query("UPDATE contacts SET display_name = ? WHERE id = ?")
                    .bind(clean_contact_name(&from_name))
                    .bind(contact_id)
                    .execute(&self.write_pool)
                    .await?;
            }
        }
        Ok(())
    }
}

/// Привести условие умной папки к нынешнему словарю полей и значений.
/// Ранние версии писали в базу русские названия полей ("Статус"), старые
/// английские ("from", "status") и значения seen/not_seen, yes/no.
fn normalize_smart_condition(field: String, op: String, value: String) -> (String, String, String) {
    let field = match field.as_str() {
        "Отправитель" | "Sender" | "from" => "sender",
        "Получатель" | "Recipient" | "to" => "recipient",
        "Тема" | "Subject" => "subject",
        "Текст письма" | "Message text" => "body",
        "Аккаунт" | "Account" => "account",
        "Статус" | "Status" | "status" => "read_state",
        "Вложение" | "Attachment" => "attachment",
        "Метка" | "Label" => "label",
        "Папка" | "Folder" => "folder",
        "Дата" | "Date" => "date",
        _ => field.as_str(),
    }
    .to_owned();
    let op = match op.as_str() {
        "содержит" => "contains",
        "не содержит" | "does not contain" => "not_contains",
        "равно" => "equals",
        "не равно" => "not_equals",
        _ => op.as_str(),
    }
    .to_owned();
    let value = match (field.as_str(), value.as_str()) {
        ("read_state", "seen" | "Прочитано" | "Read") => "read",
        ("read_state", "not_seen" | "Непрочитано" | "Не прочитано" | "Unread") => {
            "unread"
        }
        ("attachment", "yes" | "Есть") => "has",
        ("attachment", "no" | "Нет") => "none",
        _ => value.as_str(),
    }
    .to_owned();
    (field, op, value)
}

// ---------- Общий отбор писем умной папки (smart-folder-selection-shared.md) ----------

/// Папки, включённые в объединённый набор. Папка без записи в `unified_sources`
/// считается включённой.
const SMART_INCLUDED_FOLDERS_SQL: &str = "SELECT f.id FROM folders f
     LEFT JOIN unified_sources us ON us.folder_id=f.id
       AND us.unified_id=(SELECT id FROM unified_folders WHERE role='all')
     WHERE COALESCE(us.included, 1)=1";

/// Поля письма, нужные списку и счётчику умной папки. Макрос, а не константа:
/// строки запросов собираются на компиляции через `concat!`.
macro_rules! message_list_columns_sql {
    () => {
        "SELECT id, account_id, folder_id, thread_id, uid, rfc822_message_id,
                from_name, from_addr, to_addrs, cc_addrs, subject, preview, date, size,
                seen, flagged, answered, draft, has_attachments, dkim_pass, spf_pass, dmarc_pass
         FROM messages"
    };
}

/// Письмо участвует в отборе, пока не отложено и по нему нет незавершённого
/// переноса или удаления (S-002).
macro_rules! message_alive_sql {
    () => {
        "(snoozed_until IS NULL OR snoozed_until <= datetime('now'))
           AND NOT EXISTS (
             SELECT 1 FROM outbox_ops o WHERE o.message_id=messages.id
               AND o.op_kind IN ('move','delete') AND o.status IN ('pending','processing','retry')
           )"
    };
}

/// Страница списка после курсора: сортировка по убыванию даты, затем по id.
const SMART_PAGE_AFTER_CURSOR_SQL: &str = concat!(
    message_list_columns_sql!(),
    " WHERE pinned_at IS NULL AND
      (COALESCE(date, '') < ? OR (COALESCE(date, '') = ? AND id < ?)) AND ",
    message_alive_sql!(),
    " ORDER BY date DESC, id DESC LIMIT ?"
);

/// Первая страница списка.
const SMART_PAGE_FIRST_SQL: &str = concat!(
    message_list_columns_sql!(),
    " WHERE pinned_at IS NULL AND ",
    message_alive_sql!(),
    " ORDER BY date DESC, id DESC LIMIT ?"
);

/// Потоковое чтение для счётчика: без сортировки и без предела (S-007).
const SMART_STREAM_SQL: &str =
    concat!(message_list_columns_sql!(), " WHERE ", message_alive_sql!());

/// Данные, по которым решается вхождение письма в умную папку.
struct SmartSelectionContext {
    included: std::collections::HashSet<i64>,
    accounts: std::collections::HashMap<i64, String>,
    folders: std::collections::HashMap<i64, Folder>,
    labels: std::collections::HashMap<i64, Vec<String>>,
}

impl SmartSelectionContext {
    /// Письмо из строки базы: None, если его папка не включена в объединённый
    /// набор. Метки подставляются только когда их читали (S-006).
    fn prepare(&mut self, row: MessageRow, with_labels: bool) -> Option<MessageMeta> {
        let mut message = MessageMeta::from(row);
        if !self.included.contains(&message.folder_id) {
            return None;
        }
        if with_labels {
            message.labels = self.labels.remove(&message.id).unwrap_or_default();
        }
        Some(message)
    }

    /// Единственная проверка вхождения письма в умную папку - одна и та же для
    /// списка и для счётчика (S-003).
    fn matches(&self, folder: &SmartFolder, message: &MessageMeta) -> bool {
        smart_folder_matches(
            folder,
            message,
            self.accounts.get(&message.account_id).map(String::as_str),
            self.folders.get(&message.folder_id),
        )
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
            // Период приходит из условия, которое пользователь пишет руками:
            // "20000000 недель" переполняли chrono и роняли выборку писем
            // паникой. Сравниваем возраст письма с периодом, а не строим
            // пороговую дату: дата переполняется намного раньше самой
            // длительности. Непредставимый период считаем бесконечным - в него
            // попадает любое письмо, и ничего не оказывается старше него.
            let Some(offset) = amount
                .checked_mul(seconds)
                .and_then(chrono::Duration::try_seconds)
            else {
                return condition.op == "within_last";
            };
            let age = chrono::Utc::now().signed_duration_since(timestamp);
            return if condition.op == "within_last" {
                age <= offset
            } else {
                age > offset
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
    last_sync_at: Option<String>,
    last_sync_error: Option<String>,
    last_sync_error_kind: Option<String>,
    needs_reauth: i64,
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
            last_sync_at: r.last_sync_at,
            last_sync_error: r.last_sync_error,
            last_sync_error_kind: r.last_sync_error_kind,
            needs_reauth: r.needs_reauth != 0,
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
    parent_id: Option<i64>,
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
            parent_id: r.parent_id,
            unread_count: r.unread_count,
            total_count: r.total_count,
        }
    }
}

fn parse_utc_time(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&chrono::Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
                .map(|time| time.and_utc())
        })
        .ok()
}

/// Вид, в котором время хранится в базе: тот же, что даёт `datetime('now')`.
/// Сравнения сроков идут строками, и запись интерфейса в другом виде делала бы
/// любое сравнение ложным до следующей полуночи.
const TASK_TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// Сроки дела, приведённые к виду базы.
struct NormalizedTaskTimes {
    start_at: Option<String>,
    due_at: Option<String>,
    reminder_at: Option<String>,
}

fn validate_task_times(input: &MessageTaskInput) -> Result<NormalizedTaskTimes> {
    let start = input.start_at.as_deref().and_then(parse_utc_time);
    let due = input.due_at.as_deref().and_then(parse_utc_time);
    let reminder = input.reminder_at.as_deref().and_then(parse_utc_time);
    if input.start_at.is_some() && start.is_none()
        || input.due_at.is_some() && due.is_none()
        || input.reminder_at.is_some() && reminder.is_none()
    {
        return Err(crate::Error::Other("время дела записано неверно".into()));
    }
    if start.zip(due).is_some_and(|(start, due)| start > due) {
        return Err(crate::Error::Other(
            "срок начала не может быть позже срока исполнения".into(),
        ));
    }
    if reminder.is_some_and(|time| time < chrono::Utc::now()) {
        return Err(crate::Error::Other(
            "время напоминания не может быть в прошлом".into(),
        ));
    }
    let format = |time: Option<chrono::DateTime<chrono::Utc>>| {
        time.map(|time| time.format(TASK_TIME_FORMAT).to_string())
    };
    Ok(NormalizedTaskTimes {
        start_at: format(start),
        due_at: format(due),
        reminder_at: format(reminder),
    })
}

/// Чтение строки дела по письму: повторяется во всех местах, где дело
/// возвращается пользователю, а расхождение столбцов означало бы, что одна и та
/// же строка читается в разных местах по-разному.
const MESSAGE_TASK_BY_ID_SQL: &str =
    "SELECT message_id,start_at,due_at,reminder_at,state,completed_at,
    reminder_shown_at,created_at,updated_at
   FROM message_tasks WHERE message_id=?";

/// Переход состояния дела при смене признака с сервера: поднявшийся флажок
/// возвращает дело в работу и очищает время выполнения, снявшийся отсоединяет
/// активное дело, выполненное не трогает.
const TASK_SYNC_FLAG_TRANSITION_SQL: &str = "state=CASE WHEN ? THEN 'active' \
    WHEN state='active' THEN 'detached' ELSE state END, \
    completed_at=CASE WHEN ? THEN NULL ELSE completed_at END, \
    updated_at=datetime('now')";

/// Дело следует за признаком важности письма, и переход зависит от того, чьё
/// это действие: своё снятие флажка удаляет дело вместе со сроками, снятие с
/// сервера только отвязывает дело, чтобы сроки пережили чужую правку, а
/// возвращённый флажок возвращает дело в работу с прежними сроками. Пары
/// "состояние - причина" перечислены таблицей переходов
/// (specs/flag-due-dates.md, S-036), поэтому решение о переходе принимается в
/// одном месте, а не у каждого вызывающего.
async fn apply_task_flag_transition(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    message_id: i64,
    reason: FlagChangeReason,
    flagged: bool,
) -> Result<()> {
    let query = match (reason, flagged) {
        (FlagChangeReason::User, false) => "DELETE FROM message_tasks WHERE message_id=?",
        (FlagChangeReason::Sync, false) => {
            "UPDATE message_tasks SET state='detached',updated_at=datetime('now')
              WHERE message_id=? AND state='active'"
        }
        (_, true) => {
            "UPDATE message_tasks SET state='active',completed_at=NULL,
                    updated_at=datetime('now')
              WHERE message_id=? AND state IN ('done','detached')"
        }
        _ => return Ok(()),
    };
    sqlx::query(query)
        .bind(message_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn read_message_task(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    message_id: i64,
) -> Result<MessageTask> {
    Ok(sqlx::query_as::<_, MessageTaskRow>(MESSAGE_TASK_BY_ID_SQL)
        .bind(message_id)
        .fetch_one(&mut **tx)
        .await?
        .into())
}

#[derive(serde::Serialize, serde::Deserialize)]
struct TransferredMessageTraits {
    pinned_at: Option<String>,
    task: Option<MessageTask>,
    /// Время, когда приметы отложены. Письмо может не вернуться вовсе, и без
    /// срока годности запись лежала бы вечно, а письмо с повторяющимся
    /// заголовком однажды унаследовало бы чужое закрепление.
    #[serde(default)]
    saved_at: Option<String>,
}

// Сколько отложенные приметы ждут возвращения письма - настройка
// LIMIT_MESSAGE_TRAITS_DAYS (crates/core/src/model/limits.rs).

fn transferred_traits_key(account_id: i64, fixed_id: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(fixed_id.as_bytes());
    let suffix = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("message_traits:{account_id}:{suffix}")
}

/// Отложить дело и закрепление письма, чья прежняя строка удалена, а
/// единственного совпадения по `Message-ID` в ящике ещё нет: приметы сработают,
/// когда та же почта придёт с сервера (restore_transferred_message_traits).
async fn store_transferred_message_traits(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    account_id: i64,
    fixed_id: &str,
    pinned_at: Option<String>,
    task: Option<MessageTask>,
) -> Result<()> {
    let key = transferred_traits_key(account_id, fixed_id);
    let value = serde_json::to_string(&TransferredMessageTraits {
        pinned_at,
        task,
        saved_at: Some(chrono::Utc::now().format(TASK_TIME_FORMAT).to_string()),
    })?;
    sqlx::query(
        "INSERT INTO storage_meta(key,value) VALUES(?,?)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Куда уводит действие быстрого шага. Отдельный разбор цели нужен потому,
/// что быстрому действию мало знать о недостижимой цели: в отчёте пропуск
/// называет причину, а у правил (apply_takeaway) любая недостижимая цель
/// одинаково останавливает только своё правило.
enum QuickStepTarget {
    Takeaway(TakeawayTarget),
    ForeignAccount,
    NoFolder,
}

async fn resolve_quick_step_target(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    action: &MailRuleAction,
    snapshot: &RuleMessageSnapshot,
) -> Result<QuickStepTarget> {
    if action.kind == "delete" {
        return Ok(QuickStepTarget::Takeaway(TakeawayTarget::Delete));
    }
    if action.kind == "move"
        && let Some(folder_id) = action.folder_id
    {
        let folder: Option<(i64, String)> =
            sqlx::query_as("SELECT account_id,remote_path FROM folders WHERE id=?")
                .bind(folder_id)
                .fetch_optional(&mut **tx)
                .await?;
        return Ok(match folder {
            Some((account_id, path)) if account_id == snapshot.account_id => {
                QuickStepTarget::Takeaway(TakeawayTarget::Folder {
                    id: folder_id,
                    path,
                })
            }
            Some(_) => QuickStepTarget::ForeignAccount,
            None => QuickStepTarget::NoFolder,
        });
    }
    let role = if action.kind == "move" {
        action.folder_role.as_deref().unwrap_or_default()
    } else {
        takeaway_target_role(&action.kind).unwrap_or_default()
    };
    Ok(
        match resolve_role_folder(tx, snapshot.account_id, role).await? {
            Some((id, path)) => QuickStepTarget::Takeaway(TakeawayTarget::Folder { id, path }),
            None => QuickStepTarget::NoFolder,
        },
    )
}

async fn message_traits_at_risk(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    message_id: i64,
    account_id: i64,
) -> Result<bool> {
    let traits: (i64,) = sqlx::query_as(
        "SELECT (pinned_at IS NOT NULL) OR EXISTS(
            SELECT 1 FROM message_tasks t WHERE t.message_id=messages.id
             AND t.state IN ('active','detached')) FROM messages WHERE id=?",
    )
    .bind(message_id)
    .fetch_one(&mut **tx)
    .await?;
    if traits.0 == 0 {
        return Ok(false);
    }
    let fixed: Option<(Option<String>,)> =
        sqlx::query_as("SELECT rfc822_message_id FROM messages WHERE id=?")
            .bind(message_id)
            .fetch_optional(&mut **tx)
            .await?;
    let Some((Some(fixed),)) = fixed else {
        return Ok(true);
    };
    let count: (i64,) =
        sqlx::query_as("SELECT count(*) FROM messages WHERE account_id=? AND rfc822_message_id=?")
            .bind(account_id)
            .bind(fixed)
            .fetch_one(&mut **tx)
            .await?;
    Ok(count.0 != 1)
}

/// Сохранить приметы письма до удаления его строки.
///
/// Общая точка для всех путей, удаляющих строку письма: завершение своей
/// операции переноса, разбор исчезнувших писем по расширению QRESYNC, сверка
/// снимка папки, смена признака действительности папки и сверка проекций
/// провайдера. Без неё перенос письма на другом устройстве и любая
/// переиндексация ящика молча уносили бы сроки дела и закрепление
/// (flag-due-dates.md, S-081 и S-091; pin-message.md, S-051 и S-058).
///
/// Веток три. Копия письма уже лежит в ящике единственной строкой - приметы
/// переносятся сразу на неё: этот порядок событий самый частый, потому что
/// копия в новой папке приходит раньше извещения об исчезновении из старой.
/// Копии ещё нет - приметы откладываются до её появления. Копий несколько -
/// выбирать не из чего, и пользователь предупреждён об этом заранее
/// (flag-due-dates.md, S-082 и S-083).
async fn preserve_message_traits(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    message_id: i64,
    limits: &LimitSet,
) -> Result<()> {
    let source: Option<(i64, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT account_id,rfc822_message_id,pinned_at FROM messages WHERE id=?")
            .bind(message_id)
            .fetch_optional(&mut **tx)
            .await?;
    let Some((account_id, Some(fixed_id), pinned_at)) = source else {
        return Ok(());
    };
    let task: Option<MessageTask> = sqlx::query_as::<_, MessageTaskRow>(MESSAGE_TASK_BY_ID_SQL)
        .bind(message_id)
        .fetch_optional(&mut **tx)
        .await?
        .map(Into::into);
    if pinned_at.is_none() && task.is_none() {
        return Ok(());
    }
    let candidates: Vec<(i64,)> = sqlx::query_as(
        "SELECT id FROM messages WHERE account_id=? AND rfc822_message_id=? AND id<>?",
    )
    .bind(account_id)
    .bind(&fixed_id)
    .bind(message_id)
    .fetch_all(&mut **tx)
    .await?;
    if candidates.len() == 1 {
        let target_id = candidates[0].0;
        if task.is_some() {
            sqlx::query(
                "INSERT INTO message_tasks(message_id,start_at,due_at,reminder_at,state,
                                           completed_at,reminder_shown_at,created_at,updated_at)
                 SELECT ?,start_at,due_at,reminder_at,state,completed_at,
                        reminder_shown_at,created_at,updated_at
                   FROM message_tasks WHERE message_id=?
                 ON CONFLICT(message_id) DO UPDATE SET
                    start_at=excluded.start_at,due_at=excluded.due_at,
                    reminder_at=excluded.reminder_at,state=excluded.state,
                    completed_at=excluded.completed_at,
                    reminder_shown_at=excluded.reminder_shown_at,
                    updated_at=excluded.updated_at",
            )
            .bind(target_id)
            .bind(message_id)
            .execute(&mut **tx)
            .await?;
        }
        if let Some(pinned_at) = pinned_at.as_deref() {
            set_pinned_within_limit(tx, target_id, pinned_at, limits).await?;
        }
    } else if candidates.is_empty() {
        store_transferred_message_traits(tx, account_id, &fixed_id, pinned_at, task).await?;
    }
    Ok(())
}

/// Вернуть закрепление письму, не выходя за предел закреплений ящика: перенос
/// и восстановление примет идут мимо команды закрепления, и без этой проверки
/// ящик мог набрать больше предела закреплённых писем (pin-message.md,
/// S-007 и S-008).
async fn set_pinned_within_limit(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    message_id: i64,
    pinned_at: &str,
    limits: &LimitSet,
) -> Result<()> {
    let room: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM messages
          WHERE account_id=(SELECT account_id FROM messages WHERE id=?)
            AND pinned_at IS NOT NULL AND id<>?",
    )
    .bind(message_id)
    .bind(message_id)
    .fetch_one(&mut **tx)
    .await?;
    if room.0 >= limits.get(LIMIT_PINNED_PER_ACCOUNT) {
        return Ok(());
    }
    sqlx::query("UPDATE messages SET pinned_at=? WHERE id=?")
        .bind(pinned_at)
        .bind(message_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn restore_transferred_message_traits(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    account_id: i64,
    fixed_id: &str,
    message_id: i64,
    limits: &LimitSet,
) -> Result<()> {
    let key = transferred_traits_key(account_id, fixed_id);
    let value: Option<(String,)> = sqlx::query_as("SELECT value FROM storage_meta WHERE key=?")
        .bind(&key)
        .fetch_optional(&mut **tx)
        .await?;
    let Some((value,)) = value else {
        return Ok(());
    };
    let traits: TransferredMessageTraits = serde_json::from_str(&value)?;
    // Письмо, не вернувшееся за месяц, скорее всего не вернётся вовсе, а
    // заголовок письма может и повториться: наследовать чужое закрепление
    // такая запись не должна.
    let expired = traits
        .saved_at
        .as_deref()
        .and_then(parse_utc_time)
        .is_some_and(|saved| {
            chrono::Utc::now().signed_duration_since(saved)
                > chrono::Duration::days(limits.get(LIMIT_MESSAGE_TRAITS_DAYS))
        });
    if expired {
        sqlx::query("DELETE FROM storage_meta WHERE key=?")
            .bind(key)
            .execute(&mut **tx)
            .await?;
        return Ok(());
    }
    if let Some(pinned_at) = traits.pinned_at {
        set_pinned_within_limit(tx, message_id, &pinned_at, limits).await?;
    }
    if let Some(task) = traits.task {
        sqlx::query(
            "INSERT INTO message_tasks(message_id,start_at,due_at,reminder_at,state,completed_at,
                                       reminder_shown_at,created_at,updated_at)
             VALUES(?,?,?,?,?,?,?,?,?)
             ON CONFLICT(message_id) DO UPDATE SET start_at=excluded.start_at,
                 due_at=excluded.due_at,reminder_at=excluded.reminder_at,state=excluded.state,
                 completed_at=excluded.completed_at,reminder_shown_at=excluded.reminder_shown_at,
                 updated_at=excluded.updated_at",
        )
        .bind(message_id)
        .bind(task.start_at)
        .bind(task.due_at)
        .bind(task.reminder_at)
        .bind(task.state)
        .bind(task.completed_at)
        .bind(task.reminder_shown_at)
        .bind(task.created_at)
        .bind(task.updated_at)
        .execute(&mut **tx)
        .await?;
    }
    sqlx::query("DELETE FROM storage_meta WHERE key=?")
        .bind(key)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct MessageTaskRow {
    message_id: i64,
    start_at: Option<String>,
    due_at: Option<String>,
    reminder_at: Option<String>,
    state: String,
    completed_at: Option<String>,
    reminder_shown_at: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

impl From<MessageTaskRow> for MessageTask {
    fn from(row: MessageTaskRow) -> Self {
        Self {
            message_id: row.message_id,
            start_at: row.start_at,
            due_at: row.due_at,
            reminder_at: row.reminder_at,
            state: row.state,
            completed_at: row.completed_at,
            reminder_shown_at: row.reminder_shown_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct MessageTaskListRow {
    message_id: i64,
    start_at: Option<String>,
    due_at: Option<String>,
    reminder_at: Option<String>,
    state: String,
    completed_at: Option<String>,
    reminder_shown_at: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    account_id: i64,
    account_email: String,
    folder_id: i64,
    subject: String,
    sender_name: Option<String>,
    sender_address: Option<String>,
    message_date: Option<String>,
    snoozed_until: Option<String>,
    has_takeaway: i64,
    sort_group: i64,
    due_key: String,
    date_key: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct TaskPageCursor {
    group: i64,
    due: String,
    date: String,
    id: i64,
}

impl From<MessageTaskListRow> for TaskListItem {
    fn from(row: MessageTaskListRow) -> Self {
        Self {
            task: MessageTask {
                message_id: row.message_id,
                start_at: row.start_at,
                due_at: row.due_at,
                reminder_at: row.reminder_at,
                state: row.state,
                completed_at: row.completed_at,
                reminder_shown_at: row.reminder_shown_at,
                created_at: row.created_at,
                updated_at: row.updated_at,
            },
            account_id: row.account_id,
            account_email: row.account_email,
            folder_id: row.folder_id,
            subject: row.subject,
            sender_name: row.sender_name,
            sender_address: row.sender_address,
            message_date: row.message_date,
            snoozed_until: row.snoozed_until,
            has_takeaway: row.has_takeaway != 0,
        }
    }
}

#[derive(sqlx::FromRow)]
struct TaskReminderRow {
    message_id: i64,
    sender_name: Option<String>,
    sender_address: Option<String>,
    subject: String,
    due_at: Option<String>,
    reminder_at: String,
}

impl From<TaskReminderRow> for TaskReminder {
    fn from(row: TaskReminderRow) -> Self {
        Self {
            message_id: row.message_id,
            sender_name: row.sender_name,
            sender_address: row.sender_address,
            subject: row.subject,
            due_at: row.due_at,
            reminder_at: row.reminder_at,
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
            pinned_at: None,
            task_due_at: None,
            task_state: None,
            task_start_at: None,
            task_reminder_at: None,
        }
    }
}

#[derive(sqlx::FromRow)]
struct QuickStepRow {
    id: i64,
    name: String,
    icon: String,
    sort_order: i64,
    hotkey_slot: Option<i64>,
    state: String,
}

#[derive(sqlx::FromRow)]
struct QuickStepActionRow {
    quick_step_id: i64,
    kind: String,
    folder_id: Option<i64>,
    folder_role: Option<String>,
    label_id: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct MailRuleRow {
    id: String,
    name: String,
    account_id: Option<i64>,
    enabled: i64,
    progress_message_id: i64,
    sort_order: i64,
    rule_version: i64,
}

/// Правило прежней схемы: одно поле, один оператор, одно значение и одно
/// действие (S-074, S-075).
#[derive(sqlx::FromRow)]
struct LegacyMailRuleRow {
    id: String,
    field: String,
    operator: String,
    value: String,
    action: String,
    folder_id: Option<i64>,
    label_id: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct MailRuleConditionRow {
    rule_id: String,
    is_exception: i64,
    group_index: i64,
    group_logic: String,
    field: String,
    op: String,
    value: String,
    unit: Option<String>,
    value2: Option<String>,
}

#[derive(sqlx::FromRow)]
struct MailRuleActionRow {
    rule_id: String,
    kind: String,
    folder_id: Option<i64>,
    folder_role: Option<String>,
    label_id: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct MailRuleRunRow {
    id: i64,
    state: String,
    scanned: i64,
    applied: i64,
    queued: i64,
    skipped: i64,
    remaining: i64,
}

impl From<MailRuleRunRow> for MailRuleRunReport {
    fn from(row: MailRuleRunRow) -> Self {
        Self {
            run_id: row.id,
            state: row.state,
            scanned: row.scanned,
            applied: row.applied,
            queued: row.queued,
            skipped: row.skipped,
            remaining: row.remaining,
        }
    }
}

#[derive(sqlx::FromRow)]
struct MailRuleJobRow {
    folder_ids: String,
    rule_versions: String,
    max_message_id: i64,
    cursor_message_id: i64,
    scanned: i64,
    applied: i64,
    queued: i64,
    skipped: i64,
}

#[derive(sqlx::FromRow)]
struct RuleSnapshotRow {
    id: i64,
    account_id: i64,
    folder_id: i64,
    uid: i64,
    remote_path: String,
    display_name: Option<String>,
    role: Option<String>,
    email: String,
    remote_id: Option<String>,
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
    has_takeaway: i64,
}

impl From<RuleSnapshotRow> for RuleMessageSnapshot {
    fn from(row: RuleSnapshotRow) -> Self {
        // Получатели читаются из тех же столбцов JSON, что и список писем: имя
        // и адрес склеиваются, чтобы условие ловило и то, и другое.
        let addresses = |raw: Option<&str>| -> String {
            raw.and_then(|value| serde_json::from_str::<Vec<Addr>>(value).ok())
                .map(|list| {
                    list.into_iter()
                        .map(|address| {
                            format!("{} {}", address.name.unwrap_or_default(), address.email)
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default()
        };
        let recipients = format!(
            "{} {}",
            addresses(row.to_addrs.as_deref()),
            addresses(row.cc_addrs.as_deref())
        );
        Self {
            id: row.id,
            account_id: row.account_id,
            folder_id: row.folder_id,
            uid: row.uid,
            remote_path: row.remote_path.clone(),
            remote_id: row.remote_id,
            account_email: row.email,
            folder_name: format!(
                "{} {}",
                row.display_name.unwrap_or_default(),
                row.remote_path
            ),
            folder_role: row.role,
            from_name: row.from_name,
            from_addr: row.from_addr,
            recipients,
            subject: row.subject,
            preview: row.preview,
            date: row.date,
            size: row.size,
            seen: row.seen != 0,
            flagged: row.flagged != 0,
            answered: row.answered != 0,
            draft: row.draft != 0,
            has_attachments: row.has_attachments != 0,
            labels: Vec::new(),
            has_takeaway: row.has_takeaway != 0,
        }
    }
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

#[derive(sqlx::FromRow)]
struct ContactAddressRow {
    contact_id: i64,
    kind: Option<String>,
    street: Option<String>,
    city: Option<String>,
    region: Option<String>,
    postal_code: Option<String>,
    country: Option<String>,
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
            addresses: Vec::new(),
            is_favorite: r.is_favorite != 0,
            is_local_only: r.remote_url.is_none(),
        }
    }
}

// ---------- Прогон правил: снимок письма, сопоставление и постановка ----------

// Размер одной пачки ручного прогона и предел писем за один запуск
// (S-068, S-069) задаются настройками: ключи LIMIT_MANUAL_RUN_BATCH и
// LIMIT_MANUAL_RUN_MESSAGES (crates/core/src/model/limits.rs).

/// Счётчики отчёта о прогоне (S-073).
#[derive(Debug, Clone, Copy, Default)]
struct RuleRunCounters {
    scanned: i64,
    applied: i64,
    queued: i64,
    skipped: i64,
}

/// Изменяемое состояние признаков письма во время цепочки действий. Снимок
/// письма при этом не меняется: условия всех правил сверяются с ним (S-031).
#[derive(Debug, Clone, Copy)]
struct MessageFlags {
    seen: bool,
    flagged: bool,
}

/// Откуда идёт прогон. Автоматический прогон проверяет прогресс каждого
/// правила, ручной разбирает выбранные папки заново по требованию пользователя
/// (S-058, S-067).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuleRunMode {
    Automatic,
    Manual,
}

/// Кто ставит операцию увода. Пользователь заменяет собственную незавершённую
/// операцию (S-006), стадия чужую не трогает и пропускает письмо (S-007).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TakeawayActor {
    User,
    Stage,
}

/// Исход постановки. Все отказы - это исходы, а не ошибки: ошибка из цикла по
/// письмам откатила бы неделимую операцию целиком и отменила бы действие над
/// остальными письмами группы (S-005).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TakeawayOutcome {
    Queued(i64),
    Conflict,
    /// Письмо уже переносится на сервере: заменить такую операцию нельзя
    /// (S-006).
    Busy,
    /// Прошлая операция письма дошла до состояния отказа и ждёт решения
    /// пользователя (S-052, S-053).
    Failed,
    Unchanged,
    NeedsAttention,
}

/// Куда уводится письмо. Безвозвратное удаление - отдельная цель, а не
/// следствие переноса в корзину (S-012, S-013).
#[derive(Debug, Clone)]
pub(crate) enum TakeawayTarget {
    Folder { id: i64, path: String },
    Delete,
}

/// Данные письма, которых хватает для постановки операции увода.
#[derive(Debug, Clone)]
pub(crate) struct TakeawayMessage {
    pub id: i64,
    pub account_id: i64,
    pub folder_id: i64,
    pub uid: i64,
    pub remote_path: String,
    pub remote_id: Option<String>,
}

/// Снимок письма на начало прогона: все правила сверяются с ним, а не с
/// письмом, уже изменённым предыдущим правилом (S-031).
#[derive(Debug, Clone)]
pub(crate) struct RuleMessageSnapshot {
    pub id: i64,
    pub account_id: i64,
    pub folder_id: i64,
    pub uid: i64,
    pub remote_path: String,
    pub remote_id: Option<String>,
    pub account_email: String,
    pub folder_name: String,
    pub folder_role: Option<String>,
    pub from_name: Option<String>,
    pub from_addr: Option<String>,
    pub recipients: String,
    pub subject: String,
    pub preview: String,
    pub date: Option<String>,
    pub size: Option<i64>,
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
    pub draft: bool,
    pub has_attachments: bool,
    pub labels: Vec<String>,
    /// По письму уже стоит незавершённая операция увода или операция в
    /// состоянии отказа (S-002, S-052).
    pub has_takeaway: bool,
}

impl RuleMessageSnapshot {
    fn takeaway_message(&self) -> TakeawayMessage {
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

/// Перечень номеров для условия IN. Значения целые, поэтому подставляются в
/// текст запроса: связанных параметров переменного числа sqlx не даёт.
fn id_list(ids: &[i64]) -> String {
    if ids.is_empty() {
        return "NULL".into();
    }
    ids.iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// Снимки писем одной пачки. `filter` дописывается в условие запроса, а его
/// связанные параметры приходят в `binds` - значения условий в текст запроса
/// не попадают.
async fn load_rule_snapshots(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    filter: &str,
    binds: Vec<i64>,
    limit: i64,
) -> Result<Vec<RuleMessageSnapshot>> {
    let sql = format!(
        "SELECT m.id, m.account_id, m.folder_id, m.uid, f.remote_path, f.display_name,
                f.role, a.email, m.remote_id, m.from_name, m.from_addr, m.to_addrs,
                m.cc_addrs, m.subject, m.preview, m.date, m.size, m.seen, m.flagged,
                m.answered, m.draft, m.has_attachments,
                EXISTS(SELECT 1 FROM outbox_ops o WHERE o.message_id=m.id
                        AND o.op_kind IN ('move','delete')
                        AND o.status IN ('pending','processing','retry','failed')) AS has_takeaway
           FROM messages m
           JOIN folders f ON f.id=m.folder_id
           JOIN accounts a ON a.id=m.account_id
          WHERE {filter}
          ORDER BY m.id LIMIT ?"
    );
    let mut query = sqlx::query_as::<_, RuleSnapshotRow>(AssertSqlSafe(sql));
    for value in binds {
        query = query.bind(value);
    }
    let rows = query.bind(limit).fetch_all(&mut **tx).await?;
    let ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
    let mut labels: std::collections::HashMap<i64, Vec<String>> = std::collections::HashMap::new();
    if !ids.is_empty() {
        let pairs: Vec<(i64, String)> = sqlx::query_as(AssertSqlSafe(format!(
            "SELECT ml.message_id, l.name FROM message_labels ml
               JOIN labels l ON l.id=ml.label_id
              WHERE ml.message_id IN ({})",
            id_list(&ids)
        )))
        .fetch_all(&mut **tx)
        .await?;
        for (message_id, name) in pairs {
            labels.entry(message_id).or_default().push(name);
        }
    }
    Ok(rows
        .into_iter()
        .map(|row| {
            let id = row.id;
            let mut snapshot = RuleMessageSnapshot::from(row);
            snapshot.labels = labels.remove(&id).unwrap_or_default();
            snapshot
        })
        .collect())
}

/// Запросы правил живут одной строкой на оба пути чтения: список для
/// интерфейса читает из пула чтения, а прогон - из своей неделимой операции
/// записи (S-057).
const MAIL_RULES_SQL: &str =
    "SELECT id, name, account_id, enabled, progress_message_id, sort_order, rule_version
     FROM mail_rules ORDER BY sort_order, created_at, id";
const MAIL_RULE_CONDITIONS_SQL: &str =
    "SELECT rule_id, is_exception, group_index, group_logic, field, op, value, unit, value2
     FROM mail_rule_conditions ORDER BY rule_id, is_exception, group_index, position, id";
const MAIL_RULE_ACTIONS_SQL: &str = "SELECT rule_id, kind, folder_id, folder_role, label_id
     FROM mail_rule_actions ORDER BY rule_id, position, id";
const FOLDER_ROLES_SQL: &str = "SELECT account_id, role FROM folders";

/// Правила и их состояние, прочитанные внутри уже открытой неделимой операции.
/// Чтение из пула чтения до её открытия дало бы двум параллельным прогонам
/// разных ящиков одну и ту же пачку писем (S-057).
async fn load_mail_rules_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
) -> Result<Vec<MailRule>> {
    let rows: Vec<MailRuleRow> = sqlx::query_as(MAIL_RULES_SQL).fetch_all(&mut **tx).await?;
    let conditions: Vec<MailRuleConditionRow> = sqlx::query_as(MAIL_RULE_CONDITIONS_SQL)
        .fetch_all(&mut **tx)
        .await?;
    let actions: Vec<MailRuleActionRow> = sqlx::query_as(MAIL_RULE_ACTIONS_SQL)
        .fetch_all(&mut **tx)
        .await?;
    let folder_roles: Vec<(i64, Option<String>)> = sqlx::query_as(FOLDER_ROLES_SQL)
        .fetch_all(&mut **tx)
        .await?;
    Ok(assemble_mail_rules(
        rows,
        conditions,
        actions,
        &folder_roles,
    ))
}

/// Снять с письма признак стадии, закрывшей его этой операцией. Без этого
/// письмо, чья операция дошла до отказа, навсегда выпало бы из автоматического
/// прогона и из уведомлений (S-008, S-053).
async fn clear_stage_result_of_operation(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    operation_id: i64,
) -> Result<()> {
    sqlx::query(
        "UPDATE messages SET closed_by_stage=NULL
          WHERE id=(SELECT message_id FROM outbox_ops WHERE id=?)",
    )
    .bind(operation_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Проставить роль папке, у которой она не выставлена при первом обходе:
/// роль выводится из пути и названия. Без этого шага ящик, где роль ещё не
/// определилась, выглядел бы как ящик без такой папки.
async fn infer_missing_folder_role(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    account_id: i64,
    role: &str,
) -> Result<()> {
    let known: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM folders WHERE account_id=? AND role=? LIMIT 1")
            .bind(account_id)
            .bind(role)
            .fetch_optional(&mut **tx)
            .await?;
    if known.is_some() {
        return Ok(());
    }
    let expected = FolderRole::parse(role);
    let folders: Vec<(i64, String, String)> =
        sqlx::query_as("SELECT id, remote_path, display_name FROM folders WHERE account_id=?")
            .bind(account_id)
            .fetch_all(&mut **tx)
            .await?;
    if let Some((id, _, _)) = folders
        .into_iter()
        .find(|(_, path, name)| crate::model::infer_folder_role(path, name) == expected)
    {
        sqlx::query("UPDATE folders SET role=? WHERE id=?")
            .bind(role)
            .bind(id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// Папка роли в ящике: ровно одна, иначе правило переводится в состояние
/// внимания, а разбор пачки продолжается (S-046).
pub(crate) async fn resolve_role_folder(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    account_id: i64,
    role: &str,
) -> Result<Option<(i64, String)>> {
    let folders: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, remote_path FROM folders WHERE account_id=? AND role=?")
            .bind(account_id)
            .bind(role)
            .fetch_all(&mut **tx)
            .await?;
    if folders.len() == 1 {
        Ok(folders.into_iter().next())
    } else {
        Ok(None)
    }
}

/// Общая точка постановки операции увода: через неё идут стадии, правила и
/// действия пользователя (S-003, S-006, S-007, S-012).
pub(crate) async fn queue_takeaway_operation(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    message: &TakeawayMessage,
    target: &TakeawayTarget,
    actor: TakeawayActor,
    rule_id: Option<&str>,
    delay_seconds: i64,
) -> Result<TakeawayOutcome> {
    if let TakeawayTarget::Folder { id, .. } = target
        && *id == message.folder_id
    {
        // S-012: письмо уже лежит в этой папке. Прежде такой перенос в корзину
        // превращался в безвозвратное удаление - теперь не превращается ни по
        // требованию стадии, ни по требованию пользователя.
        return Ok(TakeawayOutcome::Unchanged);
    }
    let existing: Option<(i64, String)> = sqlx::query_as(
        "SELECT id, status FROM outbox_ops
          WHERE message_id=? AND op_kind IN ('move','delete')
            AND status IN ('pending','processing','retry','failed')
          ORDER BY id LIMIT 1",
    )
    .bind(message.id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some((existing_id, status)) = existing {
        match (actor, status.as_str()) {
            (TakeawayActor::User, "pending" | "retry") => {
                // S-006: передумать после ошибочного переноса можно, пока
                // операция не ушла на сервер.
                sqlx::query("DELETE FROM outbox_ops WHERE id=?")
                    .bind(existing_id)
                    .execute(&mut **tx)
                    .await?;
            }
            // S-005: занятое письмо возвращается пропуском, а не ошибкой.
            // Ошибка здесь откатывала бы неделимую операцию группового
            // действия и отменяла бы перенос всех остальных писем.
            (TakeawayActor::User, "processing") => return Ok(TakeawayOutcome::Busy),
            (TakeawayActor::User, _) => {
                // S-052, S-053: операция в состоянии отказа ждёт решения
                // пользователя - повтора или отказа от неё.
                return Ok(TakeawayOutcome::Failed);
            }
            (TakeawayActor::Stage, _) => return Ok(TakeawayOutcome::Conflict),
        }
    }
    let (kind, target_id, target_path) = match target {
        TakeawayTarget::Delete => ("delete", None, None),
        TakeawayTarget::Folder { id, path } => ("move", Some(*id), Some(path.as_str())),
    };
    let payload = serde_json::json!({
        "message_id": message.id,
        "folder_id": message.folder_id,
        "folder_path": message.remote_path,
        "uid": message.uid,
        "remote_id": message.remote_id,
        "target_folder_id": target_id,
        "target_folder_path": target_path,
        "rule_id": rule_id,
    });
    let inserted = sqlx::query(
        "INSERT INTO outbox_ops(account_id, message_id, op_kind, payload, status, next_attempt_at)
         VALUES(?, ?, ?, ?, 'pending', datetime('now', ?))",
    )
    .bind(message.account_id)
    .bind(message.id)
    .bind(kind)
    .bind(payload.to_string())
    .bind(format!("+{delay_seconds} seconds"))
    .execute(&mut **tx)
    .await;
    match inserted {
        Ok(result) => Ok(TakeawayOutcome::Queued(result.last_insert_rowid())),
        // S-005: ограничение по одному уводу на письмо отвергло вставку -
        // письмо пропускается, а групповое действие и прогон продолжаются.
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            Ok(TakeawayOutcome::Conflict)
        }
        Err(error) => Err(error.into()),
    }
}

/// Правило применимо к письму: совпала хотя бы одна обычная группа и не
/// совпала ни одна группа исключений (S-015, S-017).
fn rule_matches(rule: &MailRule, message: &RuleMessageSnapshot) -> bool {
    if rule
        .exceptions
        .iter()
        .any(|group| rule_group_matches(group, message))
    {
        return false;
    }
    rule.groups
        .iter()
        .any(|group| rule_group_matches(group, message))
}

/// Логика группы применяется ко всем её условиям, пустая группа совпадением не
/// считается (S-016, S-018).
fn rule_group_matches(group: &MailRuleGroup, message: &RuleMessageSnapshot) -> bool {
    if group.conditions.is_empty() {
        return false;
    }
    let matches = |condition: &MailRuleCondition| rule_condition_matches(condition, message);
    if group.logic == "any" {
        group.conditions.iter().any(matches)
    } else {
        group.conditions.iter().all(matches)
    }
}

fn rule_condition_matches(condition: &MailRuleCondition, message: &RuleMessageSnapshot) -> bool {
    match rule_field_kind(&condition.field) {
        Some(RuleFieldKind::Date) => return rule_date_matches(condition, message),
        Some(RuleFieldKind::Size) => return rule_size_matches(condition, message),
        None => return false,
        _ => {}
    }
    // S-023, S-024: точный адрес сравнивается целиком в канонической форме, без
    // отображаемого имени.
    let (left, right) = if condition.field == "sender_address" {
        (
            canonical_sender_address(message.from_addr.as_deref().unwrap_or("")).to_lowercase(),
            canonical_sender_address(&condition.value).to_lowercase(),
        )
    } else {
        (
            rule_field_value(&condition.field, message).to_lowercase(),
            condition.value.trim().to_lowercase(),
        )
    };
    // S-026: поле без значения - пустая строка, совпадающая только с
    // операторами отрицания.
    match condition.op.as_str() {
        "not_contains" => !left.contains(&right),
        "equals" => left == right,
        "not_equals" => left != right,
        "starts_with" => left.starts_with(&right),
        "ends_with" => left.ends_with(&right),
        _ => left.contains(&right),
    }
}

fn rule_field_value(field: &str, message: &RuleMessageSnapshot) -> String {
    match field {
        "sender" => format!(
            "{} {}",
            message.from_name.as_deref().unwrap_or(""),
            message.from_addr.as_deref().unwrap_or("")
        ),
        "recipient" => message.recipients.clone(),
        "subject" => message.subject.clone(),
        // Тело из сети ради условия не загружается: сравнивается то, что уже
        // лежит локально.
        "body" => message.preview.clone(),
        "account" => message.account_email.clone(),
        "folder" => message.folder_name.clone(),
        "folder_role" => message
            .folder_role
            .clone()
            .unwrap_or_else(|| "other".into()),
        "read_state" => if message.seen { "read" } else { "unread" }.into(),
        "importance" => if message.flagged { "flagged" } else { "normal" }.into(),
        "reply_state" => if message.answered {
            "answered"
        } else {
            "unanswered"
        }
        .into(),
        "draft_state" => if message.draft { "draft" } else { "not_draft" }.into(),
        "attachment" => if message.has_attachments {
            "has"
        } else {
            "none"
        }
        .into(),
        "label" => message.labels.join(" "),
        _ => String::new(),
    }
}

fn rule_date_matches(condition: &MailRuleCondition, message: &RuleMessageSnapshot) -> bool {
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
        let Ok(amount) = condition.value.trim().parse::<i64>() else {
            return false;
        };
        let seconds = match condition.unit.as_deref().unwrap_or("hours") {
            "minutes" => 60,
            "days" => 86_400,
            "weeks" => 604_800,
            _ => 3_600,
        };
        // Период приходит из условия, которое пользователь пишет руками:
        // непредставимую длительность считаем бесконечной, а не роняем прогон.
        let Some(offset) = amount
            .checked_mul(seconds)
            .and_then(chrono::Duration::try_seconds)
        else {
            return condition.op == "within_last";
        };
        let age = chrono::Utc::now().signed_duration_since(timestamp);
        return if condition.op == "within_last" {
            age <= offset
        } else {
            age > offset
        };
    }
    let Ok(target) = chrono::NaiveDate::parse_from_str(condition.value.trim(), "%Y-%m-%d") else {
        return false;
    };
    let actual = timestamp.date_naive();
    match condition.op.as_str() {
        "before" => actual < target,
        "after" => actual > target,
        _ => actual == target,
    }
}

fn rule_size_matches(condition: &MailRuleCondition, message: &RuleMessageSnapshot) -> bool {
    let Some(bytes) = message.size else {
        return false;
    };
    let factor = match condition.unit.as_deref().unwrap_or("mb") {
        "kb" => 1_024_f64,
        "gb" => 1_073_741_824_f64,
        _ => 1_048_576_f64,
    };
    let Ok(value) = condition.value.trim().parse::<f64>() else {
        return false;
    };
    let minimum = value * factor;
    let bytes = bytes as f64;
    match condition.op.as_str() {
        "greater_than" => bytes > minimum,
        "greater_or_equal" => bytes >= minimum,
        "less_than" => bytes < minimum,
        "less_or_equal" => bytes <= minimum,
        "between" => condition
            .value2
            .as_deref()
            .and_then(|value| value.trim().parse::<f64>().ok())
            .is_some_and(|maximum| bytes >= minimum && bytes <= maximum * factor),
        _ => (bytes - minimum).abs() < f64::EPSILON,
    }
}

/// Значения прежних столбцов правила. Ссылки на папку и метку в них больше не
/// пишутся: старый `folder_id` объявлен с каскадом, и удаление папки стёрло бы
/// правило целиком вместо перевода в состояние внимания (S-054, S-078).
struct LegacyRuleColumns {
    field: String,
    operator: String,
    value: String,
    action: String,
    folder_id: Option<i64>,
    label_id: Option<i64>,
}

fn legacy_rule_columns(rule: &MailRuleInput) -> LegacyRuleColumns {
    let first = rule
        .groups
        .first()
        .and_then(|group| group.conditions.first());
    let (field, operator) = match first {
        // S-078: в прежние столбцы попадают только значения, допустимые их
        // ограничениями check.
        Some(condition)
            if matches!(condition.field.as_str(), "sender" | "subject")
                && matches!(condition.op.as_str(), "contains" | "equals") =>
        {
            (condition.field.clone(), condition.op.clone())
        }
        _ => ("subject".to_owned(), "contains".to_owned()),
    };
    let value = first
        .map(|condition| condition.value.trim().to_owned())
        .unwrap_or_default();
    let action = rule
        .actions
        .iter()
        .find_map(|action| match action.kind.as_str() {
            "move" | "archive" | "spam" | "trash" => Some(action.kind.clone()),
            "label_add" => Some("label".to_owned()),
            _ => None,
        })
        .unwrap_or_else(|| "archive".to_owned());
    LegacyRuleColumns {
        field,
        operator,
        value,
        action,
        folder_id: None,
        label_id: None,
    }
}

/// Собрать правила из строк таблиц и определить их состояние (S-054).
fn assemble_mail_rules(
    rows: Vec<MailRuleRow>,
    conditions: Vec<MailRuleConditionRow>,
    actions: Vec<MailRuleActionRow>,
    folder_roles: &[(i64, Option<String>)],
) -> Vec<MailRule> {
    let mut grouped: std::collections::HashMap<String, (Vec<MailRuleGroup>, Vec<MailRuleGroup>)> =
        std::collections::HashMap::new();
    for row in conditions {
        let entry = grouped.entry(row.rule_id.clone()).or_default();
        let target = if row.is_exception != 0 {
            &mut entry.1
        } else {
            &mut entry.0
        };
        let index = row.group_index.max(0) as usize;
        while target.len() <= index {
            target.push(MailRuleGroup {
                logic: "all".into(),
                conditions: Vec::new(),
            });
        }
        target[index].logic = row.group_logic.clone();
        target[index].conditions.push(MailRuleCondition {
            field: row.field,
            op: row.op,
            value: row.value,
            unit: row.unit,
            value2: row.value2,
        });
    }
    let mut by_rule: std::collections::HashMap<String, Vec<MailRuleAction>> =
        std::collections::HashMap::new();
    for row in actions {
        by_rule
            .entry(row.rule_id.clone())
            .or_default()
            .push(MailRuleAction {
                kind: row.kind,
                folder_id: row.folder_id,
                folder_role: row.folder_role,
                label_id: row.label_id,
            });
    }
    rows.into_iter()
        .map(|row| {
            let (groups, exceptions) = grouped.remove(&row.id).unwrap_or_default();
            // Пустая группа, найденная в базе, действующей не считается (S-018).
            let groups: Vec<MailRuleGroup> = groups
                .into_iter()
                .filter(|group| !group.conditions.is_empty())
                .collect();
            let exceptions: Vec<MailRuleGroup> = exceptions
                .into_iter()
                .filter(|group| !group.conditions.is_empty())
                .collect();
            let actions = by_rule.remove(&row.id).unwrap_or_default();
            let reason = rule_attention_reason(row.account_id, &groups, &actions, folder_roles);
            MailRule {
                id: row.id,
                name: row.name,
                account_id: row.account_id,
                enabled: row.enabled != 0,
                progress_message_id: row.progress_message_id,
                sort_order: row.sort_order,
                version: row.rule_version,
                groups,
                exceptions,
                actions,
                state: if reason.is_some() {
                    "needs_attention".into()
                } else {
                    "ok".into()
                },
                attention_reason: reason,
            }
        })
        .collect()
}

/// Причина состояния `needs_attention`: удалённая папка, удалённая метка,
/// неоднозначная роль папки или потерянные условия (S-054).
fn rule_attention_reason(
    account_id: Option<i64>,
    groups: &[MailRuleGroup],
    actions: &[MailRuleAction],
    folder_roles: &[(i64, Option<String>)],
) -> Option<String> {
    if groups.is_empty() || actions.is_empty() {
        return Some("rule_incomplete".into());
    }
    for action in actions {
        if matches!(action.kind.as_str(), "label_add" | "label_remove") && action.label_id.is_none()
        {
            return Some("label_missing".into());
        }
        if action.kind == "move" && action.folder_id.is_none() && action.folder_role.is_none() {
            return Some("folder_missing".into());
        }
        let role = if action.kind == "move" {
            action.folder_role.as_deref()
        } else {
            takeaway_target_role(&action.kind)
        };
        // Роль проверяем только у правила одного ящика: у правила для всех
        // ящиков папка выбирается в ящике письма уже во время прогона.
        if let (Some(role), Some(account_id)) = (role, account_id) {
            let count = folder_roles
                .iter()
                .filter(|(id, value)| *id == account_id && value.as_deref() == Some(role))
                .count();
            if count != 1 {
                return Some("folder_role_ambiguous".into());
            }
        }
    }
    None
}

/// Перечень правил задания ручного прогона со снимком их версий (S-066).
#[derive(Debug, Clone, serde::Deserialize)]
struct RuleVersion {
    id: String,
    version: i64,
}

#[cfg(test)]
mod smart_condition_legacy_tests {
    use super::*;

    #[test]
    fn legacy_read_state_condition_is_understood() {
        // Так условие "Непрочитанные (все)" лежит в базах ранних версий: из-за
        // старого словаря папка оставалась пустой при непрочитанных письмах.
        let (field, op, value) = normalize_smart_condition(
            "Статус".to_owned(),
            "равно".to_owned(),
            "not_seen".to_owned(),
        );
        assert_eq!(
            (field.as_str(), op.as_str(), value.as_str()),
            ("read_state", "equals", "unread")
        );
    }

    #[test]
    fn legacy_attachment_and_sender_conditions_are_understood() {
        let (field, _, value) =
            normalize_smart_condition("Вложение".to_owned(), "equals".to_owned(), "yes".to_owned());
        assert_eq!((field.as_str(), value.as_str()), ("attachment", "has"));
        let (field, op, _) = normalize_smart_condition(
            "from".to_owned(),
            "содержит".to_owned(),
            "boss@example.com".to_owned(),
        );
        assert_eq!((field.as_str(), op.as_str()), ("sender", "contains"));
    }

    #[test]
    fn absurd_relative_period_does_not_break_the_query() {
        // Период вводит пользователь: "20000000000 недель" переполняли chrono
        // и роняли выборку писем паникой вместо результата.
        let message = MessageMeta {
            id: 1,
            account_id: 1,
            folder_id: 1,
            thread_id: None,
            uid: 1,
            message_id: None,
            from: Addr {
                name: None,
                email: "sender@example.com".to_owned(),
            },
            to: Vec::new(),
            cc: Vec::new(),
            subject: String::new(),
            preview: String::new(),
            // Дата письма считается от текущего момента: с жёстко записанной
            // датой тест перестал бы проходить на следующий день.
            date: Some(chrono::Utc::now().to_rfc3339()),
            size: None,
            flags: Flags::default(),
            has_attachments: false,
            auth: AuthResults::default(),
            labels: Vec::new(),
            pinned_at: None,
            task_due_at: None,
            task_state: None,
            task_start_at: None,
            task_reminder_at: None,
        };
        let huge = SmartCondition {
            field: "date".to_owned(),
            op: "within_last".to_owned(),
            value: "20000000000".to_owned(),
            unit: Some("weeks".to_owned()),
            value2: None,
        };
        assert!(smart_condition_matches(&huge, &message, None, None));
        let older_than = SmartCondition {
            op: "older_than".to_owned(),
            ..huge.clone()
        };
        assert!(!smart_condition_matches(&older_than, &message, None, None));
        // Период, который сам по себе представим, но выносит пороговую дату за
        // границы календаря: раньше падало именно на вычитании.
        let past_calendar = SmartCondition {
            value: "20000000".to_owned(),
            ..huge.clone()
        };
        assert!(smart_condition_matches(
            &past_calendar,
            &message,
            None,
            None
        ));
        let past_calendar_older = SmartCondition {
            op: "older_than".to_owned(),
            ..past_calendar
        };
        assert!(!smart_condition_matches(
            &past_calendar_older,
            &message,
            None,
            None
        ));
        // Отдельная ветка защиты: произведение не помещается в i64 ещё до того,
        // как дело дойдёт до длительности.
        let overflowing_product = SmartCondition {
            value: i64::MAX.to_string(),
            ..huge.clone()
        };
        assert!(smart_condition_matches(
            &overflowing_product,
            &message,
            None,
            None
        ));
        // Обычный период считается как прежде. Берём заведомо старое письмо,
        // чтобы результат не зависел от того, когда гоняются тесты.
        let old_message = MessageMeta {
            date: Some("2000-01-01T10:00:00Z".to_owned()),
            ..message.clone()
        };
        let day = SmartCondition {
            value: "24".to_owned(),
            unit: Some("hours".to_owned()),
            ..huge
        };
        assert!(!smart_condition_matches(&day, &old_message, None, None));
        assert!(smart_condition_matches(&day, &message, None, None));
    }

    #[test]
    fn current_conditions_are_left_alone() {
        let (field, op, value) = normalize_smart_condition(
            "read_state".to_owned(),
            "equals".to_owned(),
            "unread".to_owned(),
        );
        assert_eq!(
            (field.as_str(), op.as_str(), value.as_str()),
            ("read_state", "equals", "unread")
        );
    }

    /// Счётчик умных папок читает метки только тогда, когда хоть одно условие
    /// стоит на поле "label" - иначе он не тратил бы проход по всей таблице
    /// связей. Договор держится на том, что других полей, зависящих от меток,
    /// нет: появится такое поле - счётчик молча разойдётся со списком писем.
    #[test]
    fn only_the_label_field_depends_on_message_labels() {
        let base = MessageMeta {
            id: 1,
            account_id: 1,
            folder_id: 1,
            thread_id: None,
            uid: 1,
            message_id: None,
            from: Addr {
                name: None,
                email: "sender@example.com".to_owned(),
            },
            to: Vec::new(),
            cc: Vec::new(),
            subject: "важное".to_owned(),
            preview: "текст".to_owned(),
            date: Some(chrono::Utc::now().to_rfc3339()),
            size: Some(1024),
            flags: Flags::default(),
            has_attachments: false,
            auth: AuthResults::default(),
            labels: Vec::new(),
            pinned_at: None,
            task_due_at: None,
            task_state: None,
            task_start_at: None,
            task_reminder_at: None,
        };
        let labelled = MessageMeta {
            labels: vec!["важное".to_owned()],
            ..base.clone()
        };
        for field in [
            "sender",
            "recipient",
            "subject",
            "body",
            "account",
            "folder",
            "folder_role",
            "read_state",
            "importance",
            "reply_state",
            "draft_state",
            "attachment",
            "size",
            "date",
        ] {
            let condition = SmartCondition {
                field: field.to_owned(),
                op: "contains".to_owned(),
                value: "важное".to_owned(),
                unit: None,
                value2: None,
            };
            assert_eq!(
                smart_condition_matches(&condition, &base, None, None),
                smart_condition_matches(&condition, &labelled, None, None),
                "поле {field} не должно зависеть от меток письма"
            );
        }
        let by_label = SmartCondition {
            field: "label".to_owned(),
            op: "contains".to_owned(),
            value: "важное".to_owned(),
            unit: None,
            value2: None,
        };
        assert!(!smart_condition_matches(&by_label, &base, None, None));
        assert!(smart_condition_matches(&by_label, &labelled, None, None));
    }
}

#[cfg(test)]
pub(crate) mod test_storage {
    use crate::crypto::{DatabaseKey, StorageCrypto};
    use crate::storage::Db;
    use std::sync::Arc;

    /// Тестовое хранилище: база вместе со своим временным каталогом.
    /// Закрывать обязательно: незакрытые пулы доживают до выхода из процесса,
    /// и их рабочие потоки сталкиваются с обработчиком завершения SQLCipher -
    /// прогон падает уже после "test result: ok"
    /// (см. specs/core-test-process-exit.md).
    pub struct TestDb {
        db: Db,
        root: std::path::PathBuf,
    }

    impl std::ops::Deref for TestDb {
        type Target = Db;

        fn deref(&self) -> &Db {
            &self.db
        }
    }

    impl TestDb {
        /// Закрыть оба пула и убрать за собой временный каталог. Занятый каталог
        /// не роняет тест: результат теста определяют его утверждения.
        pub async fn close(self) {
            self.db.close().await;
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn random_key() -> [u8; 32] {
        use rand::Rng as _;
        let mut key = [0_u8; 32];
        rand::rng().fill_bytes(&mut key);
        key
    }

    /// Открыть тестовое хранилище с миграциями не новее указанной. Нужно
    /// проверкам обновления: база наполняется в прежней схеме, а новые миграции
    /// применяются поверх непустых таблиц.
    pub async fn open_test_db_upto(prefix: &str, version: i64) -> TestDb {
        let (db, root) = open_unmigrated(prefix).await;
        let mut migrator = sqlx::migrate!("./migrations");
        migrator.migrations = std::borrow::Cow::Owned(
            migrator
                .migrations
                .iter()
                .filter(|migration| migration.version <= version)
                .cloned()
                .collect::<Vec<_>>(),
        );
        migrator
            .run(&db.write_pool)
            .await
            .expect("apply migrations up to version");
        TestDb { db, root }
    }

    async fn open_unmigrated(prefix: &str) -> (Db, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("truemail-{prefix}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create temp data dir");
        let crypto = Arc::new(StorageCrypto::from_key(random_key()));
        let database_key = DatabaseKey::from_key(random_key());
        let db = Db::open_with_database_key(&root, crypto, &database_key)
            .await
            .expect("open database");
        (db, root)
    }

    /// Открыть тестовое хранилище с применёнными миграциями. `prefix` попадает
    /// в имя временного каталога и помогает узнать, чей это каталог.
    pub async fn open_test_db(prefix: &str) -> TestDb {
        let root = std::env::temp_dir().join(format!("truemail-{prefix}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create temp data dir");
        let crypto = Arc::new(StorageCrypto::from_key(random_key()));
        let database_key = DatabaseKey::from_key(random_key());
        let db = Db::open_with_database_key(&root, crypto, &database_key)
            .await
            .expect("open database");
        db.migrate().await.expect("migrate database");
        TestDb { db, root }
    }
}

#[cfg(test)]
mod notification_lookup_tests {
    use super::test_storage::{TestDb, open_test_db};
    use super::*;

    /// smart-folder-selection-shared.md, S-002: условие отбора живых писем
    /// объявлено один раз, поэтому все три запроса умной папки содержат его
    /// дословно.
    #[test]
    fn smart_folder_queries_share_one_alive_filter() {
        let alive = message_alive_sql!();
        assert!(SMART_PAGE_FIRST_SQL.contains(alive));
        assert!(SMART_PAGE_AFTER_CURSOR_SQL.contains(alive));
        assert!(SMART_STREAM_SQL.contains(alive));
    }

    /// S-007: список читает страницами с сортировкой, счётчик - потоком без
    /// сортировки и без предела.
    #[test]
    fn list_and_count_keep_their_own_reading_shape() {
        assert!(SMART_PAGE_FIRST_SQL.contains("ORDER BY date DESC, id DESC LIMIT ?"));
        assert!(SMART_PAGE_AFTER_CURSOR_SQL.contains("COALESCE(date, '') < ?"));
        assert!(!SMART_STREAM_SQL.contains("ORDER BY"));
        assert!(!SMART_STREAM_SQL.contains("LIMIT"));
    }

    /// gmail-local-body-prefetch.md, S-002 - S-006, S-011: какие письма
    /// попадают в фоновую догрузку тел.
    #[tokio::test]
    async fn body_prefetch_picks_recent_small_messages_without_a_body() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let folder_id = seed_folder(&db, account_id, "INBOX", Some("inbox")).await;
        let insert = |uid: i64, days_ago: i64, size: i64, body_fetched: i64| {
            let db = &db;
            async move {
                let (id,): (i64,) = sqlx::query_as(
                    "INSERT INTO messages(account_id, folder_id, uid, remote_id, subject, date, size, body_fetched)                      VALUES (?, ?, ?, ?, 'тест', datetime('now', ?), ?, ?) RETURNING id",
                )
                .bind(account_id)
                .bind(folder_id)
                .bind(uid)
                .bind(format!("remote-{uid}"))
                .bind(format!("-{days_ago} days"))
                .bind(size)
                .bind(body_fetched)
                .fetch_one(&db.write_pool)
                .await
                .expect("insert message");
                id
            }
        };
        let fresh = insert(1, 0, 1024, 0).await;
        let older = insert(2, 5, 1024, 0).await;
        let beyond_retention = insert(3, 40, 1024, 0).await;
        let huge = insert(4, 1, 40 * 1024 * 1024, 0).await;
        let already_fetched = insert(5, 1, 1024, 1).await;

        let picked = db
            .messages_missing_body(account_id, 30, 5 * 1024 * 1024, 50)
            .await
            .expect("select messages without body")
            .into_iter()
            .map(|(id, _, _, _)| id)
            .collect::<Vec<_>>();
        // S-004: сначала новые. S-002: старше глубины хранения не берём.
        // S-006: слишком большие пропускаем. S-011: письмо с телом не берём.
        assert_eq!(picked, vec![fresh, older]);
        assert!(!picked.contains(&beyond_retention));
        assert!(!picked.contains(&huge));
        assert!(!picked.contains(&already_fetched));

        // S-003: без ограничения по времени берём и старые письма.
        let unlimited = db
            .messages_missing_body(account_id, 0, 5 * 1024 * 1024, 50)
            .await
            .expect("select without retention")
            .into_iter()
            .map(|(id, _, _, _)| id)
            .collect::<Vec<_>>();
        assert_eq!(unlimited, vec![fresh, older, beyond_retention]);

        // S-005: предел за проход соблюдается.
        let limited = db
            .messages_missing_body(account_id, 0, 5 * 1024 * 1024, 1)
            .await
            .expect("select with limit");
        assert_eq!(limited.len(), 1);
        assert_eq!(limited[0].0, fresh);
        assert_eq!(limited[0].1, "INBOX");
        db.close().await;
    }

    /// Тестовое хранилище с применёнными миграциями. Закрывать обязательно:
    /// см. specs/core-test-process-exit.md.
    async fn test_db() -> TestDb {
        open_test_db("repo-notify").await
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
    async fn folder_signature_reacts_to_new_folders_and_counters() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let inbox_id = seed_folder(&db, account_id, "INBOX", Some("inbox")).await;

        let before = db
            .folder_state_signature(account_id)
            .await
            .expect("signature");
        seed_folder(&db, account_id, "Archive", Some("archive")).await;
        let after_new_folder = db
            .folder_state_signature(account_id)
            .await
            .expect("signature");
        assert_ne!(
            before, after_new_folder,
            "появление папки должно менять слепок"
        );

        sqlx::query("UPDATE folders SET unread_count = 7 WHERE id = ?")
            .bind(inbox_id)
            .execute(&db.write_pool)
            .await
            .expect("update counter");
        let after_counter = db
            .folder_state_signature(account_id)
            .await
            .expect("signature");
        assert_ne!(
            after_new_folder, after_counter,
            "сдвиг счётчика писем должен менять слепок"
        );

        let repeated = db
            .folder_state_signature(account_id)
            .await
            .expect("signature");
        assert_eq!(
            after_counter, repeated,
            "без изменений слепок обязан совпадать"
        );

        // Встречные изменения в разных папках не должны компенсировать друг
        // друга: по суммам слепок остался бы прежним, и обновление потерялось.
        let archive_id: i64 =
            sqlx::query_scalar("SELECT id FROM folders WHERE account_id = ? AND remote_path = ?")
                .bind(account_id)
                .bind("Archive")
                .fetch_one(&db.write_pool)
                .await
                .expect("archive id");
        sqlx::query("UPDATE folders SET unread_count = unread_count - 1 WHERE id = ?")
            .bind(inbox_id)
            .execute(&db.write_pool)
            .await
            .expect("decrement inbox");
        sqlx::query("UPDATE folders SET unread_count = unread_count + 1 WHERE id = ?")
            .bind(archive_id)
            .execute(&db.write_pool)
            .await
            .expect("increment archive");
        let after_swap = db
            .folder_state_signature(account_id)
            .await
            .expect("signature");
        assert_ne!(
            after_counter, after_swap,
            "перенос непрочитанного между папками обязан менять слепок"
        );

        sqlx::query("UPDATE folders SET display_name = 'Входящие' WHERE id = ?")
            .bind(inbox_id)
            .execute(&db.write_pool)
            .await
            .expect("rename folder");
        let after_rename = db
            .folder_state_signature(account_id)
            .await
            .expect("signature");
        assert_ne!(
            after_swap, after_rename,
            "переименование папки обязано менять слепок"
        );
        db.close().await;
    }

    #[tokio::test]
    async fn oldest_message_date_is_the_backfill_cursor() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let folder_id = seed_folder(&db, account_id, "INBOX", Some("inbox")).await;
        assert_eq!(
            db.folder_oldest_message_date(folder_id)
                .await
                .expect("oldest date"),
            None,
            "у пустой папки курсора нет"
        );

        for (uid, date) in [(1_i64, "2026-07-20T10:00:00Z"), (2, "2024-01-02T03:04:05Z")] {
            sqlx::query(
                "INSERT INTO messages(account_id, folder_id, uid, subject, date)                  VALUES (?, ?, ?, 'тест', ?)",
            )
            .bind(account_id)
            .bind(folder_id)
            .bind(uid)
            .bind(date)
            .execute(&db.write_pool)
            .await
            .expect("insert message");
        }

        assert_eq!(
            db.folder_oldest_message_date(folder_id)
                .await
                .expect("oldest date")
                .as_deref(),
            Some("2024-01-02T03:04:05Z"),
            "курсором должна быть самая старая дата папки"
        );
        db.close().await;
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
                None,
                None,
            )
            .await
            .expect("query inbox ids");

        assert_eq!(
            ids,
            vec![inbox_message_id],
            "письмо из архива не должно попасть в выборку"
        );
        db.close().await;
    }

    #[tokio::test]
    async fn old_messages_are_left_out_of_notifications() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let inbox_id = seed_folder(&db, account_id, "INBOX", Some("inbox")).await;

        // Свежее письмо, письмо позапрошлого года и письмо без даты. Дата
        // свежего письма записывается в том же виде, в каком её пишет рабочий
        // код (UTC с "+00:00"), иначе проверка сравнивала бы разные формы.
        let fresh_id = seed_message(&db, account_id, inbox_id, 1, "remote-fresh").await;
        let fresh_date = chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S+00:00")
            .to_string();
        sqlx::query("UPDATE messages SET date = ? WHERE id = ?")
            .bind(&fresh_date)
            .bind(fresh_id)
            .execute(&db.write_pool)
            .await
            .expect("set fresh date");
        let old_id = seed_message(&db, account_id, inbox_id, 2, "remote-old").await;
        let undated_id = seed_message(&db, account_id, inbox_id, 3, "remote-undated").await;
        sqlx::query("UPDATE messages SET date = '2022-07-12T17:00:54+00:00' WHERE id = ?")
            .bind(old_id)
            .execute(&db.write_pool)
            .await
            .expect("set old date");
        sqlx::query("UPDATE messages SET date = NULL WHERE id = ?")
            .bind(undated_id)
            .execute(&db.write_pool)
            .await
            .expect("clear date");

        // Письмо с испорченной датой далеко в будущем: без верхней границы оно
        // считалось бы свежим всегда.
        let future_id = seed_message(&db, account_id, inbox_id, 4, "remote-future").await;
        sqlx::query("UPDATE messages SET date = '2031-01-01T00:00:00+00:00' WHERE id = ?")
            .bind(future_id)
            .execute(&db.write_pool)
            .await
            .expect("set future date");

        // Письмо ровно на нижней границе: границы включающие, оно уведомляется.
        let border_id = seed_message(&db, account_id, inbox_id, 5, "remote-border").await;

        let remote_ids = [
            "remote-fresh".to_owned(),
            "remote-old".to_owned(),
            "remote-undated".to_owned(),
            "remote-future".to_owned(),
            "remote-border".to_owned(),
        ];
        let now = chrono::Utc::now();
        let not_before = (now - chrono::Duration::hours(24))
            .format("%Y-%m-%dT%H:%M:%S+00:00")
            .to_string();
        let not_after = (now + chrono::Duration::hours(24))
            .format("%Y-%m-%dT%H:%M:%S+00:00")
            .to_string();
        sqlx::query("UPDATE messages SET date = ? WHERE id = ?")
            .bind(&not_before)
            .bind(border_id)
            .execute(&db.write_pool)
            .await
            .expect("set border date");
        let mut ids = db
            .inbox_message_ids_by_remote_ids(
                account_id,
                &remote_ids,
                Some(&not_before),
                Some(&not_after),
            )
            .await
            .expect("query inbox ids");
        ids.sort_unstable();

        let mut expected = vec![fresh_id, undated_id, border_id];
        expected.sort_unstable();
        assert_eq!(
            ids, expected,
            "письма 2022 и 2031 годов не попадают; письмо без даты и письмо ровно на границе - попадают"
        );

        let all = db
            .inbox_message_ids_by_remote_ids(account_id, &remote_ids, None, None)
            .await
            .expect("query inbox ids");
        assert_eq!(all.len(), 5, "без границ выбираются все письма Входящих");
        db.close().await;
    }

    #[tokio::test]
    async fn empty_input_returns_empty_result_without_querying() {
        let db = test_db().await;
        let ids = db
            .inbox_message_ids_by_remote_ids(1, &[], None, None)
            .await
            .expect("query with empty input");
        assert!(ids.is_empty());
        db.close().await;
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
        db.close().await;
    }

    /// Счётчик умной папки и её список писем описывают одно и то же множество.
    /// Пути разные (потоковый подсчёт против постраничной выборки), поэтому
    /// расхождение фильтров - самый вероятный способ показать в панели число,
    /// которого в списке нет.
    #[tokio::test]
    async fn smart_folder_count_matches_the_message_list() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let inbox_id = seed_folder(&db, account_id, "INBOX", Some("inbox")).await;
        let archive_id = seed_folder(&db, account_id, "Archive", Some("archive")).await;
        for uid in 0..7 {
            seed_message(&db, account_id, inbox_id, uid, &format!("inbox-{uid}")).await;
        }
        for uid in 0..3 {
            seed_message(
                &db,
                account_id,
                archive_id,
                100 + uid,
                &format!("arch-{uid}"),
            )
            .await;
        }
        // Часть писем прочитана, одно отложено, одно ждёт переноса: все три
        // условия счётчик и выборка обязаны трактовать одинаково.
        sqlx::query("UPDATE messages SET seen=1 WHERE folder_id=? AND uid < 3")
            .bind(inbox_id)
            .execute(&db.write_pool)
            .await
            .expect("mark seen");
        sqlx::query(
            "UPDATE messages SET snoozed_until=datetime('now', '+1 day') WHERE folder_id=? AND uid=6",
        )
        .bind(inbox_id)
        .execute(&db.write_pool)
        .await
        .expect("snooze message");
        // Письмо с незавершённым переносом оба пути обязаны прятать одинаково.
        let (moving_id,): (i64,) =
            sqlx::query_as("SELECT id FROM messages WHERE folder_id=? AND uid=5")
                .bind(inbox_id)
                .fetch_one(&db.write_pool)
                .await
                .expect("find message to move");
        sqlx::query(
            "INSERT INTO outbox_ops(account_id, op_kind, payload, status, message_id)
             VALUES (?, 'move', '{}', 'pending', ?)",
        )
        .bind(account_id)
        .bind(moving_id)
        .execute(&db.write_pool)
        .await
        .expect("queue move");

        let counts = db
            .count_smart_folder_messages(&["all-inbox".to_owned()])
            .await
            .expect("count smart folder");
        let listed = db
            .list_smart_folder_messages("all-inbox", 500)
            .await
            .expect("list smart folder");
        let unread = listed.iter().filter(|message| !message.flags.seen).count() as i64;
        assert_eq!(counts.len(), 1);
        assert_eq!(counts[0].total, listed.len() as i64);
        assert_eq!(counts[0].unread, unread);
        // Письма архива под условие "тип папки равно Входящие" не подходят,
        // отложенное и ожидающее переноса скрыты обоими путями.
        assert_eq!(counts[0].total, 5);
        assert_eq!(counts[0].unread, 2);
        db.close().await;
    }

    /// Счётчик идёт по таблице одним потоком, а список - страницами курсора.
    /// Сверяем их за границей страницы: именно там пути расходятся легче всего.
    #[tokio::test]
    async fn smart_folder_count_matches_a_multi_page_list() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let inbox_id = seed_folder(&db, account_id, "INBOX", Some("inbox")).await;
        // Больше одной страницы скана (SCAN_PAGE_SIZE = 1000) и больше одной
        // страницы выдачи (limit ограничен 500).
        sqlx::query(
            "INSERT INTO messages(account_id, folder_id, uid, subject, date, seen)
             SELECT ?, ?, value, 'тест', datetime('now', '-' || value || ' minutes'), value % 2
             FROM (WITH RECURSIVE seq(value) AS (
                     SELECT 1 UNION ALL SELECT value + 1 FROM seq WHERE value < 1200
                   ) SELECT value FROM seq)",
        )
        .bind(account_id)
        .bind(inbox_id)
        .execute(&db.write_pool)
        .await
        .expect("seed messages");

        let counts = db
            .count_smart_folder_messages(&["all-inbox".to_owned()])
            .await
            .expect("count smart folder");
        let mut listed = 0_i64;
        let mut unread = 0_i64;
        let mut cursor: Option<(String, i64)> = None;
        loop {
            let page = db
                .list_smart_folder_messages_page(
                    "all-inbox",
                    cursor.as_ref().map(|(date, _)| date.as_str()),
                    cursor.as_ref().map(|(_, id)| *id),
                    500,
                )
                .await
                .expect("list smart folder page");
            let Some(last) = page.last() else {
                break;
            };
            cursor = Some((last.date.clone().unwrap_or_default(), last.id));
            listed += page.len() as i64;
            unread += page.iter().filter(|message| !message.flags.seen).count() as i64;
            if page.len() < 500 {
                break;
            }
        }
        assert_eq!(listed, 1200);
        assert_eq!(counts[0].total, listed);
        assert_eq!(counts[0].unread, unread);
        db.close().await;
    }
}

/// Тесты `Db::repair_broken_charset_messages` (issue #41,
/// specs/message-charset-decoding.md). Раскодирование само по себе (фича
/// `mail-parser/full_encoding`) вне границ этой задачи, поэтому письма ниже
/// собраны на чистом ASCII/UTF-8 без легаси-кодировок: заголовки с не-ASCII
/// текстом кодируются RFC 2047 (`=?UTF-8?B?...?=`) - base64+UTF-8 декодируются
/// mail-parser всегда, независимо от фичи. Маркер порчи в тестах имитирует
/// результат старого парсера: он вставляется в столбцы БД руками, а не через
/// реальное декодирование.
#[cfg(test)]
mod charset_repair_tests {
    use super::test_storage::{TestDb, open_test_db};
    use super::*;

    const ESC: char = '\u{1B}';
    const FFFD: char = '\u{FFFD}';

    /// Тестовое хранилище с применёнными миграциями. Закрывать обязательно:
    /// см. specs/core-test-process-exit.md.
    async fn test_db() -> TestDb {
        open_test_db("repo-charset").await
    }

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

    /// Заголовок в кодировке RFC 2047 (base64, UTF-8). Не зависит от фичи
    /// `full_encoding`: base64 + UTF-8 декодируются mail-parser всегда.
    fn encoded_word(text: &str) -> String {
        use base64::Engine as _;
        format!(
            "=?UTF-8?B?{}?=",
            base64::engine::general_purpose::STANDARD.encode(text.as_bytes())
        )
    }

    /// Простое письмо text/plain, без вложений.
    fn build_raw_message(subject: &str, from_name: &str, from_addr: &str, body: &str) -> Vec<u8> {
        // Без завершающего "\r\n" после тела: письмо не multipart, границы
        // MIME нет, и любой хвостовой перевод строки стал бы частью тела -
        // тесты сверяют body_text с телом побайтово.
        format!(
            "From: {} <{}>\r\n\
             Subject: {}\r\n\
             Date: Wed, 02 Sep 2026 10:00:00 +0000\r\n\
             Content-Type: text/plain; charset=utf-8\r\n\
             \r\n\
             {}",
            encoded_word(from_name),
            from_addr,
            encoded_word(subject),
            body,
        )
        .into_bytes()
    }

    /// Письмо только с HTML-частью, без text/plain - `body_text` при разборе
    /// пуст, ровно случай S-003 "письмо без текстовой части".
    fn build_raw_message_html_only(
        subject: &str,
        from_name: &str,
        from_addr: &str,
        html_body: &str,
    ) -> Vec<u8> {
        format!(
            "From: {} <{}>\r\n\
             Subject: {}\r\n\
             Date: Wed, 02 Sep 2026 10:00:00 +0000\r\n\
             Content-Type: text/html; charset=utf-8\r\n\
             \r\n\
             {}\r\n",
            encoded_word(from_name),
            from_addr,
            encoded_word(subject),
            html_body,
        )
        .into_bytes()
    }

    /// Письмо с одним вложением (multipart/mixed). Имя вложения - обычный
    /// ASCII: раскодирование легаси-кодировок вне границ этой задачи, важен
    /// только сам факт починки записи `attachments.filename`.
    fn build_raw_message_with_attachment(
        subject: &str,
        from_name: &str,
        from_addr: &str,
        body: &str,
        attachment_name: &str,
        attachment_body: &str,
    ) -> Vec<u8> {
        let boundary = "boundary-truemail-test";
        format!(
            "From: {} <{}>\r\n\
             Subject: {}\r\n\
             Date: Wed, 02 Sep 2026 10:00:00 +0000\r\n\
             Content-Type: multipart/mixed; boundary=\"{boundary}\"\r\n\
             \r\n\
             --{boundary}\r\n\
             Content-Type: text/plain; charset=utf-8\r\n\
             \r\n\
             {body}\r\n\
             --{boundary}\r\n\
             Content-Type: text/plain; name=\"{attachment_name}\"\r\n\
             Content-Disposition: attachment; filename=\"{attachment_name}\"\r\n\
             \r\n\
             {attachment_body}\r\n\
             --{boundary}--\r\n",
            encoded_word(from_name),
            from_addr,
            encoded_word(subject),
        )
        .into_bytes()
    }

    /// Прямая вставка письма нужным набором полей - тесты чинки собирают
    /// повреждённые строки руками, а не через `save_discovered_messages`.
    #[allow(clippy::too_many_arguments)]
    async fn insert_message(
        db: &Db,
        account_id: i64,
        folder_id: i64,
        uid: i64,
        subject: &str,
        from_name: Option<&str>,
        from_addr: Option<&str>,
        to_addrs: &str,
        cc_addrs: &str,
        preview: &str,
        date: Option<&str>,
        raw_blob_ref: Option<&str>,
    ) -> i64 {
        let (id,): (i64,) = sqlx::query_as(
            "INSERT INTO messages(account_id, folder_id, uid, subject, from_name, from_addr,
                                   to_addrs, cc_addrs, preview, date, raw_blob_ref)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(account_id)
        .bind(folder_id)
        .bind(uid)
        .bind(subject)
        .bind(from_name)
        .bind(from_addr)
        .bind(to_addrs)
        .bind(cc_addrs)
        .bind(preview)
        .bind(date)
        .bind(raw_blob_ref)
        .fetch_one(&db.write_pool)
        .await
        .expect("insert message");
        id
    }

    async fn charset_repair_flag_is_set(db: &Db) -> bool {
        sqlx::query_as::<_, (String,)>(
            "SELECT value FROM storage_meta WHERE key = 'charset_repair_v2'",
        )
        .fetch_optional(&db.pool)
        .await
        .expect("read storage_meta")
        .is_some()
    }

    /// from_name с U+FFFD при чистых теме и превью (S-003) - должен найтись и
    /// починиться сам по себе.
    #[tokio::test]
    async fn detects_and_repairs_broken_from_name_alone() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let folder_id = seed_folder(&db, account_id, "INBOX", Some("inbox")).await;
        let raw = build_raw_message(
            "Тема письма",
            "Иван Петров",
            "ivan@example.test",
            "Текст письма",
        );
        let raw_ref = db.blobs.put(&raw).expect("put blob");
        let message_id = insert_message(
            &db,
            account_id,
            folder_id,
            1,
            "Тема письма",
            Some(&format!("Иван{FFFD}Петров")),
            Some("ivan@example.test"),
            "[]",
            "[]",
            "Текст письма",
            None,
            Some(&raw_ref),
        )
        .await;

        let fixed = db
            .repair_broken_charset_messages()
            .await
            .expect("repair pass");
        assert_eq!(fixed, 1);

        let (from_name,): (Option<String>,) =
            sqlx::query_as("SELECT from_name FROM messages WHERE id = ?")
                .bind(message_id)
                .fetch_one(&db.pool)
                .await
                .expect("read from_name");
        assert_eq!(from_name.as_deref(), Some("Иван Петров"));
        db.close().await;
    }

    /// Маркер только в `to_addrs` при остальных чистых полях (S-003).
    #[tokio::test]
    async fn detects_and_repairs_broken_to_addrs_alone() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let folder_id = seed_folder(&db, account_id, "INBOX", Some("inbox")).await;
        let raw = build_raw_message(
            "Тема письма",
            "Иван Петров",
            "ivan@example.test",
            "Текст письма",
        );
        let raw_ref = db.blobs.put(&raw).expect("put blob");
        let message_id = insert_message(
            &db,
            account_id,
            folder_id,
            1,
            "Тема письма",
            Some("Иван Петров"),
            Some("ivan@example.test"),
            &format!("[{{\"name\":\"Мари{FFFD}\",\"email\":\"maria@example.test\"}}]"),
            "[]",
            "Текст письма",
            None,
            Some(&raw_ref),
        )
        .await;

        let fixed = db
            .repair_broken_charset_messages()
            .await
            .expect("repair pass");
        assert_eq!(fixed, 1);

        let (to_addrs,): (String,) =
            sqlx::query_as("SELECT coalesce(to_addrs, '[]') FROM messages WHERE id = ?")
                .bind(message_id)
                .fetch_one(&db.pool)
                .await
                .expect("read to_addrs");
        assert_eq!(to_addrs, "[]", "у письма без To починка обязана дать []");
        db.close().await;
    }

    /// Письмо без текстовой части: превью и поля `messages` чистые, маркер -
    /// только в `message_content_cache.body_html` (S-003, главный найденный
    /// в ревью пробел детекта).
    #[tokio::test]
    async fn detects_broken_cache_when_message_fields_are_clean() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let folder_id = seed_folder(&db, account_id, "INBOX", Some("inbox")).await;
        let raw = build_raw_message_html_only(
            "Тема письма",
            "Иван Петров",
            "ivan@example.test",
            "<p>Привет</p>",
        );
        let raw_ref = db.blobs.put(&raw).expect("put blob");
        let message_id = insert_message(
            &db,
            account_id,
            folder_id,
            1,
            "Тема письма",
            Some("Иван Петров"),
            Some("ivan@example.test"),
            "[]",
            "[]",
            "",
            None,
            Some(&raw_ref),
        )
        .await;
        sqlx::query(
            "INSERT INTO message_content_cache(message_id, raw_blob_ref, body_html, body_text, attachments_json)
             VALUES (?, ?, ?, NULL, '[]')",
        )
        .bind(message_id)
        .bind(&raw_ref)
        .bind(format!("<p>Привет{FFFD}</p>"))
        .execute(&db.write_pool)
        .await
        .expect("insert broken cache");

        let fixed = db
            .repair_broken_charset_messages()
            .await
            .expect("repair pass");
        assert_eq!(fixed, 1);

        let cached: Option<(i64,)> =
            sqlx::query_as("SELECT message_id FROM message_content_cache WHERE message_id = ?")
                .bind(message_id)
                .fetch_optional(&db.pool)
                .await
                .expect("query cache");
        assert!(
            cached.is_none(),
            "устаревший кэш обязан удаляться при починке (S-004)"
        );
        db.close().await;
    }

    /// Маркер только в `attachments.filename` (S-003, S-005).
    #[tokio::test]
    async fn detects_and_repairs_broken_attachment_filename() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let folder_id = seed_folder(&db, account_id, "INBOX", Some("inbox")).await;
        let raw = build_raw_message_with_attachment(
            "Тема письма",
            "Иван Петров",
            "ivan@example.test",
            "Текст письма",
            "report.txt",
            "содержимое",
        );
        let raw_ref = db.blobs.put(&raw).expect("put blob");
        let message_id = insert_message(
            &db,
            account_id,
            folder_id,
            1,
            "Тема письма",
            Some("Иван Петров"),
            Some("ivan@example.test"),
            "[]",
            "[]",
            "Текст письма",
            None,
            Some(&raw_ref),
        )
        .await;
        sqlx::query(
            "INSERT INTO attachments(message_id, filename, is_inline, fetched) VALUES (?, ?, 0, 0)",
        )
        .bind(message_id)
        .bind(format!("report{FFFD}.txt"))
        .execute(&db.write_pool)
        .await
        .expect("insert broken attachment");

        let fixed = db
            .repair_broken_charset_messages()
            .await
            .expect("repair pass");
        assert_eq!(fixed, 1);

        let (filename,): (String,) =
            sqlx::query_as("SELECT filename FROM attachments WHERE message_id = ?")
                .bind(message_id)
                .fetch_one(&db.pool)
                .await
                .expect("read filename");
        assert_eq!(filename, "report.txt");
        db.close().await;
    }

    /// Маркер только в `messages_fts.body`, остальное чистое (S-003, S-004).
    #[tokio::test]
    async fn detects_and_repairs_broken_fts_body() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let folder_id = seed_folder(&db, account_id, "INBOX", Some("inbox")).await;
        let raw = build_raw_message(
            "Тема письма",
            "Иван Петров",
            "ivan@example.test",
            "Уникальноеслово",
        );
        let raw_ref = db.blobs.put(&raw).expect("put blob");
        let message_id = insert_message(
            &db,
            account_id,
            folder_id,
            1,
            "Тема письма",
            Some("Иван Петров"),
            Some("ivan@example.test"),
            "[]",
            "[]",
            "Уникальноеслово",
            None,
            Some(&raw_ref),
        )
        .await;
        sqlx::query("UPDATE messages_fts SET body = ? WHERE rowid = ?")
            .bind(format!("Уник{FFFD}льноеслово"))
            .bind(message_id)
            .execute(&db.write_pool)
            .await
            .expect("corrupt fts body");

        let fixed = db
            .repair_broken_charset_messages()
            .await
            .expect("repair pass");
        assert_eq!(fixed, 1);

        let (body,): (String,) = sqlx::query_as("SELECT body FROM messages_fts WHERE rowid = ?")
            .bind(message_id)
            .fetch_one(&db.pool)
            .await
            .expect("read fts body");
        assert_eq!(body, "Уникальноеслово");
        db.close().await;
    }

    /// Письмо испорчено сразу в нескольких источниках (тема, имя, кэш,
    /// вложение, FTS) - обрабатывается один раз (S-003) и полностью
    /// починяется по всем направлениям разом (S-004, S-005).
    #[tokio::test]
    async fn message_broken_in_several_sources_is_fixed_once_and_completely() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let folder_id = seed_folder(&db, account_id, "INBOX", Some("inbox")).await;
        let raw = build_raw_message_with_attachment(
            "Тема письма",
            "Иван Петров",
            "ivan@example.test",
            "Текст письма",
            "report.txt",
            "содержимое",
        );
        let raw_ref = db.blobs.put(&raw).expect("put blob");
        let message_id = insert_message(
            &db,
            account_id,
            folder_id,
            1,
            &format!("Тема{ESC} письма"),
            Some(&format!("Иван{FFFD}Петров")),
            Some("ivan@example.test"),
            "[]",
            "[]",
            &format!("Текст{FFFD} письма"),
            None,
            Some(&raw_ref),
        )
        .await;
        sqlx::query(
            "INSERT INTO message_content_cache(message_id, raw_blob_ref, body_html, body_text, attachments_json)
             VALUES (?, ?, NULL, ?, '[]')",
        )
        .bind(message_id)
        .bind(&raw_ref)
        .bind(format!("Текст{FFFD} письма"))
        .execute(&db.write_pool)
        .await
        .expect("insert broken cache");
        sqlx::query(
            "INSERT INTO attachments(message_id, filename, is_inline, fetched) VALUES (?, ?, 0, 0)",
        )
        .bind(message_id)
        .bind(format!("report{FFFD}.txt"))
        .execute(&db.write_pool)
        .await
        .expect("insert broken attachment");
        sqlx::query("UPDATE messages_fts SET body = ? WHERE rowid = ?")
            .bind(format!("Текст{FFFD} письма"))
            .bind(message_id)
            .execute(&db.write_pool)
            .await
            .expect("corrupt fts body");

        let fixed = db
            .repair_broken_charset_messages()
            .await
            .expect("repair pass");
        assert_eq!(
            fixed, 1,
            "письмо, испорченное в нескольких источниках, чинится один раз"
        );

        let (subject, from_name, preview): (String, Option<String>, String) =
            sqlx::query_as("SELECT subject, from_name, preview FROM messages WHERE id = ?")
                .bind(message_id)
                .fetch_one(&db.pool)
                .await
                .expect("read message fields");
        assert_eq!(subject, "Тема письма");
        assert_eq!(from_name.as_deref(), Some("Иван Петров"));
        assert_eq!(preview, "Текст письма");

        let cached: Option<(i64,)> =
            sqlx::query_as("SELECT message_id FROM message_content_cache WHERE message_id = ?")
                .bind(message_id)
                .fetch_optional(&db.pool)
                .await
                .expect("query cache");
        assert!(cached.is_none());

        let (filename,): (String,) =
            sqlx::query_as("SELECT filename FROM attachments WHERE message_id = ?")
                .bind(message_id)
                .fetch_one(&db.pool)
                .await
                .expect("read filename");
        assert_eq!(filename, "report.txt");

        let (body,): (String,) = sqlx::query_as("SELECT body FROM messages_fts WHERE rowid = ?")
            .bind(message_id)
            .fetch_one(&db.pool)
            .await
            .expect("read fts body");
        assert_eq!(body, "Текст письма");
        db.close().await;
    }

    /// Имя контакта чинится по самому свежему по дате письму (S-006).
    #[tokio::test]
    async fn contact_name_is_repaired_from_the_most_recent_message() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let folder_id = seed_folder(&db, account_id, "INBOX", Some("inbox")).await;
        insert_message(
            &db,
            account_id,
            folder_id,
            1,
            "старое письмо",
            Some("Старое Имя"),
            Some("ivan@example.test"),
            "[]",
            "[]",
            "",
            Some("2020-01-01T00:00:00+00:00"),
            None,
        )
        .await;
        insert_message(
            &db,
            account_id,
            folder_id,
            2,
            "новое письмо",
            Some("Новое Имя"),
            // Регистр и пробелы отличаются от email контакта - сравнение
            // должно быть нечувствительным к ним.
            Some(" IVAN@EXAMPLE.TEST "),
            "[]",
            "[]",
            "",
            Some("2026-01-01T00:00:00+00:00"),
            None,
        )
        .await;
        let (contact_id,): (i64,) = sqlx::query_as(
            "INSERT INTO contacts(account_id, uid, display_name) VALUES (?, 'mail:ivan@example.test', ?) RETURNING id",
        )
        .bind(account_id)
        .bind(format!("Ив{FFFD}н"))
        .fetch_one(&db.write_pool)
        .await
        .expect("insert contact");
        sqlx::query("INSERT INTO contact_emails(contact_id, email, kind) VALUES (?, 'ivan@example.test', 'mail')")
            .bind(contact_id)
            .execute(&db.write_pool)
            .await
            .expect("insert contact email");

        db.repair_broken_charset_messages()
            .await
            .expect("repair pass");

        let (display_name,): (String,) =
            sqlx::query_as("SELECT display_name FROM contacts WHERE id = ?")
                .bind(contact_id)
                .fetch_one(&db.pool)
                .await
                .expect("read display_name");
        assert_eq!(display_name, "Новое Имя");
        db.close().await;
    }

    /// Контакту без подходящего письма имя не трогаем (S-006).
    #[tokio::test]
    async fn contact_without_matching_message_is_left_untouched() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let broken_name = format!("Ив{FFFD}н");
        let (contact_id,): (i64,) = sqlx::query_as(
            "INSERT INTO contacts(account_id, uid, display_name) VALUES (?, 'mail:ivan@example.test', ?) RETURNING id",
        )
        .bind(account_id)
        .bind(&broken_name)
        .fetch_one(&db.write_pool)
        .await
        .expect("insert contact");
        sqlx::query("INSERT INTO contact_emails(contact_id, email, kind) VALUES (?, 'ivan@example.test', 'mail')")
            .bind(contact_id)
            .execute(&db.write_pool)
            .await
            .expect("insert contact email");

        db.repair_broken_charset_messages()
            .await
            .expect("repair pass");

        let (display_name,): (String,) =
            sqlx::query_as("SELECT display_name FROM contacts WHERE id = ?")
                .bind(contact_id)
                .fetch_one(&db.pool)
                .await
                .expect("read display_name");
        assert_eq!(display_name, broken_name);
        db.close().await;
    }

    /// Фаза контактов идёт после починки писем (S-006): кандидат сам был
    /// испорчен в `messages.from_name` и годится в кандидаты только после
    /// того, как первая фаза его почистит.
    #[tokio::test]
    async fn contact_repair_uses_message_field_fixed_by_the_earlier_phase() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let folder_id = seed_folder(&db, account_id, "INBOX", Some("inbox")).await;
        let raw = build_raw_message(
            "Тема письма",
            "Иван Петров",
            "ivan@example.test",
            "Текст письма",
        );
        let raw_ref = db.blobs.put(&raw).expect("put blob");
        insert_message(
            &db,
            account_id,
            folder_id,
            1,
            "Тема письма",
            // from_name испорчен - без починки первой фазой не прошёл бы
            // условие "чистое имя" во второй.
            Some(&format!("Иван{FFFD}Петров")),
            Some("ivan@example.test"),
            "[]",
            "[]",
            "Текст письма",
            Some("2026-01-01T00:00:00+00:00"),
            Some(&raw_ref),
        )
        .await;
        let (contact_id,): (i64,) = sqlx::query_as(
            "INSERT INTO contacts(account_id, uid, display_name) VALUES (?, 'mail:ivan@example.test', ?) RETURNING id",
        )
        .bind(account_id)
        .bind(format!("Конт{FFFD}кт"))
        .fetch_one(&db.write_pool)
        .await
        .expect("insert contact");
        sqlx::query("INSERT INTO contact_emails(contact_id, email, kind) VALUES (?, 'ivan@example.test', 'mail')")
            .bind(contact_id)
            .execute(&db.write_pool)
            .await
            .expect("insert contact email");

        db.repair_broken_charset_messages()
            .await
            .expect("repair pass");

        let (display_name,): (String,) =
            sqlx::query_as("SELECT display_name FROM contacts WHERE id = ?")
                .bind(contact_id)
                .fetch_one(&db.pool)
                .await
                .expect("read display_name");
        assert_eq!(display_name, "Иван Петров");
        db.close().await;
    }

    /// Смена `raw_blob_ref` между выборкой и записью (S-014): письмо
    /// пропускается целиком, ничего не меняется.
    #[tokio::test]
    async fn stale_raw_blob_ref_skips_message_without_side_effects() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let folder_id = seed_folder(&db, account_id, "INBOX", Some("inbox")).await;
        // Два реальных blob - "снимок" (что видела выборка) и "текущий" (что
        // успела записать докачка тела/пересинхронизация до вызова починки).
        // Оба валидны, чтобы Ok(false) наступил из-за несовпадения ссылки в
        // UPDATE, а не из-за ошибки чтения blob.
        let raw_snapshot = build_raw_message(
            "Тема письма",
            "Иван Петров",
            "ivan@example.test",
            "Текст письма",
        );
        let snapshot_ref = db.blobs.put(&raw_snapshot).expect("put snapshot blob");
        let raw_current = build_raw_message(
            "Новая тема",
            "Иван Петров",
            "ivan@example.test",
            "Новый текст",
        );
        let current_ref = db.blobs.put(&raw_current).expect("put current blob");
        let message_id = insert_message(
            &db,
            account_id,
            folder_id,
            1,
            &format!("Тема{FFFD} письма"),
            Some("Иван Петров"),
            Some("ivan@example.test"),
            "[]",
            "[]",
            "Текст письма",
            None,
            Some(&current_ref),
        )
        .await;
        sqlx::query(
            "INSERT INTO message_content_cache(message_id, raw_blob_ref, body_html, body_text, attachments_json)
             VALUES (?, ?, NULL, 'старое тело', '[]')",
        )
        .bind(message_id)
        .bind(&current_ref)
        .execute(&db.write_pool)
        .await
        .expect("insert cache");

        // Вызываем приватный шаг починки одного письма напрямую со "снимком"
        // raw_blob_ref, который уже не совпадает с текущим значением в
        // messages - имитация докачки тела/пересинхронизации между выборкой
        // и записью (S-014).
        let fixed = db
            .repair_one_charset_message(message_id, &snapshot_ref)
            .await
            .expect("repair one message");
        assert!(!fixed, "письмо со сменившимся raw_blob_ref пропускается");

        let (subject,): (String,) = sqlx::query_as("SELECT subject FROM messages WHERE id = ?")
            .bind(message_id)
            .fetch_one(&db.pool)
            .await
            .expect("read subject");
        assert_eq!(
            subject,
            format!("Тема{FFFD} письма"),
            "поля письма не должны меняться при пропуске"
        );
        let cached: Option<(String,)> =
            sqlx::query_as("SELECT body_text FROM message_content_cache WHERE message_id = ?")
                .bind(message_id)
                .fetch_optional(&db.pool)
                .await
                .expect("query cache");
        assert_eq!(
            cached.map(|(body,)| body),
            Some("старое тело".to_owned()),
            "кэш не должен удаляться при пропуске"
        );
        db.close().await;
    }

    /// Недоступный blob (S-007): письмо пропускается с предупреждением,
    /// остальные письма прохода чинятся, флаг всё равно выставляется.
    #[tokio::test]
    async fn message_with_missing_blob_is_skipped_others_fixed_and_flag_set() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let folder_id = seed_folder(&db, account_id, "INBOX", Some("inbox")).await;

        // Письмо без реального blob - ссылка ни разу не создавалась через
        // BlobStore::put.
        let broken_message_id = insert_message(
            &db,
            account_id,
            folder_id,
            1,
            &format!("Тема{FFFD} письма"),
            Some("Иван Петров"),
            Some("ivan@example.test"),
            "[]",
            "[]",
            "",
            None,
            Some("missing/never-written"),
        )
        .await;

        let raw = build_raw_message(
            "Другая тема",
            "Мария Иванова",
            "maria@example.test",
            "Текст письма",
        );
        let raw_ref = db.blobs.put(&raw).expect("put blob");
        let good_message_id = insert_message(
            &db,
            account_id,
            folder_id,
            2,
            &format!("Друг{FFFD}я тема"),
            Some("Мария Иванова"),
            Some("maria@example.test"),
            "[]",
            "[]",
            "Текст письма",
            None,
            Some(&raw_ref),
        )
        .await;

        let fixed = db
            .repair_broken_charset_messages()
            .await
            .expect("repair pass");
        assert_eq!(fixed, 1, "починиться должно только письмо с доступным blob");

        let (broken_subject,): (String,) =
            sqlx::query_as("SELECT subject FROM messages WHERE id = ?")
                .bind(broken_message_id)
                .fetch_one(&db.pool)
                .await
                .expect("read broken subject");
        assert_eq!(broken_subject, format!("Тема{FFFD} письма"));

        let (good_subject,): (String,) =
            sqlx::query_as("SELECT subject FROM messages WHERE id = ?")
                .bind(good_message_id)
                .fetch_one(&db.pool)
                .await
                .expect("read good subject");
        assert_eq!(good_subject, "Другая тема");

        assert!(
            charset_repair_flag_is_set(&db).await,
            "флаг ставится, даже если часть писем пропущена (S-008)"
        );
        db.close().await;
    }

    /// Флаг защищает от повторного полного скана (S-008): второй вызов ничего
    /// не делает и не трогает уже починенные поля.
    #[tokio::test]
    async fn second_call_is_a_noop_because_of_the_flag() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let folder_id = seed_folder(&db, account_id, "INBOX", Some("inbox")).await;
        let raw = build_raw_message(
            "Тема письма",
            "Иван Петров",
            "ivan@example.test",
            "Текст письма",
        );
        let raw_ref = db.blobs.put(&raw).expect("put blob");
        insert_message(
            &db,
            account_id,
            folder_id,
            1,
            &format!("Тема{FFFD} письма"),
            Some("Иван Петров"),
            Some("ivan@example.test"),
            "[]",
            "[]",
            "Текст письма",
            None,
            Some(&raw_ref),
        )
        .await;

        let first = db
            .repair_broken_charset_messages()
            .await
            .expect("first pass");
        assert_eq!(first, 1);
        assert!(charset_repair_flag_is_set(&db).await);

        // Ещё одно битое письмо, заведённое уже после первого прохода -
        // второй вызов не должен увидеть даже его: флаг останавливает
        // функцию до всякого обращения к messages.
        let raw2 = build_raw_message("Вторая тема", "Пётр Иванов", "petr@example.test", "Текст");
        let raw_ref2 = db.blobs.put(&raw2).expect("put blob");
        insert_message(
            &db,
            account_id,
            folder_id,
            2,
            &format!("Втор{FFFD}я тема"),
            Some("Пётр Иванов"),
            Some("petr@example.test"),
            "[]",
            "[]",
            "Текст",
            None,
            Some(&raw_ref2),
        )
        .await;

        let second = db
            .repair_broken_charset_messages()
            .await
            .expect("second pass");
        assert_eq!(second, 0, "второй проход обязан быть no-op");
        db.close().await;
    }

    /// Битых писем больше внутреннего размера страницы (S-010) - постраничная
    /// keyset-выборка обязана дойти до конца и починить все.
    #[tokio::test]
    async fn fixes_every_message_when_there_are_more_than_one_page() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let folder_id = seed_folder(&db, account_id, "INBOX", Some("inbox")).await;
        const TOTAL: i64 = 250; // больше PAGE_SIZE (200) внутри repair_broken_charset_messages

        for uid in 0..TOTAL {
            let subject = format!("Тема {uid}");
            let raw = build_raw_message(&subject, "Иван Петров", "ivan@example.test", "Текст");
            let raw_ref = db.blobs.put(&raw).expect("put blob");
            insert_message(
                &db,
                account_id,
                folder_id,
                uid,
                &format!("Тема {uid}{FFFD}"),
                Some("Иван Петров"),
                Some("ivan@example.test"),
                "[]",
                "[]",
                "Текст",
                None,
                Some(&raw_ref),
            )
            .await;
        }

        let fixed = db
            .repair_broken_charset_messages()
            .await
            .expect("repair pass");
        assert_eq!(fixed, TOTAL as usize);

        let remaining = db
            .broken_charset_message_page(0, 10_000)
            .await
            .expect("scan for leftovers");
        assert!(
            remaining.is_empty(),
            "после прохода битых писем оставаться не должно"
        );
        db.close().await;
    }

    /// Прерванный проход продолжается со следующего запуска: уже исправленные
    /// письма выпадают из выборки по отсутствию маркера, флаг при незавершённом
    /// проходе не выставлен (S-011).
    #[tokio::test]
    async fn interrupted_pass_continues_with_remaining_messages() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let folder_id = seed_folder(&db, account_id, "INBOX", Some("inbox")).await;
        let mut ids = Vec::new();
        let mut refs = Vec::new();
        for uid in 1..=2_i64 {
            let subject = format!("Тема {uid}");
            let raw = build_raw_message(&subject, "Иван Петров", "ivan@example.test", "Текст");
            let raw_ref = db.blobs.put(&raw).expect("put blob");
            let id = insert_message(
                &db,
                account_id,
                folder_id,
                uid,
                &format!("Тема {uid}{ESC}"),
                Some("Иван Петров"),
                Some("ivan@example.test"),
                "[]",
                "[]",
                "Текст",
                None,
                Some(&raw_ref),
            )
            .await;
            ids.push(id);
            refs.push(raw_ref);
        }

        // Первое письмо чинится "до аварии": флага нет, проход прерван.
        let fixed = db
            .repair_one_charset_message(ids[0], &refs[0])
            .await
            .expect("repair one message");
        assert!(fixed);
        assert!(
            !charset_repair_flag_is_set(&db).await,
            "флаг ставится только после полного прохода"
        );

        // Следующий запуск видит только оставшееся письмо.
        let repaired = db
            .repair_broken_charset_messages()
            .await
            .expect("repair pass");
        assert_eq!(repaired, 1, "повторно чинится только оставшееся письмо");
        for id in ids {
            let (subject,): (String,) = sqlx::query_as("SELECT subject FROM messages WHERE id = ?")
                .bind(id)
                .fetch_one(&db.pool)
                .await
                .expect("read subject");
            assert!(!subject.contains(ESC), "маркер порчи не остался");
        }
        assert!(charset_repair_flag_is_set(&db).await);
        db.close().await;
    }

    /// После починки письмо находится поиском по слову из декодированной темы:
    /// проверяем не колонку, а сам поисковый путь через MATCH (S-004).
    #[tokio::test]
    async fn repaired_message_is_found_by_search() {
        use crate::search::{Fts5Index, SearchIndex};

        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let folder_id = seed_folder(&db, account_id, "INBOX", Some("inbox")).await;
        let raw = build_raw_message(
            "Отчёт квартальный",
            "Иван Петров",
            "ivan@example.test",
            "Показатели уточнены",
        );
        let raw_ref = db.blobs.put(&raw).expect("put blob");
        let message_id = insert_message(
            &db,
            account_id,
            folder_id,
            1,
            &format!("Отч{FFFD}т квартальный"),
            Some("Иван Петров"),
            Some("ivan@example.test"),
            "[]",
            "[]",
            &format!("Показ{FFFD}тели уточнены"),
            None,
            Some(&raw_ref),
        )
        .await;

        db.repair_broken_charset_messages()
            .await
            .expect("repair pass");

        let index = Fts5Index::new(db.clone());
        let by_subject = index
            .search("квартальный", 10)
            .await
            .expect("search subject");
        assert!(
            by_subject.contains(&message_id),
            "письмо должно находиться по слову из темы"
        );
        let by_body = index.search("уточнены", 10).await.expect("search body");
        assert!(
            by_body.contains(&message_id),
            "письмо должно находиться по слову из тела"
        );
        db.close().await;
    }

    /// У контакта несколько адресов, а даты писем равны: побеждает письмо с
    /// наибольшим идентификатором, результат воспроизводим (S-006).
    #[tokio::test]
    async fn contact_with_several_emails_prefers_the_last_message_on_equal_dates() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let folder_id = seed_folder(&db, account_id, "INBOX", Some("inbox")).await;
        let same_date = Some("2026-01-01T00:00:00+00:00");
        insert_message(
            &db,
            account_id,
            folder_id,
            1,
            "первое",
            Some("Имя По Первому Адресу"),
            Some("ivan@example.test"),
            "[]",
            "[]",
            "",
            same_date,
            None,
        )
        .await;
        insert_message(
            &db,
            account_id,
            folder_id,
            2,
            "второе",
            Some("Имя По Второму Адресу"),
            Some("i.petrov@example.test"),
            "[]",
            "[]",
            "",
            same_date,
            None,
        )
        .await;
        let (contact_id,): (i64,) = sqlx::query_as(
            "INSERT INTO contacts(account_id, uid, display_name) VALUES (?, 'mail:ivan@example.test', ?) RETURNING id",
        )
        .bind(account_id)
        .bind(format!("Ив{ESC}н"))
        .fetch_one(&db.write_pool)
        .await
        .expect("insert contact");
        for email in ["ivan@example.test", "i.petrov@example.test"] {
            sqlx::query(
                "INSERT INTO contact_emails(contact_id, email, kind) VALUES (?, ?, 'mail')",
            )
            .bind(contact_id)
            .bind(email)
            .execute(&db.write_pool)
            .await
            .expect("insert contact email");
        }

        db.repair_broken_charset_messages()
            .await
            .expect("repair pass");

        let (display_name,): (String,) =
            sqlx::query_as("SELECT display_name FROM contacts WHERE id = ?")
                .bind(contact_id)
                .fetch_one(&db.pool)
                .await
                .expect("read display_name");
        assert_eq!(display_name, "Имя По Второму Адресу");
        db.close().await;
    }

    /// Письмо в iso-2022-jp, прошедшее обычным путём синхронизации, попадает в
    /// базу уже разобранным: это связывает фичу full_encoding с тем, что видит
    /// пользователь в списке писем (S-001).
    #[tokio::test]
    async fn sync_saves_iso_2022_jp_message_decoded() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        seed_folder(&db, account_id, "INBOX", Some("inbox")).await;
        let raw = concat!(
            "From: =?iso-2022-jp?B?GyRCJyMnbSdqJ1YnXRsoQg==?= <marketing@example.test>
",
            "To: user@example.test
",
            "Subject: =?iso-2022-jp?B?GyRCJyMnbSdqJ1YnXRsoQiAbJEInXydgJ1MnbSdbGyhC?=
",
            "Date: Wed, 02 Sep 2026 10:00:00 +0000
",
            "MIME-Version: 1.0
",
            "Content-Type: text/plain; charset=\"iso-2022-jp\"
",
            "Content-Transfer-Encoding: quoted-printable
",
            "
",
            "=1B$B'#'m'j'V']=1B(B =1B$B'_'`'S'm'[=1B(B"
        )
        .as_bytes()
        .to_vec();
        db.save_discovered_messages(
            account_id,
            &[crate::backend::DiscoveredMessage {
                folder_path: "INBOX".into(),
                uid: 1,
                remote_id: Some("iso-2022-jp-1".into()),
                size: Some(raw.len() as u32),
                seen: false,
                flagged: false,
                answered: false,
                draft: false,
                raw,
                body_fetched: true,
                has_attachments: None,
            }],
            false,
        )
        .await
        .expect("save message");

        let (subject, from_name, preview): (String, Option<String>, String) =
            sqlx::query_as("SELECT subject, from_name, preview FROM messages WHERE account_id = ?")
                .bind(account_id)
                .fetch_one(&db.pool)
                .await
                .expect("read saved message");
        assert_eq!(subject, "Вышел новый");
        assert_eq!(from_name.as_deref(), Some("Вышел"));
        assert_eq!(preview, "Вышел новый");
        db.close().await;
    }
}

#[cfg(test)]
mod charset_decoding_tests {
    use mail_parser::MessageParser;

    /// issue #62: тема разбита на два закодированных слова, и разрез пришёлся на
    /// середину двухбайтовой буквы "г". Без склейки слов до разбора обрубки
    /// байтов давали два символа замены: "Сер??еевич".
    #[test]
    fn split_encoded_word_keeps_the_character() {
        let raw = concat!(
            "From: test@example.test\r\n",
            "Subject: =?UTF-8?B?0J/QvtC00L/QuNGI0LjRgtC1INC/0LvQsNGC0ZHQtiDQtNC70Y8g0KfQtdGA0L3QvtCyINCh0YLQsNC90LjRgdC70LDQsiDQodC10YDQ?=\r\n",
            " =?UTF-8?B?s9C10LXQstC40Yc=?=\r\n",
            "\r\n",
            "body\r\n"
        );
        let normalized = crate::storage::encoded_words::join_split_encoded_words(raw.as_bytes());
        let message = MessageParser::default()
            .parse(normalized.as_ref())
            .expect("parse");
        assert_eq!(
            message.subject().unwrap_or_default(),
            "Подпишите платёж для Чернов Станислав Сергеевич"
        );
    }

    /// Имя отправителя приходит теми же закодированными словами и ломалось так же.
    #[test]
    fn split_encoded_word_in_sender_name_keeps_the_character() {
        let raw = concat!(
            "From: =?UTF-8?B?0KfQtdGA0L3QvtCyINCh0YLQsNC90LjRgdC70LDQsiDQodC10YDQ?=\r\n",
            " =?UTF-8?B?s9C10LXQstC40Yc=?= <test@example.test>\r\n",
            "Subject: тема\r\n",
            "\r\n",
            "body\r\n"
        );
        let normalized = crate::storage::encoded_words::join_split_encoded_words(raw.as_bytes());
        let message = MessageParser::default()
            .parse(normalized.as_ref())
            .expect("parse");
        let name = message
            .from()
            .and_then(|value| value.first())
            .and_then(|address| address.name())
            .unwrap_or_default()
            .to_owned();
        assert_eq!(name, "Чернов Станислав Сергеевич");
    }

    /// Outlook Exchange рассылает кириллицу в iso-2022-jp: русские буквы лежат
    /// в 7-м ряду JIS X 0208. Без фичи full_encoding у mail-parser такие письма
    /// доходили до интерфейса сырыми байтами с ESC-последовательностями (S-001).
    #[test]
    fn iso_2022_jp_cyrillic_is_decoded() {
        let raw = concat!(
            "From: =?iso-2022-jp?B?GyRCJyMnbSdqJ1YnXRsoQg==?= <marketing@example.test>\r\n",
            "Subject: =?iso-2022-jp?B?GyRCJyMnbSdqJ1YnXRsoQiAbJEInXydgJ1MnbSdbGyhC?=\r\n",
            "MIME-Version: 1.0\r\n",
            "Content-Type: text/plain; charset=\"iso-2022-jp\"\r\n",
            "Content-Transfer-Encoding: quoted-printable\r\n",
            "\r\n",
            "=1B$B'#'m'j'V']=1B(B =1B$B'_'`'S'm'[=1B(B\r\n"
        );
        let message = MessageParser::default()
            .parse(raw.as_bytes())
            .expect("письмо разобрано");
        assert_eq!(message.subject(), Some("Вышел новый"));
        assert_eq!(message.body_text(0).as_deref(), Some("Вышел новый\r\n"));
    }

    /// iso-2022-kr и hz-gb-2312 библиотека разбора сознательно не поддерживает
    /// (решение WHATWG): результат - символы замены, но не сырые байты с ESC.
    /// Пользователю такие кодировки читаемыми не обещаны (S-002).
    #[test]
    fn unsupported_legacy_charsets_do_not_leak_escape_bytes() {
        for charset in ["iso-2022-kr", "hz-gb-2312"] {
            let raw = format!(
                concat!(
                    "From: sender@example.test\r\n",
                    "Subject: legacy\r\n",
                    "MIME-Version: 1.0\r\n",
                    "Content-Type: text/plain; charset=\"{}\"\r\n",
                    "Content-Transfer-Encoding: quoted-printable\r\n",
                    "\r\n",
                    "=1B$B'#'m'j'V']=1B(B\r\n"
                ),
                charset
            );
            let message = MessageParser::default()
                .parse(raw.as_bytes())
                .expect("письмо разобрано");
            let body = message.body_text(0).expect("тело письма").into_owned();
            assert!(
                !body.contains('\u{1B}'),
                "{charset}: в тексте не должно оставаться управляющих байтов"
            );
        }
    }
}

/// Постоянное состояние синхронизации почты аккаунта (mail-sync-visible-state.md).
#[cfg(test)]
mod mail_sync_state_tests {
    use super::test_storage::{TestDb, open_test_db};
    use super::*;

    async fn test_db() -> TestDb {
        open_test_db("repo-mail-sync-state").await
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

    async fn account(db: &Db, account_id: i64) -> Account {
        db.list_accounts()
            .await
            .expect("list accounts")
            .into_iter()
            .find(|a| a.id == account_id)
            .expect("account present")
    }

    // S-001: после миграции новые поля читаются с ожидаемыми значениями по
    // умолчанию - до первого прохода состояние синхронизации отсутствует.
    #[tokio::test]
    async fn fresh_account_has_no_sync_state_after_migration() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        let loaded = account(&db, account_id).await;
        assert_eq!(loaded.last_sync_at, None);
        assert_eq!(loaded.last_sync_error, None);
        assert_eq!(loaded.last_sync_error_kind, None);
        assert!(!loaded.needs_reauth);
        db.close().await;
    }

    // S-002: успешный исход записывает время и очищает прежнюю ошибку.
    #[tokio::test]
    async fn success_outcome_sets_last_sync_at_and_clears_error() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        db.record_mail_sync_outcome(
            account_id,
            &MailSyncOutcome::Failure {
                message: "сервер не отвечает".into(),
                kind: "timeout".into(),
                needs_reauth: false,
            },
        )
        .await
        .expect("record failure");
        db.record_mail_sync_outcome(account_id, &MailSyncOutcome::Success)
            .await
            .expect("record success");
        let loaded = account(&db, account_id).await;
        assert!(loaded.last_sync_at.is_some());
        assert_eq!(loaded.last_sync_error, None);
        assert_eq!(loaded.last_sync_error_kind, None);
        assert!(!loaded.needs_reauth);
        db.close().await;
    }

    // S-003: неудачный исход сохраняет безопасный текст и вид ошибки, не трогая
    // время последнего успеха.
    #[tokio::test]
    async fn failure_outcome_stores_message_and_kind_without_touching_last_success() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        db.record_mail_sync_outcome(account_id, &MailSyncOutcome::Success)
            .await
            .expect("record success");
        let after_success = account(&db, account_id).await;
        db.record_mail_sync_outcome(
            account_id,
            &MailSyncOutcome::Failure {
                message: "сеть недоступна".into(),
                kind: "network_unavailable".into(),
                needs_reauth: false,
            },
        )
        .await
        .expect("record failure");
        let loaded = account(&db, account_id).await;
        assert_eq!(loaded.last_sync_error.as_deref(), Some("сеть недоступна"));
        assert_eq!(
            loaded.last_sync_error_kind.as_deref(),
            Some("network_unavailable")
        );
        // Время последнего успеха не изменилось неудачным проходом.
        assert_eq!(loaded.last_sync_at, after_success.last_sync_at);
        db.close().await;
    }

    // S-004: отказ входа ставит признак "нужен повторный вход".
    #[tokio::test]
    async fn auth_failure_sets_needs_reauth() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        db.record_mail_sync_outcome(
            account_id,
            &MailSyncOutcome::Failure {
                message: "неверный пароль".into(),
                kind: "invalid_credentials".into(),
                needs_reauth: true,
            },
        )
        .await
        .expect("record failure");
        let loaded = account(&db, account_id).await;
        assert!(loaded.needs_reauth);
        db.close().await;
    }

    // S-005: последующий успех снимает ранее установленный признак.
    #[tokio::test]
    async fn success_after_auth_failure_clears_needs_reauth() {
        let db = test_db().await;
        let account_id = seed_account(&db).await;
        db.record_mail_sync_outcome(
            account_id,
            &MailSyncOutcome::Failure {
                message: "неверный пароль".into(),
                kind: "invalid_credentials".into(),
                needs_reauth: true,
            },
        )
        .await
        .expect("record failure");
        db.record_mail_sync_outcome(account_id, &MailSyncOutcome::Success)
            .await
            .expect("record success");
        let loaded = account(&db, account_id).await;
        assert!(!loaded.needs_reauth);
        db.close().await;
    }

    // Классификация видов, требующих повторного входа, строится по
    // error-kinds-and-messages.md (S-016 там), а не заново здесь: from_result
    // должен просто перенести error.requires_reauth() в поле needs_reauth.
    #[test]
    fn from_result_carries_requires_reauth_from_error_kind() {
        let auth_error: crate::Result<()> = Err(crate::Error::classified_backend(
            "test",
            crate::error::ErrorKind::InvalidCredentials,
            "неверный пароль",
        ));
        match MailSyncOutcome::from_result(&auth_error) {
            MailSyncOutcome::Failure { needs_reauth, .. } => assert!(needs_reauth),
            MailSyncOutcome::Success => panic!("ожидался Failure"),
        }
        let timeout_error: crate::Result<()> = Err(crate::Error::classified_backend(
            "test",
            crate::error::ErrorKind::Timeout,
            "нет ответа",
        ));
        match MailSyncOutcome::from_result(&timeout_error) {
            MailSyncOutcome::Failure { needs_reauth, .. } => assert!(!needs_reauth),
            MailSyncOutcome::Success => panic!("ожидался Failure"),
        }
        let ok: crate::Result<()> = Ok(());
        assert!(matches!(
            MailSyncOutcome::from_result(&ok),
            MailSyncOutcome::Success
        ));
    }
}

#[cfg(test)]
mod mail_rules_tests {
    use super::test_storage::{TestDb, open_test_db};
    use super::*;

    async fn test_db() -> TestDb {
        open_test_db("repo-rules").await
    }

    async fn seed_account(db: &Db, email: &str) -> i64 {
        sqlx::query_as::<_, (i64,)>(
            "INSERT INTO accounts(uuid, email, provider, backend_kind, auth_kind)
             VALUES(?, ?, 'generic', 'imap', 'password') RETURNING id",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(email)
        .fetch_one(&db.write_pool)
        .await
        .expect("insert account")
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
        .expect("insert folder")
        .0
    }

    struct MessageSeed<'a> {
        uid: i64,
        from_addr: &'a str,
        subject: &'a str,
        backfilled: bool,
    }

    impl<'a> MessageSeed<'a> {
        fn new(uid: i64, from_addr: &'a str, subject: &'a str) -> Self {
            Self {
                uid,
                from_addr,
                subject,
                backfilled: false,
            }
        }
    }

    async fn seed_message(db: &Db, account_id: i64, folder_id: i64, seed: MessageSeed<'_>) -> i64 {
        sqlx::query_as::<_, (i64,)>(
            "INSERT INTO messages(account_id, folder_id, uid, from_name, from_addr, subject,
                                  preview, size, backfilled, remote_id)
             VALUES(?, ?, ?, 'Отправитель', ?, ?, 'предпросмотр', 2048, ?, ?) RETURNING id",
        )
        .bind(account_id)
        .bind(folder_id)
        .bind(seed.uid)
        .bind(seed.from_addr)
        .bind(seed.subject)
        .bind(seed.backfilled as i64)
        .bind(format!("remote-{}-{}", folder_id, seed.uid))
        .fetch_one(&db.write_pool)
        .await
        .expect("insert message")
        .0
    }

    fn condition(field: &str, op: &str, value: &str) -> MailRuleCondition {
        MailRuleCondition {
            field: field.into(),
            op: op.into(),
            value: value.into(),
            unit: None,
            value2: None,
        }
    }

    fn group(logic: &str, conditions: Vec<MailRuleCondition>) -> MailRuleGroup {
        MailRuleGroup {
            logic: logic.into(),
            conditions,
        }
    }

    fn action(kind: &str) -> MailRuleAction {
        MailRuleAction {
            kind: kind.into(),
            folder_id: None,
            folder_role: None,
            label_id: None,
        }
    }

    fn rule_input(
        id: &str,
        account_id: Option<i64>,
        groups: Vec<MailRuleGroup>,
        actions: Vec<MailRuleAction>,
    ) -> MailRuleInput {
        MailRuleInput {
            id: id.into(),
            name: format!("Правило {id}"),
            account_id,
            enabled: true,
            groups,
            exceptions: Vec::new(),
            actions,
            confirm_key: None,
        }
    }

    async fn pending_operations(db: &Db) -> Vec<(i64, Option<i64>, String, String)> {
        sqlx::query_as("SELECT id, message_id, op_kind, status FROM outbox_ops ORDER BY id")
            .fetch_all(&db.pool)
            .await
            .expect("read outbox")
    }

    /// S-015 - S-017: совпала обычная группа - правило применяется, совпала
    /// группа исключений - не применяется.
    #[tokio::test]
    async fn groups_and_exceptions_decide_applicability() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-groups@example.test").await;
        let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
        seed_folder(&db, account, "Archive", Some("archive")).await;
        let wanted = seed_message(
            &db,
            account,
            inbox,
            MessageSeed::new(1, "boss@example.test", "Отчёт за месяц"),
        )
        .await;
        let excluded = seed_message(
            &db,
            account,
            inbox,
            MessageSeed::new(2, "boss@example.test", "Отчёт срочно"),
        )
        .await;
        let mut rule = rule_input(
            "r1",
            Some(account),
            vec![group(
                "all",
                vec![condition("subject", "contains", "отчёт")],
            )],
            vec![action("archive")],
        );
        rule.exceptions = vec![group(
            "any",
            vec![condition("subject", "contains", "срочно")],
        )];
        db.save_mail_rule(&rule, true, None)
            .await
            .expect("save rule");
        db.process_mail_rules().await.expect("process");
        let operations = pending_operations(&db).await;
        assert_eq!(operations.len(), 1);
        assert_eq!(operations[0].1, Some(wanted));
        assert!(operations.iter().all(|row| row.1 != Some(excluded)));
        db.close().await;
    }

    /// S-016: логика `any` внутри группы засчитывает одно совпавшее условие.
    #[tokio::test]
    async fn any_logic_needs_one_condition() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-any@example.test").await;
        let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
        seed_folder(&db, account, "Archive", Some("archive")).await;
        seed_message(
            &db,
            account,
            inbox,
            MessageSeed::new(1, "news@example.test", "Письмо"),
        )
        .await;
        let rule = rule_input(
            "any-rule",
            Some(account),
            vec![group(
                "any",
                vec![
                    condition("subject", "contains", "не подходит"),
                    condition("sender", "contains", "news@"),
                ],
            )],
            vec![action("archive")],
        );
        db.save_mail_rule(&rule, true, None)
            .await
            .expect("save rule");
        db.process_mail_rules().await.expect("process");
        assert_eq!(pending_operations(&db).await.len(), 1);
        db.close().await;
    }

    /// S-024: поле точного адреса сравнивает канонический адрес целиком, а
    /// отображаемое имя в сравнение не входит.
    #[tokio::test]
    async fn sender_address_compares_canonical_address() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-addr@example.test").await;
        let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
        seed_folder(&db, account, "Archive", Some("archive")).await;
        seed_message(
            &db,
            account,
            inbox,
            MessageSeed::new(1, "Bills@Example.TEST", "Счёт"),
        )
        .await;
        let rule = rule_input(
            "addr-rule",
            Some(account),
            vec![group(
                "all",
                vec![condition("sender_address", "equals", "bills@example.test")],
            )],
            vec![action("archive")],
        );
        db.save_mail_rule(&rule, true, None)
            .await
            .expect("save rule");
        db.process_mail_rules().await.expect("process");
        assert_eq!(pending_operations(&db).await.len(), 1);
        db.close().await;
    }

    /// S-032 - S-034, S-036: правила перебираются по порядку, применяются все
    /// подходящие, а действие остановки закрывает письмо для правил ниже.
    #[tokio::test]
    async fn rule_order_actions_and_stop() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-order@example.test").await;
        let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
        seed_folder(&db, account, "Archive", Some("archive")).await;
        let label: i64 = sqlx::query_as::<_, (i64,)>(
            "INSERT INTO labels(name, color) VALUES('Важное', '#ff0000') RETURNING id",
        )
        .fetch_one(&db.write_pool)
        .await
        .expect("insert label")
        .0;
        let message = seed_message(
            &db,
            account,
            inbox,
            MessageSeed::new(1, "boss@example.test", "Письмо"),
        )
        .await;
        let mut first = rule_input(
            "rule-a",
            Some(account),
            vec![group("all", vec![condition("sender", "contains", "boss@")])],
            vec![
                MailRuleAction {
                    kind: "label_add".into(),
                    folder_id: None,
                    folder_role: None,
                    label_id: Some(label),
                },
                action("mark_read"),
            ],
        );
        first.name = "Первое".into();
        db.save_mail_rule(&first, true, None)
            .await
            .expect("save first");
        let second = rule_input(
            "rule-b",
            Some(account),
            vec![group("all", vec![condition("sender", "contains", "boss@")])],
            vec![action("archive"), action("stop")],
        );
        db.save_mail_rule(&second, true, None)
            .await
            .expect("save second");
        let third = rule_input(
            "rule-c",
            Some(account),
            vec![group("all", vec![condition("sender", "contains", "boss@")])],
            vec![action("mark_flagged")],
        );
        db.save_mail_rule(&third, true, None)
            .await
            .expect("save third");
        db.process_mail_rules().await.expect("process");
        let (seen, flagged): (i64, i64) =
            sqlx::query_as("SELECT seen, flagged FROM messages WHERE id=?")
                .bind(message)
                .fetch_one(&db.pool)
                .await
                .expect("read flags");
        assert_eq!(seen, 1, "второе действие первого правила выполнено");
        assert_eq!(flagged, 0, "правило после остановки не выполняется");
        let labels: (i64,) =
            sqlx::query_as("SELECT count(*) FROM message_labels WHERE message_id=?")
                .bind(message)
                .fetch_one(&db.pool)
                .await
                .expect("read labels");
        assert_eq!(labels.0, 1);
        db.close().await;
    }

    /// S-002, S-003, S-007: второе уводящее действие по письму не ставится, а
    /// стадия не трогает уже стоящую операцию.
    #[tokio::test]
    async fn one_takeaway_per_message() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-single@example.test").await;
        let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
        seed_folder(&db, account, "Archive", Some("archive")).await;
        seed_folder(&db, account, "Spam", Some("spam")).await;
        seed_message(
            &db,
            account,
            inbox,
            MessageSeed::new(1, "boss@example.test", "Письмо"),
        )
        .await;
        for (id, kind) in [("rule-1", "archive"), ("rule-2", "spam")] {
            let rule = rule_input(
                id,
                Some(account),
                vec![group("all", vec![condition("sender", "contains", "boss@")])],
                vec![action(kind)],
            );
            db.save_mail_rule(&rule, true, None).await.expect("save");
        }
        db.process_mail_rules().await.expect("process");
        let operations = pending_operations(&db).await;
        assert_eq!(operations.len(), 1, "письмо уводится один раз");
        db.close().await;
    }

    /// S-003, S-004: подготовка данных миграции. На базе с операцией без связи
    /// с письмом и с двумя уводами одного письма ограничение обязано
    /// примениться, иначе у такого пользователя обновление просто не пройдёт.
    #[tokio::test]
    async fn queue_constraint_migration_repairs_links_and_duplicates() {
        const RULES_MIGRATION: &str = include_str!("../../migrations/0041_mail_rule_groups.sql");
        let db = test_db().await;
        let account = seed_account(&db, "rules-migration@example.test").await;
        let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
        let orphan = seed_message(
            &db,
            account,
            inbox,
            MessageSeed::new(1, "a@example.test", "Без связи"),
        )
        .await;
        let duplicated = seed_message(
            &db,
            account,
            inbox,
            MessageSeed::new(2, "b@example.test", "С дублями"),
        )
        .await;
        // База прежней версии: ограничения ещё нет.
        sqlx::query("DROP INDEX idx_outbox_single_takeaway")
            .execute(&db.write_pool)
            .await
            .expect("drop constraint");
        // Операция, созданная до появления столбца связи (0012): номер письма
        // живёт только в её данных.
        sqlx::query(
            "INSERT INTO outbox_ops(account_id, message_id, op_kind, payload, status)
             VALUES(?, NULL, 'move', ?, 'pending')",
        )
        .bind(account)
        .bind(format!("{{\"message_id\": {orphan}}}"))
        .execute(&db.write_pool)
        .await
        .expect("insert orphan operation");
        let mut duplicates = Vec::new();
        for _ in 0..2 {
            let inserted: (i64,) = sqlx::query_as(
                "INSERT INTO outbox_ops(account_id, message_id, op_kind, payload, status)
                 VALUES(?, ?, 'move', '{}', 'pending') RETURNING id",
            )
            .bind(account)
            .bind(duplicated)
            .fetch_one(&db.write_pool)
            .await
            .expect("insert duplicate operation");
            duplicates.push(inserted.0);
        }
        // Выполняем ровно ту часть миграции, которая готовит данные и создаёт
        // ограничение: проверяем поставляемый SQL, а не его пересказ.
        let prepare = RULES_MIGRATION
            .split_once("-- S-004: связь операции")
            .expect("подготовительная часть миграции")
            .1;
        sqlx::raw_sql(AssertSqlSafe(format!("-- S-004: связь операции{prepare}")))
            .execute(&db.write_pool)
            .await
            .expect("подготовка данных и создание ограничения");
        let restored: (Option<i64>,) =
            sqlx::query_as("SELECT message_id FROM outbox_ops WHERE payload LIKE '%message_id%'")
                .fetch_one(&db.pool)
                .await
                .expect("read restored link");
        assert_eq!(
            restored.0,
            Some(orphan),
            "пустая связь операции с письмом восстановлена из её данных"
        );
        let left: Vec<(i64,)> =
            sqlx::query_as("SELECT id FROM outbox_ops WHERE message_id=? ORDER BY id")
                .bind(duplicated)
                .fetch_all(&db.pool)
                .await
                .expect("read duplicates");
        assert_eq!(
            left,
            vec![(duplicates[0],)],
            "из дублей остаётся операция с наименьшим номером"
        );
        let second = sqlx::query(
            "INSERT INTO outbox_ops(account_id, message_id, op_kind, payload, status)
             VALUES(?, ?, 'delete', '{}', 'pending')",
        )
        .bind(account)
        .bind(duplicated)
        .execute(&db.write_pool)
        .await;
        assert!(
            second.is_err(),
            "ограничение применено и второй увод письма отвергает"
        );
        db.close().await;
    }

    /// S-005: ограничение очереди отвергает второй увод письма даже при
    /// прямой вставке, а групповое действие продолжает работать.
    #[tokio::test]
    async fn queue_constraint_skips_busy_message() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-conflict@example.test").await;
        let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
        seed_folder(&db, account, "Archive", Some("archive")).await;
        seed_folder(&db, account, "Trash", Some("trash")).await;
        let first = seed_message(
            &db,
            account,
            inbox,
            MessageSeed::new(1, "a@example.test", "Первое"),
        )
        .await;
        let second = seed_message(
            &db,
            account,
            inbox,
            MessageSeed::new(2, "b@example.test", "Второе"),
        )
        .await;
        db.queue_message_action(&[first], "archive")
            .await
            .expect("queue first");
        // Пометка операции как отправляемой: заменить её пользователь уже не
        // может, а групповое действие обязано продолжиться (S-005, S-006).
        sqlx::query("UPDATE outbox_ops SET status='processing' WHERE message_id=?")
            .bind(first)
            .execute(&db.write_pool)
            .await
            .expect("mark processing");
        let busy = db
            .queue_message_action(&[first], "trash")
            .await
            .expect("занятое письмо не ошибка, а пропуск");
        assert!(busy.operation_ids.is_empty());
        assert_eq!(busy.skipped, 1);
        assert_eq!(busy.skipped_busy, 1, "названа причина пропуска");
        // Главное в S-005: одно занятое письмо не отменяет действие над
        // остальными письмами того же группового вызова.
        let group = db
            .queue_message_action(&[first, second], "trash")
            .await
            .expect("групповое действие продолжается");
        assert_eq!(
            group.operation_ids.len(),
            1,
            "свободное письмо группы поставлено в очередь"
        );
        assert_eq!(group.skipped, 1);
        assert_eq!(group.skipped_busy, 1);
        let pending: Vec<(i64,)> = sqlx::query_as(
            "SELECT message_id FROM outbox_ops WHERE status='pending' AND op_kind='move'",
        )
        .fetch_all(&db.pool)
        .await
        .expect("read queue");
        assert_eq!(
            pending,
            vec![(second,)],
            "операция второго письма пережила пропуск первого"
        );
        db.close().await;
    }

    /// S-006: пока операция не ушла на сервер, пользователь может передумать -
    /// его новая операция заменяет собственную незавершённую.
    #[tokio::test]
    async fn user_replaces_own_pending_operation() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-replace@example.test").await;
        let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
        seed_folder(&db, account, "Archive", Some("archive")).await;
        let trash = seed_folder(&db, account, "Trash", Some("trash")).await;
        let message = seed_message(
            &db,
            account,
            inbox,
            MessageSeed::new(1, "a@example.test", "Письмо"),
        )
        .await;
        db.queue_message_action(&[message], "archive")
            .await
            .expect("queue archive");
        db.queue_message_action(&[message], "trash")
            .await
            .expect("queue trash");
        let operations = pending_operations(&db).await;
        assert_eq!(operations.len(), 1);
        let payload: (String,) = sqlx::query_as("SELECT payload FROM outbox_ops")
            .fetch_one(&db.pool)
            .await
            .expect("read payload");
        assert!(payload.0.contains(&format!("\"target_folder_id\":{trash}")));
        db.close().await;
    }

    /// S-012, S-013: перенос в корзину письма, уже лежащего в корзине, не
    /// превращается в безвозвратное удаление, а явное удаление ставится.
    #[tokio::test]
    async fn trash_move_never_becomes_permanent_delete() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-trash@example.test").await;
        let trash = seed_folder(&db, account, "Trash", Some("trash")).await;
        let message = seed_message(
            &db,
            account,
            trash,
            MessageSeed::new(1, "a@example.test", "Письмо"),
        )
        .await;
        let queued = db
            .queue_message_action(&[message], "trash")
            .await
            .expect("queue trash");
        assert!(queued.operation_ids.is_empty());
        assert!(pending_operations(&db).await.is_empty());
        let deleted = db
            .queue_message_action(&[message], "delete")
            .await
            .expect("queue delete");
        assert_eq!(deleted.operation_ids.len(), 1);
        assert_eq!(pending_operations(&db).await[0].2, "delete");
        db.close().await;
    }

    /// S-008 - S-010: уведённое правилом письмо помечается стадией и в список
    /// для уведомления не попадает.
    #[tokio::test]
    async fn queued_message_is_closed_and_not_notified() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-notify@example.test").await;
        let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
        seed_folder(&db, account, "Archive", Some("archive")).await;
        let message = seed_message(
            &db,
            account,
            inbox,
            MessageSeed::new(1, "a@example.test", "Письмо"),
        )
        .await;
        let rule = rule_input(
            "notify-rule",
            Some(account),
            vec![group("all", vec![condition("sender", "contains", "a@")])],
            vec![action("archive")],
        );
        db.save_mail_rule(&rule, true, None).await.expect("save");
        db.process_mail_rules().await.expect("process");
        let stage: (Option<String>,) =
            sqlx::query_as("SELECT closed_by_stage FROM messages WHERE id=?")
                .bind(message)
                .fetch_one(&db.pool)
                .await
                .expect("read stage");
        assert_eq!(stage.0.as_deref(), Some(RULES_STAGE_NAME));
        let ids = db
            .inbox_message_ids_by_remote_ids(account, &[format!("remote-{inbox}-1")], None, None)
            .await
            .expect("notification ids");
        assert!(ids.is_empty(), "уведённое письмо в уведомление не попадает");
        assert!(!db.message_is_notifiable(message).await.expect("check"));
        db.close().await;
    }

    /// S-041 - S-043: местные действия меняют локальную базу и ставят
    /// изменение признака в очередь, а повтор ничего не ломает.
    #[tokio::test]
    async fn local_actions_are_idempotent_and_queue_flags() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-local@example.test").await;
        let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
        let message = seed_message(
            &db,
            account,
            inbox,
            MessageSeed::new(1, "a@example.test", "Письмо"),
        )
        .await;
        let label: i64 =
            sqlx::query_as::<_, (i64,)>("INSERT INTO labels(name) VALUES('Метка') RETURNING id")
                .fetch_one(&db.write_pool)
                .await
                .expect("insert label")
                .0;
        let rule = rule_input(
            "local-rule",
            Some(account),
            vec![group("all", vec![condition("sender", "contains", "a@")])],
            vec![
                MailRuleAction {
                    kind: "label_add".into(),
                    folder_id: None,
                    folder_role: None,
                    label_id: Some(label),
                },
                action("mark_read"),
            ],
        );
        db.save_mail_rule(&rule, true, None).await.expect("save");
        db.process_mail_rules().await.expect("process");
        db.save_mail_rule(&rule, true, None)
            .await
            .expect("save again");
        db.process_mail_rules().await.expect("process again");
        let labels: (i64,) =
            sqlx::query_as("SELECT count(*) FROM message_labels WHERE message_id=?")
                .bind(message)
                .fetch_one(&db.pool)
                .await
                .expect("count labels");
        assert_eq!(labels.0, 1, "повторная метка не удваивается");
        let flags: (i64,) =
            sqlx::query_as("SELECT count(*) FROM outbox_ops WHERE op_kind='flag' AND message_id=?")
                .bind(message)
                .fetch_one(&db.pool)
                .await
                .expect("count flag ops");
        assert_eq!(
            flags.0, 1,
            "отметка о прочтении ставится в очередь один раз"
        );
        db.close().await;
    }

    /// S-036, S-041, S-042: цепочка из двух отметок выполняется целиком в
    /// обоих порядках. Снимок письма при этом служит только сверке условий:
    /// раньше вторая отметка брала первый признак из снимка и отменяла первую.
    #[tokio::test]
    async fn mark_read_and_mark_flagged_chain_keeps_both_flags() {
        for (first, second) in [("mark_read", "mark_flagged"), ("mark_flagged", "mark_read")] {
            let db = test_db().await;
            let account = seed_account(&db, "rules-flags@example.test").await;
            let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
            let message = seed_message(
                &db,
                account,
                inbox,
                MessageSeed::new(1, "a@example.test", "Письмо"),
            )
            .await;
            let rule = rule_input(
                "flags-rule",
                Some(account),
                vec![group("all", vec![condition("sender", "contains", "a@")])],
                vec![action(first), action(second)],
            );
            db.save_mail_rule(&rule, true, None).await.expect("save");
            db.process_mail_rules().await.expect("process");
            let flags: (i64, i64) = sqlx::query_as("SELECT seen, flagged FROM messages WHERE id=?")
                .bind(message)
                .fetch_one(&db.pool)
                .await
                .expect("read flags");
            assert_eq!(
                flags,
                (1, 1),
                "оба признака выставлены при порядке {first} затем {second}"
            );
            let queued: (String,) = sqlx::query_as(
                "SELECT payload FROM outbox_ops WHERE op_kind='flag' AND message_id=?",
            )
            .bind(message)
            .fetch_one(&db.pool)
            .await
            .expect("read flag operation");
            let payload: serde_json::Value =
                serde_json::from_str(&queued.0).expect("разобрать данные операции");
            assert_eq!(payload["seen"], serde_json::Value::Bool(true));
            assert_eq!(payload["flagged"], serde_json::Value::Bool(true));
            db.close().await;
        }
    }

    /// Сохранение одного правила с отметкой "применить к уже загруженным" не
    /// заставляет остальные правила заново разбирать всю историю писем: каждое
    /// правило проверяет собственный прогресс.
    #[tokio::test]
    async fn saved_rule_does_not_rewind_other_rules() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-progress@example.test").await;
        let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
        let label: i64 =
            sqlx::query_as::<_, (i64,)>("INSERT INTO labels(name) VALUES('Метка') RETURNING id")
                .fetch_one(&db.write_pool)
                .await
                .expect("insert label")
                .0;
        let message = seed_message(
            &db,
            account,
            inbox,
            MessageSeed::new(1, "a@example.test", "Письмо"),
        )
        .await;
        let labelling = rule_input(
            "label-rule",
            Some(account),
            vec![group("all", vec![condition("sender", "contains", "a@")])],
            vec![MailRuleAction {
                kind: "label_add".into(),
                folder_id: None,
                folder_role: None,
                label_id: Some(label),
            }],
        );
        db.save_mail_rule(&labelling, true, None)
            .await
            .expect("save labelling rule");
        db.process_mail_rules().await.expect("process");
        sqlx::query("DELETE FROM message_labels WHERE message_id=?")
            .bind(message)
            .execute(&db.write_pool)
            .await
            .expect("пользователь снял метку сам");
        // Второе правило сохраняется с отметкой "применить к уже загруженным":
        // его прогресс обнуляется, а прогресс первого правила остаётся.
        let marking = rule_input(
            "mark-rule",
            Some(account),
            vec![group(
                "all",
                vec![condition("subject", "contains", "Письмо")],
            )],
            vec![action("mark_read")],
        );
        db.save_mail_rule(&marking, true, None)
            .await
            .expect("save marking rule");
        db.process_mail_rules().await.expect("process again");
        let labels: (i64,) =
            sqlx::query_as("SELECT count(*) FROM message_labels WHERE message_id=?")
                .bind(message)
                .fetch_one(&db.pool)
                .await
                .expect("count labels");
        assert_eq!(
            labels.0, 0,
            "правило, уже прошедшее письмо, второй раз его не трогает"
        );
        let seen: (i64,) = sqlx::query_as("SELECT seen FROM messages WHERE id=?")
            .bind(message)
            .fetch_one(&db.pool)
            .await
            .expect("read seen");
        assert_eq!(seen.0, 1, "новое правило письмо всё же разобрало");
        db.close().await;
    }

    /// S-014: ящик без папки корзины называется отдельно - иначе стадии
    /// обработки молча оставляют его почту на месте.
    #[tokio::test]
    async fn account_without_trash_is_reported() {
        let db = test_db().await;
        let without = seed_account(&db, "rules-no-trash@example.test").await;
        seed_folder(&db, without, "INBOX", Some("inbox")).await;
        let with = seed_account(&db, "rules-with-trash@example.test").await;
        seed_folder(&db, with, "INBOX", Some("inbox")).await;
        seed_folder(&db, with, "Trash", Some("trash")).await;
        let reported = db
            .accounts_without_trash()
            .await
            .expect("list accounts without trash");
        assert_eq!(reported, vec![without]);
        db.close().await;
    }

    /// S-008, S-053: выход из состояния отказа возвращает письмо в обработку -
    /// признак закрывшей его стадии снимается и повтором, и отказом.
    #[tokio::test]
    async fn failure_exit_returns_message_to_processing() {
        for discard in [false, true] {
            let db = test_db().await;
            let account = seed_account(&db, "rules-exit@example.test").await;
            let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
            seed_folder(&db, account, "Archive", Some("archive")).await;
            let message = seed_message(
                &db,
                account,
                inbox,
                MessageSeed::new(1, "a@example.test", "Письмо"),
            )
            .await;
            let rule = rule_input(
                "exit-rule",
                Some(account),
                vec![group("all", vec![condition("sender", "contains", "a@")])],
                vec![action("archive")],
            );
            db.save_mail_rule(&rule, true, None).await.expect("save");
            db.process_mail_rules().await.expect("process");
            let operation: (i64,) =
                sqlx::query_as("SELECT id FROM outbox_ops WHERE message_id=? AND op_kind='move'")
                    .bind(message)
                    .fetch_one(&db.pool)
                    .await
                    .expect("read operation");
            sqlx::query("UPDATE outbox_ops SET status='failed', last_error='отказ' WHERE id=?")
                .bind(operation.0)
                .execute(&db.write_pool)
                .await
                .expect("mark failed");
            assert!(
                !db.message_is_notifiable(message).await.expect("check"),
                "закрытое стадией письмо в уведомление не идёт"
            );
            if discard {
                db.discard_failed_operation(operation.0)
                    .await
                    .expect("discard");
            } else {
                db.retry_failed_operation(operation.0).await.expect("retry");
            }
            let stage: (Option<String>,) =
                sqlx::query_as("SELECT closed_by_stage FROM messages WHERE id=?")
                    .bind(message)
                    .fetch_one(&db.pool)
                    .await
                    .expect("read stage");
            assert!(
                stage.0.is_none(),
                "признак стадии снят, письмо снова участвует в обработке"
            );
            db.close().await;
        }
    }

    /// Команды выхода из состояния отказа касаются только увода: операцию
    /// отметки признаков или дозаписи копии они не трогают.
    #[tokio::test]
    async fn failure_commands_touch_only_takeaway_operations() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-foreign@example.test").await;
        let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
        let message = seed_message(
            &db,
            account,
            inbox,
            MessageSeed::new(1, "a@example.test", "Письмо"),
        )
        .await;
        let foreign: (i64,) = sqlx::query_as(
            "INSERT INTO outbox_ops(account_id, message_id, op_kind, payload, status)
             VALUES(?, ?, 'flag', '{}', 'failed') RETURNING id",
        )
        .bind(account)
        .bind(message)
        .fetch_one(&db.write_pool)
        .await
        .expect("insert flag operation");
        db.retry_failed_operation(foreign.0)
            .await
            .expect_err("повтор не трогает операцию отметки");
        db.discard_failed_operation(foreign.0)
            .await
            .expect_err("отказ не трогает операцию отметки");
        let status: (String,) = sqlx::query_as("SELECT status FROM outbox_ops WHERE id=?")
            .bind(foreign.0)
            .fetch_one(&db.pool)
            .await
            .expect("read status");
        assert_eq!(status.0, "failed", "чужая операция осталась как была");
        db.close().await;
    }

    /// S-064, S-065: ручной прогон не берёт служебные папки - иначе он увёл бы
    /// отправленные письма и черновики.
    #[tokio::test]
    async fn manual_run_refuses_service_folders() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-service@example.test").await;
        let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
        let sent = seed_folder(&db, account, "Sent", Some("sent")).await;
        seed_folder(&db, account, "Archive", Some("archive")).await;
        seed_message(
            &db,
            account,
            inbox,
            MessageSeed::new(1, "a@example.test", "Письмо"),
        )
        .await;
        let rule = rule_input(
            "service-rule",
            Some(account),
            vec![group("all", vec![condition("sender", "contains", "a@")])],
            vec![action("archive")],
        );
        db.save_mail_rule(&rule, false, None).await.expect("save");
        let refused = db
            .start_mail_rule_run(Some(account), &[sent], None)
            .await
            .expect_err("служебная папка в ручной прогон не попадает");
        assert!(refused.to_string().contains("отправленным"));
        db.start_mail_rule_run(Some(account), &[inbox], None)
            .await
            .expect("рабочая папка разбирается");
        db.close().await;
    }

    /// S-070, S-072: прерванное задание видно отдельно от отчёта текущей
    /// сессии и продолжается с сохранённого курсора после запуска программы.
    #[tokio::test]
    async fn unfinished_run_is_listed_and_continues_after_restart() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-restart@example.test").await;
        let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
        for uid in 1..=3 {
            seed_message(
                &db,
                account,
                inbox,
                MessageSeed::new(uid, "a@example.test", "Письмо"),
            )
            .await;
        }
        let rule = rule_input(
            "restart-rule",
            Some(account),
            vec![group("all", vec![condition("sender", "contains", "a@")])],
            vec![action("mark_read")],
        );
        db.save_mail_rule(&rule, false, None).await.expect("save");
        let report = db
            .start_mail_rule_run(Some(account), &[inbox], None)
            .await
            .expect("start run");
        // Задание, прерванное закрытием программы: оно осталось в состоянии
        // выполнения и с курсором посередине набора.
        sqlx::query(
            "UPDATE mail_rule_runs SET state='running', cursor_message_id=0, scanned=0
             WHERE id=?",
        )
        .bind(report.run_id)
        .execute(&db.write_pool)
        .await
        .expect("mark interrupted");
        assert_eq!(
            db.restore_mail_rule_runs().await.expect("restore"),
            1,
            "задание возвращается в состояние ожидания"
        );
        let pending = db.pending_mail_rule_runs().await.expect("list pending");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].run_id, report.run_id);
        let continued = db
            .continue_mail_rule_run(report.run_id)
            .await
            .expect("continue run");
        assert_eq!(continued.state, "done");
        assert_eq!(continued.scanned, 3);
        assert!(
            db.pending_mail_rule_runs()
                .await
                .expect("list pending again")
                .is_empty(),
            "завершённое задание из перечня уходит"
        );
        db.close().await;
    }

    /// S-045, S-046: правило для всех ящиков выбирает папку по роли в ящике
    /// письма, а неоднозначная роль переводит правило в состояние внимания.
    #[tokio::test]
    async fn folder_role_target_and_ambiguity() {
        let db = test_db().await;
        let first = seed_account(&db, "rules-role1@example.test").await;
        let second = seed_account(&db, "rules-role2@example.test").await;
        let first_inbox = seed_folder(&db, first, "INBOX", Some("inbox")).await;
        let first_archive = seed_folder(&db, first, "Archive", Some("archive")).await;
        let second_inbox = seed_folder(&db, second, "INBOX", Some("inbox")).await;
        let second_archive = seed_folder(&db, second, "Archive", Some("archive")).await;
        seed_message(
            &db,
            first,
            first_inbox,
            MessageSeed::new(1, "a@example.test", "Письмо"),
        )
        .await;
        seed_message(
            &db,
            second,
            second_inbox,
            MessageSeed::new(2, "a@example.test", "Письмо"),
        )
        .await;
        let rule = rule_input(
            "role-rule",
            None,
            vec![group("all", vec![condition("sender", "contains", "a@")])],
            vec![action("archive")],
        );
        db.save_mail_rule(&rule, true, None).await.expect("save");
        db.process_mail_rules().await.expect("process");
        let targets: Vec<(String,)> = sqlx::query_as("SELECT payload FROM outbox_ops ORDER BY id")
            .fetch_all(&db.pool)
            .await
            .expect("read payloads");
        assert_eq!(targets.len(), 2);
        assert!(
            targets[0]
                .0
                .contains(&format!("\"target_folder_id\":{first_archive}"))
        );
        assert!(
            targets[1]
                .0
                .contains(&format!("\"target_folder_id\":{second_archive}"))
        );
        // Вторая папка той же роли делает выбор неоднозначным (S-046).
        seed_folder(&db, first, "Archive2", Some("archive")).await;
        let scoped = rule_input(
            "role-scoped",
            Some(first),
            vec![group("all", vec![condition("sender", "contains", "a@")])],
            vec![action("archive")],
        );
        db.save_mail_rule(&scoped, true, None)
            .await
            .expect("save scoped");
        let saved = db
            .list_mail_rules()
            .await
            .expect("list")
            .into_iter()
            .find(|rule| rule.id == "role-scoped")
            .expect("rule saved");
        assert_eq!(saved.state, "needs_attention");
        db.close().await;
    }

    /// S-054, S-055: удалённая метка переводит правило в состояние внимания и
    /// не даёт ему разбирать письма, а новая метка возвращает его в работу.
    #[tokio::test]
    async fn missing_label_needs_attention() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-attention@example.test").await;
        let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
        let message = seed_message(
            &db,
            account,
            inbox,
            MessageSeed::new(1, "a@example.test", "Письмо"),
        )
        .await;
        let label: i64 =
            sqlx::query_as::<_, (i64,)>("INSERT INTO labels(name) VALUES('Старая') RETURNING id")
                .fetch_one(&db.write_pool)
                .await
                .expect("insert label")
                .0;
        let rule = rule_input(
            "label-rule",
            Some(account),
            vec![group("all", vec![condition("sender", "contains", "a@")])],
            vec![MailRuleAction {
                kind: "label_add".into(),
                folder_id: None,
                folder_role: None,
                label_id: Some(label),
            }],
        );
        db.save_mail_rule(&rule, true, None).await.expect("save");
        sqlx::query("DELETE FROM labels WHERE id=?")
            .bind(label)
            .execute(&db.write_pool)
            .await
            .expect("delete label");
        let saved = db.list_mail_rules().await.expect("list");
        assert_eq!(saved[0].state, "needs_attention");
        let progress_before = saved[0].progress_message_id;
        db.process_mail_rules().await.expect("process");
        let after = db.list_mail_rules().await.expect("list again");
        assert_eq!(
            after[0].progress_message_id, progress_before,
            "правило в состоянии внимания прогресс не двигает"
        );
        let fresh: i64 =
            sqlx::query_as::<_, (i64,)>("INSERT INTO labels(name) VALUES('Новая') RETURNING id")
                .fetch_one(&db.write_pool)
                .await
                .expect("insert label")
                .0;
        let mut fixed = rule.clone();
        fixed.actions[0].label_id = Some(fresh);
        db.save_mail_rule(&fixed, false, None).await.expect("fix");
        let repaired = db.list_mail_rules().await.expect("list repaired");
        assert_eq!(repaired[0].state, "ok");
        db.process_mail_rules().await.expect("process repaired");
        let labels: (i64,) =
            sqlx::query_as("SELECT count(*) FROM message_labels WHERE message_id=?")
                .bind(message)
                .fetch_one(&db.pool)
                .await
                .expect("count labels");
        assert_eq!(labels.0, 1);
        db.close().await;
    }

    /// S-047, S-048, S-049: удаление навсегда ставится без переноса в корзину
    /// и требует подтверждения, связанного с составом правила.
    #[tokio::test]
    async fn delete_forever_requires_confirmation() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-delete@example.test").await;
        let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
        seed_message(
            &db,
            account,
            inbox,
            MessageSeed::new(1, "spam@example.test", "Письмо"),
        )
        .await;
        let mut rule = rule_input(
            "delete-rule",
            Some(account),
            vec![group("all", vec![condition("sender", "contains", "spam@")])],
            vec![action("delete")],
        );
        assert!(
            db.save_mail_rule(&rule, true, None).await.is_err(),
            "без подтверждения правило не сохраняется"
        );
        let key = db
            .issue_delete_confirmation(&rule)
            .await
            .expect("issue key");
        // S-049: изменение правила делает выданный ключ недействительным.
        let mut changed = rule.clone();
        changed.groups[0].conditions[0].value = "другое".into();
        changed.confirm_key = Some(key.clone());
        assert!(db.save_mail_rule(&changed, true, None).await.is_err());
        rule.confirm_key = Some(key);
        db.save_mail_rule(&rule, true, None)
            .await
            .expect("save with confirmation");
        db.process_mail_rules().await.expect("process");
        let operations = pending_operations(&db).await;
        assert_eq!(operations.len(), 1);
        assert_eq!(operations[0].2, "delete");
        db.close().await;
    }

    /// S-059, S-060: порядок правил задаётся полным перечнем, а переключатель
    /// меняет только признак включения.
    #[tokio::test]
    async fn reorder_and_enable_are_independent() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-order2@example.test").await;
        seed_folder(&db, account, "INBOX", Some("inbox")).await;
        for id in ["r1", "r2", "r3"] {
            let rule = rule_input(
                id,
                Some(account),
                vec![group("all", vec![condition("subject", "contains", "счет")])],
                vec![action("mark_read")],
            );
            db.save_mail_rule(&rule, false, None).await.expect("save");
        }
        db.reorder_mail_rules(&["r3".into(), "r1".into(), "r2".into()])
            .await
            .expect("reorder");
        let rules = db.list_mail_rules().await.expect("list");
        let order: Vec<(String, i64)> = rules
            .iter()
            .map(|rule| (rule.id.clone(), rule.sort_order))
            .collect();
        assert_eq!(
            order,
            vec![
                ("r3".to_owned(), 0),
                ("r1".to_owned(), 1),
                ("r2".to_owned(), 2)
            ]
        );
        assert!(
            db.reorder_mail_rules(&["r3".into(), "r1".into()])
                .await
                .is_err(),
            "неполный перечень отклоняется"
        );
        db.set_mail_rule_enabled("r1", false)
            .await
            .expect("disable");
        let after = db.list_mail_rules().await.expect("list again");
        let disabled = after.iter().find(|rule| rule.id == "r1").expect("rule");
        assert!(!disabled.enabled);
        assert_eq!(disabled.sort_order, 1, "порядок не изменился");
        db.close().await;
    }

    /// S-061 - S-064: автоматическая пачка ограничена 500 письмами, письма
    /// догрузки и служебные папки в неё не попадают.
    #[tokio::test]
    async fn automatic_batch_limits_and_folders() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-batch@example.test").await;
        let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
        let spam = seed_folder(&db, account, "Spam", Some("spam")).await;
        for uid in 1..=600 {
            seed_message(
                &db,
                account,
                inbox,
                MessageSeed::new(uid, "a@example.test", "Письмо"),
            )
            .await;
        }
        seed_message(
            &db,
            account,
            inbox,
            MessageSeed {
                uid: 900,
                from_addr: "a@example.test",
                subject: "Догруженное",
                backfilled: true,
            },
        )
        .await;
        seed_message(
            &db,
            account,
            spam,
            MessageSeed::new(901, "a@example.test", "Спам"),
        )
        .await;
        let rule = rule_input(
            "batch-rule",
            Some(account),
            vec![group("all", vec![condition("sender", "contains", "a@")])],
            vec![action("mark_read")],
        );
        db.save_mail_rule(&rule, true, None).await.expect("save");
        let first = db.process_mail_rules().await.expect("first batch");
        assert_eq!(first, 500);
        let second = db.process_mail_rules().await.expect("second batch");
        assert_eq!(second, 100, "остаток письма из служебной папки не включает");
        let untouched: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM messages WHERE seen=0 AND (backfilled=1 OR folder_id=?)",
        )
        .bind(spam)
        .fetch_one(&db.pool)
        .await
        .expect("count untouched");
        assert_eq!(untouched.0, 2);
        db.close().await;
    }

    /// S-065 - S-070, S-073: ручной прогон идёт пачками по выбранным папкам,
    /// останавливается на пределе и продолжается с курсора задания.
    #[tokio::test]
    async fn manual_run_limit_and_continuation() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-manual@example.test").await;
        let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
        let other = seed_account(&db, "rules-manual2@example.test").await;
        let other_inbox = seed_folder(&db, other, "INBOX", Some("inbox")).await;
        sqlx::query(
            "INSERT INTO messages(account_id, folder_id, uid, from_addr, subject, preview,
                                  backfilled)
             SELECT ?, ?, value, 'a@example.test', 'Письмо', '', 1
               FROM (WITH RECURSIVE numbers(value) AS (
                        SELECT 1 UNION ALL SELECT value+1 FROM numbers WHERE value < 5200
                     ) SELECT value FROM numbers)",
        )
        .bind(account)
        .bind(inbox)
        .execute(&db.write_pool)
        .await
        .expect("seed many messages");
        let rule = rule_input(
            "manual-rule",
            Some(account),
            vec![group("all", vec![condition("sender", "contains", "a@")])],
            vec![action("mark_read")],
        );
        db.save_mail_rule(&rule, false, None).await.expect("save");
        let progress_before = db.list_mail_rules().await.expect("list")[0].progress_message_id;
        assert!(
            db.start_mail_rule_run(Some(account), &[other_inbox], None)
                .await
                .is_err(),
            "папка другого ящика отклоняется"
        );
        let report = db
            .start_mail_rule_run(Some(account), &[inbox], None)
            .await
            .expect("start run");
        assert_eq!(report.scanned, 5000, "предел одного запуска");
        assert_eq!(report.remaining, 200);
        assert_eq!(report.state, "pending");
        let progress = db.list_mail_rules().await.expect("list")[0].progress_message_id;
        assert_eq!(
            progress, progress_before,
            "ручной прогон прогресс правил не двигает"
        );
        let finished = db
            .continue_mail_rule_run(report.run_id)
            .await
            .expect("continue run");
        assert_eq!(finished.scanned, 5200);
        assert_eq!(finished.remaining, 0);
        assert_eq!(finished.state, "done");
        let last = db
            .last_mail_rule_run()
            .await
            .expect("last run")
            .expect("report exists");
        assert_eq!(last.run_id, report.run_id);
        db.close().await;
    }

    /// S-058, S-071, S-072: повторный ручной прогон снова выполняет местные
    /// действия, изменённое правило из начатого задания выбывает, а задание в
    /// состоянии выполнения возвращается в ожидание.
    #[tokio::test]
    async fn manual_run_repeats_local_actions_and_restores_jobs() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-repeat@example.test").await;
        let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
        let message = seed_message(
            &db,
            account,
            inbox,
            MessageSeed {
                uid: 1,
                from_addr: "a@example.test",
                subject: "Письмо",
                backfilled: true,
            },
        )
        .await;
        let rule = rule_input(
            "repeat-rule",
            Some(account),
            vec![group("all", vec![condition("sender", "contains", "a@")])],
            vec![action("mark_read")],
        );
        db.save_mail_rule(&rule, false, None).await.expect("save");
        db.start_mail_rule_run(Some(account), &[inbox], None)
            .await
            .expect("first run");
        sqlx::query("UPDATE messages SET seen=0 WHERE id=?")
            .bind(message)
            .execute(&db.write_pool)
            .await
            .expect("reset flag");
        let second = db
            .start_mail_rule_run(Some(account), &[inbox], None)
            .await
            .expect("second run");
        assert_eq!(
            second.applied, 1,
            "повторный прогон снова применяет правило"
        );
        let seen: (i64,) = sqlx::query_as("SELECT seen FROM messages WHERE id=?")
            .bind(message)
            .fetch_one(&db.pool)
            .await
            .expect("read flag");
        assert_eq!(seen.0, 1);
        sqlx::query("UPDATE mail_rule_runs SET state='running'")
            .execute(&db.write_pool)
            .await
            .expect("mark running");
        assert!(db.restore_mail_rule_runs().await.expect("restore") >= 1);
        let states: Vec<(String,)> = sqlx::query_as("SELECT state FROM mail_rule_runs")
            .fetch_all(&db.pool)
            .await
            .expect("read states");
        assert!(states.iter().all(|(state,)| state != "running"));
        db.close().await;
    }

    /// S-052, S-053: операция в состоянии отказа держит письмо на месте, а
    /// новую операцию по нему можно поставить только после отказа от прежней.
    #[tokio::test]
    async fn failed_operation_blocks_until_user_decides() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-failed@example.test").await;
        let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
        seed_folder(&db, account, "Archive", Some("archive")).await;
        seed_folder(&db, account, "Trash", Some("trash")).await;
        let message = seed_message(
            &db,
            account,
            inbox,
            MessageSeed::new(1, "a@example.test", "Письмо"),
        )
        .await;
        let queued = db
            .queue_message_action(&[message], "archive")
            .await
            .expect("queue");
        let operation = queued.operation_ids[0];
        sqlx::query("UPDATE outbox_ops SET status='failed', last_error='отказ' WHERE id=?")
            .bind(operation)
            .execute(&db.write_pool)
            .await
            .expect("mark failed");
        let blocked = db
            .queue_message_action(&[message], "trash")
            .await
            .expect("письмо с отказавшей операцией пропускается, а не роняет действие");
        assert_eq!(blocked.skipped_failed, 1);
        assert!(blocked.operation_ids.is_empty());
        let failed = db
            .failed_takeaway_operations()
            .await
            .expect("list failed operations");
        assert_eq!(failed.len(), 1);
        db.retry_failed_operation(operation)
            .await
            .expect("retry operation");
        db.discard_failed_operation(operation)
            .await
            .expect_err("повторённая операция уже не в отказе");
        sqlx::query("UPDATE outbox_ops SET status='failed' WHERE id=?")
            .bind(operation)
            .execute(&db.write_pool)
            .await
            .expect("mark failed again");
        db.discard_failed_operation(operation)
            .await
            .expect("discard");
        db.queue_message_action(&[message], "trash")
            .await
            .expect("queue after discard");
        db.close().await;
    }

    /// S-056: удаление ящика уносит его правила, а правила для всех ящиков
    /// остаются.
    #[tokio::test]
    async fn deleting_account_removes_only_its_rules() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-account@example.test").await;
        seed_folder(&db, account, "INBOX", Some("inbox")).await;
        let scoped = rule_input(
            "scoped",
            Some(account),
            vec![group("all", vec![condition("subject", "contains", "счет")])],
            vec![action("mark_read")],
        );
        db.save_mail_rule(&scoped, false, None)
            .await
            .expect("save scoped");
        let shared = rule_input(
            "shared",
            None,
            vec![group("all", vec![condition("subject", "contains", "счет")])],
            vec![action("mark_read")],
        );
        db.save_mail_rule(&shared, false, None)
            .await
            .expect("save shared");
        sqlx::query("DELETE FROM accounts WHERE id=?")
            .bind(account)
            .execute(&db.write_pool)
            .await
            .expect("delete account");
        let rules = db.list_mail_rules().await.expect("list");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, "shared");
        let orphans: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM mail_rule_conditions WHERE rule_id NOT IN (SELECT id FROM mail_rules)",
        )
        .fetch_one(&db.pool)
        .await
        .expect("count orphans");
        assert_eq!(orphans.0, 0);
        db.close().await;
    }

    /// S-074 - S-078: правило прежней схемы превращается в одну группу с одним
    /// условием, первое действие сохраняется, а за ним дописывается остановка.
    #[tokio::test]
    async fn legacy_rule_is_migrated_with_stop() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-legacy@example.test").await;
        seed_folder(&db, account, "INBOX", Some("inbox")).await;
        sqlx::query(
            "INSERT INTO mail_rules(id, name, field, operator, value, account_id, action,
                                    enabled, progress_message_id, sort_order)
             VALUES('legacy', 'Старое правило', 'sender', 'contains', 'boss@', ?, 'archive', 1, 7, 3)",
        )
        .bind(account)
        .execute(&db.write_pool)
        .await
        .expect("insert legacy rule");
        let moved = db
            .migrate_mail_rules_to_groups()
            .await
            .expect("migrate rules");
        assert_eq!(moved, 1);
        assert_eq!(
            db.migrate_mail_rules_to_groups()
                .await
                .expect("migrate again"),
            0,
            "повторный перенос ничего не делает"
        );
        let rules = db.list_mail_rules().await.expect("list");
        let rule = rules.iter().find(|rule| rule.id == "legacy").expect("rule");
        assert_eq!(rule.groups.len(), 1);
        assert_eq!(rule.groups[0].logic, "all");
        assert_eq!(rule.groups[0].conditions.len(), 1);
        assert_eq!(rule.groups[0].conditions[0].field, "sender");
        assert_eq!(rule.actions.len(), 2);
        assert_eq!(rule.actions[0].kind, "archive");
        assert_eq!(rule.actions[1].kind, "stop");
        assert_eq!(rule.progress_message_id, 7);
        assert_eq!(rule.sort_order, 3);
        assert!(rule.enabled);
        db.close().await;
    }

    /// S-078: прежние столбцы получают только значения, допустимые их
    /// ограничениями check.
    #[tokio::test]
    async fn legacy_columns_keep_allowed_values() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-columns@example.test").await;
        seed_folder(&db, account, "INBOX", Some("inbox")).await;
        let rule = rule_input(
            "modern",
            Some(account),
            vec![group(
                "all",
                vec![condition("sender_address", "ends_with", "@example.test")],
            )],
            vec![action("mark_flagged")],
        );
        db.save_mail_rule(&rule, false, None).await.expect("save");
        let row: (String, String, String) =
            sqlx::query_as("SELECT field, operator, action FROM mail_rules WHERE id='modern'")
                .fetch_one(&db.pool)
                .await
                .expect("read legacy columns");
        assert!(matches!(row.0.as_str(), "sender" | "subject"));
        assert!(matches!(row.1.as_str(), "contains" | "equals"));
        assert!(matches!(
            row.2.as_str(),
            "move" | "archive" | "spam" | "trash" | "label"
        ));
        db.close().await;
    }

    /// S-031: все правила сверяются с одним снимком письма, поэтому метка,
    /// поставленная первым правилом, не включает условие второго в том же
    /// прогоне.
    #[tokio::test]
    async fn rules_share_one_message_snapshot() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-snapshot@example.test").await;
        let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
        let message = seed_message(
            &db,
            account,
            inbox,
            MessageSeed::new(1, "a@example.test", "Письмо"),
        )
        .await;
        let label: i64 =
            sqlx::query_as::<_, (i64,)>("INSERT INTO labels(name) VALUES('Важное') RETURNING id")
                .fetch_one(&db.write_pool)
                .await
                .expect("insert label")
                .0;
        let first = rule_input(
            "snapshot-a",
            Some(account),
            vec![group("all", vec![condition("sender", "contains", "a@")])],
            vec![MailRuleAction {
                kind: "label_add".into(),
                folder_id: None,
                folder_role: None,
                label_id: Some(label),
            }],
        );
        db.save_mail_rule(&first, true, None)
            .await
            .expect("save first");
        let second = rule_input(
            "snapshot-b",
            Some(account),
            vec![group("all", vec![condition("label", "contains", "Важное")])],
            vec![action("mark_read")],
        );
        db.save_mail_rule(&second, true, None)
            .await
            .expect("save second");
        db.process_mail_rules().await.expect("process");
        let seen: (i64,) = sqlx::query_as("SELECT seen FROM messages WHERE id=?")
            .bind(message)
            .fetch_one(&db.pool)
            .await
            .expect("read flag");
        assert_eq!(seen.0, 0, "второе правило видит письмо без новой метки");
        db.close().await;
    }

    /// S-002, S-050: у письма с уже поставленной операцией увода выполняются
    /// только местные действия, а их результат сохраняется.
    #[tokio::test]
    async fn local_actions_survive_when_takeaway_is_impossible() {
        let db = test_db().await;
        let account = seed_account(&db, "rules-partial@example.test").await;
        let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
        seed_folder(&db, account, "Archive", Some("archive")).await;
        seed_folder(&db, account, "Spam", Some("spam")).await;
        let message = seed_message(
            &db,
            account,
            inbox,
            MessageSeed::new(1, "a@example.test", "Письмо"),
        )
        .await;
        db.queue_message_action(&[message], "archive")
            .await
            .expect("user queues move");
        let rule = rule_input(
            "partial-rule",
            Some(account),
            vec![group("all", vec![condition("sender", "contains", "a@")])],
            vec![action("mark_read"), action("spam")],
        );
        db.save_mail_rule(&rule, true, None).await.expect("save");
        db.process_mail_rules().await.expect("process");
        let seen: (i64,) = sqlx::query_as("SELECT seen FROM messages WHERE id=?")
            .bind(message)
            .fetch_one(&db.pool)
            .await
            .expect("read flag");
        assert_eq!(seen.0, 1, "местное действие выполнено");
        let operations = pending_operations(&db).await;
        let takeaways = operations
            .iter()
            .filter(|row| matches!(row.2.as_str(), "move" | "delete"))
            .count();
        assert_eq!(takeaways, 1, "второй увод письма не ставится");
        db.close().await;
    }

    /// S-080: база с отметкой более новой версии программы не открывается, и
    /// пользователь получает объяснение вместо технической ошибки.
    #[tokio::test]
    async fn database_from_newer_version_is_refused() {
        let db = test_db().await;
        sqlx::query("UPDATE storage_meta SET value='99.0.0' WHERE key='min_app_version'")
            .execute(&db.write_pool)
            .await
            .expect("mark newer version");
        let error = db.migrate().await.expect_err("база не открывается");
        assert!(
            error.to_string().contains("более новой версией программы"),
            "{error}"
        );
        db.close().await;
    }

    /// S-079: миграция оставляет в storage_meta отметку минимальной
    /// совместимой версии программы.
    #[tokio::test]
    async fn migration_marks_minimal_app_version() {
        let db = test_db().await;
        let marked: Option<(String,)> =
            sqlx::query_as("SELECT value FROM storage_meta WHERE key='min_app_version'")
                .fetch_optional(&db.pool)
                .await
                .expect("read marker");
        assert!(marked.is_some(), "отметка версии записана");
        db.close().await;
    }
}
