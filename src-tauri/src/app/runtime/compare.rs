use super::{AppRuntime, AppRuntimeError};

impl AppRuntime {
    pub(crate) fn get_compare_sample(
        &self,
        row_id: i64,
    ) -> Result<crate::db::CompareSample, AppRuntimeError> {
        self.with_database(|db| db.get_compare_sample(row_id))
    }

    pub(crate) fn query_compare_same_artists(
        &self,
        row_id: i64,
        offset: u64,
        limit: u32,
    ) -> Result<crate::db::CompareSectionPage, AppRuntimeError> {
        self.with_database(|db| db.query_compare_same_artists(row_id, offset, limit))
    }

    pub(crate) fn query_compare_same_vibe_diff_style(
        &self,
        row_id: i64,
        offset: u64,
        limit: u32,
    ) -> Result<crate::db::CompareSectionPage, AppRuntimeError> {
        self.with_database(|db| db.query_compare_same_vibe_diff_style(row_id, offset, limit))
    }

    pub(crate) fn query_compare_same_style_diff_vibe(
        &self,
        row_id: i64,
        offset: u64,
        limit: u32,
    ) -> Result<crate::db::CompareSectionPage, AppRuntimeError> {
        self.with_database(|db| db.query_compare_same_style_diff_vibe(row_id, offset, limit))
    }

    pub(crate) fn query_compare_same_style_all_models(
        &self,
        row_id: i64,
    ) -> Result<crate::db::CompareModelSection, AppRuntimeError> {
        self.with_database(|db| db.query_compare_same_style_all_models(row_id))
    }
}
