use super::error_text;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

/// Every page's background (`--bg` / `--surface`), so the first composited frame never differs.
const PAGE_BACKGROUND: tauri::window::Color = tauri::window::Color(255, 255, 255, 255);
const REVEAL_FALLBACK: std::time::Duration = std::time::Duration::from_millis(2000);

/// Builds a secondary window hidden; its page shows it once the first screen is rendered
/// (`useRevealWindow`). If the page never does (slow load, script error), it is shown anyway.
pub(crate) fn build_hidden_until_ready(builder: WebviewWindowBuilder<'_, tauri::Wry, tauri::AppHandle>) -> Result<(), String> {
    let window = builder
        .visible(false)
        .background_color(PAGE_BACKGROUND)
        .build()
        .map_err(error_text)?;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(REVEAL_FALLBACK).await;
        if window.is_visible().is_ok_and(|visible| !visible) {
            let _ = window.show();
            let _ = window.set_focus();
        }
    });
    Ok(())
}

/// 打开应用内部的单实例工具箱窗口。
///
/// 使用 `WebviewUrl::App` 明确加载内置前端资源，避免动态 URL 被系统浏览器接管。
#[tauri::command]
pub(crate) async fn open_toolbox_window(
    app: tauri::AppHandle,
    target: Option<String>,
) -> Result<(), String> {
    let llm_settings = target.as_deref() == Some("llmSettings");
    if let Some(toolbox) = app.get_webview_window("toolbox") {
        toolbox.show().map_err(error_text)?;
        toolbox.unminimize().map_err(error_text)?;
        toolbox.set_focus().map_err(error_text)?;
        if llm_settings {
            toolbox
                .emit_to("toolbox", "toolbox://open-llm-settings", ())
                .map_err(error_text)?;
        }
        return Ok(());
    }

    build_hidden_until_ready(
        WebviewWindowBuilder::new(
            &app,
            "toolbox",
            WebviewUrl::App(if llm_settings {
                "index.html?window=toolbox&target=llmSettings".into()
            } else {
                "index.html?window=toolbox".into()
            }),
        )
        .title("工具箱")
        .inner_size(960.0, 680.0)
        .min_inner_size(760.0, 520.0)
        .center()
        .resizable(true)
        .decorations(false)
        .skip_taskbar(false),
    )
}

const COMPARE_TABS: [&str; 4] = ["artists", "vibeDiffStyle", "styleDiffVibe", "models"];
pub(crate) const COMPARE_FIELDS: [&str; 13] = [
    "artists", "positive", "character", "negative", "vibe", "model", "sampler",
    "steps", "seed", "scale", "cfgRescale", "noiseSchedule", "dimensions",
];

/// Which relation the compare window should show, or a custom comparison to run.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CompareView {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom: Option<CompareCustom>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CompareCustom {
    pub observe: String,
    #[serde(default)]
    pub dedupe: bool,
    /// Missing means every other condition may differ, like the editor's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed: Option<Vec<String>>,
}

impl CompareView {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.tab.is_some() && self.custom.is_some() {
            return Err("tab 和 custom 只能给一个".into());
        }
        if let Some(tab) = &self.tab
            && !COMPARE_TABS.contains(&tab.as_str()) {
            return Err(format!("tab 只能是 {}", COMPARE_TABS.join("、")));
        }
        if let Some(custom) = &self.custom {
            let fields = || COMPARE_FIELDS.join("、");
            if !COMPARE_FIELDS.contains(&custom.observe.as_str()) {
                return Err(format!("observe 只能是 {}", fields()));
            }
            if let Some(field) = custom.allowed.iter().flatten().find(|f| !COMPARE_FIELDS.contains(&f.as_str())) {
                return Err(format!("allowed 里的「{field}」不是可比较的条件，只能是 {}", fields()));
            }
        }
        Ok(())
    }

    /// Query-string form for a newly created window; every value is a whitelisted identifier.
    fn query(&self) -> String {
        let mut query = String::new();
        if let Some(tab) = &self.tab {
            query.push_str(&format!("&tab={tab}"));
        }
        if let Some(custom) = &self.custom {
            query.push_str(&format!("&tab=custom&observe={}&dedupe={}", custom.observe, u8::from(custom.dedupe)));
            if let Some(allowed) = &custom.allowed {
                query.push_str(&format!("&allowed={}", allowed.join(",")));
            }
        }
        query
    }
}

/// 打开（或复用）单实例图片对比窗口并切换样本。样本 id 直接写进新窗口
/// URL，新窗口自行拉取数据；窗口已存在时由后端向其推送
/// `compare://set-sample` 通知样本与可选的对比目标，
/// 从根上避免上一版“前端推送 vs 窗口就绪”的竞态。
#[tauri::command]
pub(crate) async fn open_compare_window(
    row_id: i64,
    target_id: Option<i64>,
    view: Option<CompareView>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    if let Some(view) = &view {
        view.validate()?;
    }
    if let Some(compare) = app.get_webview_window("compare") {
        compare.show().map_err(error_text)?;
        compare.unminimize().map_err(error_text)?;
        compare.set_focus().map_err(error_text)?;
        let payload = match (target_id, &view) {
            (None, None) => serde_json::json!(row_id),
            _ => serde_json::json!({ "rowId": row_id, "targetId": target_id, "view": view }),
        };
        compare
            .emit_to("compare", "compare://set-sample", payload)
            .map_err(error_text)?;
        return Ok(());
    }

    let mut url = format!("index.html?window=compare&row={row_id}");
    if let Some(target) = target_id {
        url.push_str(&format!("&target={target}"));
    }
    if let Some(view) = &view {
        url.push_str(&view.query());
    }
    build_hidden_until_ready(
        WebviewWindowBuilder::new(&app, "compare", WebviewUrl::App(url.into()))
            .title("图片对比")
            .inner_size(1080.0, 760.0)
            .min_inner_size(860.0, 600.0)
            .center()
            .resizable(true)
            .decorations(false)
            .skip_taskbar(false),
    )
}

#[tauri::command]
pub(crate) fn focus_main_window(app: tauri::AppHandle) -> Result<(), String> {
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "找不到主窗口".to_owned())?;
    main.show().map_err(error_text)?;
    main.unminimize().map_err(error_text)?;
    main.set_focus().map_err(error_text)
}
