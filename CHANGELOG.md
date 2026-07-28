**English** · [Русский](CHANGELOG.ru.md)

# Changelog

All notable changes are documented here. The format follows Keep a Changelog;
versions use Semantic Versioning.

## [Unreleased]

## [0.2.2] - 2026-07-29

### Added

- A clear-filter button next to the funnel: it shows up only when a filter is
  actually narrowing the list. Hovering lists the active conditions, clicking
  removes all of them.
- Opening the filter menu now puts the caret straight into the text field.

### Fixed

- Memory usage. The UI rendering process grew past a gigabyte; it now runs under
  a heap cap and returns the excess to the system.
- A leak in account settings: colour-picker handlers piled up on every
  background data refresh and kept obsolete markup alive.
- A hidden window now releases memory: the message list markup is dropped and is
  not rebuilt while the window stays hidden. On return the list appears
  immediately and scrolls back to the message you left off at.

## [0.2.1] - 2026-07-26

### Fixed

- The app no longer downloads mail and burns CPU while nobody is using it.
  Smart-folder backfill ran in an endless loop: it pulled old messages from the
  server, restarted itself when those messages hit the database, and kept going
  for days. A day of uptime produced thousands of pointless server round-trips
  and grew memory usage to several gigabytes.
- The message list no longer grows in memory without bound: at most 8000 recent
  messages are kept, and the open message plus everything on screen stay.
- Data refresh after a sync runs at most once every 5 seconds, and is deferred
  while the window is hidden - previously every sync event reloaded all folders,
  contacts and calendars in full.
- The routine mail-watch reconnect (roughly every 90 seconds) no longer triggers
  a full data reload when nothing changed; new mail, deletions and flag changes
  still refresh the list immediately.
- The "Message sources for smart folders" settings block no longer restarts the
  message list on every background refresh - only when the user changes the
  sources. Previously this reset the list scroll position.
- Restoring the scroll position after a data refresh is no longer mistaken for
  user scrolling and no longer triggers a server fetch.
- Message-list bookkeeping is faster: the per-folder pass is no longer quadratic.

## [0.2.0] - 2026-07-25

### Added

- Labels for mail: a section of their own in the sidebar, a list of labels with
  colours in settings, assigning a label straight from the message menu and a
  coloured badge in the list. Sorting rules can also assign a label on their
  own.
- The message list loads older mail from the server by itself when you scroll to
  the end: in batches, with an indicator and a clear status. Works for Exchange,
  Gmail and ordinary mail (Yandex, Outlook and others). If the list holds only a
  few messages, the app tops it up without waiting for you to scroll.
- For every folder you can choose what is shown next to its name: the total
  number of messages, the number of unread ones, or nothing.
- In conversation mode actions apply to the whole thread at once: mark as read,
  move, delete.
- The "Cc" list in the message header expands on click.
- Esc closes any pop-up window.

### Fixed

- The message context menu no longer runs off the edge of the window - it always
  opens towards the visible side.
- An opened message no longer disappears from unread at that very moment: it
  stays where it is until you move on to another one.
- Exchange folders line up in the same tree as on the server: nested folders no
  longer scatter across the top level.
- Meeting invitations from Exchange show their participants, and the reply
  buttons finally appear.
- Meeting reply buttons are now the same width, and the chosen reply is
  highlighted.
- The message list header no longer says "4 accounts" when a single account's
  folder is open - it shows the number of messages in that folder instead.
- The message list no longer jumps back to the top when you switch to another
  program and back, or when the data refreshes.
- Yandex Mail loads older messages again - the request used to go out for
  nothing and the list hit a ceiling.
- Loading older messages no longer repeats them or gets stuck in the same place.
- An action on a collapsed conversation applies to that conversation only:
  messages with the same subject from other senders used to be caught too.
- Mail rules no longer fire on old messages pulled in by scrolling - years-old
  correspondence stays where it is instead of scattering across folders.
- Deleting a label no longer deletes the rule that assigned it: the rule stays,
  you only pick a label for it again.
- Recurring events keep their settings when saved: day of month, ordinal week
  and the end time of the repetition stay in place, and the server accepts the
  exception dates without errors.

### Changed

- A calendar event is set up by clicking ready-made fields instead of typing
  service strings by hand. The account and the calendar are shown as text when
  editing an event, and the window is wider - every field is visible at once.
- In the Russian interface everything is called "метка": some places used to say
  "тег" and others "метка".

## [0.1.7] - 2026-07-23

### Fixed

- Installing a new version over an older one no longer asks you to remove the
  previous one first - the update installs straight away and keeps your mail
  and settings. You are still asked to uninstall only when installing the same
  version again or going back to an older one.

## [0.1.6] - 2026-07-22

### Fixed

- The app could lose access to Google mail and show "account not configured":
  sign-in stopped renewing and Gmail stopped updating. Sign-in now stays
  connected as it should.

### Changed

- The project moved to GitHub: https://github.com/bintocher/truemail. Sources
  and updates now come from there; the update arrives on its own, as usual.

## [0.1.5] - 2026-07-21

### Added

- Postal addresses for contacts: model, storage, card and edit form,
  synchronization with CardDAV (`ADR`), Exchange
  (`contacts:PhysicalAddress:*`) and Google People.
- Recurring Exchange events are read and written: daily, weekly, monthly and
  yearly rules (including relative ones such as "second Tuesday"), with
  `UNTIL` and `COUNT` bounds. Dropping the recurrence clears it on the server
  through `DeleteItemField`.
- CalDAV/CardDAV discovery through DNS SRV records (RFC 6764), including the
  `path=` hint from TXT.
- The Windows build now runs on PowerShell 7.

### Fixed

- The address book stopped at 500 contacts: the hard `LIMIT 500` is gone and
  emails, phones and addresses are read with four queries instead of `1 + 3N`.
- Phones and addresses removed from a contact stayed on the Exchange server;
  they are now deleted through `DeleteItemField`.
- The update prompt repeated every 6 hours for the same version.

### Security

- An SRV target is accepted only inside the mail domain: DNS without DNSSEC
  can be spoofed in transit, and the password is sent to that address next.

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
