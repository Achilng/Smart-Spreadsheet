use super::model::{QuickEditCondition, QuickEditError, QuickEditTextField};
use rusqlite::Connection;
use std::collections::HashSet;

#[derive(Debug)]
pub(super) struct EvaluatedCondition {
    fields: HashSet<QuickEditTextField>,
    pub(super) tokens: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct MatchingRow {
    pub(super) id: i64,
    pub(super) group_id: Option<i64>,
}
impl EvaluatedCondition {
    pub(super) fn new(condition: &QuickEditCondition) -> Result<Self, QuickEditError> {
        let fields = condition.fields.iter().copied().collect::<HashSet<_>>();
        if fields.is_empty() {
            return Err(QuickEditError::EmptyFields);
        }

        let mut seen = HashSet::with_capacity(condition.required_tokens.len());
        let mut tokens = Vec::with_capacity(condition.required_tokens.len());
        for raw in &condition.required_tokens {
            if raw.contains([',', '\n', '\r']) {
                return Err(QuickEditError::InvalidConditionToken(raw.clone()));
            }
            let token = normalize_prompt_token(raw);
            if !token.is_empty() && seen.insert(token.clone()) {
                tokens.push(token);
            }
        }
        if tokens.is_empty() {
            return Err(QuickEditError::EmptyCondition);
        }

        Ok(Self { fields, tokens })
    }

    fn matches(
        &self,
        positive: Option<&str>,
        character: Option<&str>,
        negative: Option<&str>,
        artists: Option<&str>,
        note: Option<&str>,
    ) -> bool {
        let mut available = HashSet::new();
        if self.fields.contains(&QuickEditTextField::PositivePrompt) {
            collect_prompt_tokens(positive, &mut available);
        }
        if self.fields.contains(&QuickEditTextField::CharacterPrompt) {
            collect_prompt_tokens(character, &mut available);
        }
        if self.fields.contains(&QuickEditTextField::NegativePrompt) {
            collect_prompt_tokens(negative, &mut available);
        }
        if self.fields.contains(&QuickEditTextField::Artists) {
            collect_prompt_tokens(artists, &mut available);
        }
        if self.fields.contains(&QuickEditTextField::Note) {
            collect_prompt_tokens(note, &mut available);
        }
        self.tokens.iter().all(|token| available.contains(token))
    }
}
pub(super) fn matching_rows(
    connection: &Connection,
    condition: &EvaluatedCondition,
) -> Result<Vec<MatchingRow>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT id, positive_prompt, character_prompt, negative_prompt, artists, note, group_id
         FROM rows
         ORDER BY id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<i64>>(6)?,
        ))
    })?;
    let mut matched = Vec::new();
    for row in rows {
        let (row_id, positive, character, negative, artists, note, group_id) = row?;
        if condition.matches(
            positive.as_deref(),
            character.as_deref(),
            negative.as_deref(),
            artists.as_deref(),
            note.as_deref(),
        ) {
            matched.push(MatchingRow {
                id: row_id,
                group_id,
            });
        }
    }
    Ok(matched)
}
fn collect_prompt_tokens(prompt: Option<&str>, output: &mut HashSet<String>) {
    let Some(prompt) = prompt else {
        return;
    };
    for fragment in prompt.split([',', '\n', '\r']) {
        let token = normalize_prompt_token(fragment);
        if !token.is_empty() {
            output.insert(token);
        }
    }
}

/// 严格提示词匹配会规范化大小写、常见 NovelAI 权重外壳和少量明确的泛用别名。
/// 除别名表外，内部空格、下划线及其它字符保持不变。
pub(super) fn normalize_prompt_token(raw: &str) -> String {
    let normalized = crate::pipeline::prompt_text::normalize_prompt_token(raw);
    match normalized.as_str() {
        "girl" | "1girl" | "1 girl" => "girl".to_owned(),
        _ => normalized,
    }
}
