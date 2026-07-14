-- Несекретные служебные маркеры миграций хранилища.
CREATE TABLE storage_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
