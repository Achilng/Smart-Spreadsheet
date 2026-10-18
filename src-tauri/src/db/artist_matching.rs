//! Artist identity storage and maintenance. Normalization itself lives only in prompt_matching.
use rusqlite::{
    Connection, OptionalExtension, TransactionBehavior, functions::FunctionFlags, params,
};
use serde::Serialize;

use super::{Database, DatabaseError, llm, prompt_matching};

const RULES_SETTING: &str = "artist_match_rules";

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistMatchingRefresh {
    pub rows: u64,
    pub changed_rows: u64,
    pub groups_before: u64,
    pub groups_after: u64,
    pub merged_representatives: u64,
}

/// The settings are an explicit SQL argument, so long-lived import connections cannot
/// keep using an outdated whitelist. Decode once per distinct settings document.
pub(super) fn register(connection: &Connection) -> rusqlite::Result<()> {
    let cache = std::sync::Mutex::new((None::<Option<String>>, Vec::<String>::new()));
    connection.create_scalar_function(
        "style_match_key",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        move |context| {
            let text: Option<String> = context.get(0)?;
            let settings: Option<String> = context.get(1)?;
            let mut cached = cache.lock().map_err(|_| {
                rusqlite::Error::UserFunctionError(Box::new(std::io::Error::other(
                    "画风匹配缓存不可用",
                )))
            })?;
            if cached.0.as_ref() != Some(&settings) {
                let parsed = settings
                    .as_deref()
                    .map(serde_json::from_str::<llm::Settings>)
                    .transpose()
                    .map_err(|error| rusqlite::Error::UserFunctionError(Box::new(error)))?
                    .unwrap_or_default();
                cached.1 = prompt_matching::clean_ignored_tags(&parsed.ignored_tags);
                cached.0 = Some(settings);
            }
            Ok(text
                .as_deref()
                .and_then(|text| prompt_matching::artist_key(text, &cached.1)))
        },
    )
}

impl Database {
    pub(super) fn artist_key(&self, text: &str) -> Result<Option<String>, DatabaseError> {
        Ok(self.connection.query_row(
            "SELECT style_match_key(?1, (SELECT value FROM settings WHERE key = ?2))",
            params![text, llm::SETTINGS_KEY],
            |row| row.get(0),
        )?)
    }

    /// Settings and derived identities change together, including representative/alias merging.
    pub(super) fn save_matching_settings(&mut self, value: &str) -> Result<(), DatabaseError> {
        let _: llm::Settings =
            serde_json::from_str(value).map_err(|e| llm::invalid(e.to_string()))?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO settings(key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![llm::SETTINGS_KEY, value],
        )?;
        refresh(&transaction, false)?;
        transaction.commit()?;
        self.bump_data_version();
        Ok(())
    }

    pub fn refresh_artist_matching(
        &mut self,
        force: bool,
    ) -> Result<ArtistMatchingRefresh, DatabaseError> {
        if !force
            && self.setting(RULES_SETTING)?.as_deref()
                == Some(&prompt_matching::rules_key(
                    &self.llm_settings()?.ignored_tags,
                ))
        {
            return Ok(ArtistMatchingRefresh::default());
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let result = refresh(&transaction, force)?;
        transaction.commit()?;
        self.bump_data_version();
        Ok(result)
    }
}

fn refresh(connection: &Connection, force: bool) -> Result<ArtistMatchingRefresh, DatabaseError> {
    let settings: Option<String> = connection
        .query_row(
            "SELECT value FROM settings WHERE key=?1",
            [llm::SETTINGS_KEY],
            |row| row.get(0),
        )
        .optional()?;
    let parsed = settings
        .as_deref()
        .map(serde_json::from_str::<llm::Settings>)
        .transpose()
        .map_err(|e| llm::invalid(e.to_string()))?
        .unwrap_or_default();
    let rules = prompt_matching::rules_key(&parsed.ignored_tags);
    let previous: Option<String> = connection
        .query_row(
            "SELECT value FROM settings WHERE key=?1",
            [RULES_SETTING],
            |row| row.get(0),
        )
        .optional()?;
    if !force && previous.as_deref() == Some(&rules) {
        return Ok(ArtistMatchingRefresh {
            rows: 0,
            changed_rows: 0,
            groups_before: 0,
            groups_after: 0,
            merged_representatives: 0,
        });
    }
    let before_expression = if previous.is_none() {
        "NULLIF(TRIM(artists),'')"
    } else {
        "artist_match_key"
    };
    let (rows, groups_before): (u64, u64) = connection.query_row(
        &format!("SELECT COUNT(*), COUNT(DISTINCT {before_expression}) FROM rows"),
        [],
        |row| Ok((row.get::<_, i64>(0)? as u64, row.get::<_, i64>(1)? as u64)),
    )?;
    // Keep the previously selected image IDs before update triggers invalidate old identities.
    let representatives = {
        let mut statement =
            connection.prepare("SELECT row_id FROM artist_representatives ORDER BY row_id")?;
        statement
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let changed_rows = connection.execute(
        "UPDATE rows SET artist_match_key=style_match_key(artists,?1)
         WHERE artist_match_key IS NOT style_match_key(artists,?1)",
        [&settings],
    )? as u64;
    connection.execute("DELETE FROM artist_representatives", [])?;
    for row_id in &representatives {
        connection.execute(
            "INSERT OR IGNORE INTO artist_representatives(artist_key,row_id)
             SELECT artist_match_key,id FROM rows WHERE id=?1 AND artist_match_key IS NOT NULL",
            [row_id],
        )?;
    }
    // Keep old aliases as well as their canonical lookup entries. If aliases collide,
    // prefer an existing canonical alias, then the first old key in lexical order.
    connection.execute(
        "INSERT OR IGNORE INTO dedupe_aliases(mode,key,alias)
         SELECT mode,style_match_key(key,?1),alias FROM dedupe_aliases
         WHERE mode='artists' AND style_match_key(key,?1) IS NOT NULL
         ORDER BY CASE WHEN key=style_match_key(key,?1) THEN 0 ELSE 1 END,key",
        [&settings],
    )?;
    let groups_after: u64 = connection.query_row(
        "SELECT COUNT(DISTINCT artist_match_key) FROM rows",
        [],
        |row| row.get::<_, i64>(0).map(|value| value as u64),
    )?;
    let remaining: u64 =
        connection.query_row("SELECT COUNT(*) FROM artist_representatives", [], |row| {
            row.get::<_, i64>(0).map(|value| value as u64)
        })?;
    connection.execute(
        "INSERT INTO settings(key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![RULES_SETTING, rules],
    )?;
    Ok(ArtistMatchingRefresh {
        rows,
        changed_rows,
        groups_before,
        groups_after,
        merged_representatives: (representatives.len() as u64).saturating_sub(remaining),
    })
}
