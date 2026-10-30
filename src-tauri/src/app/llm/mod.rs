pub(crate) mod api;
pub(crate) mod credentials;
use super::AppRuntime;
use crate::db::llm::*;
use crate::db::prompt_matching;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub library_id: String,
    pub job_id: String,
    pub total: usize,
    pub done: usize,
    pub failed: usize,
    pub in_flight: usize,
    pub queued: usize,
    pub message: String,
    pub running: bool,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveRequest {
    pub id: u64,
    pub channel: String,
    pub model: String,
    pub count: usize,
    pub text: String,
}
#[derive(Default)]
pub struct Engine {
    pub progress: Mutex<Progress>,
    pub live: Arc<Mutex<HashMap<u64, LiveRequest>>>,
    pub revision: AtomicU64,
}
pub struct Flight {
    id: u64,
    channel: Channel,
    revision: u64,
    rate_round: usize,
    items: Vec<Item>,
    task: tokio::task::JoinHandle<Result<Vec<Result<String, String>>, api::Failure>>,
}
impl Drop for Flight {
    fn drop(&mut self) {
        self.task.abort();
    }
}
pub enum EngineEvent {
    Progress(Progress),
    LibraryChanged(String),
}
pub fn key(channel: &Channel) -> Result<String, String> {
    if let Some(key) = &channel.key {
        return Ok(key.trim().into());
    }
    if channel.encrypted_key.is_empty() {
        return Err(format!("渠道“{}”尚未保存 Key", channel.name));
    }
    String::from_utf8(credentials::crypt(&channel.encrypted_key, false)?)
        .map_err(|_| "Key 编码无效，请重新填写".into())
}
/// Reuse saved credentials only for the matching channel and endpoint.
/// An explicitly empty key means clear it, rather than restore the saved value.
pub fn restore_saved_key(channel: &mut Channel, saved: Option<&Channel>) {
    if channel.key.is_none() {
        channel.key = saved.and_then(|value| value.key.clone());
    }
    channel.encrypted_key = if channel.key.is_some() {
        vec![]
    } else {
        saved.map(|value| value.encrypted_key.clone()).unwrap_or_default()
    };
}

/// Convert readable legacy DPAPI keys once; retain unreadable ciphertext so a
/// library opened on another account is not destructively rewritten.
pub fn migrate_keys(settings: &mut Settings) -> bool {
    let mut changed = false;
    for channel in &mut settings.channels {
        if channel.encrypted_key.is_empty() { continue; }
        if let Ok(value) = key(channel) {
            channel.has_key = !value.is_empty();
            channel.key = Some(value);
            channel.encrypted_key.clear();
            changed = true;
        }
    }
    changed
}

