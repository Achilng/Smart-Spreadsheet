use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use flate2::read::GzDecoder;
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use super::{Database, DatabaseError};

const SOURCE_TAG: u32 = 1;
const SOURCE_ARTIST_NAME: u32 = 1 << 1;
const SOURCE_OTHER_NAME: u32 = 1 << 2;
const SOURCE_ALIAS: u32 = 1 << 3;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DanbooruArtistTag {
    pub id: i64,
    pub name: String,
    pub post_count: u64,
    pub category: u32,
    #[serde(default)]
    pub is_deprecated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DanbooruArtistRecord {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub other_names: Vec<String>,
    #[serde(default)]
    pub is_deleted: bool,
    #[serde(default)]
    pub is_banned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DanbooruTagAlias {
    pub id: i64,
    pub antecedent_name: String,
    pub consequent_name: String,
    pub status: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct ArtistDictionaryInput {
    pub tags: Vec<DanbooruArtistTag>,
    pub artists: Vec<DanbooruArtistRecord>,
    pub aliases: Vec<DanbooruTagAlias>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistDictionaryEntry {
    pub match_name: String,
    pub display_name: String,
    pub canonical_name: String,
    pub post_count: u64,
    pub is_banned: bool,
    pub is_deprecated: bool,
    pub is_ambiguous: bool,
    pub source_mask: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistDictionaryStatus {
    pub synced_at: String,
    pub tag_count: u64,
    pub artist_count: u64,
    pub alias_count: u64,
    pub name_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundledArtistDictionaryHeader {
    pub snapshot_at: String,
    pub tag_count: u64,
    pub artist_count: u64,
    pub alias_count: u64,
    pub name_count: u64,
}

#[derive(Debug, Clone)]
struct EntryAccumulator {
    entry: ArtistDictionaryEntry,
    /// 0 = 单独的 artist Tag，1 = Tag Alias，2 = 现存 Artist 记录。
    identity_rank: u8,
}

#[derive(Debug, Clone)]
struct Candidate {
    display_name: String,
    canonical_name: String,
    post_count: u64,
    is_banned: bool,
    is_deprecated: bool,
    source_mask: u32,
    identity_rank: u8,
}

impl Database {
    pub fn install_bundled_artist_dictionary(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<ArtistDictionaryStatus, DatabaseError> {
        if let Some(status) = self.artist_dictionary_status()? {
            return Ok(status);
        }

        let file = File::open(path.as_ref()).map_err(bundled_dictionary_error)?;
        let decoder = GzDecoder::new(file);
        self.install_bundled_artist_dictionary_reader(BufReader::new(decoder))
    }

    fn install_bundled_artist_dictionary_reader(
        &mut self,
        reader: impl BufRead,
    ) -> Result<ArtistDictionaryStatus, DatabaseError> {
        let mut lines = reader.lines();
        let header_line = lines
            .next()
            .ok_or_else(|| bundled_dictionary_error("缺少快照头"))?
            .map_err(bundled_dictionary_error)?;
        let header = serde_json::from_str::<BundledArtistDictionaryHeader>(&header_line)
            .map_err(bundled_dictionary_error)?;

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute("DELETE FROM artist_dictionary_names", [])?;
        let mut inserted = 0_u64;
        {
            let mut insert = transaction.prepare(
                "INSERT INTO artist_dictionary_names
                    (match_name, display_name, canonical_name, post_count, is_banned,
                     is_deprecated, is_ambiguous, source_mask)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for line in lines {
                let line = line.map_err(bundled_dictionary_error)?;
                if line.is_empty() {
                    continue;
                }
                let entry = serde_json::from_str::<ArtistDictionaryEntry>(&line)
                    .map_err(bundled_dictionary_error)?;
                insert.execute(params![
                    entry.match_name,
                    entry.display_name,
                    entry.canonical_name,
                    to_i64(entry.post_count)?,
                    entry.is_banned,
                    entry.is_deprecated,
                    entry.is_ambiguous,
                    i64::from(entry.source_mask),
                ])?;
                inserted = inserted
                    .checked_add(1)
                    .ok_or(DatabaseError::RowCountOverflow)?;
            }
        }
        if inserted != header.name_count {
            return Err(bundled_dictionary_error(format!(
                "名称数量不匹配：快照声明 {}，实际读取 {inserted}",
                header.name_count
            )));
        }
        transaction.execute(
            "INSERT INTO artist_dictionary_sync
                (singleton, synced_at, tag_count, artist_count, alias_count, name_count)
             VALUES (1, ?1, ?2, ?3, ?4, ?5)",
            params![
                header.snapshot_at,
                to_i64(header.tag_count)?,
                to_i64(header.artist_count)?,
                to_i64(header.alias_count)?,
                to_i64(header.name_count)?,
            ],
        )?;
        transaction.commit()?;

        Ok(ArtistDictionaryStatus {
            synced_at: header.snapshot_at,
            tag_count: header.tag_count,
            artist_count: header.artist_count,
            alias_count: header.alias_count,
            name_count: header.name_count,
        })
    }

    pub fn replace_artist_dictionary(
        &mut self,
        input: &ArtistDictionaryInput,
        synced_at: &str,
    ) -> Result<ArtistDictionaryStatus, DatabaseError> {
        let entries = build_artist_dictionary(input);
        let status = ArtistDictionaryStatus {
            synced_at: synced_at.to_owned(),
            tag_count: u64::try_from(input.tags.len())
                .map_err(|_| DatabaseError::RowCountOverflow)?,
            artist_count: u64::try_from(input.artists.len())
                .map_err(|_| DatabaseError::RowCountOverflow)?,
            alias_count: u64::try_from(input.aliases.len())
                .map_err(|_| DatabaseError::RowCountOverflow)?,
            name_count: u64::try_from(entries.len())
                .map_err(|_| DatabaseError::RowCountOverflow)?,
        };
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute("DELETE FROM artist_dictionary_names", [])?;
        transaction.execute("DELETE FROM artist_dictionary_sync", [])?;
        {
            let mut insert = transaction.prepare(
                "INSERT INTO artist_dictionary_names
                    (match_name, display_name, canonical_name, post_count, is_banned,
                     is_deprecated, is_ambiguous, source_mask)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for entry in &entries {
                insert.execute(params![
                    entry.match_name,
                    entry.display_name,
                    entry.canonical_name,
                    to_i64(entry.post_count)?,
                    entry.is_banned,
                    entry.is_deprecated,
                    entry.is_ambiguous,
                    i64::from(entry.source_mask),
                ])?;
            }
        }
        transaction.execute(
            "INSERT INTO artist_dictionary_sync
                (singleton, synced_at, tag_count, artist_count, alias_count, name_count)
             VALUES (1, ?1, ?2, ?3, ?4, ?5)",
            params![
                status.synced_at,
                to_i64(status.tag_count)?,
                to_i64(status.artist_count)?,
                to_i64(status.alias_count)?,
                to_i64(status.name_count)?,
            ],
        )?;
        transaction.commit()?;
        Ok(status)
    }

    pub fn artist_dictionary_status(
        &self,
    ) -> Result<Option<ArtistDictionaryStatus>, DatabaseError> {
        self.connection
            .query_row(
                "SELECT synced_at, tag_count, artist_count, alias_count, name_count
                 FROM artist_dictionary_sync WHERE singleton = 1",
                [],
                |row| {
                    Ok(ArtistDictionaryStatus {
                        synced_at: row.get(0)?,
                        tag_count: from_i64(row.get(1)?)?,
                        artist_count: from_i64(row.get(2)?)?,
                        alias_count: from_i64(row.get(3)?)?,
                        name_count: from_i64(row.get(4)?)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn artist_dictionary_entries(&self) -> Result<Vec<ArtistDictionaryEntry>, DatabaseError> {
        let mut statement = self.connection.prepare(
            "SELECT match_name, display_name, canonical_name, post_count, is_banned,
                    is_deprecated, is_ambiguous, source_mask
             FROM artist_dictionary_names ORDER BY match_name",
        )?;
        statement
            .query_map([], |row| {
                Ok(ArtistDictionaryEntry {
                    match_name: row.get(0)?,
                    display_name: row.get(1)?,
                    canonical_name: row.get(2)?,
                    post_count: from_i64(row.get(3)?)?,
                    is_banned: row.get(4)?,
                    is_deprecated: row.get(5)?,
                    is_ambiguous: row.get(6)?,
                    source_mask: u32::try_from(row.get::<_, i64>(7)?).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            7,
                            rusqlite::types::Type::Integer,
                            Box::new(error),
                        )
                    })?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn artist_dictionary_entries_by_names<'a>(
        &self,
        names: impl IntoIterator<Item = &'a str>,
    ) -> Result<Vec<ArtistDictionaryEntry>, DatabaseError> {
        let mut statement = self.connection.prepare(
            "SELECT match_name, display_name, canonical_name, post_count, is_banned,
                    is_deprecated, is_ambiguous, source_mask
             FROM artist_dictionary_names WHERE match_name = ?1",
        )?;
        let mut entries = Vec::new();
        for name in names {
            if let Some(entry) = statement
                .query_row([name], |row| {
                    Ok(ArtistDictionaryEntry {
                        match_name: row.get(0)?,
                        display_name: row.get(1)?,
                        canonical_name: row.get(2)?,
                        post_count: from_i64(row.get(3)?)?,
                        is_banned: row.get(4)?,
                        is_deprecated: row.get(5)?,
                        is_ambiguous: row.get(6)?,
                        source_mask: u32::try_from(row.get::<_, i64>(7)?).map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                7,
                                rusqlite::types::Type::Integer,
                                Box::new(error),
                            )
                        })?,
                    })
                })
                .optional()?
            {
                entries.push(entry);
            }
        }
        Ok(entries)
    }
}

pub fn build_artist_dictionary(input: &ArtistDictionaryInput) -> Vec<ArtistDictionaryEntry> {
    let tag_by_name = input
        .tags
        .iter()
        .filter(|tag| tag.category == 1)
        .filter_map(|tag| normalize_name(&tag.name).map(|name| (name, tag)))
        .collect::<HashMap<_, _>>();
    let mut entries = BTreeMap::<String, EntryAccumulator>::new();

    for (match_name, tag) in &tag_by_name {
        merge_candidate(
            &mut entries,
            match_name.clone(),
            Candidate {
                display_name: tag.name.trim().to_owned(),
                canonical_name: tag.name.trim().to_owned(),
                post_count: tag.post_count,
                is_banned: false,
                is_deprecated: tag.is_deprecated,
                source_mask: SOURCE_TAG,
                identity_rank: 0,
            },
        );
    }

    let mut artists = input
        .artists
        .iter()
        .filter(|artist| !artist.is_deleted)
        .collect::<Vec<_>>();
    artists.sort_by_key(|artist| artist.id);
    for artist in artists {
        let Some(canonical_match_name) = normalize_name(&artist.name) else {
            continue;
        };
        let mut names = Vec::with_capacity(artist.other_names.len() + 1);
        names.push((artist.name.as_str(), SOURCE_ARTIST_NAME));
        names.extend(
            artist
                .other_names
                .iter()
                .map(|name| (name.as_str(), SOURCE_OTHER_NAME)),
        );
        let normalized_names = names
            .into_iter()
            .filter_map(|(name, source)| {
                normalize_name(name).map(|match_name| (name, match_name, source))
            })
            .collect::<Vec<_>>();
        let group_post_count = normalized_names
            .iter()
            .filter_map(|(_, match_name, _)| tag_by_name.get(match_name).map(|tag| tag.post_count))
            .max()
            .unwrap_or(0);

        for (name, match_name, source_mask) in normalized_names {
            let tag = tag_by_name.get(&match_name).copied();
            merge_candidate(
                &mut entries,
                match_name,
                Candidate {
                    display_name: name.trim().to_owned(),
                    canonical_name: artist.name.trim().to_owned(),
                    post_count: group_post_count,
                    is_banned: artist.is_banned,
                    is_deprecated: tag.is_some_and(|tag| tag.is_deprecated),
                    source_mask: source_mask | tag.map_or(0, |_| SOURCE_TAG),
                    identity_rank: 2,
                },
            );
        }

        debug_assert!(entries.contains_key(&canonical_match_name));
    }

    let alias_map = preferred_aliases(&input.aliases);
    let aliases = alias_map.iter().collect::<Vec<_>>();
    for (antecedent, consequent) in aliases {
        let Some(target_name) = resolve_alias_target(consequent, &alias_map, &entries) else {
            continue;
        };
        let Some(target) = entries.get(&target_name).cloned() else {
            continue;
        };
        let existing_tag = tag_by_name.get(antecedent).copied();
        merge_candidate(
            &mut entries,
            antecedent.clone(),
            Candidate {
                display_name: existing_tag
                    .map(|tag| tag.name.trim().to_owned())
                    .unwrap_or_else(|| antecedent.clone()),
                canonical_name: target.entry.canonical_name,
                post_count: target.entry.post_count,
                is_banned: target.entry.is_banned,
                is_deprecated: existing_tag.is_some_and(|tag| tag.is_deprecated),
                source_mask: SOURCE_ALIAS | existing_tag.map_or(0, |_| SOURCE_TAG),
                identity_rank: 1,
            },
        );
    }

    entries.into_values().map(|value| value.entry).collect()
}

fn preferred_aliases(aliases: &[DanbooruTagAlias]) -> HashMap<String, String> {
    let mut result = HashMap::<String, (String, bool)>::new();
    for alias in aliases {
        let status = alias.status.to_ascii_lowercase();
        if !matches!(status.as_str(), "active" | "retired") {
            continue;
        }
        let (Some(antecedent), Some(consequent)) = (
            normalize_name(&alias.antecedent_name),
            normalize_name(&alias.consequent_name),
        ) else {
            continue;
        };
        let is_active = status == "active";
        match result.get(&antecedent) {
            Some((_, existing_active)) if *existing_active && !is_active => {}
            _ => {
                result.insert(antecedent, (consequent, is_active));
            }
        }
    }
    result
        .into_iter()
        .map(|(antecedent, (consequent, _))| (antecedent, consequent))
        .collect()
}

fn resolve_alias_target(
    start: &str,
    aliases: &HashMap<String, String>,
    entries: &BTreeMap<String, EntryAccumulator>,
) -> Option<String> {
    let mut current = start.to_owned();
    let mut seen = HashSet::new();
    for _ in 0..64 {
        if !seen.insert(current.clone()) {
            return None;
        }
        match aliases.get(&current) {
            Some(next) => current = next.clone(),
            None => return entries.contains_key(&current).then_some(current),
        }
    }
    None
}

fn merge_candidate(
    entries: &mut BTreeMap<String, EntryAccumulator>,
    match_name: String,
    candidate: Candidate,
) {
    let Some(existing) = entries.get_mut(&match_name) else {
        entries.insert(
            match_name.clone(),
            EntryAccumulator {
                entry: ArtistDictionaryEntry {
                    match_name,
                    display_name: candidate.display_name,
                    canonical_name: candidate.canonical_name,
                    post_count: candidate.post_count,
                    is_banned: candidate.is_banned,
                    is_deprecated: candidate.is_deprecated,
                    is_ambiguous: false,
                    source_mask: candidate.source_mask,
                },
                identity_rank: candidate.identity_rank,
            },
        );
        return;
    };

    let previous_post_count = existing.entry.post_count;
    existing.entry.source_mask |= candidate.source_mask;
    existing.entry.post_count = previous_post_count.max(candidate.post_count);
    existing.entry.is_deprecated &= candidate.is_deprecated;

    if candidate.identity_rank > existing.identity_rank {
        existing.entry.display_name = candidate.display_name;
        existing.entry.canonical_name = candidate.canonical_name;
        existing.entry.is_banned = candidate.is_banned;
        existing.identity_rank = candidate.identity_rank;
    } else if candidate.identity_rank == existing.identity_rank {
        let existing_canonical = normalize_name(&existing.entry.canonical_name);
        let candidate_canonical = normalize_name(&candidate.canonical_name);
        if existing_canonical != candidate_canonical && candidate.identity_rank >= 2 {
            existing.entry.is_ambiguous = true;
            if candidate.post_count > previous_post_count {
                existing.entry.canonical_name = candidate.canonical_name;
                existing.entry.is_banned = candidate.is_banned;
            }
        } else if existing_canonical == candidate_canonical {
            existing.entry.is_banned = candidate.is_banned;
        }
    }
}

fn normalize_name(name: &str) -> Option<String> {
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_lowercase())
}

fn to_i64(value: u64) -> Result<i64, DatabaseError> {
    i64::try_from(value).map_err(|_| DatabaseError::RowCountOverflow)
}

fn from_i64(value: i64) -> Result<u64, rusqlite::Error> {
    u64::try_from(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })
}

fn bundled_dictionary_error(error: impl std::fmt::Display) -> DatabaseError {
    DatabaseError::InvalidBundledArtistDictionary(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;
    use std::io::Cursor;

    use super::*;

    fn tag(id: i64, name: &str, post_count: u64) -> DanbooruArtistTag {
        DanbooruArtistTag {
            id,
            name: name.to_owned(),
            post_count,
            category: 1,
            is_deprecated: false,
        }
    }

    #[test]
    fn banned_current_artist_links_zero_count_historical_name() {
        let input = ArtistDictionaryInput {
            tags: vec![tag(1, "parsley-f", 891), tag(2, "parsley_f", 0)],
            artists: vec![
                DanbooruArtistRecord {
                    id: 27_785,
                    name: "parsley-f".into(),
                    other_names: vec!["parsley_f".into(), "s_f_nov17".into()],
                    is_deleted: false,
                    is_banned: true,
                },
                DanbooruArtistRecord {
                    id: 138_271,
                    name: "parsley_f".into(),
                    other_names: Vec::new(),
                    is_deleted: true,
                    is_banned: false,
                },
            ],
            aliases: Vec::new(),
        };

        let entries = build_artist_dictionary(&input);
        let parsley_f = entries
            .iter()
            .find(|entry| entry.match_name == "parsley_f")
            .unwrap();
        assert_eq!(parsley_f.canonical_name, "parsley-f");
        assert_eq!(parsley_f.post_count, 891);
        assert!(parsley_f.is_banned);
        assert!(!parsley_f.is_ambiguous);
        assert!(entries.iter().any(|entry| entry.match_name == "s_f_nov17"));
    }

    #[test]
    fn deleted_artist_without_artist_tag_or_current_other_name_is_ignored() {
        let entries = build_artist_dictionary(&ArtistDictionaryInput {
            artists: vec![DanbooruArtistRecord {
                id: 9,
                name: "deleted_only".into(),
                other_names: Vec::new(),
                is_deleted: true,
                is_banned: false,
            }],
            ..ArtistDictionaryInput::default()
        });

        assert!(entries.is_empty());
    }

    #[test]
    fn active_and_retired_aliases_resolve_to_artist_identity() {
        let input = ArtistDictionaryInput {
            tags: vec![tag(1, "current_name", 17)],
            aliases: vec![
                DanbooruTagAlias {
                    id: 1,
                    antecedent_name: "old_name".into(),
                    consequent_name: "current_name".into(),
                    status: "retired".into(),
                },
                DanbooruTagAlias {
                    id: 2,
                    antecedent_name: "pending_name".into(),
                    consequent_name: "current_name".into(),
                    status: "pending".into(),
                },
            ],
            ..ArtistDictionaryInput::default()
        };

        let entries = build_artist_dictionary(&input);
        let old = entries
            .iter()
            .find(|entry| entry.match_name == "old_name")
            .unwrap();
        assert_eq!(old.canonical_name, "current_name");
        assert_eq!(old.post_count, 17);
        assert!(
            !entries
                .iter()
                .any(|entry| entry.match_name == "pending_name")
        );
    }

    #[test]
    fn alias_chain_reaches_the_final_artist_identity() {
        let input = ArtistDictionaryInput {
            tags: vec![tag(1, "middle_name", 2), tag(2, "current_name", 17)],
            aliases: vec![
                DanbooruTagAlias {
                    id: 1,
                    antecedent_name: "old_name".into(),
                    consequent_name: "middle_name".into(),
                    status: "retired".into(),
                },
                DanbooruTagAlias {
                    id: 2,
                    antecedent_name: "middle_name".into(),
                    consequent_name: "current_name".into(),
                    status: "active".into(),
                },
            ],
            ..ArtistDictionaryInput::default()
        };

        let entries = build_artist_dictionary(&input);
        let old = entries
            .iter()
            .find(|entry| entry.match_name == "old_name")
            .unwrap();
        assert_eq!(old.canonical_name, "current_name");
        assert_eq!(old.post_count, 17);
    }

    #[test]
    fn replacement_is_atomic_and_status_round_trips() {
        let mut database = Database::open_in_memory().unwrap();
        let input = ArtistDictionaryInput {
            tags: vec![tag(1, "first_artist", 4), tag(2, "second_artist", 12)],
            ..ArtistDictionaryInput::default()
        };

        let status = database
            .replace_artist_dictionary(&input, "2026-07-22T12:00:00Z")
            .unwrap();
        assert_eq!(status.name_count, 2);
        assert_eq!(database.artist_dictionary_status().unwrap(), Some(status));
        assert_eq!(database.artist_dictionary_entries().unwrap().len(), 2);
    }

    #[test]
    fn bundled_dictionary_stream_installs_atomically() {
        let header = BundledArtistDictionaryHeader {
            snapshot_at: "2026-07-22T17:50:38Z".into(),
            tag_count: 2,
            artist_count: 1,
            alias_count: 0,
            name_count: 2,
        };
        let entries = build_artist_dictionary(&ArtistDictionaryInput {
            tags: vec![tag(1, "parsley-f", 891), tag(2, "parsley_f", 0)],
            artists: vec![DanbooruArtistRecord {
                id: 27_785,
                name: "parsley-f".into(),
                other_names: vec!["parsley_f".into()],
                is_deleted: false,
                is_banned: true,
            }],
            aliases: Vec::new(),
        });
        let mut payload = String::new();
        writeln!(&mut payload, "{}", serde_json::to_string(&header).unwrap()).unwrap();
        for entry in entries {
            writeln!(&mut payload, "{}", serde_json::to_string(&entry).unwrap()).unwrap();
        }

        let mut database = Database::open_in_memory().unwrap();
        let status = database
            .install_bundled_artist_dictionary_reader(Cursor::new(payload))
            .unwrap();
        assert_eq!(status.name_count, 2);
        assert_eq!(database.artist_dictionary_status().unwrap(), Some(status));
        let names = database
            .artist_dictionary_entries_by_names(["parsley-f", "parsley_f"])
            .unwrap();
        assert_eq!(names.len(), 2);
        assert!(names.iter().all(|entry| entry.is_banned));
    }

    #[test]
    fn invalid_bundled_dictionary_count_rolls_back() {
        let header = BundledArtistDictionaryHeader {
            snapshot_at: "2026-07-22T17:50:38Z".into(),
            tag_count: 0,
            artist_count: 0,
            alias_count: 0,
            name_count: 1,
        };
        let payload = format!("{}\n", serde_json::to_string(&header).unwrap());
        let mut database = Database::open_in_memory().unwrap();

        let error = database
            .install_bundled_artist_dictionary_reader(Cursor::new(payload))
            .unwrap_err();

        assert!(error.to_string().contains("名称数量不匹配"));
        assert!(database.artist_dictionary_status().unwrap().is_none());
        assert!(database.artist_dictionary_entries().unwrap().is_empty());
    }
}
