[Русский](CHANGELOG.md) · **English**

# Changelog

All notable changes are documented here. The format follows Keep a Changelog;
versions use Semantic Versioning.

## [Unreleased]

## [0.3.2] - 2026-09-20

### Added

- Limits and periods are now configurable. Settings have a new "Limits" section: how many messages can be pinned in a mailbox, how many actions a quick step may chain, how long completed tasks are kept, how often the auto reply writes to the same person, how many messages a list page loads, and so on. These numbers used to be built into the program: you hit a limit nobody had written down and could not tell why the program refused. Every field says in plain words what it affects.

### Fixed

- The "what is new" window can be read to the end. It used to disappear the moment the pointer left the update button, so a long list of changes could not be read at all. The window now stays while the pointer is over it, and scrolling or a click inside pins it: it closes on the escape key or a click outside. The release notes are also split into sections and lists instead of raw text with hashes and dashes.
- A keyboard shortcut can be cleared. An assigned combination could not be removed at all, and pressing Del to erase it produced an error. Every field now has a clear button.
- The pinned message limit no longer disagrees with what the program shows. The list made room for 50 pinned messages while only 20 could be pinned, and the refusal explained nothing.
- The quick step count limit never applied: the program compared against a value that did not exist.


## [0.3.1] - 2026-09-19

### Fixed

- Task dates and pinned messages no longer disappear on their own. A routine mail check used to erase them: moving a message from a phone, or the mailbox being reindexed, was enough for the marks to vanish without warning. They now travel with the message.
- Clearing the flag on several messages at once no longer wipes their dates silently. The program used to ask about dates for one of the selected messages only: if that one had no date, no question appeared at all and the other messages lost their tasks.
- Completed tasks no longer come back to life. When the importance mark failed to reach the server eight times, the server returned the old value and the task was reopened.
- Overdue tasks are visible again and reminders arrive on time. Because a task time was stored in one format and compared in another, there were no overdue tasks at all, and a reminder only arrived after midnight. Existing dates are corrected on first start.
- A pinned message no longer disappears from smart folders and from the list shown at startup.
- Refreshing the message list no longer drops previously loaded pages: a gap appeared in the list that only a restart could close.
- The flag and task buttons in the message header are reachable again: the toolbar hid them without offering a replacement.
- A task reminder is shown in the language of the program instead of always in Russian.

### Security

- A quick step icon is accepted only from a fixed set. It used to be typed as free text that reached the window markup unprocessed, which is enough to run foreign code inside the program.
- The folder and the label of a quick step are picked from a list. They used to be entered by internal number, and a typo sent messages to someone else's folder.


## [0.3.0] - 2026-09-19

### Added

- Mail rules are now complete. A rule used to support a single condition and a single action, and only the first matching rule was applied to a message. A rule now has condition groups "all of" and "any of" with exceptions, several actions in a row, its own position in the rule order, a manual run over selected folders and an option to stop further processing of the message.
- Blocked and trusted sender lists, by address and by domain. A sender can be blocked from the message itself or from the list, and the check runs before the rules.
- Sweep by sender in four modes: remove all messages from the sender, remove new ones as they arrive, keep only the latest message, remove messages older than a given number of days. Messages go to the trash instead of being deleted for good.
- Ignore conversation: the whole thread goes to the trash together with new messages that arrive in it. A thread is identified by message identifiers rather than by subject, so messages that used to stay outside any thread are now included; restoring looks for them in the trash.
- Undo send. After "Send" a countdown appears and the message can be returned to the composer. Sending now goes through the queue as a whole, so a dropped connection no longer loses the message together with its text: it goes out on the next connection.
- Out of office replies. On Exchange accounts the reply is set on the server and works while the computer is off; on other accounts the app replies itself and states plainly that it only does so while running. Fifteen silence rules keep it from replying to mailing lists, other automatic replies and delivery reports, so two robots never start a conversation.
- Recipient suggestions use the history of sent mail: people you write to often and recently come first instead of namesakes in alphabetical order. Contacts are no longer created from messages automatically; the ones added by hand stay as they are.
- Due dates on the message flag and a separate task list: start date, due date, reminder and a completion mark. Setting the flag from the app window is fixed along the way - previously it could not be set at all. Due dates are stored on the computer only, the same way for every account.
- Pinning a message in the list: a pinned message stays on top regardless of its date. Counters of smart folders and tags keep counting as before.
- Quick steps: your own button with a name, an icon, a position in the panel and a shortcut that runs a chain of actions in one press.

