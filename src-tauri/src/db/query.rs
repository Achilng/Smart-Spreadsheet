use std::collections::HashMap;

use rusqlite::{Transaction, params};
use serde::{Deserialize, Serialize};

use super::tags::normalize_tags;
use super::{Database, DatabaseError};

pub const MAX_PAGE_SIZE: u32 = 500;
pub(super) const FILTER_TAGS_TABLE: &str = "temp.query_filter_tags";
const FILTERED_ROWS_TABLE: &str = "temp.query_filtered_rows";
const PAGE_ROWS_TABLE: &str = "temp.query_page_rows";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TagMatchMode {
    And,
    Or,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DedupeMode {
    #[default]
    None,
    PositivePrompt,
    Artists,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowQuery {
    pub offset: u64,
    pub limit: u32,
    pub tags: Vec<String>,
    pub tag_mode: TagMatchMode,
    pub dedupe: DedupeMode,
    #[serde(default)]
    pub single_artist_only: bool,
    #[serde(default)]
    pub group_view: bool,
    #[serde(default)]
    pub hide_grouped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowRecord {
    pub id: i64,
    pub batch_id: i64,
    pub source_ordinal: u32,
    pub time: Option<String>,
    pub positive_prompt: Option<String>,
    pub negative_prompt: Option<String>,
    pub artists: Option<String>,
    pub image_folder: Option<String>,
    pub image_path: Option<String>,
    pub stored_image_path: Option<String>,
    pub metadata_failed: bool,
    pub group_id: Option<i64>,
    pub group_name: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowPage {
    pub rows: Vec<RowRecord>,
    pub total_count: u64,
    pub offset: u64,
    pub limit: u32,
}

impl RowPage {
    pub fn has_more(&self) -> bool {
        self.offset.saturating_add(self.rows.len() as u64) < self.total_count
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagSummary {
    pub name: String,
    pub row_count: u64,
}

impl Database {
    pub fn query_rows(&mut self, query: &RowQuery) -> Result<RowPage, DatabaseError> {
        if query.limit == 0 || query.limit > MAX_PAGE_SIZE {
            return Err(DatabaseError::InvalidPageSize {
                requested: query.limit,
                maximum: MAX_PAGE_SIZE,
            });
        }
        let offset = i64::try_from(query.offset).map_err(|_| DatabaseError::OffsetOverflow)?;
        let tags = normalize_tags(&query.tags);
        let transaction = self.connection.transaction()?;
        create_filter_tags(&transaction, &tags)?;
        create_filtered_rows_table(&transaction)?;
        populate_filtered_rows(
            &transaction,
            FILTERED_ROWS_TABLE,
            query.tag_mode,
            query.dedupe,
            query.single_artist_only,
            query.group_view,
            query.hide_grouped,
        )?;
        create_page_rows_table(&transaction)?;
        create_page_rows(&transaction, query.limit, offset)?;

        let total_count = query_total_count(&transaction)?;
        let mut rows = query_page_metadata(&transaction)?;
        attach_page_tags(&transaction, &mut rows)?;

        transaction.execute_batch(&format!(
            "DROP TABLE {PAGE_ROWS_TABLE};
             DROP TABLE {FILTERED_ROWS_TABLE};
             DROP TABLE {FILTER_TAGS_TABLE};"
        ))?;
        transaction.commit()?;

        Ok(RowPage {
            rows,
            total_count,
            offset: query.offset,
            limit: query.limit,
        })
    }

    pub fn get_rows_by_ids(&mut self, ids: &[i64]) -> Result<Vec<RowRecord>, DatabaseError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let transaction = self.connection.transaction()?;
        transaction.execute_batch(&format!(
            "DROP TABLE IF EXISTS {PAGE_ROWS_TABLE};
             CREATE TEMP TABLE {PAGE_ROWS_TABLE} (
                 ordinal INTEGER PRIMARY KEY,
                 id INTEGER NOT NULL UNIQUE
             ) STRICT;"
        ))?;
        let mut insert =
            transaction.prepare(&format!("INSERT OR IGNORE INTO {PAGE_ROWS_TABLE}(ordinal, id) VALUES (?1, ?2)"))?;
        for (index, id) in ids.iter().enumerate() {
            insert.execute(rusqlite::params![index as i64, *id])?;
        }
        drop(insert);
        let mut rows = query_page_metadata(&transaction)?;
        attach_page_tags(&transaction, &mut rows)?;
        transaction.execute_batch(&format!("DROP TABLE {PAGE_ROWS_TABLE};"))?;
        transaction.commit()?;
        Ok(rows)
    }

    pub fn get_group_members(
        &mut self,
        group_id: i64,
        offset: u64,
        limit: u32,
    ) -> Result<RowPage, DatabaseError> {
        if limit == 0 || limit > MAX_PAGE_SIZE {
            return Err(DatabaseError::InvalidPageSize {
                requested: limit,
                maximum: MAX_PAGE_SIZE,
            });
        }
        let offset_i64 = i64::try_from(offset).map_err(|_| DatabaseError::OffsetOverflow)?;
        let transaction = self.connection.transaction()?;
        create_filtered_rows_table(&transaction)?;
        transaction.execute(
            &format!(
                "INSERT INTO {FILTERED_ROWS_TABLE}(id)
                 SELECT id FROM rows WHERE group_id = ?1"
            ),
            [group_id],
        )?;
        create_page_rows_table(&transaction)?;
        create_page_rows(&transaction, limit, offset_i64)?;

        let total_count = query_total_count(&transaction)?;
        let mut rows = query_page_metadata(&transaction)?;
        attach_page_tags(&transaction, &mut rows)?;

        transaction.execute_batch(&format!(
            "DROP TABLE {PAGE_ROWS_TABLE};
             DROP TABLE {FILTERED_ROWS_TABLE};"
        ))?;
        transaction.commit()?;

        Ok(RowPage {
            rows,
            total_count,
            offset,
            limit,
        })
    }

    pub fn list_tags(&self) -> Result<Vec<TagSummary>, DatabaseError> {
        let mut statement = self.connection.prepare(
            "SELECT tags.name, COUNT(row_tags.row_id)
             FROM tags
             LEFT JOIN row_tags ON row_tags.tag_id = tags.id
             GROUP BY tags.id, tags.name
             ORDER BY tags.name COLLATE BINARY",
        )?;
        let stored = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let summaries = stored
            .into_iter()
            .map(|(name, row_count)| {
                Ok(TagSummary {
                    name,
                    row_count: u64::try_from(row_count)
                        .map_err(|_| DatabaseError::CountOverflow)?,
                })
            })
            .collect::<Result<Vec<_>, DatabaseError>>()?;
        Ok(summaries)
    }
}

pub(super) fn create_filter_tags(
    transaction: &Transaction<'_>,
    tags: &[String],
) -> Result<(), rusqlite::Error> {
    transaction.execute_batch(&format!(
        "DROP TABLE IF EXISTS {FILTER_TAGS_TABLE};
         CREATE TEMP TABLE {FILTER_TAGS_TABLE} (
             name TEXT PRIMARY KEY COLLATE BINARY
         ) STRICT, WITHOUT ROWID;"
    ))?;
    let mut insert = transaction.prepare(&format!(
        "INSERT INTO {FILTER_TAGS_TABLE}(name) VALUES (?1)"
    ))?;
    for tag in tags {
        insert.execute([tag])?;
    }
    Ok(())
}

fn create_page_rows_table(transaction: &Transaction<'_>) -> Result<(), rusqlite::Error> {
    transaction.execute_batch(&format!(
        "DROP TABLE IF EXISTS {PAGE_ROWS_TABLE};
         CREATE TEMP TABLE {PAGE_ROWS_TABLE} (
             ordinal INTEGER PRIMARY KEY,
             id INTEGER NOT NULL UNIQUE
         ) STRICT;"
    ))
}

fn create_filtered_rows_table(transaction: &Transaction<'_>) -> Result<(), rusqlite::Error> {
    transaction.execute_batch(&format!(
        "DROP TABLE IF EXISTS {FILTERED_ROWS_TABLE};
         CREATE TEMP TABLE {FILTERED_ROWS_TABLE} (
             id INTEGER PRIMARY KEY
         ) STRICT, WITHOUT ROWID;"
    ))
}

pub(super) fn populate_filtered_rows(
    transaction: &Transaction<'_>,
    target_table: &str,
    mode: TagMatchMode,
    dedupe: DedupeMode,
    single_artist_only: bool,
    group_view: bool,
    hide_grouped: bool,
) -> Result<(), rusqlite::Error> {
    let tag_predicate = filter_predicate(mode);
    let mut predicate = tag_predicate.to_owned();
    if single_artist_only {
        predicate = format!(
            "({predicate})
             AND rows.artists IS NOT NULL
             AND TRIM(rows.artists) != ''
             AND INSTR(rows.artists, CHAR(10)) = 0"
        );
    }
    if !group_view && hide_grouped {
        predicate = format!("({predicate}) AND rows.group_id IS NULL");
    }

    if group_view {
        transaction.execute(
            &format!(
                "INSERT INTO {target_table}(id)
                 SELECT MIN(rows.id) FROM rows
                 WHERE ({predicate}) AND rows.group_id IS NOT NULL
                 GROUP BY rows.group_id
                 UNION ALL
                 SELECT rows.id FROM rows
                 WHERE ({predicate}) AND rows.group_id IS NULL"
            ),
            [],
        )?;
    } else {
        match dedupe {
            DedupeMode::None => transaction.execute(
                &format!(
                    "INSERT INTO {target_table}(id)
                     SELECT rows.id FROM rows WHERE {predicate}"
                ),
                [],
            )?,
            DedupeMode::PositivePrompt | DedupeMode::Artists => {
                let column = match dedupe {
                    DedupeMode::PositivePrompt => "positive_prompt",
                    DedupeMode::Artists => "artists",
                    DedupeMode::None => unreachable!(),
                };
                transaction.execute(
                    &format!(
                        "INSERT INTO {target_table}(id)
                         WITH filtered_rows AS (
                             SELECT rows.id,
                                    NULLIF(TRIM(COALESCE(rows.{column}, '')), '') AS dedupe_key
                             FROM rows
                             WHERE {predicate}
                         )
                         SELECT id FROM filtered_rows WHERE dedupe_key IS NULL
                         UNION ALL
                         SELECT MIN(id)
                         FROM filtered_rows
                         WHERE dedupe_key IS NOT NULL
                         GROUP BY dedupe_key"
                    ),
                    [],
                )?
            }
        };
    }
    Ok(())
}

fn create_page_rows(
    transaction: &Transaction<'_>,
    limit: u32,
    offset: i64,
) -> Result<(), rusqlite::Error> {
    // 行的展示顺序即入库顺序（rows.id 单调递增）。
    transaction.execute(
        &format!(
            "INSERT INTO {PAGE_ROWS_TABLE}(ordinal, id)
             SELECT ROW_NUMBER() OVER (ORDER BY filtered.id), filtered.id
             FROM {FILTERED_ROWS_TABLE} AS filtered
             ORDER BY filtered.id
             LIMIT ?1 OFFSET ?2"
        ),
        params![limit, offset],
    )?;
    Ok(())
}

fn query_total_count(transaction: &Transaction<'_>) -> Result<u64, DatabaseError> {
    let count: i64 = transaction.query_row(
        &format!("SELECT COUNT(*) FROM {FILTERED_ROWS_TABLE}"),
        [],
        |row| row.get(0),
    )?;
    u64::try_from(count).map_err(|_| DatabaseError::CountOverflow)
}

fn query_page_metadata(transaction: &Transaction<'_>) -> Result<Vec<RowRecord>, DatabaseError> {
    let mut statement = transaction.prepare(&format!(
        "SELECT rows.id, rows.batch_id, rows.source_ordinal, rows.time,
                rows.positive_prompt, rows.negative_prompt, rows.artists,
                rows.image_folder, rows.image_path, rows.stored_image_path,
                rows.metadata_failed, rows.group_id, groups.name
         FROM {PAGE_ROWS_TABLE} AS page
         JOIN rows ON rows.id = page.id
         LEFT JOIN groups ON groups.id = rows.group_id
         ORDER BY page.ordinal"
    ))?;
    let rows = statement
        .query_map([], |row| {
            Ok(RowRecord {
                id: row.get(0)?,
                batch_id: row.get(1)?,
                source_ordinal: row.get(2)?,
                time: row.get(3)?,
                positive_prompt: row.get(4)?,
                negative_prompt: row.get(5)?,
                artists: row.get(6)?,
                image_folder: row.get(7)?,
                image_path: row.get(8)?,
                stored_image_path: row.get(9)?,
                metadata_failed: row.get(10)?,
                group_id: row.get(11)?,
                group_name: row.get(12)?,
                tags: Vec::new(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn attach_page_tags(
    transaction: &Transaction<'_>,
    rows: &mut [RowRecord],
) -> Result<(), DatabaseError> {
    let index_by_id: HashMap<i64, usize> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.id, index))
        .collect();
    let mut statement = transaction.prepare(&format!(
        "SELECT page.id, tags.name
         FROM {PAGE_ROWS_TABLE} AS page
         JOIN row_tags ON row_tags.row_id = page.id
         JOIN tags ON tags.id = row_tags.tag_id
         ORDER BY page.ordinal, tags.name COLLATE BINARY"
    ))?;
    let mut tag_rows = statement.query([])?;
    while let Some(tag_row) = tag_rows.next()? {
        let row_id: i64 = tag_row.get(0)?;
        let tag: String = tag_row.get(1)?;
        if let Some(index) = index_by_id.get(&row_id) {
            rows[*index].tags.push(tag);
        }
    }
    Ok(())
}

pub(super) fn filter_predicate(mode: TagMatchMode) -> &'static str {
    match mode {
        TagMatchMode::And => {
            "(SELECT COUNT(*) FROM query_filter_tags) = 0
             OR (SELECT COUNT(*)
                 FROM row_tags
                 JOIN tags ON tags.id = row_tags.tag_id
                 JOIN query_filter_tags ON query_filter_tags.name = tags.name COLLATE BINARY
                 WHERE row_tags.row_id = rows.id)
                = (SELECT COUNT(*) FROM query_filter_tags)"
        }
        TagMatchMode::Or => {
            "(SELECT COUNT(*) FROM query_filter_tags) = 0
             OR EXISTS (
                 SELECT 1
                 FROM row_tags
                 JOIN tags ON tags.id = row_tags.tag_id
                 JOIN query_filter_tags ON query_filter_tags.name = tags.name COLLATE BINARY
                 WHERE row_tags.row_id = rows.id
             )"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::database_with_rows;
    use super::*;

    #[test]
    fn paginates_rows_stably_and_attaches_sorted_tags() {
        let mut database = tagged_database();

        let page = database
            .query_rows(&RowQuery {
                offset: 1,
                limit: 2,
                tags: Vec::new(),
                tag_mode: TagMatchMode::And,
                dedupe: DedupeMode::None,
                single_artist_only: false,
                group_view: false,
                hide_grouped: false,
            })
            .unwrap();

        assert_eq!(page.total_count, 5);
        assert_eq!(
            page.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert_eq!(page.rows[0].tags, vec!["Common", "red"]);
        assert_eq!(page.rows[1].tags, vec!["Blue", "Red"]);
        assert!(page.has_more());
    }

    #[test]
    fn and_filter_requires_every_case_sensitive_tag() {
        let mut database = tagged_database();

        let page = query(&mut database, &["Red", "Blue"], TagMatchMode::And);
        let wrong_case = query(&mut database, &["red", "Blue"], TagMatchMode::And);

        assert_eq!(page.total_count, 1);
        assert_eq!(page.rows[0].id, 3);
        assert_eq!(wrong_case.total_count, 0);
    }

    #[test]
    fn or_filter_matches_any_case_sensitive_tag() {
        let mut database = tagged_database();

        let page = query(&mut database, &["Red", "Blue"], TagMatchMode::Or);
        let lowercase = query(&mut database, &["red"], TagMatchMode::Or);

        assert_eq!(page.total_count, 3);
        assert_eq!(
            page.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
            vec![1, 3, 4]
        );
        assert_eq!(
            lowercase.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
            vec![2]
        );
    }

    #[test]
    fn dedupes_nonempty_keys_and_keeps_empty_rows_visible() {
        let mut database = tagged_database();
        database
            .connection
            .execute_batch(
                "UPDATE rows SET positive_prompt = CASE id
                     WHEN 1 THEN ' same '
                     WHEN 2 THEN 'same'
                     WHEN 3 THEN ''
                     WHEN 4 THEN NULL
                     ELSE 'Same'
                 END;
                 UPDATE rows SET artists = CASE id
                     WHEN 1 THEN 'artist A'
                     WHEN 2 THEN 'artist A'
                     WHEN 3 THEN ' artist B '
                     WHEN 4 THEN 'artist B'
                     ELSE ''
                 END;",
            )
            .unwrap();

        let prompts = database
            .query_rows(&RowQuery {
                offset: 0,
                limit: 100,
                tags: Vec::new(),
                tag_mode: TagMatchMode::And,
                dedupe: DedupeMode::PositivePrompt,
                single_artist_only: false,
                group_view: false,
                hide_grouped: false,
            })
            .unwrap();
        assert_eq!(prompts.total_count, 4);
        assert_eq!(
            prompts.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
            vec![1, 3, 4, 5]
        );

        let artists = database
            .query_rows(&RowQuery {
                offset: 0,
                limit: 100,
                tags: Vec::new(),
                tag_mode: TagMatchMode::And,
                dedupe: DedupeMode::Artists,
                single_artist_only: false,
                group_view: false,
                hide_grouped: false,
            })
            .unwrap();
        assert_eq!(artists.total_count, 3);
        assert_eq!(
            artists.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
            vec![1, 3, 5]
        );
    }

    #[test]
    fn applies_tag_filter_before_deduplication() {
        let mut database = tagged_database();
        database
            .connection
            .execute(
                "UPDATE rows SET positive_prompt = 'shared' WHERE id IN (1, 2, 3)",
                [],
            )
            .unwrap();

        let page = database
            .query_rows(&RowQuery {
                offset: 0,
                limit: 100,
                tags: vec!["Red".into()],
                tag_mode: TagMatchMode::And,
                dedupe: DedupeMode::PositivePrompt,
                single_artist_only: false,
                group_view: false,
                hide_grouped: false,
            })
            .unwrap();

        assert_eq!(page.total_count, 1);
        assert_eq!(page.rows[0].id, 1);
    }

    #[test]
    fn lists_all_tag_counts_in_binary_order() {
        let mut database = tagged_database();
        database.create_tag("Unused").unwrap();

        let tags = database.list_tags().unwrap();

        assert_eq!(
            tags,
            vec![
                TagSummary {
                    name: "Blue".into(),
                    row_count: 2
                },
                TagSummary {
                    name: "Common".into(),
                    row_count: 2
                },
                TagSummary {
                    name: "Red".into(),
                    row_count: 2
                },
                TagSummary {
                    name: "Unused".into(),
                    row_count: 0
                },
                TagSummary {
                    name: "red".into(),
                    row_count: 1
                },
            ]
        );
    }

    #[test]
    fn rejects_invalid_page_sizes() {
        let mut database = tagged_database();
        for limit in [0, MAX_PAGE_SIZE + 1] {
            let error = database
                .query_rows(&RowQuery {
                    offset: 0,
                    limit,
                    tags: Vec::new(),
                    tag_mode: TagMatchMode::And,
                    dedupe: DedupeMode::None,
                    single_artist_only: false,
                    group_view: false,
                    hide_grouped: false,
                })
                .unwrap_err();
            assert!(matches!(error, DatabaseError::InvalidPageSize { .. }));
        }
    }

    #[test]
    fn paginates_ten_thousand_rows_without_loading_all_records() {
        let mut database = database_with_rows(10_000);

        let page = database
            .query_rows(&RowQuery {
                offset: 9_900,
                limit: 100,
                tags: Vec::new(),
                tag_mode: TagMatchMode::Or,
                dedupe: DedupeMode::None,
                single_artist_only: false,
                group_view: false,
                hide_grouped: false,
            })
            .unwrap();

        assert_eq!(page.total_count, 10_000);
        assert_eq!(page.rows.len(), 100);
        assert_eq!(page.rows.first().unwrap().source_ordinal, 9_902);
        assert_eq!(page.rows.last().unwrap().source_ordinal, 10_001);
        assert!(!page.has_more());
    }

    fn query(database: &mut Database, tags: &[&str], mode: TagMatchMode) -> RowPage {
        database
            .query_rows(&RowQuery {
                offset: 0,
                limit: 100,
                tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
                tag_mode: mode,
                dedupe: DedupeMode::None,
                single_artist_only: false,
                group_view: false,
                hide_grouped: false,
            })
            .unwrap()
    }

    fn tagged_database() -> Database {
        let mut database = database_with_rows(5);
        database
            .add_tags_to_rows(&[1, 2], &["Common".into()])
            .unwrap();
        database.add_tags_to_rows(&[1, 3], &["Red".into()]).unwrap();
        database.add_tags_to_rows(&[2], &["red".into()]).unwrap();
        database
            .add_tags_to_rows(&[3, 4], &["Blue".into()])
            .unwrap();
        database
    }
}
