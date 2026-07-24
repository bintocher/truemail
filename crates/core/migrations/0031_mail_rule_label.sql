-- Правило может назначать тег (метку). SQLite не умеет менять CHECK-ограничение
-- у столбца action, поэтому пересобираем таблицу и заодно добавляем label_id.
ALTER TABLE mail_rules RENAME TO mail_rules_old;

CREATE TABLE mail_rules (
    id                  TEXT PRIMARY KEY,
    name                TEXT NOT NULL,
    field               TEXT NOT NULL CHECK(field IN ('sender', 'subject')),
    operator            TEXT NOT NULL CHECK(operator IN ('contains', 'equals')),
    value               TEXT NOT NULL,
    account_id          INTEGER REFERENCES accounts(id) ON DELETE CASCADE,
    action              TEXT NOT NULL CHECK(action IN ('move', 'archive', 'spam', 'trash', 'label')),
    folder_id           INTEGER REFERENCES folders(id) ON DELETE CASCADE,
    label_id            INTEGER REFERENCES labels(id) ON DELETE CASCADE,
    enabled             INTEGER NOT NULL DEFAULT 1,
    progress_message_id INTEGER NOT NULL DEFAULT 0,
    sort_order          INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO mail_rules(
    id, name, field, operator, value, account_id, action, folder_id,
    enabled, progress_message_id, sort_order, created_at, updated_at
)
SELECT id, name, field, operator, value, account_id, action, folder_id,
    enabled, progress_message_id, sort_order, created_at, updated_at
FROM mail_rules_old;

DROP TABLE mail_rules_old;

CREATE INDEX idx_mail_rules_enabled_progress
    ON mail_rules(enabled, progress_message_id, sort_order);
