use crate::automation::error::AutomationRuleError;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const MAX_AUTOMATION_RULE_FILE_BYTES: u64 = 2 * 1024 * 1024;

pub fn read_automation_rule_file(path: &Path) -> Result<(Value, String), AutomationRuleError> {
    validate_rule_file_path(path)?;
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(AutomationRuleError::InvalidRuleFile(
            "选择的路径不是文件".into(),
        ));
    }
    if metadata.len() == 0 {
        return Err(AutomationRuleError::InvalidRuleFile("文件为空".into()));
    }
    if metadata.len() > MAX_AUTOMATION_RULE_FILE_BYTES {
        return Err(AutomationRuleError::InvalidRuleFile(format!(
            "文件超过 {} MB 上限",
            MAX_AUTOMATION_RULE_FILE_BYTES / 1024 / 1024
        )));
    }
    let bytes = fs::read(path)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_AUTOMATION_RULE_FILE_BYTES {
        return Err(AutomationRuleError::InvalidRuleFile(format!(
            "文件超过 {} MB 上限",
            MAX_AUTOMATION_RULE_FILE_BYTES / 1024 / 1024
        )));
    }
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let document = serde_json::from_slice(&bytes)
        .map_err(|error| AutomationRuleError::InvalidRuleFile(format!("JSON 语法错误：{error}")))?;
    Ok((document, hash))
}

pub fn parse_automation_rule_text(input: &str) -> Result<(Value, String), AutomationRuleError> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Err(AutomationRuleError::InvalidRuleFile("粘贴内容为空".into()));
    }
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_AUTOMATION_RULE_FILE_BYTES {
        return Err(AutomationRuleError::InvalidRuleFile(format!(
            "粘贴内容超过 {} MB 上限",
            MAX_AUTOMATION_RULE_FILE_BYTES / 1024 / 1024
        )));
    }
    let json = extract_automation_rule_json_text(input)?;
    let document = serde_json::from_str(&json)
        .map_err(|error| AutomationRuleError::InvalidRuleFile(format!("JSON 语法错误：{error}")))?;
    let hash = format!("{:x}", Sha256::digest(bytes));
    Ok((document, hash))
}

pub(crate) fn extract_automation_rule_json_text(
    input: &str,
) -> Result<String, AutomationRuleError> {
    let trimmed = input.trim();
    let trimmed = trimmed.strip_prefix('\u{feff}').unwrap_or(trimmed).trim();
    if trimmed.is_empty() {
        return Err(AutomationRuleError::InvalidRuleFile("粘贴内容为空".into()));
    }
    if trimmed.starts_with('{') {
        return Ok(trimmed.to_owned());
    }

    let mut found_block = false;
    let mut inside_block = false;
    let mut body = Vec::new();
    for line in trimmed.lines() {
        let marker = line.trim();
        if marker.starts_with("```") {
            if inside_block {
                if marker != "```" {
                    return Err(AutomationRuleError::InvalidRuleFile(
                        "JSON 代码块的结束标记无效".into(),
                    ));
                }
                inside_block = false;
                continue;
            }
            if found_block {
                return Err(AutomationRuleError::InvalidRuleFile(
                    "一次只能粘贴一份 JSON 代码块".into(),
                ));
            }
            if marker != "```" && !marker.eq_ignore_ascii_case("```json") {
                return Err(AutomationRuleError::InvalidRuleFile(
                    "只支持纯 JSON 或标记为 json 的代码块".into(),
                ));
            }
            found_block = true;
            inside_block = true;
            continue;
        }
        if inside_block {
            body.push(line);
        }
    }
    if inside_block {
        return Err(AutomationRuleError::InvalidRuleFile(
            "JSON 代码块缺少结束标记 ```".into(),
        ));
    }
    if !found_block {
        return Ok(trimmed.to_owned());
    }
    let json = body.join("\n");
    if json.trim().is_empty() {
        return Err(AutomationRuleError::InvalidRuleFile(
            "JSON 代码块为空".into(),
        ));
    }
    Ok(json)
}

pub fn write_automation_rule_file(
    path: &Path,
    document: &Value,
) -> Result<(), AutomationRuleError> {
    validate_rule_file_path(path)?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        return Err(AutomationRuleError::InvalidRuleFile(
            "目标文件夹不存在".into(),
        ));
    }
    let mut contents = serde_json::to_vec_pretty(document)?;
    contents.push(b'\n');
    if u64::try_from(contents.len()).unwrap_or(u64::MAX) > MAX_AUTOMATION_RULE_FILE_BYTES {
        return Err(AutomationRuleError::InvalidRuleFile(format!(
            "导出内容超过 {} MB 上限，请减少规则数量后重试",
            MAX_AUTOMATION_RULE_FILE_BYTES / 1024 / 1024
        )));
    }

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(
        ".smart-spreadsheet-rules-{}-{nonce}.tmp",
        std::process::id()
    ));
    let backup = parent.join(format!(
        ".smart-spreadsheet-rules-{}-{nonce}.bak",
        std::process::id()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(&contents)?;
    file.sync_all()?;
    drop(file);

    let had_previous = path.exists();
    if had_previous {
        fs::rename(path, &backup)?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        if had_previous && fs::rename(&backup, path).is_err() {
            return Err(AutomationRuleError::InvalidRuleFile(
                "替换目标文件失败，且无法自动恢复原文件；备份仍保留在目标目录".into(),
            ));
        }
        return Err(error.into());
    }
    if had_previous {
        let _ = fs::remove_file(backup);
    }
    Ok(())
}

pub(crate) fn validate_rule_file_path(path: &Path) -> Result<(), AutomationRuleError> {
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("json"))
    {
        return Err(AutomationRuleError::InvalidRuleFile(
            "规则文件必须使用 .json 扩展名".into(),
        ));
    }
    Ok(())
}
