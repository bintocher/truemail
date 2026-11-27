CREATE TABLE composer_draft_files (
    id TEXT PRIMARY KEY,
    blob_ref TEXT NOT NULL UNIQUE,
    size INTEGER NOT NULL CHECK (size >= 0)
);
