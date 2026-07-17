**English** · [Русский](README.ru.md)

<p align="center">
  <img src="assets/brand/truemail-logo.svg" alt="truemail" width="380">
</p>

<p align="center">
  An open-source desktop mail client written in Rust.
</p>

---

The program runs on your own computer: mail, calendars and contacts are stored
locally in an encrypted database. Yandex and Gmail are supported.

## What it does

Mail:

- Connects Yandex and Gmail over OAuth, without entering your mailbox password.
- Receives mail over IMAP; new messages arrive immediately, without waiting for a poll.
- Sends over SMTP, with drafts, attachments and scheduled sending.
- Sending queue: with no network, a message goes out on the next connection.
- Groups a thread into a conversation.
- Smart folders by conditions, folders spanning every mailbox, processing rules, labels.
- Search across mail, including text typed in the wrong keyboard layout.

Calendars and contacts:

- Yandex: calendars and contacts over CalDAV and CardDAV.
- Gmail: calendars, contacts and tasks through Google services.
- Meeting reminders.

Interface:

- Russian and English.
- Light and dark themes, accent colour, three list densities.
- Two modes: normal and expert, the latter with additional settings.
- Built-in new-mail notifications, a system tray icon, start on system startup.

## Build and run

```sh
make setup     # install tauri-cli and sqlx-cli (once)
make dev       # run the program
```

The database schema updates automatically on startup. On Windows the first build
downloads a verified portable Strawberry Perl into `temp/` if a full Perl is not
on `PATH` — it is only needed while building.

After `make dev` stops, only build files unused for 30 days are removed. To see
the list beforehand: `make sweep-preview`.

## Connecting mail

A released build connects mailboxes on its own. If you build from source, you
need to register your own application with Yandex and Google and provide the
identifiers they issue: the repository does not contain them.

Copy `.env.example` to `.env` and fill in the values. `.env` stays out of Git,
and `make dev` reads it while building.

```dotenv
TRUEMAIL_YANDEX_CLIENT_ID=your_yandex_application_id
TRUEMAIL_YANDEX_REDIRECT_URI=http://127.0.0.1:34982/oauth/yandex/callback
TRUEMAIL_GOOGLE_CLIENT_ID=your_google_application_id
TRUEMAIL_GOOGLE_CLIENT_SECRET=the_string_google_issues
```

Yandex needs no application password. Google issues one even for programs on a
computer and requires it when connecting, so it is listed here.

Yandex: an application of type `Web services`, exact callback URL
`http://127.0.0.1:34982/oauth/yandex/callback`, permissions `mail:imap_full`,
`mail:smtp`, `calendar:all`, `directory:read_external_contacts`,
`directory:write_external_contacts`.

Google: a project in [Google Cloud Console](https://console.cloud.google.com/),
with `Gmail API` enabled, access permission `https://mail.google.com/` and an
application of type `Desktop app`. No callback URL is needed: the program
receives the answer on a temporary `http://127.0.0.1` address on a random port.

## How data is stored

On first run you choose a language, review the setup process, choose a folder for the data, and create the
encryption keys by moving the mouse. Those random movements are mixed with
random numbers from the operating system, so the key cannot be predicted. Keys
are kept in the system password store.

The whole database is encrypted, including internal data and the search index.
Message texts and attachments are encrypted separately. The program neither
stores nor sees mailbox passwords: access is granted over OAuth.

## Structure

```
crates/core/            core: models, transport, storage, search, encryption
  migrations/           database schema
  src/model/              shared model (message, event, contact, account, folder)
  src/backend/             IMAP and SMTP for Yandex and Gmail
  src/storage/             encrypted database and attachment store
  src/crypto/              data encryption, keys in the system store
  src/search/               search aware of the keyboard layout
  src/account/              accounts and automatic setup
  src/i18n/                  translations
apps/desktop/            desktop program
  src-tauri/                link between the interface and the core
  ui/                       interface, modules and RU/EN JSON catalogs
```

## License

Dual licensing: [AGPL-3.0](LICENSE) (open) plus a commercial license for those
who do not want to open their own code. See [LICENSING.md](LICENSING.md) for
details. For commercial inquiries: bintocher@yandex.com.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Contributions are accepted under
[CLA.md](CLA.md).

## Security

See [SECURITY.md](SECURITY.md) for how to report vulnerabilities.

## Support

The project is free and open source. [Support development](DONATE.md).
