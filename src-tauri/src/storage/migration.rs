use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use super::{DATABASE_FILE, DataDirectory, MARKER_FILE, StorageError};

const DATABASE_WAL_FILE: &str = "smart-spreadsheet.sqlite3-wal";
const DATABASE_SHM_FILE: &str = "smart-spreadsheet.sqlite3-shm";

#[derive(Debug)]
pub struct MigrationOutcome {
    pub data_directory: DataDirectory,
    pub retired_source: Option<PathBuf>,
}

#[derive(Debug)]
pub struct PreparedMigration {
    data_directory: DataDirectory,
    source: PathBuf,
    source_marker: PathBuf,
    retired_marker: PathBuf,
    finalized: bool,
}

impl DataDirectory {
    pub fn migrate_to(
        &self,
        destination: impl AsRef<Path>,
    ) -> Result<MigrationOutcome, StorageError> {
        Ok(self.prepare_migration(destination)?.commit())
    }

    pub fn prepare_migration(
        &self,
        destination: impl AsRef<Path>,
    ) -> Result<PreparedMigration, StorageError> {
        let source = fs::canonicalize(self.root())?;
        let destination = prepare_destination(&source, destination.as_ref())?;
        let staging = create_staging_directory(&destination)?;
        let mut staging_guard = StagingGuard::new(staging.clone());

        copy_non_database_tree(&source, &staging)?;
        let source_database = self.open_database()?;
        source_database.backup_to(staging.join(DATABASE_FILE))?;
        drop(source_database);

        verify_non_database_tree(&source, &staging)?;
        let staged_directory = DataDirectory::open(&staging)?;
        staged_directory.open_database()?.verify_integrity()?;

        let source_marker = source.join(MARKER_FILE);
        let retired_marker = available_retired_marker(self.migration_path())?;
        fs::rename(&source_marker, &retired_marker)?;

        if let Err(error) = fs::rename(&staging, &destination) {
            if fs::rename(&retired_marker, &source_marker).is_err() {
                return Err(StorageError::MarkerRestoreFailed(source_marker));
            }
            return Err(error.into());
        }
        staging_guard.disarm();

        Ok(PreparedMigration {
            data_directory: DataDirectory::open(&destination)?,
            source,
            source_marker,
            retired_marker,
            finalized: false,
        })
    }
}

impl PreparedMigration {
    pub fn data_directory(&self) -> &DataDirectory {
        &self.data_directory
    }

    pub fn commit(mut self) -> MigrationOutcome {
        let retired_source = if can_remove_automatically(&self.source) {
            match fs::remove_dir_all(&self.source) {
                Ok(()) => None,
                Err(_) => Some(self.source.clone()),
            }
        } else {
            Some(self.source.clone())
        };
        self.finalized = true;
        MigrationOutcome {
            data_directory: self.data_directory.clone(),
            retired_source,
        }
    }

    pub fn rollback(mut self) -> Result<(), StorageError> {
        let result = self.rollback_inner();
        if result.is_ok() {
            self.finalized = true;
        }
        result
    }

    fn rollback_inner(&self) -> Result<(), StorageError> {
        let destination_marker = self.data_directory.root().join(MARKER_FILE);
        let disabled_marker = self
            .data_directory
            .migration_path()
            .join(format!("rollback-destination-{}.json", std::process::id()));
        fs::rename(&destination_marker, &disabled_marker)?;
        if fs::rename(&self.retired_marker, &self.source_marker).is_err() {
            let _ = fs::rename(&disabled_marker, &destination_marker);
            return Err(StorageError::MarkerRestoreFailed(
                self.source_marker.clone(),
            ));
        }
        let _ = fs::remove_dir_all(self.data_directory.root());
        Ok(())
    }
}

impl Drop for PreparedMigration {
    fn drop(&mut self) {
        if !self.finalized && self.rollback_inner().is_ok() {
            self.finalized = true;
        }
    }
}

