**English** · [Русский](README.ru.md)

<p align="center">
  <img src="assets/brand/truemail-logo.svg" alt="truemail" width="380">
</p>

<p align="center">
  A fast, beautiful, cross-platform open-source mail client written in Rust.
</p>

---

A standalone desktop application built on IMAP/SMTP/MIME, iCalendar, vCard,
CalDAV and CardDAV. The current working provider is Yandex; other providers and
the external automation API are roadmap items. Local data is encrypted.

## Development

```sh
make setup     # install tauri-cli and sqlx-cli (one-time)
make dev       # run the desktop application (Tauri v2)
```

SQLCipher database migrations are applied automatically on startup. On
Windows, the first SQLCipher build downloads a verified portable Strawberry
Perl build into `temp/` if a full Perl installation is not on `PATH`.

After `make dev` stops, cargo-sweep removes only build artifacts that have
been unused for 30 days; the active build cache is preserved. Preview the
list with `make sweep-preview`.

### Yandex OAuth

Create a Yandex OAuth application of type `Web services`, callback URL
`https://oauth.yandex.ru/verification_code`, and scopes `mail:imap_full`,
`mail:smtp`, `calendar:all`, `directory:read_external_contacts`,
`directory:write_external_contacts`. Set the public OAuth `client_id` when
building or running a development copy:

```powershell
$env:TRUEMAIL_YANDEX_CLIENT_ID="your_application_id"
make dev
```

Or copy `.env.example` to `.env`, paste the public `client_id`, and run
`make dev`. The Makefile loads `.env` before building Tauri. `.env` is not
tracked by Git. The desktop app does not need a `client_secret`: OAuth uses
Authorization Code + PKCE.

No app secret is embedded in the desktop client: authorization uses
Authorization Code with PKCE. OAuth tokens are stored in the system keychain,
and IMAP, CalDAV, and CardDAV are verified immediately on first connection.

### Local storage

In the first-run wizard the user picks a language, a data folder, and
generates keys by moving the mouse. The persistent SQLCipher and blob-store
keys are derived from that input combined with the OS CSPRNG through HKDF,
and are stored in the keychain. SQLCipher encrypts the entire SQLite
database, including metadata, FTS, and WAL; ChaCha20-Poly1305 separately
encrypts the blobs.

## Structure

```
crates/core/            core: RFC models, transport, storage, search, crypto, API
  migrations/           database schema (sqlx migrations)
  src/model/             canonical model (message, event, contact, account, folder)
  src/backend/            MailBackend trait + Yandex IMAP/SMTP adapter
  src/storage/            SQLCipher + encrypted blob store
  src/crypto/             storage encryption (keys in the keychain)
  src/search/              FTS5 search + layout-independent matching
  src/account/             account manager + autoconfiguration
  src/api/                 capability model for the planned external API
  src/i18n/                 localization (Fluent)
apps/desktop/            desktop application (Tauri v2)
  src-tauri/               application backend (commands -> core)
  ui/                      frontend (index.html + styles.css + app.js), per the mockups
locales/                 translations: ru.ftl / en.ftl
```

## Highlights

- Standalone: local storage, full at-rest encryption, secrets in the keychain.
- Instant Yandex mail delivery over IMAP IDLE with incremental catch-up.
- Yandex calendars and contacts over CalDAV/CardDAV.
- Simple / Expert modes; RU+EN localization; live dark and light themes.
- Real SMTP sending, encrypted drafts, attachments and scheduled outbox delivery.
- Provider-neutral `MailBackend` boundary for future adapters; Yandex is implemented now.

## License

Dual licensing: [AGPL-3.0](LICENSE) (open) plus a commercial license for those
who do not want to open their own code. See [LICENSING.md](LICENSING.md) for
details. For commercial inquiries: bintocher@yandex.ru.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Contributions are accepted under
[CLA.md](CLA.md).

## Security

See [SECURITY.md](SECURITY.md) for how to report vulnerabilities.

## Support

The project is free and open source. [Support development](DONATE.md).
