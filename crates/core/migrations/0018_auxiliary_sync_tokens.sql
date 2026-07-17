-- Provider cursors for incremental calendar/contact/task synchronization.
ALTER TABLE calendars ADD COLUMN sync_token TEXT;
ALTER TABLE auxiliary_collections ADD COLUMN ctag TEXT;

CREATE TABLE auxiliary_sync_state (
    account_id  INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL,
    sync_token  TEXT NOT NULL,
    updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY(account_id, kind)
);
