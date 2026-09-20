use crate::automation::error::AutomationRuleError;
use crate::automation::model::{AutomationRule, AutomationRuleDraft, RuleAction};
use crate::automation::validation::validate_draft;
use crate::db::{Database, DatabaseError};
use rusqlite::{Connection, Transaction, TransactionBehavior, params};
use std::collections::HashSet;

pub(crate) fn count_u32(value: usize) -> Result<u32, AutomationRuleError> {
    u32::try_from(value).map_err(|_| DatabaseError::CountOverflow.into())
}

impl Database {
    pub fn list_automation_rules(&self) -> Result<Vec<AutomationRule>, AutomationRuleError> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, description, enabled, position, run_on_import, run_on_update,
                    conditions_json, actions_json, created_at, updated_at
             FROM automation_rules ORDER BY position, id",
        )?;
        let stored = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, bool>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, bool>(5)?,
                    row.get::<_, bool>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        stored
            .into_iter()
            .map(|row| automation_rule_from_stored(row).map_err(AutomationRuleError::from))
            .collect()
    }

    pub fn create_automation_rule(
        &mut self,
        draft: &AutomationRuleDraft,
    ) -> Result<AutomationRule, AutomationRuleError> {
        validate_draft(draft)?;
        validate_group_targets(&self.connection, &draft.actions)?;
        let conditions = serde_json::to_string(&draft.conditions)?;
        let actions = serde_json::to_string(&draft.actions)?;
        let position: u32 = self.connection.query_row(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM automation_rules",
            [],
            |row| row.get(0),
        )?;
        self.connection.execute(
            "INSERT INTO automation_rules
                (name, description, enabled, position, run_on_import, run_on_update,
                 conditions_json, actions_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                draft.name.trim(),
                draft.description.trim(),
                draft.enabled,
                position,
                draft.run_on_import,
                draft.run_on_update,
                conditions,
                actions,
            ],
        )?;
        let id = self.connection.last_insert_rowid();
        self.automation_rule(id)
    }

    pub fn update_automation_rule(
        &mut self,
        id: i64,
        draft: &AutomationRuleDraft,
    ) -> Result<AutomationRule, AutomationRuleError> {
        validate_draft(draft)?;
        validate_group_targets(&self.connection, &draft.actions)?;
        let conditions = serde_json::to_string(&draft.conditions)?;
        let actions = serde_json::to_string(&draft.actions)?;
        let changed = self.connection.execute(
            "UPDATE automation_rules SET
                name = ?2, description = ?3, enabled = ?4,
                run_on_import = ?5, run_on_update = ?6,
                conditions_json = ?7, actions_json = ?8,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1",
            params![
                id,
                draft.name.trim(),
                draft.description.trim(),
                draft.enabled,
                draft.run_on_import,
                draft.run_on_update,
                conditions,
                actions,
            ],
        )?;
        if changed == 0 {
            return Err(AutomationRuleError::RuleNotFound(id));
        }
        self.automation_rule(id)
    }

    pub fn set_automation_rule_enabled(
        &mut self,
        id: i64,
        enabled: bool,
    ) -> Result<(), AutomationRuleError> {
        if self.connection.execute(
            "UPDATE automation_rules SET enabled = ?2,
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?1",
            params![id, enabled],
        )? == 0
        {
            return Err(AutomationRuleError::RuleNotFound(id));
        }
        Ok(())
    }

    pub fn delete_automation_rule(&mut self, id: i64) -> Result<bool, AutomationRuleError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let deleted = transaction.execute("DELETE FROM automation_rules WHERE id = ?1", [id])?;
        if deleted > 0 {
            normalize_rule_positions(&transaction)?;
        }
        transaction.commit()?;
        Ok(deleted > 0)
    }

    pub fn reorder_automation_rules(&mut self, ids: &[i64]) -> Result<(), AutomationRuleError> {
        let existing = self
            .list_automation_rules()?
            .into_iter()
            .map(|rule| rule.id)
            .collect::<HashSet<_>>();
        let requested = ids.iter().copied().collect::<HashSet<_>>();
        if ids.len() != existing.len() || requested.len() != ids.len() || requested != existing {
            return Err(AutomationRuleError::InvalidOrder);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "UPDATE automation_rules SET position = position + 1000000",
            [],
        )?;
        for (position, id) in ids.iter().enumerate() {
            transaction.execute(
                "UPDATE automation_rules SET position = ?2,
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?1",
                params![
                    id,
                    i64::try_from(position).map_err(|_| DatabaseError::CountOverflow)?
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub(super) fn automation_rule(&self, id: i64) -> Result<AutomationRule, AutomationRuleError> {
        self.list_automation_rules()?
            .into_iter()
            .find(|rule| rule.id == id)
            .ok_or(AutomationRuleError::RuleNotFound(id))
    }
}

pub(crate) fn automation_rule_from_stored(
    stored: (
        i64,
        String,
        String,
        bool,
        u32,
        bool,
        bool,
        String,
        String,
        String,
        String,
    ),
) -> Result<AutomationRule, serde_json::Error> {
    let (
        id,
        name,
        description,
        enabled,
        position,
        run_on_import,
        run_on_update,
        conditions,
        actions,
        created_at,
        updated_at,
    ) = stored;
    Ok(AutomationRule {
        id,
        name,
        description,
        enabled,
        position,
        run_on_import,
        run_on_update,
        conditions: serde_json::from_str(&conditions)?,
        actions: serde_json::from_str(&actions)?,
        created_at,
        updated_at,
    })
}

pub(crate) fn normalize_rule_positions(
    transaction: &Transaction<'_>,
) -> Result<(), rusqlite::Error> {
    let ids = {
        let mut statement =
            transaction.prepare("SELECT id FROM automation_rules ORDER BY position, id")?;
        statement
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?
    };
    transaction.execute(
        "UPDATE automation_rules SET position = position + 1000000",
        [],
    )?;
    for (position, id) in ids.into_iter().enumerate() {
        transaction.execute(
            "UPDATE automation_rules SET position = ?2 WHERE id = ?1",
            params![id, i64::try_from(position).unwrap_or(i64::MAX)],
        )?;
    }
    Ok(())
}

pub(crate) fn validate_group_targets(
    connection: &Connection,
    actions: &[RuleAction],
) -> Result<(), AutomationRuleError> {
    for action in actions {
        let RuleAction::SetGroup { group_id, .. } = action else {
            continue;
        };
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM groups WHERE id = ?1)",
            [group_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(AutomationRuleError::MissingTargetGroup(*group_id));
        }
    }
    Ok(())
}
