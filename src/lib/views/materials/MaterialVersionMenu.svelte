<script lang="ts">
  import { onDestroy } from "svelte";
  import type { Material, MaterialVersion } from "../../api/materials";
  import { setNotice } from "../../stores/app-state.svelte";
  import ContextMenuShell from "../../ui/ContextMenuShell.svelte";

  let { material, onmanage }: { material: Material; onmanage: () => void } = $props();
  let open = $state(false), x = $state(0), y = $state(0);
  let copiedId = $state<number | null>(null), busy = $state(false);
  let session = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  function close() { session++; clearTimeout(timer); open = false; copiedId = null; busy = false; }
  function show(event: MouseEvent) {
    if (open) { close(); return; }
    const rect = (event.currentTarget as HTMLButtonElement).getBoundingClientRect();
    close(); x = Math.max(8, rect.right - 200); y = rect.bottom + 6; open = true;
  }
  async function copy(version: MaterialVersion) {
    if (busy) return;
    if (!version.text) { setNotice({ tone: "error", text: `「${version.name}」还没有文本。` }); return; }
    const token = session;
    busy = true;
    try {
      await navigator.clipboard.writeText(version.text);
      if (token !== session) return;
      copiedId = version.id;
      timer = setTimeout(close, 550);
    } catch {
      if (token === session) { busy = false; setNotice({ tone: "error", text: "复制失败，请检查剪贴板权限。" }); }
    }
  }
  onDestroy(close);
  $effect(() => {
    if (!open) return;
    const scroll = (event: Event) => {
      if (!(event.target instanceof Element && event.target.closest('[role="menu"]'))) close();
    };
    window.addEventListener("scroll", scroll, true);
    window.addEventListener("resize", close);
    return () => { window.removeEventListener("scroll", scroll, true); window.removeEventListener("resize", close); };
  });
</script>

<button class="version-count" aria-label={`${material.title}：选择版本复制`} aria-haspopup="menu" aria-expanded={open} onclick={show} onpointerdown={event => { if (open) event.stopPropagation(); }}>
  {material.versions.length} 个版本 <span aria-hidden="true">⌄</span>
</button>
<ContextMenuShell {open} {x} {y} onclose={close}>
  <div class="version-options">
    {#each material.versions as version, index (version.id)}
      <button type="button" role="menuitem" class:copied={copiedId === version.id} aria-disabled={busy} onclick={() => void copy(version)}>
        <span class="name">{version.name}</span>
        <small aria-live="polite">{copiedId === version.id ? "已复制" : index === 0 ? "默认" : ""}</small>
      </button>
    {/each}
  </div>
  <div class="separator"></div>
  <button type="button" role="menuitem" onclick={() => { close(); onmanage(); }}>管理版本…</button>
</ContextMenuShell>

<style>
  .version-count { position: relative; z-index: 1; }
  .version-count { display: flex; align-items: center; gap: 5px; padding: 4px 8px; border: 1px solid color-mix(in srgb, var(--border) 60%, transparent); border-radius: var(--radius-full); background: color-mix(in srgb, var(--surface) 88%, transparent); color: var(--text-2); backdrop-filter: blur(12px); box-shadow: var(--shadow-1); font-size: 11px; line-height: 1.4; transition: background var(--motion-fast) var(--ease-responsive), color var(--motion-fast) var(--ease-responsive); }
  .version-count:hover, .version-count[aria-expanded="true"] { background: var(--surface); color: var(--text); }
  .version-count:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .version-options { width: 200px; max-height: min(320px, 60vh); overflow-y: auto; }
  .version-options button { gap: 14px; min-height: 34px; }
  .name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  small { flex: none; color: var(--text-3); font-size: 10px; }
  .copied small { color: var(--accent); }
</style>
