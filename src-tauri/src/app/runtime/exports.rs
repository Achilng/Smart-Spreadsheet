use super::{AppRuntime, AppRuntimeError};
use crate::db::RowSelection;
use crate::storage::{
    ExportProgress, ImageFileExportMode, ImageFileNaming, ImageFilesExportOutcome,
    ImageFilesProgress, JsonExportOptions, JsonExportOutcome, JsonExportProgress,
    PromptRotationJsonExportOutcome, XlsxExportOutcome,
};
use std::path::{Path, PathBuf};

impl AppRuntime {
    pub(crate) fn image_export_settings(
        &self,
    ) -> Result<crate::db::ImageExportSettings, AppRuntimeError> {
        self.with_database(|db| db.image_export_settings())
    }

    pub(crate) fn set_image_export_settings(
        &self,
        settings: &crate::db::ImageExportSettings,
    ) -> Result<(), AppRuntimeError> {
        self.with_database(|db| db.set_image_export_settings(settings))
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
        options: JsonExportOptions,
        progress: impl Fn(JsonExportProgress) + Sync,
    ) -> Result<JsonExportOutcome, AppRuntimeError> {
        let directory = self.active_directory()?;
        Ok(directory.export_zhihuiji_json(selection, destination, options, progress)?)
    }

    pub(crate) fn export_prompt_rotation_json(
        &self,
        selection: &RowSelection,
        destination: impl AsRef<Path>,
        progress: impl Fn(JsonExportProgress) + Sync,
    ) -> Result<PromptRotationJsonExportOutcome, AppRuntimeError> {
        let directory = self.active_directory()?;
        Ok(directory.export_prompt_rotation_json(selection, destination, progress)?)
    }

    pub(crate) fn inspect_zhihuiji_export_notes(
        &self,
        selection: &RowSelection,
    ) -> Result<(usize, usize), AppRuntimeError> {
        let rows = self.with_database(|database| database.export_rows(selection))?;
        let empty_notes = rows
            .iter()
            .filter(|row| {
                row.note
                    .as_deref()
                    .is_none_or(|note| note.trim().is_empty())
            })
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
}
