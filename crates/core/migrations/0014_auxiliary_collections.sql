-- Обнаруженные writable DAV-коллекции. Они нужны даже при пустой адресной
-- книге, когда ни одного remote_url контакта ещё нет.
CREATE TABLE auxiliary_collections (
    id          INTEGER PRIMARY KEY,
    account_id  INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    kind        TEXT    NOT NULL,
    url         TEXT    NOT NULL,
    UNIQUE(account_id, kind, url)
);

CREATE INDEX idx_auxiliary_collections_account
    ON auxiliary_collections(account_id, kind);
