**English** · [Русский](CHANGELOG.ru.md)

# Changelog

All notable changes are documented here. The format follows Keep a Changelog;
versions use Semantic Versioning.

## [Unreleased]

### Added

- SQLCipher storage, encrypted blob store and system-keychain integration.
- Yandex OAuth/IMAP/CalDAV/CardDAV synchronization with IMAP IDLE.
- Desktop onboarding, mail, calendar, contacts, search and settings UI.

### Security

- IMAP downloads use `BODY.PEEK[]` and never mark messages read implicitly.
- Blob references are random and bound to XChaCha20-Poly1305 ciphertext via AAD.
- Installation keys combine OS CSPRNG and user input through HKDF.
