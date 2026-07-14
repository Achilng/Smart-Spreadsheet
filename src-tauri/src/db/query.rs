use std::collections::HashMap;

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use super::tags::normalize_tags;
use super::{Database, DatabaseError};

pub const MAX_PAGE_SIZE: u32 = 500;
pub(super) const FILTER_TAGS_TABLE: &str = "temp.query_filter_tags";
/// 主查询的筛选结果缓存表：随 `Database::query_cache` 一起跨调用复用。
const FILTERED_ROWS_TABLE: &str = "temp.query_filtered_rows";
/// 分组成员等一次性查询的物化表，与缓存表分开以免破坏缓存。
const SCRATCH_ROWS_TABLE: &str = "temp.query_scratch_rows";
const PAGE_ROWS_TABLE: &str = "temp.query_page_rows";

/// 已物化筛选结果的标识。key 覆盖全部筛选参数；数据变更通过
/// `Database::bump_data_version` 直接清空缓存。
#[derive(Debug)]
pub(super) struct QueryCache {
    key: String,
    total_count: u64,
}

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
    #[serde(default)]
    pub search: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowRecord {
    pub id: i64,
    pub batch_id: i64,
    pub source_ordinal: u32,
    pub time: Option<String>,
    pub positive_prompt: Option<String>,
    pub character_prompt: Option<String>,
    pub negative_prompt: Option<String>,
    pub note: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DedupeCluster {
    pub key: String,
    pub member_count: u64,
    pub alias: Option<String>,
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
        let cache_key = query_cache_key(query, &tags);
        let cached_total = self
            .query_cache
            .as_ref()
            .filter(|cache| cache.key == cache_key)
            .map(|cache| cache.total_count);

        let total_count = match cached_total {
            Some(total) => total,
            None => {
                self.query_cache = None;
                let transaction = self.connection.transaction()?;
                create_filter_tags(&transaction, &tags)?;
                create_filtered_rows_table(&transaction, FILTERED_ROWS_TABLE)?;
                populate_filtered_rows(
                    &transaction,
                    FILTERED_ROWS_TABLE,
                    query.tag_mode,
                    query.dedupe,
                    query.single_artist_only,
                    query.group_view,
                    query.hide_grouped,
                    &query.search,
                )?;
                transaction.execute_batch(&format!("DROP TABLE {FILTER_TAGS_TABLE};"))?;
                transaction.commit()?;
                let total = query_total_count(&self.connection, FILTERED_ROWS_TABLE)?;
                self.query_cache = Some(QueryCache {
                    key: cache_key,
                    total_count: total,
                });
                total
            }
        };

        create_page_rows_table(&self.connection)?;
        create_page_rows(&self.connection, FILTERED_ROWS_TABLE, query.limit, offset)?;
        let mut rows = query_page_metadata(&self.connection)?;
        attach_page_tags(&self.connection, &mut rows)?;
        self.connection
            .execute_batch(&format!("DROP TABLE {PAGE_ROWS_TABLE};"))?;

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
        create_filtered_rows_table(&transaction, SCRATCH_ROWS_TABLE)?;
        transaction.execute(
            &format!(
                "INSERT INTO {SCRATCH_ROWS_TABLE}(id)
                 SELECT id FROM rows WHERE group_id = ?1"
            ),
            [group_id],
        )?;
        create_page_rows_table(&transaction)?;
        create_page_rows(&transaction, SCRATCH_ROWS_TABLE, limit, offset_i64)?;

        let total_count = query_total_count(&transaction, SCRATCH_ROWS_TABLE)?;
        let mut rows = query_page_metadata(&transaction)?;
        attach_page_tags(&transaction, &mut rows)?;

        transaction.execute_batch(&format!(
            "DROP TABLE {PAGE_ROWS_TABLE};
             DROP TABLE {SCRATCH_ROWS_TABLE};"
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

    /// 返回全库去重后的画师片段：逐行画师串按换行拆分，trim、去空、去重后排序。
    pub fn list_distinct_artists(&self) -> Result<Vec<String>, DatabaseError> {
        let mut statement = self.connection.prepare(
            "SELECT artists FROM rows
             WHERE artists IS NOT NULL AND TRIM(artists) != ''",
        )?;
        let stored = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut set = std::collections::BTreeSet::new();
        for value in stored {
            for fragment in value.split('\n') {
                let trimmed = fragment.trim();
                if !trimmed.is_empty() {
                    set.insert(trimmed.to_string());
                }
            }
        }
        Ok(set.into_iter().collect())
    }

    /// 返回画师串与给定值完全相同的所有行 ID（全库，忽略 Tag 筛选）。
    pub fn row_ids_with_artists(&self, artists: &str) -> Result<Vec<i64>, DatabaseError> {
        let trimmed = artists.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        let mut statement = self.connection.prepare(
            "SELECT id FROM rows
             WHERE NULLIF(TRIM(COALESCE(artists, '')), '') = ?1
             ORDER BY id",
        )?;
        let ids = statement
            .query_map(params![trimmed], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ids)
    }

    pub fn list_dedupe_clusters(
        &mut self,
        dedupe: DedupeMode,
        tags: &[String],
        tag_mode: TagMatchMode,
        single_artist_only: bool,
        hide_grouped: bool,
    ) -> Result<Vec<DedupeCluster>, DatabaseError> {
        let (column, mode_str) = match dedupe {
            DedupeMode::PositivePrompt => ("positive_prompt", "positivePrompt"),
            DedupeMode::Artists => ("artists", "artists"),
            DedupeMode::None => return Ok(Vec::new()),
        };
        let normalized = normalize_tags(tags);
        let transaction = self.connection.transaction()?;
        create_filter_tags(&transaction, &normalized)?;

        let tag_predicate = filter_predicate(tag_mode);
        let mut predicate = tag_predicate.to_owned();
        if single_artist_only {
            predicate = format!(
                "({predicate})
                 AND rows.artists IS NOT NULL
                 AND TRIM(rows.artists) != ''
                 AND INSTR(rows.artists, CHAR(10)) = 0"
            );
        }
        if hide_grouped {
            predicate = format!("({predicate}) AND rows.group_id IS NULL");
        }

        let clusters = {
            let mut statement = transaction.prepare(&format!(
                "SELECT g.dedupe_key, g.cnt, da.alias
                 FROM (
                     SELECT dedupe_key, COUNT(*) AS cnt
                     FROM (
                         SELECT NULLIF(TRIM(COALESCE(rows.{column}, '')), '') AS dedupe_key
                         FROM rows
                         WHERE {predicate}
                     )
                     WHERE dedupe_key IS NOT NULL
                     GROUP BY dedupe_key
                     HAVING cnt >= 2
                 ) g
                 LEFT JOIN dedupe_aliases da ON da.mode = ?1 AND da.key = g.dedupe_key
                 ORDER BY g.cnt DESC, g.dedupe_key"
            ))?;
            statement
                .query_map([mode_str], |row| {
                    Ok(DedupeCluster {
                        key: row.get(0)?,
                        member_count: row.get::<_, i64>(1)? as u64,
                        alias: row.get(2)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?
        };

        transaction.execute_batch(&format!("DROP TABLE {FILTER_TAGS_TABLE};"))?;
        transaction.commit()?;
        Ok(clusters)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn get_dedupe_cluster_members(
        &mut self,
        dedupe: DedupeMode,
        key: &str,
        tags: &[String],
        tag_mode: TagMatchMode,
        single_artist_only: bool,
        hide_grouped: bool,
        offset: u64,
        limit: u32,
    ) -> Result<RowPage, DatabaseError> {
        if limit == 0 || limit > MAX_PAGE_SIZE {
            return Err(DatabaseError::InvalidPageSize {
                requested: limit,
                maximum: MAX_PAGE_SIZE,
            });
        }
        let column = match dedupe {
            DedupeMode::PositivePrompt => "positive_prompt",
            DedupeMode::Artists => "artists",
            DedupeMode::None => {
                return Ok(RowPage {
                    rows: Vec::new(),
                    total_count: 0,
                    offset,
                    limit,
                })
            }
        };
        let offset_i64 = i64::try_from(offset).map_err(|_| DatabaseError::OffsetOverflow)?;
        let normalized = normalize_tags(tags);
        let transaction = self.connection.transaction()?;
        create_filter_tags(&transaction, &normalized)?;

        let tag_predicate = filter_predicate(tag_mode);
        let mut predicate = tag_predicate.to_owned();
        if single_artist_only {
            predicate = format!(
                "({predicate})
                 AND rows.artists IS NOT NULL
                 AND TRIM(rows.artists) != ''
                 AND INSTR(rows.artists, CHAR(10)) = 0"
            );
        }
        if hide_grouped {
            predicate = format!("({predicate}) AND rows.group_id IS NULL");
        }

        create_filtered_rows_table(&transaction, SCRATCH_ROWS_TABLE)?;
        transaction.execute(
            &format!(
                "INSERT INTO {SCRATCH_ROWS_TABLE}(id)
                 SELECT rows.id FROM rows
                 WHERE NULLIF(TRIM(COALESCE(rows.{column}, '')), '') = ?1
                   AND ({predicate})"
            ),
            params![key],
        )?;
        create_page_rows_table(&transaction)?;
        create_page_rows(&transaction, SCRATCH_ROWS_TABLE, limit, offset_i64)?;

        let total_count = query_total_count(&transaction, SCRATCH_ROWS_TABLE)?;
        let mut rows = query_page_metadata(&transaction)?;
        attach_page_tags(&transaction, &mut rows)?;

        transaction.execute_batch(&format!(
            "DROP TABLE {PAGE_ROWS_TABLE};
             DROP TABLE {SCRATCH_ROWS_TABLE};
             DROP TABLE {FILTER_TAGS_TABLE};"
        ))?;
        transaction.commit()?;

        Ok(RowPage {
            rows,
            total_count,
            offset,
            limit,
        })
    }

    pub fn set_dedupe_alias(
        &mut self,
        mode: DedupeMode,
        key: &str,
        alias: &str,
    ) -> Result<(), DatabaseError> {
        let mode_str = match mode {
            DedupeMode::PositivePrompt => "positivePrompt",
            DedupeMode::Artists => "artists",
            DedupeMode::None => return Ok(()),
        };
        let alias = alias.trim();
        if alias.is_empty() {
            self.connection.execute(
                "DELETE FROM dedupe_aliases WHERE mode = ?1 AND key = ?2",
                params![mode_str, key],
            )?;
        } else {
            self.connection.execute(
                "INSERT INTO dedupe_aliases (mode, key, alias) VALUES (?1, ?2, ?3)
                 ON CONFLICT(mode, key) DO UPDATE SET alias = excluded.alias",
                params![mode_str, key, alias],
            )?;
        }
        Ok(())
    }
}

/// 缓存键覆盖影响筛选结果集的全部参数（分页参数除外）。
fn query_cache_key(query: &RowQuery, normalized_tags: &[String]) -> String {
    format!(
        "{tags:?}\u{1}{mode:?}\u{1}{dedupe:?}\u{1}{sao}\u{1}{gv}\u{1}{hg}\u{1}{search}",
        tags = normalized_tags,
        mode = query.tag_mode,
        dedupe = query.dedupe,
        sao = query.single_artist_only,
        gv = query.group_view,
        hg = query.hide_grouped,
        search = query.search.trim().to_lowercase(),
    )
}

pub(super) fn create_filter_tags(
    connection: &Connection,
    tags: &[String],
) -> Result<(), rusqlite::Error> {
    connection.execute_batch(&format!(
        "DROP TABLE IF EXISTS {FILTER_TAGS_TABLE};
         CREATE TEMP TABLE {FILTER_TAGS_TABLE} (
             name TEXT PRIMARY KEY COLLATE BINARY
         ) STRICT, WITHOUT ROWID;"
    ))?;
    let mut insert = connection.prepare(&format!(
        "INSERT INTO {FILTER_TAGS_TABLE}(name) VALUES (?1)"
    ))?;
    for tag in tags {
        insert.execute([tag])?;
    }
    Ok(())
}

fn create_page_rows_table(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch(&format!(
        "DROP TABLE IF EXISTS {PAGE_ROWS_TABLE};
         CREATE TEMP TABLE {PAGE_ROWS_TABLE} (
             ordinal INTEGER PRIMARY KEY,
             id INTEGER NOT NULL UNIQUE
         ) STRICT;"
    ))
}

fn create_filtered_rows_table(
    connection: &Connection,
    table: &str,
) -> Result<(), rusqlite::Error> {
    connection.execute_batch(&format!(
        "DROP TABLE IF EXISTS {table};
         CREATE TEMP TABLE {table} (
             id INTEGER PRIMARY KEY
         ) STRICT, WITHOUT ROWID;"
    ))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn populate_filtered_rows(
    connection: &Connection,
    target_table: &str,
    mode: TagMatchMode,
    dedupe: DedupeMode,
    single_artist_only: bool,
    group_view: bool,
    hide_grouped: bool,
    search: &str,
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

    let search_lower = search.trim().to_lowercase();
    let has_search = !search_lower.is_empty();
    if has_search {
        predicate = format!(
            "({predicate}) AND (
                INSTR(LOWER(COALESCE(rows.image_path, '')), ?1) > 0
                OR INSTR(LOWER(COALESCE(rows.positive_prompt, '')), ?1) > 0
                OR INSTR(LOWER(COALESCE(rows.character_prompt, '')), ?1) > 0
                OR INSTR(LOWER(COALESCE(rows.negative_prompt, '')), ?1) > 0
                OR INSTR(LOWER(COALESCE(rows.note, '')), ?1) > 0
                OR INSTR(LOWER(COALESCE(rows.artists, '')), ?1) > 0
            )"
        );
    }
    let search_params: &[&dyn rusqlite::types::ToSql] = if has_search {
        &[&search_lower]
    } else {
        &[]
    };

    if group_view {
        connection.execute(
            &format!(
                "INSERT INTO {target_table}(id)
                 SELECT MIN(rows.id) FROM rows
                 WHERE ({predicate}) AND rows.group_id IS NOT NULL
                 GROUP BY rows.group_id
                 UNION ALL
                 SELECT rows.id FROM rows
                 WHERE ({predicate}) AND rows.group_id IS NULL"
            ),
            search_params,
        )?;
    } else {
        match dedupe {
            DedupeMode::None => connection.execute(
                &format!(
                    "INSERT INTO {target_table}(id)
                     SELECT rows.id FROM rows WHERE {predicate}"
                ),
                search_params,
            )?,
            DedupeMode::PositivePrompt | DedupeMode::Artists => {
                let column = match dedupe {
                    DedupeMode::PositivePrompt => "positive_prompt",
                    DedupeMode::Artists => "artists",
                    DedupeMode::None => unreachable!(),
                };
                connection.execute(
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
                    search_params,
                )?
            }
        };
    }
    Ok(())
}

fn create_page_rows(
    connection: &Connection,
    source_table: &str,
    limit: u32,
    offset: i64,
) -> Result<(), rusqlite::Error> {
    // 行的展示顺序即入库顺序（rows.id 单调递增）。
    connection.execute(
        &format!(
            "INSERT INTO {PAGE_ROWS_TABLE}(ordinal, id)
             SELECT ROW_NUMBER() OVER (ORDER BY filtered.id), filtered.id
             FROM {source_table} AS filtered
             ORDER BY filtered.id
             LIMIT ?1 OFFSET ?2"
        ),
        params![limit, offset],
    )?;
    Ok(())
}

fn query_total_count(connection: &Connection, table: &str) -> Result<u64, DatabaseError> {
    let count: i64 = connection.query_row(
        &format!("SELECT COUNT(*) FROM {table}"),
        [],
        |row| row.get(0),
    )?;
    u64::try_from(count).map_err(|_| DatabaseError::CountOverflow)
}

fn query_page_metadata(connection: &Connection) -> Result<Vec<RowRecord>, DatabaseError> {
    let mut statement = connection.prepare(&format!(
        "SELECT rows.id, rows.batch_id, rows.source_ordinal, rows.time,
                rows.positive_prompt, rows.character_prompt, rows.negative_prompt, rows.note,
                rows.artists, rows.image_folder, rows.image_path, rows.stored_image_path,
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
                character_prompt: row.get(5)?,
                negative_prompt: row.get(6)?,
                note: row.get(7)?,
                artists: row.get(8)?,
                image_folder: row.get(9)?,
                image_path: row.get(10)?,
                stored_image_path: row.get(11)?,
                metadata_failed: row.get(12)?,
                group_id: row.get(13)?,
                group_name: row.get(14)?,
                tags: Vec::new(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn attach_page_tags(
    connection: &Connection,
    rows: &mut [RowRecord],
) -> Result<(), DatabaseError> {
    let index_by_id: HashMap<i64, usize> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.id, index))
        .collect();
    let mut statement = connection.prepare(&format!(
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

/// Tag 筛选谓词。子查询与外层行无关联，SQLite 只物化一次命中行集合，
/// 避免旧实现对每一行执行相关子查询（万行库下的主要卡顿来源）。
pub(super) fn filter_predicate(mode: TagMatchMode) -> &'static str {
    match mode {
        TagMatchMode::And => {
            "(SELECT COUNT(*) FROM query_filter_tags) = 0
             OR rows.id IN (
                 SELECT row_tags.row_id
                 FROM row_tags
                 JOIN tags ON tags.id = row_tags.tag_id
                 JOIN query_filter_tags ON query_filter_tags.name = tags.name COLLATE BINARY
                 GROUP BY row_tags.row_id
                 HAVING COUNT(*) = (SELECT COUNT(*) FROM query_filter_tags))"
        }
        TagMatchMode::Or => {
            "(SELECT COUNT(*) FROM query_filter_tags) = 0
             OR rows.id IN (
                 SELECT row_tags.row_id
                 FROM row_tags
                 JOIN tags ON tags.id = row_tags.tag_id
                 JOIN query_filter_tags ON query_filter_tags.name = tags.name COLLATE BINARY)"
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
                search: String::new(),
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
                search: String::new(),
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
                search: String::new(),
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
                search: String::new(),
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
                    search: String::new(),
                })
                .unwrap_err();
            assert!(matches!(error, DatabaseError::InvalidPageSize { .. }));
        }
    }

    #[test]
    fn cached_filter_serves_pages_and_refreshes_after_bump() {
        let mut database = database_with_rows(5);

        assert_eq!(query(&mut database, &[], TagMatchMode::And).total_count, 5);
        // 同参数二次查询走缓存，结果一致
        let cached = query(&mut database, &[], TagMatchMode::And);
        assert_eq!(cached.total_count, 5);
        assert_eq!(cached.rows.len(), 5);

        super::super::test_support::append_rows(
            &mut database,
            &[super::super::batches::NewRow {
                source_ordinal: 99,
                identity: r"file:d:\test\extra.png".into(),
                ..super::super::batches::NewRow::default()
            }],
        );
        // 缓存语义：数据变更后、失效前结果保持不变
        assert_eq!(query(&mut database, &[], TagMatchMode::And).total_count, 5);

        database.bump_data_version();
        assert_eq!(query(&mut database, &[], TagMatchMode::And).total_count, 6);
    }

    #[test]
    fn search_matches_character_prompt() {
        let mut database = database_with_rows(3);
        database
            .connection
            .execute(
                "UPDATE rows SET character_prompt = 'silver hair, unique_role_token' WHERE id = 2",
                [],
            )
            .unwrap();

        let result = database
            .query_rows(&RowQuery {
                offset: 0,
                limit: 10,
                tags: Vec::new(),
                tag_mode: TagMatchMode::And,
                dedupe: DedupeMode::None,
                single_artist_only: false,
                group_view: false,
                hide_grouped: false,
                search: "UNIQUE_ROLE_TOKEN".into(),
            })
            .unwrap();

        assert_eq!(result.total_count, 1);
        assert_eq!(result.rows[0].id, 2);
        assert_eq!(
            result.rows[0].character_prompt.as_deref(),
            Some("silver hair, unique_role_token")
        );
    }

    #[test]
    fn search_matches_note() {
        let mut database = database_with_rows(3);
        database.update_note(2, "夏日海边预设").unwrap();

        let result = database
            .query_rows(&RowQuery {
                offset: 0,
                limit: 10,
                tags: Vec::new(),
                tag_mode: TagMatchMode::And,
                dedupe: DedupeMode::None,
                single_artist_only: false,
                group_view: false,
                hide_grouped: false,
                search: "海边".into(),
            })
            .unwrap();

        assert_eq!(result.total_count, 1);
        assert_eq!(result.rows[0].id, 2);
        assert_eq!(result.rows[0].note.as_deref(), Some("夏日海边预设"));
    }

    #[test]
    fn cache_hit_paging_returns_consistent_pages() {
        let mut database = database_with_rows(10);
        let page_query = |offset: u64| RowQuery {
            offset,
            limit: 3,
            tags: Vec::new(),
            tag_mode: TagMatchMode::And,
            dedupe: DedupeMode::None,
            single_artist_only: false,
            group_view: false,
            hide_grouped: false,
            search: String::new(),
        };

        let first = database.query_rows(&page_query(0)).unwrap();
        let second = database.query_rows(&page_query(3)).unwrap();
        let last = database.query_rows(&page_query(9)).unwrap();

        assert_eq!(first.rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!(second.rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![4, 5, 6]);
        assert_eq!(last.rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![10]);
        assert_eq!(last.total_count, 10);
    }

    #[test]
    fn scratch_member_queries_do_not_clobber_cached_filter() {
        let mut database = database_with_rows(4);
        database
            .connection
            .execute("UPDATE rows SET artists = 'artist:a' WHERE id IN (1, 2)", [])
            .unwrap();
        database.bump_data_version();

        let page_query = |offset: u64| RowQuery {
            offset,
            limit: 2,
            tags: Vec::new(),
            tag_mode: TagMatchMode::And,
            dedupe: DedupeMode::None,
            single_artist_only: false,
            group_view: false,
            hide_grouped: false,
            search: String::new(),
        };
        assert_eq!(database.query_rows(&page_query(0)).unwrap().total_count, 4);

        // 成员查询使用 scratch 表，不得破坏已缓存的主筛选结果
        let members = database
            .get_dedupe_cluster_members(
                DedupeMode::Artists,
                "artist:a",
                &[],
                TagMatchMode::And,
                false,
                false,
                0,
                100,
            )
            .unwrap();
        assert_eq!(members.total_count, 2);

        let page = database.query_rows(&page_query(2)).unwrap();
        assert_eq!(page.rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![3, 4]);
        assert_eq!(page.total_count, 4);
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
                search: String::new(),
            })
            .unwrap();

        assert_eq!(page.total_count, 10_000);
        assert_eq!(page.rows.len(), 100);
        assert_eq!(page.rows.first().unwrap().source_ordinal, 9_902);
        assert_eq!(page.rows.last().unwrap().source_ordinal, 10_001);
        assert!(!page.has_more());
    }

    /// 万行库性能基准（手动运行）：
    /// cargo test --release bench_ten_thousand -- --ignored --nocapture
    #[test]
    #[ignore = "性能基准，手动运行"]
    fn bench_ten_thousand_row_filters() {
        use std::time::Instant;

        let filler = "masterpiece, best quality, highly detailed background, cinematic lighting, \
                      1girl, solo, long hair, looking at viewer, intricate details, depth of field, \
                      soft shadows, volumetric light, dynamic angle, wide shot, detailed face"
            .repeat(3);
        let rows: Vec<super::super::batches::NewRow> = (1..=10_000)
            .map(|index| super::super::batches::NewRow {
                source_ordinal: index as u32,
                identity: format!(r"file:d:\bench\{index}.png"),
                positive_prompt: Some(format!("artist:painter{}, {filler}", index % 97)),
                artists: Some(format!("artist:painter{}", index % 97)),
                ..super::super::batches::NewRow::default()
            })
            .collect();
        let mut database = Database::open_in_memory().unwrap();
        super::super::test_support::append_rows(&mut database, &rows);
        let all_ids: Vec<i64> = (1..=10_000).collect();
        database
            .add_tags_to_rows(&all_ids, &["常用".into()])
            .unwrap();
        let half_ids: Vec<i64> = (1..=5_000).collect();
        database
            .add_tags_to_rows(&half_ids, &["精选".into()])
            .unwrap();
        database.bump_data_version();

        let base = RowQuery {
            offset: 0,
            limit: 200,
            tags: Vec::new(),
            tag_mode: TagMatchMode::And,
            dedupe: DedupeMode::None,
            single_artist_only: false,
            group_view: false,
            hide_grouped: false,
            search: String::new(),
        };
        let mut bench = |label: &str, query: &RowQuery| {
            let start = Instant::now();
            let page = database.query_rows(query).unwrap();
            println!(
                "{label}: {:?}（命中 {} 行）",
                start.elapsed(),
                page.total_count
            );
        };

        bench("无筛选 首查（全量物化）", &base);
        bench("无筛选 缓存翻页", &RowQuery { offset: 5_000, ..base.clone() });
        let tagged = RowQuery {
            tags: vec!["常用".into(), "精选".into()],
            ..base.clone()
        };
        bench("双 Tag AND 首查", &tagged);
        bench("双 Tag AND 缓存翻页", &RowQuery { offset: 2_000, ..tagged.clone() });
        let searched = RowQuery {
            search: "painter42".into(),
            ..base.clone()
        };
        bench("文本搜索 首查（INSTR 全扫）", &searched);
        bench("文本搜索 缓存翻页", &RowQuery { offset: 50, ..searched.clone() });
        let deduped = RowQuery {
            dedupe: DedupeMode::Artists,
            ..base.clone()
        };
        bench("画师串去重 首查", &deduped);
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
                search: String::new(),
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

    #[test]
    fn list_distinct_artists_splits_dedupes_and_sorts() {
        let database = database_with_rows(4);
        database
            .connection
            .execute_batch(
                "UPDATE rows SET artists = CASE id
                     WHEN 1 THEN 'artist:b'
                     WHEN 2 THEN 'artist:a' || CHAR(10) || 'artist:b'
                     WHEN 3 THEN '  artist:c  '
                     WHEN 4 THEN ''
                 END;",
            )
            .unwrap();

        let artists = database.list_distinct_artists().unwrap();
        assert_eq!(artists, vec!["artist:a", "artist:b", "artist:c"]);
    }

    #[test]
    fn row_ids_with_artists_matches_exact_trimmed_value() {
        let database = database_with_rows(4);
        database
            .connection
            .execute_batch(
                "UPDATE rows SET artists = CASE id
                     WHEN 1 THEN 'artist:a'
                     WHEN 2 THEN ' artist:a '
                     WHEN 3 THEN 'artist:b'
                     WHEN 4 THEN NULL
                 END;",
            )
            .unwrap();

        assert_eq!(database.row_ids_with_artists("artist:a").unwrap(), vec![1, 2]);
        assert_eq!(database.row_ids_with_artists("artist:b").unwrap(), vec![3]);
        assert!(database.row_ids_with_artists("   ").unwrap().is_empty());
        assert!(
            database
                .row_ids_with_artists("artist:missing")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn list_dedupe_clusters_excludes_singletons() {
        let mut database = database_with_rows(3);
        database
            .connection
            .execute_batch(
                "UPDATE rows SET artists = CASE id
                     WHEN 1 THEN 'artist:a'
                     WHEN 2 THEN 'artist:a'
                     WHEN 3 THEN 'artist:solo'
                 END;",
            )
            .unwrap();

        let duplicates = database
            .list_dedupe_clusters(DedupeMode::Artists, &[], TagMatchMode::And, false, false)
            .unwrap();
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].key, "artist:a");
        assert_eq!(duplicates[0].member_count, 2);
    }
}
