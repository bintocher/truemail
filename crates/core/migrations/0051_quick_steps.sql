-- Быстрые действия используют тот же словарь действий, что и правила
-- обработки почты (specs/quick-steps.md).
CREATE TABLE quick_steps (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    icon        TEXT NOT NULL DEFAULT 'star',
    sort_order  INTEGER NOT NULL DEFAULT 0,
    hotkey_slot INTEGER CHECK(hotkey_slot BETWEEN 1 AND 10),
    state       TEXT NOT NULL DEFAULT 'ok'
                    CHECK(state IN ('ok', 'needs_attention')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE UNIQUE INDEX idx_quick_steps_hotkey_slot
    ON quick_steps(hotkey_slot) WHERE hotkey_slot IS NOT NULL;

CREATE TABLE quick_step_actions (
    id          INTEGER PRIMARY KEY,
    quick_step_id INTEGER NOT NULL REFERENCES quick_steps(id) ON DELETE CASCADE,
    sort_order    INTEGER NOT NULL DEFAULT 0,
    kind        TEXT NOT NULL CHECK(kind IN (
                    'move', 'archive', 'spam', 'trash', 'delete',
                    'label_add', 'label_remove', 'mark_read', 'mark_flagged'
                )),
    folder_id   INTEGER REFERENCES folders(id) ON DELETE SET NULL,
    folder_role TEXT,
    label_id    INTEGER REFERENCES labels(id) ON DELETE SET NULL
);

CREATE INDEX idx_quick_step_actions_step
    ON quick_step_actions(quick_step_id, sort_order);

CREATE TRIGGER quick_step_folder_lost
AFTER UPDATE OF folder_id ON quick_step_actions
WHEN OLD.folder_id IS NOT NULL AND NEW.folder_id IS NULL
BEGIN
    UPDATE quick_steps SET state = 'needs_attention', updated_at = datetime('now')
     WHERE id = NEW.quick_step_id;
END;

CREATE TRIGGER quick_step_label_lost
AFTER UPDATE OF label_id ON quick_step_actions
WHEN OLD.label_id IS NOT NULL AND NEW.label_id IS NULL
BEGIN
    UPDATE quick_steps SET state = 'needs_attention', updated_at = datetime('now')
     WHERE id = NEW.quick_step_id;
END;
