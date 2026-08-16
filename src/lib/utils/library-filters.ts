import type { GroupSummary, LibraryFilter } from "../api";

/** Svelte 响应式代理不能交给 structuredClone；经 JSON 复制同时得到可安全传 IPC 的纯对象。 */
export function cloneLibraryFilters(filters: readonly LibraryFilter[]): LibraryFilter[] {
  return JSON.parse(JSON.stringify(filters)) as LibraryFilter[];
}

const numericLabels = {
  equal: "=",
  notEqual: "≠",
  greaterThan: ">",
  greaterOrEqual: "≥",
  lessThan: "<",
  lessOrEqual: "≤",
  between: "介于",
} as const;

function comparisonLabel(filter: { comparison: { operator: keyof typeof numericLabels; value: number; secondValue: number | null } }): string {
  const { operator, value, secondValue } = filter.comparison;
  return operator === "between"
    ? `${numericLabels[operator]} ${value}～${secondValue ?? value}`
    : `${numericLabels[operator]} ${value}`;
}

export function libraryFilterLabel(filter: LibraryFilter, groups: readonly GroupSummary[]): string {
  switch (filter.type) {
    case "tag": {
      if (filter.operator === "isEmpty") return "Tag：无 Tag";
      const operator = { hasAll: "包含全部", hasAny: "包含任意", hasNone: "不包含" }[filter.operator];
      return `Tag：${operator} ${filter.values.join("、")}`;
    }
    case "group": {
      if (filter.operator === "isEmpty") return "分组：未分组";
      const name = groups.find(group => group.id === filter.groupId)?.name ?? `#${filter.groupId ?? "?"}`;
      return `分组：${filter.operator === "is" ? "属于" : "不属于"} ${name}`;
    }
    case "artist": {
      const fixed = { isSingle: "单画师", isMultiple: "多画师", isEmpty: "无画师" } as const;
      if (filter.operator in fixed) return `画师：${fixed[filter.operator as keyof typeof fixed]}`;
      return `画师：${filter.operator === "containsAny" ? "包含" : "不包含"} ${filter.values.join("、")}`;
    }
    case "vibe":
      return filter.operator === "hasAny"
        ? "VIBE：存在"
        : filter.operator === "hasNone"
          ? "VIBE：不存在"
          : `VIBE 数量 ${comparisonLabel({ comparison: filter.comparison! })}`;
    case "note":
      return filter.operator === "contains" ? `备注：包含 ${filter.value}` : `备注：${filter.operator === "isEmpty" ? "为空" : "不为空"}`;
    case "metadata":
      return `元数据：${filter.parsed ? "解析成功" : "解析失败"}`;
    case "orientation":
      return `构图：${{ landscape: "横图", portrait: "竖图", square: "正方形" }[filter.orientation]}`;
    case "imageDimension":
      return `${{ width: "宽度", height: "高度", aspectRatio: "宽高比" }[filter.field]} ${comparisonLabel(filter)}`;
    case "generationText":
      return `${{ model: "模型", sampler: "采样器", noiseSchedule: "噪声调度", seed: "种子" }[filter.field]}：${filter.operator === "contains" ? "包含" : "等于"} ${filter.value}`;
    case "generationNumber":
      return `${{ steps: "步数", scale: "Guidance", cfgRescale: "CFG Rescale" }[filter.field]} ${comparisonLabel(filter)}`;
  }
}