fn prepare_destination(source: &Path, destination: &Path) -> Result<PathBuf, StorageError> {
    let destination = if destination.is_absolute() {
        destination.to_owned()
    } else {
        std::env::current_dir()?.join(destination)
    };

    if destination.exists() {
        if !destination.is_dir() {
            return Err(StorageError::NotDirectory(destination));
        }
        let destination = fs::canonicalize(&destination)?;
        if destination == source {
            return Err(StorageError::SameMigrationDestination(destination));
        }
        if destination.starts_with(source) {
            return Err(StorageError::DestinationInsideSource(destination));
        }
        if fs::read_dir(&destination)?.next().is_some() {
            return Err(StorageError::NonEmptyDestination(destination));
        }
        fs::remove_dir(&destination)?;
        return Ok(user_facing_path(destination));
    }

    let file_name = destination
        .file_name()
        .ok_or_else(|| StorageError::NotDirectory(destination.clone()))?
        .to_owned();
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let mut existing_ancestor = parent;
    let mut missing_directories = Vec::new();
    while !existing_ancestor.exists() {
        let name = existing_ancestor
            .file_name()
            .ok_or_else(|| StorageError::NotDirectory(destination.clone()))?;
        missing_directories.push(name.to_owned());
        existing_ancestor = existing_ancestor
            .parent()
            .ok_or_else(|| StorageError::NotDirectory(destination.clone()))?;
    }
    if !existing_ancestor.is_dir() {
        return Err(StorageError::NotDirectory(existing_ancestor.to_owned()));
    }

    let mut canonical_parent = fs::canonicalize(existing_ancestor)?;
    for directory in missing_directories.iter().rev() {
        canonical_parent.push(directory);
    }
    let destination = canonical_parent.join(file_name);
    if destination == source {
        return Err(StorageError::SameMigrationDestination(destination));
    }
    if destination.starts_with(source) {
        return Err(StorageError::DestinationInsideSource(destination));
    }
    fs::create_dir_all(&canonical_parent)?;
    Ok(user_facing_path(destination))
}

#[cfg(windows)]
fn user_facing_path(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path
    }
}

#[cfg(not(windows))]
fn user_facing_path(path: PathBuf) -> PathBuf {
    path
}

fn create_staging_directory(destination: &Path) -> Result<PathBuf, StorageError> {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let name = destination
        .file_name()
        .ok_or_else(|| StorageError::NotDirectory(destination.to_owned()))?
        .to_string_lossy();

    for attempt in 0..100_u32 {
        let staging = parent.join(format!(
            ".{name}.smart-spreadsheet-migration-{}-{attempt}",
            std::process::id()
        ));
        match fs::create_dir(&staging) {
            Ok(()) => return Ok(staging),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }

    Err(StorageError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "无法创建唯一的数据迁移暂存目录",
    )))
}

fn available_retired_marker(migration_directory: PathBuf) -> Result<PathBuf, StorageError> {
    for attempt in 0..100_u32 {
        let path = migration_directory.join(format!(
            "retired-data-directory-{}-{attempt}.json",
            std::process::id()
        ));
        if !path.exists() {
            return Ok(path);
        }
    }
    Err(StorageError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "无法创建唯一的旧数据目录标记",
    )))
}

fn copy_non_database_tree(source: &Path, destination: &Path) -> Result<(), StorageError> {
    copy_directory(source, destination, Path::new(""))
}

fn copy_directory(
    source_root: &Path,
    destination_root: &Path,
    relative: &Path,
) -> Result<(), StorageError> {
    for entry in fs::read_dir(source_root.join(relative))? {
        let entry = entry?;
        let child_relative = relative.join(entry.file_name());
        if is_database_runtime_file(&child_relative) {
            continue;
        }

        let metadata = fs::symlink_metadata(entry.path())?;
        let destination = destination_root.join(&child_relative);
        if metadata.is_dir() {
            fs::create_dir(&destination)?;
            copy_directory(source_root, destination_root, &child_relative)?;
        } else if metadata.is_file() {
            fs::copy(entry.path(), destination)?;
        } else {
            return Err(StorageError::UnsupportedEntry(entry.path()));
        }
    }
    Ok(())
}

fn verify_non_database_tree(source: &Path, destination: &Path) -> Result<(), StorageError> {
    verify_directory(source, destination, Path::new(""))
}

fn verify_directory(
    source_root: &Path,
    destination_root: &Path,
    relative: &Path,
) -> Result<(), StorageError> {
    for entry in fs::read_dir(source_root.join(relative))? {
        let entry = entry?;
        let child_relative = relative.join(entry.file_name());
        if is_database_runtime_file(&child_relative) {
            continue;
        }

        let metadata = fs::symlink_metadata(entry.path())?;
        let destination = destination_root.join(&child_relative);
        if metadata.is_dir() {
            if !destination.is_dir() {
                return Err(StorageError::MigrationVerificationFailed(child_relative));
            }
            verify_directory(source_root, destination_root, &child_relative)?;
        } else if metadata.is_file() {
            if !destination.is_file() || !files_equal(&entry.path(), &destination)? {
                return Err(StorageError::MigrationVerificationFailed(child_relative));
            }
        } else {
            return Err(StorageError::UnsupportedEntry(entry.path()));
        }
    }
    Ok(())
}

