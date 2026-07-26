<script lang="ts">
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import { tick } from "svelte";

  import type { SortMode } from "../../api";
  import { rowStore, setSort } from "../../stores/row-store.svelte";
  import { softPop } from "../../ui/motion";

  interface SortOption {
    value: SortMode;
    label: string;
    description: string;
  }

  const options: SortOption[] = [
    {
      value: "timeAsc",
      label: "时间正序",
      description: "早期导入在前，新图片在后",
    },
    {
      value: "timeDesc",
      label: "时间倒序",
      description: "新导入的图片优先显示",
    },
    {
      value: "recentlyUpdated",
      label: "最近更新",
      description: "最近编辑或整理的图片在前",
    },
  ];

  let { controlId }: { controlId: string } = $props();

  let open = $state(false);
  let root = $state<HTMLDivElement | null>(null);
  let trigger = $state<HTMLButtonElement | null>(null);
  let menu = $state<HTMLDivElement | null>(null);

  const activeOption = $derived(
    options.find(option => option.value === rowStore.sort) ?? options[0],
  );

  function closeMenu(restoreFocus = false): void {
    if (!open) return;
    open = false;
    if (restoreFocus) {
      void tick().then(() => trigger?.focus());
    }
  }

  function onWindowPointerDown(event: PointerEvent): void {
    if (open && root && !root.contains(event.target as Node)) {
      closeMenu();
    }
  }

  function pick(option: SortOption): void {
    closeMenu();
    setSort(option.value);
  }

  async function focusMenuOption(position: "selected" | "first"): Promise<void> {
    await tick();
    const buttons = menu?.querySelectorAll<HTMLButtonElement>(".sort-option");
    if (!buttons?.length) return;
    if (position === "first") {
      buttons[0].focus();
      return;
    }
    const selectedIndex = options.findIndex(option => option.value === rowStore.sort);
    buttons[Math.max(0, selectedIndex)]?.focus();
  }

  function onTriggerKeydown(event: KeyboardEvent): void {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      open = true;
      void focusMenuOption(event.key === "ArrowDown" ? "selected" : "first");
    }
  }

  function onMenuKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      closeMenu(true);
      return;
    }
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;

    const buttons = Array.from(
      menu?.querySelectorAll<HTMLButtonElement>(".sort-option") ?? [],
    );
    if (buttons.length === 0) return;
    event.preventDefault();
    const currentIndex = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const nextIndex =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? buttons.length - 1
          : event.key === "ArrowDown"
            ? (Math.max(0, currentIndex) + 1) % buttons.length
            : (currentIndex <= 0 ? buttons.length : currentIndex) - 1;
    buttons[nextIndex]?.focus();
  }
</script>

<svelte:window
  onpointerdown={onWindowPointerDown}
  onkeydown={event => {
    if (event.key === "Escape" && open) closeMenu(true);
  }}
/>

<div class="sort-picker" bind:this={root}>
  <button
    bind:this={trigger}
    id={controlId}
    type="button"
    class="sort-trigger"
    class:is-open={open}
    aria-label={`排序方式：${activeOption.label}`}
    aria-haspopup="menu"
    aria-expanded={open}
    aria-controls={`${controlId}-menu`}
    onclick={() => (open = !open)}
    onkeydown={onTriggerKeydown}
  >
    <span class="trigger-label">{activeOption.label}</span>
    <ChevronDown
      class={open ? "is-open" : undefined}
      size={13}
      strokeWidth={2}
      aria-hidden="true"
    />
  </button>

  {#if open}
    <div
      bind:this={menu}
      id={`${controlId}-menu`}
      class="sort-menu"
      role="menu"
      tabindex="-1"
      aria-label="选择图片排序方式"
      onkeydown={onMenuKeydown}
      transition:softPop={{ duration: 180, y: -7, start: 0.975 }}
    >
      <div class="menu-heading">选择图片顺序</div>
      {#each options as option (option.value)}
        {@const selected = rowStore.sort === option.value}
        <button
          type="button"
          class="sort-option"
          class:is-selected={selected}
          role="menuitemradio"
          aria-checked={selected}
          onclick={() => pick(option)}
        >
          <span class="option-copy">
            <span class="option-label">{option.label}</span>
            <span class="option-description">{option.description}</span>
          </span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .sort-picker {
    position: relative;
    flex: none;
    max-width: 100%;
  }

  .option-copy {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .sort-trigger {
    max-width: 100%;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 8px;
    border: none;
    border-radius: var(--radius-full);
    background: none;
    color: var(--text-2);
    transition:
      background var(--motion-fast) var(--ease-responsive),
      color var(--motion-fast) var(--ease-responsive),
      transform var(--motion-press) var(--ease-responsive);
  }

  .sort-trigger:hover,
  .sort-trigger.is-open {
    background: var(--surface-2);
    color: var(--text);
  }

  .sort-trigger:active {
    transform: translateY(1px) scale(0.99);
  }

  .trigger-label {
    overflow: hidden;
    font-size: 12.5px;
    font-weight: 400;
    line-height: 1.1;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sort-trigger :global(.lucide-chevron-down) {
    flex: none;
    color: var(--text-3);
    transition: transform var(--motion-base) var(--ease-responsive);
  }

  .sort-trigger :global(.lucide-chevron-down.is-open) {
    transform: rotate(180deg);
  }

  .sort-menu {
    position: absolute;
    top: calc(100% + 7px);
    right: 0;
    z-index: var(--z-menu);
    width: 286px;
    max-width: calc(100vw - 32px);
    padding: 6px;
    border-radius: var(--radius-m);
    background: var(--surface);
    box-shadow: var(--shadow-2);
    transform-origin: top right;
  }

  .menu-heading {
    padding: 5px 8px 7px;
    color: var(--text-3);
    font-size: var(--font-xs);
    font-weight: 650;
    letter-spacing: var(--ls-caps);
  }

  .sort-option {
    width: 100%;
    min-height: 54px;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    align-items: center;
    gap: 9px;
    padding: 7px 8px;
    border: 1px solid transparent;
    border-radius: var(--radius-s);
    background: transparent;
    text-align: left;
    transition:
      background var(--motion-fast) var(--ease-responsive),
      border-color var(--motion-fast) var(--ease-responsive),
      transform var(--motion-fast) var(--ease-responsive);
  }

  .sort-option + .sort-option {
    margin-top: 2px;
  }

  .sort-option:hover,
  .sort-option:focus-visible {
    background: var(--surface-2);
    transform: translateX(2px);
  }

  .sort-option.is-selected {
    border-color: var(--accent-soft-border);
    background: var(--accent-soft);
  }

  .sort-option.is-selected:hover,
  .sort-option.is-selected:focus-visible {
    background: color-mix(in srgb, var(--accent-soft) 82%, var(--surface));
  }

  .option-label {
    color: var(--text);
    font-size: var(--font-md);
    font-weight: 650;
    line-height: 1.25;
  }

  .option-description {
    margin-top: 3px;
    overflow: hidden;
    color: var(--text-3);
    font-size: var(--font-xs);
    line-height: 1.2;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  @media (prefers-reduced-motion: reduce) {
    .sort-trigger,
    .sort-trigger :global(.lucide-chevron-down),
    .sort-option {
      transition-duration: 0ms;
    }
  }
</style>
