-- Письма, догруженные прокруткой вглубь папки, помечаются отдельно. Они
-- получают новые rowid, и правила приняли бы старую переписку за только что
-- пришедшую: прокрутка входящих в архив разослала бы её по папкам на сервере.
ALTER TABLE messages ADD COLUMN backfilled INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_messages_backfilled ON messages(backfilled);
