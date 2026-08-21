use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use thiserror::Error;

use super::export_json::JsonExportProgress;
use super::{DataDirectory, StorageError};
use crate::db::{ExportRow, RowSelection, TagMutationError};
use crate::fsx::{TemporaryFile, has_extension, replace_output_file, unique_sibling_path};

const FORMAT: &str = "smart-spreadsheet-prompt-rotation";
const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptRotationJsonExportOutcome {
    pub destination: PathBuf,
    pub exported: usize,
}

#[derive(Debug, Error)]
pub enum PromptRotationJsonExportError {
    #[error("导出路径必须是 .json 文件: {0}")]
    InvalidExtension(PathBuf),
    #[error("没有选中可导出的图片")]
    EmptySelection,
    #[error("应用数据目录不可用: {0}")]
    Storage(#[from] StorageError),
    #[error("{0}")]
    Selection(#[from] TagMutationError),
    #[error("JSON 序列化失败: {0}")]
    Json(#[from] serde_json::Error),
    #[error("导出文件操作失败: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PromptRotationDocument {
    format: &'static str,
    schema_version: u32,
    exported_at: String,
    items: Vec<PromptRotationItem>,
}

#[derive(Debug, Serialize)]
struct PromptRotationItem {
    name: String,
    prompts: PromptRotationPrompts,
}

#[derive(Debug, Serialize)]
struct PromptRotationPrompts {
    positive: String,
    character: String,
    negative: String,
}

impl DataDirectory {
    /// 将用户明确选中的图片导出为 NovelAI 轮询脚本的批量填入 JSON。
    /// 一行对应一个项目，三类提示词均原样保留；项目名优先使用备注，其次使用
    /// 原图片文件名，最后使用稳定序号。成功写完临时文件后原子替换目标。
    pub fn export_prompt_rotation_json(
        &self,
        selection: &RowSelection,
        destination: impl AsRef<Path>,
        progress: impl Fn(JsonExportProgress) + Sync,
    ) -> Result<PromptRotationJsonExportOutcome, PromptRotationJsonExportError> {
        let destination = destination.as_ref();
        if !has_extension(destination, "json") {
            return Err(PromptRotationJsonExportError::InvalidExtension(
                destination.to_owned(),
            ));
        }

        let rows = self.open_database()?.export_rows(selection)?;
        if rows.is_empty() {
            return Err(PromptRotationJsonExportError::EmptySelection);
        }
        let total = rows.len();
        progress(JsonExportProgress {
            processed: 0,
            total,
        });
        let items = rows
            .into_iter()
            .enumerate()
            .map(|(index, row)| prompt_rotation_item(row, index))
            .collect();
        let document = PromptRotationDocument {
            format: FORMAT,
            schema_version: SCHEMA_VERSION,
            exported_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
            items,
        };

        let temp_path = unique_sibling_path(destination, "json-tmp");
        let mut temp_guard = TemporaryFile::new(temp_path.clone());
        let file = File::create(&temp_path)?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, &document)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        writer.get_ref().sync_all()?;
        drop(writer);

        replace_output_file(&temp_path, destination)?;
        temp_guard.commit();
        progress(JsonExportProgress {
            processed: total,
            total,
        });

        Ok(PromptRotationJsonExportOutcome {
            destination: destination.to_owned(),
            exported: total,
        })
    }
}

fn prompt_rotation_item(row: ExportRow, index: usize) -> PromptRotationItem {
    PromptRotationItem {
        name: prompt_rotation_item_name(&row, index),
        prompts: PromptRotationPrompts {
            positive: row.positive_prompt.unwrap_or_default(),
            character: row.character_prompt.unwrap_or_default(),
            negative: row.negative_prompt.unwrap_or_default(),
        },
    }
}

fn prompt_rotation_item_name(row: &ExportRow, index: usize) -> String {
    if let Some(note) = row
        .note
        .as_deref()
        .map(str::trim)
        .filter(|note| !note.is_empty())
    {
        return note.to_owned();
    }
    for candidate in [row.image_path.as_deref(), row.stored_image_path.as_deref()]
        .into_iter()
        .flatten()
    {
        let file_name = candidate.rsplit(['/', '\\']).next().unwrap_or(candidate);
        if let Some(stem) = Path::new(file_name)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::trim)
            .filter(|stem| !stem.is_empty())
        {
            return stem.to_owned();
        }
    }
    format!("图片 {:03}", index + 1)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::Value;

    use super::*;
    use crate::db::{NewRow, SourceType};

    #[test]
    fn exports_selected_images_with_names_and_all_prompt_fields() {
        let temporary = TemporaryPromptRotationExport::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        {
            let mut database = directory.open_database().unwrap();
            database
                .append_batch(
                    SourceType::Folder,
                    r"D:\images",
                    &[
                        NewRow {
                            source_ordinal: 1,
                            identity: "first".into(),
                            positive_prompt: Some("quality,\n1girl".into()),
                            character_prompt: Some("blue eyes,\nlong hair".into()),
                            negative_prompt: Some("bad hands,\ntext".into()),
                            note: Some("  夏日白裙  ".into()),
                            image_path: Some(r"D:\images\first.png".into()),
                            ..NewRow::default()
                        },
                        NewRow {
                            source_ordinal: 2,
                            identity: "second".into(),
                            positive_prompt: Some("landscape".into()),
                            image_path: Some(r"D:\images\海边.png".into()),
                            ..NewRow::default()
                        },
                    ],
                    |_| Ok(()),
                )
                .unwrap();
        }

        let progress = std::sync::Mutex::new(Vec::new());
        let outcome = directory
            .export_prompt_rotation_json(
                &RowSelection::Explicit {
                    row_ids: vec![2, 1],
                },
                &temporary.destination,
                |value| progress.lock().unwrap().push(value),
            )
            .unwrap();
        assert_eq!(outcome.exported, 2);
        let progress = progress.into_inner().unwrap();
        assert_eq!(progress.first().unwrap().processed, 0);
        assert_eq!(progress.last().unwrap().processed, 2);

        let json: Value =
            serde_json::from_slice(&fs::read(&temporary.destination).unwrap()).unwrap();
        assert_eq!(json["format"], FORMAT);
        assert_eq!(json["schemaVersion"], 1);
        assert!(json["exportedAt"].as_str().unwrap().ends_with('Z'));
        assert_eq!(json["items"][0]["name"], "夏日白裙");
        assert_eq!(json["items"][0]["prompts"]["positive"], "quality,\n1girl");
        assert_eq!(
            json["items"][0]["prompts"]["character"],
            "blue eyes,\nlong hair"
        );
        assert_eq!(json["items"][0]["prompts"]["negative"], "bad hands,\ntext");
        assert_eq!(json["items"][1]["name"], "海边");
        assert_eq!(json["items"][1]["prompts"]["character"], "");
    }

    #[test]
    fn falls_back_to_stable_name_and_rejects_empty_selection() {
        let row = ExportRow {
            id: 7,
            time: None,
            positive_prompt: None,
            character_prompt: None,
            negative_prompt: None,
            note: Some("  ".into()),
            artists: None,
            image_folder: None,
            image_path: None,
            stored_image_path: None,
            tags: Vec::new(),
        };
        assert_eq!(prompt_rotation_item_name(&row, 4), "图片 005");

        let temporary = TemporaryPromptRotationExport::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        let error = directory
            .export_prompt_rotation_json(
                &RowSelection::Explicit {
                    row_ids: Vec::new(),
                },
                &temporary.destination,
                |_| {},
            )
            .unwrap_err();
        assert!(matches!(
            error,
            PromptRotationJsonExportError::EmptySelection
        ));
    }

    struct TemporaryPromptRotationExport {
        root: PathBuf,
        data: PathBuf,
        destination: PathBuf,
    }

    impl TemporaryPromptRotationExport {
        fn new() -> Self {
            let parent = Path::new(r"D:\Agent\Agent_temp");
            let parent = if parent.is_dir() {
                parent.to_owned()
            } else {
                std::env::temp_dir()
            };
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = parent.join(format!(
                "smart-spreadsheet-prompt-rotation-export-{}-{nonce}",
                std::process::id()
            ));
            Self {
                data: root.join("data"),
                destination: root.join("exported.json"),
                root,
            }
        }
    }

    impl Drop for TemporaryPromptRotationExport {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
