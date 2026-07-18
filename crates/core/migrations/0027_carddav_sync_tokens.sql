-- RFC 6578 sync-token for each CardDAV address-book collection.
-- A token is advanced in the same transaction as the corresponding delta.
ALTER TABLE auxiliary_collections ADD COLUMN sync_token TEXT;