### Fixed

- A tag on a message is visible in the list again. The colored badge used to go into the date column, where there is no spare room, and fell below the row border: the only way to see a tag was the context menu. Now the tag color is carried by a stripe along the left edge of the row, and the number of tags is shown by dots next to the sender; with several tags the stripe splits into segments, so all of them are visible. Tag names appear in a tooltip on the row and as chips in the header of the open message, where a tag can be removed right away.

## [0.2.19] - 2026-09-17

### Fixed

- Mail from Exchange mailboxes loads again. Since September 5 the app asked the server for three message properties under wrong names, and the server rejected the whole request - no mail arrived, and the app showed only a generic error. The names are fixed, and flagging a message works again too.
- The reason behind an Exchange refusal is no longer lost: the app reads the server answer and shows what exactly it replied and on which operation. Access denied and a protocol version mismatch are no longer passed off as an unavailable server.
- One and the same trouble on one mailbox is reported once instead of on every attempt: the repeat counter used to climb into the dozens while saying nothing new. The message comes back if the cause changes or the mailbox recovers.
- The mailbox card in settings no longer overlaps itself: buttons move to their own line when space runs out, a long address is trimmed, and the note about the password stands on its own.

### Changed

- The update button moved next to the app name, and hovering it opens the release notes right away: what is new, without installing and without opening a website.

## [0.2.18] - 2026-09-17

### Fixed

- Error messages now name the mailbox. With several mailboxes connected, two identical messages could appear in a row with no way to tell which one they were about.
- The app stopped being noisy. Dropped connections, timeouts and a busy server are things it recovers from by reconnecting, so they no longer pop up: such a failure shows in the mailbox state, and a message appears only after three failed passes in a row. Only what needs a decision shows up at once: password rejected, sign-in required, access denied, certificate not verified, account settings error.
- One cause across several mailboxes now shows as a single message listing them, instead of one message per mailbox.
- The button inside a message no longer disappears into the background: it shows a waiting state and then the result - success or a new reason for failure.
- Message details now show the server, the response code and the time of the attempt. The response code is exact: previously a port from the address could end up there instead of the code.
- The message about a deferred first sync appears only right after connecting a mailbox, not during a regular mail update.
- The diagnostics archive no longer keeps the user name from an email address: calendar and address book URLs store it in an encoded form that the anonymizer did not recognise.
- The mailbox card in settings no longer breaks on a long error text: the state takes its own line and does not push the buttons around. Folder and calendar counts agree with the number. Mailboxes that sign in with a token offer a fresh sign-in instead of a password change - no password is stored for them.

## [0.2.17] - 2026-09-16

### Added

- Settings now has a diagnostics button in the storage section. The app packs its own logs into a zip archive and opens the folder with it, so the file is easy to send to support. Before collecting, it shows what goes into the archive: email addresses, server names, paths, mailbox folder names and internal identifiers are replaced with pseudonyms; the same value gets the same pseudonym inside one archive and a different one in the next archive, so two archives cannot be linked together. Passwords, confirmation codes and sign-in keys never reach the archive.
- Every account now shows its mail state at all times: syncing, ready, retrying after a drop, error, or sign-in required. The time of the last successful update and the reason for a failure survive a restart, and the "sign-in required" state leads straight to reconnecting the account.
- The connection wizard shows progress: the button displays a waiting state and the current step instead of staying silent for the whole check. Exchange now has time limits for server discovery, password check and the attempt as a whole, so an unreachable server ends with a clear error instead of an endless wait.

### Changed

