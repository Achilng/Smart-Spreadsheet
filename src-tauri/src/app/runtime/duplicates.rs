use super::{AppRuntime, AppRuntimeError};
use crate::db::{DedupeCluster, DedupeMode, LibraryFilter, RowPage, TagMatchMode};

impl AppRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn list_dedupe_clusters(
        &self,
        dedupe: DedupeMode,
        tags: &[String],
        tag_mode: TagMatchMode,
        single_artist_only: bool,
        has_vibe: bool,
        untagged_only: bool,
        filters: &[LibraryFilter],
        hide_grouped: bool,
    ) -> Result<Vec<DedupeCluster>, AppRuntimeError> {
        self.with_database(|db| {
            db.list_dedupe_clusters(
                dedupe,
                tags,
                tag_mode,
                single_artist_only,
                has_vibe,
                untagged_only,
                filters,
                hide_grouped,
            )
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn get_dedupe_cluster_members(
        &self,
        dedupe: DedupeMode,
        key: &str,
        tags: &[String],
        tag_mode: TagMatchMode,
        single_artist_only: bool,
        has_vibe: bool,
        untagged_only: bool,
        filters: &[LibraryFilter],
        hide_grouped: bool,
        offset: u64,
        limit: u32,
    ) -> Result<RowPage, AppRuntimeError> {
        self.with_database(|db| {
            db.get_dedupe_cluster_members(
                dedupe,
                key,
                tags,
                tag_mode,
                single_artist_only,
                has_vibe,
                untagged_only,
                filters,
                hide_grouped,
                offset,
                limit,
            )
        })
    }

    pub(crate) fn set_dedupe_alias(
        &self,
        mode: DedupeMode,
        key: &str,
        alias: &str,
    ) -> Result<(), AppRuntimeError> {
        self.with_database_mut(|db| db.set_dedupe_alias(mode, key, alias))
    }
}
