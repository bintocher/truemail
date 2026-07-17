//! Шифрование локального хранилища (at-rest).
//!
//! SQLCipher шифрует всю SQLite-базу, а XChaCha20-Poly1305 — отдельные
//! блобы. Оба постоянных ключа выводятся только из движений мыши в
//! визарде первого запуска, системного CSPRNG и хранятся в keychain.
//!
//! Argon2id используется только для переносимого парольного backup ключей;
//! рабочие ключи установки остаются в системном keychain.

use crate::{Error, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce, XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::path::Path;
use zeroize::Zeroize;

const KEYCHAIN_SERVICE: &str = "truemail";
const KEYCHAIN_KEY: &str = "storage-key";
const DATABASE_KEYCHAIN_KEY: &str = "database-key";
const DATA_DIR_KEYCHAIN_KEY: &str = "data-dir";
const LEGACY_NONCE_LEN: usize = 12;
const NONCE_LEN: usize = 24;
const ENCRYPTION_V2_HEADER: &[u8] = b"TMXCHACHA2\0";
const MIN_ENTROPY_BYTES: usize = 4 * 1024;
const BACKUP_AAD: &[u8] = b"truemail/key-backup/v1";
const BACKUP_MAGIC: &[u8] = b"TMKEYS1\0";
const BACKUP_SALT_LEN: usize = 16;
const BACKUP_MEMORY_KIB: u32 = 64 * 1024;
const BACKUP_ITERATIONS: u32 = 3;
const BACKUP_PARALLELISM: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct KeyBackup {
    format: String,
    version: u32,
    kdf: BackupKdf,
    salt: String,
    nonce: String,
    ciphertext: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupKdf {
    algorithm: String,
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
}

struct BackupKeys {
    storage: [u8; 32],
    database: [u8; 32],
}

struct OptionalBackupKeys {
    storage: Option<[u8; 32]>,
    database: Option<[u8; 32]>,
}

impl Drop for BackupKeys {
    fn drop(&mut self) {
        self.storage.zeroize();
        self.database.zeroize();
    }
}

impl Drop for OptionalBackupKeys {
    fn drop(&mut self) {
        if let Some(key) = &mut self.storage {
            key.zeroize();
        }
        if let Some(key) = &mut self.database {
            key.zeroize();
        }
    }
}

pub struct StorageCrypto {
    cipher: XChaCha20Poly1305,
    legacy_cipher: ChaCha20Poly1305,
}

/// Отдельный 256-битный ключ SQLCipher. В SQLite и файлах приложения не
/// сохраняется: источник истины — системный keychain текущего пользователя.
pub struct DatabaseKey([u8; 32]);

impl Drop for DatabaseKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl DatabaseKey {
    pub fn open() -> Result<Self> {
        load_key(DATABASE_KEYCHAIN_KEY)?
            .map(Self)
            .ok_or_else(|| Error::Crypto("ключ SQLCipher ещё не создан".into()))
    }

    #[cfg(test)]
    pub(crate) fn from_key(key: [u8; 32]) -> Self {
        Self(key)
    }

    /// SQLCipher raw-key literal: 32 байта уже случайны, дополнительный PBKDF
    /// для пользовательской парольной фразы здесь не требуется.
    pub(crate) fn pragma_value(&self) -> String {
        // SQLCipher raw-key syntax intentionally has two quote layers:
        // PRAGMA key = "x'<64 hex digits>'".
        let mut value = String::with_capacity(69);
        value.push_str("\"x'");
        for byte in self.0 {
            write!(&mut value, "{byte:02X}").expect("write to String");
        }
        value.push_str("'\"");
        value
    }
}

impl StorageCrypto {
    /// Открыть шифрование с уже созданным в визарде ключом.
    pub fn open(_data_dir: &Path) -> Result<Self> {
        let key_bytes = load_key(KEYCHAIN_KEY)?
            .ok_or_else(|| Error::Crypto("ключ blob-store ещё не создан".into()))?;
        Ok(Self::from_key(key_bytes))
    }

    pub(crate) fn from_key(mut key_bytes: [u8; 32]) -> Self {
        let cipher = XChaCha20Poly1305::new_from_slice(&key_bytes)
            .expect("XChaCha20-Poly1305 accepts a 256-bit key");
        let legacy_cipher = ChaCha20Poly1305::new_from_slice(&key_bytes)
            .expect("ChaCha20-Poly1305 accepts a 256-bit key");
        key_bytes.zeroize();
        Self {
            cipher,
            legacy_cipher,
        }
    }

    /// Зашифровать произвольные данные без контекстной привязки.
    pub fn encrypt(&self, plain: &[u8]) -> Result<Vec<u8>> {
        self.encrypt_with_aad(plain, &[])
    }

    /// Зашифровать с AAD, связывающим ciphertext с записью/ссылкой владельца.
    pub fn encrypt_with_aad(&self, plain: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::rng().fill_bytes(&mut nonce_bytes);
        let nonce = XNonce::from(nonce_bytes);
        let mut out = Vec::with_capacity(ENCRYPTION_V2_HEADER.len() + NONCE_LEN + plain.len() + 16);
        out.extend_from_slice(ENCRYPTION_V2_HEADER);
        out.extend_from_slice(&nonce_bytes);
        let ct = self
            .cipher
            .encrypt(&nonce, Payload { msg: plain, aad })
            .map_err(|e| Error::Crypto(e.to_string()))?;
        out.extend_from_slice(&ct);
        Ok(out)
    }

    /// Расшифровать без контекстной привязки.
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        self.decrypt_with_aad(data, &[])
    }

    /// Расшифровать v2 с проверкой AAD. Старый nonce(12)||ciphertext читается
    /// только для бесшовной миграции уже существующих локальных данных.
    pub fn decrypt_with_aad(&self, data: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
        if let Some(versioned) = data.strip_prefix(ENCRYPTION_V2_HEADER) {
            if versioned.len() < NONCE_LEN {
                return Err(Error::Crypto("слишком короткие данные".into()));
            }
            let (nonce_bytes, ct) = versioned.split_at(NONCE_LEN);
            let nonce_bytes: [u8; NONCE_LEN] = nonce_bytes
                .try_into()
                .map_err(|_| Error::Crypto("некорректный XChaCha nonce".into()))?;
            let nonce = XNonce::from(nonce_bytes);
            return self
                .cipher
                .decrypt(&nonce, Payload { msg: ct, aad })
                .map_err(|e| Error::Crypto(e.to_string()));
        }
        if data.len() < LEGACY_NONCE_LEN {
            return Err(Error::Crypto("слишком короткие данные".into()));
        }
        let (nonce_bytes, ct) = data.split_at(LEGACY_NONCE_LEN);
        let nonce_bytes: [u8; LEGACY_NONCE_LEN] = nonce_bytes
            .try_into()
            .map_err(|_| Error::Crypto("некорректный nonce".into()))?;
        let nonce = Nonce::from(nonce_bytes);
        self.legacy_cipher
            .decrypt(&nonce, ct)
            .map_err(|e| Error::Crypto(e.to_string()))
    }
}

/// Создать ключи из независимых источников: системного CSPRNG и движений
/// пользователя. HKDF разводит итог на отдельные домены blob-store/SQLCipher.
pub fn initialize_keys_from_entropy(entropy: &[u8]) -> Result<()> {
    if entropy.len() < MIN_ENTROPY_BYTES {
        return Err(Error::Crypto(format!(
            "недостаточно данных движений мыши: нужно не менее {MIN_ENTROPY_BYTES} байт"
        )));
    }
    if keys_initialized()? {
        return Err(Error::Crypto("ключи этой установки уже созданы".into()));
    }

    let mut os_random = [0_u8; 32];
    rand::rng().fill_bytes(&mut os_random);
    let mut user_digest: [u8; 32] = Sha256::digest(entropy).into();
    let mut combined = [0_u8; 32];
    for index in 0..combined.len() {
        combined[index] = os_random[index] ^ user_digest[index];
    }
    let hkdf = Hkdf::<Sha256>::new(Some(b"truemail/install-keys/v2"), &combined);
    let mut storage_key = [0_u8; 32];
    let mut database_key = [0_u8; 32];
    hkdf.expand(b"blob-store", &mut storage_key)
        .map_err(|_| Error::Crypto("HKDF: неверная длина blob-ключа".into()))?;
    hkdf.expand(b"sqlcipher", &mut database_key)
        .map_err(|_| Error::Crypto("HKDF: неверная длина ключа БД".into()))?;
    os_random.zeroize();
    user_digest.zeroize();
    combined.zeroize();
    store_key(KEYCHAIN_KEY, &storage_key)?;
    if let Err(error) = store_key(DATABASE_KEYCHAIN_KEY, &database_key) {
        storage_key.zeroize();
        database_key.zeroize();
        let _ = delete_key(KEYCHAIN_KEY);
        return Err(error);
    }
    storage_key.zeroize();
    database_key.zeroize();
    Ok(())
}

fn backup_params(memory_kib: u32, iterations: u32, parallelism: u32) -> Result<Params> {
    if !(8 * 1024..=256 * 1024).contains(&memory_kib)
        || !(1..=10).contains(&iterations)
        || !(1..=4).contains(&parallelism)
    {
        return Err(Error::Crypto(
            "параметры Argon2id в резервной копии вне безопасных границ".into(),
        ));
    }
    Params::new(memory_kib, iterations, parallelism, Some(32))
        .map_err(|error| Error::Crypto(format!("Argon2id: {error}")))
}

fn derive_backup_key(
    password: &str,
    salt: &[u8],
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
) -> Result<[u8; 32]> {
    if password.is_empty() {
        return Err(Error::Crypto("пароль резервной копии пуст".into()));
    }
    let params = backup_params(memory_kib, iterations, parallelism)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0_u8; 32];
    argon
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|error| Error::Crypto(format!("Argon2id: {error}")))?;
    Ok(key)
}

