-- Настройки (ключ-значение), горячие клавиши, внешний API (capability), доверие изображениям.

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE keybindings (
    id     INTEGER PRIMARY KEY,
    action TEXT    NOT NULL UNIQUE,
    scope  TEXT    NOT NULL DEFAULT 'local',        -- local | global
    combo  TEXT    NOT NULL
);

-- Внешний API доступа к почте (см. docs/06-ai-api.md): токены и права (capability).
CREATE TABLE api_clients (
    id         INTEGER PRIMARY KEY,
    name       TEXT    NOT NULL,
    token_ref  TEXT    NOT NULL,                    -- ссылка на токен в keychain
    caps       TEXT    NOT NULL DEFAULT '',         -- JSON: [read, search, send, labels, calendar, network]
    created_at TEXT    NOT NULL DEFAULT (datetime('now')),
    last_used  TEXT
);

CREATE TABLE api_audit (
    id        INTEGER PRIMARY KEY,
    client_id INTEGER REFERENCES api_clients(id) ON DELETE SET NULL,
    action    TEXT    NOT NULL,
    detail    TEXT,
    at        TEXT    NOT NULL DEFAULT (datetime('now'))
);

-- Доверенные отправители изображений ("запомнить выбор для отправителя")
CREATE TABLE image_trust (
    id     INTEGER PRIMARY KEY,
    sender TEXT    NOT NULL UNIQUE,
    allow  INTEGER NOT NULL DEFAULT 1
);

-- Значения по умолчанию
INSERT INTO settings (key, value) VALUES
    ('locale',              'ru'),
    ('theme',               'light'),
    ('density',             'normal'),
    ('accent',              'indigo'),
    ('ui_scale',            '100'),
    ('expert_mode',         '0'),
    ('external_images',     'block'),        -- block | ask | always
    ('image_banner',        'always'),       -- always | remember_sender | never
    ('strip_utm',           '1'),
    ('show_auth_status',    '1'),
    ('cache_mode',          'unlimited'),    -- unlimited | limited
    ('cache_keep_days',     '0'),
    ('cache_limit_per_acc', '0'),
    ('data_dir',            ''),
    ('master_password',     '0'),
    ('calendar_view',       'month'),        -- запоминаемый вид календаря
    ('start_on_boot',       '1'),
    ('minimize_to_tray',    '1'),
    ('notifications',       '1');

-- Горячие клавиши по умолчанию (локальные + глобальные), см. мокап
INSERT INTO keybindings (action, scope, combo) VALUES
    ('toggle_window',   'global', 'Ctrl+Shift+M'),
    ('compose_global',  'global', 'Ctrl+Shift+C'),
    ('quick_search',    'global', 'Ctrl+Shift+F'),
    ('palette',         'local',  'Ctrl+K'),
    ('compose',         'local',  'C'),
    ('reply',           'local',  'R'),
    ('reply_all',       'local',  'A'),
    ('forward',         'local',  'F'),
    ('archive',         'local',  'E'),
    ('snooze',          'local',  'H'),
    ('next_message',    'local',  'J'),
    ('prev_message',    'local',  'K'),
    ('delete',          'local',  'Del');
