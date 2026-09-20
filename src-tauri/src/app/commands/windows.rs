use super::error_text;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

/// 打开应用内部的单实例工具箱窗口。
///
/// 使用 `WebviewUrl::App` 明确加载内置前端资源，避免动态 URL 被系统浏览器接管。
#[tauri::command]
pub(crate) async fn open_toolbox_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(toolbox) = app.get_webview_window("toolbox") {
        toolbox.show().map_err(error_text)?;
        toolbox.unminimize().map_err(error_text)?;
        toolbox.set_focus().map_err(error_text)?;
        return Ok(());
    }

    WebviewWindowBuilder::new(
        &app,
        "toolbox",
        WebviewUrl::App("index.html?window=toolbox".into()),
    )
    .title("工具箱")
    .inner_size(960.0, 680.0)
    .min_inner_size(760.0, 520.0)
    .center()
    .resizable(true)
    .decorations(false)
    .skip_taskbar(false)
    .build()
    .map_err(error_text)?;
    Ok(())
}

/// 打开（或复用）单实例图片对比窗口并切换样本。样本 id 直接写进新窗口
/// URL，新窗口自行拉取数据；窗口已存在时由后端向其推送
/// `compare://set-sample`（载荷仅 row id）——此时窗口早已初始化完毕，
/// 从根上避免上一版“前端推送 vs 窗口就绪”的竞态。
#[tauri::command]
pub(crate) async fn open_compare_window(row_id: i64, app: tauri::AppHandle) -> Result<(), String> {
    if let Some(compare) = app.get_webview_window("compare") {
        compare.show().map_err(error_text)?;
        compare.unminimize().map_err(error_text)?;
        compare.set_focus().map_err(error_text)?;
        compare
            .emit_to("compare", "compare://set-sample", row_id)
            .map_err(error_text)?;
        return Ok(());
    }

    WebviewWindowBuilder::new(
        &app,
        "compare",
        WebviewUrl::App(format!("index.html?window=compare&row={row_id}").into()),
    )
    .title("图片对比")
    .inner_size(1080.0, 760.0)
    .min_inner_size(860.0, 600.0)
    .center()
    .resizable(true)
    .decorations(false)
    .skip_taskbar(false)
    .build()
    .map_err(error_text)?;
    Ok(())
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
