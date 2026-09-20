<script lang="ts">
  import { setNotice } from "../../stores/notices.svelte";
  import { onDestroy } from "svelte";
  import Layers from "@lucide/svelte/icons/layers";
  import Copy from "@lucide/svelte/icons/copy";
  import Check from "@lucide/svelte/icons/check";
  import type { Material, MaterialVersion } from "../../api/materials";

  import ContextMenuShell from "../../ui/ContextMenuShell.svelte";

  let { material, onmanage }: { material: Material; onmanage: () => void } = $props();
  let open = $state(false), x = $state(0), y = $state(0);
  let copiedId = $state<number | null>(null), busy = $state(false);
  let session = 0;
  let feedbackTimer: ReturnType<typeof setTimeout> | undefined;
  let leaveTimer: ReturnType<typeof setTimeout> | undefined;
  function keepOpen() { clearTimeout(leaveTimer); }
  function leave(event: PointerEvent) {
    if (!open || event.pointerType === "touch") return;
    keepOpen();
    // Allow crossing the small gap between the image corner and the floating menu.
    leaveTimer = setTimeout(close, 180);
  }
  function close() { session++; keepOpen(); clearTimeout(feedbackTimer); open = false; copiedId = null; busy = false; }
  function show(event: MouseEvent) {
    keepOpen();
    if (open) return;
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
      clearTimeout(feedbackTimer);
      feedbackTimer = setTimeout(() => copiedId = null, 1200);
    } catch {
      if (token === session) setNotice({ tone: "error", text: "复制失败，请检查剪贴板权限。" });
    } finally { if (token === session) busy = false; }
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

<div class="version-control" role="group" aria-label="素材版本快捷复制" onpointerenter={keepOpen} onpointerleave={leave}>
<button class="version-count" title={`${material.versions.length} 个版本 · 快速复制`} aria-label={`${material.title}：选择版本复制`} aria-haspopup="menu" aria-expanded={open} onclick={show} onpointerdown={event => { if (open) event.stopPropagation(); }}>
  <Layers size={13} strokeWidth={1.6} /><span>{material.versions.length}</span>
</button>
<ContextMenuShell {open} {x} {y} onclose={close}>
  <div class="menu-heading">复制版本</div>
  <div class="version-options">
    {#each material.versions as version, index (version.id)}
      <button type="button" role="menuitem" class:copied={copiedId === version.id} aria-disabled={busy} onclick={() => void copy(version)}>
        <span class="name">{version.name}</span>
        {#if index === 0 && copiedId !== version.id}<small>默认</small>{/if}
        <span class="copy-indicator" class:success={copiedId === version.id} aria-label={copiedId === version.id ? "已复制" : "复制"} aria-live="polite">
          {#if copiedId === version.id}<Check size={13} />{:else}<Copy size={13} strokeWidth={1.5} />{/if}
        </span>
      </button>
    {/each}
  </div>
  <div class="separator"></div>
  <button type="button" role="menuitem" onclick={() => { close(); onmanage(); }}>管理版本…</button>
</ContextMenuShell>
</div>

<style>
  .version-control { line-height: 1; }
  .version-count { position: relative; z-index: 1; display: flex; align-items: center; justify-content: center; gap: 5px; min-width: 34px; height: 26px; padding: 0 8px; border: 1px solid rgb(255 255 255 / 48%); border-radius: 8px; background: rgb(255 255 255 / 68%); color: rgb(35 42 52 / 90%); backdrop-filter: blur(8px); box-shadow: 0 1px 5px rgb(0 0 0 / 10%); font-size: 11px; font-weight: 500; font-variant-numeric: tabular-nums; line-height: 1; }
  /* Use the image's center and motion for the trigger only; its fixed menu stays viewport-relative. */
  .version-count {
    transform: var(--gallery-image-transform);
    transform-origin: calc(100% + 8px - var(--gallery-image-width) / 2) calc(var(--gallery-image-height) / 2 - 8px);
    transition: transform var(--gallery-image-duration) var(--ease-responsive), background-color var(--motion-fast) var(--ease-responsive), color var(--motion-fast) var(--ease-responsive);
  }
  .version-count:hover, .version-count[aria-expanded="true"] { background: rgb(255 255 255 / 84%); color: rgb(35 42 52); }
  .version-count:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .version-options { width: 200px; max-height: min(320px, 60vh); overflow-y: auto; }
  .menu-heading { padding: 7px 10px 5px; color: var(--text-3); font-size: 10px; }
  .version-options button { gap: 10px; min-height: 36px; }
  .name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  small { flex: none; color: var(--text-3); font-size: 10px; }
  .copy-indicator { display: flex; flex: none; color: var(--text-3); opacity: 0; transition: opacity var(--motion-fast) var(--ease-responsive); }
  button:hover .copy-indicator, button:focus-visible .copy-indicator, .copy-indicator.success { opacity: 1; }
  .copy-indicator.success { color: var(--accent); }
</style>
