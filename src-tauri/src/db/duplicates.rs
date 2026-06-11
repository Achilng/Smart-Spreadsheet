use rusqlite::params;

use super::{Database, DatabaseError};

/// 查重分组依据。比较裁剪首尾空白后的完整文本，区分大小写，空值不参与。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicateKey {
    PositivePrompt,
    Artists,
}

impl DuplicateKey {
    fn column(self) -> &'static str {
        match self {
            Self::PositivePrompt => "positive_prompt",
            Self::Artists => "artists",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateRow {
    pub id: i64,
    pub batch_id: i64,
    pub source_ordinal: u32,
    pub time: Option<String>,
    pub image_path: Option<String>,
    pub stored_image_path: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateGroup {
    /// 组内共享的提示词/画师串文本。
    pub key: String,
    pub rows: Vec<DuplicateRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateReport {
    /// 全库重复组总数（可能大于返回的组数）。
    pub total_groups: u64,
    /// 全库“多余”行总数（每组行数 - 1 之和）。
    pub total_redundant_rows: u64,
    /// 按首行入库顺序返回的前 N 组。
    pub groups: Vec<DuplicateGroup>,
}

const DUP_KEYS_TABLE: &str = "temp.duplicate_keys";

impl Database {
    /// 按正向提示词或画师串精确分组查找重复行。
    /// 返回的组按组内最早入库行排序，最多 `group_limit` 组；行内附带 Tag 便于取舍。
    pub fn find_duplicates(
        &mut self,
        key: DuplicateKey,
        group_limit: u32,
    ) -> Result<DuplicateReport, DatabaseError> {
        let column = key.column();
        let transaction = self.connection.transaction()?;
        transaction.execute_batch(&format!(
            "DROP TABLE IF EXISTS {DUP_KEYS_TABLE};
             CREATE TEMP TABLE {DUP_KEYS_TABLE} AS
             SELECT TRIM({column}) AS key, COUNT(*) AS row_count, MIN(id) AS first_id
             FROM rows
             WHERE TRIM(COALESCE({column}, '')) <> ''
             GROUP BY TRIM({column})
             HAVING COUNT(*) > 1;
             CREATE INDEX temp.idx_duplicate_keys ON duplicate_keys(key);"
        ))?;

        let (total_groups, total_redundant_rows): (i64, i64) = transaction.query_row(
            &format!(
                "SELECT COUNT(*), COALESCE(SUM(row_count - 1), 0) FROM {DUP_KEYS_TABLE}"
            ),
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let mut groups: Vec<DuplicateGroup> = Vec::new();
        {
            let mut statement = transaction.prepare(&format!(
                "SELECT keys.key, rows.id, rows.batch_id, rows.source_ordinal,
                        rows.time, rows.image_path, rows.stored_image_path
                 FROM (SELECT key, first_id FROM {DUP_KEYS_TABLE}
                       ORDER BY first_id LIMIT ?1) AS keys
                 JOIN rows ON TRIM(rows.{column}) = keys.key
                 ORDER BY keys.first_id, rows.id"
            ))?;
            let mut matched = statement.query(params![group_limit])?;
            while let Some(row) = matched.next()? {
                let group_key: String = row.get(0)?;
                let summary = DuplicateRow {
                    id: row.get(1)?,
                    batch_id: row.get(2)?,
                    source_ordinal: row.get(3)?,
                    time: row.get(4)?,
                    image_path: row.get(5)?,
                    stored_image_path: row.get(6)?,
                    tags: Vec::new(),
                };
                if groups.last().is_none_or(|last| last.key != group_key) {
                    groups.push(DuplicateGroup {
                        key: group_key,
                        rows: Vec::new(),
                    });
                }
                groups
                    .last_mut()
                    .expect("group was inserted for current row")
                    .rows
                    .push(summary);
            }
        }

        // 为返回的行附加 Tag（一次查询，按行 ID 回填）。
        {
            let mut index_by_id = std::collections::HashMap::new();
            for (group_index, group) in groups.iter().enumerate() {
                for (row_index, row) in group.rows.iter().enumerate() {
                    index_by_id.insert(row.id, (group_index, row_index));
                }
            }
            let mut statement = transaction.prepare(&format!(
                "SELECT row_tags.row_id, tags.name
                 FROM row_tags
                 JOIN tags ON tags.id = row_tags.tag_id
                 WHERE row_tags.row_id IN (
                     SELECT rows.id
                     FROM (SELECT key, first_id FROM {DUP_KEYS_TABLE}
                           ORDER BY first_id LIMIT ?1) AS keys
                     JOIN rows ON TRIM(rows.{column}) = keys.key
                 )
                 ORDER BY row_tags.row_id, tags.name COLLATE BINARY"
            ))?;
            let mut tag_rows = statement.query(params![group_limit])?;
            while let Some(tag_row) = tag_rows.next()? {
                let row_id: i64 = tag_row.get(0)?;
                let tag: String = tag_row.get(1)?;
                if let Some(&(group_index, row_index)) = index_by_id.get(&row_id) {
                    groups[group_index].rows[row_index].tags.push(tag);
                }
            }
        }

        transaction.execute_batch(&format!("DROP TABLE {DUP_KEYS_TABLE};"))?;
        transaction.commit()?;

        Ok(DuplicateReport {
            total_groups: u64::try_from(total_groups)
                .map_err(|_| DatabaseError::CountOverflow)?,
            total_redundant_rows: u64::try_from(total_redundant_rows)
                .map_err(|_| DatabaseError::CountOverflow)?,
            groups,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::NewRow;
    use super::super::test_support::append_rows;
    use super::*;

    fn row(ordinal: u32, prompt: Option<&str>, artists: Option<&str>) -> NewRow {
        NewRow {
            source_ordinal: ordinal,
            identity: format!("file:test\\{ordinal}.png"),
            positive_prompt: prompt.map(str::to_owned),
            artists: artists.map(str::to_owned),
            ..NewRow::default()
        }
    }

    fn duplicate_database() -> Database {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(
            &mut database,
            &[
                row(1, Some("masterpiece, artist:a"), Some("artist:a")),
                row(2, Some("  masterpiece, artist:a  "), Some("artist:b")),
                row(3, Some("unique prompt"), Some("artist:a")),
                row(4, Some("Masterpiece, artist:a"), None),
                row(5, None, Some("artist:a")),
                row(6, Some("second group"), None),
                row(7, Some("second group"), None),
            ],
        );
        database
    }

    #[test]
    fn groups_by_trimmed_case_sensitive_positive_prompt() {
        let mut database = duplicate_database();

        let report = database
            .find_duplicates(DuplicateKey::PositivePrompt, 50)
            .unwrap();

        assert_eq!(report.total_groups, 2);
        assert_eq!(report.total_redundant_rows, 2);
        assert_eq!(report.groups.len(), 2);
        // 第一组：行 1 与行 2（裁剪空白后相同）；行 4 大小写不同不算重复。
        assert_eq!(report.groups[0].key, "masterpiece, artist:a");
        assert_eq!(
            report.groups[0]
                .rows
                .iter()
                .map(|row| row.id)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(report.groups[1].key, "second group");
        assert_eq!(
            report.groups[1]
                .rows
                .iter()
                .map(|row| row.id)
                .collect::<Vec<_>>(),
            vec![6, 7]
        );
    }

    #[test]
    fn groups_by_artists_and_attaches_tags() {
        let mut database = duplicate_database();
        database
            .add_tags_to_rows(&[1, 3], &["Keep".into()])
            .unwrap();

        let report = database.find_duplicates(DuplicateKey::Artists, 50).unwrap();

        assert_eq!(report.total_groups, 1);
        assert_eq!(report.groups[0].key, "artist:a");
        let ids: Vec<i64> = report.groups[0].rows.iter().map(|row| row.id).collect();
        assert_eq!(ids, vec![1, 3, 5]);
        assert_eq!(report.groups[0].rows[0].tags, vec!["Keep"]);
        assert_eq!(report.groups[0].rows[1].tags, vec!["Keep"]);
        assert!(report.groups[0].rows[2].tags.is_empty());
    }

    #[test]
    fn respects_group_limit_while_reporting_full_totals() {
        let mut database = duplicate_database();

        let report = database
            .find_duplicates(DuplicateKey::PositivePrompt, 1)
            .unwrap();

        assert_eq!(report.total_groups, 2);
        assert_eq!(report.groups.len(), 1);
        assert_eq!(report.groups[0].key, "masterpiece, artist:a");
    }

    #[test]
    fn reports_empty_when_no_duplicates() {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(
            &mut database,
            &[row(1, Some("one"), None), row(2, Some("two"), None)],
        );

        let report = database
            .find_duplicates(DuplicateKey::PositivePrompt, 50)
            .unwrap();

        assert_eq!(report.total_groups, 0);
        assert_eq!(report.total_redundant_rows, 0);
        assert!(report.groups.is_empty());
    }
}
