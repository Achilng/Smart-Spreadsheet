import type { FilterNumericComparison, LibraryFilter } from "../../lib/api";
import { splitListText as splitValues } from "../../lib/utils/list-text";
type OptionalMode<T extends string> = "any" | T;

export interface FilterDraft {
  tagMode: OptionalMode<"hasAll" | "hasAny" | "hasNone" | "isEmpty">;
  tagValues: string[];
  tagSearch: string;
  groupMode: OptionalMode<"is" | "isNot" | "isEmpty">;
  groupId: number | null;
  artistMode: OptionalMode<"containsAny" | "containsNone" | "isSingle" | "isMultiple" | "isEmpty">;
  artistText: string;
  promptMode: OptionalMode<"containsAll" | "containsAny" | "containsNone" | "isEmpty">;
  promptText: string;
  vibeMode: OptionalMode<"hasAny" | "hasNone" | "count">;
  vibeComparison: FilterNumericComparison;
  noteMode: OptionalMode<"contains" | "isEmpty" | "isNotEmpty">;
  noteText: string;
  metadataMode: "any" | "parsed" | "failed";
  orientation: "any" | "landscape" | "portrait" | "square";
  dimensionField: "any" | "width" | "height" | "aspectRatio";
  dimensionComparison: FilterNumericComparison;
  generationTextField: "any" | "model" | "sampler" | "noiseSchedule" | "seed";
  generationTextOperator: "contains" | "equals";
  generationTextValue: string;
  generationNumberField: "any" | "steps" | "scale" | "cfgRescale";
  generationNumberComparison: FilterNumericComparison;
}

const comparison = (): FilterNumericComparison => ({ operator: "equal", value: 0, secondValue: null });

export function emptyFilterDraft(): FilterDraft {
  return {
    tagMode: "any",
    tagValues: [],
    tagSearch: "",
    groupMode: "any",
    groupId: null,
    artistMode: "any",
    artistText: "",
    promptMode: "any",
    promptText: "",
    vibeMode: "any",
    vibeComparison: comparison(),
    noteMode: "any",
    noteText: "",
    metadataMode: "any",
    orientation: "any",
    dimensionField: "any",
    dimensionComparison: comparison(),
    generationTextField: "any",
    generationTextOperator: "contains",
    generationTextValue: "",
    generationNumberField: "any",
    generationNumberComparison: comparison(),
  };
}

export function hydrateFilterDraft(filters: LibraryFilter[]): FilterDraft {
  const draft = emptyFilterDraft();
  for (const filter of filters) {
    switch (filter.type) {
      case "tag": draft.tagMode = filter.operator; draft.tagValues = [...filter.values]; break;
      case "group": draft.groupMode = filter.operator; draft.groupId = filter.groupId; break;
      case "artist": draft.artistMode = filter.operator; draft.artistText = filter.values.join(", "); break;
      case "prompt": draft.promptMode = filter.operator; draft.promptText = filter.values.join(", "); break;
      case "vibe": draft.vibeMode = filter.operator; if (filter.comparison) draft.vibeComparison = { ...filter.comparison }; break;
      case "note": draft.noteMode = filter.operator; draft.noteText = filter.value; break;
      case "metadata": draft.metadataMode = filter.parsed ? "parsed" : "failed"; break;
      case "orientation": draft.orientation = filter.orientation; break;
      case "imageDimension": draft.dimensionField = filter.field; draft.dimensionComparison = { ...filter.comparison }; break;
      case "generationText": draft.generationTextField = filter.field; draft.generationTextOperator = filter.operator; draft.generationTextValue = filter.value; break;
      case "generationNumber": draft.generationNumberField = filter.field; draft.generationNumberComparison = { ...filter.comparison }; break;
    }
  }
  return draft;
}

function validComparison(value: FilterNumericComparison, integer = false, positive = false): boolean {
  const valid = (number: number | null) => number !== null && Number.isFinite(number) && (positive ? number > 0 : number >= 0) && (!integer || Number.isInteger(number));
  return valid(value.value) && (value.operator !== "between" || valid(value.secondValue) && value.secondValue! >= value.value);
}

export function buildFilters(draft: FilterDraft): LibraryFilter[] {
  const filters: LibraryFilter[] = [];
  if (draft.tagMode !== "any") {
    if (draft.tagMode !== "isEmpty" && draft.tagValues.length === 0) {
      throw new Error("请选择至少一个 Tag。");
    }
    filters.push({ type: "tag", operator: draft.tagMode, values: [...draft.tagValues] });
  }
  if (draft.groupMode !== "any") {
    if (draft.groupMode !== "isEmpty" && draft.groupId === null) {
      throw new Error("请选择一个分组。");
    }
    filters.push({ type: "group", operator: draft.groupMode, groupId: draft.groupMode === "isEmpty" ? null : draft.groupId });
  }
  if (draft.artistMode !== "any") {
    const values = splitValues(draft.artistText);
    if (["containsAny", "containsNone"].includes(draft.artistMode) && values.length === 0) {
      throw new Error("请输入至少一个画师名。");
    }
    filters.push({ type: "artist", operator: draft.artistMode, values });
  }
  if (draft.promptMode !== "any") {
    const values = splitValues(draft.promptText);
    if (draft.promptMode !== "isEmpty" && values.length === 0) {
      throw new Error("请输入至少一个提示词。");
    }
    filters.push({ type: "prompt", operator: draft.promptMode, values, caseSensitive: false });
  }
  if (draft.vibeMode !== "any") {
    if (draft.vibeMode === "count" && !validComparison(draft.vibeComparison, true)) {
      throw new Error("VIBE 数量需为非负整数，范围上限不能小于下限。");
    }
    filters.push({ type: "vibe", operator: draft.vibeMode, comparison: draft.vibeMode === "count" ? { ...draft.vibeComparison } : null });
  }
  if (draft.noteMode !== "any") {
    if (draft.noteMode === "contains" && draft.noteText.trim() === "") {
      throw new Error("请输入要在备注中查找的内容。");
    }
    filters.push({ type: "note", operator: draft.noteMode, value: draft.noteText.trim(), caseSensitive: false });
  }
  if (draft.metadataMode !== "any") filters.push({ type: "metadata", parsed: draft.metadataMode === "parsed" });
  if (draft.orientation !== "any") filters.push({ type: "orientation", orientation: draft.orientation });
  if (draft.dimensionField !== "any") {
    if (!validComparison(draft.dimensionComparison, draft.dimensionField !== "aspectRatio", true)) {
      throw new Error("图片尺寸或比例需大于零，宽高需为整数，范围上限不能小于下限。");
    }
    filters.push({ type: "imageDimension", field: draft.dimensionField, comparison: { ...draft.dimensionComparison } });
  }
  if (draft.generationTextField !== "any") {
    if (draft.generationTextValue.trim() === "") {
      throw new Error("请输入要匹配的生成参数内容。");
    }
    filters.push({ type: "generationText", field: draft.generationTextField, operator: draft.generationTextOperator, value: draft.generationTextValue.trim(), caseSensitive: false });
  }
  if (draft.generationNumberField !== "any") {
    if (!validComparison(draft.generationNumberComparison, draft.generationNumberField === "steps", draft.generationNumberField === "steps")) {
      throw new Error("生成数值需为非负数，步数需为正整数，范围上限不能小于下限。");
    }
    filters.push({ type: "generationNumber", field: draft.generationNumberField, comparison: { ...draft.generationNumberComparison } });
  }
  return filters;
}
