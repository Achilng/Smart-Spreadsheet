use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::Serialize;

use super::tags::{RowSelection, TagMutationError, create_selection_rows, drop_selection_tables};
use super::{Database, DatabaseError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupSummary {
    pub id: i64,
    pub name: String,
    pub member_count: u64,
    pub created_at: String,
}

impl Database {
    pub fn create_group(&mut self, name: &str) -> Result<GroupSummary, DatabaseError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(DatabaseError::EmptyGroupName);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute("INSERT INTO groups(name) VALUES (?1)", [name])?;
        let id = transaction.last_insert_rowid();
        let created_at: String = transaction.query_row(
            "SELECT created_at FROM groups WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;
        transaction.commit()?;
        Ok(GroupSummary {
            id,
            name: name.to_owned(),
            member_count: 0,
            created_at,
        })
    }

    pub fn rename_group(&mut self, group_id: i64, new_name: &str) -> Result<GroupSummary, DatabaseError> {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(DatabaseError::EmptyGroupName);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let updated = transaction.execute(
            "UPDATE groups SET name = ?2 WHERE id = ?1",
            params![group_id, new_name],
        )?;
        if updated == 0 {
            return Err(DatabaseError::GroupNotFound(group_id));
        }
        let summary = query_single_group(&transaction, group_id)?;
        transaction.commit()?;
        Ok(summary)
    }

    pub fn delete_group(&mut self, group_id: i64) -> Result<bool, DatabaseError> {
        let deleted = self
            .connection
            .execute("DELETE FROM groups WHERE id = ?1", [group_id])?;
        Ok(deleted > 0)
    }

    pub fn delete_empty_groups(&mut self) -> Result<u64, DatabaseError> {
        let deleted = self.connection.execute(
            "DELETE FROM groups WHERE id NOT IN (SELECT DISTINCT group_id FROM rows WHERE group_id IS NOT NULL)",
            [],
        )?;
        Ok(deleted as u64)
    }

    pub fn list_groups(&self) -> Result<Vec<GroupSummary>, DatabaseError> {
        let mut statement = self.connection.prepare(
            "SELECT g.id, g.name, COUNT(r.id), g.created_at
             FROM groups g
             LEFT JOIN rows r ON r.group_id = g.id
             GROUP BY g.id
             ORDER BY g.name COLLATE BINARY",
        )?;
        let groups = statement
            .query_map([], |row| {
                Ok(GroupSummary {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    member_count: row.get::<_, i64>(2)? as u64,
                    created_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(groups)
    }

    pub fn assign_rows_to_group(
        &mut self,
        selection: &RowSelection,
        group_id: i64,
    ) -> Result<u64, TagMutationError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let group_exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM groups WHERE id = ?1)",
            [group_id],
            |row| row.get(0),
        )?;
        if !group_exists {
            return Err(DatabaseError::GroupNotFound(group_id).into());
        }
        create_selection_rows(&transaction, selection)?;
        let affected = transaction.execute(
            &format!(
                "UPDATE rows SET group_id = ?1 WHERE id IN (SELECT id FROM {})",
                super::tags::TARGET_ROWS_TABLE
            ),
            [group_id],
        )?;
        drop_selection_tables(&transaction)?;
        transaction.commit()?;
        Ok(affected as u64)
    }

    pub fn ungroup_rows(&mut self, selection: &RowSelection) -> Result<u64, TagMutationError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        create_selection_rows(&transaction, selection)?;
        let affected = transaction.execute(
            &format!(
                "UPDATE rows SET group_id = NULL WHERE id IN (SELECT id FROM {})",
                super::tags::TARGET_ROWS_TABLE
            ),
            [],
        )?;
        drop_selection_tables(&transaction)?;
        transaction.commit()?;
        Ok(affected as u64)
    }

    pub fn ungrouped_keys(
        &self,
        mode: crate::pipeline::similarity::SimilarityMode,
    ) -> Result<Vec<(i64, String)>, DatabaseError> {
        let column = match mode {
            crate::pipeline::similarity::SimilarityMode::Artists => "artists",
            crate::pipeline::similarity::SimilarityMode::PositivePrompt => "positive_prompt",
        };
        let mut stmt = self.connection.prepare(&format!(
            "SELECT id, {column} FROM rows WHERE group_id IS NULL AND {column} IS NOT NULL AND TRIM({column}) != ''"
        ))?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<(i64, String)>, _>>()?;
        Ok(rows)
    }
}

fn query_single_group(
    conn: &rusqlite::Connection,
    group_id: i64,
) -> Result<GroupSummary, DatabaseError> {
    conn.query_row(
        "SELECT g.id, g.name, COUNT(r.id), g.created_at
         FROM groups g
         LEFT JOIN rows r ON r.group_id = g.id
         WHERE g.id = ?1
         GROUP BY g.id",
        [group_id],
        |row| {
            Ok(GroupSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                member_count: row.get::<_, i64>(2)? as u64,
                created_at: row.get(3)?,
            })
        },
    )
    .optional()?
    .ok_or(DatabaseError::GroupNotFound(group_id))
}

