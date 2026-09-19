-- Сроки и состояние дела у письма (specs/flag-due-dates.md).
CREATE TABLE message_tasks (
    message_id          INTEGER PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    start_at            TEXT,
    due_at              TEXT,
    reminder_at         TEXT,
    state               TEXT NOT NULL DEFAULT 'active'
                            CHECK(state IN ('active', 'done', 'detached')),
    completed_at        TEXT,
    reminder_shown_at   TEXT,
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_message_tasks_due
    ON message_tasks(state, due_at, message_id);

CREATE INDEX idx_message_tasks_reminder
    ON message_tasks(reminder_at, message_id)
    WHERE state = 'active' AND reminder_shown_at IS NULL;
