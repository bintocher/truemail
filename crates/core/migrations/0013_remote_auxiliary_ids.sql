-- Адреса серверных объектов нужны для безопасного CRUD календаря и контактов.
ALTER TABLE events ADD COLUMN remote_url TEXT;
ALTER TABLE contacts ADD COLUMN remote_url TEXT;

CREATE INDEX IF NOT EXISTS idx_events_remote_url ON events(remote_url);
CREATE INDEX IF NOT EXISTS idx_contacts_remote_url ON contacts(remote_url);
