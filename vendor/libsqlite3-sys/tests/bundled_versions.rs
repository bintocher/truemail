#![cfg(feature = "bundled-sqlcipher")]

use std::ffi::{CStr, CString};
use std::ptr;

use libsqlite3_sys as ffi;

#[test]
fn bundled_runtime_versions_are_exact() {
    unsafe {
        let sqlite_version = CStr::from_ptr(ffi::sqlite3_libversion())
            .to_str()
            .expect("SQLite version is not UTF-8");
        assert_eq!(sqlite_version, "3.53.3");

        let mut db = ptr::null_mut();
        let memory = CString::new(":memory:").unwrap();
        assert_eq!(ffi::sqlite3_open(memory.as_ptr(), &mut db), ffi::SQLITE_OK);

        let mut statement = ptr::null_mut();
        let pragma = CString::new("PRAGMA cipher_version").unwrap();
        assert_eq!(
            ffi::sqlite3_prepare_v2(db, pragma.as_ptr(), -1, &mut statement, ptr::null_mut()),
            ffi::SQLITE_OK
        );
        assert_eq!(ffi::sqlite3_step(statement), ffi::SQLITE_ROW);

        let value = ffi::sqlite3_column_text(statement, 0);
        assert!(!value.is_null(), "PRAGMA cipher_version returned NULL");
        let sqlcipher_version = CStr::from_ptr(value.cast())
            .to_str()
            .expect("SQLCipher version is not UTF-8");
        assert_eq!(sqlcipher_version, "4.17.0 community");

        assert_eq!(ffi::sqlite3_finalize(statement), ffi::SQLITE_OK);
        assert_eq!(ffi::sqlite3_close(db), ffi::SQLITE_OK);
    }
}
