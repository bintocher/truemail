ALTER TABLE draft_submissions ADD COLUMN cleanup_failed INTEGER NOT NULL DEFAULT 0;
ALTER TABLE send_request_keys ADD COLUMN draft_recorded INTEGER NOT NULL DEFAULT 0;
ALTER TABLE send_request_keys ADD COLUMN draft_cleanup_failed INTEGER NOT NULL DEFAULT 0;

UPDATE send_request_keys SET draft_recorded=1
 WHERE EXISTS (
    SELECT 1 FROM draft_submissions d JOIN outbox_ops o ON o.id=d.operation_id
     WHERE o.id=send_request_keys.operation_id
       AND d.account_id=send_request_keys.account_id
       AND o.request_key=send_request_keys.request_key
 );
