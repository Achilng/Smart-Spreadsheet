import { invoke } from "@tauri-apps/api/core";

export type RuleMatchMode = "all" | "any";
export type PromptScope = "positive" | "character" | "negative" | "positiveAndCharacter" | "all";
export type PromptOperator = "containsAll" | "containsAny" | "containsNone" | "textContains" | "textEquals" | "regex";
export type TagOperator = "hasAll" | "hasAny" | "hasNone" | "isEmpty";
export type GroupOperator = "is" | "isNot" | "isEmpty";
export type ArtistOperator = "containsAny" | "containsNone" | "isSingle" | "isMultiple" | "isEmpty";
export type NoteOperator = "contains" | "isEmpty";
export type TextOperator = "contains" | "equals" | "regex";
export type NumericOperator = "equal" | "notEqual" | "greaterThan" | "greaterOrEqual" | "lessThan" | "lessOrEqual" | "between";
export type PromptActionField = "positive" | "character" | "negative";

export interface NumericComparison {
  operator: NumericOperator;
  value: number;
  secondValue: number | null;
}

export type RuleCondition =
  | { type: "prompt"; scope: PromptScope; operator: PromptOperator; value: string; caseSensitive: boolean }
  | { type: "tag"; operator: TagOperator; tags: string[] }
  | { type: "group"; operator: GroupOperator; groupId: number | null }
  | { type: "artist"; operator: ArtistOperator; artists: string[] }
  | { type: "note"; operator: NoteOperator; value: string; caseSensitive: boolean }
  | { type: "fileText"; field: "fileName" | "originalPath" | "importSource"; operator: TextOperator; value: string; caseSensitive: boolean }
  | { type: "fileSize"; comparison: NumericComparison }
  | { type: "sourceType"; sourceType: "folder" | "archive"; negate: boolean }
  | { type: "vibe"; operator: "hasAny" | "hasNone" | "count"; comparison: NumericComparison | null }
  | { type: "metadata"; parsed: boolean }
  | { type: "imageDimension"; field: "width" | "height" | "aspectRatio"; comparison: NumericComparison }
  | { type: "orientation"; orientation: "landscape" | "portrait" | "square"; negate: boolean }
  | { type: "generationText"; field: "model" | "sampler" | "noiseSchedule" | "seed"; operator: TextOperator; value: string; caseSensitive: boolean }
  | { type: "generationNumber"; field: "steps" | "scale" | "cfgRescale"; comparison: NumericComparison };

export interface RuleConditionGroup {
  mode: RuleMatchMode;
  conditions: RuleCondition[];
}

export interface RuleConditionSet {
  mode: RuleMatchMode;
  negate: boolean;
  groups: RuleConditionGroup[];
}

export type RuleAction =
  | { type: "addTags"; tags: string[] }
  | { type: "removeTags"; tags: string[] }
  | { type: "setGroup"; groupId: number; onlyIfUngrouped: boolean }
  | { type: "clearGroup" }
  | { type: "appendPrompt"; field: PromptActionField; value: string }
  | { type: "deletePromptTags"; field: PromptActionField; value: string }
  | { type: "replacePrompt"; field: PromptActionField; find: string; replace: string; caseSensitive: boolean }
  | { type: "prefixArtist"; artists: string[] }
  | { type: "setNote"; value: string }
  | { type: "appendNote"; value: string; separator: string }
  | { type: "clearNote" }
  | { type: "stopProcessing" };

export interface AutomationRuleDraft {
  name: string;
  description: string;
  enabled: boolean;
  runOnImport: boolean;
  runOnUpdate: boolean;
  conditions: RuleConditionSet;
  actions: RuleAction[];
}

export interface AutomationRule extends AutomationRuleDraft {
  id: number;
  position: number;
  createdAt: string;
  updatedAt: string;
}

export interface AutomationRuleImportPreview {
  name: string;
  importedName: string;
  conditionCount: number;
  actionCount: number;
  runOnImport: boolean;
  runOnUpdate: boolean;
}

export interface AutomationRuleImportInspection {
  contentHash: string;
  version: number;
  ruleCount: number;
  rules: AutomationRuleImportPreview[];
  missingTags: string[];
  missingGroups: string[];
  renamedRules: number;
}

export interface AutomationRuleImportResult {
  importedRules: number;
  createdTags: number;
  createdGroups: number;
  renamedRules: number;
  importedRuleIds: number[];
}

export interface AutomationRuleExportResult {
  path: string;
  exportedRules: number;
}

