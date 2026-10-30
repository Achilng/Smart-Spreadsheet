use rusqlite::{TransactionBehavior, params};

use super::tags::{
    RowSelection, TARGET_ROWS_TABLE, TagMutationError, create_selection_rows, drop_selection_tables,
};
use super::{Database, DatabaseError};

impl Database {
    pub fn set_selection_favorite(
        &mut self,
        selection: &RowSelection,
        favorite: bool,
    ) -> Result<u64, TagMutationError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        create_selection_rows(&transaction, selection)?;
        let updated = transaction.execute(
            &format!("UPDATE rows SET favorite = ?1 WHERE favorite != ?1 AND id IN (SELECT id FROM {TARGET_ROWS_TABLE})"),
            [favorite],
        )?;
        drop_selection_tables(&transaction)?;
        transaction.commit()?;
        if updated > 0 {
            self.bump_data_version();
        }
        Ok(updated as u64)
    }

    pub fn set_favorite(&mut self, row_id: i64, favorite: bool) -> Result<u64, DatabaseError> {
        let updated = self.connection.execute(
            "UPDATE rows SET favorite = ?2 WHERE id = ?1",
            params![row_id, favorite],
        )?;
        if updated > 0 {
            self.bump_data_version();
        }
        Ok(updated as u64)
    }
}
