<script lang="ts">
  import type { RowRecord } from "../../api";
  import Check from "@lucide/svelte/icons/check";
  import Copy from "@lucide/svelte/icons/copy";
  import CompareLargeImage from "./CompareLargeImage.svelte";
  import { modelVersionBadge } from "../../utils/model-version";
  import { rowFileName, rowResolution } from "../../utils/row-display";

  let { row }: { row: RowRecord } = $props();

  const hasImage = $derived(
    Boolean(row.imagePath?.trim() || row.storedImagePath?.trim()),
  );
  const fileName = $derived(rowFileName(row));
  const resolution = $derived(rowResolution(row));
  const versionBadge = $derived(modelVersionBadge(row.generationModel));
  const artistLines = $derived(
    (row.artists ?? "")
      .split("\n")
      .map(line => line.trim())
      .filter(Boolean),
  );
  const generationEntries = $derived.by(() => {
    const entries: Array<{ label: string; value: string }> = [];
    if (row.generationSampler) entries.push({ label: "采样器", value: row.generationSampler });
    if (row.generationSteps != null) entries.push({ label: "步数", value: String(row.generationSteps) });
    if (row.generationScale != null) entries.push({ label: "Guidance", value: row.generationScale });
    if (row.generationSeed != null) entries.push({ label: "种子", value: row.generationSeed });
    return entries;
  });

  const promptFields = $derived.by(() => {
    const fields: Array<{ key: string; label: string; value: string }> = [];
    if (row.positivePrompt?.trim()) {
      fields.push({ key: "positive", label: "正向提示词", value: row.positivePrompt });
    }
    if (row.characterPrompt?.trim()) {
      fields.push({ key: "character", label: "角色提示词", value: row.characterPrompt });
    }
    if (row.negativePrompt?.trim()) {
      fields.push({ key: "negative", label: "负向提示词", value: row.negativePrompt });
    }
    return fields;
  });

  let expandedPrompts = $state<Set<string>>(new Set());
  let copiedKey = $state<string | null>(null);
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  function togglePrompt(key: string): void {
    const next = new Set(expandedPrompts);
    if (next.has(key)) {
      next.delete(key);
    } else {
      next.add(key);
    }
    expandedPrompts = next;
  }

  async function copyField(key: string, label: string, value: string | null): Promise<void> {
    if (!value) return;
    try {
      await navigator.clipboard.writeText(value);
      copiedKey = key;
      if (copyTimer) clearTimeout(copyTimer);
      copyTimer = setTimeout(() => {
        copiedKey = null;
      }, 1500);
    } catch {
      copiedKey = null;
      void label;
    }
  }
</script>

