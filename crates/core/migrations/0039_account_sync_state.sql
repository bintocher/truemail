-- Постоянное состояние синхронизации почты аккаунта (mail-sync-visible-state.md,
-- issue #69). Раньше исход прохода жил только в памяти процесса и терялся при
-- перезапуске - теперь последний успех/ошибка хранятся в самой таблице
-- аккаунтов. Столбцы добавляются со значениями по умолчанию (NULL/0), поэтому
-- у существующих аккаунтов состояние до первого нового прохода просто
-- отсутствует и загрузке базы не мешает.
ALTER TABLE accounts ADD COLUMN last_sync_at TEXT NULL;
ALTER TABLE accounts ADD COLUMN last_sync_error TEXT NULL;
ALTER TABLE accounts ADD COLUMN last_sync_error_kind TEXT NULL;
ALTER TABLE accounts ADD COLUMN needs_reauth INTEGER NOT NULL DEFAULT 0;
