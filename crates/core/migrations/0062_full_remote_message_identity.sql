-- Полный серверный идентификатор используется при записи и сверке.
CREATE INDEX idx_messages_folder_remote_identity ON messages(folder_id, remote_id)
WHERE remote_id IS NOT NULL;

UPDATE folders SET sync_token=NULL WHERE account_id IN (
    SELECT id FROM accounts WHERE provider='gmail' OR backend_kind IN ('ews','jmap')
);
