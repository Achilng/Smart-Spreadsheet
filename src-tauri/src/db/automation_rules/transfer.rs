use crate::automation::error::AutomationRuleError;
use crate::automation::model::{
    AutomationRuleDraft, AutomationRuleImportInspection, AutomationRuleImportPreview,
    AutomationRuleImportResult, RuleAction, RuleCondition,
};
use crate::automation::text::normalized_strings;
use crate::automation::validation::validate_draft;
use crate::db::automation_rules::execution::validate_group_targets;
use crate::db::automation_rules::repository::{count_u32, draft_from_rule};
use crate::db::{Database, DatabaseError};
use rusqlite::{Connection, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub(crate) const MAX_IMPORTED_RULE_NAME_CHARS: usize = 120;

pub(crate) const MAX_IMPORTED_DEPENDENCIES: usize = 1_000;

pub(crate) const MAX_IMPORTED_RULES: usize = 200;

pub(crate) const AUTOMATION_RULE_FILE_VERSION: u32 = 1;

pub(crate) const AUTOMATION_RULE_FILE_FORMAT: &str = "smart-spreadsheet.automation-rules";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AutomationRuleTransferFile {
    pub(crate) format: String,
    pub(crate) version: u32,
    pub(crate) rules: Vec<Value>,
}

#[derive(Debug)]
pub(crate) struct PreparedRuleImport {
    pub(crate) portable_rules: Vec<Value>,
    pub(crate) final_names: Vec<String>,
    pub(crate) previews: Vec<AutomationRuleImportPreview>,
    pub(crate) missing_tags: Vec<String>,
    pub(crate) missing_groups: Vec<String>,
    pub(crate) renamed_rules: u32,
}

impl Database {
    pub fn export_automation_rule_document(
        &self,
        ids: &[i64],
    ) -> Result<Value, AutomationRuleError> {
        if ids.is_empty() {
            return Err(AutomationRuleError::InvalidRuleFile(
                "至少选择一条已保存的规则才能导出".into(),
            ));
        }
        let requested = ids.iter().copied().collect::<HashSet<_>>();
        if requested.len() != ids.len() {
            return Err(AutomationRuleError::InvalidRuleFile(
                "导出范围包含重复的规则".into(),
            ));
        }
        let selected = self
            .list_automation_rules()?
            .into_iter()
            .filter(|rule| requested.contains(&rule.id))
            .collect::<Vec<_>>();
        if selected.len() != requested.len() {
            return Err(AutomationRuleError::InvalidRuleFile(
                "导出范围包含已不存在的规则".into(),
            ));
        }
        let group_names = query_group_names_by_id(&self.connection)?;
        let mut rules = Vec::with_capacity(selected.len());
        for rule in selected {
            let mut value = serde_json::to_value(draft_from_rule(&rule))?;
            replace_group_ids_with_names(&mut value, &group_names)?;
            rules.push(value);
        }
        Ok(serde_json::to_value(AutomationRuleTransferFile {
            format: AUTOMATION_RULE_FILE_FORMAT.into(),
            version: AUTOMATION_RULE_FILE_VERSION,
            rules,
        })?)
    }

    pub fn inspect_automation_rule_document(
        &self,
        document: &Value,
        content_hash: String,
    ) -> Result<AutomationRuleImportInspection, AutomationRuleError> {
        let prepared = prepare_rule_import(&self.connection, document)?;
        Ok(AutomationRuleImportInspection {
            content_hash,
            version: AUTOMATION_RULE_FILE_VERSION,
            rule_count: count_u32(prepared.portable_rules.len())?,
            rules: prepared.previews,
            missing_tags: prepared.missing_tags,
            missing_groups: prepared.missing_groups,
            renamed_rules: prepared.renamed_rules,
        })
    }

    pub fn import_automation_rule_document(
        &mut self,
        document: &Value,
    ) -> Result<AutomationRuleImportResult, AutomationRuleError> {
        let prepared = prepare_rule_import(&self.connection, document)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        let mut created_groups = 0_u32;
        for name in &prepared.missing_groups {
            created_groups = created_groups
                .checked_add(count_u32(transaction.execute(
                    "INSERT OR IGNORE INTO groups(name) VALUES (?1)",
                    [name],
                )?)?)
                .ok_or(DatabaseError::CountOverflow)?;
        }
        let mut created_tags = 0_u32;
        for name in &prepared.missing_tags {
            created_tags = created_tags
                .checked_add(count_u32(
                    transaction.execute("INSERT OR IGNORE INTO tags(name) VALUES (?1)", [name])?,
                )?)
                .ok_or(DatabaseError::CountOverflow)?;
        }

        let group_ids = query_group_ids_by_name(&transaction)?;
        let mut position: u32 = transaction.query_row(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM automation_rules",
            [],
            |row| row.get(0),
        )?;
        let mut imported_rule_ids = Vec::with_capacity(prepared.portable_rules.len());
        for (portable, final_name) in prepared
            .portable_rules
            .iter()
            .zip(prepared.final_names.iter())
        {
            let mut value = portable.clone();
            replace_group_names_with_ids(&mut value, &group_ids, false, &mut HashSet::new())?;
            let mut draft: AutomationRuleDraft = serde_json::from_value(value)?;
            normalize_draft_lists(&mut draft);
            draft.name = final_name.clone();
            draft.enabled = false;
            validate_draft(&draft)?;
            validate_group_targets(&transaction, &draft.actions)?;
            let conditions = serde_json::to_string(&draft.conditions)?;
            let actions = serde_json::to_string(&draft.actions)?;
            transaction.execute(
                "INSERT INTO automation_rules
                    (name, description, enabled, position, run_on_import, run_on_update,
                     conditions_json, actions_json)
                 VALUES (?1, ?2, 0, ?3, ?4, ?5, ?6, ?7)",
                params![
                    draft.name.trim(),
                    draft.description.trim(),
                    position,
                    draft.run_on_import,
                    draft.run_on_update,
                    conditions,
                    actions,
                ],
            )?;
            imported_rule_ids.push(transaction.last_insert_rowid());
            position = position
                .checked_add(1)
                .ok_or(DatabaseError::CountOverflow)?;
        }
        transaction.commit()?;
        Ok(AutomationRuleImportResult {
            imported_rules: count_u32(imported_rule_ids.len())?,
            created_tags,
            created_groups,
            renamed_rules: prepared.renamed_rules,
            imported_rule_ids,
        })
    }
}

pub(crate) fn prepare_rule_import(
    connection: &Connection,
    document: &Value,
) -> Result<PreparedRuleImport, AutomationRuleError> {
    let transfer: AutomationRuleTransferFile =
        serde_json::from_value(document.clone()).map_err(|error| {
            AutomationRuleError::InvalidRuleFile(format!(
                "这不是智能表格导出的规则文件，或文件结构已损坏：{error}"
            ))
        })?;
    if transfer.format != AUTOMATION_RULE_FILE_FORMAT {
        return Err(AutomationRuleError::InvalidRuleFile(
            "这不是智能表格导出的规则文件".into(),
        ));
    }
    if transfer.version != AUTOMATION_RULE_FILE_VERSION {
        return Err(AutomationRuleError::InvalidRuleFile(format!(
            "文件版本为 {}，当前仅支持版本 {}",
            transfer.version, AUTOMATION_RULE_FILE_VERSION
        )));
    }
    if transfer.rules.is_empty() {
        return Err(AutomationRuleError::InvalidRuleFile(
            "文件中没有规则".into(),
        ));
    }
    if transfer.rules.len() > MAX_IMPORTED_RULES {
        return Err(AutomationRuleError::InvalidRuleFile(format!(
            "文件包含 {} 条规则，超过单次最多 {} 条的限制",
            transfer.rules.len(),
            MAX_IMPORTED_RULES
        )));
    }

    let group_ids = query_group_ids_by_name(connection)?;
    let existing_tags = query_name_set(connection, "SELECT name FROM tags")?;
    let mut used_rule_names = query_name_set(connection, "SELECT name FROM automation_rules")?
        .into_iter()
        .map(|name| name.to_lowercase())
        .collect::<HashSet<_>>();
    let mut referenced_tags = HashSet::new();
    let mut referenced_groups = HashSet::new();
    let mut portable_rules = Vec::with_capacity(transfer.rules.len());
    let mut final_names = Vec::with_capacity(transfer.rules.len());
    let mut previews = Vec::with_capacity(transfer.rules.len());
    let mut renamed_rules = 0_u32;

    for (index, portable) in transfer.rules.into_iter().enumerate() {
        let mut value = portable.clone();
        replace_group_names_with_ids(&mut value, &group_ids, true, &mut referenced_groups)?;
        let mut draft: AutomationRuleDraft = serde_json::from_value(value).map_err(|error| {
            AutomationRuleError::InvalidRuleFile(format!(
                "第 {} 条规则结构无效：{error}",
                index + 1
            ))
        })?;
        normalize_draft_lists(&mut draft);
        let name = draft.name.trim();
        if name.chars().count() > MAX_IMPORTED_RULE_NAME_CHARS {
            return Err(AutomationRuleError::InvalidRuleFile(format!(
                "第 {} 条规则名称超过 {} 个字符",
                index + 1,
                MAX_IMPORTED_RULE_NAME_CHARS
            )));
        }
        if name.chars().any(char::is_control) {
            return Err(AutomationRuleError::InvalidRuleFile(format!(
                "第 {} 条规则名称包含不可见控制字符",
                index + 1
            )));
        }
        draft.name = name.to_owned();
        draft.enabled = false;
        validate_draft(&draft).map_err(|error| {
            AutomationRuleError::InvalidRuleFile(format!(
                "第 {} 条规则“{}”无效：{error}",
                index + 1,
                draft.name
            ))
        })?;
        collect_referenced_tags(&draft, &mut referenced_tags)?;
        let (imported_name, renamed) = unique_imported_rule_name(&draft.name, &mut used_rule_names);
        if renamed {
            renamed_rules = renamed_rules
                .checked_add(1)
                .ok_or(DatabaseError::CountOverflow)?;
        }
        previews.push(AutomationRuleImportPreview {
            name: draft.name.clone(),
            imported_name: imported_name.clone(),
            condition_count: count_u32(
                draft
                    .conditions
                    .groups
                    .iter()
                    .map(|group| group.conditions.len())
                    .sum(),
            )?,
            action_count: count_u32(draft.actions.len())?,
            run_on_import: draft.run_on_import,
            run_on_update: draft.run_on_update,
        });
        portable_rules.push(portable);
        final_names.push(imported_name);
    }

    if referenced_tags.len() + referenced_groups.len() > MAX_IMPORTED_DEPENDENCIES {
        return Err(AutomationRuleError::InvalidRuleFile(format!(
            "规则引用的 Tag 与分组合计超过 {} 个",
            MAX_IMPORTED_DEPENDENCIES
        )));
    }
    let mut missing_tags = referenced_tags
        .into_iter()
        .filter(|name| !existing_tags.contains(name))
        .collect::<Vec<_>>();
    let mut missing_groups = referenced_groups
        .into_iter()
        .filter(|name| !group_ids.contains_key(name))
        .collect::<Vec<_>>();
    missing_tags.sort();
    missing_groups.sort();

    Ok(PreparedRuleImport {
        portable_rules,
        final_names,
        previews,
        missing_tags,
        missing_groups,
        renamed_rules,
    })
}

pub(crate) fn query_name_set(
    connection: &Connection,
    sql: &str,
) -> Result<HashSet<String>, AutomationRuleError> {
    let mut statement = connection.prepare(sql)?;
    Ok(statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<HashSet<_>, _>>()?)
}

pub(crate) fn query_group_names_by_id(
    connection: &Connection,
) -> Result<HashMap<i64, String>, AutomationRuleError> {
    let mut statement = connection.prepare("SELECT id, name FROM groups")?;
    Ok(statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<HashMap<_, _>, _>>()?)
}

pub(crate) fn query_group_ids_by_name(
    connection: &Connection,
) -> Result<HashMap<String, i64>, AutomationRuleError> {
    Ok(query_group_names_by_id(connection)?
        .into_iter()
        .map(|(id, name)| (name, id))
        .collect())
}

pub(crate) fn replace_group_ids_with_names(
    value: &mut Value,
    group_names: &HashMap<i64, String>,
) -> Result<(), AutomationRuleError> {
    match value {
        Value::Array(items) => {
            for item in items {
                replace_group_ids_with_names(item, group_names)?;
            }
        }
        Value::Object(object) => {
            rename_internal_keys_for_export(object)?;
            let kind = object
                .get("type")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if matches!(kind.as_deref(), Some("group") | Some("setGroup")) {
                let raw_id = object
                    .remove("group_id")
                    .or_else(|| object.remove("groupId"))
                    .ok_or_else(|| {
                        AutomationRuleError::InvalidDefinition(format!(
                            "分组规则缺少内部 group_id（类型 {}，字段 {:?}）",
                            kind.as_deref().unwrap_or("unknown"),
                            object.keys().collect::<Vec<_>>()
                        ))
                    })?;
                let name = match raw_id.as_i64() {
                    Some(id) => Value::String(
                        group_names
                            .get(&id)
                            .cloned()
                            .ok_or(AutomationRuleError::MissingTargetGroup(id))?,
                    ),
                    None if raw_id.is_null() && kind.as_deref() == Some("group") => Value::Null,
                    _ => {
                        return Err(AutomationRuleError::InvalidDefinition(
                            "分组规则包含无效的内部 groupId".into(),
                        ));
                    }
                };
                object.insert("groupName".into(), name);
            }
            for child in object.values_mut() {
                replace_group_ids_with_names(child, group_names)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn replace_group_names_with_ids(
    value: &mut Value,
    group_ids: &HashMap<String, i64>,
    allow_missing: bool,
    referenced_groups: &mut HashSet<String>,
) -> Result<(), AutomationRuleError> {
    match value {
        Value::Array(items) => {
            for item in items {
                replace_group_names_with_ids(item, group_ids, allow_missing, referenced_groups)?;
            }
        }
        Value::Object(object) => {
            rename_portable_keys_for_import(object)?;
            let kind = object
                .get("type")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if matches!(kind.as_deref(), Some("group") | Some("setGroup")) {
                if object.contains_key("groupId") || object.contains_key("group_id") {
                    return Err(AutomationRuleError::InvalidRuleFile(
                        "规则文件包含不可跨资料库使用的分组 ID".into(),
                    ));
                }
                let raw_name = object.remove("groupName").ok_or_else(|| {
                    AutomationRuleError::InvalidRuleFile("分组规则缺少 groupName".into())
                })?;
                let is_empty_condition = kind.as_deref() == Some("group")
                    && object.get("operator").and_then(Value::as_str) == Some("isEmpty");
                if is_empty_condition {
                    if !raw_name.is_null() {
                        return Err(AutomationRuleError::InvalidRuleFile(
                            "检查未分组图片的条件不应指定分组名称".into(),
                        ));
                    }
                    object.insert("group_id".into(), Value::Null);
                } else {
                    let name = raw_name.as_str().map(str::trim).ok_or_else(|| {
                        AutomationRuleError::InvalidRuleFile("分组名称必须是文本".into())
                    })?;
                    validate_dependency_name(name, "分组")?;
                    referenced_groups.insert(name.to_owned());
                    let id = group_ids
                        .get(name)
                        .copied()
                        .or_else(|| allow_missing.then_some(1));
                    let id = id.ok_or_else(|| {
                        AutomationRuleError::InvalidRuleFile(format!(
                            "本地缺少分组“{name}”，请重新预览后再导入"
                        ))
                    })?;
                    object.insert("group_id".into(), Value::from(id));
                }
            }
            for child in object.values_mut() {
                replace_group_names_with_ids(child, group_ids, allow_missing, referenced_groups)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn rename_internal_keys_for_export(
    object: &mut serde_json::Map<String, Value>,
) -> Result<(), AutomationRuleError> {
    for (internal, portable) in [
        ("case_sensitive", "caseSensitive"),
        ("source_type", "sourceType"),
        ("second_value", "secondValue"),
        ("only_if_ungrouped", "onlyIfUngrouped"),
    ] {
        if let Some(value) = object.remove(internal)
            && object.insert(portable.into(), value).is_some()
        {
            return Err(AutomationRuleError::InvalidDefinition(format!(
                "规则同时包含 {internal} 与 {portable}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn rename_portable_keys_for_import(
    object: &mut serde_json::Map<String, Value>,
) -> Result<(), AutomationRuleError> {
    for (portable, internal) in [
        ("caseSensitive", "case_sensitive"),
        ("sourceType", "source_type"),
        ("secondValue", "second_value"),
        ("onlyIfUngrouped", "only_if_ungrouped"),
    ] {
        if object.contains_key(internal) {
            return Err(AutomationRuleError::InvalidRuleFile(format!(
                "规则文件字段应使用 {portable}，不能使用内部字段 {internal}"
            )));
        }
        if let Some(value) = object.remove(portable) {
            object.insert(internal.into(), value);
        }
    }
    Ok(())
}

pub(crate) fn normalize_draft_lists(draft: &mut AutomationRuleDraft) {
    for group in &mut draft.conditions.groups {
        for condition in &mut group.conditions {
            match condition {
                RuleCondition::Tag { tags, .. } => *tags = normalized_strings(tags),
                RuleCondition::Artist { artists, .. } => *artists = normalized_strings(artists),
                _ => {}
            }
        }
    }
    for action in &mut draft.actions {
        match action {
            RuleAction::AddTags { tags } | RuleAction::RemoveTags { tags } => {
                *tags = normalized_strings(tags);
            }
            RuleAction::PrefixArtist { artists } => *artists = normalized_strings(artists),
            RuleAction::SetNoteSequence { prefix } => *prefix = prefix.trim().to_owned(),
            _ => {}
        }
    }
}

pub(crate) fn collect_referenced_tags(
    draft: &AutomationRuleDraft,
    destination: &mut HashSet<String>,
) -> Result<(), AutomationRuleError> {
    for group in &draft.conditions.groups {
        for condition in &group.conditions {
            if let RuleCondition::Tag { tags, .. } = condition {
                for name in tags {
                    validate_dependency_name(name, "Tag")?;
                    destination.insert(name.clone());
                }
            }
        }
    }
    for action in &draft.actions {
        if let RuleAction::AddTags { tags } | RuleAction::RemoveTags { tags } = action {
            for name in tags {
                validate_dependency_name(name, "Tag")?;
                destination.insert(name.clone());
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_dependency_name(name: &str, kind: &str) -> Result<(), AutomationRuleError> {
    if name.is_empty() || name.chars().count() > MAX_IMPORTED_RULE_NAME_CHARS {
        return Err(AutomationRuleError::InvalidRuleFile(format!(
            "{kind} 名称为空或超过 {MAX_IMPORTED_RULE_NAME_CHARS} 个字符"
        )));
    }
    if name.chars().any(char::is_control) {
        return Err(AutomationRuleError::InvalidRuleFile(format!(
            "{kind} 名称包含不可见控制字符"
        )));
    }
    Ok(())
}

pub(crate) fn unique_imported_rule_name(base: &str, used: &mut HashSet<String>) -> (String, bool) {
    if used.insert(base.to_lowercase()) {
        return (base.to_owned(), false);
    }
    for number in 1_u32.. {
        let suffix = if number == 1 {
            "（导入）".to_owned()
        } else {
            format!("（导入 {number}）")
        };
        let available = MAX_IMPORTED_RULE_NAME_CHARS.saturating_sub(suffix.chars().count());
        let root = base.chars().take(available).collect::<String>();
        let candidate = format!("{root}{suffix}");
        if used.insert(candidate.to_lowercase()) {
            return (candidate, true);
        }
    }
    unreachable!("the imported rule suffix space is unbounded")
}
