//! 行身份键：增量导入时判定“同一张图片是否已入库”的唯一键。
//!
//! 规则必须与 `migrations.rs` 中 v1→v2 迁移 SQL 保持一致：
//! 路径取 TRIM 后做 `/`→`\` 替换和 ASCII 小写化（Windows 路径不区分大小写；
//! SQLite 的 LOWER 仅处理 ASCII，与 `to_ascii_lowercase` 行为一致）。

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

/// xlsx 行在“图片路径”列为空或与同批次其他行重复时的退化身份键。
///
/// 使用小写文件名而不是完整路径，与 v1→v2 迁移保持一致（v1 只保存了文件名），
/// 这样旧库迁移后重新导入同一份 xlsx 时仍能正确跳过。
pub fn xlsx_row_identity(workbook_file_name: &str, source_row: u32) -> String {
    format!(
        "xlsxrow:{}!{source_row}",
        workbook_file_name.trim().to_ascii_lowercase()
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

    #[test]
    fn xlsx_row_identity_uses_lowercased_file_name() {
        assert_eq!(
            xlsx_row_identity("NovelAI_Metadata.XLSX", 17),
            "xlsxrow:novelai_metadata.xlsx!17"
        );
    }
}
