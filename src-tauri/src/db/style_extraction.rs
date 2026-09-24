//! External LLM exchange. Matching never trims or normalizes prompt text.
use super::{Database, DatabaseError, RowSelection};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

pub const REQUEST_FORMAT: &str = "smart-spreadsheet.style-extraction.request";
pub const RESULT_FORMAT: &str = "smart-spreadsheet.style-extraction.result";
pub fn prompt_hash(text: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(text.as_bytes()))
}
fn invalid(text: impl Into<String>) -> DatabaseError {
    DatabaseError::IntegrityCheckFailed(text.into())
}
fn stamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestItem {
    pub id: String,
    pub positive_prompt: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct RequestDocument {
    pub format: String,
    pub version: u32,
    pub export_id: String,
    pub items: Vec<RequestItem>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSummary {
    pub rows: usize,
    pub skipped: usize,
    pub unique_prompts: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Processor {
    pub model: String,
    pub prompt_version: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ResultItem {
    pub id: String,
    pub positive_prompt: String,
    pub status: String,
    #[serde(default)]
    pub artist_string: String,
    #[serde(default)]
    pub processed_at: Option<String>,
}
#[derive(Debug, Deserialize)]
pub struct ResultDocument {
    pub format: String,
    pub version: u32,
    pub export_id: String,
    pub processor: Processor,
    pub items: Vec<ResultItem>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LlmSource {
    pub input_hash: String,
    pub artist_string: String,
    pub model: String,
    pub prompt_version: String,
    pub processed_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleChange {
    pub row_id: i64,
    pub input_hash: String,
    pub old_artists: Option<String>,
    pub old_source: Option<String>,
    pub new_artists: Option<String>,
    pub new_source: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub library_id: String,
    pub changes: Vec<StyleChange>,
    pub matched_rows: usize,
    pub unchanged: usize,
    pub empty_results: usize,
    pub failed: usize,
    pub unmatched: usize,
    pub invalid_items: usize,
    pub issues: Vec<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub changes: Vec<StyleChange>,
    pub conflicts: usize,
}

impl Database {
    pub fn export_style_request(
        &mut self,
        selection: Option<&RowSelection>,
        include_processed: bool,
    ) -> Result<(RequestDocument, ExportSummary), DatabaseError> {
        let ids = if let Some(selection) = selection {
            Some(
                self.selected_row_ids(selection)
                    .map_err(|e| invalid(e.to_string()))?
                    .into_iter()
                    .collect::<HashSet<_>>(),
            )
        } else {
            None
        };
        let mut statement = self
            .connection
            .prepare("SELECT id, positive_prompt, artist_llm FROM rows ORDER BY id")?;
        let mut rows = statement.query([])?;
        let mut items = Vec::new();
        let mut seen = HashSet::new();
        let mut count = 0;
        let mut skipped = 0;
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            if ids.as_ref().is_some_and(|ids| !ids.contains(&id)) {
                continue;
            }
            count += 1;
            let prompt: Option<String> = row.get(1)?;
            let source: Option<String> = row.get(2)?;
            let Some(prompt) = prompt.filter(|text| !text.trim().is_empty()) else {
                skipped += 1;
                continue;
            };
            if !include_processed && source.is_some() {
                skipped += 1;
                continue;
            }
            if seen.insert(prompt.clone()) {
                items.push(RequestItem {
                    id: prompt_hash(&prompt),
                    positive_prompt: prompt,
                });
            }
        }
        let summary = ExportSummary {
            rows: count,
            skipped,
            unique_prompts: items.len(),
        };
        Ok((
            RequestDocument {
                format: REQUEST_FORMAT.into(),
                version: 1,
                export_id: stamp(),
                items,
            },
            summary,
        ))
    }

    pub fn preview_style_result(
        &self,
        doc: &ResultDocument,
    ) -> Result<ImportPreview, DatabaseError> {
        if doc.format != RESULT_FORMAT
            || doc.version != 1
            || doc.export_id.is_empty()
            || doc.processor.model.trim().is_empty()
            || doc.processor.prompt_version.trim().is_empty()
        {
            return Err(invalid("不是支持的画风提取结果 v1 文件"));
        }
        let mut preview = ImportPreview {
            library_id: self
                .setting("llm_library_id")?
                .ok_or_else(|| invalid("资料库缺少标识"))?,
            changes: vec![],
            matched_rows: 0,
            unchanged: 0,
            empty_results: 0,
            failed: 0,
            unmatched: 0,
            invalid_items: 0,
            issues: vec![],
        };
        let mut counts = HashMap::new();
        for item in &doc.items {
            *counts.entry(&item.id).or_insert(0) += 1;
        }
        let mut valid = HashMap::new();
        for item in &doc.items {
            let reason = if counts[&item.id] > 1 {
                Some("编号重复")
            } else if item.id != prompt_hash(&item.positive_prompt)
                || item.positive_prompt.trim().is_empty()
            {
                Some("原文哈希不符或正文为空")
            } else if item.status == "error" {
                preview.failed += 1;
                continue;
            } else if !((item.status == "none" && item.artist_string.is_empty())
                || (item.status == "ok"
                    && !item.artist_string.is_empty()
                    && item.positive_prompt.contains(&item.artist_string)))
            {
                Some("状态错误或结果不是连续原文")
            } else {
                None
            };
            if let Some(reason) = reason {
                preview.invalid_items += 1;
                if preview.issues.len() < 100 {
                    preview.issues.push(format!("{}：{reason}", item.id));
                }
                continue;
            }
            valid.insert(item.id.as_str(), item);
        }
        let mut matched = HashSet::new();
        let mut statement = self.connection.prepare("SELECT id, positive_prompt, artists, artist_llm FROM rows WHERE positive_prompt IS NOT NULL ORDER BY id")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let positive: String = row.get(1)?;
            let hash = prompt_hash(&positive);
            let Some(item) = valid
                .get(hash.as_str())
                .filter(|item| item.positive_prompt == positive)
            else {
                continue;
            };
            matched.insert(item.id.as_str());
            preview.matched_rows += 1;
            let old_artists: Option<String> = row.get(2)?;
            let old_source: Option<String> = row.get(3)?;
            if item.status == "none" {
                preview.empty_results += 1;
            }
            if old_artists.as_deref() == Some(&item.artist_string)
                && old_source
                    .as_ref()
                    .and_then(|s| serde_json::from_str::<LlmSource>(s).ok())
                    .is_some_and(|s| {
                        s.input_hash == hash
                            && s.model == doc.processor.model
                            && s.prompt_version == doc.processor.prompt_version
                    })
            {
                preview.unchanged += 1;
                continue;
            }
            let source = LlmSource {
                input_hash: hash.clone(),
                artist_string: item.artist_string.clone(),
                model: doc.processor.model.clone(),
                prompt_version: doc.processor.prompt_version.clone(),
                processed_at: item.processed_at.clone().unwrap_or_else(stamp),
            };
            preview.changes.push(StyleChange {
                row_id: row.get(0)?,
                input_hash: hash,
                old_artists,
                old_source,
                new_artists: Some(item.artist_string.clone()),
                new_source: Some(
                    serde_json::to_string(&source).map_err(|e| invalid(e.to_string()))?,
                ),
            });
        }
        preview.unmatched = valid.len() - matched.len();
        Ok(preview)
    }

    /// Optimistic, field-specific changes avoid undo overwriting unrelated edits.
    pub fn apply_style_changes(
        &mut self,
        library_id: &str,
        changes: &[StyleChange],
        reverse: bool,
        strict: bool,
    ) -> Result<ApplyResult, DatabaseError> {
        if self.setting("llm_library_id")?.as_deref() != Some(library_id) {
            return Err(invalid("资料库已切换，请重新预览"));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut applied = Vec::new();
        let mut conflicts = 0;
        let mut seen = HashSet::new();
        for change in changes {
            if !seen.insert(change.row_id) {
                return Err(invalid("修改列表包含重复行"));
            }
            let row: Option<(Option<String>, Option<String>, Option<String>)> = transaction
                .query_row(
                    "SELECT positive_prompt, artists, artist_llm FROM rows WHERE id = ?1",
                    [change.row_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let (expected_artists, expected_source, desired_artists, desired_source) = if reverse {
                (
                    &change.new_artists,
                    &change.new_source,
                    &change.old_artists,
                    &change.old_source,
                )
            } else {
                (
                    &change.old_artists,
                    &change.old_source,
                    &change.new_artists,
                    &change.new_source,
                )
            };
            let Some((prompt, artists, source)) = row.filter(|(p, a, s)| {
                prompt_hash(p.as_deref().unwrap_or_default()) == change.input_hash
                    && a == expected_artists
                    && s == expected_source
            }) else {
                conflicts += 1;
                if strict {
                    return Err(invalid("记录已被其他操作修改，未执行撤销或重做"));
                }
                continue;
            };
            let _ = (artists, source);
            if let Some(source) = desired_source {
                let info: LlmSource =
                    serde_json::from_str(source).map_err(|e| invalid(e.to_string()))?;
                if info.input_hash != change.input_hash
                    || desired_artists.as_deref() != Some(&info.artist_string)
                    || !prompt
                        .as_deref()
                        .unwrap_or_default()
                        .contains(&info.artist_string)
                {
                    return Err(invalid("LLM 来源与当前正文不一致"));
                }
            }
            transaction.execute(
                "UPDATE rows SET artists = ?2, artist_llm = ?3 WHERE id = ?1",
                params![change.row_id, desired_artists, desired_source],
            )?;
            applied.push(change.clone());
        }
        transaction.commit()?;
        self.bump_data_version();
        Ok(ApplyResult {
            changes: applied,
            conflicts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::database_with_rows;
    fn document(prompt: &str, artist: &str) -> ResultDocument {
        ResultDocument {
            format: RESULT_FORMAT.into(),
            version: 1,
            export_id: "test".into(),
            processor: Processor {
                model: "gpt-6-luna".into(),
                prompt_version: "v1".into(),
            },
            items: vec![ResultItem {
                id: prompt_hash(prompt),
                positive_prompt: prompt.into(),
                status: if artist.is_empty() { "none" } else { "ok" }.into(),
                artist_string: artist.into(),
                processed_at: None,
            }],
        }
    }
    #[test]
    fn exact_dedupe_apply_empty_protection_invalidation_and_undo() {
        let mut db = database_with_rows(3);
        db.update_positive_prompt(1, "artist:a, girl").unwrap();
        db.update_positive_prompt(2, "artist:a, girl").unwrap();
        db.update_positive_prompt(3, "artist:a, girl ").unwrap();
        let (doc, summary) = db.export_style_request(None, false).unwrap();
        assert_eq!(doc.items.len(), 2);
        assert_eq!(summary.rows, 3);
        let preview = db
            .preview_style_result(&document("artist:a, girl", ""))
            .unwrap();
        assert_eq!(preview.changes.len(), 2);
        let result = db
            .apply_style_changes(&preview.library_id, &preview.changes, false, false)
            .unwrap();
        assert_eq!(result.changes.len(), 2);
        db.update_character_prompt(1, "artist:b").unwrap();
        let row = db.get_rows_by_ids(&[1]).unwrap().remove(0);
        assert_eq!(row.artists.as_deref(), Some(""));
        assert!(row.artist_llm.is_some());
        assert_eq!(
            db.preview_style_result(&document("artist:a, girl", ""))
                .unwrap()
                .unchanged,
            2
        );
        db.apply_style_changes(&preview.library_id, &result.changes, true, true)
            .unwrap();
        assert!(db.get_rows_by_ids(&[1]).unwrap()[0].artist_llm.is_none());
        db.apply_style_changes(&preview.library_id, &result.changes, false, true)
            .unwrap();
        db.update_positive_prompt(1, "artist:c, girl").unwrap();
        assert!(db.get_rows_by_ids(&[1]).unwrap()[0].artist_llm.is_none());
        assert!(
            db.apply_style_changes(&preview.library_id, &result.changes, true, true)
                .is_err()
        );
        assert!(db.get_rows_by_ids(&[2]).unwrap()[0].artist_llm.is_some());
    }
    #[test]
    fn rejects_rewrites_duplicates_and_changed_previews() {
        let mut db = database_with_rows(1);
        db.update_positive_prompt(1, "0.5::A, B::, girl").unwrap();
        let p = "0.5::A, B::, girl";
        assert_eq!(
            db.preview_style_result(&document(p, "0.5::a, B::"))
                .unwrap()
                .invalid_items,
            1
        );
        let mut doc = document(p, "0.5::A, B::, ");
        doc.items.push(doc.items[0].clone());
        assert_eq!(db.preview_style_result(&doc).unwrap().invalid_items, 2);
        let preview = db
            .preview_style_result(&document(p, "0.5::A, B::, "))
            .unwrap();
        db.update_positive_prompt(1, "changed").unwrap();
        assert_eq!(
            db.apply_style_changes(&preview.library_id, &preview.changes, false, false)
                .unwrap()
                .conflicts,
            1
        );
    }
    #[test]
    fn source_survives_reopen_and_prefix_undo() {
        let folder = std::path::Path::new("D:/Agent/Agent_temp/style-extractor-db-tests");
        std::fs::create_dir_all(folder).unwrap();
        let path = folder.join(format!("{}.sqlite3", stamp()));
        let mut db = Database::open(&path).unwrap();
        crate::db::test_support::append_rows(&mut db, &crate::db::test_support::test_rows(1));
        db.update_positive_prompt(1, "alice, quality, girl")
            .unwrap();
        let preview = db
            .preview_style_result(&document("alice, quality, girl", "alice, quality, "))
            .unwrap();
        db.apply_style_changes(&preview.library_id, &preview.changes, false, false)
            .unwrap();
        let changes = db.apply_quick_artist_prefix("alice").unwrap().changes;
        assert!(db.get_rows_by_ids(&[1]).unwrap()[0].artist_llm.is_none());
        db.revert_quick_artist_prefix_changes(&changes).unwrap();
        assert!(db.get_rows_by_ids(&[1]).unwrap()[0].artist_llm.is_some());
        drop(db);
        let mut db = Database::open(&path).unwrap();
        let row = &db.get_rows_by_ids(&[1]).unwrap()[0];
        assert_eq!(row.artists.as_deref(), Some("alice, quality, "));
        assert!(row.artist_llm.is_some());
        let preview = db
            .preview_style_result(&document("alice, quality, girl", ""))
            .unwrap();
        db.apply_style_changes(&preview.library_id, &preview.changes, false, false)
            .unwrap();
        // Force startup repair paths to run again; the authoritative empty result wins.
        db.connection
            .execute("DELETE FROM settings WHERE key LIKE '%artist%version%'", [])
            .unwrap();
        drop(db);
        let mut db = Database::open(&path).unwrap();
        assert_eq!(
            db.get_rows_by_ids(&[1]).unwrap()[0].artists.as_deref(),
            Some("")
        );
    }
}