fn seal_backup(
    storage_key: &[u8; 32],
    database_key: &[u8; 32],
    password: &str,
    memory_kib: u32,
    iterations: u32,
) -> Result<String> {
    let mut salt = [0_u8; BACKUP_SALT_LEN];
    let mut nonce_bytes = [0_u8; NONCE_LEN];
    rand::rng().fill_bytes(&mut salt);
    rand::rng().fill_bytes(&mut nonce_bytes);
    let mut wrapping_key =
        derive_backup_key(password, &salt, memory_kib, iterations, BACKUP_PARALLELISM)?;
    let cipher = XChaCha20Poly1305::new_from_slice(&wrapping_key)
        .map_err(|error| Error::Crypto(error.to_string()))?;
    let mut plaintext = Vec::with_capacity(BACKUP_MAGIC.len() + 64);
    plaintext.extend_from_slice(BACKUP_MAGIC);
    plaintext.extend_from_slice(storage_key);
    plaintext.extend_from_slice(database_key);
    let encrypted = cipher.encrypt(
        &XNonce::from(nonce_bytes),
        Payload {
            msg: &plaintext,
            aad: BACKUP_AAD,
        },
    );
    plaintext.zeroize();
    wrapping_key.zeroize();
    let ciphertext = encrypted
        .map_err(|error| Error::Crypto(format!("резервная копия не зашифрована: {error}")))?;
    let base64 = base64::engine::general_purpose::STANDARD_NO_PAD;
    serde_json::to_string_pretty(&KeyBackup {
        format: "truemail-key-backup".into(),
        version: 1,
        kdf: BackupKdf {
            algorithm: "argon2id".into(),
            memory_kib,
            iterations,
            parallelism: BACKUP_PARALLELISM,
        },
        salt: base64.encode(salt),
        nonce: base64.encode(nonce_bytes),
        ciphertext: base64.encode(ciphertext),
    })
    .map_err(Into::into)
}

