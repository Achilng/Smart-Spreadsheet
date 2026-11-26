import type { SortMode } from "../../lib/api";
import { Select, type SelectOption } from "./controls";

const options: readonly SelectOption<SortMode>[] = [
  ["timeAsc", "时间正序", "早期导入在前，新图片在后"],
  ["timeDesc", "时间倒序", "新导入的图片优先显示"],
  ["recentlyUpdated", "最近更新", "最近编辑或整理的图片在前"],
];

export function ImageSortMenu({ value, onChange }: { value: SortMode; onChange: (sort: SortMode) => void }) {
  return <Select appearance="menu" className="r-sort-trigger" align="end" label="图片顺序" heading="选择图片顺序" value={value} onChange={onChange} options={options} />;
}
