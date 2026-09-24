//! Active library lifecycle and shared database access.
mod artists;
mod style_extraction;
mod automation;
mod compare;
mod duplicates;
mod editing;
mod exports;
mod groups;
mod images;
mod imports;
mod library;
mod maintenance;
mod prompt_docs;
mod quick_edit;
mod rows;
mod tags;
#[cfg(test)]
mod tests;

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::db::LibrarySummary;
use crate::db::{AutomationRuleError, QuickEditError, TagMutationError};
use crate::images::RowImageError;
use crate::storage::{
    ContentHashProgress, DataDirectory, ImageFilesExportError, ImageImportError, JsonExportError,
    MigrationProgress, PromptDocError, PromptRotationJsonExportError, RowDeletionError,
    StorageError, XlsxExportError,
};

const LOCATOR_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeSnapshot {
    pub data_directory: Option<PathBuf>,
    pub rejected_images_directory: Option<PathBuf>,
    pub library: Option<LibrarySummary>,
    pub auto_artist_prefix_on_import: bool,
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
    #[error("图片导入失败: {0}")]
    ImageImport(#[from] ImageImportError),
    #[error("数据库操作失败: {0}")]
    Database(#[from] crate::db::DatabaseError),
    #[error("Tag 操作失败: {0}")]
    TagMutation(#[from] TagMutationError),
    #[error("快速整理失败: {0}")]
    QuickEdit(#[from] QuickEditError),
    #[error("自动规则操作失败: {0}")]
    AutomationRule(#[from] AutomationRuleError),
    #[error("删除行失败: {0}")]
    RowDeletion(#[from] RowDeletionError),
    #[error("图片读取失败: {0}")]
    Image(#[from] RowImageError),
    #[error("xlsx 导出失败: {0}")]
    XlsxExport(#[from] XlsxExportError),
    #[error("JSON 导出失败: {0}")]
    JsonExport(#[from] JsonExportError),
    #[error("轮询脚本 JSON 导出失败: {0}")]
    PromptRotationJsonExport(#[from] PromptRotationJsonExportError),
    #[error("图片文件导出失败: {0}")]
    ImageFilesExport(#[from] ImageFilesExportError),
    #[error("提示词文档操作失败: {0}")]
    PromptDoc(#[from] PromptDocError),
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
    /// 常驻数据库连接：跨命令复用（承载查询结果缓存），
    /// 首次使用时懒打开；重置/迁移数据目录前必须先置 None 释放文件句柄。
    database: Option<crate::db::Database>,
}

impl RuntimeState {
    fn database(&mut self) -> Result<&mut crate::db::Database, AppRuntimeError> {
        if let Some(error) = &self.startup_error {
            return Err(AppRuntimeError::StartupStateInvalid(error.clone()));
        }
        let directory = self.active.as_ref().ok_or(AppRuntimeError::NotConfigured)?;
        if self.database.is_none() {
            self.database = Some(directory.open_database()?);
        }
        Ok(self.database.as_mut().expect("database opened above"))
    }

    /// 其它连接写入数据后调用：清空常驻连接上的查询缓存。
    fn invalidate_query_cache(&mut self) {
        if let Some(database) = self.database.as_mut() {
            database.bump_data_version();
        }
    }
}

#[derive(Debug)]
pub(crate) struct AppRuntime {
    locator_path: PathBuf,
    state: Mutex<RuntimeState>,
}

impl AppRuntime {
    pub(crate) fn load(locator_path: PathBuf, default_data_dir: PathBuf) -> Self {
        let (active, startup_error) = match load_directory(&locator_path) {
            Ok(Some(directory)) => (Some(directory), None),
            Ok(None) => match DataDirectory::initialize(&default_data_dir) {
                Ok(dir) => match write_locator(&locator_path, dir.root()) {
                    Ok(()) => (Some(dir), None),
                    Err(error) => (None, Some(error.to_string())),
                },
                Err(error) => (None, Some(error.to_string())),
            },
            Err(error) => (None, Some(error.to_string())),
        };
        Self {
            locator_path,
            state: Mutex::new(RuntimeState {
                active,
                startup_error,
                database: None,
            }),
        }
    }

    pub(crate) fn snapshot(&self) -> Result<RuntimeSnapshot, AppRuntimeError> {
        let mut state = self.lock_state()?;
        let (library, rejected_images_directory, auto_artist_prefix_on_import) =
            if state.active.is_some() {
                let library = state.database()?.library_summary()?;
                let auto_artist_prefix_on_import =
                    state.database()?.auto_artist_prefix_on_import()?;
                let directory = state.active.as_ref().expect("checked above");
                (
                    Some(library),
                    directory.rejected_images_directory()?,
                    auto_artist_prefix_on_import,
                )
            } else {
                (None, None, false)
            };
        Ok(RuntimeSnapshot {
            data_directory: state
                .active
                .as_ref()
                .map(|directory| directory.root().to_owned()),
            rejected_images_directory,
            library,
            auto_artist_prefix_on_import,
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

    fn with_database<T, E>(
        &self,
        f: impl FnOnce(&mut crate::db::Database) -> Result<T, E>,
    ) -> Result<T, AppRuntimeError>
    where
        AppRuntimeError: From<E>,
    {
        let mut state = self.lock_state()?;
        ensure_startup_valid(&state)?;
        Ok(f(state.database()?)?)
    }

    /// 与 `with_database` 相同，但操作成功后使查询缓存失效。
    /// 所有会改动行/Tag/分组/提示词数据的调用必须走这个入口。
    fn with_database_mut<T, E>(
        &self,
        f: impl FnOnce(&mut crate::db::Database) -> Result<T, E>,
    ) -> Result<T, AppRuntimeError>
    where
        AppRuntimeError: From<E>,
    {
        let mut state = self.lock_state()?;
        ensure_startup_valid(&state)?;
        let database = state.database()?;
        let result = f(database)?;
        database.bump_data_version();
        Ok(result)
    }

    /// 全库扫描级长任务专用：克隆数据目录后立即释放状态锁，在独立连接上
    /// 执行只读操作。期间查询等其它命令不会被本任务阻塞。
    fn with_cloned_database<T, E>(
        &self,
        f: impl FnOnce(&mut crate::db::Database) -> Result<T, E>,
    ) -> Result<T, AppRuntimeError>
    where
        AppRuntimeError: From<E>,
    {
        let directory = self.active_directory()?;
        let mut database = directory.open_database()?;
        Ok(f(&mut database)?)
    }

    /// 与 `with_cloned_database` 相同，但写入完成后使常驻连接的查询缓存失效。
    fn with_cloned_database_mut<T, E>(
        &self,
        f: impl FnOnce(&mut crate::db::Database) -> Result<T, E>,
    ) -> Result<T, AppRuntimeError>
    where
        AppRuntimeError: From<E>,
    {
        let directory = self.active_directory()?;
        let mut database = directory.open_database()?;
        let result = f(&mut database)?;
        self.lock_state()?.invalidate_query_cache();
        Ok(result)
    }

    /// 取出活动数据目录的克隆并立即释放状态锁，供导出等长耗时操作使用。
    pub(crate) fn active_directory(&self) -> Result<DataDirectory, AppRuntimeError> {
        let state = self.lock_state()?;
        ensure_startup_valid(&state)?;
        Ok(state
            .active
            .as_ref()
            .ok_or(AppRuntimeError::NotConfigured)?
            .clone())
    }

    #[cfg(test)]
    pub(crate) fn migrate_directory(
        &self,
        destination: impl AsRef<Path>,
    ) -> Result<RuntimeMigrationOutcome, AppRuntimeError> {
        self.migrate_directory_with_progress(destination, |_| {})
    }

    pub(crate) fn migrate_directory_with_progress(
        &self,
        destination: impl AsRef<Path>,
        progress: impl FnMut(MigrationProgress),
    ) -> Result<RuntimeMigrationOutcome, AppRuntimeError> {
        let mut state = self.lock_state()?;
        ensure_startup_valid(&state)?;
        let current = state
            .active
            .as_ref()
            .ok_or(AppRuntimeError::NotConfigured)?
            .clone();
        // 迁移会复制并清理旧目录文件，必须先关闭常驻连接释放文件句柄。
        state.database = None;
        let prepared = current.prepare_migration_with_progress(destination, progress)?;
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
        state.database = None;
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
        state.database = None;
        drop(state);
        self.snapshot()
    }

    pub(crate) fn reset_data(&self) -> Result<RuntimeSnapshot, AppRuntimeError> {
        let mut state = self.lock_state()?;
        ensure_startup_valid(&state)?;
        let directory = state
            .active
            .as_ref()
            .ok_or(AppRuntimeError::NotConfigured)?
            .clone();
        // 重置会删除数据库文件，必须先关闭常驻连接释放文件句柄。
        state.database = None;
        directory.reset_data()?;
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
        state.database = None;
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
