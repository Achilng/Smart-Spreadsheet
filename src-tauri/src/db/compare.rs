//! 图片对比窗口的分区查询。全部只读、走既有索引的短查询：
//! 正常短持锁即可，不需要克隆数据目录。
//!
//! 匹配口径见 `对比功能实现计划.md` §1.3/§2：
//! - 画师串整串精确相同（与“只看当前画师串”同一归一：首尾空白）；
//! - vibe 分区比较 `vibe_signature`（整体精确），目标行的空串
//!   “已扫描但不可读”标记视为未知，一律排除；
//! - 画风分区的 `style_signature` 由 `pipeline::style_signature` 维护，
//!   目标行签名为 NULL 时在②的“不同提示词”判定中视为不同、在③④不参与；
//! - 全部分区排除样本自身，时间倒序。

use rusqlite::types::Value;
use rusqlite::{OptionalExtension, params_from_iter};
use serde::Serialize;

use super::query::{MAX_PAGE_SIZE, PAGE_ROWS_TABLE, attach_page_tags, query_page_metadata};
use super::{Database, DatabaseError};

/// ④ 分区单次返回的上限；超过时如实标记截断。
pub const COMPARE_MODEL_SECTION_CAP: i64 = 500;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareSample {
    pub row: super::RowRecord,
    /// 画风签名是否存在；不存在时③④显示“没有可比较的提示词”空态。
    pub has_style_signature: bool,
    /// 样本是否有已知的 VIBE 组合签名；无时②显示空态。
    pub has_vibe_signature: bool,
    /// 样本 VIBE 为“已扫描但原图不可读”的空串标记。
    pub vibe_signature_unreadable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareSectionPage {
    pub rows: Vec<super::RowRecord>,
    pub total_count: u64,
    pub offset: u64,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareModelSection {
    /// 画风签名相同的行按时间倒序截取，最多 500 张。
    pub rows: Vec<super::RowRecord>,
    pub total_count: u64,
    /// 匹配总数超过上限、结果被截断时为 true。
    pub truncated: bool,
}

/// 样本参与匹配的三类签名键。
struct SampleKeys {
    artists: Option<String>,
    vibe_signature: Option<String>,
    style_signature: Option<String>,
}

fn empty_page(offset: u64, limit: u32) -> CompareSectionPage {
    CompareSectionPage {
        rows: Vec::new(),
        total_count: 0,
        offset,
        limit,
    }
}

fn validate_page(offset: u64, limit: u32) -> Result<i64, DatabaseError> {
    if limit == 0 || limit > MAX_PAGE_SIZE {
        return Err(DatabaseError::InvalidPageSize {
            requested: limit,
            maximum: MAX_PAGE_SIZE,
        });
    }
    i64::try_from(offset).map_err(|_| DatabaseError::OffsetOverflow)
}

impl Database {
    /// 样本完整信息：行摘要（含三类提示词、画师串、生成参数、VIBE 数量）
    /// 加上决定各分区空态文案的签名状态。
    pub fn get_compare_sample(&mut self, row_id: i64) -> Result<CompareSample, DatabaseError> {
        let mut rows = self.get_rows_by_ids(&[row_id])?;
        let Some(row) = rows.pop() else {
            return Err(DatabaseError::RowNotFound(row_id));
        };
        let (style_signature, vibe_signature): (Option<String>, Option<String>) =
            self.connection
                .query_row(
                    "SELECT style_signature, vibe_signature FROM rows WHERE id = ?1",
                    [row_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?
                .ok_or(DatabaseError::RowNotFound(row_id))?;
        let vibe_signature_unreadable = vibe_signature.as_deref() == Some("");
        Ok(CompareSample {
            row,
            has_style_signature: style_signature.is_some(),
            has_vibe_signature: vibe_signature
                .as_deref()
                .is_some_and(|value| !value.is_empty()),
            vibe_signature_unreadable,
        })
    }

    fn compare_sample_keys(&self, row_id: i64) -> Result<SampleKeys, DatabaseError> {
        self.connection
            .query_row(
                "SELECT artists, vibe_signature, style_signature FROM rows WHERE id = ?1",
                [row_id],
                |row| {
                    Ok(SampleKeys {
                        artists: row.get(0)?,
                        vibe_signature: row.get(1)?,
                        style_signature: row.get(2)?,
                    })
                },
            )
            .optional()?
            .ok_or(DatabaseError::RowNotFound(row_id))
    }

    /// 分区①：画师串整串精确相同（首尾空白归一，与“只看当前画师串”同口径）。
    pub fn query_compare_same_artists(
        &mut self,
        row_id: i64,
        offset: u64,
        limit: u32,
    ) -> Result<CompareSectionPage, DatabaseError> {
        let offset_i64 = validate_page(offset, limit)?;
        let keys = self.compare_sample_keys(row_id)?;
        let Some(artists) = keys
            .artists
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            // 样本没有画师串：如实空态，不隐藏分区。
            return Ok(empty_page(offset, limit));
        };
        let params: Vec<Value> = vec![Value::Integer(row_id), Value::Text(artists.to_owned())];
        let predicate = "rows.id != ?1
             AND NULLIF(TRIM(COALESCE(rows.artists, '')), '') = ?2";
        self.query_compare_section(predicate, &params, offset, limit, offset_i64)
    }

    /// 分区②：相同 vibe 组合 × 不同提示词（画风签名不同）。
    /// 样本无已知 vibe 组合时如实空态；目标行画风签名为 NULL 视为不同。
    pub fn query_compare_same_vibe_diff_style(
        &mut self,
        row_id: i64,
        offset: u64,
        limit: u32,
    ) -> Result<CompareSectionPage, DatabaseError> {
        let offset_i64 = validate_page(offset, limit)?;
        let keys = self.compare_sample_keys(row_id)?;
        let Some(vibe_signature) = keys
            .vibe_signature
            .as_deref()
            .filter(|value| !value.is_empty())
        else {
            return Ok(empty_page(offset, limit));
        };
        let params: Vec<Value> = vec![
            Value::Integer(row_id),
            Value::Text(vibe_signature.to_owned()),
            Value::Text(keys.style_signature.unwrap_or_default()),
        ];
        // 目标行签名为 NULL 时 COALESCE 归一为 ''；同为空（都无提示词）
        // 视为相同，其余与样本签名不等即“不同提示词”。
        let predicate = "rows.id != ?1
             AND rows.vibe_signature = ?2
             AND COALESCE(rows.style_signature, '') != ?3";
        self.query_compare_section(predicate, &params, offset, limit, offset_i64)
    }

    /// 分区③：相同提示词（画风签名）× 不同 vibe 组合，含“无 vibe”作为一种取值。
    /// 样本无画风签名时如实空态；目标行 vibe 空串标记（未知）一律排除。
    pub fn query_compare_same_style_diff_vibe(
        &mut self,
        row_id: i64,
        offset: u64,
        limit: u32,
    ) -> Result<CompareSectionPage, DatabaseError> {
        let offset_i64 = validate_page(offset, limit)?;
        let keys = self.compare_sample_keys(row_id)?;
        let Some(style_signature) = keys.style_signature.as_deref() else {
            return Ok(empty_page(offset, limit));
        };
        let params: Vec<Value> = vec![
            Value::Integer(row_id),
            Value::Text(style_signature.to_owned()),
            keys.vibe_signature
                .map(Value::Text)
                .unwrap_or(Value::Null),
        ];
        let predicate = "rows.id != ?1
             AND rows.style_signature = ?2
             AND rows.vibe_signature IS DISTINCT FROM ?3
             AND (rows.vibe_signature IS NULL OR rows.vibe_signature != '')";
        self.query_compare_section(predicate, &params, offset, limit, offset_i64)
    }

    /// 分区④：画风签名相同的全部行（前端按模型版本分组并剔除与样本
    /// 同档位的组）。上限 500 张，超出时标记截断。
    pub fn query_compare_same_style_all_models(
        &mut self,
        row_id: i64,
    ) -> Result<CompareModelSection, DatabaseError> {
        let keys = self.compare_sample_keys(row_id)?;
        let Some(style_signature) = keys.style_signature.as_deref() else {
            return Ok(CompareModelSection {
                rows: Vec::new(),
                total_count: 0,
                truncated: false,
            });
        };
        let params: Vec<Value> = vec![
            Value::Integer(row_id),
            Value::Text(style_signature.to_owned()),
        ];
        let predicate = "rows.id != ?1 AND rows.style_signature = ?2";
        let total = self.count_compare_rows(predicate, &params)?;
        let rows = self.fetch_compare_rows(predicate, &params, COMPARE_MODEL_SECTION_CAP, 0)?;
        Ok(CompareModelSection {
            rows,
            total_count: total,
            truncated: i64::try_from(total).unwrap_or(i64::MAX) > COMPARE_MODEL_SECTION_CAP,
        })
    }

    fn query_compare_section(
        &mut self,
        predicate: &str,
        params: &[Value],
        offset: u64,
        limit: u32,
        offset_i64: i64,
    ) -> Result<CompareSectionPage, DatabaseError> {
        let total_count = self.count_compare_rows(predicate, params)?;
        let rows = self.fetch_compare_rows(predicate, params, limit as i64, offset_i64)?;
        Ok(CompareSectionPage {
            rows,
            total_count,
            offset,
            limit,
        })
    }

    fn count_compare_rows(&self, predicate: &str, params: &[Value]) -> Result<u64, DatabaseError> {
        let count: i64 = self.connection.query_row(
            &format!("SELECT COUNT(*) FROM rows WHERE {predicate}"),
            params_from_iter(params.iter()),
            |row| row.get(0),
        )?;
        u64::try_from(count).map_err(|_| DatabaseError::CountOverflow)
    }

    /// 把谓词命中、时间倒序分页后的行物化到页面临时表，复用画廊的
    /// 行摘要与 Tag 附着逻辑。
    fn fetch_compare_rows(
        &mut self,
        predicate: &str,
        params: &[Value],
        limit: i64,
        offset: i64,
    ) -> Result<Vec<super::RowRecord>, DatabaseError> {
        let transaction = self.connection.transaction()?;
        transaction.execute_batch(&format!(
            "DROP TABLE IF EXISTS {PAGE_ROWS_TABLE};
             CREATE TEMP TABLE {PAGE_ROWS_TABLE} (
                 ordinal INTEGER PRIMARY KEY,
                 id INTEGER NOT NULL UNIQUE
             ) STRICT;"
        ))?;
        transaction.execute(
            &format!(
                "INSERT INTO {PAGE_ROWS_TABLE}(ordinal, id)
                 SELECT ROW_NUMBER() OVER (ORDER BY rows.id DESC), rows.id
                 FROM rows
                 WHERE {predicate}
                 ORDER BY rows.id DESC
                 LIMIT ?{} OFFSET ?{}",
                params.len() + 1,
                params.len() + 2,
            ),
            rusqlite::params_from_iter(
                params
                    .iter()
                    .cloned()
                    .chain([Value::Integer(limit), Value::Integer(offset)]),
            ),
        )?;
        let mut rows = query_page_metadata(&transaction)?;
        attach_page_tags(&transaction, &mut rows)?;
        transaction.execute_batch(&format!("DROP TABLE IF EXISTS {PAGE_ROWS_TABLE};"))?;
        transaction.commit()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{NewRow, test_support::append_rows};

    fn row(identity: &str, positive: Option<&str>, artists: Option<&str>) -> NewRow {
        NewRow {
            source_ordinal: 1,
            identity: identity.into(),
            positive_prompt: positive.map(str::to_owned),
            artists: artists.map(str::to_owned),
            ..NewRow::default()
        }
    }

    /// 直接写入 vibe/style 签名（绕过导入管线），构造分区匹配场景。
    fn set_signatures(database: &mut Database, row_id: i64, vibe: Option<&str>, style: Option<&str>) {
        database
            .connection
            .execute(
                "UPDATE rows SET vibe_signature = ?2, style_signature = ?3 WHERE id = ?1",
                rusqlite::params![row_id, vibe, style],
            )
            .unwrap();
    }

    /// 三行同画师串 + 三行同画风 + 一行无关：
    /// 行1 样本（artist:a / style S1 / vibe V1）。
    fn sample_database() -> Database {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(
            &mut database,
            &[
                // 1: 样本
                row("s", Some("cat, masterpiece"), Some("artist:a")),
                // 2: 同画师串、同画风、同 vibe → 各分区都排除（不是“不同”）
                row("same-all", Some("cat, no text"), Some("artist:a")),
                // 3: 同画师串、同画风、无 vibe → ③命中（无vibe是一种取值）
                row("same-novibe", Some("cat, very aesthetic"), Some(" artist:a ")),
                // 4: 同画师串、不同画风（NULL）→ ①②命中（NULL 视为不同提示词）
                row("same-nostyle", None, Some("artist:a")),
                // 5: 同画风、不同 vibe → ③命中
                row("style-v2", Some("cat, best quality"), Some("artist:b")),
                // 6: 同画风、同 vibe → ②排除
                row("style-v1", Some("CAT, amazing quality"), None),
                // 7: 无关行
                row("other", Some("dog"), Some("artist:c")),
            ],
        );
        // append_batch 已按提示词算好 style 签名（质量词剥离、大小写归一），
        // 行4 无提示词签名为 NULL；这里补齐 vibe 签名并覆盖部分画风。
        set_signatures(&mut database, 1, Some("V1"), Some("S1"));
        set_signatures(&mut database, 2, Some("V1"), Some("S1"));
        set_signatures(&mut database, 3, None, Some("S1"));
        set_signatures(&mut database, 4, Some("V1"), None);
        set_signatures(&mut database, 5, Some("V2"), Some("S1"));
        set_signatures(&mut database, 6, Some("V1"), Some("S1"));
        set_signatures(&mut database, 7, Some("V3"), None);
        database
    }

    #[test]
    fn sample_reports_signature_status_and_missing_row_errors() {
        let mut database = sample_database();
        let sample = database.get_compare_sample(1).unwrap();
        assert_eq!(sample.row.id, 1);
        assert!(sample.has_style_signature);
        assert!(sample.has_vibe_signature);
        assert!(!sample.vibe_signature_unreadable);

        // 空串 vibe 标记 → 已扫描不可读。
        set_signatures(&mut database, 1, Some(""), Some("S1"));
        let sample = database.get_compare_sample(1).unwrap();
        assert!(!sample.has_vibe_signature);
        assert!(sample.vibe_signature_unreadable);

        assert!(matches!(
            database.get_compare_sample(999),
            Err(DatabaseError::RowNotFound(999))
        ));
    }

    #[test]
    fn section_same_artists_matches_trimmed_exact_string_only() {
        let mut database = sample_database();
        let page = database.query_compare_same_artists(1, 0, 10).unwrap();
        // 行3（首尾空白归一后相同）与行4 命中；行5/6/7 画师串不同。
        assert_eq!(page.rows.iter().map(|row| row.id).collect::<Vec<_>>(), vec![4, 3, 2]);
        assert_eq!(page.total_count, 3);
        // 时间倒序。
        assert!(page.rows.windows(2).all(|pair| pair[0].id > pair[1].id));
    }

    #[test]
    fn section_same_artists_empty_without_sample_artists() {
        let mut database = sample_database();
        database
            .connection
            .execute("UPDATE rows SET artists = '   ' WHERE id = 1", [])
            .unwrap();
        let page = database.query_compare_same_artists(1, 0, 10).unwrap();
        assert_eq!(page.total_count, 0);
        assert!(page.rows.is_empty());
    }

    #[test]
    fn section_same_vibe_diff_style_excludes_same_and_null_marked_targets() {
        let mut database = sample_database();
        // 行7 的 vibe 改为空串标记（未知）：即使画风不同也不得进入②。
        set_signatures(&mut database, 7, Some(""), Some("S9"));
        let page = database.query_compare_same_vibe_diff_style(1, 0, 10).unwrap();
        // 同 vibe V1：行2（同画风，排除）、行6（同画风，排除）、行4（画风 NULL 视为不同，命中）。
        assert_eq!(page.rows.iter().map(|row| row.id).collect::<Vec<_>>(), vec![4]);
        assert_eq!(page.total_count, 1);
    }

    #[test]
    fn section_same_vibe_empty_without_sample_vibe() {
        let mut database = sample_database();
        set_signatures(&mut database, 1, None, Some("S1"));
        let page = database.query_compare_same_vibe_diff_style(1, 0, 10).unwrap();
        assert_eq!(page.total_count, 0);
        // 空串标记（不可读）同样视为无已知 vibe。
        set_signatures(&mut database, 1, Some(""), Some("S1"));
        let page = database.query_compare_same_vibe_diff_style(1, 0, 10).unwrap();
        assert_eq!(page.total_count, 0);
    }

    #[test]
    fn section_same_style_diff_vibe_treats_no_vibe_as_a_value() {
        let mut database = sample_database();
        let page = database.query_compare_same_style_diff_vibe(1, 0, 10).unwrap();
        // 同画风 S1：行2（同 vibe 排除）、行3（无 vibe 命中）、行5（不同 vibe 命中）、
        // 行6（同 vibe 排除）；行4/7 画风签名不同或 NULL 不参与。
        assert_eq!(
            page.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
            vec![5, 3]
        );
        assert_eq!(page.total_count, 2);
    }

    #[test]
    fn section_same_style_excludes_unknown_vibe_targets() {
        let mut database = sample_database();
        set_signatures(&mut database, 5, Some(""), Some("S1"));
        let page = database.query_compare_same_style_diff_vibe(1, 0, 10).unwrap();
        // 行5 的 vibe 变为空串标记（未知）→ 从“不同”判定中排除。
        assert_eq!(
            page.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
            vec![3]
        );
    }

    #[test]
    fn section_same_style_empty_without_sample_style() {
        let mut database = sample_database();
        set_signatures(&mut database, 1, Some("V1"), None);
        let page = database.query_compare_same_style_diff_vibe(1, 0, 10).unwrap();
        assert_eq!(page.total_count, 0);
    }

    #[test]
    fn model_section_returns_all_same_style_rows_with_truncation_flag() {
        let mut database = sample_database();
        let section = database.query_compare_same_style_all_models(1).unwrap();
        // 同画风全部行（含同 vibe 的行4 无关；②③的“不同”判定不影响④），
        // 但行4 画风为 NULL 不参与：2、3、5、6。
        assert_eq!(
            section.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
            vec![6, 5, 3, 2]
        );
        assert_eq!(section.total_count, 4);
        assert!(!section.truncated);

        // 追加 600 行同画风 → 截断标记 + 上限行数。
        let more: Vec<NewRow> = (0..600)
            .map(|index| row(&format!("extra{index}"), Some("cat"), None))
            .collect();
        append_rows(&mut database, &more);
        // append_batch 为新行现算的签名是真实哈希，与手工写入的 S1 不同；
        // 统一改成 S1 以构造超限场景。
        database
            .connection
            .execute("UPDATE rows SET style_signature = 'S1' WHERE id > 7", [])
            .unwrap();
        let section = database.query_compare_same_style_all_models(1).unwrap();
        assert_eq!(section.rows.len(), COMPARE_MODEL_SECTION_CAP as usize);
        assert_eq!(section.total_count, 604);
        assert!(section.truncated);
    }

    #[test]
    fn pagination_slices_and_reports_total() {
        let mut database = sample_database();
        let page = database.query_compare_same_artists(1, 1, 2).unwrap();
        // 时间倒序 [4, 3, 2]，offset=1 limit=2 → [3, 2]。
        assert_eq!(page.rows.iter().map(|row| row.id).collect::<Vec<_>>(), vec![3, 2]);
        assert_eq!(page.total_count, 3);
        assert_eq!(page.offset, 1);
        assert_eq!(page.limit, 2);

        assert!(matches!(
            database.query_compare_same_artists(1, 0, 0),
            Err(DatabaseError::InvalidPageSize { requested: 0, .. })
        ));
    }
}
