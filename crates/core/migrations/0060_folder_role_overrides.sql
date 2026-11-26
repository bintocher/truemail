-- NULL означает, что пользователь явно снял назначение.
CREATE TABLE folder_role_overrides (
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    folder_id INTEGER REFERENCES folders(id) ON DELETE SET NULL,
    PRIMARY KEY(account_id, role)
);
