<script lang="ts">
  import { errorText, formatCount } from "../../utils/format";
  import { setNotice } from "../../stores/notices.svelte";
  import { onMount } from "svelte";
  import Dices from "@lucide/svelte/icons/dices";

  import {
    getCustomArtists,
    listDistinctArtists,
    setCustomArtists,
  } from "../../api";

  import { softFade, softFly } from "../../ui/motion";

  let libraryArtists = $state<string[]>([]);
  let customText = $state("");
  let lastSaved = "";
  let useLibrary = $state(true);
  let useCustom = $state(true);
  let cleanOnly = $state(true);
  let count = $state(3);
  let result = $state("");
  let drawVersion = $state(0);
  let resultField = $state<HTMLTextAreaElement>();
  let diceIcon = $state<HTMLSpanElement>();
  let loading = $state(true);
  let loadError = $state<string | null>(null);

  let saveTimer: ReturnType<typeof setTimeout> | undefined;

  // 复用输出框，只动画文字透明度和图标；相同结果也能重播，连点时取消上一轮。
  $effect(() => {
    if (drawVersion === 0 || !resultField || !diceIcon) return;
    const preference = window.matchMedia("(prefers-reduced-motion: reduce)");
    if (preference.matches) return;
    const animations = [
      resultField.animate([{ opacity: 0.55 }, { opacity: 1 }], {
        duration: 280,
        easing: "ease-out",
      }),
      diceIcon.animate([{ transform: "rotate(0deg)" }, { transform: "rotate(360deg)" }], {
        duration: 420,
        easing: "cubic-bezier(0.22, 1, 0.36, 1)",
      }),
    ];
    const cancel = (): void => animations.forEach(animation => animation.cancel());
    preference.addEventListener("change", cancel);
    return () => {
      cancel();
      preference.removeEventListener("change", cancel);
    };
  });

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
    drawVersion += 1;
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

</script>

<div class="ag-page">
    <div class="ag-body tool-card">
      {#if loading}
        <p class="faint" transition:softFade={{ duration: 130 }}>正在加载画师池…</p>
      {:else if loadError}
        <p class="error-text" transition:softFly={{ duration: 150, y: 4 }}>{loadError}</p>
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
          <div class="custom-box" transition:softFly={{ duration: 175, y: 6 }}>
            <label class="field-label overline" for="ag-custom">自定义名单（一行一个，自动保存）</label>
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
          <label class="field-label overline" for="ag-count">数量</label>
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
            class="btn btn-primary generate-btn"
            disabled={pool.length === 0}
            onclick={generate}
          >
            <span class="dice-icon" bind:this={diceIcon} aria-hidden="true"><Dices size={16} /></span>
            {result ? "再来一个" : "生成"}
          </button>
        </div>

        <div class="result-box">
          <textarea
            bind:this={resultField}
            aria-label="随机画师串结果"
            readonly
            rows="3"
            placeholder="点击生成，随机画师串会显示在这里"
            value={result}
          ></textarea>
          <button type="button" class="btn" disabled={!result} onclick={() => void copyResult()}>复制</button>
        </div>
      {/if}
    </div>
  </div>

<style>
  .ag-page {
    min-height: 100%;
    padding: 24px 28px 32px;
  }

  .ag-body {
    width: min(680px, 100%);
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    font-size: var(--font-md);
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
    color: var(--text-3);
  }

  textarea {
    width: 100%;
    box-sizing: border-box;
    resize: vertical;
    font-size: var(--font-md);
    font-family: inherit;
    padding: 8px;
    outline: none;
  }

  .controls {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .count-input {
    width: 64px;
    height: 32px;
    font-size: var(--font-md);
    padding: 0 8px;
    box-sizing: border-box;
  }

  .result-box {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .generate-btn {
    min-width: 112px;
  }

  .dice-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .result-box .btn {
    align-self: flex-end;
  }

  .error-text {
    color: var(--danger);
  }
</style>
