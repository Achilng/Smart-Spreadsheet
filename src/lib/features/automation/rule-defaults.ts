import type { RuleAction } from "../../api/automation-rules";

export function defaultAction(type: RuleAction["type"]): RuleAction {
  switch (type) {
    case "addTags": return { type, tags: [] };
    case "removeTags": return { type, tags: [] };
    case "setGroup": return { type, groupId: 0, onlyIfUngrouped: false };
    case "clearGroup": return { type };
    case "appendPrompt": return { type, field: "positive", value: "" };
    case "deletePromptTags": return { type, field: "positive", value: "" };
    case "replacePrompt": return { type, field: "positive", find: "", replace: "", caseSensitive: true };
    case "prefixArtist": return { type, artists: [] };
    case "setNote": return { type, value: "" };
    case "setNoteSequence": return { type, prefix: "" };
    case "appendNote": return { type, value: "", separator: "\n" };
    case "clearNote": return { type };
    case "stopProcessing": return { type };
  }
}
