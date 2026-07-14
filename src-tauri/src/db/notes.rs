use rusqlite::{TransactionBehavior, params};

use super::{Database, DatabaseError};

impl Database {
    /// 更新单张图片的用户备注。空白备注统一存为 NULL。
    pub fn update_note(&mut self, row_id: i64, note: &str) -> Result<u64, DatabaseError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let normalized = note.trim();
        let updated = transaction.execute(
            "UPDATE rows SET note = ?2 WHERE id = ?1",
            params![row_id, (!normalized.is_empty()).then_some(normalized)],
        )?;
        transaction.commit()?;
        Ok(updated as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{append_rows, test_rows};
    use super::*;

    #[test]
    fn updates_and_clears_trimmed_note() {
        let mut database = Database::open_in_memory().unwrap();
        append_rows(&mut database, &test_rows(1));

        assert_eq!(database.update_note(1, "  我的预设  ").unwrap(), 1);
        assert_eq!(
            database.get_rows_by_ids(&[1]).unwrap()[0].note.as_deref(),
            Some("我的预设")
        );

        assert_eq!(database.update_note(1, "   ").unwrap(), 1);
        assert_eq!(database.get_rows_by_ids(&[1]).unwrap()[0].note, None);
    }
}
