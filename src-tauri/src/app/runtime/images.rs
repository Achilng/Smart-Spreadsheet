use super::{AppRuntime, AppRuntimeError, ensure_startup_valid};
use crate::images::ImageVariant;
use crate::storage::SimilarImageMatch;
use std::path::Path;

impl AppRuntime {
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

    pub(crate) fn search_similar_images(
        &self,
        query_path: impl AsRef<Path>,
        threshold: u32,
    ) -> Result<Vec<SimilarImageMatch>, AppRuntimeError> {
        let directory = self.active_directory()?;
        Ok(directory.search_similar_images(query_path.as_ref(), threshold)?)
    }
}