pub(super) fn files_equal(left: &Path, right: &Path) -> Result<bool, StorageError> {
    if fs::metadata(left)?.len() != fs::metadata(right)?.len() {
        return Ok(false);
    }
    let mut left = File::open(left)?;
    let mut right = File::open(right)?;
    let mut left_buffer = [0_u8; 64 * 1024];
    let mut right_buffer = [0_u8; 64 * 1024];

    loop {
        let left_read = left.read(&mut left_buffer)?;
        let right_read = right.read(&mut right_buffer)?;
        if left_read != right_read || left_buffer[..left_read] != right_buffer[..right_read] {
            return Ok(false);
        }
        if left_read == 0 {
            return Ok(true);
        }
    }
}

fn is_database_runtime_file(relative: &Path) -> bool {
    relative.components().count() == 1
        && relative.file_name().is_some_and(|name| {
            name == DATABASE_FILE || name == DATABASE_WAL_FILE || name == DATABASE_SHM_FILE
        })
}

fn can_remove_automatically(path: &Path) -> bool {
    path.parent().and_then(Path::parent).is_some()
}

struct StagingGuard {
    path: PathBuf,
    active: bool,
}

impl StagingGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, active: true }
    }

    fn disarm(&mut self) {
        self.active = false;
    }
}

impl Drop for StagingGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use rusqlite::Connection;

    use super::*;

    #[test]
    fn migrates_database_workbook_and_cache_then_removes_source() {
        let temporary = TemporaryMigration::new();
        let source = DataDirectory::initialize(&temporary.source).unwrap();
        let sample = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("Examples")
            .join("novelai_metadata.xlsx");
        fs::copy(&sample, source.source_workbook_path()).unwrap();
        fs::write(source.thumbnail_cache_path().join("2.webp"), b"thumbnail").unwrap();
        let connection = Connection::open(source.database_path()).unwrap();
        connection
            .execute(
                "INSERT INTO settings(key, value) VALUES ('migration-test', 'preserved')",
                [],
            )
            .unwrap();
        drop(connection);

        fs::create_dir_all(&temporary.destination).unwrap();
        let outcome = source.migrate_to(&temporary.destination).unwrap();

        assert!(outcome.retired_source.is_none());
        assert_eq!(outcome.data_directory.root(), temporary.destination);
        assert!(!temporary.source.exists());
        assert_eq!(
            fs::read(outcome.data_directory.thumbnail_cache_path().join("2.webp")).unwrap(),
            b"thumbnail"
        );
        assert_eq!(
            fs::read(outcome.data_directory.source_workbook_path()).unwrap(),
            fs::read(sample).unwrap()
        );
        let migrated = Connection::open(outcome.data_directory.database_path()).unwrap();
        let value: String = migrated
            .query_row(
                "SELECT value FROM settings WHERE key = 'migration-test'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(value, "preserved");
    }

    #[test]
    fn refuses_nonempty_destination_without_touching_source() {
        let temporary = TemporaryMigration::new();
        let source = DataDirectory::initialize(&temporary.source).unwrap();
        fs::create_dir_all(&temporary.destination).unwrap();
        fs::write(temporary.destination.join("keep.txt"), b"keep").unwrap();

        let error = source.migrate_to(&temporary.destination).unwrap_err();

        assert!(matches!(error, StorageError::NonEmptyDestination(_)));
        assert!(DataDirectory::open(&temporary.source).is_ok());
        assert_eq!(
            fs::read(temporary.destination.join("keep.txt")).unwrap(),
            b"keep"
        );
    }

    #[test]
    fn refuses_destination_inside_source() {
        let temporary = TemporaryMigration::new();
        let source = DataDirectory::initialize(&temporary.source).unwrap();
        let nested_parent = temporary.source.join("nested");
        let nested_destination = nested_parent.join("destination");

        let error = source.migrate_to(&nested_destination).unwrap_err();

        assert!(matches!(error, StorageError::DestinationInsideSource(_)));
        assert!(DataDirectory::open(&temporary.source).is_ok());
        assert!(!nested_parent.exists());
    }

    struct TemporaryMigration {
        root: PathBuf,
        source: PathBuf,
        destination: PathBuf,
    }

    impl TemporaryMigration {
        fn new() -> Self {
            let local_agent_temp = Path::new(r"D:\Agent\Agent_temp");
            let parent = if local_agent_temp.is_dir() {
                local_agent_temp.to_owned()
            } else {
                std::env::temp_dir()
            };
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be valid")
                .as_nanos();
            let root = parent.join(format!(
                "smart-spreadsheet-migration-{}-{nonce}",
                std::process::id()
            ));
            Self {
                source: root.join("source"),
                destination: root.join("destination"),
                root,
            }
        }
    }

    impl Drop for TemporaryMigration {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
