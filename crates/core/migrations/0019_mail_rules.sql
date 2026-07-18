-- Mail rules are core-owned so they run even when no UI window is open.
CREATE TABLE mail_rules (
    id                  TEXT PRIMARY KEY,
    name                TEXT NOT NULL,
    field               TEXT NOT NULL CHECK(field IN ('sender', 'subject')),
    operator            TEXT NOT NULL CHECK(operator IN ('contains', 'equals')),
    value               TEXT NOT NULL,
    account_id          INTEGER REFERENCES accounts(id) ON DELETE CASCADE,
    action              TEXT NOT NULL CHECK(action IN ('move', 'archive', 'spam', 'trash')),
    folder_id           INTEGER REFERENCES folders(id) ON DELETE CASCADE,
    enabled             INTEGER NOT NULL DEFAULT 1,
    progress_message_id INTEGER NOT NULL DEFAULT 0,
    sort_order          INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_mail_rules_enabled_progress
    ON mail_rules(enabled, progress_message_id, sort_order);
