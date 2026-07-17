-- Parsed MIME cache. The raw blob remains the canonical source; the cache is
-- valid only while raw_blob_ref matches the current message version.
CREATE TABLE message_content_cache (
    message_id          INTEGER PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    raw_blob_ref        TEXT    NOT NULL,
    body_html           TEXT,
    body_text           TEXT,
    attachments_json    TEXT    NOT NULL DEFAULT '[]',
    has_remote_content  INTEGER NOT NULL DEFAULT 0,
    is_newsletter       INTEGER NOT NULL DEFAULT 0,
    unsubscribe_json    TEXT,
    parsed_at           TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE TRIGGER message_content_cache_raw_au
AFTER UPDATE OF raw_blob_ref ON messages
WHEN old.raw_blob_ref IS NOT new.raw_blob_ref
BEGIN
    DELETE FROM message_content_cache WHERE message_id = new.id;
END;
