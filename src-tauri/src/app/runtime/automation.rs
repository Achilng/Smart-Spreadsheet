use super::{AppRuntime, AppRuntimeError};
use crate::db::{
    AutomationRule, AutomationRuleDraft, AutomationRuleError, AutomationRuleExportResult,
    AutomationRuleImportInspection, AutomationRuleImportResult, RuleExecutionSummary, RulePreview,
    parse_automation_rule_text, read_automation_rule_file, write_automation_rule_file,
};
use std::path::Path;

impl AppRuntime {
    pub(crate) fn list_automation_rules(&self) -> Result<Vec<AutomationRule>, AppRuntimeError> {
        self.with_database(|db| db.list_automation_rules())
    }

    pub(crate) fn inspect_automation_rule_file(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<AutomationRuleImportInspection, AppRuntimeError> {
        let (document, content_hash) = read_automation_rule_file(path.as_ref())?;
        self.with_database(|db| db.inspect_automation_rule_document(&document, content_hash))
    }

    pub(crate) fn import_automation_rule_file(
        &self,
        path: impl AsRef<Path>,
        expected_hash: &str,
    ) -> Result<AutomationRuleImportResult, AppRuntimeError> {
        let (document, content_hash) = read_automation_rule_file(path.as_ref())?;
        if content_hash != expected_hash {
            return Err(AutomationRuleError::InvalidRuleFile(
                "文件在预览后发生了变化，请重新选择并检查".into(),
            )
            .into());
        }
        self.with_database_mut(|db| db.import_automation_rule_document(&document))
    }

    pub(crate) fn inspect_automation_rule_text(
        &self,
        text: &str,
    ) -> Result<AutomationRuleImportInspection, AppRuntimeError> {
        let (document, content_hash) = parse_automation_rule_text(text)?;
        self.with_database(|db| db.inspect_automation_rule_document(&document, content_hash))
    }

    pub(crate) fn import_automation_rule_text(
        &self,
        text: &str,
        expected_hash: &str,
    ) -> Result<AutomationRuleImportResult, AppRuntimeError> {
        let (document, content_hash) = parse_automation_rule_text(text)?;
        if content_hash != expected_hash {
            return Err(AutomationRuleError::InvalidRuleFile(
                "文本在预览后发生了变化，请重新检查".into(),
            )
            .into());
        }
        self.with_database_mut(|db| db.import_automation_rule_document(&document))
    }

    pub(crate) fn export_automation_rules(
        &self,
        path: impl AsRef<Path>,
        ids: &[i64],
    ) -> Result<AutomationRuleExportResult, AppRuntimeError> {
        let path = path.as_ref();
        let document = self.with_database(|db| db.export_automation_rule_document(ids))?;
        write_automation_rule_file(path, &document)?;
        Ok(AutomationRuleExportResult {
            path: path.to_string_lossy().into_owned(),
            exported_rules: u32::try_from(ids.len())
                .map_err(|_| crate::db::DatabaseError::CountOverflow)?,
        })
    }

    pub(crate) fn create_automation_rule(
        &self,
        draft: &AutomationRuleDraft,
    ) -> Result<AutomationRule, AppRuntimeError> {
        self.with_database(|db| db.create_automation_rule(draft))
    }

    pub(crate) fn update_automation_rule(
        &self,
        id: i64,
        draft: &AutomationRuleDraft,
    ) -> Result<AutomationRule, AppRuntimeError> {
        self.with_database(|db| db.update_automation_rule(id, draft))
    }

    pub(crate) fn set_automation_rule_enabled(
        &self,
        id: i64,
        enabled: bool,
    ) -> Result<(), AppRuntimeError> {
        self.with_database(|db| db.set_automation_rule_enabled(id, enabled))
    }

    pub(crate) fn delete_automation_rule(&self, id: i64) -> Result<bool, AppRuntimeError> {
        self.with_database(|db| db.delete_automation_rule(id))
    }

    pub(crate) fn reorder_automation_rules(&self, ids: &[i64]) -> Result<(), AppRuntimeError> {
        self.with_database(|db| db.reorder_automation_rules(ids))
    }

    pub(crate) fn preview_automation_rule(&self, id: i64) -> Result<RulePreview, AppRuntimeError> {
        self.with_cloned_database(|db| db.preview_automation_rule(id))
    }

    pub(crate) fn preview_automation_rule_draft(
        &self,
        draft: &AutomationRuleDraft,
    ) -> Result<RulePreview, AppRuntimeError> {
        self.with_cloned_database(|db| db.preview_automation_rule_draft(draft))
    }

    pub(crate) fn run_automation_rule_on_library(
        &self,
        id: i64,
    ) -> Result<RuleExecutionSummary, AppRuntimeError> {
        self.with_cloned_database_mut(|db| db.run_automation_rule_on_library(id))
    }
}
