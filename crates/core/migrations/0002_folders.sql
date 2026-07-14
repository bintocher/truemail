-- Папки ящиков и состояние синхронизации (UIDVALIDITY/HIGHESTMODSEQ - см. docs/05-protocols.md).

CREATE TABLE folders (
    id            INTEGER PRIMARY KEY,
    account_id    INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    remote_path   TEXT    NOT NULL,                 -- путь на сервере (IMAP mailbox)
    display_name  TEXT    NOT NULL,
    special_use   TEXT,                             -- \Inbox \Sent \Drafts \Junk \Trash \Archive (RFC 6154)
    role          TEXT,                             -- inbox | sent | drafts | spam | trash | archive
    parent_id     INTEGER REFERENCES folders(id) ON DELETE CASCADE,

    uidvalidity   INTEGER,
    uidnext       INTEGER,
    highestmodseq INTEGER,
    unread_count  INTEGER NOT NULL DEFAULT 0,
    total_count   INTEGER NOT NULL DEFAULT 0,
    subscribed    INTEGER NOT NULL DEFAULT 1,
    last_synced   TEXT,
    UNIQUE(account_id, remote_path)
);

CREATE INDEX idx_folders_account ON folders(account_id);
CREATE INDEX idx_folders_role    ON folders(account_id, role);
