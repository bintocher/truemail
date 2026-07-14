-- Целостность синхронизации, производительность главных запросов и безопасное
-- удаление FTS-данных. Миграция сначала убирает возможные дубли старых версий.

ALTER TABLE events ADD COLUMN recurrence_id TEXT;
ALTER TABLE events ADD COLUMN exdates TEXT;
ALTER TABLE events ADD COLUMN rdates TEXT;
ALTER TABLE event_alarms ADD COLUMN trigger_at TEXT;
ALTER TABLE messages ADD COLUMN snoozed_until TEXT;
ALTER TABLE messages ADD COLUMN scheduled_send_at TEXT;
ALTER TABLE outbox_ops ADD COLUMN status TEXT NOT NULL DEFAULT 'pending';
ALTER TABLE outbox_ops ADD COLUMN next_attempt_at TEXT;
UPDATE outbox_ops SET next_attempt_at = datetime('now') WHERE next_attempt_at IS NULL;

DELETE FROM contact_emails
 WHERE id NOT IN (SELECT min(id) FROM contact_emails GROUP BY contact_id, lower(email));
DELETE FROM event_attendees
 WHERE id NOT IN (SELECT min(id) FROM event_attendees GROUP BY event_id, lower(email));
DELETE FROM event_alarms
 WHERE id NOT IN (SELECT min(id) FROM event_alarms GROUP BY event_id, trigger_minutes, action);
DELETE FROM contacts
 WHERE uid IS NOT NULL
   AND id NOT IN (SELECT min(id) FROM contacts WHERE uid IS NOT NULL GROUP BY account_id, uid);
DELETE FROM events
 WHERE uid IS NOT NULL
   AND id NOT IN (SELECT min(id) FROM events WHERE uid IS NOT NULL GROUP BY calendar_id, uid, coalesce(recurrence_id, ''));
DELETE FROM calendars
 WHERE id NOT IN (
   SELECT min(id) FROM calendars
   GROUP BY account_id, coalesce(uid, url, name)
 );
DELETE FROM threads
 WHERE root_message_id IS NOT NULL
   AND id NOT IN (SELECT min(id) FROM threads WHERE root_message_id IS NOT NULL GROUP BY account_id, root_message_id);

CREATE UNIQUE INDEX IF NOT EXISTS uq_contacts_account_uid
    ON contacts(account_id, uid) WHERE uid IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS uq_contact_emails_contact_email
    ON contact_emails(contact_id, lower(email));
CREATE UNIQUE INDEX IF NOT EXISTS uq_calendars_account_source
    ON calendars(account_id, coalesce(uid, url, name));
CREATE UNIQUE INDEX IF NOT EXISTS uq_events_calendar_uid
    ON events(calendar_id, uid, coalesce(recurrence_id, '')) WHERE uid IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS uq_event_attendees_event_email
    ON event_attendees(event_id, lower(email));
CREATE UNIQUE INDEX IF NOT EXISTS uq_event_alarms_event_trigger
    ON event_alarms(event_id, trigger_minutes, action);
CREATE UNIQUE INDEX IF NOT EXISTS uq_threads_account_root
    ON threads(account_id, root_message_id) WHERE root_message_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_messages_folder_date
    ON messages(folder_id, date DESC);
CREATE INDEX IF NOT EXISTS idx_messages_rfc822_message_id
    ON messages(rfc822_message_id);
CREATE INDEX IF NOT EXISTS idx_messages_in_reply_to
    ON messages(in_reply_to);
CREATE INDEX IF NOT EXISTS idx_attachments_message
    ON attachments(message_id);

CREATE INDEX IF NOT EXISTS idx_outbox_ready
    ON outbox_ops(status, next_attempt_at, id);

-- При удалении родителя каскад SQLite может не вызвать DELETE-trigger FTS в
-- зависимости от recursive_triggers. BEFORE-trigger удаляет индекс заранее.
CREATE TRIGGER IF NOT EXISTS messages_fts_folder_bd BEFORE DELETE ON folders BEGIN
    DELETE FROM messages_fts
     WHERE rowid IN (SELECT id FROM messages WHERE folder_id = old.id);
END;
CREATE TRIGGER IF NOT EXISTS messages_fts_account_bd BEFORE DELETE ON accounts BEGIN
    DELETE FROM messages_fts
     WHERE rowid IN (SELECT id FROM messages WHERE account_id = old.id);
END;
