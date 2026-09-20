//! Strict text matching and transactional tag, group and artist-prefix operations.
mod artist_prefix;
mod groups;
mod matching;
mod model;
mod tags;
#[cfg(test)]
mod tests;

pub use model::*;

use crate::db::DatabaseError;
use rusqlite::Connection;

const PREVIEW_SAMPLE_LIMIT: usize = 12;

fn row_count(connection: &Connection) -> Result<u64, QuickEditError> {
    let count: i64 = connection.query_row("SELECT COUNT(*) FROM rows", [], |row| row.get(0))?;
    u64::try_from(count).map_err(|_| DatabaseError::CountOverflow.into())
}
