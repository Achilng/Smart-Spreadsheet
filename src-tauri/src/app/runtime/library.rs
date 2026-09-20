use super::{AppRuntime, AppRuntimeError, RuntimeSnapshot};
use std::path::Path;

impl AppRuntime {
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
        self.with_database_mut(|database| database.set_auto_artist_prefix_on_import(enabled))?;
        self.snapshot()
    }
}
