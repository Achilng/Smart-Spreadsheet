import { listTags, type TagSummary } from "../api";
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
