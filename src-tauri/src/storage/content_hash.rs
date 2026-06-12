use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::{DataDirectory, StorageError};
use crate::db::ContentHashCandidate;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ContentHashProgress {
    pub processed: usize,
    pub total: usize,
    pub updated: usize,
    pub unreadable: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ContentHashBackfillOutcome {
    pub total: usize,
    pub updated: usize,
    pub unreadable: usize,
}

impl DataDirectory {
    pub fn backfill_content_hashes(
        &self,
        progress: impl Fn(ContentHashProgress),
    ) -> Result<ContentHashBackfillOutcome, StorageError> {
        let mut database = self.open_database()?;
        let candidates = database.missing_content_hashes()?;
        let total = candidates.len();
        if total == 0 {
            return Ok(ContentHashBackfillOutcome::default());
        }

        let mut hashes = Vec::with_capacity(total);
        let mut unreadable = 0;
        progress(ContentHashProgress {
            total,
            ..ContentHashProgress::default()
        });

        for (index, candidate) in candidates.iter().enumerate() {
            match resolve_image_path(self, candidate).and_then(|path| sha256_file(&path)) {
                Ok(hash) => hashes.push((candidate.row_id, hash)),
                Err(_) => unreadable += 1,
            }
            progress(ContentHashProgress {
                processed: index + 1,
                total,
                updated: hashes.len(),
                unreadable,
            });
        }

        database.update_content_hashes(&hashes)?;
        Ok(ContentHashBackfillOutcome {
            total,
            updated: hashes.len(),
            unreadable,
        })
    }
}

pub(crate) fn sha256_file(path: &Path) -> Result<String, io::Error> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn resolve_image_path(
    directory: &DataDirectory,
    candidate: &ContentHashCandidate,
) -> Result<PathBuf, io::Error> {
    if let Some(path) = nonempty(candidate.image_path.as_deref()).map(PathBuf::from)
        && path.is_file()
    {
        return Ok(path);
    }
    if let Some(relative) = nonempty(candidate.stored_image_path.as_deref()) {
        let path = directory.root().join(relative);
        if path.is_file() {
            return Ok(path);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("第 {} 行没有可读图片", candidate.row_id),
    ))
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::db::{NewRow, SourceType};

    use super::*;

    #[test]
    fn hashes_exact_file_bytes_with_sha256() {
        let temporary = TemporaryHashDirectory::new();
        fs::create_dir_all(&temporary.root).unwrap();
        let file = temporary.root.join("vector.bin");
        fs::write(&file, b"abc").unwrap();

        assert_eq!(
            sha256_file(&file).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn opening_directory_backfills_external_and_stored_images_but_keeps_unreadable_null() {
        let temporary = TemporaryHashDirectory::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        let external = temporary.root.join("external.png");
        fs::write(&external, b"external bytes").unwrap();

        let mut database = directory.open_database().unwrap();
        database
            .append_batch(
                SourceType::Folder,
                &temporary.root.to_string_lossy(),
                &[
                    NewRow {
                        source_ordinal: 1,
                        identity: "file:external".into(),
                        image_path: Some(external.to_string_lossy().into_owned()),
                        ..NewRow::default()
                    },
                    NewRow {
                        source_ordinal: 2,
                        identity: "archive:stored".into(),
                        image_path: Some("missing.zip > stored.png".into()),
                        stored_image_rel: Some("stored.png".into()),
                        ..NewRow::default()
                    },
                    NewRow {
                        source_ordinal: 3,
                        identity: "file:missing".into(),
                        image_path: Some(
                            temporary
                                .root
                                .join("missing.png")
                                .to_string_lossy()
                                .into_owned(),
                        ),
                        ..NewRow::default()
                    },
                ],
                |_| Ok(()),
            )
            .unwrap();
        drop(database);

        let stored = directory.files_path().join("1").join("stored.png");
        fs::create_dir_all(stored.parent().unwrap()).unwrap();
        fs::write(&stored, b"stored bytes").unwrap();

        let events = Mutex::new(Vec::new());
        let reopened = DataDirectory::open_with_hash_progress(&temporary.data, |progress| {
            events.lock().unwrap().push(progress);
        })
        .unwrap();
        let database = reopened.open_database().unwrap();

        assert_eq!(
            database.content_hash_for_row(1).unwrap(),
            Some(sha256_file(&external).unwrap())
        );
        assert_eq!(
            database.content_hash_for_row(2).unwrap(),
            Some(sha256_file(&stored).unwrap())
        );
        assert_eq!(database.content_hash_for_row(3).unwrap(), None);
        assert_eq!(
            events.lock().unwrap().last().copied(),
            Some(ContentHashProgress {
                processed: 3,
                total: 3,
                updated: 2,
                unreadable: 1,
            })
        );
    }

    struct TemporaryHashDirectory {
        root: PathBuf,
        data: PathBuf,
    }

    impl TemporaryHashDirectory {
        fn new() -> Self {
            let parent = Path::new(r"D:\Agent\Agent_temp");
            let parent = if parent.is_dir() {
                parent.to_owned()
            } else {
                std::env::temp_dir()
            };
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be valid")
                .as_nanos();
            let root = parent.join(format!(
                "smart-spreadsheet-content-hash-{}-{nonce}",
                std::process::id()
            ));
            Self {
                data: root.join("data"),
                root,
            }
        }
    }

    impl Drop for TemporaryHashDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
