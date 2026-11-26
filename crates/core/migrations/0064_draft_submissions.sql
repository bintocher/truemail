CREATE TABLE draft_submissions (
    draft_id INTEGER NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    version TEXT NOT NULL,
    operation_id INTEGER REFERENCES outbox_ops(id) ON DELETE SET NULL,
    cleanup_operation_id INTEGER REFERENCES outbox_ops(id) ON DELETE SET NULL,
    PRIMARY KEY(draft_id, version)
);
