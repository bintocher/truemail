-- Номер строки может использоваться повторно после удаления метки.
-- Временный перенос признаков хранит постоянный идентификатор метки.
ALTER TABLE labels ADD COLUMN transfer_identity TEXT;
UPDATE labels SET transfer_identity=lower(hex(randomblob(16)));
CREATE TRIGGER labels_transfer_identity AFTER INSERT ON labels
WHEN NEW.transfer_identity IS NULL
BEGIN
    UPDATE labels SET transfer_identity=lower(hex(randomblob(16))) WHERE id=NEW.id;
END;
