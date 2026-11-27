CREATE INDEX idx_messages_raw_blob ON messages(raw_blob_ref) WHERE raw_blob_ref IS NOT NULL;
CREATE INDEX idx_attachments_blob ON attachments(blob_ref) WHERE blob_ref IS NOT NULL;
CREATE INDEX idx_contacts_vcard_blob ON contacts(vcard_ref) WHERE vcard_ref IS NOT NULL;
CREATE INDEX idx_events_ical_blob ON events(ical_ref) WHERE ical_ref IS NOT NULL;
