use std::sync::atomic::{AtomicBool, Ordering};

use super::{AppRuntime, AppRuntimeError};
use crate::storage::PerceptualHashProgress;

/// One pHash backfill at a time: the toolbox and the agent can both start one, and two runs
/// would race to fill the same rows.
static PHASH_RUNNING: AtomicBool = AtomicBool::new(false);

struct PhashRun;

impl Drop for PhashRun {
    fn drop(&mut self) {
        PHASH_RUNNING.store(false, Ordering::SeqCst);
    }
}

impl AppRuntime {
    pub(crate) fn perceptual_hash_backfill_running(&self) -> bool {
        PHASH_RUNNING.load(Ordering::SeqCst)
    }

    pub(crate) fn backfill_perceptual_hashes(
        &self,
        progress: impl Fn(PerceptualHashProgress),
    ) -> Result<PerceptualHashProgress, AppRuntimeError> {
        if PHASH_RUNNING.swap(true, Ordering::SeqCst) {
            return Err(AppRuntimeError::Busy("感知哈希正在计算中，请等这一轮结束".into()));
        }
        let _run = PhashRun;
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
