use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::PathBuf,
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::Manager;

static START_LOCK: Mutex<()> = Mutex::new(());

fn hidden(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}

fn probe(port: u16, identity: &str) -> Result<bool, String> {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut socket = match TcpStream::connect_timeout(&address, Duration::from_millis(300)) {
        Ok(socket) => socket,
        Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => return Ok(false),
        Err(error) => return Err(format!("无法检查网页工具端口 {port}：{error}")),
    };
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(super::error_text)?;
    socket
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(super::error_text)?;
    write!(
        socket,
        "GET /health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )
    .map_err(super::error_text)?;
    let mut response = String::new();
    socket
        .take(4096)
        .read_to_string(&mut response)
        .map_err(super::error_text)?;
    if response.starts_with("HTTP/1.1 200") && response.split("\r\n\r\n").nth(1) == Some(identity) {
        Ok(true)
    } else {
        Err(format!(
            "端口 {port} 已被其他程序或旧版网页工具占用，请关闭旧服务后重试。"
        ))
    }
}

fn ensure_service(app: &tauri::AppHandle, tool: &str) -> Result<String, String> {
    let (folder, port, identity) = match tool {
        "extractor" => (
            "style-extractor",
            17321,
            "smart-spreadsheet.style-extractor.v1",
        ),
        "review" => ("style-review", 17324, "smart-spreadsheet.style-review.v1"),
        _ => return Err("未知网页工具".into()),
    };
    let _guard = START_LOCK.lock().map_err(super::error_text)?;
    let url = format!("http://127.0.0.1:{port}");
    if probe(port, identity)? {
        return Ok(url);
    }
    let tools_root = if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tools")
    } else {
        app.path()
            .resource_dir()
            .map_err(super::error_text)?
            .join("web-tools")
    };
    let script = tools_root.join(folder).join("server.mjs");
    if !script.is_file() {
        return Err(format!("缺少网页工具文件：{}", script.display()));
    }
    let output = hidden(Command::new("node").arg("--version"))
        .output()
        .map_err(|_| {
            "网页工具需要 Node.js 22 或更高版本，请安装后重新启动智能表格。".to_string()
        })?;
    let major = String::from_utf8_lossy(&output.stdout)
        .trim()
        .trim_start_matches('v')
        .split('.')
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    if !output.status.success() || major < 22 {
        return Err("网页工具需要 Node.js 22 或更高版本。".into());
    }
    let data = if cfg!(debug_assertions) {
        tools_root.join("style-extractor/data")
    } else {
        app.state::<crate::app::AppRuntime>()
            .active_directory()
            .map_err(super::error_text)?
            .root()
            .join("web-tools/style-extractor")
    };
    fs::create_dir_all(&data).map_err(super::error_text)?;
    let log_path = data.join(format!("{folder}.log"));
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(super::error_text)?;
    let mut command = Command::new("node");
    command
        .arg(script)
        .current_dir(&data)
        .env("STYLE_DATA_DIR", &data)
        .env("STYLE_PORT", "17321")
        .env("STYLE_REVIEW_PORT", "17324")
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(super::error_text)?)
        .stderr(log);
    let mut child = hidden(&mut command).spawn().map_err(super::error_text)?;
    let deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().map_err(super::error_text)? {
            return Err(format!(
                "网页工具启动失败（{status}），日志：{}",
                log_path.display()
            ));
        }
        match probe(port, identity) {
            Ok(true) => return Ok(url),
            Ok(false) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    let _ = child.kill();
    let _ = child.wait();
    Err(format!("网页工具启动超时，日志：{}", log_path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn health_check_reuses_only_the_expected_service() {
        for (body, expected) in [("expected-tool", true), ("other-tool", false)] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 1024];
                let count = stream.read(&mut request).unwrap();
                assert!(
                    String::from_utf8_lossy(&request[..count]).starts_with("GET /health HTTP/1.1")
                );
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            });
            let result = probe(port, "expected-tool");
            if expected {
                assert_eq!(result.unwrap(), true);
            } else {
                assert!(result.unwrap_err().contains("占用"));
            }
            server.join().unwrap();
        }
    }
}

#[tauri::command]
pub(crate) async fn open_style_web_tool(tool: String, app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let url = ensure_service(&app, &tool)?;
        #[cfg(target_os = "windows")]
        let mut command = {
            let mut command = Command::new("rundll32.exe");
            command.arg("url.dll,FileProtocolHandler").arg(&url);
            command
        };
        #[cfg(target_os = "macos")]
        let mut command = {
            let mut command = Command::new("open");
            command.arg(&url);
            command
        };
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let mut command = {
            let mut command = Command::new("xdg-open");
            command.arg(&url);
            command
        };
        hidden(&mut command)
            .spawn()
            .map_err(|error| format!("服务已启动，但打开浏览器失败：{error}。请访问 {url}"))?;
        Ok(())
    })
    .await
    .map_err(super::error_text)?
}
