-- Аккаунты и подписи. Секреты (пароли/токены) здесь НЕ хранятся -
-- они в системном keychain, тут только ссылка secret_ref.

CREATE TABLE accounts (
    id                 INTEGER PRIMARY KEY,
    uuid               TEXT    NOT NULL UNIQUE,
    email              TEXT    NOT NULL UNIQUE,
    display_name       TEXT    NOT NULL DEFAULT '',
    provider           TEXT    NOT NULL,            -- yandex | mailru | icloud | exchange | gmail | outlook | generic
    backend_kind       TEXT    NOT NULL,            -- imap | ews | jmap
    auth_kind          TEXT    NOT NULL,            -- oauth2 | app_password | password | ntlm

    imap_host          TEXT,
    imap_port          INTEGER,
    imap_security      TEXT,                        -- ssl | starttls | none
    smtp_host          TEXT,
    smtp_port          INTEGER,
    smtp_security      TEXT,
    ews_url            TEXT,
    username           TEXT,
    secret_ref         TEXT,                        -- ключ записи в keychain

    sync_range         TEXT    NOT NULL DEFAULT 'all',  -- all | days30 | days90 | year
    include_in_unified INTEGER NOT NULL DEFAULT 1,
    color              TEXT,
    enabled            INTEGER NOT NULL DEFAULT 1,
    sort_order         INTEGER NOT NULL DEFAULT 0,
    created_at         TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at         TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE signatures (
    id          INTEGER PRIMARY KEY,
    account_id  INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    kind        TEXT    NOT NULL,                   -- new | reply
    body_html   TEXT    NOT NULL DEFAULT '',
    enabled     INTEGER NOT NULL DEFAULT 1,
    UNIQUE(account_id, kind)
);
