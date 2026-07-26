//! 简单的有序并行映射（自 Novelai工具 移植）：
//! 工作线程从共享队列取任务，结果按原顺序聚合，完成数实时回调。

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};

pub const MAX_WORKER_THREADS: usize = 32;

pub fn worker_count(job_count: usize) -> usize {
    if job_count == 0 {
        return 0;
    }
    std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .min(MAX_WORKER_THREADS)
        .min(job_count)
        .max(1)
}

pub fn parallel_map<I, T, F, C>(
    items: Vec<I>,
    worker_count: usize,
    worker: F,
    on_complete: C,
) -> Vec<T>
where
    I: Send,
    T: Send,
    F: Fn(usize, I) -> T + Sync,
    C: FnMut(usize),
{
    let never = AtomicBool::new(false);
    parallel_map_cancellable(items, worker_count, &never, worker, on_complete)
        .expect("map without cancel flag always completes")
}

/// 可取消版本：`cancel` 置位后工作线程停止取新任务，返回 `None`。
/// 已完成的部分结果被丢弃——调用方应把取消视为整体失败并自行清理。
pub fn parallel_map_cancellable<I, T, F, C>(
    items: Vec<I>,
    worker_count: usize,
    cancel: &AtomicBool,
    worker: F,
    mut on_complete: C,
) -> Option<Vec<T>>
where
    I: Send,
    T: Send,
    F: Fn(usize, I) -> T + Sync,
    C: FnMut(usize),
{
    if items.is_empty() {
        return Some(Vec::new());
    }
    let total = items.len();

    if worker_count <= 1 {
        let mut results = Vec::with_capacity(total);
        for (index, item) in items.into_iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            results.push(worker(index, item));
            on_complete(index + 1);
        }
        return Some(results);
    }

    let jobs = Arc::new(Mutex::new(
        items.into_iter().enumerate().collect::<VecDeque<_>>(),
    ));
    let (sender, receiver) = mpsc::channel();

    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            let jobs = Arc::clone(&jobs);
            let sender = sender.clone();
            let worker = &worker;

            scope.spawn(move || {
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let job = jobs
                        .lock()
                        .expect("worker queue should not be poisoned")
                        .pop_front();
                    let Some((index, item)) = job else {
                        break;
                    };

                    if sender.send((index, worker(index, item))).is_err() {
                        break;
                    }
                }
            });
        }
        drop(sender);

        let mut results = (0..total).map(|_| None).collect::<Vec<_>>();
        let mut completed = 0_usize;
        for (index, result) in receiver {
            results[index] = Some(result);
            completed += 1;
            on_complete(completed);
        }

        if cancel.load(Ordering::Relaxed) && completed < total {
            return None;
        }
        Some(
            results
                .into_iter()
                .map(|result| result.expect("worker should return one result per job"))
                .collect(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_in_order_with_multiple_workers() {
        let inputs: Vec<usize> = (0..100).collect();

        let mut completions = 0;
        let outputs = parallel_map(inputs, 4, |_, value| value * 2, |_| completions += 1);

        assert_eq!(outputs, (0..100).map(|v| v * 2).collect::<Vec<_>>());
        assert_eq!(completions, 100);
    }

    #[test]
    fn worker_count_respects_job_and_thread_limits() {
        assert_eq!(worker_count(0), 0);
        assert_eq!(worker_count(1), 1);
        assert!(worker_count(10_000) <= MAX_WORKER_THREADS);
        assert!(worker_count(10_000) >= 1);
    }

    #[test]
    fn cancellable_map_completes_when_not_cancelled() {
        let cancel = AtomicBool::new(false);
        let inputs: Vec<usize> = (0..50).collect();
        let outputs =
            parallel_map_cancellable(inputs, 4, &cancel, |_, value| value + 1, |_| {});
        assert_eq!(outputs, Some((1..=50).collect::<Vec<_>>()));
    }

    #[test]
    fn cancellable_map_returns_none_after_cancel() {
        let cancel = AtomicBool::new(true);
        let inputs: Vec<usize> = (0..50).collect();
        // 取消已置位：单线程路径立即返回 None
        let outputs =
            parallel_map_cancellable(inputs, 1, &cancel, |_, value| value, |_| {});
        assert_eq!(outputs, None);
    }
}
