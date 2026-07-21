**English** · [Русский](CHANGELOG.ru.md)

# Changelog

All notable changes are documented here. The format follows Keep a Changelog;
versions use Semantic Versioning.

## [Unreleased]

## [0.1.4] - 2026-07-21

### Added

- Notifications for meeting changes: created, rescheduled, cancelled, renamed,
  location changed, attendee list changed. Cards show the date, time, location,
  organizer and attendee count.
- Replying to an invitation straight from the notification, sending an iTIP
  REPLY to the organizer; the answer can be changed later.
- Exchange: creating, updating and deleting events and contacts over EWS.
- CalDAV and CardDAV for iCloud, Mail.ru, Outlook and arbitrary servers with
  `.well-known` discovery (RFC 6764) and sync-collection (RFC 6578).
- Creating a folder on the server: IMAP, Exchange, JMAP and Gmail.
- The selected calendar view persists across restarts; the grid stretches to
  the available height and follows the configured working hours.
- Sending mail in the background and update checks every 6 hours.

### Fixed

- New-mail notifications appeared twice and for messages that were not new:
  the card is now built from the actual new messages and deduplication is
  shared across all synchronization paths.
- Flag synchronization overwrote the seen state in the outbox payload.

### Security

- Attachment saving sanitizes the name, canonicalizes the path and requires it
  to sit exactly in the chosen directory.
- List-Unsubscribe One-Click refuses private addresses and does not follow
  redirects; the address is pinned before connecting (DNS rebinding).
- Only `data:` URIs of raster images are allowed in messages.
- Changing the authentication method removes the stale keychain entry.
- Mail addresses are masked in logs; logs are kept for 7 days and release
  builds log at `info`.

## [0.1.0]

### Added

- SQLCipher storage, encrypted blob store and system-keychain integration.
- Yandex OAuth/IMAP/CalDAV/CardDAV synchronization with IMAP IDLE.
- Desktop onboarding, mail, calendar, contacts, search and settings UI.

### Security

- IMAP downloads use `BODY.PEEK[]` and never mark messages read implicitly.
- Blob references are random and bound to XChaCha20-Poly1305 ciphertext via AAD.
- Installation keys combine OS CSPRNG and user input through HKDF.