- Error messages now make sense. The app determines the kind of failure - password rejected, sign-in required, server unavailable, server rate limit, no network, timed out, certificate not verified - states it in plain words and offers the matching action: reconnect the account, retry, or wait. Technical details stay in the log and in expandable details. Identical messages no longer flicker one after another: repeats collapse into one with a counter, and a message carrying an action button does not disappear on its own.
- The macOS build ships as a single package for both architectures (Apple Silicon and Intel). Since 0.2.15 the package was built for Apple Silicon only, so Intel machines saw no update - now it arrives again.

### Fixed

- The setup wizard can be closed. When setup is already finished, the wizard closes with a button or the Escape key and returns to mail; on the very first run there is no exit, because nothing works without setup. Escape inside a wizard input field does not close the wizard - there it cancels the input.
- The outline around the selected message no longer stays after a mouse click. It appears only when moving through the list with the keyboard, like on buttons and fields; the selected message is still visible by its background and the stripe on the left.

## [0.2.16] - 2026-09-10

### Changed

- The application version is shown in the window status bar, at the right edge,
  instead of the sidebar. Clicking it still opens that release page on GitHub.

## [0.2.15] - 2026-09-10

### Fixed

- The version at the bottom of the sidebar is visible again. The number and the
  release address are now written into the interface at build time, so the label
  no longer depends on the core or on the bridge being ready: it used to stay
  silently empty when the core had not answered by the time the window loaded.
  The label sticks to the bottom edge of the sidebar and no longer scrolls out
  of sight behind a long folder list.

## [0.2.14] - 2026-09-10

### Fixed

- Clicking a message no longer gets lost. List rows are rebuilt as a whole, and
  with several mailboxes background sync refreshes the list every few seconds:
  when the refresh landed between pressing and releasing the button, the browser
  raised no click on the row and the message did not open, even though the row
  had already taken the focus ring. While the button is held on the list, the
  markup is not dropped and the window is not rebuilt.
- A header cut by the sender in the middle of a character now reads in full.
  Bulk senders split a long subject by bytes rather than character boundaries;
  each encoded word was decoded on its own, so the halves produced two
  replacement marks - "Ser??eevich" instead of "Sergeevich". Adjacent words of
  the same charset are now joined before parsing, and stored messages are
  re-read on start.

## [0.2.13] - 2026-09-09

### Added

- The installed version is shown at the bottom of the sidebar. Clicking it
  opens that release page on GitHub in the default browser.

### Fixed

- Only one popup menu stays on screen. Right-clicking a regular folder used to
  leave the previously opened menu next to it, and the "More" menu, filter,
  sort, smart folder icon picker and account color palette closed neither on
  Escape nor when another menu opened. Escape now closes menus first and only
  closes a modal window on the next press.
- Notifications are no longer raised for old messages. A message counted as new
  when its identifier was missing from the local database - which also happens
  when mail history is backfilled, a folder appears for the first time or a
  folder is re-fetched after a UIDVALIDITY change, so a years-old message could
  trigger a notification. Only messages whose date is within a day of the
  current moment are notified now; a message without a Date header still is.
- The core test run no longer crashes on process exit. Storage tests left their
  connection pools open, and their worker threads raced with the encryption
  library shutdown, so the build went red on green tests.

## [0.2.12] - 2026-09-05

### Added

- Unified lists and the message header now show which mailbox received the
  message. A message delivered to two connected mailboxes (for example, the
  original in Gmail and the copy Yandex collects from it over POP3) used to
  appear as two identical rows, and both headers showed the same address in
  "To", because the collecting server does not rewrite that header. With a
  single connected mailbox the label is not shown.
- Message bodies are fetched in the background after a sync, within the
  configured local retention. Gmail messages arrive without a body, so opening
  each message for the first time depended on the network, and the "keep mail
  locally" setting had no effect on Gmail. A single pass fetches at most 50
  messages, one request at a time; messages larger than 5 MB are still fetched
  on open.

### Fixed

