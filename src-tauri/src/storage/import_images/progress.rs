use super::types::{ImageImportProgress, ImageImportStage};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

pub(super) struct ProgressReporter<F: Fn(ImageImportProgress) + Sync> {
    pub(super) callback: F,
    pub(super) last_emit: Mutex<Option<Instant>>,
}

impl<F: Fn(ImageImportProgress) + Sync> ProgressReporter<F> {
    pub(super) fn new(callback: F) -> Self {
        Self {
            callback,
            last_emit: Mutex::new(None),
        }
    }

    /// 进度事件按最小间隔节流，`force` 用于阶段切换和完成事件。
    pub(super) fn emit(
        &self,
        stage: ImageImportStage,
        processed: usize,
        total: usize,
        force: bool,
    ) {
        let now = Instant::now();
        {
            let mut last = self.last_emit.lock().expect("progress lock");
            if !force
                && let Some(previous) = *last
                && now.duration_since(previous) < PROGRESS_INTERVAL
            {
                return;
            }
            *last = Some(now);
        }
        (self.callback)(ImageImportProgress {
            stage,
            processed,
            total,
        });
    }
}
