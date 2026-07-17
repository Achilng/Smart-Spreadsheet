//! 行身份键：增量导入时判定“同一张图片是否已入库”的唯一键。
//!
//! Windows 路径取 TRIM 后做 `/`→`\` 替换和 ASCII 小写化。

/// 文件夹或单 PNG 来源的身份键。
pub fn file_identity(path: &str) -> String {
    format!("file:{}", normalize_path_text(path))
}

/// 压缩包内图片的身份键：压缩包路径 + 包内相对路径。
pub fn archive_member_identity(archive_path: &str, member_path: &str) -> String {
    format!(
        "archive:{}!{}",
        normalize_path_text(archive_path),
        normalize_path_text(member_path)
    )
}

fn normalize_path_text(path: &str) -> String {
    path.trim().replace('/', "\\").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_identity_normalizes_case_and_separators() {
        assert_eq!(
            file_identity(r"  D:/图片/Sample IMG.PNG "),
            r"file:d:\图片\sample img.png"
        );
        assert_eq!(
            file_identity(r"D:\图片\sample img.png"),
            file_identity(r"d:/图片/SAMPLE IMG.png")
        );
    }

    #[test]
    fn archive_identity_combines_archive_and_member() {
        assert_eq!(
            archive_member_identity(r"D:\Packs\Set.ZIP", "inner/Image 1.png"),
            r"archive:d:\packs\set.zip!inner\image 1.png"
        );
    }
}
