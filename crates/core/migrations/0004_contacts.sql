-- Контакты (vCard / CardDAV). Оригинал vCard - в зашифрованном blob (vcard_ref).

CREATE TABLE contacts (
    id             INTEGER PRIMARY KEY,
    account_id     INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    uid            TEXT,                            -- CardDAV UID
    display_name   TEXT    NOT NULL,
    first_name     TEXT,
    last_name      TEXT,
    organization   TEXT,
    photo_blob_ref TEXT,
    vcard_ref      TEXT,
    is_favorite    INTEGER NOT NULL DEFAULT 0,
    etag           TEXT
);

CREATE TABLE contact_emails (
    id         INTEGER PRIMARY KEY,
    contact_id INTEGER NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    email      TEXT    NOT NULL,
    kind       TEXT                                 -- home | work | other
);

CREATE INDEX idx_contact_emails ON contact_emails(email);
CREATE INDEX idx_contacts_name  ON contacts(display_name);
