-- Whether the letter has a text/plain part of its own. Stored parse results
-- carry no mark yet, so they are parsed again on the first opening.
ALTER TABLE message_content_cache ADD COLUMN has_text_part INTEGER NOT NULL DEFAULT 0;
DELETE FROM message_content_cache;
