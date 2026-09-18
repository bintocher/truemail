-- Отбор писем по точному адресу отправителя (mail-rules-conditions-and-actions.md,
-- S-024): сравнение идёт по адресу в нижнем регистре, поэтому нужен индекс по
-- выражению, а не по столбцу. Таблица писем при этом не перестраивается.
CREATE INDEX IF NOT EXISTS idx_messages_from_addr_lower ON messages(lower(from_addr));
