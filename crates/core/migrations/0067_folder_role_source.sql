ALTER TABLE folders ADD COLUMN role_source TEXT
    CHECK (role_source IN ('server','name','user','legacy'));
UPDATE folders SET role_source = CASE WHEN EXISTS (
    SELECT 1 FROM folder_role_overrides o
    WHERE o.account_id=folders.account_id AND o.folder_id=folders.id AND o.role=folders.role
) THEN 'user' ELSE 'legacy' END WHERE role IS NOT NULL;