export interface RulePreview {
  scannedRows: number;
  matchedRows: number;
  rowsNeedingChanges: number;
  stoppedRows: number;
  sampleRowIds: number[];
}

export interface RuleExecutionReport {
  ruleId: number;
  ruleName: string;
  scannedRows: number;
  matchedRows: number;
  changedRows: number;
  actionsChanged: number;
  stoppedRows: number;
  error: string | null;
}

export interface RuleExecutionSummary {
  trigger: "import" | "update" | "manual";
  inputRows: number;
  changedRows: number;
  reports: RuleExecutionReport[];
  engineError: string | null;
}

export function listAutomationRules(): Promise<AutomationRule[]> {
  return invoke<AutomationRule[]>("list_automation_rules");
}

export function inspectAutomationRuleFile(path: string): Promise<AutomationRuleImportInspection> {
  return invoke<AutomationRuleImportInspection>("inspect_automation_rule_file", { path });
}

export function importAutomationRuleFile(
  path: string,
  expectedHash: string,
): Promise<AutomationRuleImportResult> {
  return invoke<AutomationRuleImportResult>("import_automation_rule_file", { path, expectedHash });
}

export function exportAutomationRules(
  path: string,
  ids: number[],
): Promise<AutomationRuleExportResult> {
  return invoke<AutomationRuleExportResult>("export_automation_rules", { path, ids });
}

export function createAutomationRule(draft: AutomationRuleDraft): Promise<AutomationRule> {
  return invoke<AutomationRule>("create_automation_rule", { draft });
}

export function updateAutomationRule(id: number, draft: AutomationRuleDraft): Promise<AutomationRule> {
  return invoke<AutomationRule>("update_automation_rule", { id, draft });
}

export function setAutomationRuleEnabled(id: number, enabled: boolean): Promise<void> {
  return invoke<void>("set_automation_rule_enabled", { id, enabled });
}

export function deleteAutomationRule(id: number): Promise<boolean> {
  return invoke<boolean>("delete_automation_rule", { id });
}

export function reorderAutomationRules(ids: number[]): Promise<void> {
  return invoke<void>("reorder_automation_rules", { ids });
}

export function previewAutomationRule(id: number): Promise<RulePreview> {
  return invoke<RulePreview>("preview_automation_rule", { id });
}

/** 未保存草稿的只读预览：不要求先保存，也不会让草稿生效。 */
export function previewAutomationRuleDraft(draft: AutomationRuleDraft): Promise<RulePreview> {
  return invoke<RulePreview>("preview_automation_rule_draft", { draft });
}

export function runAutomationRuleOnLibrary(id: number): Promise<RuleExecutionSummary> {
  return invoke<RuleExecutionSummary>("run_automation_rule_on_library", { id });
}

export function emptyAutomationRuleDraft(): AutomationRuleDraft {
  return {
    name: "",
    description: "",
    enabled: true,
    runOnImport: true,
    runOnUpdate: false,
    conditions: {
      mode: "any",
      negate: false,
      groups: [{ mode: "all", conditions: [defaultRuleCondition("prompt")] }],
    },
    actions: [{ type: "addTags", tags: [] }],
  };
}

export function defaultRuleCondition(type: RuleCondition["type"]): RuleCondition {
  switch (type) {
    case "prompt": return { type, scope: "positiveAndCharacter", operator: "containsAll", value: "", caseSensitive: false };
    case "tag": return { type, operator: "hasAll", tags: [] };
    case "group": return { type, operator: "is", groupId: null };
    case "artist": return { type, operator: "containsAny", artists: [] };
    case "note": return { type, operator: "contains", value: "", caseSensitive: false };
    case "fileText": return { type, field: "fileName", operator: "contains", value: "", caseSensitive: false };
    case "fileSize": return { type, comparison: defaultComparison() };
    case "sourceType": return { type, sourceType: "folder", negate: false };
    case "vibe": return { type, operator: "hasAny", comparison: null };
    case "metadata": return { type, parsed: true };
    case "imageDimension": return { type, field: "width", comparison: defaultComparison() };
    case "orientation": return { type, orientation: "landscape", negate: false };
    case "generationText": return { type, field: "model", operator: "contains", value: "", caseSensitive: false };
    case "generationNumber": return { type, field: "steps", comparison: defaultComparison() };
  }
}

export function defaultComparison(): NumericComparison {
  return { operator: "equal", value: 0, secondValue: null };
}
