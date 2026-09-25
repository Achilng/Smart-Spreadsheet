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
        let bytes = std::fs::read(path)?;
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
