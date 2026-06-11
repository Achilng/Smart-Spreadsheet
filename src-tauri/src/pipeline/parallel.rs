//! 简单的有序并行映射（自 Novelai工具 移植）：
//! 工作线程从共享队列取任务，结果按原顺序聚合，完成数实时回调。

use std::collections::VecDeque;
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
    mut on_complete: C,
) -> Vec<T>
where
    I: Send,
    T: Send,
    F: Fn(usize, I) -> T + Sync,
    C: FnMut(usize),
{
    if items.is_empty() {
        return Vec::new();
    }
    let total = items.len();

    if worker_count <= 1 {
        return items
            .into_iter()
            .enumerate()
            .map(|(index, item)| {
                let result = worker(index, item);
                on_complete(index + 1);
                result
            })
            .collect();
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

        results
            .into_iter()
            .map(|result| result.expect("worker should return one result per job"))
            .collect()
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
}
