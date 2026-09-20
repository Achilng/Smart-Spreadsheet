use super::error_text;
use crate::app::AppRuntime;
use crate::db::{
    AutomationRule, AutomationRuleDraft, AutomationRuleExportResult,
    AutomationRuleImportInspection, AutomationRuleImportResult, RuleExecutionSummary, RulePreview,
};
use tauri::{Manager, State};

#[tauri::command]
pub(crate) fn list_automation_rules(
    runtime: State<'_, AppRuntime>,
) -> Result<Vec<AutomationRule>, String> {
    runtime.list_automation_rules().map_err(error_text)
}

#[tauri::command]
pub(crate) fn inspect_automation_rule_file(
    path: String,
    runtime: State<'_, AppRuntime>,
) -> Result<AutomationRuleImportInspection, String> {
    runtime
        .inspect_automation_rule_file(path)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn import_automation_rule_file(
    path: String,
    expected_hash: String,
    runtime: State<'_, AppRuntime>,
) -> Result<AutomationRuleImportResult, String> {
    runtime
        .import_automation_rule_file(path, &expected_hash)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn inspect_automation_rule_text(
    text: String,
    runtime: State<'_, AppRuntime>,
) -> Result<AutomationRuleImportInspection, String> {
    runtime
        .inspect_automation_rule_text(&text)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn import_automation_rule_text(
    text: String,
    expected_hash: String,
    runtime: State<'_, AppRuntime>,
) -> Result<AutomationRuleImportResult, String> {
    runtime
        .import_automation_rule_text(&text, &expected_hash)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn export_automation_rules(
    path: String,
    ids: Vec<i64>,
    runtime: State<'_, AppRuntime>,
) -> Result<AutomationRuleExportResult, String> {
    runtime
        .export_automation_rules(path, &ids)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn create_automation_rule(
    draft: AutomationRuleDraft,
    runtime: State<'_, AppRuntime>,
) -> Result<AutomationRule, String> {
    runtime.create_automation_rule(&draft).map_err(error_text)
}

#[tauri::command]
pub(crate) fn update_automation_rule(
    id: i64,
    draft: AutomationRuleDraft,
    runtime: State<'_, AppRuntime>,
) -> Result<AutomationRule, String> {
    runtime
        .update_automation_rule(id, &draft)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn set_automation_rule_enabled(
    id: i64,
    enabled: bool,
    runtime: State<'_, AppRuntime>,
) -> Result<(), String> {
    runtime
        .set_automation_rule_enabled(id, enabled)
        .map_err(error_text)
}

#[tauri::command]
pub(crate) fn delete_automation_rule(
    id: i64,
    runtime: State<'_, AppRuntime>,
) -> Result<bool, String> {
    runtime.delete_automation_rule(id).map_err(error_text)
}

#[tauri::command]
pub(crate) fn reorder_automation_rules(
    ids: Vec<i64>,
    runtime: State<'_, AppRuntime>,
) -> Result<(), String> {
    runtime.reorder_automation_rules(&ids).map_err(error_text)
}

/// 规则预览要全库扫描，大库上是秒级到分钟级任务，放阻塞线程避免卡住两个窗口。
#[tauri::command]
pub(crate) async fn preview_automation_rule(
    id: i64,
    app: tauri::AppHandle,
) -> Result<RulePreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime.preview_automation_rule(id).map_err(error_text)
    })
    .await
    .map_err(|error| format!("规则预览任务异常中止: {error}"))?
}

#[tauri::command]
pub(crate) async fn preview_automation_rule_draft(
    draft: AutomationRuleDraft,
    app: tauri::AppHandle,
) -> Result<RulePreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .preview_automation_rule_draft(&draft)
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("规则预览任务异常中止: {error}"))?
}

#[tauri::command]
pub(crate) async fn run_automation_rule_on_library(
    id: i64,
    app: tauri::AppHandle,
) -> Result<RuleExecutionSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = app.state::<AppRuntime>();
        runtime
            .run_automation_rule_on_library(id)
            .map_err(error_text)
    })
    .await
    .map_err(|error| format!("规则执行任务异常中止: {error}"))?
}
