-- Вторая очередь пределов: ритм фоновых циклов, ожидания автоочистки, размер
-- серверной догрузки, страница раздела истории, пороги свежести ранга и срок
-- жизни снимков стадий (crates/core/src/model/limits.rs). Значения здесь равны
-- прежним зашитым, поэтому обновление не меняет поведение ни у кого.
-- INSERT OR IGNORE, а не REPLACE - значение, уже выбранное пользователем,
-- миграция не трогает.
INSERT OR IGNORE INTO settings(key, value) VALUES
    ('limit_gmail_poll_seconds', '25'),
    ('limit_snooze_release_seconds', '30'),
    ('limit_backfill_page', '15'),
    ('limit_body_prefetch_messages', '50'),
    ('limit_body_prefetch_size_mb', '5'),
    ('limit_history_page', '100'),
    ('limit_rank_fresh_days', '30'),
    ('limit_rank_recent_days', '90'),
    ('limit_rank_old_days', '365'),
    ('limit_stage_batch', '500'),
    ('limit_stage_snapshot_hours', '24'),
    ('limit_sweep_wait_seconds', '60'),
    ('limit_sweep_max_waits', '8'),
    ('limit_sweep_full_pass_hours', '24'),
    ('limit_reminder_check_seconds', '60'),
    ('limit_update_check_hours', '6');
