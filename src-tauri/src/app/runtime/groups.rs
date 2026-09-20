use super::{AppRuntime, AppRuntimeError};
use crate::db::{GroupSummary, RowPage, RowSelection};

impl AppRuntime {
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

    pub(crate) fn ungroup_rows(&self, selection: &RowSelection) -> Result<u64, AppRuntimeError> {
        self.with_database_mut(|db| db.ungroup_rows(selection))
    }

    pub(crate) fn get_group_members(
        &self,
        group_id: i64,
        offset: u64,
        limit: u32,
    ) -> Result<RowPage, AppRuntimeError> {
        self.with_database(|db| db.get_group_members(group_id, offset, limit))
    }
}
