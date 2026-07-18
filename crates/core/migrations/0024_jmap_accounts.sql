-- JMAP stores a Session resource URL, not an IMAP host or EWS endpoint.
ALTER TABLE accounts ADD COLUMN jmap_url TEXT;
