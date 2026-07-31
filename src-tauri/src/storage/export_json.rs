use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use thiserror::Error;

use super::{DataDirectory, StorageError};
use crate::db::{RowSelection, TagMutationError};
use crate::fsx::{TemporaryFile, has_extension, replace_output_file, unique_sibling_path};

const PROGRESS_EVERY_ROWS: usize = 250;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JsonExportProgress {
    pub processed: usize,
    pub total: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonExportOutcome {
    pub destination: PathBuf,
    pub exported: usize,
}

#[derive(Debug, Error)]
pub enum JsonExportError {
    #[error("导出路径必须是 .json 文件: {0}")]
    InvalidExtension(PathBuf),
    #[error("没有可导出的行")]
    EmptySelection,
    #[error("第 {position} 个导出项未填写备注；智绘姬 JSON 的预设名称不能为空")]
    EmptyNote { position: usize },
    #[error("导出项 {first_position} 和 {second_position} 的备注重复：{note}")]
    DuplicateNote {
        note: String,
        first_position: usize,
        second_position: usize,
    },
    #[error("应用数据目录不可用: {0}")]
    Storage(#[from] StorageError),
    #[error("{0}")]
    Selection(#[from] TagMutationError),
    #[error("JSON 序列化失败: {0}")]
    Json(#[from] serde_json::Error),
    #[error("导出文件操作失败: {0}")]
    Io(#[from] std::io::Error),
}

impl DataDirectory {
    /// 把选中行导出为智绘姬 JSON：按入库顺序输出，以备注作为 preset 键，
    /// 正向提示词 → fixedPrompt、负向提示词 → negativePrompt，
    /// `fixedPrompt_end` 为空串、顶层 `images` 为空对象。
    /// 逐条写入临时文件，成功后原子替换目标。
    pub fn export_zhihuiji_json(
        &self,
        selection: &RowSelection,
        destination: impl AsRef<Path>,
        use_numeric_names_for_empty: bool,
        progress: impl Fn(JsonExportProgress) + Sync,
    ) -> Result<JsonExportOutcome, JsonExportError> {
        let destination = destination.as_ref();
        if !has_extension(destination, "json") {
            return Err(JsonExportError::InvalidExtension(destination.to_owned()));
        }

        let rows = self.open_database()?.export_rows(selection)?;
        if rows.is_empty() {
            return Err(JsonExportError::EmptySelection);
        }
        let mut preset_names = Vec::with_capacity(rows.len());
        let mut first_position_by_name = HashMap::with_capacity(rows.len());
        for (index, row) in rows.iter().enumerate() {
            let position = index + 1;
            let name = row
                .note
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .or_else(|| use_numeric_names_for_empty.then(|| position.to_string()))
                .ok_or(JsonExportError::EmptyNote { position })?;
            if let Some(first_position) = first_position_by_name.insert(name.clone(), position) {
                return Err(JsonExportError::DuplicateNote {
                    note: name,
                    first_position,
                    second_position: position,
                });
            }
            preset_names.push(name);
        }
        let total = rows.len();
        progress(JsonExportProgress {
            processed: 0,
            total,
        });

        let temp_path = unique_sibling_path(destination, "json-tmp");
        let mut temp_guard = TemporaryFile::new(temp_path.clone());
        let file = File::create(&temp_path)?;
        let mut writer = BufWriter::new(file);

        writer.write_all(b"{\n  \"presets\": {")?;
        for (index, row) in rows.iter().enumerate() {
            let exported = index + 1;
            if exported > 1 {
                writer.write_all(b",")?;
            }
            writer.write_all(b"\n    ")?;
            serde_json::to_writer(&mut writer, &preset_names[index])?;
            writer.write_all(b": {\n      \"fixedPrompt\": ")?;
            serde_json::to_writer(&mut writer, row.positive_prompt.as_deref().unwrap_or(""))?;
            writer.write_all(b",\n      \"fixedPrompt_end\": \"\",\n      \"negativePrompt\": ")?;
            serde_json::to_writer(&mut writer, row.negative_prompt.as_deref().unwrap_or(""))?;
            writer.write_all(b"\n    }")?;

            if exported % PROGRESS_EVERY_ROWS == 0 || exported == total {
                progress(JsonExportProgress {
                    processed: exported,
                    total,
                });
            }
        }
        writer.write_all(b"\n  },\n  \"images\": {}\n}\n")?;
        writer.flush()?;
        writer.get_ref().sync_all()?;
        drop(writer);

        replace_output_file(&temp_path, destination)?;
        temp_guard.commit();

        Ok(JsonExportOutcome {
            destination: destination.to_owned(),
            exported: total,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::Value;

    use super::*;
    use crate::db::{NewRow, SourceType, TagMatchMode};

    #[test]
    fn exports_selection_with_notes_as_preset_names() {
        let temporary = TemporaryJsonExport::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        {
            let mut database = directory.open_database().unwrap();
            let rows: Vec<NewRow> = [
                ("第一行\n\"引号\"与中文", Some("负向一")),
                ("second", None),
            ]
            .iter()
            .enumerate()
            .map(|(index, (positive, negative))| NewRow {
                source_ordinal: (index + 1) as u32,
                identity: format!("file:test\\{index}.png"),
                positive_prompt: Some((*positive).to_owned()),
                negative_prompt: negative.map(str::to_owned),
                note: Some(["预设一", "预设二"][index].into()),
                ..NewRow::default()
            })
            .collect();
            database
                .append_batch(SourceType::Folder, r"D:\test", &rows, |_| Ok(()))
                .unwrap();
        }

        let outcome = directory
            .export_zhihuiji_json(
                &RowSelection::Filtered {
                    tags: Vec::new(),
                    tag_mode: TagMatchMode::And,
                    dedupe: crate::db::DedupeMode::None,
                    single_artist_only: false,
                    artist_filter: String::new(),
                    has_vibe: false,
                    untagged_only: false,
                    search: String::new(),
                    excluded_row_ids: Vec::new(),
                },
                &temporary.destination,
                false,
                |_| {},
            )
            .unwrap();

        assert_eq!(outcome.exported, 2);
        let json: Value =
            serde_json::from_slice(&fs::read(&temporary.destination).unwrap()).unwrap();
        assert_eq!(json["presets"]["预设一"]["fixedPrompt"], "第一行\n\"引号\"与中文");
        assert_eq!(json["presets"]["预设一"]["fixedPrompt_end"], "");
        assert_eq!(json["presets"]["预设一"]["negativePrompt"], "负向一");
        assert_eq!(json["presets"]["预设二"]["fixedPrompt"], "second");
        assert_eq!(json["presets"]["预设二"]["negativePrompt"], "");
        assert_eq!(json["images"], serde_json::json!({}));
    }

    #[test]
    fn replaces_existing_destination_atomically() {
        let temporary = TemporaryJsonExport::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        {
            let mut database = directory.open_database().unwrap();
            database
                .append_batch(
                    SourceType::Folder,
                    r"D:\test",
                    &[NewRow {
                        source_ordinal: 1,
                        identity: "file:one".into(),
                        positive_prompt: Some("replaced".into()),
                        note: Some("替换后的预设".into()),
                        ..NewRow::default()
                    }],
                    |_| Ok(()),
                )
                .unwrap();
        }
        fs::write(&temporary.destination, b"{\"old\": true}").unwrap();

        directory
            .export_zhihuiji_json(
                &RowSelection::Explicit { row_ids: vec![1] },
                &temporary.destination,
                false,
                |_| {},
            )
            .unwrap();

        let json: Value =
            serde_json::from_slice(&fs::read(&temporary.destination).unwrap()).unwrap();
        assert_eq!(json["presets"]["替换后的预设"]["fixedPrompt"], "replaced");
    }

    #[test]
    fn confirms_blank_fallback_and_rejects_all_name_collisions() {
        let temporary = TemporaryJsonExport::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        {
            let mut database = directory.open_database().unwrap();
            database
                .append_batch(
                    SourceType::Folder,
                    r"D:\test",
                    &[
                        NewRow {
                            source_ordinal: 1,
                            identity: "file:one".into(),
                            positive_prompt: Some("one".into()),
                            ..NewRow::default()
                        },
                        NewRow {
                            source_ordinal: 2,
                            identity: "file:two".into(),
                            positive_prompt: Some("two".into()),
                            note: Some("同名".into()),
                            ..NewRow::default()
                        },
                    ],
                    |_| Ok(()),
                )
                .unwrap();
        }

        let blank = directory
            .export_zhihuiji_json(
                &RowSelection::Explicit { row_ids: vec![1] },
                &temporary.destination,
                false,
                |_| {},
            )
            .unwrap_err();
        assert!(matches!(blank, JsonExportError::EmptyNote { position: 1 }));
        assert!(!temporary.destination.exists());

        directory
            .export_zhihuiji_json(
                &RowSelection::Explicit { row_ids: vec![1, 2] },
                &temporary.destination,
                true,
                |_| {},
            )
            .unwrap();
        let json: Value =
            serde_json::from_slice(&fs::read(&temporary.destination).unwrap()).unwrap();
        assert_eq!(json["presets"]["1"]["fixedPrompt"], "one");
        assert_eq!(json["presets"]["同名"]["fixedPrompt"], "two");
        fs::remove_file(&temporary.destination).unwrap();

        {
            let mut database = directory.open_database().unwrap();
            database.update_note(1, "同名").unwrap();
        }
        let duplicate = directory
            .export_zhihuiji_json(
                &RowSelection::Explicit { row_ids: vec![1, 2] },
                &temporary.destination,
                false,
                |_| {},
            )
            .unwrap_err();
        assert!(matches!(
            duplicate,
            JsonExportError::DuplicateNote {
                first_position: 1,
                second_position: 2,
                ..
            }
        ));
        assert!(!temporary.destination.exists());

        {
            let mut database = directory.open_database().unwrap();
            database.update_note(1, "2").unwrap();
            database.update_note(2, "").unwrap();
        }
        let fallback_collision = directory
            .export_zhihuiji_json(
                &RowSelection::Explicit { row_ids: vec![1, 2] },
                &temporary.destination,
                true,
                |_| {},
            )
            .unwrap_err();
        assert!(matches!(
            fallback_collision,
            JsonExportError::DuplicateNote { ref note, .. } if note == "2"
        ));
        assert!(!temporary.destination.exists());
    }

    struct TemporaryJsonExport {
        root: PathBuf,
        data: PathBuf,
        destination: PathBuf,
    }

    impl TemporaryJsonExport {
        fn new() -> Self {
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
            let root = parent.join(format!(
                "smart-spreadsheet-export-json-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(&root).unwrap();
            Self {
                data: root.join("data"),
                destination: root.join("exported.json"),
                root,
            }
        }
    }

    impl Drop for TemporaryJsonExport {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
