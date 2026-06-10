import { createTag, listTags, type TagSummary } from "../api";
import { errorText } from "./app-state.svelte";

export const tagStore = $state({
  list: [] as TagSummary[],
  loaded: false,
  error: null as string | null,
});

export async function loadTags(): Promise<void> {
  try {
    tagStore.list = await listTags();
    tagStore.loaded = true;
    tagStore.error = null;
  } catch (error) {
    tagStore.error = errorText(error);
  }
}

/** 新建 Tag；返回 true 表示新建，false 表示已存在。 */
export async function createTagAndRefresh(name: string): Promise<boolean> {
  const created = await createTag(name);
  await loadTags();
  return created;
}
