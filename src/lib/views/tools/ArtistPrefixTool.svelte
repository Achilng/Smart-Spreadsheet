<script lang="ts">
  import { emitTo, listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  import {
    applyAutoArtistPrefix,
    getArtistDictionaryStatus,
    previewAutoArtistPrefix,
    reapplyQuickArtistPrefixChanges,
    revertQuickArtistPrefixChanges,
    syncArtistDictionary,
    type ArtistDictionaryStatus,
    type ArtistDictionarySyncProgress,
    type AutoArtistCandidate,
    type AutoArtistPrefixPreview,
  } from "../../api";
  import {
    app,
    errorText,
    formatCount,
    notifyMainStateChanged,
    setNotice,
  } from "../../stores/app-state.svelte";
  import { history, recordHistory } from "../../stores/history.svelte";
  import { focusMainWindow, type ToolboxRowRequest } from "../../windows/toolbox";

  let status = $state<ArtistDictionaryStatus | null>(null);
  let progress = $state<ArtistDictionarySyncProgress | null>(null);
  let preview = $state<AutoArtistPrefixPreview | null>(null);
  let selectedNames = $state<string[]>([]);
  let search = $state("");
  let syncing = $state(false);
  let previewing = $state(false);
  let applying = $state(false);
  let loadingStatus = $state(false);
  let openingRowId = $state<number | null>(null);
  let error = $state<string | null>(null);

  const filteredCandidates = $derived(
    (preview?.candidates ?? []).filter(candidate => {
      const query = search.trim().toLocaleLowerCase();
      return !query ||
        candidate.matchName.toLocaleLowerCase().includes(query) ||
        candidate.canonicalName.toLocaleLowerCase().includes(query);
    }),
  );
  const selectedCount = $derived(selectedNames.length);
  const busy = $derived(loadingStatus || syncing || previewing || applying || history.busy || app.busy);

  onMount(() => {
    void loadStatus();
  });

  async function loadStatus(): Promise<void> {
    loadingStatus = true;
    try {
      status = await getArtistDictionaryStatus();
    } catch (cause) {
      error = errorText(cause);
    } finally {
      loadingStatus = false;
    }
  }

  async function synchronize(): Promise<void> {
    if (busy) return;
    syncing = true;
    error = null;
    progress = null;
    let unlisten: (() => void) | null = null;
    try {
      unlisten = await listen<ArtistDictionarySyncProgress>(
        "artist-dictionary://progress",
        event => {
          progress = event.payload;
        },
      );
      status = await syncArtistDictionary();
      preview = null;
      selectedNames = [];
      setNotice({
        tone: "success",
        text: `画师词典更新完成：已整理 ${formatCount(status.nameCount)} 个可识别名称。`,
      });
    } catch (cause) {
      error = errorText(cause);
    } finally {
      unlisten?.();
      progress = null;
      syncing = false;
    }
  }

  async function scanLibrary(): Promise<void> {
    if (busy) return;
    previewing = true;
    error = null;
    try {
      preview = await previewAutoArtistPrefix();
      selectedNames = preview.candidates
        .filter(candidate => !candidate.needsConfirmation)
        .map(candidate => candidate.matchName);
    } catch (cause) {
      preview = null;
      selectedNames = [];
      error = errorText(cause);
    } finally {
      previewing = false;
    }
  }

  async function applySelected(): Promise<void> {
    if (selectedNames.length === 0 || busy) return;
    applying = true;
    error = null;
    try {
      const result = await applyAutoArtistPrefix(selectedNames);
      if (result.changes.length > 0) {
        const changes = result.changes.map(change => ({ ...change }));
        recordHistory({
          label: `自动补全画师前缀（${formatCount(result.changedRows)} 张）`,
          undo: async () => {
            await revertQuickArtistPrefixChanges(changes);
            preview = null;
            selectedNames = [];
            await notifyMainStateChanged("libraryEdited");
          },
          redo: async () => {
            await reapplyQuickArtistPrefixChanges(changes);
            preview = null;
            selectedNames = [];
            await notifyMainStateChanged("libraryEdited");
          },
        });
      }
      await notifyMainStateChanged("libraryEdited");
      setNotice({
        tone: "success",
        text: result.changedRows > 0
          ? `自动修正完成：${formatCount(result.changedRows)} 张图片的 ${formatCount(result.promptFieldsChanged)} 个提示词字段已更新。`
          : "所选候选没有产生修改。",
      });
      await scanLibraryAfterApply();
    } catch (cause) {
      error = errorText(cause);
    } finally {
      applying = false;
    }
  }

  async function scanLibraryAfterApply(): Promise<void> {
    try {
      preview = await previewAutoArtistPrefix();
      selectedNames = preview.candidates
        .filter(candidate => !candidate.needsConfirmation)
        .map(candidate => candidate.matchName);
    } catch {
      preview = null;
      selectedNames = [];
    }
  }

  function toggleCandidate(name: string): void {
    selectedNames = selectedNames.includes(name)
      ? selectedNames.filter(selected => selected !== name)
      : [...selectedNames, name];
  }

  function selectRecommended(): void {
    selectedNames = (preview?.candidates ?? [])
      .filter(candidate => !candidate.needsConfirmation)
      .map(candidate => candidate.matchName);
  }

  function selectAll(): void {
    selectedNames = (preview?.candidates ?? []).map(candidate => candidate.matchName);
  }

  async function openSample(candidate: AutoArtistCandidate): Promise<void> {
    const rowId = candidate.sampleRowIds[0];
    if (!rowId || openingRowId !== null) return;
    openingRowId = rowId;
    try {
      const request: ToolboxRowRequest = { rowId };
      await emitTo("main", "toolbox://open-row", request);
      await focusMainWindow();
    } catch (cause) {
      setNotice({ tone: "error", text: `无法在主窗口打开图片：${errorText(cause)}` });
    } finally {
      openingRowId = null;
    }
  }

  function progressLabel(value: ArtistDictionarySyncProgress): string {
    const stage = {
      tags: "正在下载画师 Tag",
      artists: "正在合并画师其它名称",
      aliases: "正在下载历史别名",
      saving: "正在保存本地词典",
    }[value.stage];
    return `${stage} · ${formatCount(value.itemsFetched)} 条`;
  }

  function syncTime(value: string): string {
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? value : date.toLocaleString("zh-CN");
  }
</script>

<div class="artist-prefix-page">
  <section class="intro-card tool-card">
    <div>
      <span class="eyebrow overline">DANBOORU ARTIST DICTIONARY</span>
      <h3>自动识别裸画师 Tag</h3>
      <p>
        结合库内已有的 <code>artist:</code> 证据和 Danbooru 词典，扫描正向、角色和
        负向提示词，为没有前缀的画师名称补上 <code>artist:</code>。
      </p>
    </div>
    <button class="btn" type="button" disabled={busy} onclick={() => void synchronize()}>
      {syncing ? "更新中…" : status ? "更新画师词典" : "下载画师词典"}
    </button>
  </section>

  <section class="status-card tool-card">
    {#if loadingStatus}
      <span>正在准备内置画师词典，首次使用可能需要一些时间…</span>
    {:else if status}
      <div>
        <strong>{formatCount(status.nameCount)}</strong>
        <span>可识别名称</span>
      </div>
      <div>
        <strong>{formatCount(status.tagCount)}</strong>
        <span>画师 Tag</span>
      </div>
      <div>
        <strong>{formatCount(status.artistCount)}</strong>
        <span>含其它名称的画师</span>
      </div>
      <div class="status-time">
        <strong>{syncTime(status.syncedAt)}</strong>
        <span>最近更新</span>
      </div>
    {:else}
      <p>内置画师词典暂不可用，可联网从 Danbooru 重新同步公开的画师元数据。</p>
    {/if}
  </section>

  {#if progress}
    <div class="progress-line">
      <span class="spinner" aria-hidden="true"></span>
      <span>{progressLabel(progress)}</span>
    </div>
  {/if}

  <section class="scan-card tool-card">
    <div>
      <h3>扫描资料库</h3>
      <p>只扫描已经入库的提示词，不重新读取或改写原始图片文件。</p>
    </div>
    <button
      class="btn btn-primary"
      type="button"
      disabled={!status || busy}
      onclick={() => void scanLibrary()}
    >
      {previewing ? "扫描中…" : "扫描裸画师 Tag"}
    </button>
  </section>

  {#if error}
    <p class="error-message">{error}</p>
  {/if}

  {#if preview}
    <section class="result-card tool-card">
      <div class="metric-grid metrics">
        <div><strong>{formatCount(preview.scannedRows)}</strong><span>扫描图片</span></div>
        <div><strong>{formatCount(preview.matchedRows)}</strong><span>命中图片</span></div>
        <div><strong>{formatCount(preview.candidates.length)}</strong><span>候选名称</span></div>
        <div><strong>{formatCount(preview.promptFieldsNeedingChanges)}</strong><span>提示词字段</span></div>
      </div>

      {#if preview.candidates.length > 0}
        <div class="candidate-toolbar">
          <input type="search" bind:value={search} placeholder="搜索候选或主名称" />
          <div>
            <button type="button" class="text-button" onclick={selectRecommended}>仅选推荐项</button>
            <button type="button" class="text-button" onclick={selectAll}>全选</button>
            <button type="button" class="text-button" onclick={() => (selectedNames = [])}>清空</button>
          </div>
        </div>

        <div class="candidate-list">
          {#each filteredCandidates as candidate (candidate.matchName)}
            <div class:needs-review={candidate.needsConfirmation} class="candidate-row">
              <label>
                <input
                  type="checkbox"
                  checked={selectedNames.includes(candidate.matchName)}
                  onchange={() => toggleCandidate(candidate.matchName)}
                />
              </label>
              <div class="candidate-main">
                <div class="candidate-name">
                  <strong>{candidate.displayName}</strong>
                  {#if candidate.canonicalName.toLocaleLowerCase() !== candidate.matchName}
                    <span>归属于 {candidate.canonicalName}</span>
                  {/if}
                </div>
                <div class="badges">
                  {#if candidate.isLibraryConfirmed}<span class="badge confirmed">库内已有明确标注</span>{/if}
                  {#if candidate.isBanned}<span class="badge restricted">受限／下架仍识别</span>{/if}
                  {#if candidate.isLowUsage}<span class="badge">低使用量</span>{/if}
                  {#if candidate.isDeprecated}<span class="badge">历史 Tag</span>{/if}
                  {#if candidate.isShortName}<span class="badge warning">短名称</span>{/if}
                  {#if candidate.isCommonWord}<span class="badge warning">常见词</span>{/if}
                  {#if candidate.isAmbiguous}<span class="badge warning">身份冲突</span>{/if}
                </div>
              </div>
              <div class="candidate-stats">
                <strong>{formatCount(candidate.matchedRows)} 张</strong>
                {#if candidate.hasDanbooruMatch}
                  <span>Danbooru {formatCount(candidate.postCount)} 帖</span>
                {:else}
                  <span>仅使用库内证据</span>
                {/if}
              </div>
              <button
                type="button"
                class="sample-button"
                disabled={candidate.sampleRowIds.length === 0 || openingRowId !== null}
                onclick={() => void openSample(candidate)}
              >查看示例</button>
            </div>
          {/each}
        </div>

        <div class="apply-bar">
          <div>
            <strong>已选择 {formatCount(selectedCount)} 个候选</strong>
            <span>库内已有明确标注的画师优先选中；其余低于 20 帖的候选默认不勾选。</span>
          </div>
          <button
            class="btn btn-primary"
            type="button"
            disabled={selectedCount === 0 || busy}
            onclick={() => void applySelected()}
          >
            {applying ? "正在应用…" : "为所选画师补全前缀"}
          </button>
        </div>
      {:else}
        <div class="empty-state">
          <strong>没有发现需要补前缀的画师 Tag</strong>
          <span>已带 <code>artist:</code> 的内容和仅名称相似的 Tag 会自动跳过。</span>
        </div>
      {/if}
    </section>
  {/if}
</div>

<style>
  .artist-prefix-page {
    display: grid;
    gap: 14px;
    padding-bottom: 24px;
  }

  .intro-card,
  .scan-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    padding: 22px;
  }

  h3,
  p {
    margin: 0;
  }

  h3 {
    color: var(--text);
    font-size: var(--font-lg);
  }

  p {
    margin-top: 7px;
    color: var(--text-2);
    line-height: 1.65;
  }

  code {
    padding: 1px 5px;
    border-radius: var(--radius-s);
    background: var(--surface-2);
    color: var(--accent);
  }

  .eyebrow {
    display: block;
    margin-bottom: 7px;
    color: var(--accent);
  }

  .status-card {
    min-height: 78px;
    display: grid;
    grid-template-columns: repeat(3, minmax(110px, 1fr)) minmax(210px, 1.45fr);
    align-items: center;
    padding: 12px 18px;
  }

  .status-card > div {
    display: grid;
    gap: 3px;
    padding: 4px 16px;
    border-right: 1px solid var(--border);
  }

  .status-card > div:first-child {
    padding-left: 0;
  }

  .status-card > div:last-child {
    border-right: 0;
  }

  .status-card strong {
    color: var(--text);
    font-size: var(--font-lg);
  }

  .status-card span,
  .status-card p,
  .scan-card p {
    color: var(--text-3);
    font-size: var(--font-sm);
  }

  .status-time strong {
    font-size: var(--font-sm);
  }

  .progress-line {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 10px 13px;
    border-radius: var(--radius-m);
    background: color-mix(in srgb, var(--accent) 9%, var(--surface));
    color: var(--text-2);
    font-size: var(--font-sm);
  }

  .progress-line :global(.spinner) {
    color: var(--accent);
  }

  .error-message {
    margin: 0;
    padding: 11px 13px;
    border: 1px solid color-mix(in srgb, var(--danger) 35%, var(--border));
    border-radius: var(--radius-m);
    background: color-mix(in srgb, var(--danger) 7%, var(--surface));
    color: var(--danger);
  }

  .result-card {
    overflow: hidden;
  }

  .metrics {
    padding: 14px 16px;
    border-bottom: 1px solid var(--border);
  }

  .candidate-toolbar {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--border);
  }

  .candidate-toolbar input {
    width: min(320px, 45%);
  }

  .candidate-toolbar > div {
    display: flex;
    gap: 5px;
  }

  .text-button,
  .sample-button {
    border: 0;
    background: transparent;
    padding: 4px 8px;
    border-radius: var(--radius-full);
    color: var(--accent);
    font-size: 12.5px;
    cursor: pointer;
    transition: background var(--motion-fast) var(--ease-responsive);
  }

  .text-button:hover:not(:disabled),
  .sample-button:hover:not(:disabled) {
    background: var(--surface-2);
  }

  .candidate-list {
    max-height: 460px;
    overflow: auto;
  }

  .candidate-row {
    display: grid;
    grid-template-columns: 28px minmax(210px, 1fr) minmax(130px, auto) 78px;
    align-items: center;
    gap: 12px;
    min-height: 70px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--border);
  }

  .candidate-row.needs-review {
    background: color-mix(in srgb, var(--warning) 7%, var(--surface));
  }

  .candidate-row label {
    display: grid;
    place-items: center;
  }

  .candidate-row input[type="checkbox"] {
    width: 16px;
    height: 16px;
  }

  .candidate-main {
    min-width: 0;
    display: grid;
    gap: 7px;
  }

  .candidate-name {
    display: flex;
    align-items: baseline;
    gap: 9px;
    min-width: 0;
  }

  .candidate-name strong {
    overflow: hidden;
    color: var(--text);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .candidate-name span {
    overflow: hidden;
    color: var(--text-3);
    font-size: var(--font-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .badges {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }

  .badge {
    padding: 2px 6px;
    border-radius: var(--radius-full);
    background: var(--surface-2);
    color: var(--text-3);
    font-size: 10px;
    font-weight: 600;
  }

  .badge.warning {
    background: var(--warning-soft);
    color: var(--warning);
  }

  .badge.restricted {
    background: var(--accent-soft);
    color: var(--accent);
  }

  .badge.confirmed {
    background: var(--success-soft);
    color: var(--success);
  }

  .candidate-stats {
    display: grid;
    justify-items: end;
    gap: 3px;
  }

  .candidate-stats strong {
    color: var(--text);
  }

  .candidate-stats span {
    color: var(--text-3);
    font-size: var(--font-xs);
  }

  .sample-button {
    padding: 5px;
    font-size: var(--font-xs);
  }

  .apply-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    padding: 16px;
    background: var(--surface-2);
  }

  .apply-bar > div {
    display: grid;
    gap: 4px;
  }

  .apply-bar strong {
    color: var(--text);
  }

  .apply-bar span {
    color: var(--text-3);
    font-size: var(--font-xs);
  }

  .empty-state {
    padding: 54px 20px;
  }

  .empty-state strong {
    color: var(--text-2);
  }

  @media (max-width: 820px) {
    .status-card {
      grid-template-columns: repeat(2, 1fr);
    }

    .status-card > div:nth-child(2) {
      border-right: 0;
    }

    .candidate-row {
      grid-template-columns: 28px minmax(0, 1fr) 90px;
    }

    .sample-button {
      display: none;
    }
  }
</style>
