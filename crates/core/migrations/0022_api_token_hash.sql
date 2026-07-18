ALTER TABLE api_clients ADD COLUMN token_hash TEXT;
CREATE UNIQUE INDEX idx_api_clients_token_hash ON api_clients(token_hash) WHERE token_hash IS NOT NULL;
CREATE INDEX idx_api_audit_at ON api_audit(at DESC);
