use std::process::Command;

#[tauri::command]
pub(crate) async fn open_style_web_tool(tool: String, url: String) -> Result<(), String> {
    if !matches!(tool.as_str(), "extractor" | "review") {
        return Err("未知网页工具".into());
    }
    let parsed = tauri::Url::parse(&url).map_err(|_| "请填写有效的服务器 HTTPS 地址")?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err("网页工具地址必须是 HTTPS，且不能包含用户名或密码。".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(target_os = "windows")]
        let mut command = {
            let mut command = Command::new("rundll32.exe");
            command
                .arg("url.dll,FileProtocolHandler")
                .arg(parsed.as_str());
            command
        };
        #[cfg(target_os = "macos")]
        let mut command = {
            let mut command = Command::new("open");
            command.arg(parsed.as_str());
            command
        };
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let mut command = {
            let mut command = Command::new("xdg-open");
            command.arg(parsed.as_str());
            command
        };
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        command
            .spawn()
            .map_err(|error| format!("打开浏览器失败：{error}。请访问 {url}"))?;
        Ok(())
    })
    .await
    .map_err(super::error_text)?
}
