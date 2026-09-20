import { mutableRowState, type RowRecord, type MutableRowState } from "../../api/rows";
import { errorText } from "../../utils/format";

export function createPromptEditor<Result>(
  getRow: () => RowRecord | null,
  recordChange: (label: string, before: MutableRowState[]) => Promise<void>,
  label: string,
  getField: () => string | null | undefined,
  saveFn: (rowId: number, value: string) => Promise<Result>,
  patchField: (rowId: number, value: string, result: Result) => void,
) {
  let editing = $state(false);
  let value = $state("");
  let saving = $state(false);
  let error = $state<string | null>(null);
  let restored = $state(false);
  let initialValue = "";
  let editingRowId: number | null = null;
  /** 切行时未保存的编辑按行暂存，回到该行再点“编辑”可继续 */
  const drafts = new Map<number, string>();

  function start(): void {
    const row = getRow();
    if (!row) return;
    const base = getField() ?? "";
    const draft = drafts.get(row.id);
    value = draft ?? base;
    initialValue = base;
    restored = draft !== undefined && draft !== base;
    editingRowId = row.id;
    editing = true;
    error = null;
  }

  function isDirty(): boolean {
    return editing && value !== initialValue;
  }

  function cancel(): void {
    if (isDirty() && !window.confirm(`放弃「${label.replace("编辑", "")}」未保存的修改吗？`)) {
      return;
    }
    if (editingRowId !== null) drafts.delete(editingRowId);
    editing = false;
    error = null;
    restored = false;
  }

  async function save(): Promise<void> {
    const current = getRow();
    if (!current || saving) return;
    saving = true;
    error = null;
    const before = mutableRowState(current);
    try {
      const result = await saveFn(current.id, value);
      drafts.delete(current.id);
      // 先把新值写入行缓存，再退出编辑态，避免展示态短暂回显旧值。
      patchField(current.id, value, result);
      editing = false;
      restored = false;
      await recordChange(label, [before]);
    } catch (e) {
      error = errorText(e);
    } finally {
      saving = false;
    }
  }

  function onKeydown(event: KeyboardEvent): void {
    // 中文输入法用 Enter 确认候选词时不应触发保存。
    if (event.isComposing) return;
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      cancel();
    } else if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      event.stopPropagation();
      void save();
    }
  }

  /** 切换行时调用：不丢内容，未保存的编辑暂存为原行草稿。 */
  function reset(): void {
    if (isDirty() && editingRowId !== null) {
      drafts.set(editingRowId, value);
    }
    editing = false;
    error = null;
    restored = false;
  }

  return {
    get editing() { return editing; },
    get value() { return value; },
    set value(v: string) { value = v; },
    get saving() { return saving; },
    get error() { return error; },
    get restored() { return restored; },
    start,
    cancel,
    save,
    onKeydown,
    reset,
  };
}