<div class="sample-card">
  <div class="sample-image">
    <CompareLargeImage rowId={row.id} {hasImage} alt={fileName ?? `#${row.sourceOrdinal}`} />
  </div>
  <div class="sample-info">
    <header class="sample-head">
      <h2 class="sample-name" title={fileName ?? ""}>{fileName ?? `#${row.sourceOrdinal}`}</h2>
      <p class="sample-sub tabular">
        {#each [resolution, row.time].filter(Boolean) as part, index}
          {#if index > 0}<span class="dot">·</span>{/if}{part}
        {/each}
      </p>
    </header>

    <dl class="facts">
      <div class="fact">
        <dt>画师串</dt>
        <dd>
          {#if artistLines.length > 0}
            <span class="artist-lines">{artistLines.join("  ·  ")}</span>
            <button
              type="button"
              class="mini-copy"
              title="复制画师串"
              onclick={() => void copyField("artists", "画师串", row.artists)}
            >
              {#if copiedKey === "artists"}<Check size={13} strokeWidth={1.7} />{:else}<Copy size={13} strokeWidth={1.7} />{/if}
            </button>
          {:else}
            <span class="muted">无</span>
          {/if}
        </dd>
      </div>
      <div class="fact">
        <dt>VIBE</dt>
        <dd>
          {#if row.vibeReferenceCount}
            {row.vibeReferenceCount} 个引用
          {:else}
            <span class="muted">无</span>
          {/if}
        </dd>
      </div>
      <div class="fact">
        <dt>模型</dt>
        <dd class="model-fact">
          {#if versionBadge}
            <span class="version-badge {versionBadge.className}">{versionBadge.label}</span>
          {/if}
          <span class="model-name" title={row.generationModel ?? ""}>{row.generationModel || "未知"}</span>
        </dd>
      </div>
      {#each generationEntries as entry}
        <div class="fact">
          <dt>{entry.label}</dt>
          <dd class="tabular">{entry.value}</dd>
        </div>
      {/each}
    </dl>

    {#if promptFields.length > 0}
      <div class="prompts">
        {#each promptFields as field (field.key)}
          <div class="prompt-field">
            <div class="prompt-head">
              <button type="button" class="prompt-toggle" onclick={() => togglePrompt(field.key)}>
                <span class="prompt-label">{field.label}</span>
                <span class="prompt-preview" title={field.value ?? ""}>
                  {field.value.replace(/\s+/g, " ").slice(0, 120)}
                  {#if field.value.replace(/\s+/g, " ").length > 120}…{/if}
                </span>
              </button>
              <button
                type="button"
                class="mini-copy"
                title={`复制${field.label}`}
                onclick={() => void copyField(field.key, field.label, field.value)}
              >
                {#if copiedKey === field.key}<Check size={13} strokeWidth={1.7} />{:else}<Copy size={13} strokeWidth={1.7} />{/if}
              </button>
            </div>
            {#if expandedPrompts.has(field.key)}
              <pre class="prompt-body">{field.value}</pre>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .sample-card {
    display: grid;
    grid-template-columns: minmax(220px, 320px) 1fr;
    gap: 20px;
    padding: 18px 22px 20px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }

  .sample-image {
    align-self: start;
    max-height: 340px;
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-1);
  }

  .sample-info {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
  }

  .sample-name {
    margin: 0;
    font-size: 1.25rem;
    font-weight: 700;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .sample-sub {
    margin: 2px 0 0;
    font-size: var(--font-sm);
    color: var(--text-3);
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .dot {
    color: var(--text-3);
  }

  .facts {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 22px;
    margin: 0;
  }

  .fact {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }

  .fact dt {
    font-size: var(--font-xs);
    color: var(--text-3);
    flex: none;
  }

  .fact dd {
    margin: 0;
    font-size: var(--font-sm);
    min-width: 0;
  }

  .artist-lines {
    word-break: break-all;
  }

  .model-fact {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .model-name {
    font-size: var(--font-xs);
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 340px;
  }

  .muted {
    color: var(--text-3);
  }

  .mini-copy {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border: none;
    border-radius: var(--radius-s);
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
    flex: none;
    transition:
      background var(--motion-fast) var(--ease-responsive),
      color var(--motion-fast) var(--ease-responsive);
  }

  .mini-copy:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .prompts {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .prompt-field {
    border: 1px solid var(--border);
    border-radius: var(--radius-m);
    overflow: hidden;
  }

  .prompt-head {
    display: flex;
    align-items: center;
    gap: 4px;
    padding-right: 6px;
  }

  .prompt-toggle {
    flex: 1;
    display: flex;
    align-items: baseline;
    gap: 10px;
    min-width: 0;
    border: none;
    background: transparent;
    padding: 8px 10px;
    cursor: pointer;
    font: inherit;
    color: inherit;
    text-align: left;
  }

  .prompt-toggle:hover {
    background: var(--surface-2);
  }

  .prompt-label {
    font-size: var(--font-xs);
    font-weight: 600;
    color: var(--text-2);
    flex: none;
  }

  .prompt-preview {
    font-size: var(--font-xs);
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .prompt-body {
    margin: 0;
    padding: 10px 12px;
    border-top: 1px solid var(--border);
    background: var(--surface-2);
    font-size: var(--font-sm);
    line-height: 1.6;
    white-space: pre-wrap;
    word-break: break-all;
    max-height: 220px;
    overflow: auto;
  }
</style>