fn open_backup(serialized: &str, password: &str) -> Result<BackupKeys> {
    let backup: KeyBackup = serde_json::from_str(serialized)
        .map_err(|error| Error::Crypto(format!("файл резервной копии повреждён: {error}")))?;
    if backup.format != "truemail-key-backup"
        || backup.version != 1
        || backup.kdf.algorithm != "argon2id"
    {
        return Err(Error::Crypto(
            "неподдерживаемый формат резервной копии ключей".into(),
        ));
    }
    let base64 = base64::engine::general_purpose::STANDARD_NO_PAD;
    let salt = base64
        .decode(backup.salt)
        .map_err(|error| Error::Crypto(format!("некорректная salt: {error}")))?;
    if salt.len() != BACKUP_SALT_LEN {
        return Err(Error::Crypto("некорректная длина salt".into()));
    }
    let nonce: [u8; NONCE_LEN] = base64
        .decode(backup.nonce)
        .map_err(|error| Error::Crypto(format!("некорректный nonce: {error}")))?
        .try_into()
        .map_err(|_| Error::Crypto("некорректная длина nonce".into()))?;
    let ciphertext = base64
        .decode(backup.ciphertext)
        .map_err(|error| Error::Crypto(format!("некорректный ciphertext: {error}")))?;
    let mut wrapping_key = derive_backup_key(
        password,
        &salt,
        backup.kdf.memory_kib,
        backup.kdf.iterations,
        backup.kdf.parallelism,
    )?;
    let cipher = XChaCha20Poly1305::new_from_slice(&wrapping_key)
        .map_err(|error| Error::Crypto(error.to_string()))?;
    let decrypted = cipher.decrypt(
        &XNonce::from(nonce),
        Payload {
            msg: &ciphertext,
            aad: BACKUP_AAD,
        },
    );
    wrapping_key.zeroize();
    let mut plaintext = decrypted
        .map_err(|_| Error::Crypto("неверный пароль или повреждённый backup ключей".into()))?;
    if plaintext.len() != BACKUP_MAGIC.len() + 64 || !plaintext.starts_with(BACKUP_MAGIC) {
        plaintext.zeroize();
        return Err(Error::Crypto(
            "backup ключей имеет неверное содержимое".into(),
        ));
    }
    let mut keys = BackupKeys {
        storage: [0_u8; 32],
        database: [0_u8; 32],
    };
    keys.storage
        .copy_from_slice(&plaintext[BACKUP_MAGIC.len()..BACKUP_MAGIC.len() + 32]);
    keys.database
        .copy_from_slice(&plaintext[BACKUP_MAGIC.len() + 32..]);
    plaintext.zeroize();
    Ok(keys)
}

