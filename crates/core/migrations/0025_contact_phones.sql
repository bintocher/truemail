-- Несколько телефонных номеров контакта, включая тип и добавочный номер.

CREATE TABLE contact_phones (
    id         INTEGER PRIMARY KEY,
    contact_id INTEGER NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    number     TEXT    NOT NULL,
    kind       TEXT,
    extension  TEXT
);

CREATE INDEX idx_contact_phones ON contact_phones(contact_id);

-- Некоторые почтовые заголовки содержат отображаемое имя в одинарных или
-- двойных кавычках. В интерфейсе эти служебные кавычки не нужны.
UPDATE contacts
SET display_name = trim(display_name, ' ''"')
WHERE display_name LIKE '''%'''
   OR display_name LIKE '"%"';
