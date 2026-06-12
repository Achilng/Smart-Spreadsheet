//! 文件输出共用工具：临时文件 + 原子替换（自 Novelai工具 移植）。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(extension))
}

pub fn canonical_or_original(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// 输出文件同目录下的唯一隐藏临时路径。
pub fn unique_sibling_path(output_path: &Path, suffix: &str) -> PathBuf {
    let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = output_path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    output_path.with_file_name(format!(
        ".{file_name}.{}.{}.{}",
        std::process::id(),
        counter,
        suffix
    ))
}

/// 用临时文件原子替换目标：先把旧目标改名备份，替换失败时恢复。
pub fn replace_output_file(temp_path: &Path, output_path: &Path) -> std::io::Result<()> {
    let backup_path = unique_sibling_path(output_path, "backup");
    let had_existing_output = output_path.exists();

    if had_existing_output {
        fs::rename(output_path, &backup_path)?;
    }

    if let Err(error) = fs::rename(temp_path, output_path) {
        if had_existing_output {
            let _ = fs::rename(&backup_path, output_path);
        }
        return Err(error);
    }

    if had_existing_output {
        fs::remove_file(&backup_path)?;
    }
    Ok(())
}

/// 未提交即析构时自动删除的临时文件守卫。
pub struct TemporaryFile {
    path: PathBuf,
    committed: bool,
}

impl TemporaryFile {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            committed: false,
        }
    }

    pub fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.path);
        }
    }
}
