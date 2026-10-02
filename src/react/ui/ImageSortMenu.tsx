import type { SortMode } from "../../lib/api";
import { Menu } from "./controls";

const options = [
  { value: "timeAsc", label: "时间正序", hint: "早期导入在前，新图片在后" },
  { value: "timeDesc", label: "时间倒序", hint: "新导入的图片优先显示" },
  { value: "recentlyUpdated", label: "最近更新", hint: "最近编辑或整理的图片在前" },
] as const;

export function ImageSortMenu({ value, onChange }: { value: SortMode; onChange: (sort: SortMode) => void }) {
  return <Menu className="r-sort-trigger" label={options.find(option => option.value === value)?.label ?? "时间正序"} heading="选择图片顺序" items={options.map(option => ({ ...option, checked: option.value === value, action: () => onChange(option.value) }))} />;
}
