<script lang="ts">
  import type { LucideIcon } from "@lucide/svelte";
  import ArrowDown01 from "@lucide/svelte/icons/arrow-down-0-1";
  import ArrowUp01 from "@lucide/svelte/icons/arrow-up-0-1";
  import Check from "@lucide/svelte/icons/check";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import ClockArrowUp from "@lucide/svelte/icons/clock-arrow-up";
  import ListFilter from "@lucide/svelte/icons/list-filter";
  import { tick } from "svelte";

  import type { SortMode } from "../../api";
  import { rowStore, setSort } from "../../stores/row-store.svelte";
  import { softPop } from "../../ui/motion";

  interface SortOption {
    value: SortMode;
    label: string;
    description: string;
    icon: LucideIcon;
  }

  const options: SortOption[] = [
    {
      value: "timeAsc",
      label: "时间正序",
      description: "早期导入在前，新图片在后",
      icon: ArrowDown01,
    },
    {
      value: "timeDesc",
      label: "时间倒序",
      description: "新导入的图片优先显示",
      icon: ArrowUp01,
    },
    {
      value: "recentlyUpdated",
      label: "最近更新",
      description: "最近编辑或整理的图片在前",
      icon: ClockArrowUp,
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

<div class="sort-toolbar">
  <div class="toolbar-context" aria-hidden="true">
    <span class="context-icon"><ListFilter size={14} strokeWidth={1.8} /></span>
    <span class="context-copy">
      <span class="context-title">浏览顺序</span>
      <span class="context-hint">{activeOption.description}</span>
    </span>
  </div>

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
      <span class="trigger-icon" aria-hidden="true">
        <activeOption.icon size={15} strokeWidth={1.9} />
      </span>
      <span class="trigger-copy">
        <span class="trigger-eyebrow">排序方式</span>
        <span class="trigger-label">{activeOption.label}</span>
      </span>
      <ChevronDown class={open ? "is-open" : undefined} size={15} strokeWidth={1.9} aria-hidden="true" />
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
            <span class="option-icon" aria-hidden="true">
              <option.icon size={17} strokeWidth={1.8} />
            </span>
            <span class="option-copy">
              <span class="option-label">{option.label}</span>
              <span class="option-description">{option.description}</span>
            </span>
            <span class="option-check" aria-hidden="true">
              {#if selected}
                <span class="check-mark" transition:softPop={{ duration: 120, y: 0, start: 0.8 }}>
                  <Check size={15} strokeWidth={2.2} />
                </span>
              {/if}
            </span>
          </button>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .sort-toolbar {
    height: 48px;
    flex: none;
    position: relative;
    z-index: var(--z-dropdown);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 0 16px;
    border-bottom: 1px solid var(--border);
    background: linear-gradient(180deg, var(--surface) 0%, #fbfcfd 100%);
  }

  .toolbar-context {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 9px;
  }

  .context-icon {
    width: 26px;
    height: 26px;
    flex: none;
    display: grid;
    place-items: center;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    background: var(--surface-2);
    color: var(--text-2);
  }

  .context-copy,
  .trigger-copy,
  .option-copy {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .context-title {
    color: var(--text);
    font-size: var(--font-sm);
    font-weight: 650;
    line-height: 1.2;
  }

  .context-hint {
    margin-top: 2px;
    color: var(--text-3);
    font-size: var(--font-xs);
    line-height: 1.2;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sort-picker {
    position: relative;
    flex: none;
    max-width: 100%;
  }

  .sort-trigger {
    width: 174px;
    max-width: 100%;
    height: 34px;
    display: grid;
    grid-template-columns: 26px minmax(0, 1fr) 16px;
    align-items: center;
    gap: 7px;
    padding: 3px 8px 3px 5px;
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow-1);
    text-align: left;
    transition:
      background var(--motion-fast) var(--ease-responsive),
      border-color var(--motion-fast) var(--ease-responsive),
      box-shadow var(--motion-fast) var(--ease-responsive),
      transform var(--motion-press) var(--ease-responsive);
  }

  .sort-trigger:hover,
  .sort-trigger.is-open {
    border-color: var(--accent-soft-border);
    background: var(--surface-2);
    box-shadow: var(--shadow-2);
  }

  .sort-trigger:active {
    transform: translateY(1px) scale(0.99);
  }

  .trigger-icon {
    width: 26px;
    height: 26px;
    display: grid;
    place-items: center;
    border-radius: 7px;
    background: var(--accent-soft);
    color: var(--accent);
    transition: transform var(--motion-base) var(--ease-responsive);
  }

  .sort-trigger:hover .trigger-icon,
  .sort-trigger.is-open .trigger-icon {
    transform: scale(1.06);
  }

  .trigger-eyebrow {
    color: var(--text-3);
    font-size: 9px;
    font-weight: 600;
    line-height: 1;
  }

  .trigger-label {
    margin-top: 2px;
    overflow: hidden;
    color: var(--text);
    font-size: var(--font-sm);
    font-weight: 650;
    line-height: 1.1;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sort-trigger :global(.lucide-chevron-down) {
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
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-m);
    background: color-mix(in srgb, var(--surface) 97%, transparent);
    box-shadow: var(--shadow-3);
    backdrop-filter: blur(14px);
    transform-origin: top right;
  }

  .menu-heading {
    padding: 5px 8px 7px;
    color: var(--text-3);
    font-size: var(--font-xs);
    font-weight: 650;
    letter-spacing: 0.04em;
  }

  .sort-option {
    width: 100%;
    min-height: 54px;
    display: grid;
    grid-template-columns: 34px minmax(0, 1fr) 20px;
    align-items: center;
    gap: 9px;
    padding: 7px 8px;
    border: 1px solid transparent;
    border-radius: 9px;
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

  .option-icon {
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    border-radius: 9px;
    background: var(--surface-3);
    color: var(--text-2);
    transition:
      background var(--motion-fast) var(--ease-responsive),
      color var(--motion-fast) var(--ease-responsive),
      transform var(--motion-base) var(--ease-responsive);
  }

  .sort-option:hover .option-icon {
    transform: scale(1.05);
  }

  .sort-option.is-selected .option-icon {
    background: var(--surface);
    color: var(--accent);
    box-shadow: 0 1px 3px rgb(49 99 232 / 12%);
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

  .option-check {
    height: 20px;
    display: grid;
    place-items: center;
    color: var(--accent);
  }

  .check-mark {
    display: grid;
    place-items: center;
  }

  @media (max-width: 850px) {
    .sort-toolbar {
      justify-content: flex-end;
      padding-inline: 10px;
    }

    .toolbar-context {
      display: none;
    }

    .sort-picker,
    .sort-trigger {
      width: 100%;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .sort-trigger,
    .trigger-icon,
    .sort-trigger :global(.lucide-chevron-down),
    .sort-option,
    .option-icon {
      transition-duration: 0ms;
    }
  }
</style>