- A dropped connection in the middle of a sync no longer wipes out the whole
  pass over a mailbox. One connection served the entire folder walk, so after a
  disconnect every remaining folder was skipped silently and the pass finished
  as a success with zero messages - up to a quarter of all passes on Yandex
  mailboxes according to the logs. The program now reconnects and repeats the
  interrupted work, and if no folder could be read at all, the sync reports an
  error.
- Switching the interface language no longer translates user data: a tag named
  "Настройки", a message subject "Календарь", a mailbox folder named
  "Контакты". The substitution matched text against an internal phrase
  dictionary, and the protection relied on listing sections by hand.
- Switching the language immediately redraws the mailbox folder tree, the
  cards in "Accounts", "Folder mapping" and "Unified folders", and the tag
  lists. Their labels used to stay in the previous language until the next data
  load.

### Changed

- The Exchange message list is built without downloading the full raw message:
  only the needed properties and the message text are requested for a list row,
  while the complete message with attachments is downloaded on open or by the
  background prefetch.
- Russian is the primary documentation language: `README.md`, `CHANGELOG.md`,
  `CLA.md`, `CONTRIBUTING.md`, `DONATE.md`, `SECURITY.md` and `LICENSING.md`
  hold the Russian text, and the English versions sit next to them with an
  `.en` suffix.

### Internal

- The rule that decides whether a message belongs to a smart folder is written
  once and used by both the list and the counter; it used to be duplicated, so
  editing one copy drifted from the other.
- The build checks that a changed interface file got a new version tag in its
  URL. Without the tag, an update would leave the user running the old copy of
  the file from the embedded browser cache.

## [0.2.11] - 2026-09-04

### Added

- A pasted screenshot or an image copied from a browser lands right in the
  message body at the cursor instead of silently becoming a file attachment.
  The image is sent as a proper inline part with a Content-ID inside
  multipart/related, so recipients see it in every mail client. The same now
  holds for forwarded messages and signatures with inline images: they used to
  arrive as a raw data string in the body and stayed invisible. A normal undo
  removes the pasted image, and the image survives the saved draft.
- Files dragged from Explorer onto the program window attach to the open
  message; when no message is open, one opens on its own and nothing already
  typed is lost. Dragging used to do nothing at all: file events never reached
  the interface. Bytes of images in the body count toward the same 25 MB
  per-message limit as attachments.

## [0.2.10] - 2026-09-03

### Fixed

- Messages in iso-2022-jp, shift_jis, big5, gbk and euc-jp/kr are readable:
  subject and body used to appear as a run of control characters. Exchange
  newsletters carrying Cyrillic inside iso-2022-jp now open correctly.
  Already downloaded messages repair themselves after the update: subject,
  sender, preview, attachment names, address book names and full-text search
  are restored from the stored source, with no re-download from the server.
  The repair runs in the background and does not delay startup.
- Folder names are readable in sync error messages: instead of "Spam" the text
  used to show a raw IMAP string like "&BCEEPwQwBDw-".

## [0.2.9] - 2026-09-02

### Added

- Account cards in settings collapse: only one stays expanded, so the section no
  longer requires long scrolling with several mailboxes. The state is remembered
  between runs.
- Password mailboxes get a "Change password" button. It verifies the new
  password against the server and changes only that: the account name and
  settings stay, and mail is not downloaded again. The previous password is kept
  if the new one does not work.

### Changed

- The "Unified folders" section shows plain folder names with their nesting
  instead of encoded strings and long identifiers.
- Recipient suggestions, quick search and the contacts section understand
  transliteration: typing "коннова" finds "Valentina Konnova" and the other way
  round.
- The "To" and "Cc" lines in the message header show the address next to the
  name, just like the "From" line.

### Fixed

- Clicking a message always shows the message you picked. Previously, if the
  earlier message took longer to load, its content was drawn over the selected
  one.
- Focus stays on the list row after a click, so keys keep working with the list.
  List rows are now reachable from the keyboard and exposed to screen readers.
- The keys for the next and previous message work after a click inside the
  message body as well.
- The list no longer shifts under the pointer when new mail arrives during work:
  the position is restored by message instead of by pixel count.
- Shift selection takes the range you actually see, even if the list was rebuilt
  in the meantime.


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
