CREATE TABLE draft_submissions (
    draft_id INTEGER NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    version TEXT NOT NULL,
    operation_id INTEGER NOT NULL,
    PRIMARY KEY(draft_id, version)
);
