use super::Db;
use crate::{Error, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

pub(super) const COMPOSER_DRAFT_KEY: &str = "composer_draft";
const MAX_DRAFT_FILES_BYTES: usize = 25 * 1024 * 1024;

#[derive(Deserialize)]
struct DraftFile {
    id: Option<String>,
    filename: String,
    mime_type: Option<String>,
    base64: Option<String>,
    data: Option<Vec<u8>>,
}

impl DraftFile {
    fn take_contents(&mut self, remaining: usize) -> Result<Option<Vec<u8>>> {
        let bytes = match (self.base64.take(), self.data.take()) {
            (Some(encoded), None) if encoded.len() <= remaining.div_ceil(3) * 4 => STANDARD
                .decode(encoded)
                .map_err(|_| Error::Other("не удалось прочитать вложение черновика".into()))?,
            (Some(_), None) => return Err(draft_files_too_large()),
            (None, Some(bytes)) => bytes,
            (None, None) => return Ok(None),
            (Some(_), Some(_)) => {
                return Err(Error::Other("содержимое вложения указано дважды".into()));
            }
        };
        if bytes.len() > remaining {
            return Err(draft_files_too_large());
        }
        Ok(Some(bytes))
    }
}

impl Db {
    pub(super) async fn migrate_composer_draft_files(&self) -> Result<()> {
        let Some(value) = self.setting(COMPOSER_DRAFT_KEY).await? else {
            return Ok(());
        };
        let Ok(draft) = serde_json::from_str::<Value>(&value) else {
            return Ok(());
        };
        if draft
            .get("attachments")
            .and_then(Value::as_array)
            .is_some_and(|files| files.iter().any(|file| file.get("data").is_some()))
        {
            self.save_composer_draft(&value).await?;
        }
        Ok(())
    }

    pub(super) async fn save_composer_draft(&self, value: &str) -> Result<()> {
        let mut draft: Value = if value.is_empty() {
            json!({})
        } else {
            serde_json::from_str(value)?
        };
        let object = draft
            .as_object_mut()
            .ok_or_else(|| Error::Other("недопустимый формат черновика".into()))?;
        let files: Vec<DraftFile> = match object.remove("attachments") {
            Some(files) => serde_json::from_value(files)?,
            None => Vec::new(),
        };

        let mut created = Vec::new();
        let saved: Result<Vec<String>> = async {
            let mut tx = self.begin_write().await?;
            let existing: Vec<(String, String, i64)> =
                sqlx::query_as("SELECT id,blob_ref,size FROM composer_draft_files")
                    .fetch_all(&mut *tx)
                    .await?;
            let existing: HashMap<_, _> = existing
                .into_iter()
                .map(|(id, reference, size)| (id, (reference, size)))
                .collect();
            let mut wanted = HashSet::new();
            let mut metadata = Vec::with_capacity(files.len());
            let mut remaining = MAX_DRAFT_FILES_BYTES;
            for mut file in files {
                let id = file
                    .id
                    .take()
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                if !wanted.insert(id.clone()) {
                    return Err(Error::Other("вложение черновика указано дважды".into()));
                }
                let upload = file.take_contents(remaining)?;
                let size = if let Some((reference, size)) = existing.get(&id) {
                    if let Some(bytes) = upload {
                        // A missing response can cause an identical upload to be
                        // repeated. A file identifier must never silently change contents.
                        let blobs = self.blobs.clone();
                        let reference = reference.clone();
                        let identical = tokio::task::spawn_blocking(move || {
                            blobs.get(&reference).map(|saved| saved == bytes)
                        })
                        .await
                        .map_err(|error| {
                            Error::Other(format!("не удалось проверить вложение черновика: {error}"))
                        })??;
                        if !identical {
                            return Err(Error::Other(
                                "содержимое вложения изменилось, прикрепите файл заново".into(),
                            ));
                        }
                    }
                    *size as usize
                } else {
                    let bytes =
                        upload.ok_or_else(|| Error::Other("вложение черновика не найдено".into()))?;
                    let size = bytes.len();
                    let blobs = self.blobs.clone();
                    let reference = tokio::task::spawn_blocking(move || blobs.put(&bytes))
                        .await
                        .map_err(|error| {
                            Error::Other(format!("не удалось сохранить вложение черновика: {error}"))
                        })??;
                    created.push(reference.clone());
                    sqlx::query("INSERT INTO composer_draft_files(id,blob_ref,size) VALUES(?,?,?)")
                        .bind(&id)
                        .bind(&reference)
                        .bind(size as i64)
                        .execute(&mut *tx)
                        .await?;
                    size
                };
                remaining = remaining
                    .checked_sub(size)
                    .ok_or_else(draft_files_too_large)?;
                metadata.push(
                    json!({"id":id,"filename":file.filename,"mime_type":file.mime_type,"size":size}),
                );
            }
            object.insert("attachments".into(), json!(metadata));

            let mut removed = Vec::new();
            for (id, (reference, _)) in existing {
                if !wanted.contains(&id) {
                    sqlx::query("DELETE FROM composer_draft_files WHERE id=?")
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                    removed.push(reference);
                }
            }
            let value = if value.is_empty() {
                String::new()
            } else {
                draft.to_string()
            };
            sqlx::query(
                "INSERT INTO settings(key,value) VALUES(?,?)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            )
            .bind(COMPOSER_DRAFT_KEY)
            .bind(self.encrypt_setting(&value)?)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(removed)
        }
        .await;
        match saved {
            Ok(removed) => self.release_mail_blobs(removed).await,
            Err(error) => {
                self.release_mail_blobs(created).await;
                return Err(error);
            }
        }
        Ok(())
    }

    pub async fn composer_draft_file(&self, id: &str) -> Result<Vec<u8>> {
        let reference: String =
            sqlx::query_scalar("SELECT blob_ref FROM composer_draft_files WHERE id=?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or_else(|| Error::Other("вложение черновика не найдено".into()))?;
        let blobs = self.blobs.clone();
        tokio::task::spawn_blocking(move || blobs.get(&reference))
            .await
            .map_err(|error| {
                Error::Other(format!("не удалось прочитать вложение черновика: {error}"))
            })?
    }
}

fn draft_files_too_large() -> Error {
    Error::AccountConfig("общий размер вложений черновика превышает 25 МБ".into())
}

#[cfg(test)]
mod tests {
    use super::{COMPOSER_DRAFT_KEY, json};
    use crate::storage::repo::test_storage::open_test_db;

    #[tokio::test]
    async fn migrates_existing_draft_and_saves_edits_without_rewriting_files() {
        let db = open_test_db("composer-draft-files").await;
        let legacy = json!({"subject":"старый","attachments":[
            {"filename":"report.txt","mime_type":"text/plain","data":[97,98,99]}
        ]});
        sqlx::query("INSERT INTO settings(key,value) VALUES(?,?)")
            .bind(COMPOSER_DRAFT_KEY)
            .bind(db.encrypt_setting(&legacy.to_string()).unwrap())
            .execute(&db.write_pool)
            .await
            .unwrap();

        db.migrate_composer_draft_files().await.unwrap();
        let mut draft: serde_json::Value =
            serde_json::from_str(&db.setting(COMPOSER_DRAFT_KEY).await.unwrap().unwrap()).unwrap();
        let file = &draft["attachments"][0];
        let id = file["id"].as_str().unwrap().to_owned();
        assert_eq!(file["size"], 3);
        assert_eq!(file["filename"], "report.txt");
        assert!(file.get("data").is_none());
        assert_eq!(db.composer_draft_file(&id).await.unwrap(), b"abc");

        draft["subject"] = json!("изменённый");
        let references = db.blobs.references().unwrap();
        db.set_setting(COMPOSER_DRAFT_KEY, &draft.to_string())
            .await
            .unwrap();
        let (removed, missing) = db.garbage_collect_blobs().await.unwrap();
        assert_eq!(removed, 0);
        assert!(missing.is_empty());
        assert_eq!(db.blobs.references().unwrap(), references);

        let mut retry = draft.clone();
        retry["attachments"][0]["base64"] = json!("YWJj");
        db.set_setting(COMPOSER_DRAFT_KEY, &retry.to_string())
            .await
            .unwrap();
        retry["attachments"][0]["base64"] = json!("ZGVm");
        assert!(
            db.set_setting(COMPOSER_DRAFT_KEY, &retry.to_string())
                .await
                .is_err()
        );
        assert_eq!(db.composer_draft_file(&id).await.unwrap(), b"abc");

        draft["attachments"][0]["id"] = json!("missing");
        assert!(
            db.set_setting(COMPOSER_DRAFT_KEY, &draft.to_string())
                .await
                .is_err()
        );
        assert_eq!(db.composer_draft_file(&id).await.unwrap(), b"abc");

        let invalid_batch = json!({"attachments":[
            {"id":"new","filename":"new.txt","base64":"ZGVm"},
            {"id":"invalid","filename":"invalid.txt","base64":"!!"}
        ]});
        assert!(
            db.set_setting(COMPOSER_DRAFT_KEY, &invalid_batch.to_string())
                .await
                .is_err()
        );
        assert_eq!(db.blobs.references().unwrap(), references);

        db.set_setting(COMPOSER_DRAFT_KEY, "").await.unwrap();
        assert!(db.blobs.references().unwrap().is_empty());
        assert!(db.composer_draft_file(&id).await.is_err());
    }
}
