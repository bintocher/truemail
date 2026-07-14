//! Репозитории: типобезопасные запросы к таблицам.

use super::Db;
use crate::Result;
use crate::model::*;

#[derive(Debug, Clone, serde::Serialize)]
pub struct QueuedAction {
    pub operation_ids: Vec<i64>,
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

impl Db {
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
                username, secret_ref, color
             ) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
        .bind(input.username.as_deref())
        .bind(&input.secret_ref)
        .bind(input.color.as_deref())
        .execute(&self.pool)
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
                ews_url, username, secret_ref, include_in_unified, color, enabled
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
        .execute(&self.pool)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(crate::Error::Other("аккаунт не найден".into()));
        }
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
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_folder_local(&self, folder_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM folders WHERE id=?")
            .bind(folder_id)
            .execute(&self.pool)
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
        let mut tx = self.pool.begin().await?;
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
        let mut tx = self.pool.begin().await?;
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
                    highestmodseq = coalesce(excluded.highestmodseq, folders.highestmodseq),
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

    pub async fn folder_sync_cursors(
        &self,
        account_id: i64,
    ) -> Result<std::collections::HashMap<String, crate::backend::FolderSyncCursor>> {
        let rows: Vec<(String, Option<i64>, Option<i64>)> = sqlx::query_as(
            "SELECT f.remote_path, f.uidvalidity, max(m.uid)
             FROM folders f LEFT JOIN messages m ON m.folder_id=f.id
             WHERE f.account_id=? GROUP BY f.id, f.remote_path, f.uidvalidity",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(path, uidvalidity, last_uid)| {
                (
                    path,
                    crate::backend::FolderSyncCursor {
                        uidvalidity: uidvalidity.and_then(|value| u32::try_from(value).ok()),
                        last_uid: last_uid.and_then(|value| u32::try_from(value).ok()),
                    },
                )
            })
            .collect())
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
        let mut tx = self.pool.begin().await?;
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

    pub async fn save_yandex_dav(
        &self,
        account_id: i64,
        data: &crate::account::DavSyncResult,
    ) -> Result<(usize, usize, usize)> {
        self.save_auxiliary_data(account_id, "caldav", data).await
    }

    pub async fn save_google_services(
        &self,
        account_id: i64,
        data: &crate::account::DavSyncResult,
    ) -> Result<(usize, usize, usize)> {
        self.save_auxiliary_data(account_id, "google", data).await
    }

    /// Сохранить календарные источники и контакты конкретного провайдера.
    pub async fn save_auxiliary_data(
        &self,
        account_id: i64,
        source_kind: &str,
        data: &crate::account::DavSyncResult,
    ) -> Result<(usize, usize, usize)> {
        use std::collections::HashSet;

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
        for calendar in &data.calendars {
            let mut events = Vec::new();
            for event in &calendar.events {
                let reference = self.blobs.put(event.raw.as_bytes())?;
                created_refs.push(reference.clone());
                events.push((event, reference));
            }
            calendar_rows.push((calendar, events));
        }
        let mut contact_rows = Vec::new();
        if data.contacts_available {
            for contact in &data.contacts {
                let reference = self.blobs.put(contact.raw.as_bytes())?;
                created_refs.push(reference.clone());
                contact_rows.push((contact, reference));
            }
        }

        let save_result: Result<(usize, usize, usize)> = async {
            let mut tx = self.pool.begin().await?;
            let existing_calendars: Vec<(i64,)> =
                sqlx::query_as("SELECT id FROM calendars WHERE account_id=? AND kind=?")
                    .bind(account_id)
                    .bind(source_kind)
                    .fetch_all(&mut *tx)
                    .await?;
            let mut active_calendars = HashSet::new();
            let mut event_count = 0;
            for (calendar, events) in calendar_rows {
                let (calendar_id,): (i64,) = sqlx::query_as(
                    "INSERT INTO calendars(account_id, uid, name, kind, url, ctag)
                     VALUES(?, ?, ?, ?, ?, ?)
                     ON CONFLICT DO UPDATE SET name=excluded.name, url=excluded.url,
                         kind=excluded.kind, ctag=excluded.ctag
                     RETURNING id",
                )
                .bind(account_id)
                .bind(&calendar.url)
                .bind(&calendar.name)
                .bind(source_kind)
                .bind(&calendar.url)
                .bind(&calendar.ctag)
                .fetch_one(&mut *tx)
                .await?;
                active_calendars.insert(calendar_id);

                let existing_events: Vec<(i64,)> =
                    sqlx::query_as("SELECT id FROM events WHERE calendar_id=?")
                        .bind(calendar_id)
                        .fetch_all(&mut *tx)
                        .await?;
                let mut active_events = HashSet::new();
                for (event, blob_ref) in events {
                    let (event_id,): (i64,) = sqlx::query_as(
                        "INSERT INTO events(calendar_id, uid, summary, description, location,
                                            dtstart, dtend, all_day, rrule, recurrence_id, exdates, rdates,
                                            status, ical_ref, etag, remote_url)
                         VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                         ON CONFLICT DO UPDATE SET summary=excluded.summary,
                            description=excluded.description, location=excluded.location,
                            dtstart=excluded.dtstart, dtend=excluded.dtend,
                            all_day=excluded.all_day,
                            rrule=excluded.rrule, recurrence_id=excluded.recurrence_id,
                            exdates=excluded.exdates, rdates=excluded.rdates,
                            status=excluded.status,
                            ical_ref=excluded.ical_ref, etag=excluded.etag,
                            remote_url=excluded.remote_url
                         RETURNING id",
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
                    .fetch_one(&mut *tx)
                    .await?;
                    active_events.insert(event_id);
                    event_count += 1;
                }
                for (event_id,) in existing_events {
                    if !active_events.contains(&event_id) {
                        sqlx::query("DELETE FROM events WHERE id=?")
                            .bind(event_id)
                            .execute(&mut *tx)
                            .await?;
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
                sqlx::query("DELETE FROM auxiliary_collections WHERE account_id=? AND kind='carddav'")
                    .bind(account_id)
                    .execute(&mut *tx)
                    .await?;
                for collection in &data.contact_collections {
                    sqlx::query(
                        "INSERT OR IGNORE INTO auxiliary_collections(account_id, kind, url) VALUES(?, 'carddav', ?)",
                    )
                    .bind(account_id)
                    .bind(collection)
                    .execute(&mut *tx)
                    .await?;
                }
                let existing_contacts: Vec<(i64,)> = sqlx::query_as(
                    "SELECT id FROM contacts WHERE account_id=? AND uid NOT LIKE 'mail:%'",
                )
                .bind(account_id)
                .fetch_all(&mut *tx)
                .await?;
                let mut active_contacts = HashSet::new();
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
                    .bind(&contact.display_name)
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
                }
                for (contact_id,) in existing_contacts {
                    if !active_contacts.contains(&contact_id) {
                        sqlx::query("DELETE FROM contacts WHERE id=?")
                            .bind(contact_id)
                            .execute(&mut *tx)
                            .await?;
                    }
                }
            }

            tx.commit().await?;
            Ok((data.calendars.len(), event_count, data.contacts.len()))
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

        let current_refs: HashSet<&str> = created_refs.iter().map(String::as_str).collect();
        for (reference,) in old_event_refs.into_iter().chain(old_contact_refs) {
            if let Some(reference) = reference
                && !current_refs.contains(reference.as_str())
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
        let save_result: Result<()> = async {
            let mut tx = self.pool.begin().await?;
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
            ) in rows
            {
                let folder: Option<(i64,)> = sqlx::query_as(
                    "SELECT id FROM folders WHERE account_id = ? AND remote_path = ? LIMIT 1",
                )
                .bind(account_id)
                .bind(&source.folder_path)
                .fetch_optional(&mut *tx)
                .await?;
                let Some((folder_id,)) = folder else { continue };
                let old_ref: Option<(Option<String>,)> = sqlx::query_as(
                    "SELECT raw_blob_ref FROM messages WHERE folder_id=? AND uid=?",
                )
                .bind(folder_id)
                .bind(source.uid as i64)
                .fetch_optional(&mut *tx)
                .await?;
                let is_new_message = old_ref.is_none();
                sqlx::query(
                    "INSERT INTO messages(account_id, folder_id, uid, remote_id, rfc822_message_id, in_reply_to, references_ids, from_name, from_addr, to_addrs, cc_addrs, subject, preview, date, size, seen, flagged, answered, draft, has_attachments, dkim_pass, spf_pass, dmarc_pass, raw_blob_ref, body_fetched)
                     VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)
                     ON CONFLICT(folder_id, uid) DO UPDATE SET
                        remote_id=coalesce(excluded.remote_id,messages.remote_id), rfc822_message_id=excluded.rfc822_message_id, in_reply_to=excluded.in_reply_to,
                        references_ids=excluded.references_ids, from_name=excluded.from_name,
                        from_addr=excluded.from_addr, to_addrs=excluded.to_addrs, cc_addrs=excluded.cc_addrs,
                        subject=excluded.subject, preview=excluded.preview, date=excluded.date, size=excluded.size,
                        seen=excluded.seen, flagged=excluded.flagged, answered=excluded.answered,
                        draft=excluded.draft, has_attachments=excluded.has_attachments,
                        dkim_pass=excluded.dkim_pass, spf_pass=excluded.spf_pass,
                        dmarc_pass=excluded.dmarc_pass, raw_blob_ref=excluded.raw_blob_ref, body_fetched=1",
                )
                .bind(account_id).bind(folder_id).bind(source.uid as i64).bind(&source.remote_id).bind(&message_id)
                .bind(&in_reply_to).bind(&references).bind(from_name).bind(from_addr).bind(to).bind(cc)
                .bind(&subject).bind(&preview).bind(&date).bind(source.size.map(i64::from))
                .bind(source.seen as i64).bind(source.flagged as i64).bind(source.answered as i64)
                .bind(source.draft as i64).bind(!attachments.is_empty() as i64).bind(dkim).bind(spf)
                .bind(dmarc).bind(&raw_ref).execute(&mut *tx).await?;
                active_refs.insert(raw_ref.clone());
                if let Some((Some(reference),)) = old_ref
                    && reference != raw_ref
                {
                    stale_refs.push(reference);
                }
                let (message_row_id,): (i64,) =
                    sqlx::query_as("SELECT id FROM messages WHERE folder_id = ? AND uid = ?")
                        .bind(folder_id)
                        .bind(source.uid as i64)
                        .fetch_one(&mut *tx)
                        .await?;
                sqlx::query("DELETE FROM attachments WHERE message_id=?")
                    .bind(message_row_id)
                    .execute(&mut *tx)
                    .await?;
                for (filename, mime_type, size, content_id) in attachments {
                    sqlx::query("INSERT INTO attachments(message_id, filename, mime_type, size, content_id, is_inline, fetched) VALUES(?, ?, ?, ?, ?, ?, 0)")
                        .bind(message_row_id).bind(filename).bind(mime_type).bind(size)
                        .bind(&content_id).bind(content_id.is_some() as i64).execute(&mut *tx).await?;
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
                sqlx::query("UPDATE messages_fts SET body = ? WHERE rowid = ?")
                    .bind(body_text)
                    .bind(message_row_id)
                    .execute(&mut *tx)
                    .await?;
            }
            tx.commit().await?;
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
            let display = name
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(&normalized)
                .to_owned();
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
        let mut tx = self.pool.begin().await?;
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
        let limit = limit.clamp(1, 200);
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
             WHERE NOT EXISTS (
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
            let value = message.header("List-Unsubscribe")?.as_text()?;
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
            let one_click = message
                .header("List-Unsubscribe-Post")
                .and_then(|header| header.as_text())
                .is_some_and(|value| value.to_ascii_lowercase().contains("one-click"));
            Some(Unsubscribe {
                one_click_url: one_click.then(|| http.clone()).flatten(),
                mailto,
                http,
            })
        });
        Ok(MessageFull {
            meta,
            has_remote_content: body_html
                .as_deref()
                .is_some_and(|html| html.contains("http://") || html.contains("https://")),
            body_html,
            body_text,
            attachments,
            is_newsletter,
            unsubscribe,
        })
    }

    pub async fn list_calendars_and_events(&self) -> Result<(Vec<CalendarSummary>, Vec<Event>)> {
        let calendar_rows: Vec<(i64, i64, String, Option<String>, i64, i64)> = sqlx::query_as(
            "SELECT id, account_id, name, color, visible, read_only FROM calendars ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?;
        let calendars = calendar_rows
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
                    all_day, rrule, recurrence_id, exdates, rdates, timezone, transp, class,
                    categories, url, organizer, sequence
             FROM events ORDER BY dtstart",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok((calendars, rows.into_iter().map(Into::into).collect()))
    }

    /// Отметить письмо прочитанным (локально; в outbox уйдёт синхронизация флага).
    pub async fn mark_seen(&self, message_id: i64, seen: bool) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let locator: (i64, i64, String, Option<String>) = sqlx::query_as(
            "SELECT m.account_id, m.uid, f.remote_path, m.remote_id FROM messages m
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
        let payload = serde_json::json!({
            "message_id": message_id,
            "folder_path": locator.2,
            "uid": locator.1,
            "remote_id": locator.3,
            "seen": seen,
        });
        sqlx::query("DELETE FROM outbox_ops WHERE message_id=? AND op_kind='flag' AND status IN ('pending','retry')")
            .bind(message_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO outbox_ops(account_id, message_id, op_kind, payload, status, next_attempt_at)
             VALUES(?, ?, 'flag', ?, 'pending', datetime('now'))",
        )
        .bind(locator.0)
        .bind(message_id)
        .bind(payload.to_string())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Поставить реальное перемещение/удаление на сервере в очередь. Локальный
    /// список скрывает такие письма сразу, но undo может отменить pending op.
    pub async fn queue_message_action(
        &self,
        message_ids: &[i64],
        target_role: &str,
    ) -> Result<QueuedAction> {
        let mut tx = self.pool.begin().await?;
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
        let mut tx = self.pool.begin().await?;
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
                    .execute(&self.pool)
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
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
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
        .execute(&self.pool)
        .await?;
        Ok(result.last_insert_rowid())
    }

    pub async fn complete_outbox_operation(&self, operation: &OutboxOperation) -> Result<()> {
        let mut tx = self.pool.begin().await?;
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
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ---------- Умные папки ----------

    pub async fn list_smart_folders(&self) -> Result<Vec<SmartFolder>> {
        let rows = sqlx::query_as::<_, SmartRow>(
            "SELECT id, name, icon, match_logic, is_builtin, enabled
             FROM smart_folders WHERE enabled = 1 ORDER BY sort_order, id",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::new();
        for r in rows {
            let conditions = sqlx::query_as::<_, CondRow>(
                "SELECT field, op, value FROM smart_conditions WHERE smart_folder_id = ?",
            )
            .bind(r.id)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|c| SmartCondition {
                field: c.field,
                op: c.op,
                value: c.value,
            })
            .collect();
            out.push(SmartFolder {
                id: r.id,
                name: r.name,
                icon: r.icon,
                match_logic: r.match_logic,
                is_builtin: r.is_builtin != 0,
                enabled: r.enabled != 0,
                conditions,
            });
        }
        Ok(out)
    }

    // ---------- Контакты ----------

    pub async fn list_contacts(&self, query: Option<&str>) -> Result<Vec<Contact>> {
        let like = format!("%{}%", query.unwrap_or(""));
        let rows = sqlx::query_as::<_, ContactRow>(
            "SELECT id, account_id, uid, display_name, first_name, last_name, organization, is_favorite
             FROM contacts WHERE display_name LIKE ? ORDER BY display_name LIMIT 500",
        )
        .bind(like)
        .fetch_all(&self.pool)
        .await?;
        let mut contacts: Vec<Contact> = rows.into_iter().map(Into::into).collect();
        for contact in &mut contacts {
            if let Some(id) = contact.id {
                contact.emails = sqlx::query_as::<_, ContactEmailRow>(
                    "SELECT email, kind FROM contact_emails WHERE contact_id = ? ORDER BY id",
                )
                .bind(id)
                .fetch_all(&self.pool)
                .await?
                .into_iter()
                .map(|row| ContactEmail {
                    email: row.email,
                    kind: row.kind,
                })
                .collect();
            }
        }
        Ok(contacts)
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
    username: Option<String>,
    secret_ref: Option<String>,
    include_in_unified: i64,
    color: Option<String>,
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
            username: r.username,
            secret_ref: r.secret_ref,
            include_in_unified: r.include_in_unified != 0,
            color: r.color,
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
struct SmartRow {
    id: i64,
    name: String,
    icon: Option<String>,
    match_logic: String,
    is_builtin: i64,
    enabled: i64,
}
#[derive(sqlx::FromRow)]
struct CondRow {
    field: String,
    op: String,
    value: String,
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
}

#[derive(sqlx::FromRow)]
struct ContactEmailRow {
    email: String,
    kind: Option<String>,
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
    transp: Option<String>,
    class: Option<String>,
    categories: Option<String>,
    url: Option<String>,
    organizer: Option<String>,
    sequence: i64,
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
        }
    }
}
impl From<ContactRow> for Contact {
    fn from(r: ContactRow) -> Self {
        Contact {
            id: Some(r.id),
            account_id: r.account_id,
            uid: r.uid,
            display_name: r.display_name,
            first_name: r.first_name,
            last_name: r.last_name,
            organization: r.organization,
            emails: Vec::new(),
            is_favorite: r.is_favorite != 0,
        }
    }
}
