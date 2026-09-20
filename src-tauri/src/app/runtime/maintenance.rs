use super::{AppRuntime, AppRuntimeError};
use crate::storage::PerceptualHashProgress;

impl AppRuntime {
    pub(crate) fn backfill_perceptual_hashes(
        &self,
        progress: impl Fn(PerceptualHashProgress),
    ) -> Result<PerceptualHashProgress, AppRuntimeError> {
        let directory = self.active_directory()?;
        let outcome = directory.backfill_perceptual_hashes(progress)?;
        Ok(PerceptualHashProgress {
            processed: outcome.total,
            total: outcome.total,
            updated: outcome.updated,
            unreadable: outcome.unreadable,
        })
    }

    /// 升级后首启为历史图片补齐 VIBE 数量与组合签名。写库走独立连接，
    /// 完成后失效常驻连接的查询缓存，让重复项视图立即看到新签名。
    pub(crate) fn backfill_vibe_statuses(
        &self,
        progress: impl Fn(crate::storage::VibeStatusProgress),
    ) -> Result<crate::storage::VibeStatusProgress, AppRuntimeError> {
        let directory = self.active_directory()?;
        let outcome = directory.backfill_vibe_statuses(progress)?;
        if outcome.total > 0 {
            self.lock_state()?.invalidate_query_cache();
        }
        Ok(outcome)
    }

    /// 为历史图片补齐画风签名；算法版本落后时全量重算。写库走独立连接，
    /// 完成后失效常驻连接的查询缓存。
    pub(crate) fn backfill_style_signatures(
        &self,
        progress: impl Fn(crate::storage::StyleSignatureProgress),
    ) -> Result<crate::storage::StyleSignatureProgress, AppRuntimeError> {
        let directory = self.active_directory()?;
        let outcome = directory.backfill_style_signatures(progress)?;
        if outcome.total > 0 {
            self.lock_state()?.invalidate_query_cache();
        }
        Ok(outcome)
    }
}
