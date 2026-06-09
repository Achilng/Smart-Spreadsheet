use std::collections::HashMap;

use rusqlite::{Transaction, params};

use super::tags::normalize_tags;
use super::{Database, DatabaseError};

pub const MAX_PAGE_SIZE: u32 = 500;
const FILTER_TAGS_TABLE: &str = "temp.query_filter_tags";
const PAGE_ROWS_TABLE: &str = "temp.query_page_rows";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagMatchMode {
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowQuery {
    pub offset: u64,
    pub limit: u32,
    pub tags: Vec<String>,
    pub tag_mode: TagMatchMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowRecord {
    pub id: i64,
    pub source_row: u32,
    pub time: Option<String>,
    pub positive_prompt: Option<String>,
    pub negative_prompt: Option<String>,
    pub artists: Option<String>,
    pub image_folder: Option<String>,
    pub image_path: Option<String>,
    pub embedded_image_ref: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
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
        create_page_rows(&transaction, query.tag_mode, query.limit, offset)?;

        let total_count = query_total_count(&transaction, query.tag_mode)?;
        let mut rows = query_page_metadata(&transaction)?;
        attach_page_tags(&transaction, &mut rows)?;

        transaction.execute_batch(&format!(
            "DROP TABLE {PAGE_ROWS_TABLE};
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

    pub fn list_used_tags(&self) -> Result<Vec<TagSummary>, DatabaseError> {
        let mut statement = self.connection.prepare(
            "SELECT tags.name, COUNT(row_tags.row_id)
             FROM tags
             JOIN row_tags ON row_tags.tag_id = tags.id
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

fn create_filter_tags(
    transaction: &Transaction<'_>,
    tags: &[String],
) -> Result<(), rusqlite::Error> {
    transaction.execute_batch(&format!(
        "DROP TABLE IF EXISTS {FILTER_TAGS_TABLE};
         CREATE TEMP TABLE {FILTER_TAGS_TABLE} (
             name TEXT PRIMARY KEY COLLATE BINARY
         ) STRICT, WITHOUT ROWID;
         DROP TABLE IF EXISTS {PAGE_ROWS_TABLE};
         CREATE TEMP TABLE {PAGE_ROWS_TABLE} (
             ordinal INTEGER PRIMARY KEY,
             id INTEGER NOT NULL UNIQUE
         ) STRICT;"
    ))?;
    let mut insert = transaction.prepare(&format!(
        "INSERT INTO {FILTER_TAGS_TABLE}(name) VALUES (?1)"
    ))?;
    for tag in tags {
        insert.execute([tag])?;
    }
    Ok(())
}

fn create_page_rows(
    transaction: &Transaction<'_>,
    mode: TagMatchMode,
    limit: u32,
    offset: i64,
) -> Result<(), rusqlite::Error> {
    let predicate = filter_predicate(mode);
    transaction.execute(
        &format!(
            "INSERT INTO {PAGE_ROWS_TABLE}(ordinal, id)
             SELECT ROW_NUMBER() OVER (ORDER BY rows.source_row, rows.id), rows.id
             FROM rows
             WHERE {predicate}
             ORDER BY rows.source_row, rows.id
             LIMIT ?1 OFFSET ?2"
        ),
        params![limit, offset],
    )?;
    Ok(())
}

fn query_total_count(
    transaction: &Transaction<'_>,
    mode: TagMatchMode,
) -> Result<u64, DatabaseError> {
    let predicate = filter_predicate(mode);
    let count: i64 = transaction.query_row(
        &format!("SELECT COUNT(*) FROM rows WHERE {predicate}"),
        [],
        |row| row.get(0),
    )?;
    u64::try_from(count).map_err(|_| DatabaseError::CountOverflow)
}

fn query_page_metadata(transaction: &Transaction<'_>) -> Result<Vec<RowRecord>, DatabaseError> {
    let mut statement = transaction.prepare(&format!(
        "SELECT rows.id, rows.source_row, rows.time, rows.positive_prompt,
                rows.negative_prompt, rows.artists, rows.image_folder,
                rows.image_path, rows.embedded_image_ref
         FROM {PAGE_ROWS_TABLE} AS page
         JOIN rows ON rows.id = page.id
         ORDER BY page.ordinal"
    ))?;
    let rows = statement
        .query_map([], |row| {
            Ok(RowRecord {
                id: row.get(0)?,
                source_row: row.get(1)?,
                time: row.get(2)?,
                positive_prompt: row.get(3)?,
                negative_prompt: row.get(4)?,
                artists: row.get(5)?,
                image_folder: row.get(6)?,
                image_path: row.get(7)?,
                embedded_image_ref: row.get(8)?,
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

fn filter_predicate(mode: TagMatchMode) -> &'static str {
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
    use crate::excel::{ImportedRow, ParsedWorkbook};

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
    fn lists_used_tag_counts_in_binary_order() {
        let database = tagged_database();

        let tags = database.list_used_tags().unwrap();

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
            })
            .unwrap();

        assert_eq!(page.total_count, 10_000);
        assert_eq!(page.rows.len(), 100);
        assert_eq!(page.rows.first().unwrap().source_row, 9_902);
        assert_eq!(page.rows.last().unwrap().source_row, 10_001);
        assert!(!page.has_more());
    }

    fn query(database: &mut Database, tags: &[&str], mode: TagMatchMode) -> RowPage {
        database
            .query_rows(&RowQuery {
                offset: 0,
                limit: 100,
                tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
                tag_mode: mode,
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

    fn database_with_rows(count: i64) -> Database {
        let mut database = Database::open_in_memory().unwrap();
        let workbook = ParsedWorkbook {
            sheet_name: "NovelAI Metadata".into(),
            rows: (1..=count)
                .map(|id| ImportedRow {
                    source_row: u32::try_from(id + 1).unwrap(),
                    time: Some(format!("time {id}")),
                    positive_prompt: Some(format!("prompt {id}")),
                    negative_prompt: None,
                    artists: None,
                    image_folder: None,
                    image_path: None,
                })
                .collect(),
        };
        database
            .replace_workbook("query.xlsx", &workbook, &[])
            .unwrap();
        database
    }
}
