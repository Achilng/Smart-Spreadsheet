use super::{AppRuntime, AppRuntimeError, RuntimeSnapshot, ensure_startup_valid};
use crate::db::{BatchSummary, RowSelection};
use crate::storage::{
    ExistingImageUpdateOutcome, ImageImportOutcome, ImageImportProgress, RowDeletionReport,
};
use std::path::Path;

impl AppRuntime {
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
        let (_, report) = self.delete_rows(&RowSelection::Explicit { row_ids }, false)?;
        let removed = self.with_database_mut(|db| db.delete_batch_if_empty(batch_id))?;
        if !removed {
            return Err(crate::db::DatabaseError::BatchNotFound(batch_id).into());
        }
        // delete_rows 后的快照仍包含空批次，删除批次后重新取摘要。
        Ok((self.snapshot()?, report))
    }

    pub(crate) fn list_batches(&self) -> Result<Vec<BatchSummary>, AppRuntimeError> {
        self.with_database(|db| db.list_batches())
    }
}
