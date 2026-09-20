use super::{AppRuntime, AppRuntimeError};
use crate::db::{
    ArtistTextPrefixResult, AutoArtistPrefixApplyResult, AutoArtistPrefixPreview,
    QuickArtistPrefixApplyResult, QuickArtistPrefixChange, QuickArtistPrefixPreview,
    QuickEditCondition, QuickGroupApplyResult, QuickGroupChange, QuickGroupPreview,
    QuickTagApplyResult, QuickTagAssociation, QuickTagPreview,
};

impl AppRuntime {
    pub(crate) fn preview_quick_tag(
        &self,
        condition: &QuickEditCondition,
        tags: &[String],
    ) -> Result<QuickTagPreview, AppRuntimeError> {
        self.with_cloned_database(|db| db.preview_quick_tag(condition, tags))
    }

    pub(crate) fn apply_quick_tag(
        &self,
        condition: &QuickEditCondition,
        tags: &[String],
    ) -> Result<QuickTagApplyResult, AppRuntimeError> {
        self.with_cloned_database_mut(|db| db.apply_quick_tag(condition, tags))
    }

    pub(crate) fn revert_quick_tag_changes(
        &self,
        changes: &[QuickTagAssociation],
    ) -> Result<u64, AppRuntimeError> {
        self.with_database_mut(|db| db.revert_quick_tag_changes(changes))
    }

    pub(crate) fn reapply_quick_tag_changes(
        &self,
        changes: &[QuickTagAssociation],
    ) -> Result<u64, AppRuntimeError> {
        self.with_database_mut(|db| db.reapply_quick_tag_changes(changes))
    }

    pub(crate) fn preview_quick_group(
        &self,
        condition: &QuickEditCondition,
        group_id: i64,
        only_ungrouped: bool,
    ) -> Result<QuickGroupPreview, AppRuntimeError> {
        self.with_cloned_database(|db| db.preview_quick_group(condition, group_id, only_ungrouped))
    }

    pub(crate) fn apply_quick_group(
        &self,
        condition: &QuickEditCondition,
        group_id: i64,
        only_ungrouped: bool,
    ) -> Result<QuickGroupApplyResult, AppRuntimeError> {
        self.with_cloned_database_mut(|db| {
            db.apply_quick_group(condition, group_id, only_ungrouped)
        })
    }

    pub(crate) fn revert_quick_group_changes(
        &self,
        changes: &[QuickGroupChange],
    ) -> Result<u64, AppRuntimeError> {
        self.with_database_mut(|db| db.revert_quick_group_changes(changes))
    }

    pub(crate) fn reapply_quick_group_changes(
        &self,
        changes: &[QuickGroupChange],
    ) -> Result<u64, AppRuntimeError> {
        self.with_database_mut(|db| db.reapply_quick_group_changes(changes))
    }

    pub(crate) fn preview_quick_artist_prefix(
        &self,
        artist_name: &str,
    ) -> Result<QuickArtistPrefixPreview, AppRuntimeError> {
        self.with_cloned_database(|db| db.preview_quick_artist_prefix(artist_name))
    }

    pub(crate) fn apply_quick_artist_prefix(
        &self,
        artist_name: &str,
    ) -> Result<QuickArtistPrefixApplyResult, AppRuntimeError> {
        self.with_cloned_database_mut(|db| db.apply_quick_artist_prefix(artist_name))
    }

    pub(crate) fn revert_quick_artist_prefix_changes(
        &self,
        changes: &[QuickArtistPrefixChange],
    ) -> Result<u64, AppRuntimeError> {
        self.with_database_mut(|db| db.revert_quick_artist_prefix_changes(changes))
    }

    pub(crate) fn reapply_quick_artist_prefix_changes(
        &self,
        changes: &[QuickArtistPrefixChange],
    ) -> Result<u64, AppRuntimeError> {
        self.with_database_mut(|db| db.reapply_quick_artist_prefix_changes(changes))
    }

    pub(crate) fn preview_auto_artist_prefix(
        &self,
    ) -> Result<AutoArtistPrefixPreview, AppRuntimeError> {
        self.with_cloned_database(|db| db.preview_auto_artist_prefix())
    }

    pub(crate) fn prefix_confirmed_artists_in_text(
        &self,
        text: &str,
    ) -> Result<ArtistTextPrefixResult, AppRuntimeError> {
        self.with_cloned_database(|db| db.prefix_confirmed_artists_in_text(text))
    }

    pub(crate) fn apply_auto_artist_prefix(
        &self,
        selected_names: &[String],
    ) -> Result<AutoArtistPrefixApplyResult, AppRuntimeError> {
        self.with_cloned_database_mut(|db| db.apply_auto_artist_prefix(selected_names))
    }
}
