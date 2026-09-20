<script lang="ts">
  import { setNotice } from "../../stores/notices.svelte";
  import { onDestroy } from "svelte";
  import type { MaterialVersion } from "../../api/materials";
  import type { ImageLoader } from "../../images/image-loader";

  import MaterialDetailImage from "./MaterialDetailImage.svelte";

  let { version, isDefault, loader, revision, active, onedit }: {
    version: MaterialVersion; isDefault: boolean; loader: ImageLoader; revision: number; active: boolean; onedit: () => void;
  } = $props();
  let copied = $state(false), copying = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let generation = 0;
  $effect(() => { void version.text; void revision; generation++; copied = false; copying = false; clearTimeout(timer); });
  async function copy() {
    if (!version.text || copying) return;
    const token = generation;
    copying = true;
    try {
      await navigator.clipboard.writeText(version.text);
      if (token !== generation) return;
      copied = true;
      clearTimeout(timer);
      timer = setTimeout(() => copied = false, 1200);
    } catch { setNotice({ tone: "error", text: "复制失败，请检查剪贴板权限。" }); }
    finally { if (token === generation) copying = false; }
  }
  onDestroy(() => { generation++; clearTimeout(timer); });
</script>

<section class="field version-field">
  <div class="field-head">
    <div class="version-title"><h4>{version.name}</h4>{#if isDefault}<span>默认</span>{/if}</div>
    <div class="field-head-actions">
      <button type="button" class="copy-btn" onclick={onedit}>编辑</button>
      <button type="button" class="copy-btn" disabled={!version.text || copying} onclick={() => void copy()}>{copied ? "已复制" : "复制"}</button>
    </div>
  </div>
  {#if version.hasImage}<div class="version-image"><MaterialDetailImage id={version.id} {loader} {revision} {active} alt={version.name} /></div>{/if}
  <pre class:is-empty={!version.text}>{version.text || "—"}</pre>
</section>

<style>
  .version-field { flex: none; min-width: 0; }
  .version-field .field-head { gap: 12px; margin-bottom: 8px; }
  .version-title { display: flex; align-items: baseline; flex-wrap: wrap; gap: 6px; min-width: 0; }
  .version-field .version-title h4 { margin: 0; overflow-wrap: anywhere; }
  .version-title > span { color: var(--text-3); font-size: 10px; font-weight: 400; }
  .version-field .field-head-actions { flex: none; }
  .version-field.field pre { max-height: none; overflow: visible; line-height: 1.65; }
  .version-image { margin-bottom: 10px; }
</style>
