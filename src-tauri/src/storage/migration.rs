use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{DATABASE_FILE, DataDirectory, MARKER_FILE, StorageError};

const DATABASE_WAL_FILE: &str = "smart-spreadsheet.sqlite3-wal";
const DATABASE_SHM_FILE: &str = "smart-spreadsheet.sqlite3-shm";
const COPY_BUFFER_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MigrationStage {
    Preparing,
    CopyingFiles,
    BackingUpDatabase,
    VerifyingFiles,
    VerifyingDatabase,
    Switching,
}

/// 迁移总进度使用文件字节数作为权重：非数据库文件复制与校验各计一次，
/// 数据库备份与完整性校验各计一次。`stage_completed/total` 是当前阶段的原始单位：
/// 复制/文件校验为字节，数据库备份为 SQLite 页数。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationProgress {
    pub stage: MigrationStage,
    pub completed: u64,
    pub total: u64,
    pub stage_completed: u64,
    pub stage_total: u64,
}

impl MigrationProgress {
    fn new(
        stage: MigrationStage,
        completed: u64,
        total: u64,
        stage_completed: u64,
        stage_total: u64,
    ) -> Self {
        Self {
            stage,
            completed: completed.min(total),
            total,
            stage_completed,
            stage_total,
        }
    }
}

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
        self.migrate_to_with_progress(destination, |_| {})
    }

    pub fn migrate_to_with_progress(
        &self,
        destination: impl AsRef<Path>,
        progress: impl FnMut(MigrationProgress),
    ) -> Result<MigrationOutcome, StorageError> {
        Ok(self
            .prepare_migration_with_progress(destination, progress)?
            .commit())
    }

    pub fn prepare_migration(
        &self,
        destination: impl AsRef<Path>,
    ) -> Result<PreparedMigration, StorageError> {
        self.prepare_migration_with_progress(destination, |_| {})
    }

    pub fn prepare_migration_with_progress(
        &self,
        destination: impl AsRef<Path>,
        mut progress: impl FnMut(MigrationProgress),
    ) -> Result<PreparedMigration, StorageError> {
        progress(MigrationProgress::new(
            MigrationStage::Preparing,
            0,
            0,
            0,
            0,
        ));
        let source = fs::canonicalize(self.root())?;
        let destination = prepare_destination(&source, destination.as_ref())?;
        let staging = create_staging_directory(&destination)?;
        let mut staging_guard = StagingGuard::new(staging.clone());

        let manifest = build_migration_manifest(&source)?;
        let non_database_bytes = manifest.total_bytes;
        let database_bytes = database_runtime_bytes(&source)?.max(1);
        let total_work = non_database_bytes
            .saturating_add(database_bytes)
            .saturating_mul(2);

        let mut copied_bytes = 0_u64;
        progress(MigrationProgress::new(
            MigrationStage::CopyingFiles,
            0,
            total_work,
            0,
            non_database_bytes,
        ));
        let copied_files = copy_migration_manifest(&source, &staging, &manifest, |bytes| {
            copied_bytes = copied_bytes.saturating_add(bytes);
            progress(MigrationProgress::new(
                MigrationStage::CopyingFiles,
                copied_bytes,
                total_work,
                copied_bytes,
                non_database_bytes,
            ));
        })?;

        progress(MigrationProgress::new(
            MigrationStage::BackingUpDatabase,
            non_database_bytes,
            total_work,
            0,
            0,
        ));
        let source_database = self.open_database()?;
        source_database.backup_to_with_progress(
            staging.join(DATABASE_FILE),
            |completed_pages, total_pages| {
                let database_progress = if total_pages == 0 {
                    0
                } else {
                    database_bytes
                        .saturating_mul(completed_pages)
                        .checked_div(total_pages)
                        .unwrap_or_default()
                };
                progress(MigrationProgress::new(
                    MigrationStage::BackingUpDatabase,
                    non_database_bytes.saturating_add(database_progress),
                    total_work,
                    completed_pages,
                    total_pages,
                ));
            },
        )?;
        drop(source_database);

        let after_backup = non_database_bytes.saturating_add(database_bytes);
        let mut verified_bytes = 0_u64;
        progress(MigrationProgress::new(
            MigrationStage::VerifyingFiles,
            after_backup,
            total_work,
            0,
            non_database_bytes,
        ));
        verify_migration_manifest(&staging, &manifest, &copied_files, |bytes| {
            verified_bytes = verified_bytes.saturating_add(bytes);
            progress(MigrationProgress::new(
                MigrationStage::VerifyingFiles,
                after_backup.saturating_add(verified_bytes),
                total_work,
                verified_bytes,
                non_database_bytes,
            ));
        })?;
        let staged_directory = DataDirectory::from_verified_root(staging.clone());
        progress(MigrationProgress::new(
            MigrationStage::VerifyingDatabase,
            after_backup.saturating_add(non_database_bytes),
            total_work,
            0,
            database_bytes,
        ));
        staged_directory.open_database()?.verify_integrity()?;

        progress(MigrationProgress::new(
            MigrationStage::Switching,
            total_work,
            total_work,
            1,
            1,
        ));

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
            data_directory: DataDirectory::from_verified_root(destination),
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

fn database_runtime_bytes(source: &Path) -> Result<u64, StorageError> {
    let mut total = 0_u64;
    for name in [DATABASE_FILE, DATABASE_WAL_FILE] {
        let path = source.join(name);
        if path.is_file() {
            total = total.saturating_add(fs::metadata(path)?.len());
        }
    }
    Ok(total)
}

#[derive(Debug)]
struct MigrationManifest {
    directories: Vec<PathBuf>,
    files: Vec<MigrationFile>,
    total_bytes: u64,
}

#[derive(Debug)]
struct MigrationFile {
    relative: PathBuf,
    bytes: u64,
    modified: Option<SystemTime>,
}

#[derive(Debug)]
struct CopiedMigrationFile {
    relative: PathBuf,
    bytes: u64,
    sha256: [u8; 32],
}

fn build_migration_manifest(source: &Path) -> Result<MigrationManifest, StorageError> {
    let mut manifest = MigrationManifest {
        directories: Vec::new(),
        files: Vec::new(),
        total_bytes: 0,
    };
    collect_migration_entries(source, Path::new(""), &mut manifest)?;
    Ok(manifest)
}

fn collect_migration_entries(
    source_root: &Path,
    relative: &Path,
    manifest: &mut MigrationManifest,
) -> Result<(), StorageError> {
    for entry in fs::read_dir(source_root.join(relative))? {
        let entry = entry?;
        let child_relative = relative.join(entry.file_name());
        if should_skip_migration_entry(&child_relative) {
            continue;
        }

        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.is_dir() {
            manifest.directories.push(child_relative.clone());
            collect_migration_entries(source_root, &child_relative, manifest)?;
        } else if metadata.is_file() {
            manifest.total_bytes = manifest.total_bytes.saturating_add(metadata.len());
            manifest.files.push(MigrationFile {
                relative: child_relative,
                bytes: metadata.len(),
                modified: metadata.modified().ok(),
            });
        } else {
            return Err(StorageError::UnsupportedEntry(entry.path()));
        }
    }
    Ok(())
}

fn copy_migration_manifest(
    source_root: &Path,
    destination_root: &Path,
    manifest: &MigrationManifest,
    mut progress: impl FnMut(u64),
) -> Result<Vec<CopiedMigrationFile>, StorageError> {
    for relative in &manifest.directories {
        fs::create_dir_all(destination_root.join(relative))?;
    }
    // 缩略图是派生数据，迁移时保留目录结构但不复制缓存内容。
    fs::create_dir_all(destination_root.join(thumbnail_cache_relative_path()))?;

    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
    let mut copied_files = Vec::with_capacity(manifest.files.len());
    for file in &manifest.files {
        let source = source_root.join(&file.relative);
        let destination = destination_root.join(&file.relative);
        let (bytes, sha256) =
            copy_file_with_hash(&source, &destination, &mut buffer, &mut progress)?;
        let source_metadata = fs::metadata(&source)?;
        let source_changed = bytes != file.bytes
            || source_metadata.len() != file.bytes
            || file
                .modified
                .is_some_and(|modified| source_metadata.modified().ok() != Some(modified));
        if source_changed {
            return Err(StorageError::MigrationVerificationFailed(
                file.relative.clone(),
            ));
        }
        fs::set_permissions(&destination, source_metadata.permissions())?;
        copied_files.push(CopiedMigrationFile {
            relative: file.relative.clone(),
            bytes,
            sha256,
        });
    }
    Ok(copied_files)
}

fn verify_migration_manifest(
    destination_root: &Path,
    manifest: &MigrationManifest,
    copied_files: &[CopiedMigrationFile],
    mut progress: impl FnMut(u64),
) -> Result<(), StorageError> {
    for relative in &manifest.directories {
        if !destination_root.join(relative).is_dir() {
            return Err(StorageError::MigrationVerificationFailed(relative.clone()));
        }
    }
    if !destination_root
        .join(thumbnail_cache_relative_path())
        .is_dir()
    {
        return Err(StorageError::MigrationVerificationFailed(
            thumbnail_cache_relative_path(),
        ));
    }

    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
    for file in copied_files {
        let destination = destination_root.join(&file.relative);
        if !destination.is_file() {
            return Err(StorageError::MigrationVerificationFailed(
                file.relative.clone(),
            ));
        }
        let (bytes, sha256) = hash_file(&destination, &mut buffer, &mut progress)?;
        if bytes != file.bytes || sha256 != file.sha256 {
            return Err(StorageError::MigrationVerificationFailed(
                file.relative.clone(),
            ));
        }
    }
    Ok(())
}

fn copy_file_with_hash<F: FnMut(u64)>(
    source: &Path,
    destination: &Path,
    buffer: &mut [u8],
    progress: &mut F,
) -> Result<(u64, [u8; 32]), StorageError> {
    let mut reader = File::open(source)?;
    let mut writer = File::create(destination)?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    loop {
        let read = reader.read(buffer)?;
        if read == 0 {
            break;
        }
        writer.write_all(&buffer[..read])?;
        hasher.update(&buffer[..read]);
        let read = u64::try_from(read).unwrap_or(u64::MAX);
        total = total.saturating_add(read);
        progress(read);
    }
    writer.flush()?;
    Ok((total, hasher.finalize().into()))
}

fn hash_file<F: FnMut(u64)>(
    path: &Path,
    buffer: &mut [u8],
    progress: &mut F,
) -> Result<(u64, [u8; 32]), StorageError> {
    let mut reader = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    loop {
        let read = reader.read(buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        let read = u64::try_from(read).unwrap_or(u64::MAX);
        total = total.saturating_add(read);
        progress(read);
    }
    Ok((total, hasher.finalize().into()))
}

fn is_database_runtime_file(relative: &Path) -> bool {
    relative.components().count() == 1
        && relative.file_name().is_some_and(|name| {
            name == DATABASE_FILE || name == DATABASE_WAL_FILE || name == DATABASE_SHM_FILE
        })
}

fn should_skip_migration_entry(relative: &Path) -> bool {
    is_database_runtime_file(relative) || relative.starts_with(thumbnail_cache_relative_path())
}

fn thumbnail_cache_relative_path() -> PathBuf {
    Path::new("cache").join("thumbnails")
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
    fn migrates_database_and_managed_files_without_derived_cache() {
        let temporary = TemporaryMigration::new();
        let source = DataDirectory::initialize(&temporary.source).unwrap();
        let managed_file = source.files_path().join("1").join("image.png");
        fs::create_dir_all(managed_file.parent().unwrap()).unwrap();
        fs::write(&managed_file, b"managed image").unwrap();
        fs::write(source.thumbnail_cache_path().join("2.webp"), b"thumbnail").unwrap();
        let connection = Connection::open(source.database_path()).unwrap();
        connection
            .execute(
                "INSERT INTO settings(key, value) VALUES ('migration-test', 'preserved')",
                [],
            )
            .unwrap();
        connection
            .execute("INSERT INTO tags(name) VALUES ('迁移保留标签')", [])
            .unwrap();
        drop(connection);

        fs::create_dir_all(&temporary.destination).unwrap();
        let outcome = source.migrate_to(&temporary.destination).unwrap();

        assert!(outcome.retired_source.is_none());
        assert_eq!(outcome.data_directory.root(), temporary.destination);
        assert!(!temporary.source.exists());
        assert!(outcome.data_directory.thumbnail_cache_path().is_dir());
        assert!(
            fs::read_dir(outcome.data_directory.thumbnail_cache_path())
                .unwrap()
                .next()
                .is_none()
        );
        assert_eq!(
            fs::read(
                outcome
                    .data_directory
                    .files_path()
                    .join("1")
                    .join("image.png")
            )
            .unwrap(),
            b"managed image"
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
        let migrated_tag_count: i64 = migrated
            .query_row(
                "SELECT COUNT(*) FROM tags WHERE name = '迁移保留标签'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(migrated_tag_count, 1);
    }

    #[test]
    fn reports_monotonic_progress_for_every_migration_stage() {
        let temporary = TemporaryMigration::new();
        let source = DataDirectory::initialize(&temporary.source).unwrap();
        let managed_file = source.files_path().join("1").join("image.png");
        fs::create_dir_all(managed_file.parent().unwrap()).unwrap();
        fs::write(&managed_file, vec![7_u8; 256 * 1024]).unwrap();
        let mut events = Vec::new();

        source
            .migrate_to_with_progress(&temporary.destination, |progress| {
                events.push(progress);
            })
            .unwrap();

        assert!(!events.is_empty());
        assert!(
            events
                .windows(2)
                .all(|pair| pair[0].completed <= pair[1].completed)
        );
        let mut stages = events
            .iter()
            .map(|progress| progress.stage)
            .collect::<Vec<_>>();
        stages.dedup();
        assert_eq!(
            stages,
            vec![
                MigrationStage::Preparing,
                MigrationStage::CopyingFiles,
                MigrationStage::BackingUpDatabase,
                MigrationStage::VerifyingFiles,
                MigrationStage::VerifyingDatabase,
                MigrationStage::Switching,
            ]
        );
        let completed = events.last().unwrap();
        assert_eq!(completed.stage, MigrationStage::Switching);
        assert!(completed.total > 0);
        assert_eq!(completed.completed, completed.total);
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
