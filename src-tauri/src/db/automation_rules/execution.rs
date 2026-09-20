use super::repository::validate_group_targets;
use crate::automation::actions::{
    note_sequence_prefixes, parse_note_sequence_number, simulate_actions,
};
use crate::automation::error::AutomationRuleError;
use crate::automation::matching::PreparedConditionSet;
use crate::automation::model::draft_from_rule;
use crate::automation::model::{
    AutomationRule, AutomationRuleDraft, RuleAction, RuleExecutionReport, RuleExecutionSummary,
    RuleExecutionTrigger, RulePreview, RuleRow,
};
use crate::automation::text::normalized_strings;
use crate::automation::validation::validate_draft;
use crate::db::{Database, DatabaseError};
use rusqlite::{Connection, Transaction, TransactionBehavior, params};
use std::collections::{HashMap, HashSet};

pub(crate) const CANDIDATE_TABLE: &str = "temp.automation_rule_candidates";

pub(crate) const SAMPLE_LIMIT: usize = 12;

pub(crate) struct RuleApplyOutcome {
    pub(crate) report: RuleExecutionReport,
    pub(crate) changed_row_ids: HashSet<i64>,
    pub(crate) stopped_row_ids: HashSet<i64>,
}

impl Database {
    pub fn preview_automation_rule(&self, id: i64) -> Result<RulePreview, AutomationRuleError> {
        let rule = self.automation_rule(id)?;
        self.preview_rule_draft_internal(&draft_from_rule(&rule))
    }

    pub fn preview_automation_rule_draft(
        &self,
        draft: &AutomationRuleDraft,
    ) -> Result<RulePreview, AutomationRuleError> {
        self.preview_rule_draft_internal(draft)
    }

    pub(super) fn preview_rule_draft_internal(
        &self,
        draft: &AutomationRuleDraft,
    ) -> Result<RulePreview, AutomationRuleError> {
        validate_draft(draft)?;
        validate_group_targets(&self.connection, &draft.actions)?;
        let prepared = PreparedConditionSet::new(&draft.conditions)?;
        let rows = load_rule_rows(&self.connection, None)?;
        let mut sequence_counters =
            seed_note_sequence_counters(&self.connection, &note_sequence_prefixes(&draft.actions))?;
        let mut matched = Vec::new();
        let mut needing_changes = 0_u64;
        let mut stopped = 0_u64;
        for row in &rows {
            if !prepared.matches(row) {
                continue;
            }
            matched.push(row.id);
            let (_, changed_actions, should_stop) =
                simulate_actions(row, &draft.actions, &mut sequence_counters)?;
            if changed_actions > 0 {
                needing_changes += 1;
            }
            if should_stop {
                stopped += 1;
            }
        }
        Ok(RulePreview {
            scanned_rows: u64::try_from(rows.len()).map_err(|_| DatabaseError::CountOverflow)?,
            matched_rows: u64::try_from(matched.len()).map_err(|_| DatabaseError::CountOverflow)?,
            rows_needing_changes: needing_changes,
            stopped_rows: stopped,
            sample_row_ids: matched.into_iter().take(SAMPLE_LIMIT).collect(),
        })
    }

    pub fn run_automation_rule_on_library(
        &mut self,
        id: i64,
    ) -> Result<RuleExecutionSummary, AutomationRuleError> {
        let rule = self.automation_rule(id)?;
        let row_ids = self.all_row_ids()?;
        let outcome = self.apply_rule(&rule, &row_ids)?;
        let changed_rows = u64::try_from(outcome.changed_row_ids.len())
            .map_err(|_| DatabaseError::CountOverflow)?;
        if changed_rows > 0 {
            self.bump_data_version();
        }
        Ok(RuleExecutionSummary {
            trigger: RuleExecutionTrigger::Manual,
            input_rows: u64::try_from(row_ids.len()).map_err(|_| DatabaseError::CountOverflow)?,
            changed_rows,
            reports: vec![outcome.report],
            engine_error: None,
        })
    }