/// Export both installation keys as a password-encrypted, portable JSON file.
/// The returned string never contains plaintext keys.
pub fn export_key_backup(password: &str) -> Result<String> {
    if password.chars().count() < 12 {
        return Err(Error::Crypto(
            "пароль резервной копии должен содержать не менее 12 символов".into(),
        ));
    }
    let mut keys = BackupKeys {
        storage: load_key(KEYCHAIN_KEY)?
            .ok_or_else(|| Error::Crypto("ключ blob-store ещё не создан".into()))?,
        database: [0_u8; 32],
    };
    keys.database = load_key(DATABASE_KEYCHAIN_KEY)?
        .ok_or_else(|| Error::Crypto("ключ SQLCipher ещё не создан".into()))?;
    let result = seal_backup(
        &keys.storage,
        &keys.database,
        password,
        BACKUP_MEMORY_KIB,
        BACKUP_ITERATIONS,
    );
    result
}

/// Restore keys into an empty system credential store. Existing installation
/// keys are never overwritten: doing so while a different archive is open
/// would make that archive inaccessible after restart.
pub fn restore_key_backup(serialized: &str, password: &str) -> Result<()> {
    let keys = open_backup(serialized, password)?;
    let mut existing = OptionalBackupKeys {
        storage: load_key(KEYCHAIN_KEY)?,
        database: None,
    };
    existing.database = load_key(DATABASE_KEYCHAIN_KEY)?;
    if existing.storage.is_some() || existing.database.is_some() {
        return Err(Error::Crypto(
            "в системном хранилище уже есть ключи truemail; восстановление отменено".into(),
        ));
    }
    store_key(KEYCHAIN_KEY, &keys.storage)?;
    if let Err(error) = store_key(DATABASE_KEYCHAIN_KEY, &keys.database) {
        let _ = delete_key(KEYCHAIN_KEY);
        return Err(error);
    }
    Ok(())
}

