<script lang="ts">
  import { onMount } from "svelte";

  import {
    getCustomArtists,
    listDistinctArtists,
    setCustomArtists,
  } from "../../api";
  import { app, errorText, formatCount, setNotice } from "../../stores/app-state.svelte";

  let libraryArtists = $state<string[]>([]);
  let customText = $state("");
  let lastSaved = "";
  let useLibrary = $state(true);
  let useCustom = $state(true);
  let cleanOnly = $state(true);
  let count = $state(3);
  let result = $state("");
  let loading = $state(true);
  let loadError = $state<string | null>(null);

  let saveTimer: ReturnType<typeof setTimeout> | undefined;

  onMount(() => {
    void load();
    return () => {
      clearTimeout(saveTimer);
      void flushSave();
    };
  });

  async function load(): Promise<void> {
    loading = true;
    loadError = null;
    try {
      const [artists, custom] = await Promise.all([
        listDistinctArtists(),
        getCustomArtists(),
      ]);
      libraryArtists = artists;
      customText = custom;
      lastSaved = custom;
    } catch (e) {
      loadError = errorText(e);
    } finally {
      loading = false;
    }
  }

  function customLines(): string[] {
    return customText
      .split(/\r?\n/)
      .map(line => line.trim())
      .filter(line => line.length > 0);
  }

  /** 合并启用来源、去重、可选过滤带 :: 权重的脏片段后的画师池。 */
  const pool = $derived.by(() => {
    const set = new Set<string>();
    if (useLibrary) {
      for (const artist of libraryArtists) set.add(artist);
    }
    if (useCustom) {
      for (const artist of customLines()) set.add(artist);
    }
    let arr = [...set];
    if (cleanOnly) {
      arr = arr.filter(artist => !artist.includes("::"));
    }
    return arr;
  });

  function generate(): void {
    const arr = [...pool];
    if (arr.length === 0) {
      result = "";
      setNotice({ tone: "error", text: "画师池为空，请启用来源或填写自定义名单。" });
      return;
    }
    const n = Math.min(Math.max(1, Math.floor(count) || 1), arr.length);
    // Fisher-Yates 洗牌后取前 n 个，保证无放回。
    for (let i = arr.length - 1; i > 0; i -= 1) {
      const j = Math.floor(Math.random() * (i + 1));
      [arr[i], arr[j]] = [arr[j], arr[i]];
    }
    result = arr.slice(0, n).join(", ");
  }

  async function copyResult(): Promise<void> {
    if (!result) return;
    try {
      await navigator.clipboard.writeText(result);
      setNotice({ tone: "success", text: "已复制画师串到剪贴板。" });
    } catch {
      setNotice({ tone: "error", text: "复制失败，请检查剪贴板权限。" });
    }
  }

  function onCustomInput(event: Event): void {
    customText = (event.target as HTMLTextAreaElement).value;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => void flushSave(), 600);
  }

  async function flushSave(): Promise<void> {
    if (customText === lastSaved) return;
    const value = customText;
    try {
      await setCustomArtists(value);
      lastSaved = value;
    } catch (e) {
      setNotice({ tone: "error", text: `保存自定义名单失败：${errorText(e)}` });
    }
  }

  function close(): void {
    clearTimeout(saveTimer);
    void flushSave();
    app.artistGenOpen = false;
  }
</script>

<svelte:window
  onkeydown={event => {
    if (event.key === "Escape") close();
  }}
/>

<div class="ag-overlay">
  <div class="ag-panel">
    <header class="ag-header">
      <h3>随机画师串</h3>
      <button type="button" class="btn" onclick={close}>关闭</button>
    </header>

    <div class="ag-body">
      {#if loading}
        <p class="faint">正在加载画师池…</p>
      {:else if loadError}
        <p class="error-text">{loadError}</p>
      {:else}
        <p class="faint">从启用的来源随机抽取画师拼成提示词串，复制后可直接喂给 NovelAI。</p>

        <div class="sources">
          <label class="chk">
            <input type="checkbox" bind:checked={useLibrary} />
            库内画师（{formatCount(libraryArtists.length)} 个）
          </label>
          <label class="chk">
            <input type="checkbox" bind:checked={useCustom} />
            自定义名单
          </label>
          <label class="chk">
            <input type="checkbox" bind:checked={cleanOnly} />
            只用干净 artist: 片段（去掉带 :: 权重的）
          </label>
        </div>

        {#if useCustom}
          <div class="custom-box">
            <label class="field-label" for="ag-custom">自定义名单（一行一个，自动保存）</label>
            <textarea
              id="ag-custom"
              rows="4"
              placeholder={"artist:wlop\nartist:ask\n..."}
              value={customText}
              oninput={onCustomInput}
            ></textarea>
          </div>
        {/if}

        <div class="controls">
          <label class="field-label" for="ag-count">数量</label>
          <input
            id="ag-count"
            type="number"
            min="1"
            bind:value={count}
            class="count-input"
          />
          <span class="faint">当前池 {formatCount(pool.length)} 个画师</span>
          <button
            type="button"
            class="btn btn-primary"
            disabled={pool.length === 0}
            onclick={generate}
          >
            {result ? "再来一个" : "生成"}
          </button>
        </div>

        {#if result}
          <div class="result-box">
            <textarea readonly rows="3" value={result}></textarea>
            <button type="button" class="btn" onclick={() => void copyResult()}>复制</button>
          </div>
        {/if}
      {/if}
    </div>
  </div>
</div>

<style>
  .ag-overlay {
    position: fixed;
    inset: 0;
    z-index: 60;
    background: rgb(0 0 0 / 35%);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 40px;
  }

  .ag-panel {
    width: min(560px, 100%);
    max-height: 100%;
    background: var(--bg);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-2);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .ag-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    border-radius: var(--radius-m) var(--radius-m) 0 0;
  }

  .ag-header h3 {
    font-size: 15px;
    margin: 0;
  }

  .ag-body {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    overflow-y: auto;
    font-size: 13px;
  }

  .ag-body p {
    margin: 0;
  }

  .sources {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .chk {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text);
  }

  .custom-box {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .field-label {
    font-size: 12px;
    color: var(--text-3);
  }

  textarea {
    width: 100%;
    box-sizing: border-box;
    resize: vertical;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    background: var(--bg);
    color: var(--text);
    font-size: 13px;
    font-family: inherit;
    padding: 8px;
    outline: none;
  }

  textarea:focus {
    border-color: var(--accent);
    box-shadow: var(--focus-ring);
  }

  .controls {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .count-input {
    width: 64px;
    height: 28px;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    background: var(--bg);
    color: var(--text);
    font-size: 13px;
    padding: 0 8px;
    box-sizing: border-box;
  }

  .result-box {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .result-box .btn {
    align-self: flex-end;
  }

  .error-text {
    color: var(--danger);
  }
</style>
