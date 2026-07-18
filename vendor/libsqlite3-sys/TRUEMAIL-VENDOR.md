# TrueMail SQLCipher vendor provenance

This directory is `libsqlite3-sys` 0.37.0 with its bundled SQLCipher
amalgamation updated to SQLCipher 4.17.0 / SQLite 3.53.3.

The 0.37.0 package version is intentionally retained because SQLx 0.9 declares
`libsqlite3-sys >=0.30.1, <0.38.0`. The Rust API and build integration remain
from 0.37.0; only these generated SQLCipher files were replaced:

- `sqlcipher/sqlite3.c`
- `sqlcipher/sqlite3.h`
- `sqlcipher/sqlite3ext.h`
- `sqlcipher/bindgen_bundled_version.rs`

Source commit:
`https://github.com/rusqlite/rusqlite/commit/62648175c23f84b45238f4a1fbb0133b75ce68f1`
(`Bump bundled SQLCipher to version 4.17.0`). The amalgamation in that commit
was generated from the official SQLCipher `v4.17.0` release.

SHA-256 checksums:

```text
8ADAFF6B464052A74E7ADAA3CFA2725400F48ECA68F47856FA806EAF30BDF2C9  sqlcipher/sqlite3.c
E564D0492E7556A8AD2F30C8EC645B5A6ABB89F32F7B40465A3032D937596401  sqlcipher/sqlite3.h
AC9645E5C9FF0CF176EFDD6E75CB5E98F46295D38E02DB5C4D208826A39AB4BE  sqlcipher/sqlite3ext.h
88C81786FC0D3A258407641F8713311FB9D165191FF0507DB9CF18ACA49AFBDC  sqlcipher/bindgen_bundled_version.rs
```

The build script fails before compilation if the vendored SQLite or SQLCipher
version marker differs from the required versions. The linked runtime can be
verified with:

```powershell
cargo test --manifest-path vendor/libsqlite3-sys/Cargo.toml `
  --target-dir target --features bundled-sqlcipher-vendored-openssl `
  --test bundled_versions
```
