use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::db::{
    AutoArtistPrefixApplyResult, AutomationRule, AutomationRuleDraft, AutomationRuleError,
    AutomationRuleExportResult, AutomationRuleImportInspection, AutomationRuleImportResult,
    AutoArtistPrefixPreview, BatchSummary,
    DedupeCluster, DedupeMode, GroupSummary, LibrarySummary,
    MutableRowState, QuickArtistPrefixApplyResult, QuickArtistPrefixChange,
    QuickArtistPrefixPreview,
    QuickEditCondition, QuickEditError, QuickGroupApplyResult, QuickGroupChange,
    QuickGroupPreview, QuickTagApplyResult, QuickTagAssociation, QuickTagPreview, RowPage,
    RowQuery, RowSelection, SortMode, TagMatchMode, TagMutationError, TagMutationResult,
    RuleExecutionSummary, RulePreview, TagSelectionSummary, TagSummary,
    parse_automation_rule_text, read_automation_rule_file, write_automation_rule_file,
};
use crate::images::{ImageVariant, RowImageError};
use crate::storage::{
    ContentHashProgress, DataDirectory, ExportProgress, ImageFileExportMode, ImageFileNaming,
    ExistingImageUpdateOutcome, ImageFilesExportError, ImageFilesExportOutcome,
    ImageFilesProgress, ImageImportError, ImageImportOutcome, ImageImportProgress, JsonExportError,
    JsonExportOutcome, JsonExportProgress, PerceptualHashProgress, PromptDocAsset, PromptDocDetail,
    PromptDocError, PromptDocSummary, RowDeletionError, RowDeletionReport, SimilarImageMatch,
    StorageError, XlsxExportError, XlsxExportOutcome,
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
        let directory = self
            .active
            .as_ref()
            .ok_or(AppRuntimeError::NotConfigured)?;
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
            Ok(None) => {
                match DataDirectory::initialize(&default_data_dir) {
                    Ok(dir) => match write_locator(&locator_path, dir.root()) {
                        Ok(()) => (Some(dir), None),
                        Err(error) => (None, Some(error.to_string())),
                    },
                    Err(error) => (None, Some(error.to_string())),
                }
            }
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
        // 导入走独立连接写库，常驻连接上的查询缓存必须失效。
        self.lock_state()?.invalidate_query_cache();
        Ok((self.snapshot()?, outcome))
    }

    pub(crate) fn update_existing_images(
        &self,
        path: impl AsRef<Path>,
        progress: impl Fn(ImageImportProgress) + Sync,
    ) -> Result<(RuntimeSnapshot, ExistingImageUpdateOutcome), AppRuntimeError> {
        let state = self.lock_state()?;
        ensure_startup_valid(&state)?;
        let directory = state
            .active
            .as_ref()
            .ok_or(AppRuntimeError::NotConfigured)?
            .clone();
        drop(state);
        let outcome = directory.update_existing_images(path.as_ref(), progress)?;
        self.lock_state()?.invalidate_query_cache();
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

    pub(crate) fn set_auto_artist_prefix_on_import(
        &self,
        enabled: bool,
    ) -> Result<RuntimeSnapshot, AppRuntimeError> {
        self.with_database_mut(|database| {
            database.set_auto_artist_prefix_on_import(enabled)
        })?;
        self.snapshot()
    }

    #[cfg(test)]
    pub(crate) fn query_rows(&self, query: &RowQuery) -> Result<RowPage, AppRuntimeError> {
        self.with_database(|db| db.query_rows(query))
    }

    pub(crate) fn query_rows_sorted(
        &self,
        query: &RowQuery,
        sort: SortMode,
    ) -> Result<RowPage, AppRuntimeError> {
        self.with_database(|db| db.query_rows_sorted(query, sort))
    }

    pub(crate) fn get_rows_by_ids(&self, ids: &[i64]) -> Result<Vec<crate::db::RowRecord>, AppRuntimeError> {
        self.with_database(|db| db.get_rows_by_ids(ids))
    }

    pub(crate) fn row_index_by_id_sorted(
        &self,
        row_id: i64,
        sort: SortMode,
    ) -> Result<u64, AppRuntimeError> {
        self.with_database(|db| db.row_index_by_id_sorted(row_id, sort))
    }

    pub(crate) fn list_tags(&self) -> Result<Vec<TagSummary>, AppRuntimeError> {
        self.with_database(|db| db.list_tags())
    }

    pub(crate) fn delete_tag(&self, name: &str) -> Result<bool, AppRuntimeError> {
        self.with_database_mut(|db| db.delete_tag(name))
    }

    pub(crate) fn rename_tag(&self, old_name: &str, new_name: &str) -> Result<bool, AppRuntimeError> {
        self.with_database_mut(|db| db.rename_tag(old_name, new_name))
    }

    pub(crate) fn create_tag(&self, name: &str) -> Result<bool, AppRuntimeError> {
        self.with_database_mut(|db| db.create_tag(name))
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
        self.with_database_mut(|db| db.add_tags_to_selection(selection, tags))
    }

    pub(crate) fn remove_tags_from_selection(
        &self,
        selection: &RowSelection,
        tags: &[String],
    ) -> Result<TagMutationResult, AppRuntimeError> {
        self.with_database_mut(|db| db.remove_tags_from_selection(selection, tags))
    }

    pub(crate) fn set_tags_for_row(
        &self,
        row_id: i64,
        tags: &[String],
    ) -> Result<TagMutationResult, AppRuntimeError> {
        self.with_database_mut(|db| db.set_tags_for_row(row_id, tags))
    }

    pub(crate) fn list_automation_rules(&self) -> Result<Vec<AutomationRule>, AppRuntimeError> {
        self.with_database(|db| db.list_automation_rules())
    }

    pub(crate) fn inspect_automation_rule_file(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<AutomationRuleImportInspection, AppRuntimeError> {
        let (document, content_hash) = read_automation_rule_file(path.as_ref())?;
        self.with_database(|db| db.inspect_automation_rule_document(&document, content_hash))
    }

    pub(crate) fn import_automation_rule_file(
        &self,
        path: impl AsRef<Path>,
        expected_hash: &str,
    ) -> Result<AutomationRuleImportResult, AppRuntimeError> {
        let (document, content_hash) = read_automation_rule_file(path.as_ref())?;
        if content_hash != expected_hash {
            return Err(AutomationRuleError::InvalidRuleFile(
                "文件在预览后发生了变化，请重新选择并检查".into(),
            )
            .into());
        }
        self.with_database_mut(|db| db.import_automation_rule_document(&document))
    }

    pub(crate) fn inspect_automation_rule_text(
        &self,
        text: &str,
    ) -> Result<AutomationRuleImportInspection, AppRuntimeError> {
        let (document, content_hash) = parse_automation_rule_text(text)?;
        self.with_database(|db| db.inspect_automation_rule_document(&document, content_hash))
    }

    pub(crate) fn import_automation_rule_text(
        &self,
        text: &str,
        expected_hash: &str,
    ) -> Result<AutomationRuleImportResult, AppRuntimeError> {
        let (document, content_hash) = parse_automation_rule_text(text)?;
        if content_hash != expected_hash {
            return Err(AutomationRuleError::InvalidRuleFile(
                "文本在预览后发生了变化，请重新检查".into(),
            )
            .into());
        }
        self.with_database_mut(|db| db.import_automation_rule_document(&document))
    }

    pub(crate) fn export_automation_rules(
        &self,
        path: impl AsRef<Path>,
        ids: &[i64],
    ) -> Result<AutomationRuleExportResult, AppRuntimeError> {
        let path = path.as_ref();
        let document = self.with_database(|db| db.export_automation_rule_document(ids))?;
        write_automation_rule_file(path, &document)?;
        Ok(AutomationRuleExportResult {
            path: path.to_string_lossy().into_owned(),
            exported_rules: u32::try_from(ids.len())
                .map_err(|_| crate::db::DatabaseError::CountOverflow)?,
        })
    }

    pub(crate) fn create_automation_rule(
        &self,
        draft: &AutomationRuleDraft,
    ) -> Result<AutomationRule, AppRuntimeError> {
        self.with_database(|db| db.create_automation_rule(draft))
    }

    pub(crate) fn update_automation_rule(
        &self,
        id: i64,
        draft: &AutomationRuleDraft,
    ) -> Result<AutomationRule, AppRuntimeError> {
        self.with_database(|db| db.update_automation_rule(id, draft))
    }

    pub(crate) fn set_automation_rule_enabled(
        &self,
        id: i64,
        enabled: bool,
    ) -> Result<(), AppRuntimeError> {
        self.with_database(|db| db.set_automation_rule_enabled(id, enabled))
    }

    pub(crate) fn delete_automation_rule(&self, id: i64) -> Result<bool, AppRuntimeError> {
        self.with_database(|db| db.delete_automation_rule(id))
    }

    pub(crate) fn reorder_automation_rules(&self, ids: &[i64]) -> Result<(), AppRuntimeError> {
        self.with_database(|db| db.reorder_automation_rules(ids))
    }

    pub(crate) fn preview_automation_rule(&self, id: i64) -> Result<RulePreview, AppRuntimeError> {
        self.with_cloned_database(|db| db.preview_automation_rule(id))
    }

    pub(crate) fn preview_automation_rule_draft(
        &self,
        draft: &AutomationRuleDraft,
    ) -> Result<RulePreview, AppRuntimeError> {
        self.with_cloned_database(|db| db.preview_automation_rule_draft(draft))
    }

    pub(crate) fn run_automation_rule_on_library(
        &self,
        id: i64,
    ) -> Result<RuleExecutionSummary, AppRuntimeError> {
        self.with_cloned_database_mut(|db| db.run_automation_rule_on_library(id))
    }

    pub(crate) fn preview_quick_tag(
        &self,
        condition: &QuickEditCondition,
        tags: &[String],
    ) -> Result<QuickTagPreview, AppRuntimeError> {
        self.with_cloned_database(|db| db.preview_quick_tag(condition, tags))
    }

    pub(crate) fn apply_quick_tag(
        &self,
        condition: &QuickEditCondition,
        tags: &[String],
    ) -> Result<QuickTagApplyResult, AppRuntimeError> {
        self.with_cloned_database_mut(|db| db.apply_quick_tag(condition, tags))
    }

    pub(crate) fn revert_quick_tag_changes(
        &self,
        changes: &[QuickTagAssociation],
    ) -> Result<u64, AppRuntimeError> {
        self.with_database_mut(|db| db.revert_quick_tag_changes(changes))
    }

    pub(crate) fn reapply_quick_tag_changes(
        &self,
        changes: &[QuickTagAssociation],
    ) -> Result<u64, AppRuntimeError> {
        self.with_database_mut(|db| db.reapply_quick_tag_changes(changes))
    }

    pub(crate) fn preview_quick_group(
        &self,
        condition: &QuickEditCondition,
        group_id: i64,
        only_ungrouped: bool,
    ) -> Result<QuickGroupPreview, AppRuntimeError> {
        self.with_cloned_database(|db| db.preview_quick_group(condition, group_id, only_ungrouped))
    }

    pub(crate) fn apply_quick_group(
        &self,
        condition: &QuickEditCondition,
        group_id: i64,
        only_ungrouped: bool,
    ) -> Result<QuickGroupApplyResult, AppRuntimeError> {
        self.with_cloned_database_mut(|db| {
            db.apply_quick_group(condition, group_id, only_ungrouped)
        })
    }

    pub(crate) fn revert_quick_group_changes(
        &self,
        changes: &[QuickGroupChange],
    ) -> Result<u64, AppRuntimeError> {
        self.with_database_mut(|db| db.revert_quick_group_changes(changes))
    }

    pub(crate) fn reapply_quick_group_changes(
        &self,
        changes: &[QuickGroupChange],
    ) -> Result<u64, AppRuntimeError> {
        self.with_database_mut(|db| db.reapply_quick_group_changes(changes))
    }

    pub(crate) fn preview_quick_artist_prefix(
        &self,
        artist_name: &str,
    ) -> Result<QuickArtistPrefixPreview, AppRuntimeError> {
        self.with_cloned_database(|db| db.preview_quick_artist_prefix(artist_name))
    }

    pub(crate) fn apply_quick_artist_prefix(
        &self,
        artist_name: &str,
    ) -> Result<QuickArtistPrefixApplyResult, AppRuntimeError> {
        self.with_cloned_database_mut(|db| db.apply_quick_artist_prefix(artist_name))
    }

    pub(crate) fn revert_quick_artist_prefix_changes(
        &self,
        changes: &[QuickArtistPrefixChange],
    ) -> Result<u64, AppRuntimeError> {
        self.with_database_mut(|db| db.revert_quick_artist_prefix_changes(changes))
    }

    pub(crate) fn reapply_quick_artist_prefix_changes(
        &self,
        changes: &[QuickArtistPrefixChange],
    ) -> Result<u64, AppRuntimeError> {
        self.with_database_mut(|db| db.reapply_quick_artist_prefix_changes(changes))
    }

    pub(crate) fn preview_auto_artist_prefix(
        &self,
    ) -> Result<AutoArtistPrefixPreview, AppRuntimeError> {
        self.with_cloned_database(|db| db.preview_auto_artist_prefix())
    }

    pub(crate) fn apply_auto_artist_prefix(
        &self,
        selected_names: &[String],
    ) -> Result<AutoArtistPrefixApplyResult, AppRuntimeError> {
        self.with_cloned_database_mut(|db| db.apply_auto_artist_prefix(selected_names))
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
        // 删除走独立连接写库，常驻连接上的查询缓存必须失效。
        self.lock_state()?.invalidate_query_cache();
        Ok((self.snapshot()?, report))
    }

    pub(crate) fn undo_import_batch(
        &self,
        batch_id: i64,
    ) -> Result<(RuntimeSnapshot, RowDeletionReport), AppRuntimeError> {
        let row_ids = self.with_database(|db| db.row_ids_for_batch(batch_id))?;
        let (_, report) = self.delete_rows(
            &RowSelection::Explicit { row_ids },
            false,
        )?;
        let removed = self.with_database_mut(|db| db.delete_batch_if_empty(batch_id))?;
        if !removed {
            return Err(crate::db::DatabaseError::BatchNotFound(batch_id).into());
        }
        // delete_rows 后的快照仍包含空批次，删除批次后重新取摘要。
        Ok((self.snapshot()?, report))
    }

    pub(crate) fn restore_mutable_row_states(
        &self,
        states: &[MutableRowState],
    ) -> Result<u64, AppRuntimeError> {
        self.with_database_mut(|db| db.restore_mutable_row_states(states))
    }

    pub(crate) fn list_batches(&self) -> Result<Vec<BatchSummary>, AppRuntimeError> {
        self.with_database(|db| db.list_batches())
    }

    pub(crate) fn create_group(&self, name: &str) -> Result<GroupSummary, AppRuntimeError> {
        self.with_database_mut(|db| db.create_group(name))
    }

    pub(crate) fn restore_group(
        &self,
        group: &GroupSummary,
    ) -> Result<GroupSummary, AppRuntimeError> {
        self.with_database_mut(|db| db.restore_group(group))
    }

    pub(crate) fn rename_group(
        &self,
        group_id: i64,
        new_name: &str,
    ) -> Result<GroupSummary, AppRuntimeError> {
        self.with_database_mut(|db| db.rename_group(group_id, new_name))
    }

    pub(crate) fn delete_group(&self, group_id: i64) -> Result<bool, AppRuntimeError> {
        self.with_database_mut(|db| db.delete_group(group_id))
    }

    pub(crate) fn delete_empty_groups(&self) -> Result<u64, AppRuntimeError> {
        self.with_database_mut(|db| db.delete_empty_groups())
    }

    pub(crate) fn list_groups(&self) -> Result<Vec<GroupSummary>, AppRuntimeError> {
        self.with_database(|db| db.list_groups())
    }

    pub(crate) fn assign_rows_to_group(
        &self,
        selection: &RowSelection,
        group_id: i64,
    ) -> Result<u64, AppRuntimeError> {
        self.with_database_mut(|db| db.assign_rows_to_group(selection, group_id))
    }

    pub(crate) fn ungroup_rows(
        &self,
        selection: &RowSelection,
    ) -> Result<u64, AppRuntimeError> {
        self.with_database_mut(|db| db.ungroup_rows(selection))
    }

    pub(crate) fn update_positive_prompt(
        &self,
        row_id: i64,
        new_prompt: &str,
    ) -> Result<crate::db::SinglePromptEditResult, AppRuntimeError> {
        self.with_database_mut(|db| db.update_positive_prompt(row_id, new_prompt))
    }

    pub(crate) fn update_negative_prompt(
        &self,
        row_id: i64,
        new_prompt: &str,
    ) -> Result<u64, AppRuntimeError> {
        self.with_database_mut(|db| db.update_negative_prompt(row_id, new_prompt))
    }

    pub(crate) fn update_note(
        &self,
        row_id: i64,
        note: &str,
    ) -> Result<u64, AppRuntimeError> {
        self.with_database_mut(|db| db.update_note(row_id, note))
    }

    pub(crate) fn update_character_prompt(
        &self,
        row_id: i64,
        new_prompt: &str,
    ) -> Result<crate::db::SinglePromptEditResult, AppRuntimeError> {
        self.with_database_mut(|db| db.update_character_prompt(row_id, new_prompt))
    }

    pub(crate) fn find_replace_prompt(
        &self,
        selection: &RowSelection,
        find: &str,
        replace: &str,
    ) -> Result<crate::db::PromptEditResult, AppRuntimeError> {
        self.with_database_mut(|db| db.find_replace_prompt(selection, find, replace))
    }

    pub(crate) fn prepend_artist(
        &self,
        selection: &RowSelection,
        artist_name: &str,
    ) -> Result<crate::db::PromptEditResult, AppRuntimeError> {
        self.with_database_mut(|db| db.prepend_artist(selection, artist_name))
    }

    pub(crate) fn get_group_members(
        &self,
        group_id: i64,
        offset: u64,
        limit: u32,
    ) -> Result<RowPage, AppRuntimeError> {
        self.with_database(|db| db.get_group_members(group_id, offset, limit))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn list_dedupe_clusters(
        &self,
        dedupe: DedupeMode,
        tags: &[String],
        tag_mode: TagMatchMode,
        single_artist_only: bool,
        has_vibe: bool,
        untagged_only: bool,
        hide_grouped: bool,
    ) -> Result<Vec<DedupeCluster>, AppRuntimeError> {
        self.with_database(|db| {
            db.list_dedupe_clusters(
                dedupe,
                tags,
                tag_mode,
                single_artist_only,
                has_vibe,
                untagged_only,
                hide_grouped,
            )
        })
    }

    pub(crate) fn list_distinct_artists(&self) -> Result<Vec<String>, AppRuntimeError> {
        self.with_database(|db| db.list_distinct_artists())
    }

    pub(crate) fn row_ids_with_artists(
        &self,
        artists: &str,
    ) -> Result<Vec<i64>, AppRuntimeError> {
        self.with_database(|db| db.row_ids_with_artists(artists))
    }

    pub(crate) fn get_custom_artists(&self) -> Result<String, AppRuntimeError> {
        self.with_database(|db| db.setting("custom-artists").map(Option::unwrap_or_default))
    }

    pub(crate) fn set_custom_artists(&self, text: &str) -> Result<(), AppRuntimeError> {
        self.with_database(|db| db.set_setting("custom-artists", text))
    }

    pub(crate) fn get_recent_tags(&self) -> Result<String, AppRuntimeError> {
        self.with_database(|db| db.setting("recent-tags").map(Option::unwrap_or_default))
    }

    pub(crate) fn set_recent_tags(&self, json: &str) -> Result<(), AppRuntimeError> {
        self.with_database(|db| db.set_setting("recent-tags", json))
    }

    pub(crate) fn list_prompt_docs(&self) -> Result<Vec<PromptDocSummary>, AppRuntimeError> {
        Ok(self.active_directory()?.list_prompt_docs()?)
    }

    pub(crate) fn create_prompt_doc(
        &self,
        title: &str,
    ) -> Result<PromptDocDetail, AppRuntimeError> {
        Ok(self.active_directory()?.create_prompt_doc(title)?)
    }

    pub(crate) fn load_prompt_doc(&self, doc_id: &str) -> Result<PromptDocDetail, AppRuntimeError> {
        Ok(self.active_directory()?.load_prompt_doc(doc_id)?)
    }

    pub(crate) fn save_prompt_doc(
        &self,
        doc_id: &str,
        title: &str,
        content: &serde_json::Value,
        plain_text: &str,
    ) -> Result<PromptDocDetail, AppRuntimeError> {
        Ok(self
            .active_directory()?
            .save_prompt_doc(doc_id, title, content, plain_text)?)
    }

    pub(crate) fn delete_prompt_doc(&self, doc_id: &str) -> Result<(), AppRuntimeError> {
        Ok(self.active_directory()?.delete_prompt_doc(doc_id)?)
    }

    pub(crate) fn import_prompt_doc_image_from_path(
        &self,
        doc_id: &str,
        path: impl AsRef<Path>,
    ) -> Result<PromptDocAsset, AppRuntimeError> {
        Ok(self
            .active_directory()?
            .import_prompt_doc_image_from_path(doc_id, path)?)
    }

    pub(crate) fn import_prompt_doc_image_bytes(
        &self,
        doc_id: &str,
        file_name: &str,
        bytes: &[u8],
    ) -> Result<PromptDocAsset, AppRuntimeError> {
        Ok(self
            .active_directory()?
            .import_prompt_doc_image_bytes(doc_id, file_name, bytes)?)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn get_dedupe_cluster_members(
        &self,
        dedupe: DedupeMode,
        key: &str,
        tags: &[String],
        tag_mode: TagMatchMode,
        single_artist_only: bool,
        has_vibe: bool,
        untagged_only: bool,
        hide_grouped: bool,
        offset: u64,
        limit: u32,
    ) -> Result<RowPage, AppRuntimeError> {
        self.with_database(|db| {
            db.get_dedupe_cluster_members(
                dedupe,
                key,
                tags,
                tag_mode,
                single_artist_only,
                has_vibe,
                untagged_only,
                hide_grouped,
                offset,
                limit,
            )
        })
    }

    pub(crate) fn set_dedupe_alias(
        &self,
        mode: DedupeMode,
        key: &str,
        alias: &str,
    ) -> Result<(), AppRuntimeError> {
        self.with_database_mut(|db| db.set_dedupe_alias(mode, key, alias))
    }

    pub(crate) fn row_thumbnail(&self, row_id: i64) -> Result<Vec<u8>, AppRuntimeError> {
        self.row_image(row_id, ImageVariant::Thumbnail)
    }

    pub(crate) fn row_gallery_preview(&self, row_id: i64) -> Result<Vec<u8>, AppRuntimeError> {
        self.row_image(row_id, ImageVariant::GalleryPreview)
    }

    pub(crate) fn row_preview(&self, row_id: i64) -> Result<Vec<u8>, AppRuntimeError> {
        self.row_image(row_id, ImageVariant::Preview)
    }

    pub(crate) fn row_original(&self, row_id: i64) -> Result<Vec<u8>, AppRuntimeError> {
        self.row_image(row_id, ImageVariant::Original)
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
        use_numeric_names_for_empty: bool,
        progress: impl Fn(JsonExportProgress) + Sync,
    ) -> Result<JsonExportOutcome, AppRuntimeError> {
        let directory = self.active_directory()?;
        Ok(directory.export_zhihuiji_json(
            selection,
            destination,
            use_numeric_names_for_empty,
            progress,
        )?)
    }

    pub(crate) fn inspect_zhihuiji_export_notes(
        &self,
        selection: &RowSelection,
    ) -> Result<(usize, usize), AppRuntimeError> {
        let rows = self.with_database(|database| database.export_rows(selection))?;
        let empty_notes = rows
            .iter()
            .filter(|row| row.note.as_deref().is_none_or(|note| note.trim().is_empty()))
            .count();
        Ok((rows.len(), empty_notes))
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

    pub(crate) fn export_selected_images(
        &self,
        selection: &RowSelection,
        extra_sources: &[PathBuf],
        parent_dir: impl AsRef<Path>,
        naming: ImageFileNaming,
        strip_metadata: bool,
        progress: impl Fn(ImageFilesProgress) + Sync,
    ) -> Result<ImageFilesExportOutcome, AppRuntimeError> {
        let directory = self.active_directory()?;
        Ok(directory.export_selected_images(
            selection,
            extra_sources,
            parent_dir,
            naming,
            strip_metadata,
            progress,
        )?)
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

    /// 升级后首启为历史图片补齐 VIBE 数量与组合签名。写库走独立连接，
    /// 完成后失效常驻连接的查询缓存，让重复项视图立即看到新签名。
    pub(crate) fn backfill_vibe_statuses(
        &self,
        progress: impl Fn(crate::storage::VibeStatusProgress),
    ) -> Result<crate::storage::VibeStatusProgress, AppRuntimeError> {
        let directory = self.active_directory()?;
        let outcome = directory.backfill_vibe_statuses(progress)?;
        if outcome.total > 0 {
            self.lock_state()?.invalidate_query_cache();
        }
        Ok(outcome)
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
        // 迁移会复制并清理旧目录文件，必须先关闭常驻连接释放文件句柄。
        state.database = None;
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

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn auto_initializes_and_reloads_configured_directory() {
        let temporary = TemporaryRuntime::new();
        let runtime = AppRuntime::load(temporary.locator.clone(), temporary.data.clone());

        let snapshot = runtime.snapshot().unwrap();
        let reloaded = AppRuntime::load(temporary.locator.clone(), temporary.data.clone())
            .snapshot()
            .unwrap();

        assert_eq!(snapshot.data_directory, reloaded.data_directory);
        assert_eq!(reloaded.data_directory, Some(temporary.data.clone()));
        let library = reloaded.library.unwrap();
        assert_eq!(library.row_count, 0);
        assert_eq!(library.batch_count, 0);
        assert!(reloaded.startup_error.is_none());
    }

    #[test]
    fn refuses_pointer_switch_after_auto_init() {
        let temporary = TemporaryRuntime::new();
        let runtime = AppRuntime::load(temporary.locator.clone(), temporary.data.clone());

        let error = runtime
            .initialize_directory(temporary.root.join("other-data"))
            .unwrap_err();

        assert!(matches!(error, AppRuntimeError::AlreadyConfigured));
        assert!(!temporary.root.join("other-data").exists());
    }

    #[test]
    fn auto_init_backfills_content_hashes_for_legacy_rows() {
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

        let runtime = AppRuntime::load(temporary.locator.clone(), temporary.data.clone());
        let snapshot = runtime.snapshot().unwrap();

        assert_eq!(snapshot.library.unwrap().row_count, 1);
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
    fn imports_images_and_restores_summary_after_reload() {
        let temporary = TemporaryRuntime::new();
        let runtime = AppRuntime::load(temporary.locator.clone(), temporary.data.clone());
        let folder = crate::storage::test_fixtures::sample_image_folder(&temporary.root, 5);

        let (_, outcome) = runtime.import_images(&folder, |_| {}).unwrap();
        let reloaded = AppRuntime::load(temporary.locator.clone(), temporary.data.clone())
            .snapshot()
            .unwrap();

        assert_eq!(outcome.added, 5);
        let library = reloaded.library.unwrap();
        assert_eq!(library.row_count, 5);
        assert_eq!(library.batch_count, 1);
        let last_batch = library.last_batch.unwrap();
        assert!(last_batch.source_path.contains("sample-images"));
        assert_eq!(last_batch.added_count, 5);
    }

    #[test]
    fn undo_import_batch_removes_only_library_copies_and_batch_record() {
        let temporary = TemporaryRuntime::new();
        let runtime = AppRuntime::load(temporary.locator.clone(), temporary.data.clone());
        let folder = crate::storage::test_fixtures::sample_image_folder(&temporary.root, 3);
        let original = folder.join("sample-1.png");

        let (_, outcome) = runtime.import_images(&folder, |_| {}).unwrap();
        let (snapshot, report) = runtime.undo_import_batch(outcome.batch_id).unwrap();

        assert_eq!(report.deleted_rows, 3);
        assert_eq!(report.trashed_original_files, 0);
        assert!(original.is_file());
        let library = snapshot.library.unwrap();
        assert_eq!(library.row_count, 0);
        assert_eq!(library.batch_count, 0);
        assert!(library.last_batch.is_none());

        let (redone, outcome) = runtime.import_images(&folder, |_| {}).unwrap();
        assert_eq!(outcome.added, 3);
        let library = redone.library.unwrap();
        assert_eq!(library.row_count, 3);
        assert_eq!(library.batch_count, 1);
    }

    #[test]
    fn appends_second_import_and_deletes_rows() {
        let temporary = TemporaryRuntime::new();
        let runtime = AppRuntime::load(temporary.locator.clone(), temporary.data.clone());
        let folder = crate::storage::test_fixtures::sample_image_folder(&temporary.root, 5);
        runtime.import_images(&folder, |_| {}).unwrap();

        // 重复导入：全部跳过，行数不变。
        let (snapshot, outcome) = runtime.import_images(&folder, |_| {}).unwrap();
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
        let runtime = AppRuntime::load(temporary.locator.clone(), temporary.data.clone());
        let folder = crate::storage::test_fixtures::sample_image_folder(&temporary.root, 5);
        runtime.import_images(&folder, |_| {}).unwrap();

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
            has_vibe: false,
            untagged_only: false,
            search: String::new(),
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
    fn loads_all_image_tiers_for_imported_row() {
        let temporary = TemporaryRuntime::new();
        let runtime = AppRuntime::load(temporary.locator.clone(), temporary.data.clone());
        let folder = crate::storage::test_fixtures::sample_image_folder(&temporary.root, 1);
        runtime.import_images(&folder, |_| {}).unwrap();

        let thumbnail = runtime.row_thumbnail(1).unwrap();
        let gallery = runtime.row_gallery_preview(1).unwrap();
        let preview = runtime.row_preview(1).unwrap();
        let original = runtime.row_original(1).unwrap();

        assert!(thumbnail.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(gallery.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(preview.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(original.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(thumbnail.len() <= preview.len());
    }

    #[test]
    fn migrates_directory_then_reloads_from_new_locator() {
        let temporary = TemporaryRuntime::new();
        let runtime = AppRuntime::load(temporary.locator.clone(), temporary.data.clone());
        let folder = crate::storage::test_fixtures::sample_image_folder(&temporary.root, 3);
        runtime.import_images(&folder, |_| {}).unwrap();
        runtime
            .add_tags_to_selection(
                &RowSelection::Explicit { row_ids: vec![1] },
                &["migrated".into()],
            )
            .unwrap();
        let destination = temporary.root.join("migrated-data");

        let outcome = runtime.migrate_directory(&destination).unwrap();
        let reloaded = AppRuntime::load(temporary.locator.clone(), temporary.data.clone());

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
        let runtime = AppRuntime::load(temporary.locator.clone(), temporary.data.clone());
        let folder = crate::storage::test_fixtures::sample_image_folder(&temporary.root, 2);
        runtime.import_images(&folder, |_| {}).unwrap();
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
            AppRuntime::load(temporary.locator.clone(), temporary.data.clone())
                .snapshot()
                .unwrap()
                .data_directory,
            Some(temporary.data.clone())
        );
    }

    #[test]
    fn query_cache_invalidates_across_tag_mutations_and_deletes() {
        let temporary = TemporaryRuntime::new();
        let runtime = AppRuntime::load(temporary.locator.clone(), temporary.data.clone());
        let folder = crate::storage::test_fixtures::sample_image_folder(&temporary.root, 3);
        runtime.import_images(&folder, |_| {}).unwrap();

        let tagged_query = RowQuery {
            offset: 0,
            limit: 100,
            tags: vec!["Keep".into()],
            tag_mode: crate::db::TagMatchMode::And,
            dedupe: crate::db::DedupeMode::None,
            single_artist_only: false,
            has_vibe: false,
            untagged_only: false,
            group_view: false,
            hide_grouped: false,
            search: String::new(),
        };
        assert_eq!(runtime.query_rows(&tagged_query).unwrap().total_count, 0);

        runtime
            .add_tags_to_selection(
                &RowSelection::Explicit { row_ids: vec![1, 2] },
                &["Keep".into()],
            )
            .unwrap();
        assert_eq!(runtime.query_rows(&tagged_query).unwrap().total_count, 2);

        runtime
            .delete_rows(&RowSelection::Explicit { row_ids: vec![1] }, false)
            .unwrap();
        assert_eq!(runtime.query_rows(&tagged_query).unwrap().total_count, 1);
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
