use super::{AppRuntime, AppRuntimeError};
use crate::db::{MutableRowState, RowSelection};

impl AppRuntime {
    pub(crate) fn restore_mutable_row_states(
        &self,
        states: &[MutableRowState],
    ) -> Result<u64, AppRuntimeError> {
        self.with_database_mut(|db| db.restore_mutable_row_states(states))
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

    pub(crate) fn update_note(&self, row_id: i64, note: &str) -> Result<u64, AppRuntimeError> {
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
}
