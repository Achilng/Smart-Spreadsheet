use super::{AppRuntime, AppRuntimeError};

impl AppRuntime {
    pub(crate) fn list_distinct_artists(&self) -> Result<Vec<String>, AppRuntimeError> {
        self.with_database(|db| db.list_distinct_artists())
    }

    pub(crate) fn row_ids_with_artists(&self, artists: &str) -> Result<Vec<i64>, AppRuntimeError> {
        self.with_database(|db| db.row_ids_with_artists(artists))
    }

    pub(crate) fn get_custom_artists(&self) -> Result<String, AppRuntimeError> {
        self.with_database(|db| db.setting("custom-artists").map(Option::unwrap_or_default))
    }

    pub(crate) fn set_custom_artists(&self, text: &str) -> Result<(), AppRuntimeError> {
        self.with_database(|db| db.set_setting("custom-artists", text))
    }
}
