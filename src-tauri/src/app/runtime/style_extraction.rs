use super::{AppRuntime, AppRuntimeError};
use crate::db::{RowSelection, style_extraction::*};
use std::path::Path;

impl AppRuntime {
    pub(crate) fn export_style_request(
        &self,
        selection: Option<&RowSelection>,
        include_processed: bool,
        path: &Path,
    ) -> Result<ExportSummary, AppRuntimeError> {
        let (document, summary) =
            self.with_database(|db| db.export_style_request(selection, include_processed))?;
        std::fs::write(path, serde_json::to_vec_pretty(&document)?)?;
        Ok(summary)
    }
    pub(crate) fn preview_style_result(
        &self,
        path: &Path,
    ) -> Result<ImportPreview, AppRuntimeError> {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 64 * 1024 * 1024 {
            return Err(crate::db::DatabaseError::IntegrityCheckFailed(
                "结果文件超过 64 MB".into(),
            )
            .into());
        }
        let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
        let doc: ResultDocument = serde_json::from_slice(bytes)?;
        self.with_database(|db| db.preview_style_result(&doc))
    }
    pub(crate) fn apply_style_changes(
        &self,
        library_id: &str,
        changes: &[StyleChange],
        reverse: bool,
        strict: bool,
    ) -> Result<ApplyResult, AppRuntimeError> {
        self.with_database_mut(|db| db.apply_style_changes(library_id, changes, reverse, strict))
    }
}
