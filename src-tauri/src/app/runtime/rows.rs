use super::{AppRuntime, AppRuntimeError};
use crate::db::{RowPage, RowQuery, RowSelection, SortMode};

impl AppRuntime {
    #[cfg(test)]
    pub(crate) fn query_rows(&self, query: &RowQuery) -> Result<RowPage, AppRuntimeError> {
        self.with_database(|db| db.query_rows(query))
    }

    pub(crate) fn query_rows_sorted(
        &self,
        query: &RowQuery,
        sort: SortMode,
    ) -> Result<RowPage, AppRuntimeError> {
        self.with_database(|db| db.query_rows_sorted(query, sort))
    }

    pub(crate) fn get_rows_by_ids(
        &self,
        ids: &[i64],
    ) -> Result<Vec<crate::db::RowRecord>, AppRuntimeError> {
        self.with_database(|db| db.get_rows_by_ids(ids))
    }

    pub(crate) fn row_index_by_id_sorted(
        &self,
        row_id: i64,
        sort: SortMode,
    ) -> Result<u64, AppRuntimeError> {
        self.with_database(|db| db.row_index_by_id_sorted(row_id, sort))
    }

    pub(crate) fn count_selected_rows(
        &self,
        selection: &RowSelection,
    ) -> Result<u64, AppRuntimeError> {
        self.with_database(|db| db.count_selected_rows(selection))
    }

    pub(crate) fn selected_row_ids(
        &self,
        selection: &RowSelection,
    ) -> Result<Vec<i64>, AppRuntimeError> {
        self.with_database(|db| db.selected_row_ids(selection))
    }
}
