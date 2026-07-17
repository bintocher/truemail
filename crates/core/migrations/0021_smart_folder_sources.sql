ALTER TABLE smart_folders ADD COLUMN stable_id TEXT;
ALTER TABLE smart_conditions ADD COLUMN group_index INTEGER NOT NULL DEFAULT 0;
ALTER TABLE smart_conditions ADD COLUMN group_logic TEXT NOT NULL DEFAULT 'all';
ALTER TABLE smart_conditions ADD COLUMN unit TEXT;
ALTER TABLE smart_conditions ADD COLUMN value2 TEXT;

UPDATE smart_folders SET stable_id = CASE id
    WHEN 1 THEN 'all-inbox'
    WHEN 2 THEN 'all-important'
    WHEN 3 THEN 'all-sent'
    WHEN 4 THEN 'all-drafts'
    WHEN 5 THEN 'last-24-hours'
    WHEN 6 THEN 'all-unread'
    WHEN 7 THEN 'with-attachments'
    WHEN 8 THEN 'awaiting-my-reply'
    ELSE 'custom-' || id
END WHERE stable_id IS NULL;

CREATE UNIQUE INDEX idx_smart_folders_stable_id ON smart_folders(stable_id);

INSERT INTO smart_conditions(smart_folder_id, field, op, value, group_index, group_logic)
SELECT id, 'folder_role', 'equals', 'inbox', 0, 'all' FROM smart_folders
WHERE stable_id='all-inbox' AND NOT EXISTS (SELECT 1 FROM smart_conditions WHERE smart_folder_id=smart_folders.id);
INSERT INTO smart_conditions(smart_folder_id, field, op, value, group_index, group_logic)
SELECT id, 'importance', 'equals', 'flagged', 0, 'all' FROM smart_folders
WHERE stable_id='all-important' AND NOT EXISTS (SELECT 1 FROM smart_conditions WHERE smart_folder_id=smart_folders.id);
INSERT INTO smart_conditions(smart_folder_id, field, op, value, group_index, group_logic)
SELECT id, 'folder_role', 'equals', 'sent', 0, 'all' FROM smart_folders
WHERE stable_id='all-sent' AND NOT EXISTS (SELECT 1 FROM smart_conditions WHERE smart_folder_id=smart_folders.id);
INSERT INTO smart_conditions(smart_folder_id, field, op, value, group_index, group_logic)
SELECT id, 'folder_role', 'equals', 'drafts', 0, 'all' FROM smart_folders
WHERE stable_id='all-drafts' AND NOT EXISTS (SELECT 1 FROM smart_conditions WHERE smart_folder_id=smart_folders.id);
INSERT INTO smart_conditions(smart_folder_id, field, op, value, group_index, group_logic)
SELECT id, 'draft_state', 'equals', 'draft', 1, 'all' FROM smart_folders
WHERE stable_id='all-drafts';
INSERT INTO smart_conditions(smart_folder_id, field, op, value, group_index, group_logic, unit)
SELECT id, 'date', 'within_last', '24', 0, 'all', 'hours' FROM smart_folders
WHERE stable_id='last-24-hours' AND NOT EXISTS (SELECT 1 FROM smart_conditions WHERE smart_folder_id=smart_folders.id);
INSERT INTO smart_conditions(smart_folder_id, field, op, value, group_index, group_logic)
SELECT id, 'read_state', 'equals', 'unread', 0, 'all' FROM smart_folders
WHERE stable_id='all-unread' AND NOT EXISTS (SELECT 1 FROM smart_conditions WHERE smart_folder_id=smart_folders.id);
INSERT INTO smart_conditions(smart_folder_id, field, op, value, group_index, group_logic)
SELECT id, 'attachment', 'equals', 'has', 0, 'all' FROM smart_folders
WHERE stable_id='with-attachments' AND NOT EXISTS (SELECT 1 FROM smart_conditions WHERE smart_folder_id=smart_folders.id);
INSERT INTO smart_conditions(smart_folder_id, field, op, value, group_index, group_logic)
SELECT id, 'folder_role', 'equals', 'inbox', 0, 'all' FROM smart_folders
WHERE stable_id='awaiting-my-reply' AND NOT EXISTS (SELECT 1 FROM smart_conditions WHERE smart_folder_id=smart_folders.id);
INSERT INTO smart_conditions(smart_folder_id, field, op, value, group_index, group_logic)
SELECT id, 'reply_state', 'equals', 'unanswered', 0, 'all' FROM smart_folders
WHERE stable_id='awaiting-my-reply';

INSERT OR IGNORE INTO unified_folders(role) VALUES ('all');
