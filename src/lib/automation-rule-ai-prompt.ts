import template from "../../AUTOMATION_RULE_AI_PROMPT.md?raw";

const TAGS_MACRO = "{{SMART_SPREADSHEET_TAGS_JSON}}";
const GROUPS_MACRO = "{{SMART_SPREADSHEET_GROUPS_JSON}}";
const MACRO_PATTERN = /\{\{SMART_SPREADSHEET_(?:TAGS|GROUPS)_JSON\}\}/g;

function safeJsonArray(values: readonly string[]): string {
  return JSON.stringify(values).replace(
    /[<>&\u2028\u2029]/g,
    value => `\\u${value.codePointAt(0)?.toString(16).padStart(4, "0")}`,
  );
}

export function buildAutomationRuleAiPrompt(
  tagNames: readonly string[],
  groupNames: readonly string[],
): string {
  for (const macro of [TAGS_MACRO, GROUPS_MACRO]) {
    if (template.split(macro).length !== 2) {
      throw new Error(`AI 规则提示词模板中的宏数量无效：${macro}`);
    }
  }

  const replacements: Record<string, string> = {
    [TAGS_MACRO]: safeJsonArray(tagNames),
    [GROUPS_MACRO]: safeJsonArray(groupNames),
  };
  return template.replace(MACRO_PATTERN, macro => replacements[macro]);
}
