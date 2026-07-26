//! 全局长任务取消令牌。
//!
//! 应用同一时刻只允许一个可取消长任务（前端用 app.busy 互斥），
//! 因此用单个全局标志即可，避免把取消参数穿透每一层函数签名。
//! 长任务入口调用 [`begin`] 复位标志；各阶段边界检查 [`is_requested`]；
//! 前端通过 `cancel_current_task` 命令置位。

use std::sync::atomic::{AtomicBool, Ordering};

static CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);

/// 长任务开始时调用：清掉上一次任务遗留的取消请求。
pub fn begin() {
    CANCEL_REQUESTED.store(false, Ordering::Relaxed);
}

/// 请求取消当前长任务。
pub fn request() {
    CANCEL_REQUESTED.store(true, Ordering::Relaxed);
}

pub fn is_requested() -> bool {
    CANCEL_REQUESTED.load(Ordering::Relaxed)
}

/// 供 `parallel_map_cancellable` 使用的标志引用。
pub fn flag() -> &'static AtomicBool {
    &CANCEL_REQUESTED
}
