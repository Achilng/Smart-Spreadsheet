//! 文件输出共用工具：同目录临时文件 + 可恢复替换。

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

/// 先把旧目标改名备份，替换失败时恢复。这是可恢复替换，不保证断电原子性。
pub fn replace_output_file(temp_path: &Path, output_path: &Path) -> std::io::Result<()> {
    let backup_path = unique_sibling_path(output_path, "backup");
    let had_existing_output = output_path.exists();

    if had_existing_output {
        if !output_path.is_file() {
            return Err(std::io::Error::other("目标路径不是普通文件，未保存"));
        }
        fs::rename(output_path, &backup_path).map_err(|error| {
            std::io::Error::new(error.kind(), format!("新文件未保存，无法备份原文件：{error}"))
        })?;
    }

    if let Err(error) = fs::rename(temp_path, output_path) {
        if had_existing_output {
            if let Err(restore) = fs::rename(&backup_path, output_path) {
                return Err(std::io::Error::other(format!(
                    "新文件保存失败：{error}；旧文件恢复失败：{restore}；备份保留在 {}",
                    backup_path.display()
                )));
            }
        }
        return Err(std::io::Error::new(error.kind(), format!("新文件保存失败，原文件未丢失：{error}")));
    }

    if had_existing_output {
        fs::remove_file(&backup_path).map_err(|error| {
            std::io::Error::new(error.kind(), format!(
                "新文件已保存，但旧备份清理失败：{error}；备份位于 {}", backup_path.display()
            ))
        })?;
    }
    Ok(())
}

/// Small exports share the same replacement path as streaming exports.
pub fn write_output_file(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let temporary = unique_sibling_path(path, "tmp");
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
    let mut guard = TemporaryFile::new(temporary.clone());
    // Close the handle before the guard removes a failed write on Windows.
    let written = file.write_all(contents).and_then(|()| file.sync_all());
    drop(file);
    written?;
    replace_output_file(&temporary, path)?;
    guard.commit();
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
