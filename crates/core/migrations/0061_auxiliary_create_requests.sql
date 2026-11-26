CREATE TABLE auxiliary_create_requests (
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    request_key TEXT NOT NULL,
    saved INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY(account_id, kind, request_key)
);
