use crate::automation::error::AutomationRuleError;
use crate::automation::matching::PreparedConditionSet;
use crate::automation::model::{
    ArtistOperator, AutomationRuleDraft, GroupOperator, NoteOperator, NumericComparison,
    NumericOperator, PromptOperator, RuleAction, RuleCondition, TagOperator, VibeOperator,
};
use crate::automation::text::{normalized_strings, parse_prompt_tokens};

pub(crate) fn validate_draft(draft: &AutomationRuleDraft) -> Result<(), AutomationRuleError> {
    if draft.name.trim().is_empty() {
        return Err(AutomationRuleError::EmptyName);
    }
    if draft.conditions.groups.is_empty() {
        return Err(AutomationRuleError::EmptyConditionSet);
    }
    for (index, group) in draft.conditions.groups.iter().enumerate() {
        if group.conditions.is_empty() {
            return Err(AutomationRuleError::EmptyConditionGroup(index + 1));
        }
        for condition in &group.conditions {
            validate_condition(condition)?;
        }
    }
    if draft.actions.is_empty() {
        return Err(AutomationRuleError::EmptyActions);
    }
    for action in &draft.actions {
        validate_action(action)?;
    }
    PreparedConditionSet::new(&draft.conditions)?;
    Ok(())
}

pub(crate) fn validate_condition(condition: &RuleCondition) -> Result<(), AutomationRuleError> {
    match condition {
        RuleCondition::Prompt {
            operator, value, ..
        } => {
            if value.trim().is_empty() {
                return Err(AutomationRuleError::EmptyValue("提示词条件"));
            }
            if matches!(
                operator,
                PromptOperator::ContainsAll
                    | PromptOperator::ContainsAny
                    | PromptOperator::ContainsNone
            ) && parse_prompt_tokens(value).is_empty()
            {
                return Err(AutomationRuleError::EmptyValue("提示词条件"));
            }
        }
        RuleCondition::Tag { operator, tags } => {
            if !matches!(operator, TagOperator::IsEmpty) && normalized_strings(tags).is_empty() {
                return Err(AutomationRuleError::EmptyValue("Tag 条件"));
            }
        }
        RuleCondition::Group { operator, group_id } => {
            if !matches!(operator, GroupOperator::IsEmpty) && group_id.is_none_or(|id| id <= 0) {
                return Err(AutomationRuleError::EmptyValue("分组条件"));
            }
        }
        RuleCondition::Artist { operator, artists } => {
            if matches!(
                operator,
                ArtistOperator::ContainsAny | ArtistOperator::ContainsNone
            ) && normalized_strings(artists).is_empty()
            {
                return Err(AutomationRuleError::EmptyValue("画师条件"));
            }
        }
        RuleCondition::Note {
            operator: NoteOperator::Contains,
            value,
            ..
        }
        | RuleCondition::FileText { value, .. }
        | RuleCondition::GenerationText { value, .. }
            if value.is_empty() =>
        {
            return Err(AutomationRuleError::EmptyValue("文本条件"));
        }
        RuleCondition::FileSize { comparison }
        | RuleCondition::ImageDimension { comparison, .. }
        | RuleCondition::GenerationNumber { comparison, .. } => {
            validate_comparison(comparison)?;
        }
        RuleCondition::Vibe {
            operator: VibeOperator::Count,
            comparison,
        } => validate_comparison(
            comparison
                .as_ref()
                .ok_or(AutomationRuleError::EmptyValue("VIBE 数量条件"))?,
        )?,
        _ => {}
    }
    Ok(())
}

pub(crate) fn validate_action(action: &RuleAction) -> Result<(), AutomationRuleError> {
    match action {
        RuleAction::AddTags { tags } | RuleAction::RemoveTags { tags }
            if normalized_strings(tags).is_empty() =>
        {
            Err(AutomationRuleError::EmptyValue("Tag 任务"))
        }
        RuleAction::SetGroup { group_id, .. } if *group_id <= 0 => {
            Err(AutomationRuleError::EmptyValue("目标分组"))
        }
        RuleAction::AppendPrompt { value, .. } | RuleAction::DeletePromptTags { value, .. }
            if value.trim().is_empty() =>
        {
            Err(AutomationRuleError::EmptyValue("提示词任务"))
        }
        RuleAction::ReplacePrompt { find, .. } if find.is_empty() => {
            Err(AutomationRuleError::EmptyValue("查找内容"))
        }
        RuleAction::PrefixArtist { artists } if normalized_strings(artists).is_empty() => {
            Err(AutomationRuleError::EmptyValue("画师前缀任务"))
        }
        RuleAction::AppendNote { value, .. } if value.is_empty() => {
            Err(AutomationRuleError::EmptyValue("备注任务"))
        }
        RuleAction::SetNoteSequence { prefix } if prefix.trim().is_empty() => {
            Err(AutomationRuleError::EmptyValue("备注编号前缀"))
        }
        _ => Ok(()),
    }
}

pub(crate) fn validate_comparison(
    comparison: &NumericComparison,
) -> Result<(), AutomationRuleError> {
    if !comparison.value.is_finite()
        || comparison
            .second_value
            .is_some_and(|value| !value.is_finite())
    {
        return Err(AutomationRuleError::InvalidDefinition(
            "数值条件必须是有限数字".into(),
        ));
    }
    if comparison.operator == NumericOperator::Between && comparison.second_value.is_none() {
        return Err(AutomationRuleError::MissingRangeEnd);
    }
    Ok(())
}
