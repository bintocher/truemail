-- Opaque provider change token. Gmail stores profile/historyId here; unlike
-- IMAP numeric cursors it must remain TEXT because the API defines uint64 as a
-- JSON string and other providers may use non-numeric tokens.
ALTER TABLE folders ADD COLUMN sync_token TEXT;