    pub fn execute_automation_rules(
        &mut self,
        trigger: RuleExecutionTrigger,
        row_ids: &[i64],
    ) -> Result<RuleExecutionSummary, AutomationRuleError> {
        let rules = self
            .list_automation_rules()?
            .into_iter()
            .filter(|rule| {
                rule.enabled
                    && match trigger {
                        RuleExecutionTrigger::Import => rule.run_on_import,
                        RuleExecutionTrigger::Update => rule.run_on_update,
                        RuleExecutionTrigger::Manual => true,
                    }
            })
            .collect::<Vec<_>>();
        let mut active = row_ids.iter().copied().collect::<HashSet<_>>();
        let mut changed = HashSet::new();
        let mut reports = Vec::new();
        for rule in rules {
            if active.is_empty() {
                break;
            }
            let candidates = active.iter().copied().collect::<Vec<_>>();
            match self.apply_rule(&rule, &candidates) {
                Ok(outcome) => {
                    changed.extend(outcome.changed_row_ids);
                    for row_id in &outcome.stopped_row_ids {
                        active.remove(row_id);
                    }
                    reports.push(outcome.report);
                }
                Err(error) => reports.push(RuleExecutionReport {
                    rule_id: rule.id,
                    rule_name: rule.name,
                    scanned_rows: u64::try_from(candidates.len()).unwrap_or(u64::MAX),
                    matched_rows: 0,
                    changed_rows: 0,
                    actions_changed: 0,
                    stopped_rows: 0,
                    error: Some(error.to_string()),
                }),
            }
        }
        if !changed.is_empty() {
            self.bump_data_version();
        }
        Ok(RuleExecutionSummary {
            trigger,
            input_rows: u64::try_from(row_ids.len()).map_err(|_| DatabaseError::CountOverflow)?,
            changed_rows: u64::try_from(changed.len()).map_err(|_| DatabaseError::CountOverflow)?,
            reports,
            engine_error: None,
        })
    }

