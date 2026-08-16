use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

use super::{DataDirectory, StorageError};
use crate::db::{ExportRow, RowSelection, TagMutationError};
use crate::fsx::{TemporaryFile, has_extension, replace_output_file, unique_sibling_path};
use crate::pipeline::extract_artist_tags;

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
    pub duplicates_removed: usize,
    pub artists_added: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonExportOptions {
    pub note_number_names: bool,
    pub include_artists: bool,
    pub deduplicate: bool,
}

impl Default for JsonExportOptions {
    fn default() -> Self {
        Self {
            note_number_names: true,
            include_artists: true,
            deduplicate: true,
        }
    }
}

#[derive(Debug)]
struct PreparedPreset {
    fixed_prompt: String,
    negative_prompt: String,
    note: Option<String>,
    artists_added: bool,
}

#[derive(Debug, Error)]
pub enum JsonExportError {
    #[error("导出路径必须是 .json 文件: {0}")]
    InvalidExtension(PathBuf),
    #[error("没有可导出的行")]
    EmptySelection,
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
    /// 把选中行导出为智绘姬 JSON。可把资料库画师串补入正向提示词，随后按最终
    /// `fixedPrompt` 去重；同组优先保留有备注的行。预设名称默认使用“备注_序号”，
    /// 空备注只使用序号。`fixedPrompt_end` 为空串、顶层 `images` 为空对象。
    /// 逐条写入临时文件，成功后原子替换目标。
    pub fn export_zhihuiji_json(
        &self,
        selection: &RowSelection,
        destination: impl AsRef<Path>,
        options: JsonExportOptions,
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
        let original_count = rows.len();
        let rows = prepare_presets(rows, options);
        let duplicates_removed = original_count - rows.len();
        let artists_added = rows.iter().filter(|row| row.artists_added).count();

        let mut preset_names = Vec::with_capacity(rows.len());
        let mut first_position_by_name = HashMap::with_capacity(rows.len());
        for (index, row) in rows.iter().enumerate() {
            let position = index + 1;
            let note = row.note.as_deref().unwrap_or("");
            let name = if options.note_number_names {
                if note.is_empty() {
                    position.to_string()
                } else {
                    format!("{note}_{position}")
                }
            } else {
                let name = if note.is_empty() {
                    position.to_string()
                } else {
                    note.to_owned()
                };
                if let Some(first_position) = first_position_by_name.insert(name.clone(), position)
                {
                    return Err(JsonExportError::DuplicateNote {
                        note: name,
                        first_position,
                        second_position: position,
                    });
                }
                name
            };
            if options.note_number_names {
                first_position_by_name.insert(name.clone(), position);
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
            serde_json::to_writer(&mut writer, &row.fixed_prompt)?;
            writer.write_all(b",\n      \"fixedPrompt_end\": \"\",\n      \"negativePrompt\": ")?;
            serde_json::to_writer(&mut writer, &row.negative_prompt)?;
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
            duplicates_removed,
            artists_added,
        })
    }
}

fn prepare_presets(rows: Vec<ExportRow>, options: JsonExportOptions) -> Vec<PreparedPreset> {
    let mut retained: Vec<PreparedPreset> = Vec::with_capacity(rows.len());
    let mut index_by_prompt = HashMap::<String, usize>::with_capacity(rows.len());

    for row in rows {
        let positive_prompt = row.positive_prompt.as_deref().unwrap_or("");
        let (fixed_prompt, artists_added) = if options.include_artists {
            merge_missing_artists(positive_prompt, row.artists.as_deref())
        } else {
            (positive_prompt.to_owned(), false)
        };
        let preset = PreparedPreset {
            fixed_prompt,
            negative_prompt: row.negative_prompt.unwrap_or_default(),
            note: row
                .note
                .as_deref()
                .map(str::trim)
                .filter(|note| !note.is_empty())
                .map(str::to_owned),
            artists_added,
        };

        let key = preset.fixed_prompt.trim();
        if !options.deduplicate || key.is_empty() {
            retained.push(preset);
            continue;
        }

        if let Some(&existing_index) = index_by_prompt.get(key) {
            if retained[existing_index].note.is_none() && preset.note.is_some() {
                retained[existing_index] = preset;
            }
        } else {
            index_by_prompt.insert(key.to_owned(), retained.len());
            retained.push(preset);
        }
    }

    retained
}

