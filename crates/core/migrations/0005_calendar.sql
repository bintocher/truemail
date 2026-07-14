-- Календари и события по RFC 5545 (iCalendar). Оригинал VEVENT - в ical_ref (зашифр.).

CREATE TABLE calendars (
    id          INTEGER PRIMARY KEY,
    account_id  INTEGER REFERENCES accounts(id) ON DELETE CASCADE,
    uid         TEXT,
    name        TEXT    NOT NULL,
    color       TEXT,
    kind        TEXT    NOT NULL,                   -- caldav | exchange | ical_subscription
    url         TEXT,
    visible     INTEGER NOT NULL DEFAULT 1,
    read_only   INTEGER NOT NULL DEFAULT 0,
    ctag        TEXT
);

CREATE TABLE events (
    id           INTEGER PRIMARY KEY,
    calendar_id  INTEGER NOT NULL REFERENCES calendars(id) ON DELETE CASCADE,
    uid          TEXT,                              -- iCalendar UID
    summary      TEXT    NOT NULL DEFAULT '',
    description  TEXT,
    location     TEXT,
    url          TEXT,
    dtstart      TEXT    NOT NULL,                  -- ISO 8601
    dtend        TEXT,
    all_day      INTEGER NOT NULL DEFAULT 0,
    timezone     TEXT,                              -- TZID
    rrule        TEXT,                              -- RFC5545 RRULE
    status       TEXT,                              -- CONFIRMED | TENTATIVE | CANCELLED
    transp       TEXT,                              -- OPAQUE(занят) | TRANSPARENT(свободен)
    class        TEXT,                              -- PUBLIC | PRIVATE | CONFIDENTIAL
    organizer    TEXT,
    categories   TEXT,
    ical_ref     TEXT,
    etag         TEXT,
    sequence     INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_events_start ON events(dtstart);
CREATE INDEX idx_events_cal   ON events(calendar_id);

CREATE TABLE event_attendees (
    id        INTEGER PRIMARY KEY,
    event_id  INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    email     TEXT    NOT NULL,
    name      TEXT,
    role      TEXT,                                 -- REQ-PARTICIPANT | OPT-PARTICIPANT | CHAIR
    partstat  TEXT,                                 -- NEEDS-ACTION | ACCEPTED | DECLINED | TENTATIVE
    rsvp      INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE event_alarms (
    id              INTEGER PRIMARY KEY,
    event_id        INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    trigger_minutes INTEGER NOT NULL,               -- за N минут до начала
    action          TEXT    NOT NULL DEFAULT 'DISPLAY'
);
