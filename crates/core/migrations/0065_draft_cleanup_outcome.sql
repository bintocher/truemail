ALTER TABLE draft_submissions ADD COLUMN cleanup_failed INTEGER NOT NULL DEFAULT 0;
ALTER TABLE send_request_keys ADD COLUMN draft_recorded INTEGER NOT NULL DEFAULT 0;
ALTER TABLE send_request_keys ADD COLUMN draft_cleanup_failed INTEGER NOT NULL DEFAULT 0;
