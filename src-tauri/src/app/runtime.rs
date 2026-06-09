use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::db::WorkbookSummary;
use crate::storage::{DataDirectory, ImportOutcome, StorageError, WorkbookImportError};

const LOCATOR_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeSnapshot {
    pub data_directory: Option<PathBuf>,
    pub workbook: Option<WorkbookSummary>,
    pub startup_error: Option<String>,
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
    #[error("数据库操作失败: {0}")]
    Database(#[from] crate::db::DatabaseError),
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
        let workbook = if let Some(directory) = state.active.as_ref() {
            directory.open_database()?.workbook_summary()?
        } else {
            None
        };
        Ok(RuntimeSnapshot {
            data_directory: state
                .active
                .as_ref()
                .map(|directory| directory.root().to_owned()),
            workbook,
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
    ) -> Result<RuntimeSnapshot, AppRuntimeError> {
        self.configure_directory(path, |path| DataDirectory::open(path))
    }

    pub(crate) fn import_workbook(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<(RuntimeSnapshot, ImportOutcome), AppRuntimeError> {
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
        assert!(reloaded.workbook.is_none());
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
    fn imports_workbook_and_restores_summary_after_reload() {
        let temporary = TemporaryRuntime::new();
        let runtime = AppRuntime::load(temporary.locator.clone());
        runtime.initialize_directory(&temporary.data).unwrap();

        let (_, outcome) = runtime.import_workbook(sample_workbook()).unwrap();
        let reloaded = AppRuntime::load(temporary.locator.clone())
            .snapshot()
            .unwrap();

        assert_eq!(outcome.row_count, 5);
        let workbook = reloaded.workbook.unwrap();
        assert_eq!(workbook.imported_name, "novelai_metadata.xlsx");
        assert_eq!(workbook.row_count, 5);
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
