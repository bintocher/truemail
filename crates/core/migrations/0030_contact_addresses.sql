-- Почтовые адреса контакта. Одна запись на адрес: kind повторяет типы vCard
-- (home/work/other), остальные поля - компоненты ADR из RFC 6350 и полей
-- contacts:PhysicalAddress:* в EWS. Все компоненты необязательны: серверы
-- регулярно присылают адрес, где заполнен только город или только страна.

CREATE TABLE contact_addresses (
    id          INTEGER PRIMARY KEY,
    contact_id  INTEGER NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    kind        TEXT,
    street      TEXT,
    city        TEXT,
    region      TEXT,
    postal_code TEXT,
    country     TEXT
);

CREATE INDEX idx_contact_addresses ON contact_addresses(contact_id);