    pub(super) fn all_row_ids(&self) -> Result<Vec<i64>, AutomationRuleError> {
        let mut statement = self.connection.prepare("SELECT id FROM rows ORDER BY id")?;
        Ok(statement
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub(super) fn apply_rule(
        &mut self,
        rule: &AutomationRule,
        row_ids: &[i64],
    ) -> Result<RuleApplyOutcome, AutomationRuleError> {
        validate_draft(&draft_from_rule(rule))?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_group_targets(&transaction, &rule.actions)?;
        ensure_action_tags(&transaction, &rule.actions)?;
        let prepared = PreparedConditionSet::new(&rule.conditions)?;
        let rows = load_rule_rows(&transaction, Some(row_ids))?;
        let scanned_rows = u64::try_from(rows.len()).map_err(|_| DatabaseError::CountOverflow)?;
        let mut sequence_counters =
            seed_note_sequence_counters(&transaction, &note_sequence_prefixes(&rule.actions))?;
        let mut matched_rows = 0_u64;
        let mut actions_changed = 0_u64;
        let mut changed_row_ids = HashSet::new();
        let mut stopped_row_ids = HashSet::new();
        for row in rows {
            if !prepared.matches(&row) {
                continue;
            }
            matched_rows += 1;
            let (updated, changed_actions, should_stop) =
                simulate_actions(&row, &rule.actions, &mut sequence_counters)?;
            if changed_actions > 0 {
                persist_rule_row(&transaction, &row, &updated)?;
                changed_row_ids.insert(row.id);
                actions_changed += changed_actions;
            }
            if should_stop {
                stopped_row_ids.insert(row.id);
            }
        }
        transaction.commit()?;
        Ok(RuleApplyOutcome {
            report: RuleExecutionReport {
                rule_id: rule.id,
                rule_name: rule.name.clone(),
                scanned_rows,
                matched_rows,
                changed_rows: u64::try_from(changed_row_ids.len())
                    .map_err(|_| DatabaseError::CountOverflow)?,
                actions_changed,
                stopped_rows: u64::try_from(stopped_row_ids.len())
                    .map_err(|_| DatabaseError::CountOverflow)?,
                error: None,
            },
            changed_row_ids,
            stopped_row_ids,
        })
    }
}

pub(crate) fn ensure_action_tags(
    transaction: &Transaction<'_>,
    actions: &[RuleAction],
) -> Result<(), AutomationRuleError> {
    let tags = actions
        .iter()
        .filter_map(|action| match action {
            RuleAction::AddTags { tags } => Some(tags.as_slice()),
            _ => None,
        })
        .flat_map(normalized_strings)
        .collect::<HashSet<_>>();
    for tag in tags {
        transaction.execute("INSERT OR IGNORE INTO tags(name) VALUES (?1)", [&tag])?;
    }
    Ok(())
}

pub(crate) fn load_rule_rows(
    connection: &Connection,
    candidate_ids: Option<&[i64]>,
) -> Result<Vec<RuleRow>, AutomationRuleError> {
    connection.execute_batch(&format!(
        "DROP TABLE IF EXISTS {CANDIDATE_TABLE};
         CREATE TEMP TABLE {CANDIDATE_TABLE} (id INTEGER PRIMARY KEY) STRICT;"
    ))?;
    if let Some(ids) = candidate_ids {
        let mut insert = connection.prepare(&format!(
            "INSERT OR IGNORE INTO {CANDIDATE_TABLE}(id) VALUES (?1)"
        ))?;
        for id in ids {
            if *id > 0 {
                insert.execute([id])?;
            }
        }
    } else {
        connection.execute(
            &format!("INSERT INTO {CANDIDATE_TABLE}(id) SELECT id FROM rows"),
            [],
        )?;
    }

    let mut rows = {
        let mut statement = connection.prepare(&format!(
            "SELECT rows.id, rows.positive_prompt, rows.character_prompt, rows.negative_prompt,
                    rows.artists, rows.note, rows.group_id, rows.image_path, rows.source_size,
                    rows.metadata_failed, COALESCE(rows.vibe_reference_count, 0),
                    rows.image_width, rows.image_height, rows.generation_model,
                    rows.generation_sampler, rows.generation_steps, rows.generation_seed,
                    rows.generation_scale, rows.generation_cfg_rescale,
                    rows.generation_noise_schedule, import_batches.source_type,
                    import_batches.source_path
             FROM {CANDIDATE_TABLE} AS candidates
             JOIN rows ON rows.id = candidates.id
             JOIN import_batches ON import_batches.id = rows.batch_id
             ORDER BY rows.id"
        ))?;
        statement
            .query_map([], |row| {
                Ok(RuleRow {
                    id: row.get(0)?,
                    positive_prompt: row.get(1)?,
                    character_prompt: row.get(2)?,
                    negative_prompt: row.get(3)?,
                    artists: row.get(4)?,
                    note: row.get(5)?,
                    group_id: row.get(6)?,
                    tags: HashSet::new(),
                    image_path: row.get(7)?,
                    source_size: row.get(8)?,
                    metadata_failed: row.get(9)?,
                    vibe_count: row.get(10)?,
                    image_width: row.get(11)?,
                    image_height: row.get(12)?,
                    generation_model: row.get(13)?,
                    generation_sampler: row.get(14)?,
                    generation_steps: row.get(15)?,
                    generation_seed: row.get(16)?,
                    generation_scale: row.get(17)?,
                    generation_cfg_rescale: row.get(18)?,
                    generation_noise_schedule: row.get(19)?,
                    source_type: row.get(20)?,
                    source_path: row.get(21)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    let row_indices = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.id, index))
        .collect::<HashMap<_, _>>();
    {
        let mut statement = connection.prepare(&format!(
            "SELECT row_tags.row_id, tags.name
             FROM {CANDIDATE_TABLE} AS candidates
             JOIN row_tags ON row_tags.row_id = candidates.id
             JOIN tags ON tags.id = row_tags.tag_id"
        ))?;
        let pairs = statement
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (row_id, tag) in pairs {
            if let Some(index) = row_indices.get(&row_id) {
                rows[*index].tags.insert(tag);
            }
        }
    }
    connection.execute_batch(&format!("DROP TABLE {CANDIDATE_TABLE};"))?;
    Ok(rows)
}

pub(crate) fn persist_rule_row(
    transaction: &Transaction<'_>,
    original: &RuleRow,
    updated: &RuleRow,
) -> Result<(), AutomationRuleError> {
    if original.positive_prompt != updated.positive_prompt
        || original.character_prompt != updated.character_prompt
        || original.negative_prompt != updated.negative_prompt
        || original.artists != updated.artists
        || original.note != updated.note
        || original.group_id != updated.group_id
    {
        transaction.execute(
            "UPDATE rows SET positive_prompt = ?2, character_prompt = ?3,
                    negative_prompt = ?4, artists = ?5, note = ?6, group_id = ?7,
                    style_signature = ?8
             WHERE id = ?1",
            params![
                updated.id,
                updated.positive_prompt,
                updated.character_prompt,
                updated.negative_prompt,
                updated.artists,
                updated.note,
                updated.group_id,
                crate::pipeline::style_signature_of(updated.positive_prompt.as_deref()),
            ],
        )?;
    }
    for tag in original.tags.difference(&updated.tags) {
        transaction.execute(
            "DELETE FROM row_tags WHERE row_id = ?1
             AND tag_id = (SELECT id FROM tags WHERE name = ?2 COLLATE BINARY)",
            params![updated.id, tag],
        )?;
    }
    for tag in updated.tags.difference(&original.tags) {
        transaction.execute(
            "INSERT OR IGNORE INTO row_tags(row_id, tag_id)
             SELECT ?1, id FROM tags WHERE name = ?2 COLLATE BINARY",
            params![updated.id, tag],
        )?;
    }
    Ok(())
}

pub(crate) fn seed_note_sequence_counters(
    connection: &Connection,
    prefixes: &[String],
) -> Result<HashMap<String, u64>, AutomationRuleError> {
    let mut counters = HashMap::new();
    if prefixes.is_empty() {
        return Ok(counters);
    }
    let mut statement = connection.prepare("SELECT note FROM rows WHERE note IS NOT NULL")?;
    let notes = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for prefix in prefixes {
        let max = notes
            .iter()
            .filter_map(|note| parse_note_sequence_number(note, prefix))
            .max()
            .unwrap_or(0);
        counters.insert(prefix.clone(), max);
    }
    Ok(counters)
}
