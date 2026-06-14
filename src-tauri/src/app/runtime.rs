use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::db::{
    BatchSummary, GroupSummary, LibrarySummary, RowPage, RowQuery, RowSelection,
    TagMutationError, TagMutationResult, TagSelectionSummary, TagSummary,
};
use crate::images::{ImageVariant, RowImageError};
use crate::storage::{
    DataDirectory, ExportProgress, ImageFileExportMode, ImageFilesExportError,
    ImageFilesExportOutcome, ImageFilesProgress, ImageImportError, ImageImportOutcome,
    ImageImportProgress, JsonExportError, JsonExportOutcome, JsonExportProgress,
    PerceptualHashProgress, RowDeletionError, RowDeletionReport, SimilarImageMatch, StorageError,
    WorkbookImportError, XlsxExportError, XlsxExportOutcome, ContentHashProgress,
};

const LOCATOR_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeSnapshot {
    pub data_directory: Option<PathBuf>,
    pub rejected_images_directory: Option<PathBuf>,
    pub library: Option<LibrarySummary>,
    pub startup_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeMigrationOutcome {
    pub snapshot: RuntimeSnapshot,
    pub retired_source: Option<PathBuf>,
}

#[derive(Debug, Error)]
pub(crate) enum AppRuntimeError {
    #[error("应用状态锁不可用")]
    StatePoisoned,
    #[error("应用已配置数据目录；更换目录必须使用迁移功能")]
    AlreadyConfigured,
    #[error("尚未配置应用数据目录")]
    NotConfigured,
    #[error("启动状态存在错误，请先修复定位文件: {0}")]
    StartupStateInvalid(String),
    #[error("定位文件操作失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("定位文件格式无效: {0}")]
    LocatorJson(#[from] serde_json::Error),
    #[error("数据目录操作失败: {0}")]
    Storage(#[from] StorageError),
    #[error("工作簿导入失败: {0}")]
    Import(#[from] WorkbookImportError),
    #[error("图片导入失败: {0}")]
    ImageImport(#[from] ImageImportError),
    #[error("数据库操作失败: {0}")]
    Database(#[from] crate::db::DatabaseError),
    #[error("Tag 操作失败: {0}")]
    TagMutation(#[from] TagMutationError),
    #[error("删除行失败: {0}")]
    RowDeletion(#[from] RowDeletionError),
    #[error("图片读取失败: {0}")]
    Image(#[from] RowImageError),
    #[error("xlsx 导出失败: {0}")]
    XlsxExport(#[from] XlsxExportError),
    #[error("JSON 导出失败: {0}")]
    JsonExport(#[from] JsonExportError),
    #[error("图片文件导出失败: {0}")]
    ImageFilesExport(#[from] ImageFilesExportError),
    #[error("定位文件更新失败且迁移回滚失败。定位错误: {locator}; 回滚错误: {rollback}")]
    MigrationRollbackFailed { locator: String, rollback: String },
    #[error("无法恢复此前的数据目录定位文件: {0}")]
    LocatorRollbackFailed(PathBuf),
}

#[derive(Debug, Serialize, Deserialize)]
struct Locator {
    version: u32,
    data_directory: PathBuf,
}

#[derive(Debug)]
struct RuntimeState {
    active: Option<DataDirectory>,
    startup_error: Option<String>,
}

#[derive(Debug)]
pub(crate) struct AppRuntime {
    locator_path: PathBuf,
    state: Mutex<RuntimeState>,
}

impl AppRuntime {
    pub(crate) fn load(locator_path: PathBuf) -> Self {
        let (active, startup_error) = match load_directory(&locator_path) {
            Ok(active) => (active, None),
            Err(error) => (None, Some(error.to_string())),
        };
        Self {
            locator_path,
            state: Mutex::new(RuntimeState {
                active,
                startup_error,
            }),
        }
    }

    pub(crate) fn snapshot(&self) -> Result<RuntimeSnapshot, AppRuntimeError> {
        let state = self.lock_state()?;
        let (library, rejected_images_directory) = if let Some(directory) = state.active.as_ref() {
            (
                Some(directory.open_database()?.library_summary()?),
                directory.rejected_images_directory()?,
            )
        } else {
            (None, None)
        };
        Ok(RuntimeSnapshot {
            data_directory: state
                .active
                .as_ref()
                .map(|directory| directory.root().to_owned()),
            rejected_images_directory,
            library,
            startup_error: state.startup_error.clone(),
        })
    }

    pub(crate) fn initialize_directory(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<RuntimeSnapshot, AppRuntimeError> {
        self.configure_directory(path, |path| DataDirectory::initialize(path))
    }

    pub(crate) fn open_directory(
        &self,
        path: impl AsRef<Path>,
        progress: impl Fn(ContentHashProgress),
    ) -> Result<RuntimeSnapshot, AppRuntimeError> {
        self.configure_directory(path, |path| {
            DataDirectory::open_with_hash_progress(path, progress)
        })
    }

    #[cfg(test)]
    pub(crate) fn import_workbook(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<(RuntimeSnapshot, crate::storage::ImportOutcome), AppRuntimeError> {
        let state = self.lock_state()?;
        ensure_startup_valid(&state)?;
        let directory = state
            .active
            .as_ref()
            .ok_or(AppRuntimeError::NotConfigured)?;
        let outcome = directory.import_workbook(path)?;
        drop(state);
        Ok((self.snapshot()?, outcome))
    }

    pub(crate) fn import_images(
        &self,
        path: impl AsRef<Path>,
        progress: impl Fn(ImageImportProgress) + Sync,
    ) -> Result<(RuntimeSnapshot, ImageImportOutcome), AppRuntimeError> {
        let state = self.lock_state()?;
        ensure_startup_valid(&state)?;
        let directory = state
            .active
            .as_ref()
            .ok_or(AppRuntimeError::NotConfigured)?
            .clone();
        // 导入可能持续较久，提前释放状态锁，避免阻塞查询等其他操作。
        drop(state);
        let outcome = directory.import_images(path.as_ref(), progress)?;
        Ok((self.snapshot()?, outcome))
    }

    pub(crate) fn set_rejected_images_directory(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<RuntimeSnapshot, AppRuntimeError> {
        self.active_directory()?
            .set_rejected_images_directory(path)?;
        self.snapshot()
    }

    pub(crate) fn query_rows(&self, query: &RowQuery) -> Result<RowPage, AppRuntimeError> {
        self.with_database(|db| db.query_rows(query))
    }

    pub(crate) fn get_rows_by_ids(&self, ids: &[i64]) -> Result<Vec<crate::db::RowRecord>, AppRuntimeError> {
        self.with_database(|db| db.get_rows_by_ids(ids))
    }

    pub(crate) fn list_tags(&self) -> Result<Vec<TagSummary>, AppRuntimeError> {
        self.with_database(|db| db.list_tags())
    }

    pub(crate) fn delete_tag(&self, name: &str) -> Result<bool, AppRuntimeError> {
        self.with_database(|db| db.delete_tag(name))
    }

    pub(crate) fn create_tag(&self, name: &str) -> Result<bool, AppRuntimeError> {
        self.with_database(|db| db.create_tag(name))
    }

    pub(crate) fn count_selected_rows(
        &self,
        selection: &RowSelection,
    ) -> Result<u64, AppRuntimeError> {
        self.with_database(|db| db.count_selected_rows(selection))
    }

    pub(crate) fn list_selection_tags(
        &self,
        selection: &RowSelection,
    ) -> Result<Vec<TagSelectionSummary>, AppRuntimeError> {
        self.with_database(|db| db.list_selection_tags(selection))
    }

    pub(crate) fn selected_row_ids(
        &self,
        selection: &RowSelection,
    ) -> Result<Vec<i64>, AppRuntimeError> {
        self.with_database(|db| db.selected_row_ids(selection))
    }

    pub(crate) fn add_tags_to_selection(
        &self,
        selection: &RowSelection,
        tags: &[String],
    ) -> Result<TagMutationResult, AppRuntimeError> {
        self.with_database(|db| db.add_tags_to_selection(selection, tags))
    }

    pub(crate) fn remove_tags_from_selection(
        &self,
        selection: &RowSelection,
        tags: &[String],
    ) -> Result<TagMutationResult, AppRuntimeError> {
        self.with_database(|db| db.remove_tags_from_selection(selection, tags))
    }

    pub(crate) fn set_tags_for_row(
        &self,
        row_id: i64,
        tags: &[String],
    ) -> Result<TagMutationResult, AppRuntimeError> {
        self.with_database(|db| db.set_tags_for_row(row_id, tags))
    }

    pub(crate) fn delete_rows(
        &self,
        selection: &RowSelection,
        trash_originals: bool,
    ) -> Result<(RuntimeSnapshot, RowDeletionReport), AppRuntimeError> {
        let state = self.lock_state()?;
        ensure_startup_valid(&state)?;
        let directory = state
            .active
            .as_ref()
            .ok_or(AppRuntimeError::NotConfigured)?
            .clone();
        drop(state);
        let report = directory.delete_rows(selection, trash_originals)?;
        Ok((self.snapshot()?, report))
    }

    pub(crate) fn list_batches(&self) -> Result<Vec<BatchSummary>, AppRuntimeError> {
        self.with_database(|db| db.list_batches())
    }

    pub(crate) fn create_group(&self, name: &str) -> Result<GroupSummary, AppRuntimeError> {
        self.with_database(|db| db.create_group(name))
    }

    pub(crate) fn rename_group(
        &self,
        group_id: i64,
        new_name: &str,
    ) -> Result<GroupSummary, AppRuntimeError> {
        self.with_database(|db| db.rename_group(group_id, new_name))
    }

    pub(crate) fn delete_group(&self, group_id: i64) -> Result<bool, AppRuntimeError> {
        self.with_database(|db| db.delete_group(group_id))
    }

    pub(crate) fn delete_empty_groups(&self) -> Result<u64, AppRuntimeError> {
        self.with_database(|db| db.delete_empty_groups())
    }

    pub(crate) fn list_groups(&self) -> Result<Vec<GroupSummary>, AppRuntimeError> {
        self.with_database(|db| db.list_groups())
    }

    pub(crate) fn assign_rows_to_group(
        &self,
        selection: &RowSelection,
        group_id: i64,
    ) -> Result<u64, AppRuntimeError> {
        self.with_database(|db| db.assign_rows_to_group(selection, group_id))
    }

    pub(crate) fn ungroup_rows(
        &self,
        selection: &RowSelection,
    ) -> Result<u64, AppRuntimeError> {
        self.with_database(|db| db.ungroup_rows(selection))
    }

    pub(crate) fn get_group_members(
        &self,
        group_id: i64,
        offset: u64,
        limit: u32,
    ) -> Result<RowPage, AppRuntimeError> {
        self.with_database(|db| db.get_group_members(group_id, offset, limit))
    }

    pub(crate) fn row_thumbnail(&self, row_id: i64) -> Result<Vec<u8>, AppRuntimeError> {
        self.row_image(row_id, ImageVariant::Thumbnail)
    }

    pub(crate) fn row_preview(&self, row_id: i64) -> Result<Vec<u8>, AppRuntimeError> {
        self.row_image(row_id, ImageVariant::Preview)
    }

    fn row_image(&self, row_id: i64, variant: ImageVariant) -> Result<Vec<u8>, AppRuntimeError> {
        let state = self.lock_state()?;
        ensure_startup_valid(&state)?;
        let directory = state
            .active
            .as_ref()
            .ok_or(AppRuntimeError::NotConfigured)?
            .clone();
        drop(state);
        Ok(directory.load_row_image(row_id, variant)?.png_bytes)
    }

    pub(crate) fn export_xlsx(
        &self,
        selection: &RowSelection,
        destination: impl AsRef<Path>,
        progress: impl Fn(ExportProgress) + Sync,
    ) -> Result<XlsxExportOutcome, AppRuntimeError> {
        let directory = self.active_directory()?;
        Ok(directory.export_xlsx(selection, destination, progress)?)
    }

    pub(crate) fn export_zhihuiji_json(
        &self,
        selection: &RowSelection,
        destination: impl AsRef<Path>,
        progress: impl Fn(JsonExportProgress) + Sync,
    ) -> Result<JsonExportOutcome, AppRuntimeError> {
        let directory = self.active_directory()?;
        Ok(directory.export_zhihuiji_json(selection, destination, progress)?)
    }

    pub(crate) fn export_row_image(
        &self,
        row_id: i64,
        destination: impl AsRef<Path>,
    ) -> Result<(), AppRuntimeError> {
        let directory = self.active_directory()?;
        Ok(directory.export_single_image(row_id, destination.as_ref())?)
    }

    pub(crate) fn export_image_files(
        &self,
        selection: &RowSelection,
        parent_dir: impl AsRef<Path>,
        mode: ImageFileExportMode,
        progress: impl Fn(ImageFilesProgress) + Sync,
    ) -> Result<ImageFilesExportOutcome, AppRuntimeError> {
        let directory = self.active_directory()?;
        Ok(directory.export_image_files(selection, parent_dir, mode, progress)?)
    }

    pub(crate) fn backfill_perceptual_hashes(
        &self,
        progress: impl Fn(PerceptualHashProgress),
    ) -> Result<PerceptualHashProgress, AppRuntimeError> {
        let directory = self.active_directory()?;
        let outcome = directory.backfill_perceptual_hashes(progress)?;
        Ok(PerceptualHashProgress {
            processed: outcome.total,
            total: outcome.total,
            updated: outcome.updated,
            unreadable: outcome.unreadable,
        })
    }

    pub(crate) fn search_similar_images(
        &self,
        query_path: impl AsRef<Path>,
        threshold: u32,
    ) -> Result<Vec<SimilarImageMatch>, AppRuntimeError> {
        let directory = self.active_directory()?;
        Ok(directory.search_similar_images(query_path.as_ref(), threshold)?)
    }

    fn with_database<T, E>(
        &self,
        f: impl FnOnce(&mut crate::db::Database) -> Result<T, E>,
    ) -> Result<T, AppRuntimeError>
    where
        AppRuntimeError: From<E>,
    {
        let state = self.lock_state()?;
        ensure_startup_valid(&state)?;
        let directory = state
            .active
            .as_ref()
            .ok_or(AppRuntimeError::NotConfigured)?;
        let mut db = directory.open_database()?;
        Ok(f(&mut db)?)
    }

    /// 取出活动数据目录的克隆并立即释放状态锁，供导出等长耗时操作使用。
    fn active_directory(&self) -> Result<DataDirectory, AppRuntimeError> {
        let state = self.lock_state()?;
        ensure_startup_valid(&state)?;
        Ok(state
            .active
            .as_ref()
            .ok_or(AppRuntimeError::NotConfigured)?
            .clone())
    }

    pub(crate) fn migrate_directory(
        &self,
        destination: impl AsRef<Path>,
    ) -> Result<RuntimeMigrationOutcome, AppRuntimeError> {
        let mut state = self.lock_state()?;
        ensure_startup_valid(&state)?;
        let current = state
            .active
            .as_ref()
            .ok_or(AppRuntimeError::NotConfigured)?
            .clone();
        let prepared = current.prepare_migration(destination)?;
        let next_root = prepared.data_directory().root().to_owned();
        if let Err(locator_error) = write_locator(&self.locator_path, &next_root) {
            if let Err(rollback_error) = prepared.rollback() {
                return Err(AppRuntimeError::MigrationRollbackFailed {
                    locator: locator_error.to_string(),
                    rollback: rollback_error.to_string(),
                });
            }
            return Err(locator_error);
        }

        let outcome = prepared.commit();
        state.active = Some(outcome.data_directory);
        drop(state);
        Ok(RuntimeMigrationOutcome {
            snapshot: self.snapshot()?,
            retired_source: outcome.retired_source,
        })
    }

    pub(crate) fn reset_configuration(&self) -> Result<RuntimeSnapshot, AppRuntimeError> {
        let mut state = self.lock_state()?;
        if self.locator_path.exists() {
            fs::remove_file(&self.locator_path)?;
        }
        state.active = None;
        state.startup_error = None;
        drop(state);
        self.snapshot()
    }

    fn configure_directory<F>(
        &self,
        path: impl AsRef<Path>,
        open: F,
    ) -> Result<RuntimeSnapshot, AppRuntimeError>
    where
        F: FnOnce(&Path) -> Result<DataDirectory, StorageError>,
    {
        let mut state = self.lock_state()?;
        ensure_startup_valid(&state)?;
        if state.active.is_some() {
            return Err(AppRuntimeError::AlreadyConfigured);
        }
        let directory = open(path.as_ref())?;
        write_locator(&self.locator_path, directory.root())?;
        state.active = Some(directory);
        drop(state);
        self.snapshot()
    }

    fn lock_state(&self) -> Result<MutexGuard<'_, RuntimeState>, AppRuntimeError> {
        self.state
            .lock()
            .map_err(|_| AppRuntimeError::StatePoisoned)
    }
}

fn ensure_startup_valid(state: &RuntimeState) -> Result<(), AppRuntimeError> {
    if let Some(error) = &state.startup_error {
        Err(AppRuntimeError::StartupStateInvalid(error.clone()))
    } else {
        Ok(())
    }
}

fn load_directory(locator_path: &Path) -> Result<Option<DataDirectory>, AppRuntimeError> {
    if !locator_path.exists() {
        return Ok(None);
    }
    let locator: Locator = serde_json::from_reader(File::open(locator_path)?)?;
    if locator.version != LOCATOR_VERSION {
        return Err(AppRuntimeError::StartupStateInvalid(format!(
            "不支持的定位文件版本 {}",
            locator.version
        )));
    }
    Ok(Some(DataDirectory::open(locator.data_directory)?))
}

fn write_locator(locator_path: &Path, data_directory: &Path) -> Result<(), AppRuntimeError> {
    let parent = locator_path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".smart-spreadsheet-state-{}.tmp",
        std::process::id()
    ));
    let backup = parent.join(format!(
        ".smart-spreadsheet-state-{}.bak",
        std::process::id()
    ));
    let locator = Locator {
        version: LOCATOR_VERSION,
        data_directory: data_directory.to_owned(),
    };
    let contents = serde_json::to_vec_pretty(&locator)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(&contents)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    drop(file);

    let had_previous = locator_path.exists();
    if had_previous {
        fs::rename(locator_path, &backup)?;
    }
    if let Err(error) = fs::rename(&temporary, locator_path) {
        let _ = fs::remove_file(&temporary);
        if had_previous && fs::rename(&backup, locator_path).is_err() {
            return Err(AppRuntimeError::LocatorRollbackFailed(
                locator_path.to_owned(),
            ));
        }
        return Err(error.into());
    }
    if had_previous {
        let _ = fs::remove_file(backup);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn persists_and_reloads_configured_directory() {
        let temporary = TemporaryRuntime::new();
        let runtime = AppRuntime::load(temporary.locator.clone());

        let configured = runtime.initialize_directory(&temporary.data).unwrap();
        let reloaded = AppRuntime::load(temporary.locator.clone())
            .snapshot()
            .unwrap();

        assert_eq!(configured.data_directory, reloaded.data_directory);
        assert_eq!(reloaded.data_directory, Some(temporary.data.clone()));
        let library = reloaded.library.unwrap();
        assert_eq!(library.row_count, 0);
        assert_eq!(library.batch_count, 0);
        assert!(reloaded.startup_error.is_none());
    }

    #[test]
    fn refuses_pointer_switch_after_configuration() {
        let temporary = TemporaryRuntime::new();
        let runtime = AppRuntime::load(temporary.locator.clone());
        runtime.initialize_directory(&temporary.data).unwrap();

        let error = runtime
            .initialize_directory(temporary.root.join("other-data"))
            .unwrap_err();

        assert!(matches!(error, AppRuntimeError::AlreadyConfigured));
        assert!(!temporary.root.join("other-data").exists());
    }

    #[test]
    fn opening_existing_directory_reports_content_hash_backfill_progress() {
        let temporary = TemporaryRuntime::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        fs::create_dir_all(&temporary.root).unwrap();
        let image = temporary.root.join("legacy.png");
        fs::write(&image, b"legacy image bytes").unwrap();
        directory
            .open_database()
            .unwrap()
            .append_batch(
                crate::db::SourceType::Folder,
                &temporary.root.to_string_lossy(),
                &[crate::db::NewRow {
                    source_ordinal: 1,
                    identity: "file:legacy".into(),
                    image_path: Some(image.to_string_lossy().into_owned()),
                    ..crate::db::NewRow::default()
                }],
                |_| Ok(()),
            )
            .unwrap();
        let runtime = AppRuntime::load(temporary.locator.clone());
        let events = Mutex::new(Vec::new());

        let snapshot = runtime
            .open_directory(&temporary.data, |progress| {
                events.lock().unwrap().push(progress);
            })
            .unwrap();

        assert_eq!(snapshot.library.unwrap().row_count, 1);
        assert_eq!(
            events.lock().unwrap().last().copied(),
            Some(ContentHashProgress {
                processed: 1,
                total: 1,
                updated: 1,
                unreadable: 0,
            })
        );
        assert!(
            directory
                .open_database()
                .unwrap()
                .content_hash_for_row(1)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn imports_workbook_and_restores_summary_after_reload() {
        let temporary = TemporaryRuntime::new();
        let runtime = AppRuntime::load(temporary.locator.clone());
        runtime.initialize_directory(&temporary.data).unwrap();

        let (_, outcome) = runtime.import_workbook(sample_workbook()).unwrap();
        let reloaded = AppRuntime::load(temporary.locator.clone())
            .snapshot()
            .unwrap();

        assert_eq!(outcome.added, 5);
        let library = reloaded.library.unwrap();
        assert_eq!(library.row_count, 5);
        assert_eq!(library.batch_count, 1);
        let last_batch = library.last_batch.unwrap();
        assert!(last_batch.source_path.ends_with("novelai_metadata.xlsx"));
        assert_eq!(last_batch.added_count, 5);
    }

    #[test]
    fn appends_second_import_and_deletes_rows() {
        let temporary = TemporaryRuntime::new();
        let runtime = AppRuntime::load(temporary.locator.clone());
        runtime.initialize_directory(&temporary.data).unwrap();
        runtime.import_workbook(sample_workbook()).unwrap();

        // 重复导入：全部跳过，行数不变。
        let (snapshot, outcome) = runtime.import_workbook(sample_workbook()).unwrap();
        assert_eq!(outcome.added, 0);
        assert_eq!(outcome.skipped_existing, 5);
        assert_eq!(snapshot.library.as_ref().unwrap().row_count, 5);
        assert_eq!(snapshot.library.as_ref().unwrap().batch_count, 2);
        assert_eq!(runtime.list_batches().unwrap().len(), 2);

        let (after_delete, report) = runtime
            .delete_rows(&RowSelection::Explicit { row_ids: vec![1, 2] }, false)
            .unwrap();
        assert_eq!(report.deleted_rows, 2);
        assert_eq!(after_delete.library.unwrap().row_count, 3);
    }

    #[test]
    fn exposes_tag_queries_and_filtered_mutations() {
        let temporary = TemporaryRuntime::new();
        let runtime = AppRuntime::load(temporary.locator.clone());
        runtime.initialize_directory(&temporary.data).unwrap();
        runtime.import_workbook(sample_workbook()).unwrap();

        let explicit = RowSelection::Explicit {
            row_ids: vec![1, 2, 3],
        };
        let added = runtime
            .add_tags_to_selection(&explicit, &[" Keep ".into(), "keep".into()])
            .unwrap();
        assert_eq!(added.affected_rows, 3);
        assert_eq!(added.associations_changed, 6);
        assert_eq!(runtime.list_tags().unwrap().len(), 2);

        let filtered = RowSelection::Filtered {
            tags: vec!["Keep".into()],
            tag_mode: crate::db::TagMatchMode::And,
            dedupe: crate::db::DedupeMode::None,
            single_artist_only: false,
            excluded_row_ids: vec![2],
        };
        assert_eq!(runtime.count_selected_rows(&filtered).unwrap(), 2);
        let removed = runtime
            .remove_tags_from_selection(&filtered, &["Keep".into()])
            .unwrap();
        assert_eq!(removed.affected_rows, 2);
        assert_eq!(removed.associations_changed, 2);
    }

    #[test]
    fn loads_thumbnail_and_preview_for_imported_row() {
        let temporary = TemporaryRuntime::new();
        let runtime = AppRuntime::load(temporary.locator.clone());
        runtime.initialize_directory(&temporary.data).unwrap();
        runtime.import_workbook(sample_workbook()).unwrap();

        let thumbnail = runtime.row_thumbnail(1).unwrap();
        let preview = runtime.row_preview(1).unwrap();

        assert!(thumbnail.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(preview.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(thumbnail.len() <= preview.len());
    }

    #[test]
    fn migrates_directory_then_reloads_from_new_locator() {
        let temporary = TemporaryRuntime::new();
        let runtime = AppRuntime::load(temporary.locator.clone());
        runtime.initialize_directory(&temporary.data).unwrap();
        runtime.import_workbook(sample_workbook()).unwrap();
        runtime
            .add_tags_to_selection(
                &RowSelection::Explicit { row_ids: vec![1] },
                &["migrated".into()],
            )
            .unwrap();
        let destination = temporary.root.join("migrated-data");

        let outcome = runtime.migrate_directory(&destination).unwrap();
        let reloaded = AppRuntime::load(temporary.locator.clone());

        assert_eq!(outcome.snapshot.data_directory, Some(destination.clone()));
        assert!(outcome.retired_source.is_none());
        assert!(!temporary.data.exists());
        assert_eq!(
            reloaded.snapshot().unwrap().data_directory,
            Some(destination.clone())
        );
        assert_eq!(reloaded.list_tags().unwrap()[0].name, "migrated");
    }

    #[test]
    fn locator_write_failure_restores_source_and_disables_destination() {
        let temporary = TemporaryRuntime::new();
        let runtime = AppRuntime::load(temporary.locator.clone());
        runtime.initialize_directory(&temporary.data).unwrap();
        runtime.import_workbook(sample_workbook()).unwrap();
        let destination = temporary.root.join("failed-migration");
        let blocked_temporary = temporary.locator.parent().unwrap().join(format!(
            ".smart-spreadsheet-state-{}.tmp",
            std::process::id()
        ));
        fs::create_dir(&blocked_temporary).unwrap();

        assert!(runtime.migrate_directory(&destination).is_err());

        assert!(DataDirectory::open(&temporary.data).is_ok());
        assert!(DataDirectory::open(&destination).is_err());
        assert_eq!(
            AppRuntime::load(temporary.locator.clone())
                .snapshot()
                .unwrap()
                .data_directory,
            Some(temporary.data.clone())
        );
    }

    fn sample_workbook() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("Examples")
            .join("novelai_metadata.xlsx")
    }

    struct TemporaryRuntime {
        root: PathBuf,
        locator: PathBuf,
        data: PathBuf,
    }

    impl TemporaryRuntime {
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
                "smart-spreadsheet-runtime-{}-{nonce}",
                std::process::id()
            ));
            Self {
                locator: root.join("config").join("state.json"),
                data: root.join("data"),
                root,
            }
        }
    }

    impl Drop for TemporaryRuntime {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
