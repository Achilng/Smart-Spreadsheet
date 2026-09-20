use super::{AppRuntime, AppRuntimeError};
use crate::db::{RowSelection, TagMutationResult, TagSelectionSummary, TagSummary};

impl AppRuntime {
    pub(crate) fn list_tags(&self) -> Result<Vec<TagSummary>, AppRuntimeError> {
        self.with_database(|db| db.list_tags())
    }

    pub(crate) fn delete_tag(&self, name: &str) -> Result<bool, AppRuntimeError> {
        self.with_database_mut(|db| db.delete_tag(name))
    }

    pub(crate) fn rename_tag(
        &self,
        old_name: &str,
        new_name: &str,
    ) -> Result<bool, AppRuntimeError> {
        self.with_database_mut(|db| db.rename_tag(old_name, new_name))
    }

    pub(crate) fn create_tag(&self, name: &str) -> Result<bool, AppRuntimeError> {
        self.with_database_mut(|db| db.create_tag(name))
    }

    pub(crate) fn list_selection_tags(
        &self,
        selection: &RowSelection,
    ) -> Result<Vec<TagSelectionSummary>, AppRuntimeError> {
        self.with_database(|db| db.list_selection_tags(selection))
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

    pub(crate) fn get_recent_tags(&self) -> Result<String, AppRuntimeError> {
        self.with_database(|db| db.setting("recent-tags").map(Option::unwrap_or_default))
    }

    pub(crate) fn set_recent_tags(&self, json: &str) -> Result<(), AppRuntimeError> {
        self.with_database(|db| db.set_setting("recent-tags", json))
    }
}
