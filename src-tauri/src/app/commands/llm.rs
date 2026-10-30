use crate::{
    app::{
        AppRuntime,
        llm::{self, Engine, LiveRequest, Progress},
    },
    db::{RowSelection, llm::*},
};
use serde::Serialize;
use std::sync::atomic::Ordering;
use tauri::{Emitter, Manager};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    library_id: String,
    settings: Settings,
    default_ignored_tags: Vec<String>,
    jobs: Vec<Job>,
    progress: Progress,
    live: Vec<LiveRequest>,
    live_total: usize,
}
#[tauri::command]
pub(crate) async fn llm_snapshot(
    live_page: Option<usize>,
    app: tauri::AppHandle,
) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        let (_, library_id, settings) = runtime.llm_context()?;
        let jobs = runtime.llm_db(&library_id, |db| db.llm_jobs())?;
        let engine = app.state::<Engine>();
        let mut progress = engine
            .progress
            .lock()
            .map_err(|_| "任务状态不可用")?
            .clone();
        let live_guard = engine.live.lock().map_err(|_| "实时状态不可用")?;
        let mut keys = live_guard.keys().copied().collect::<Vec<_>>();
        keys.sort_unstable();
        let mut live_total = keys.len();
        let mut live = keys
            .into_iter()
            .skip(live_page.unwrap_or(0).saturating_mul(40))
            .take(40)
            .filter_map(|id| live_guard.get(&id).cloned())
            .collect::<Vec<_>>();
        if progress.library_id != library_id {
            progress = Progress::default();
            live.clear();
            live_total = 0;
        }
        Ok(Snapshot {
            library_id,
            settings: llm::public_settings(settings),
            default_ignored_tags: crate::db::prompt_matching::default_ignored_tags(),
            jobs,
            progress,
            live,
            live_total,
        })
    })
    .await
    .map_err(super::error_text)?
}
#[tauri::command]
pub(crate) async fn artist_highlight_segments(
    texts: Vec<String>,
    app: tauri::AppHandle,
) -> Result<Vec<Vec<crate::db::prompt_matching::ArtistHighlightSegment>>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (_, _, settings) = app.state::<AppRuntime>().llm_context()?;
        Ok(texts
            .iter()
            .map(|text| {
                crate::db::prompt_matching::artist_highlight_segments(text, &settings.ignored_tags)
            })
            .collect())
    })
    .await
    .map_err(super::error_text)?
}
#[tauri::command]
pub(crate) async fn llm_save_settings(
    library_id: String,
    settings: Settings,
    app: tauri::AppHandle,
) -> Result<Settings, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let result = llm::save_settings(&app.state::<AppRuntime>(), &library_id, settings)?;
        app.state::<Engine>()
            .revision
            .fetch_add(1, Ordering::SeqCst);
        let _ = app.emit("llm://library-changed", &library_id);
        let _ = app.emit("artist-matching://changed", &library_id);
        Ok(result)
    })
    .await
    .map_err(super::error_text)?
}
#[tauri::command]
pub(crate) async fn refresh_artist_matching(
    library_id: String,
    app: tauri::AppHandle,
) -> Result<crate::db::ArtistMatchingRefresh, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let result = app
            .state::<AppRuntime>()
            .llm_db(&library_id, |db| db.refresh_artist_matching(true))?;
        let _ = app.emit("llm://library-changed", &library_id);
        let _ = app.emit("artist-matching://changed", &library_id);
        Ok(result)
    })
    .await
    .map_err(super::error_text)?
}
#[tauri::command]
pub(crate) async fn llm_estimate(
    library_id: String,
    selection: Option<RowSelection>,
    include_processed: bool,
    all_matching: bool,
    app: tauri::AppHandle,
) -> Result<Estimate, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppRuntime>().llm_db(&library_id, |db| {
            db.llm_estimate(selection.as_ref(), include_processed, all_matching)
        })
    })
    .await
    .map_err(super::error_text)?
}
#[tauri::command]
pub(crate) async fn llm_create(
    library_id: String,
    selection: Option<RowSelection>,
    include_processed: bool,
    all_matching: bool,
    channel_ids: Vec<String>,
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppRuntime>().llm_db(&library_id, |db| {
            db.llm_create(
                selection.as_ref(),
                include_processed,
                all_matching,
                channel_ids,
                false,
            )
        })
    })
    .await
    .map_err(super::error_text)?
}
#[tauri::command]
pub(crate) async fn llm_action(
    library_id: String,
    job_id: String,
    action: String,
    channel_ids: Vec<String>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppRuntime>().llm_db(&library_id, |db| {
            db.llm_action(&job_id, &action, channel_ids)
        })?;
        app.state::<Engine>()
            .revision
            .fetch_add(1, Ordering::SeqCst);
        if action == "undo" {
            let _ = app.emit("llm://library-changed", &library_id);
        }
        Ok(())
    })
    .await
    .map_err(super::error_text)?
}
#[tauri::command]
pub(crate) async fn llm_items(
    library_id: String,
    job_id: String,
    page: usize,
    app: tauri::AppHandle,
) -> Result<Vec<Item>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppRuntime>().llm_db(&library_id, |db| {
            db.llm_items(&job_id, None, page.saturating_mul(20), 20)
        })
    })
    .await
    .map_err(super::error_text)?
}
#[tauri::command]
pub(crate) async fn llm_models(
    library_id: String,
    mut channel: Channel,
    app: tauri::AppHandle,
) -> Result<Vec<String>, String> {
    channel.base_url = llm::api::normalize_url(&channel.base_url)?;
    let settings = app
        .state::<AppRuntime>()
        .llm_db(&library_id, |db| db.llm_settings())?;
    // Restore a saved key for model discovery without exposing it to the webview.
    let previous = settings.channels.iter()
        .find(|c| c.id == channel.id && c.base_url == channel.base_url);
    llm::restore_saved_key(&mut channel, previous);
    let key = llm::key(&channel)?;
    if key.is_empty() {
        return Err("请先填写 API Key".into());
    }
    llm::api::models(&channel, &key).await
}
#[tauri::command]
pub(crate) async fn llm_export(
    library_id: String,
    job_id: String,
    path: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move||{
        let (job,items)=app.state::<AppRuntime>().llm_db(&library_id,|db|{
            let job=db.llm_jobs()?.into_iter().find(|j|j.id==job_id).ok_or_else(||invalid("任务不存在"))?;
            Ok((job,db.llm_items(&job_id,None,0,i64::MAX as usize)?))
        })?;
        let models=items.iter().map(|i|i.model.clone()).filter(|m|!m.is_empty()).collect::<std::collections::BTreeSet<_>>().into_iter().collect::<Vec<_>>().join(" / ");
        let values=items.into_iter().filter(|i|i.status!="pending").map(|i|serde_json::json!({"id":i.hash,"positive_prompt":i.prompt,"status":i.status,"artist_string":i.artists,"model":i.model,"channel_name":i.channel,"error":i.error})).collect::<Vec<_>>();
        let doc=serde_json::json!({"format":"smart-spreadsheet.style-extraction.result","version":1,"export_id":job_id,"processor":{"model":if models.is_empty(){"未完成"}else{&models},"prompt_version":job.prompt_version},"items":values});
        let path=std::path::Path::new(&path);let temp=crate::fsx::unique_sibling_path(path,"llm-export");let mut guard=crate::fsx::TemporaryFile::new(temp.clone());
        std::fs::write(&temp,serde_json::to_vec_pretty(&doc).map_err(super::error_text)?).map_err(super::error_text)?;
        crate::fsx::replace_output_file(&temp,path).map_err(super::error_text)?;guard.commit();Ok(())
    }).await.map_err(super::error_text)?
}
