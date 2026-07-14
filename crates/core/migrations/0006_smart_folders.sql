-- Умные папки (сохранённые фильтры), сквозные (unified) папки и панель действий над письмом.
-- Соответствует мокапу: единый раздел "Умные папки", настройка сквозных папок, настройка панели.

CREATE TABLE smart_folders (
    id          INTEGER PRIMARY KEY,
    name        TEXT    NOT NULL,
    icon        TEXT,
    match_logic TEXT    NOT NULL DEFAULT 'all',     -- all (И) | any (ИЛИ)
    is_builtin  INTEGER NOT NULL DEFAULT 0,
    enabled     INTEGER NOT NULL DEFAULT 1,
    sort_order  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE smart_conditions (
    id              INTEGER PRIMARY KEY,
    smart_folder_id INTEGER NOT NULL REFERENCES smart_folders(id) ON DELETE CASCADE,
    field           TEXT    NOT NULL,               -- from|to|subject|body|account|status|attachment|label|folder|date
    op              TEXT    NOT NULL,               -- contains|not_contains|equals
    value           TEXT    NOT NULL DEFAULT ''
);

-- Сквозные папки: какие папки аккаунтов входят в объединённые (Все входящие и т.п.)
CREATE TABLE unified_folders (
    id   INTEGER PRIMARY KEY,
    role TEXT NOT NULL UNIQUE                       -- inbox | important | sent | drafts
);

CREATE TABLE unified_sources (
    id         INTEGER PRIMARY KEY,
    unified_id INTEGER NOT NULL REFERENCES unified_folders(id) ON DELETE CASCADE,
    folder_id  INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    included   INTEGER NOT NULL DEFAULT 1,
    UNIQUE(unified_id, folder_id)
);

-- Настройка панели кнопок над письмом (видимость и порядок)
CREATE TABLE toolbar_actions (
    id         INTEGER PRIMARY KEY,
    action_key TEXT    NOT NULL UNIQUE,             -- reply|replyall|forward|snooze|archive|trash|unsub|spam|...
    visible    INTEGER NOT NULL DEFAULT 1,
    sort_order INTEGER NOT NULL DEFAULT 0
);

-- Встроенные умные папки по умолчанию (см. мокап)
INSERT INTO smart_folders (name, icon, is_builtin, sort_order) VALUES
    ('Все входящие',        'inbox',     1, 0),
    ('Все важные',          'star',      1, 1),
    ('Все отправленные',    'send',      1, 2),
    ('Все черновики',       'draft',     1, 3),
    ('Сегодня',             'cal',       1, 4),
    ('Непрочитанные (все)', 'search',    1, 5),
    ('С вложениями',        'paperclip', 1, 6),
    ('Ждут ответа',         'flag',      1, 7);

INSERT INTO unified_folders (role) VALUES ('inbox'), ('important'), ('sent'), ('drafts');

INSERT INTO toolbar_actions (action_key, visible, sort_order) VALUES
    ('reply', 1, 0), ('replyall', 1, 1), ('forward', 1, 2),
    ('snooze', 1, 3), ('archive', 1, 4), ('trash', 1, 5),
    ('unsub', 0, 6), ('spam', 0, 7), ('folder', 0, 8),
    ('flag', 0, 9), ('pin', 0, 10), ('translate', 0, 11), ('print', 0, 12);