pub fn public_settings(mut settings: Settings) -> Settings {
    for channel in &mut settings.channels {
        channel.has_key = key(channel).is_ok_and(|value| !value.is_empty());
        channel.encrypted_key.clear();
        channel.key = None;
    }
    settings
}
pub fn save_settings(
    runtime: &AppRuntime,
    library: &str,
    mut settings: Settings,
) -> Result<Settings, String> {
    if settings.batch_size == 0 || settings.batch_size > 100 {
        return Err("每次调用条数请设置为 1–100".into());
    }
    if !["low", "medium", "high"].contains(&settings.reasoning.as_str()) {
        return Err("请选择有效的推理等级".into());
    }
    settings.ignored_tags = prompt_matching::clean_ignored_tags(&settings.ignored_tags);
    prompt_matching::validate_ignored_tags(&settings.ignored_tags)?;
    let old = runtime.llm_db(library, |db| db.llm_settings())?;
    let mut ids = HashSet::new();
    for channel in &mut settings.channels {
        if channel.id.is_empty() || !ids.insert(channel.id.clone()) {
            return Err("渠道编号为空或重复".into());
        }
        channel.name = channel.name.trim().into();
        channel.model = channel.model.trim().into();
        if channel.concurrency == 0 || channel.name.is_empty() || channel.model.is_empty() {
            return Err("请填写渠道名称、模型和大于零的并发数".into());
        }
        channel.base_url = api::normalize_url(&channel.base_url)?;
        let previous = old
            .channels
            .iter()
            .find(|c| c.id == channel.id && c.base_url == channel.base_url);
        restore_saved_key(channel, previous);
        channel.key = channel.key.take().map(|value| value.trim().to_owned());
        channel.has_key = key(channel).is_ok_and(|value| !value.is_empty());
    }
    migrate_keys(&mut settings);
    runtime.llm_db(library, |db| {
        db.set_setting(SETTINGS_KEY, &json(&settings)?)
    })?;
    Ok(public_settings(settings))
}
pub fn start(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let runtime = app.state::<AppRuntime>();
        let engine = app.state::<Engine>();
        run(&runtime, &engine, |event| match event {
            EngineEvent::Progress(value) => {
                let _ = app.emit("llm://progress", value);
            }
            EngineEvent::LibraryChanged(value) => {
                let _ = app.emit("llm://library-changed", value);
            }
        })
        .await;
    });
}
async fn stop(flights: &mut Vec<Flight>, engine: &Engine) {
    for mut flight in flights.drain(..) {
        flight.task.abort();
        let _ = (&mut flight.task).await;
    }
    if let Ok(mut live) = engine.live.lock() {
        live.clear();
    }
}
fn publish(engine: &Engine, emit: &impl Fn(EngineEvent), progress: Progress) {
    if let Ok(mut current) = engine.progress.lock() {
        *current = progress.clone();
    }
    emit(EngineEvent::Progress(progress));
}
pub(crate) async fn run(
    runtime: &AppRuntime,
    engine: &Engine,
    emit: impl Fn(EngineEvent) + Send + Sync,
) {
    let mut context = String::new();
    let mut active = String::new();
    let mut flights = Vec::<Flight>::new();
    let mut unavailable = HashMap::<String, (Instant, usize, String)>::new();
    let mut serial = 0u64;
    let mut revision = 0;
    let mut persistence_fault: Option<String> = None;
    let mut interval = tokio::time::interval(Duration::from_millis(500));
    loop {
        interval.tick().await;
        let (root, library, settings) = match runtime.llm_context() {
            Ok(value) => value,
            Err(error) => {
                stop(&mut flights, &engine).await;
                context.clear();
                active.clear();
                publish(
                    engine,
                    &emit,
                    Progress {
                        message: error,
                        ..Default::default()
                    },
                );
                continue;
            }
        };
        let next_context = format!("{root}|{library}");
        if context != next_context {
            stop(&mut flights, &engine).await;
            active.clear();
            unavailable.clear();
            persistence_fault = None;
            if let Err(error) = runtime.llm_db(&library, |db| db.llm_recover()) {
                publish(
                    engine,
                    &emit,
                    Progress {
                        message: error,
                        ..Default::default()
                    },
                );
                continue;
            }
            context = next_context;
        }
        let next_revision = engine.revision.load(Ordering::SeqCst);
        if revision != next_revision {
            unavailable.clear();
            revision = next_revision;
            persistence_fault = None;
        }
        if let Some(error) = &persistence_fault {
            stop(&mut flights, engine).await;
            publish(
                engine,
                &emit,
                Progress {
                    library_id: library,
                    message: format!("保存进度失败，已停止调用：{error}"),
                    ..Default::default()
                },
            );
            continue;
        }
        let mut jobs = match runtime.llm_db(&library, |db| db.llm_jobs()) {
            Ok(v) => v,
            Err(error) => {
                publish(
                    engine,
                    &emit,
                    Progress {
                        library_id: library,
                        message: error,
                        ..Default::default()
                    },
                );
                continue;
            }
        };
        let current = jobs
            .iter()
            .find(|j| j.id == active && ["queued", "running"].contains(&j.state.as_str()))
            .cloned();
        let job = match current.or_else(|| {
            jobs.iter()
                .rev()
                .find(|j| ["queued", "running"].contains(&j.state.as_str()))
                .cloned()
        }) {
            Some(job) => job,
            None => {
                stop(&mut flights, &engine).await;
                active.clear();
                let message = jobs
                    .iter()
                    .find(|j| j.state == "blocked")
                    .map(|j| j.message.clone())
                    .unwrap_or_else(|| {
                        runtime
                            .llm_db(&library, |db| db.setting("local_llm_enqueue_error"))
                            .ok()
                            .flatten()
                            .unwrap_or_default()
                    });
                publish(
                    engine,
                    &emit,
                    Progress {
                        library_id: library,
                        message,
                        ..Default::default()
                    },
                );
                continue;
            }
        };
        if active != job.id {
            stop(&mut flights, &engine).await;
            unavailable.clear();
            active = job.id.clone();
        }
        if job.prompt_version != PROMPT_VERSION {
            let _ = runtime.llm_db(&library, |db| {
                db.llm_worker_state(&job.id, "blocked", "提取规则已变化，请创建新任务")
            });
            continue;
        }
        let mut changed = false;
        let mut fatal = None;
        for index in (0..flights.len()).rev() {
            if !flights[index].task.is_finished() {
                continue;
            }
            let mut flight = flights.remove(index);
            if let Ok(mut live) = engine.live.lock() {
                live.remove(&flight.id);
            }
            let result = (&mut flight.task).await;
            match result {
                Ok(Ok(results)) => {
                    if flight.revision == revision
                        && unavailable
                            .get(&flight.channel.id)
                            .is_some_and(|(until, round, _)| {
                                *until <= Instant::now() && *round == flight.rate_round
                            })
                    {
                        unavailable.remove(&flight.channel.id);
                    }
                    for (item, result) in flight.items.iter().zip(results) {
                        let saved = match result {
                            Ok(artists) => runtime
                                .llm_db(&library, |db| {
                                    db.llm_save_result(
                                        &job.id,
                                        item,
                                        &artists,
                                        &flight.channel.model,
                                        &flight.channel.name,
                                    )
                                })
                                .map(|n| {
                                    changed |= n > 0;
                                }),
                            Err(error) => {
                                runtime.llm_db(&library, |db| db.llm_fail(&job.id, item, &error))
                            }
                        };
                        if let Err(error) = saved {
                            fatal = Some(error);
                            break;
                        }
                    }
                }
                Ok(Err(error)) if [401, 403, 404, 429].contains(&error.status) => {
                    if flight.revision == revision {
                        let previous = unavailable.get(&flight.channel.id);
                        // A burst of concurrent 429 responses counts as one recovery round.
                        let rounds = previous
                            .map(|(until, n, _)| if *until > Instant::now() { *n } else { n + 1 })
                            .unwrap_or(1);
                        let seconds = if error.status != 429 || rounds >= 3 {
                            86400 * 365
                        } else {
                            error.retry_after.max(5 * rounds as u64).min(86400)
                        };
                        unavailable.insert(
                            flight.channel.id.clone(),
                            (
                                Instant::now() + Duration::from_secs(seconds),
                                rounds,
                                error.message,
                            ),
                        );
                    }
                }
                other => {
                    let message = match other {
                        Ok(Err(error)) => error.message,
                        _ => "调用任务异常中止".into(),
                    };
                    for item in &flight.items {
                        if let Err(error) =
                            runtime.llm_db(&library, |db| db.llm_fail(&job.id, item, &message))
                        {
                            fatal = Some(error);
                        }
                    }
                }
            }
        }
        if changed {
            emit(EngineEvent::LibraryChanged(library.clone()));
        }
        if let Some(error) = fatal {
            stop(&mut flights, &engine).await;
            persistence_fault = Some(error.clone());
            let _ = runtime.llm_db(&library, |db| {
                db.llm_worker_state(&job.id, "blocked", &error)
            });
            publish(
                engine,
                &emit,
                Progress {
                    library_id: library,
                    message: error,
                    ..Default::default()
                },
            );
            continue;
        }
        let busy = flights
            .iter()
            .flat_map(|f| f.items.iter().map(|i| i.hash.clone()))
            .collect::<HashSet<_>>();
        let mut occupied_keys = flights
            .iter()
            .flat_map(|f| f.items.iter())
            .map(|i| prompt_matching::key(&i.prompt, &settings.ignored_tags))
            .collect::<HashSet<_>>();
        // Fetch only enough pending work for actual capacity; large configured concurrency does not allocate workers.
        let channels = settings
            .channels
            .iter()
            .filter(|c| c.enabled && job.channel_ids.contains(&c.id))
            .cloned()
            .collect::<Vec<_>>();
        let capacity = channels
            .iter()
            .map(|c| {
                c.concurrency
                    .saturating_sub(flights.iter().filter(|f| f.channel.id == c.id).count())
            })
            .fold(0usize, usize::saturating_add);
        let pending = match runtime.llm_db(&library, |db| {
            db.llm_items(
                &job.id,
                Some("pending"),
                0,
                capacity
                    .saturating_mul(job.batch_size)
                    .saturating_add(busy.len())
                    .max(64),
            )
        }) {
            Ok(v) => v,
            Err(error) => {
                let _ = runtime.llm_db(&library, |db| {
                    db.llm_worker_state(&job.id, "blocked", &error)
                });
                continue;
            }
        };
        let mut queue = pending
            .into_iter()
            .filter(|i| !busy.contains(&i.hash))
            .collect::<Vec<_>>();
        // Reuse before allocating requests. Keep each original prompt for safe writeback and export.
        let mut remaining = Vec::new();
        let mut reused_any = false;
        for item in queue.drain(..) {
            match runtime.llm_db(&library, |db| {
                db.llm_cached(&item, (!job.reuse).then_some(job.id.as_str()))
            }) {
                Ok(Some((artists, model, channel))) => match runtime.llm_db(&library, |db| {
                    db.llm_save_result(
                        &job.id,
                        &item,
                        &artists,
                        &model,
                        &format!("{}（复用）", channel.trim_end_matches("（复用）")),
                    )
                }) {
                    Ok(n) => {
                        changed |= n > 0;
                        reused_any = true;
                    }
                    Err(error) => fatal = Some(error),
                },
                Ok(None) => {
                    // Variants already queued in this scheduling round or in flight wait for its result.
                    if occupied_keys.insert(prompt_matching::key(&item.prompt, &settings.ignored_tags))
                    {
                        remaining.push(item);
                    }
                }
                Err(error) => fatal = Some(error),
            }
        }
        if let Some(error) = fatal {
            stop(&mut flights, engine).await;
            persistence_fault = Some(error.clone());
            let _ = runtime.llm_db(&library, |db| {
                db.llm_worker_state(&job.id, "blocked", &error)
            });
            continue;
        }
        let needs_request = !remaining.is_empty();
        let mut queue = remaining.into_iter().peekable();
        let mut can_run = false;
        let mut waiting = String::new();
        for channel in channels {
            if let Some((until, rounds, message)) = unavailable.get(&channel.id) {
                if *until > Instant::now() {
                    if *rounds < 3
                        && until.saturating_duration_since(Instant::now()).as_secs() < 86401
                    {
                        can_run = true;
                        waiting = format!(
                            "{}：等待 {} 秒后重试",
                            channel.name,
                            until.saturating_duration_since(Instant::now()).as_secs()
                        );
                    } else {
                        waiting = format!("{}：{}", channel.name, message);
                    }
                    continue;
                }
            }
            let secret = match key(&channel) {
                Ok(key) if !key.is_empty() => key,
                _ => {
                    waiting = format!("渠道“{}”需要重新填写 Key", channel.name);
                    continue;
                }
            };
            can_run = true;
            let used = flights
                .iter()
                .filter(|f| f.channel.id == channel.id)
                .count();
            for _ in used..channel.concurrency {
                if engine.revision.load(Ordering::SeqCst) != revision {
                    break;
                }
                if queue.peek().is_none() {
                    break;
                }
                let mut items = Vec::new();
                let mut length = 0;
                while let Some(item) = queue.peek() {
                    if !items.is_empty()
                        && (items.len() >= job.batch_size || length + item.prompt.len() > 60000)
                    {
                        break;
                    }
                    let item = queue.next().unwrap();
                    length += item.prompt.len();
                    items.push(item);
                }
                serial += 1;
                let id = serial;
                match runtime.llm_db(&library, |db| db.llm_claim(&job.id)) {
                    Ok(true) => {}
                    Ok(false) => break,
                    Err(error) => {
                        waiting = error;
                        break;
                    }
                }
                if let Ok(mut live) = engine.live.lock() {
                    live.insert(
                        id,
                        LiveRequest {
                            id,
                            channel: channel.name.clone(),
                            model: channel.model.clone(),
                            count: items.len(),
                            text: String::new(),
                        },
                    );
                }
                let live_requests = engine.live.clone();
                let request_channel = channel.clone();
                let request_items = items.clone();
                let request_job = job.clone();
                let request_key = secret.clone();
                let task = tokio::spawn(api::extract(
                    request_channel,
                    request_key,
                    request_job,
                    request_items,
                    move |text| {
                        if let Ok(mut live) = live_requests.lock() {
                            if let Some(entry) = live.get_mut(&id) {
                                entry.text = text;
                            }
                        }
                    },
                ));
                flights.push(Flight {
                    id,
                    channel: channel.clone(),
                    revision,
                    rate_round: unavailable
                        .get(&channel.id)
                        .map(|(_, round, _)| *round)
                        .unwrap_or(0),
                    items,
                    task,
                });
            }
        }
        if changed {
            emit(EngineEvent::LibraryChanged(library.clone()));
        }
        if let Ok(latest) = runtime.llm_db(&library, |db| db.llm_jobs()) {
            jobs = latest;
        }
        let latest = jobs.iter().find(|j| j.id == job.id).unwrap_or(&job);
        if flights.is_empty() {
            if latest.done + latest.failed == latest.total {
                let _ = runtime.llm_db(&library, |db| {
                    db.llm_worker_state(
                        &job.id,
                        "completed",
                        if latest.failed > 0 {
                            "已完成，可单独重试失败项"
                        } else {
                            "已完成并自动应用"
                        },
                    )
                });
            } else if !can_run && (needs_request || !reused_any) {
                let _ = runtime.llm_db(&library, |db| {
                    db.llm_worker_state(
                        &job.id,
                        "blocked",
                        if waiting.is_empty() {
                            "没有可用渠道，请保存配置并继续任务"
                        } else {
                            &waiting
                        },
                    )
                });
            }
        }
        publish(
            engine,
            &emit,
            Progress {
                library_id: library,
                job_id: job.id.clone(),
                total: latest.total,
                done: latest.done,
                failed: latest.failed,
                in_flight: flights.len(),
                queued: jobs.iter().filter(|j| j.state == "queued").count(),
                message: waiting,
                running: !flights.is_empty() || can_run,
            },
        );
    }
}
