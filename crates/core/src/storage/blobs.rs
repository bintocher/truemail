//! Зашифрованный blob-store для тел писем, вложений и оригиналов vCard/iCalendar.
//! Файлы получают случайные непрогнозируемые имена; содержимое зашифровано,
//! а ссылка криптографически привязана к ciphertext через AEAD AAD.

use crate::Result;
use crate::crypto::StorageCrypto;
use rand::Rng as _;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct BlobStore {
    root: PathBuf,
    crypto: Arc<StorageCrypto>,
}

impl BlobStore {
    pub fn new(root: PathBuf, crypto: Arc<StorageCrypto>) -> Result<Self> {
        std::fs::create_dir_all(&root)?;
        Ok(Self { root, crypto })
    }

    /// Сохранить данные, вернуть случайную относительную ссылку.
    pub fn put(&self, data: &[u8]) -> Result<String> {
        loop {
            let reference = random_reference();
            let (dir, file) = reference.split_once('/').expect("valid blob reference");
            let dir_path = self.root.join(dir);
            std::fs::create_dir_all(&dir_path)?;
            let encrypted = self.crypto.encrypt_with_aad(data, reference.as_bytes())?;
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(dir_path.join(file))
            {
                Ok(mut target) => {
                    use std::io::Write as _;
                    target.write_all(&encrypted)?;
                    target.sync_data()?;
                    return Ok(reference);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
    }

    /// Прочитать и расшифровать данные по ссылке.
    pub fn get(&self, reference: &str) -> Result<Vec<u8>> {
        let enc = std::fs::read(self.root.join(reference))?;
        self.crypto.decrypt_with_aad(&enc, reference.as_bytes())
    }

    /// Удалить blob.
    pub fn remove(&self, reference: &str) -> Result<()> {
        std::fs::remove_file(self.root.join(reference)).ok();
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        if self.root.exists() {
            std::fs::remove_dir_all(&self.root)?;
        }
        std::fs::create_dir_all(&self.root)?;
        Ok(())
    }

    pub fn references(&self) -> Result<Vec<String>> {
        let mut references = Vec::new();
        if !self.root.exists() {
            return Ok(references);
        }
        for directory in std::fs::read_dir(&self.root)? {
            let directory = directory?;
            if !directory.file_type()?.is_dir() {
                continue;
            }
            let prefix = directory.file_name().to_string_lossy().into_owned();
            for file in std::fs::read_dir(directory.path())? {
                let file = file?;
                if file.file_type()?.is_file() {
                    references.push(format!("{prefix}/{}", file.file_name().to_string_lossy()));
                }
            }
        }
        Ok(references)
    }

    pub fn exists(&self, reference: &str) -> bool {
        self.root.join(reference).is_file()
    }
}

fn random_reference() -> String {
    let mut random = [0_u8; 24];
    rand::rng().fill_bytes(&mut random);
    let mut encoded = String::with_capacity(48);
    use std::fmt::Write as _;
    for byte in random {
        write!(&mut encoded, "{byte:02x}").expect("write to String");
    }
    format!("{}/{}", &encoded[..2], &encoded[2..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> (BlobStore, PathBuf) {
        let root = std::env::temp_dir().join(format!("truemail-blobs-{}", uuid::Uuid::new_v4()));
        let crypto = Arc::new(StorageCrypto::from_key([7_u8; 32]));
        (
            BlobStore::new(root.clone(), crypto).expect("blob store"),
            root,
        )
    }

    #[test]
    fn equal_plaintexts_get_unrelated_names_and_round_trip() {
        let (store, root) = test_store();
        let first = store.put(b"same message").expect("first blob");
        let second = store.put(b"same message").expect("second blob");
        assert_ne!(first, second);
        assert_eq!(store.get(&first).expect("read first"), b"same message");
        assert_eq!(store.get(&second).expect("read second"), b"same message");
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn ciphertext_cannot_be_moved_to_another_reference() {
        let (store, root) = test_store();
        let first = store.put(b"first").expect("first blob");
        let second = store.put(b"second").expect("second blob");
        std::fs::copy(root.join(&first), root.join(&second)).expect("replace ciphertext");
        assert!(store.get(&second).is_err());
        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
