<script lang="ts">
  import type { MaterialVersion } from "../../api/materials";
  let { versions, selected, onselect }: { versions: MaterialVersion[]; selected: number; onselect: (id: number) => void } = $props();
  let strip = $state<HTMLDivElement | null>(null);
  let left = $state(0), width = $state(0);
  function measure() {
    const button = strip?.querySelector<HTMLButtonElement>(`[data-version-id="${selected}"]`);
    if (button) { left = button.offsetLeft; width = button.offsetWidth; }
  }
  $effect(() => {
    void selected; void versions;
    if (!strip) return;
    const observer = new ResizeObserver(measure);
    observer.observe(strip);
    measure();
    const button = strip.querySelector<HTMLButtonElement>(`[data-version-id="${selected}"]`);
    if (button) {
      const position = button.offsetLeft, end = position + button.offsetWidth;
      if (position < strip.scrollLeft) strip.scrollLeft = position;
      else if (end > strip.scrollLeft + strip.clientWidth) strip.scrollLeft = end - strip.clientWidth;
    }
    return () => observer.disconnect();
  });
  function key(event: KeyboardEvent, index: number) {
    let next = index;
    if (event.key === "ArrowRight") next = (index + 1) % versions.length;
    else if (event.key === "ArrowLeft") next = (index - 1 + versions.length) % versions.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = versions.length - 1;
    else return;
    event.preventDefault(); onselect(versions[next].id);
    strip?.querySelector<HTMLButtonElement>(`[data-version-id="${versions[next].id}"]`)?.focus();
  }
</script>
<div class="version-switcher">
  <div class="strip" bind:this={strip} role="tablist" aria-label="素材版本">
    <span class="indicator" style:width="{width}px" style:transform="translateX({left}px)" aria-hidden="true"></span>
    {#each versions as version, index (version.id)}
      <button type="button" role="tab" aria-selected={version.id === selected} tabindex={version.id === selected ? 0 : -1}
        data-version-id={version.id} class:active={version.id === selected} title={index === 0 ? `${version.name} · 默认复制` : version.name}
        onclick={() => onselect(version.id)} onkeydown={event => key(event,index)}>{version.name}</button>
    {/each}
  </div>
  {#if versions.length > 3}
    <select aria-label="选择所有版本" value={selected} onchange={event => onselect(Number(event.currentTarget.value))}>
      {#each versions as version, index (version.id)}<option value={version.id}>{version.name}{index === 0 ? " · 默认" : ""}</option>{/each}
    </select>
  {/if}
</div>
<style>
  .version-switcher { display: flex; gap: 6px; min-width: 0; margin: 12px 0 2px; flex: none; }
  .strip { position: relative; flex: 1; min-width: 0; display: flex; overflow-x: auto; scrollbar-width: thin; border-radius: 9px; background: var(--surface-2); padding: 3px 0; isolation: isolate; }
  .indicator { position: absolute; top: 3px; bottom: 3px; left: 0; border-radius: 7px; background: var(--surface); box-shadow: var(--shadow-1); z-index: -1; transition: transform 180ms var(--ease-responsive), width 180ms var(--ease-responsive); }
  button { flex: none; border: 0; padding: 7px 10px; background: transparent; border-radius: 7px; font-size: var(--font-sm); color: var(--text-3); white-space: nowrap; max-width: 160px; overflow: hidden; text-overflow: ellipsis; transition: color 180ms var(--ease-responsive); }
  button.active { color: var(--text); font-weight: 600; }
  select { max-width: 34px; border: 1px solid var(--border); border-radius: 8px; color: var(--text-2); background: var(--surface); font-size: 11px; }
  @media (prefers-reduced-motion: reduce) { .indicator, button { transition: none; } }
</style>