pub fn keys_initialized() -> Result<bool> {
    match (load_key(KEYCHAIN_KEY)?, load_key(DATABASE_KEYCHAIN_KEY)?) {
        (Some(_), Some(_)) => Ok(true),
        (None, None) => Ok(false),
        _ => Err(Error::Keyring(
            "найден только один из двух ключей truemail".into(),
        )),
    }
}

pub fn remove_installation_keys() -> Result<()> {
    delete_key(KEYCHAIN_KEY)?;
    delete_key(DATABASE_KEYCHAIN_KEY)?;
    delete_credential(DATA_DIR_KEYCHAIN_KEY)?;
    Ok(())
}

pub fn store_data_dir(path: &Path) -> Result<()> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, DATA_DIR_KEYCHAIN_KEY)
        .map_err(|e| Error::Keyring(e.to_string()))?;
    entry
        .set_password(&path.to_string_lossy())
        .map_err(|e| Error::Keyring(e.to_string()))
}

pub fn load_data_dir() -> Result<Option<std::path::PathBuf>> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, DATA_DIR_KEYCHAIN_KEY)
        .map_err(|e| Error::Keyring(e.to_string()))?;
    match entry.get_password() {
        Ok(path) if !path.trim().is_empty() => Ok(Some(path.into())),
        Ok(_) | Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(Error::Keyring(error.to_string())),
    }
}

fn load_key(key_name: &str) -> Result<Option<[u8; 32]>> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, key_name)
        .map_err(|e| Error::Keyring(e.to_string()))?;

    match entry.get_secret() {
        Ok(mut bytes) if bytes.len() == 32 => {
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            bytes.zeroize();
            Ok(Some(key))
        }
        Ok(mut bytes) => {
            bytes.zeroize();
            Err(Error::Keyring(format!(
                "повреждён ключ {key_name}: неверная длина"
            )))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(Error::Keyring(error.to_string())),
    }
}

fn store_key(key_name: &str, key: &[u8; 32]) -> Result<()> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, key_name)
        .map_err(|e| Error::Keyring(e.to_string()))?;
    entry
        .set_secret(key)
        .map_err(|e| Error::Keyring(e.to_string()))
}

fn delete_key(key_name: &str) -> Result<()> {
    delete_credential(key_name)
}

fn delete_credential(key_name: &str) -> Result<()> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, key_name)
        .map_err(|e| Error::Keyring(e.to_string()))?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(Error::Keyring(error.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_backup_round_trips_both_keys() {
        let storage = [0x11_u8; 32];
        let database = [0x22_u8; 32];
        let serialized = seal_backup(&storage, &database, "correct horse battery", 8 * 1024, 1)
            .expect("seal backup");
        assert!(serialized.contains("argon2id"));
        let restored = open_backup(&serialized, "correct horse battery").expect("open backup");
        assert_eq!(restored.storage, storage);
        assert_eq!(restored.database, database);
    }

    #[test]
    fn password_backup_rejects_wrong_password() {
        let serialized = seal_backup(
            &[0x11_u8; 32],
            &[0x22_u8; 32],
            "correct horse battery",
            8 * 1024,
            1,
        )
        .expect("seal backup");
        let error = open_backup(&serialized, "wrong horse battery")
            .err()
            .expect("wrong password must fail");
        assert!(error.to_string().contains("неверный пароль"));
    }
}