fn merge_missing_artists(positive_prompt: &str, artists: Option<&str>) -> (String, bool) {
    let Some(artists) = artists.filter(|value| !value.trim().is_empty()) else {
        return (positive_prompt.to_owned(), false);
    };

    let mut seen: HashSet<String> = extract_artist_tags(positive_prompt).into_iter().collect();
    let missing: Vec<String> = extract_artist_tags(artists)
        .into_iter()
        .filter(|artist| seen.insert(artist.clone()))
        .collect();
    if missing.is_empty() {
        return (positive_prompt.to_owned(), false);
    }

    let prompt = positive_prompt.trim();
    let suffix = missing.join(", ");
    let merged = if prompt.is_empty() {
        suffix
    } else if prompt.ends_with(',') {
        format!("{prompt} {suffix}")
    } else {
        format!("{prompt}, {suffix}")
    };
    (merged, true)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::Value;

    use super::*;
    use crate::db::{NewRow, SourceType, TagMatchMode};

    #[test]
    fn exports_selection_with_note_number_names() {
        let temporary = TemporaryJsonExport::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        {
            let mut database = directory.open_database().unwrap();
            let rows: Vec<NewRow> = [("第一行\n\"引号\"与中文", Some("负向一")), ("second", None)]
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
                    filters: vec![],
                    search: String::new(),
                    excluded_row_ids: Vec::new(),
                },
                &temporary.destination,
                JsonExportOptions::default(),
                |_| {},
            )
            .unwrap();

        assert_eq!(outcome.exported, 2);
        let json: Value =
            serde_json::from_slice(&fs::read(&temporary.destination).unwrap()).unwrap();
        assert_eq!(
            json["presets"]["预设一_1"]["fixedPrompt"],
            "第一行\n\"引号\"与中文"
        );
        assert_eq!(json["presets"]["预设一_1"]["fixedPrompt_end"], "");
        assert_eq!(json["presets"]["预设一_1"]["negativePrompt"], "负向一");
        assert_eq!(json["presets"]["预设二_2"]["fixedPrompt"], "second");
        assert_eq!(json["presets"]["预设二_2"]["negativePrompt"], "");
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
                JsonExportOptions::default(),
                |_| {},
            )
            .unwrap();

        let json: Value =
            serde_json::from_slice(&fs::read(&temporary.destination).unwrap()).unwrap();
        assert_eq!(json["presets"]["替换后的预设_1"]["fixedPrompt"], "replaced");
    }

    #[test]
    fn merges_artists_then_deduplicates_and_prefers_a_note() {
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
                            positive_prompt: Some("summer dress".into()),
                            negative_prompt: Some("first negative".into()),
                            artists: Some("artist:alice".into()),
                            ..NewRow::default()
                        },
                        NewRow {
                            source_ordinal: 2,
                            identity: "file:two".into(),
                            positive_prompt: Some("summer dress, artist:alice".into()),
                            negative_prompt: Some("retained negative".into()),
                            note: Some("夏日白裙".into()),
                            artists: Some("artist:alice".into()),
                            ..NewRow::default()
                        },
                        NewRow {
                            source_ordinal: 3,
                            identity: "file:three".into(),
                            positive_prompt: Some("summer dress".into()),
                            note: Some("另一位画师".into()),
                            artists: Some("artist:bob".into()),
                            ..NewRow::default()
                        },
                    ],
                    |_| Ok(()),
                )
                .unwrap();
        }

        let outcome = directory
            .export_zhihuiji_json(
                &RowSelection::Explicit {
                    row_ids: vec![1, 2, 3],
                },
                &temporary.destination,
                JsonExportOptions::default(),
                |_| {},
            )
            .unwrap();

        assert_eq!(outcome.exported, 2);
        assert_eq!(outcome.duplicates_removed, 1);
        assert_eq!(outcome.artists_added, 1);
        let json: Value =
            serde_json::from_slice(&fs::read(&temporary.destination).unwrap()).unwrap();
        assert_eq!(
            json["presets"]["夏日白裙_1"]["fixedPrompt"],
            "summer dress, artist:alice"
        );
        assert_eq!(
            json["presets"]["夏日白裙_1"]["negativePrompt"],
            "retained negative"
        );
        assert_eq!(
            json["presets"]["另一位画师_2"]["fixedPrompt"],
            "summer dress, artist:bob"
        );
    }

    #[test]
    fn options_can_keep_rows_and_legacy_names() {
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
                            positive_prompt: Some("same".into()),
                            note: Some("预设一".into()),
                            artists: Some("artist:alice".into()),
                            ..NewRow::default()
                        },
                        NewRow {
                            source_ordinal: 2,
                            identity: "file:two".into(),
                            positive_prompt: Some("same".into()),
                            note: Some("预设二".into()),
                            artists: Some("artist:alice".into()),
                            ..NewRow::default()
                        },
                    ],
                    |_| Ok(()),
                )
                .unwrap();
        }

        let outcome = directory
            .export_zhihuiji_json(
                &RowSelection::Explicit {
                    row_ids: vec![1, 2],
                },
                &temporary.destination,
                JsonExportOptions {
                    note_number_names: false,
                    include_artists: false,
                    deduplicate: false,
                },
                |_| {},
            )
            .unwrap();

        assert_eq!(outcome.exported, 2);
        assert_eq!(outcome.duplicates_removed, 0);
        assert_eq!(outcome.artists_added, 0);
        let json: Value =
            serde_json::from_slice(&fs::read(&temporary.destination).unwrap()).unwrap();
        assert_eq!(json["presets"]["预设一"]["fixedPrompt"], "same");
        assert_eq!(json["presets"]["预设二"]["fixedPrompt"], "same");
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
