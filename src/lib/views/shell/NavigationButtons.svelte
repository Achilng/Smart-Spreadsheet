<script lang="ts">
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import ArrowRight from "@lucide/svelte/icons/arrow-right";
  import { navigateHistory, navigation } from "../../stores/navigation.svelte";
  import { anyModalOpen } from "../../stores/modal-layer.svelte";
</script>

<nav class="browse-navigation" aria-label="浏览历史" aria-busy={navigation.restoring}>
  <button type="button" aria-label="后退" aria-keyshortcuts="Alt+ArrowLeft"
    title={navigation.backLabel ? `后退到：${navigation.backLabel}（Alt + ←）` : "没有可后退的浏览记录"}
    disabled={!navigation.backLabel || navigation.restoring || anyModalOpen()}
    onclick={() => navigateHistory(-1)}><ArrowLeft size={18} strokeWidth={1.8} /></button>
  <button type="button" aria-label="前进" aria-keyshortcuts="Alt+ArrowRight"
    title={navigation.forwardLabel ? `前进到：${navigation.forwardLabel}（Alt + →）` : "没有可前进的浏览记录"}
    disabled={!navigation.forwardLabel || navigation.restoring || anyModalOpen()}
    onclick={() => navigateHistory(1)}><ArrowRight size={18} strokeWidth={1.8} /></button>
</nav>

<style>
  .browse-navigation { display: flex; gap: 2px; flex: none; }
  button {
    display: grid; place-items: center; width: 30px; height: 30px; padding: 0;
    border: 0; border-radius: var(--radius-full); color: var(--text-2);
    background: transparent; cursor: pointer;
    transition: background var(--motion-fast), color var(--motion-fast);
  }
  button:hover:not(:disabled) { background: var(--surface-3); color: var(--text); }
  button:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  button:disabled { opacity: 0.3; cursor: default; }
  @media (prefers-reduced-motion: reduce) { button { transition: none; } }
</style>
