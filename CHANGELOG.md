**English** · [Русский](CHANGELOG.ru.md)

# Changelog

All notable changes are documented here. The format follows Keep a Changelog;
versions use Semantic Versioning.

## [Unreleased]

## [0.2.8] - 2026-09-02

### Changed

- In "Sent" and "Drafts" the message list shows the recipient instead of the
  sender. It used to show the user's own name, so the list looked like a column
  of the same name. With several recipients the first one and a counter of the
  rest are shown. The rule follows the role of the message's own folder, so it
  also applies in smart folders, tag views and conversations.
- The message header shows every available address: "From", "To" and "Cc". The
  "To" line did not exist before. Empty lines are not rendered, and each address
  has a tooltip with the full "Name (email)" form.

### Fixed

- Selected weekday and reminder buttons in the event window are readable again:
  the caption colour on an accent fill now comes from a dedicated variable
  instead of the accent text colour meant for a plain background.
- A recipient name made of spaces no longer breaks the avatar initial - the
  address is shown instead.

## [0.2.7] - 2026-08-09

### Fixed

- A smart folder counter changes the moment a message is read. The sidebar
  number used to wait for the next background reload, so in the "Unread" folder
  it disagreed with the list for up to half a minute.
- Moving messages in Gmail between "All mail" and "Inbox" is no longer rejected
  by the server. The request asked to both add and remove the same label, so the
  operations got stuck in the queue and retried for nothing.
- The message list memory limit applies again while a smart folder is open.

### Added

- Interface journal: page errors and memory usage are written to the common log.
  A window that dies from memory exhaustion now leaves a trail - previously the
  journal stayed empty.

## [0.2.6] - 2026-08-07

### Added

- Message counters for smart folders: the context menu of a smart folder in the
  sidebar can show unread, total, or both - just like ordinary mailbox folders.
  The counter is off by default.

### Fixed

- The minimize button minimizes the window again: hiding to the tray was
  rejected by the permission list, so the click did nothing.
- A renamed built-in smart folder shows its own name in the sidebar and in the
  settings list. The default caption used to win there, so the name set in
  settings was only visible inside the edit dialog. The name field of a built-in
  folder now shows the default caption as a hint: clear the field to bring it
  back and have the name follow the interface language again.

## [0.2.5] - 2026-08-01

### Added

- A "Minimize to tray" setting: the minimize button hides the window into the
  tray icon instead of the taskbar. Turn it off for ordinary minimizing.

### Fixed

- Smart folders with legacy conditions show messages again. Conditions created
  by early versions kept the old vocabulary ("Status" instead of "Read state",
  not_seen instead of unread) and the query did not understand them: "Unread
  (all)", for one, stayed empty while the mailbox had unread mail.
- Smart folders with a date condition no longer break the message query: an
  absurdly large period (billions of weeks) failed instead of returning results.
- The window now opens by itself after an update. The installer restarted the
  app with the previous process arguments, so a copy started by autostart hid
  itself in the tray and could only be restored from the tray icon.

## [0.2.4] - 2026-07-31

### Added

- A custom title bar replaces the system one: drag the window by it, and next to
  the minimize/maximize/close buttons an "Update" button appears when a new
  version is out. Closing still hides the app to the tray.
- Updates are checked automatically every 6 hours and downloaded ahead of time,
  so the "Update" button installs right away instead of waiting for a download.
- Downloaded update packages no longer pile up: once an update is installed the
  old installers are removed, leaving at most the one still pending.

## [0.2.3] - 2026-07-31

### Added

- "Send to -> truemail" in the Windows Explorer context menu: the selected files
  open as attachments in a new message. The installer adds the entry, and a
  setting turns it off and back on.
- A "Today" button in the calendar header: it jumps back to the current date in
  whichever view is active - month, week or day.

### Fixed

- The "Launch on system startup" switch showed the off state even when autostart
  was enabled: it read the state before the bridge to the core was up.

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
