//! 智绘姬 JSON 去重（自 Novelai工具 原样移植）。
//! 依赖 serde_json 的 preserve_order 特性保持 presets 的原始顺序。

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::fsx::{
    TemporaryFile, canonical_or_original, has_extension, replace_output_file,
    unique_sibling_path,
};

const PREVIEW_LIMIT: usize = 3;
const PROGRESS_INTERVAL: usize = 250;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JsonDedupePreviewItem {
    pub preset_key: String,
    pub fixed_prompt: String,
    pub negative_prompt: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JsonDedupeInspection {
    pub original_count: usize,
    pub duplicate_count: usize,
    pub unique_count: usize,
    pub preview: Vec<JsonDedupePreviewItem>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JsonDedupeProgress {
    pub total: usize,
    pub processed: usize,
    pub duplicate_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JsonDedupeSummary {
    pub original_count: usize,
    pub duplicate_count: usize,
    pub unique_count: usize,
    pub output_path: String,
}

pub fn inspect_zhihuiji_json_file(input_path: &Path) -> Result<JsonDedupeInspection> {
    let document = read_document(input_path)?;
    let presets = presets_object(&document)?;
    Ok(inspect_presets(presets))
}

pub fn dedupe_zhihuiji_json_file(
    input_path: &Path,
    output_path: &Path,
    mut on_progress: impl FnMut(JsonDedupeProgress),
) -> Result<JsonDedupeSummary> {
    validate_output_path(input_path, output_path)?;
    let mut document = read_document(input_path)?;
    let presets = presets_object_mut(&mut document)?;
    let original_presets = std::mem::take(presets);
    let original_count = original_presets.len();

    let mut seen = HashSet::new();
    let mut retained = Map::new();
    let mut duplicate_count = 0;

    for (index, (_, preset)) in original_presets.into_iter().enumerate() {
        let processed = index + 1;
        let is_duplicate = dedupe_key(&preset).is_some_and(|key| !seen.insert(key));
        if is_duplicate {
            duplicate_count += 1;
        } else {
            retained.insert((retained.len() + 1).to_string(), preset);
        }

        if processed % PROGRESS_INTERVAL == 0 || processed == original_count {
            on_progress(JsonDedupeProgress {
                total: original_count,
                processed,
                duplicate_count,
            });
        }
    }

    let unique_count = retained.len();
    *presets = retained;
    write_document_safely(&document, output_path)?;

    Ok(JsonDedupeSummary {
        original_count,
        duplicate_count,
        unique_count,
        output_path: output_path.display().to_string(),
    })
}

fn inspect_presets(presets: &Map<String, Value>) -> JsonDedupeInspection {
    let mut seen = HashSet::new();
    let mut duplicate_count = 0;
    let mut preview = Vec::with_capacity(PREVIEW_LIMIT);

    for (preset_key, preset) in presets {
        if dedupe_key(preset).is_some_and(|key| !seen.insert(key)) {
            duplicate_count += 1;
        }

        if preview.len() < PREVIEW_LIMIT {
            preview.push(JsonDedupePreviewItem {
                preset_key: preset_key.clone(),
                fixed_prompt: string_field(preset, "fixedPrompt")
                    .unwrap_or_default()
                    .to_string(),
                negative_prompt: string_field(preset, "negativePrompt")
                    .unwrap_or_default()
                    .to_string(),
            });
        }
    }

    JsonDedupeInspection {
        original_count: presets.len(),
        duplicate_count,
        unique_count: presets.len() - duplicate_count,
        preview,
    }
}

fn dedupe_key(preset: &Value) -> Option<String> {
    let prompt = string_field(preset, "fixedPrompt")?.trim();
    (!prompt.is_empty()).then(|| prompt.to_string())
}

fn string_field<'a>(preset: &'a Value, field: &str) -> Option<&'a str> {
    preset.as_object()?.get(field)?.as_str()
}

fn read_document(input_path: &Path) -> Result<Value> {
    validate_input_path(input_path)?;
    let file = File::open(input_path)
        .with_context(|| format!("无法打开 JSON 文件：{}", input_path.display()))?;
    serde_json::from_reader(BufReader::new(file))
        .with_context(|| format!("无法解析 JSON 文件：{}", input_path.display()))
}

fn presets_object(document: &Value) -> Result<&Map<String, Value>> {
    let root = document.as_object().context("JSON 顶层必须是对象。")?;
    root.get("presets")
        .and_then(Value::as_object)
        .context("JSON 中缺少有效的 presets 对象。")
}

fn presets_object_mut(document: &mut Value) -> Result<&mut Map<String, Value>> {
    let root = document.as_object_mut().context("JSON 顶层必须是对象。")?;
    root.get_mut("presets")
        .and_then(Value::as_object_mut)
        .context("JSON 中缺少有效的 presets 对象。")
}

fn validate_input_path(input_path: &Path) -> Result<()> {
    if !input_path.is_file() {
        bail!("请选择有效的 JSON 文件。");
    }
    if !has_extension(input_path, "json") {
        bail!("输入文件必须是 .json 文件。");
    }
    Ok(())
}

fn validate_output_path(input_path: &Path, output_path: &Path) -> Result<()> {
    if !has_extension(output_path, "json") {
        bail!("输出路径必须是 .json 文件。");
    }
    if canonical_or_original(input_path) == canonical_or_original(output_path) {
        bail!("输入和输出路径不能相同。");
    }
    let parent = output_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .context("输出路径缺少父目录。")?;
    if !parent.is_dir() {
        bail!("输出目录不存在：{}", parent.display());
    }
    Ok(())
}

fn write_document_safely(document: &Value, output_path: &Path) -> Result<()> {
    let temp_path = unique_sibling_path(output_path, "tmp");
    let mut temp_guard = TemporaryFile::new(temp_path.clone());
    let file = File::create(&temp_path)
        .with_context(|| format!("无法创建临时 JSON 文件：{}", temp_path.display()))?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, document).context("无法序列化去重后的 JSON")?;
    writer.write_all(b"\n")?;
    writer.flush().context("无法写完 JSON 文件")?;
    writer
        .get_ref()
        .sync_all()
        .context("无法同步 JSON 文件到磁盘")?;
    drop(writer);

    replace_output_file(&temp_path, output_path)
        .with_context(|| format!("无法保存 JSON 文件：{}", output_path.display()))?;
    temp_guard.commit();
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use serde_json::Value;

    use super::{dedupe_zhihuiji_json_file, inspect_zhihuiji_json_file};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn deduplicates_in_input_order_and_preserves_other_data() {
        let root = test_root("standard");
        let input = root.join("input.json");
        let output = root.join("output.json");
        fs::write(
            &input,
            r#"{
  "meta": {"name": "测试"},
  "presets": {
    "8": {"fixedPrompt": " same prompt ", "negativePrompt": "first", "extra": 1},
    "2": {"fixedPrompt": "same prompt", "negativePrompt": "duplicate"},
    "9": {"fixedPrompt": "Same prompt", "negativePrompt": "case-sensitive"},
    "11": {"fixedPrompt": "   ", "negativePrompt": "blank-one"},
    "12": {"fixedPrompt": "", "negativePrompt": "blank-two"},
    "13": {"negativePrompt": "missing"}
  },
  "images": {"keep": {"path": "a.png"}}
}"#,
        )
        .unwrap();

        let inspection = inspect_zhihuiji_json_file(&input).unwrap();
        assert_eq!(inspection.original_count, 6);
        assert_eq!(inspection.duplicate_count, 1);
        assert_eq!(inspection.unique_count, 5);
        assert_eq!(inspection.preview[0].preset_key, "8");

        let mut progress = Vec::new();
        let summary =
            dedupe_zhihuiji_json_file(&input, &output, |event| progress.push(event)).unwrap();
        assert_eq!(summary.original_count, 6);
        assert_eq!(summary.duplicate_count, 1);
        assert_eq!(summary.unique_count, 5);
        assert_eq!(progress.last().unwrap().processed, 6);

        let document: Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
        let presets = document["presets"].as_object().unwrap();
        assert_eq!(
            presets.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["1", "2", "3", "4", "5"]
        );
        assert_eq!(presets["1"]["negativePrompt"], "first");
        assert_eq!(presets["1"]["extra"], 1);
        assert_eq!(presets["2"]["negativePrompt"], "case-sensitive");
        assert_eq!(presets["3"]["negativePrompt"], "blank-one");
        assert_eq!(presets["4"]["negativePrompt"], "blank-two");
        assert_eq!(presets["5"]["negativePrompt"], "missing");
        assert_eq!(document["images"]["keep"]["path"], "a.png");
        assert_eq!(document["meta"]["name"], "测试");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_invalid_structure_and_same_output_path() {
        let root = test_root("invalid");
        let missing_presets = root.join("missing.json");
        let wrong_presets = root.join("wrong.json");
        fs::write(&missing_presets, r#"{"images": {}}"#).unwrap();
        fs::write(&wrong_presets, r#"{"presets": []}"#).unwrap();

        assert!(
            inspect_zhihuiji_json_file(&missing_presets)
                .unwrap_err()
                .to_string()
                .contains("presets")
        );
        assert!(
            inspect_zhihuiji_json_file(&wrong_presets)
                .unwrap_err()
                .to_string()
                .contains("presets")
        );

        let valid = root.join("valid.json");
        fs::write(&valid, r#"{"presets": {}, "images": {}}"#).unwrap();
        assert!(
            dedupe_zhihuiji_json_file(&valid, &valid, |_| {})
                .unwrap_err()
                .to_string()
                .contains("不能相同")
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn handles_large_batches_with_contiguous_numbering() {
        let root = test_root("large");
        let input = root.join("input.json");
        let output = root.join("output.json");
        let mut presets = serde_json::Map::new();
        for index in 0..1_000 {
            presets.insert(
                (index + 10).to_string(),
                serde_json::json!({"fixedPrompt": format!("prompt-{}", index % 500)}),
            );
        }
        let document = serde_json::json!({"presets": presets, "images": {}});
        fs::write(&input, serde_json::to_vec(&document).unwrap()).unwrap();

        let summary = dedupe_zhihuiji_json_file(&input, &output, |_| {}).unwrap();
        assert_eq!(summary.original_count, 1_000);
        assert_eq!(summary.duplicate_count, 500);
        assert_eq!(summary.unique_count, 500);

        let output_document: Value =
            serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
        let output_presets = output_document["presets"].as_object().unwrap();
        assert_eq!(output_presets.len(), 500);
        assert!(output_presets.contains_key("500"));

        fs::remove_dir_all(root).unwrap();
    }

    fn test_root(name: &str) -> PathBuf {
        let counter = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = Path::new(r"D:\Agent\Agent_temp")
            .join("smart-spreadsheet-tests")
            .join(format!("json-dedupe-{name}-{}-{counter}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        root
    }
}
