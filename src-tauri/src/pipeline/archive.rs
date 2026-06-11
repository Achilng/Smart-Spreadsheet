//! zip / 7z / rar 解压（自 Novelai工具 移植）。
//! 解压目标固定为调用方提供的临时目录，原压缩包只读。

use std::fs::{self, File};
use std::io;
use std::path::Path;

use thiserror::Error;
use unrar_ng::Archive;
use zip::ZipArchive;

#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error("压缩包文件操作失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("无法读取 ZIP 压缩包: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("无法解压 7z 压缩包: {0}")]
    SevenZ(String),
    #[error("无法解压 RAR 压缩包: {0}")]
    Rar(String),
}

/// 返回受支持的压缩包扩展名（小写），不支持时为 None。
pub fn archive_extension(path: &Path) -> Option<String> {
    let extension = path.extension()?.to_str()?.to_lowercase();
    match extension.as_str() {
        "zip" | "7z" | "rar" => Some(extension),
        _ => None,
    }
}

pub fn extract_archive(archive_path: &Path, destination: &Path) -> Result<(), ArchiveError> {
    let extension = archive_extension(archive_path).ok_or_else(|| {
        ArchiveError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "仅支持 .zip、.7z 和 .rar 压缩包",
        ))
    })?;
    fs::create_dir_all(destination)?;
    match extension.as_str() {
        "zip" => extract_zip_archive(archive_path, destination),
        "7z" => sevenz_rust::decompress_file(archive_path, destination)
            .map_err(|error| ArchiveError::SevenZ(error.to_string())),
        "rar" => Archive::new(archive_path)
            .open_for_processing()
            .and_then(|archive| archive.extract_all(destination))
            .map_err(|error| ArchiveError::Rar(error.to_string())),
        _ => unreachable!("archive_extension only returns supported extensions"),
    }
}

fn extract_zip_archive(archive_path: &Path, destination: &Path) -> Result<(), ArchiveError> {
    let file = File::open(archive_path)?;
    let mut archive = ZipArchive::new(file)?;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(enclosed_name) = entry.enclosed_name() else {
            continue;
        };
        let output_path = destination.join(enclosed_name);

        if entry.is_dir() {
            fs::create_dir_all(&output_path)?;
            continue;
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut output = File::create(&output_path)?;
        io::copy(&mut entry, &mut output)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn detects_supported_archive_extensions() {
        assert_eq!(archive_extension(Path::new("a.ZIP")).as_deref(), Some("zip"));
        assert_eq!(archive_extension(Path::new("a.7z")).as_deref(), Some("7z"));
        assert_eq!(archive_extension(Path::new("a.rar")).as_deref(), Some("rar"));
        assert_eq!(archive_extension(Path::new("a.png")), None);
        assert_eq!(archive_extension(Path::new("archive")), None);
    }

    #[test]
    fn extracts_zip_with_nested_entries() {
        let root = test_root("zip-extract");
        fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("sample.zip");
        {
            let file = File::create(&archive_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("nested/图片 1.png", options).unwrap();
            writer.write_all(b"png-bytes").unwrap();
            writer.finish().unwrap();
        }
        let destination = root.join("out");

        extract_archive(&archive_path, &destination).unwrap();

        assert_eq!(
            fs::read(destination.join("nested").join("图片 1.png")).unwrap(),
            b"png-bytes"
        );
        let _ = fs::remove_dir_all(&root);
    }

    fn test_root(label: &str) -> PathBuf {
        let parent = Path::new(r"D:\Agent\Agent_temp");
        let parent = if parent.is_dir() {
            parent.to_owned()
        } else {
            std::env::temp_dir()
        };
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be valid")
            .as_nanos();
        parent.join(format!(
            "smart-spreadsheet-archive-{label}-{}-{nonce}",
            std::process::id()
        ))
    }
}