#[cfg(test)]
mod tests {
    use super::super::test_support::database_with_rows;
    use super::super::tags::RowSelection;

    #[test]
    fn creates_group_and_lists_with_zero_members() {
        let mut db = database_with_rows(3);

        let group = db.create_group("  test group  ").unwrap();

        assert_eq!(group.name, "test group");
        assert_eq!(group.member_count, 0);

        let groups = db.list_groups().unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "test group");
        assert_eq!(groups[0].member_count, 0);
    }

    #[test]
    fn rejects_empty_group_name() {
        let mut db = database_with_rows(1);
        assert!(db.create_group("  ").is_err());
    }

    #[test]
    fn rejects_duplicate_group_name() {
        let mut db = database_with_rows(1);
        db.create_group("dup").unwrap();
        assert!(db.create_group("dup").is_err());
    }

    #[test]
    fn renames_group() {
        let mut db = database_with_rows(1);
        let group = db.create_group("old").unwrap();

        let renamed = db.rename_group(group.id, " new ").unwrap();

        assert_eq!(renamed.name, "new");
        assert_eq!(renamed.id, group.id);
    }

    #[test]
    fn rename_nonexistent_group_fails() {
        let mut db = database_with_rows(1);
        assert!(db.rename_group(999, "x").is_err());
    }

    #[test]
    fn deletes_group_and_ungroups_members() {
        let mut db = database_with_rows(3);
        let group = db.create_group("g").unwrap();
        db.assign_rows_to_group(
            &RowSelection::Explicit { row_ids: vec![1, 2] },
            group.id,
        )
        .unwrap();

        let deleted = db.delete_group(group.id).unwrap();
        assert!(deleted);

        let groups = db.list_groups().unwrap();
        assert!(groups.is_empty());

        let group_id: Option<i64> = db
            .connection
            .query_row("SELECT group_id FROM rows WHERE id = 1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(group_id, None);
    }

    #[test]
    fn assigns_rows_to_group_and_counts_members() {
        let mut db = database_with_rows(5);
        let group = db.create_group("artists A").unwrap();

        let assigned = db
            .assign_rows_to_group(
                &RowSelection::Explicit { row_ids: vec![1, 3, 5] },
                group.id,
            )
            .unwrap();

        assert_eq!(assigned, 3);
        let groups = db.list_groups().unwrap();
        assert_eq!(groups[0].member_count, 3);
    }

    #[test]
    fn assign_to_nonexistent_group_fails() {
        let mut db = database_with_rows(1);
        assert!(db
            .assign_rows_to_group(&RowSelection::Explicit { row_ids: vec![1] }, 999)
            .is_err());
    }

    #[test]
    fn ungroups_rows() {
        let mut db = database_with_rows(3);
        let group = db.create_group("g").unwrap();
        db.assign_rows_to_group(
            &RowSelection::Explicit { row_ids: vec![1, 2, 3] },
            group.id,
        )
        .unwrap();

        let ungrouped = db
            .ungroup_rows(&RowSelection::Explicit { row_ids: vec![2] })
            .unwrap();

        assert_eq!(ungrouped, 1);
        let groups = db.list_groups().unwrap();
        assert_eq!(groups[0].member_count, 2);
    }

    #[test]
    fn reassigns_row_to_different_group() {
        let mut db = database_with_rows(2);
        let g1 = db.create_group("g1").unwrap();
        let g2 = db.create_group("g2").unwrap();
        db.assign_rows_to_group(&RowSelection::Explicit { row_ids: vec![1] }, g1.id)
            .unwrap();

        db.assign_rows_to_group(&RowSelection::Explicit { row_ids: vec![1] }, g2.id)
            .unwrap();

        let groups = db.list_groups().unwrap();
        let g1_summary = groups.iter().find(|g| g.id == g1.id).unwrap();
        let g2_summary = groups.iter().find(|g| g.id == g2.id).unwrap();
        assert_eq!(g1_summary.member_count, 0);
        assert_eq!(g2_summary.member_count, 1);
    }

    #[test]
    fn deletes_empty_groups() {
        let mut db = database_with_rows(2);
        let _g1 = db.create_group("empty").unwrap();
        let g2 = db.create_group("has members").unwrap();
        db.assign_rows_to_group(&RowSelection::Explicit { row_ids: vec![1] }, g2.id)
            .unwrap();

        let deleted = db.delete_empty_groups().unwrap();

        assert_eq!(deleted, 1);
        let groups = db.list_groups().unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, g2.id);
    }
}
