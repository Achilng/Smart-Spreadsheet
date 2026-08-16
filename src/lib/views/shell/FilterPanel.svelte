<script lang="ts">
  import X from "@lucide/svelte/icons/x";

  import {
    listDistinctArtists,
    type FilterNumericComparison,
    type FilterNumericOperator,
    type LibraryFilter,
  } from "../../api";
  import { app, errorText } from "../../stores/app-state.svelte";
  import { groupStore } from "../../stores/group-store.svelte";
  import { rowStore, setLibraryFilters } from "../../stores/row-store.svelte";
  import { clearSelection } from "../../stores/selection-store.svelte";
  import { tagStore } from "../../stores/tag-store.svelte";
  import Modal from "../../ui/Modal.svelte";
  import { cloneLibraryFilters } from "../../utils/library-filters";

  type OptionalMode<T extends string> = "any" | T;

  interface Draft {
    tagMode: OptionalMode<"hasAll" | "hasAny" | "hasNone" | "isEmpty">;
    tagValues: string[];
    tagSearch: string;
    groupMode: OptionalMode<"is" | "isNot" | "isEmpty">;
    groupId: number | null;
    artistMode: OptionalMode<"containsAny" | "containsNone" | "isSingle" | "isMultiple" | "isEmpty">;
    artistText: string;
    vibeMode: OptionalMode<"hasAny" | "hasNone" | "count">;
    vibeComparison: FilterNumericComparison;
    noteMode: OptionalMode<"contains" | "isEmpty" | "isNotEmpty">;
    noteText: string;
    metadataMode: "any" | "parsed" | "failed";
    orientation: "any" | "landscape" | "portrait" | "square";
    dimensionField: "any" | "width" | "height" | "aspectRatio";
    dimensionComparison: FilterNumericComparison;
    generationTextField: "any" | "model" | "sampler" | "noiseSchedule" | "seed";
    generationTextOperator: "contains" | "equals";
    generationTextValue: string;
    generationNumberField: "any" | "steps" | "scale" | "cfgRescale";
    generationNumberComparison: FilterNumericComparison;
  }

  const comparison = (): FilterNumericComparison => ({ operator: "equal", value: 0, secondValue: null });

  function emptyDraft(): Draft {
    return {
      tagMode: "any",
      tagValues: [],
      tagSearch: "",
      groupMode: "any",
      groupId: null,
      artistMode: "any",
      artistText: "",
      vibeMode: "any",
      vibeComparison: comparison(),
      noteMode: "any",
      noteText: "",
      metadataMode: "any",
      orientation: "any",
      dimensionField: "any",
      dimensionComparison: comparison(),
      generationTextField: "any",
      generationTextOperator: "contains",
      generationTextValue: "",
      generationNumberField: "any",
      generationNumberComparison: comparison(),
    };
  }

  let draft = $state<Draft>(emptyDraft());
  let error = $state<string | null>(null);
  let artists = $state<string[]>([]);
  let artistsLoading = $state(false);
  let initialized = false;

  const filteredTags = $derived.by(() => {
    const needle = draft.tagSearch.trim().toLocaleLowerCase();
    return tagStore.list.filter(tag => needle === "" || tag.name.toLocaleLowerCase().includes(needle));
  });

  function hydrate(): void {
    draft = emptyDraft();
    for (const filter of rowStore.filters) {
      switch (filter.type) {
        case "tag": draft.tagMode = filter.operator; draft.tagValues = [...filter.values]; break;
        case "group": draft.groupMode = filter.operator; draft.groupId = filter.groupId; break;
        case "artist": draft.artistMode = filter.operator; draft.artistText = filter.values.join(", "); break;
        case "vibe": draft.vibeMode = filter.operator; if (filter.comparison) draft.vibeComparison = { ...filter.comparison }; break;
        case "note": draft.noteMode = filter.operator; draft.noteText = filter.value; break;
        case "metadata": draft.metadataMode = filter.parsed ? "parsed" : "failed"; break;
        case "orientation": draft.orientation = filter.orientation; break;
        case "imageDimension": draft.dimensionField = filter.field; draft.dimensionComparison = { ...filter.comparison }; break;
        case "generationText": draft.generationTextField = filter.field; draft.generationTextOperator = filter.operator; draft.generationTextValue = filter.value; break;
        case "generationNumber": draft.generationNumberField = filter.field; draft.generationNumberComparison = { ...filter.comparison }; break;
      }
    }
    error = null;
  }

  $effect(() => {
    if (app.filterOpen && !initialized) {
      initialized = true;
      hydrate();
      if (artists.length === 0 && !artistsLoading) {
        artistsLoading = true;
        void listDistinctArtists()
          .then(values => { artists = values; })
          .catch(cause => { error = `画师列表加载失败：${errorText(cause)}`; })
          .finally(() => { artistsLoading = false; });
      }
    } else if (!app.filterOpen) {
      initialized = false;
    }
  });

  function close(): void {
    app.filterOpen = false;
  }

  function toggleTag(name: string): void {
    draft.tagValues = draft.tagValues.includes(name)
      ? draft.tagValues.filter(value => value !== name)
      : [...draft.tagValues, name];
  }

  function splitValues(value: string): string[] {
    return [...new Set(value.split(/[,，\n\r]/).map(item => item.trim()).filter(Boolean))];
  }

  function validComparison(value: FilterNumericComparison): boolean {
    return Number.isFinite(value.value) && (value.operator !== "between" || Number.isFinite(value.secondValue));
  }

  function buildFilters(): LibraryFilter[] | null {
    const filters: LibraryFilter[] = [];
    if (draft.tagMode !== "any") {
      if (draft.tagMode !== "isEmpty" && draft.tagValues.length === 0) {
        error = "请选择至少一个 Tag。";
        return null;
      }
      filters.push({ type: "tag", operator: draft.tagMode, values: [...draft.tagValues] });
    }
    if (draft.groupMode !== "any") {
      if (draft.groupMode !== "isEmpty" && draft.groupId === null) {
        error = "请选择一个分组。";
        return null;
      }
      filters.push({ type: "group", operator: draft.groupMode, groupId: draft.groupMode === "isEmpty" ? null : draft.groupId });
    }
    if (draft.artistMode !== "any") {
      const values = splitValues(draft.artistText);
      if (["containsAny", "containsNone"].includes(draft.artistMode) && values.length === 0) {
        error = "请输入至少一个画师名。";
        return null;
      }
      filters.push({ type: "artist", operator: draft.artistMode, values });
    }
    if (draft.vibeMode !== "any") {
      if (draft.vibeMode === "count" && !validComparison(draft.vibeComparison)) {
        error = "请填写有效的 VIBE 数量。";
        return null;
      }
      filters.push({ type: "vibe", operator: draft.vibeMode, comparison: draft.vibeMode === "count" ? { ...draft.vibeComparison } : null });
    }
    if (draft.noteMode !== "any") {
      if (draft.noteMode === "contains" && draft.noteText.trim() === "") {
        error = "请输入要在备注中查找的内容。";
        return null;
      }
      filters.push({ type: "note", operator: draft.noteMode, value: draft.noteText.trim(), caseSensitive: false });
    }
    if (draft.metadataMode !== "any") filters.push({ type: "metadata", parsed: draft.metadataMode === "parsed" });
    if (draft.orientation !== "any") filters.push({ type: "orientation", orientation: draft.orientation });
    if (draft.dimensionField !== "any") {
      if (!validComparison(draft.dimensionComparison)) {
        error = "请填写有效的图片尺寸或比例。";
        return null;
      }
      filters.push({ type: "imageDimension", field: draft.dimensionField, comparison: { ...draft.dimensionComparison } });
    }
    if (draft.generationTextField !== "any") {
      if (draft.generationTextValue.trim() === "") {
        error = "请输入要匹配的生成参数内容。";
        return null;
      }
      filters.push({ type: "generationText", field: draft.generationTextField, operator: draft.generationTextOperator, value: draft.generationTextValue.trim(), caseSensitive: false });
    }
    if (draft.generationNumberField !== "any") {
      if (!validComparison(draft.generationNumberComparison)) {
        error = "请填写有效的生成数值。";
        return null;
      }
      filters.push({ type: "generationNumber", field: draft.generationNumberField, comparison: { ...draft.generationNumberComparison } });
    }
    return filters;
  }

  function apply(): void {
    error = null;
    const filters = buildFilters();
    if (!filters) return;
    setLibraryFilters(cloneLibraryFilters(filters));
    clearSelection();
    close();
  }

  function clearFilters(): void {
    setLibraryFilters([]);
    clearSelection();
    close();
  }

  function patchComparison(target: "vibe" | "dimension" | "generation", values: Partial<FilterNumericComparison>): void {
    const key = target === "vibe" ? "vibeComparison" : target === "dimension" ? "dimensionComparison" : "generationNumberComparison";
    draft[key] = { ...draft[key], ...values } as FilterNumericComparison;
  }
</script>

<Modal open={app.filterOpen} onclose={close} labelledby="filter-panel-title" width="520px">
  <div class="filter-panel">
    <header class="panel-head">
      <div>
        <h2 id="filter-panel-title">过滤</h2>
        <p>图片需同时满足所有已选择的条件</p>
      </div>
      <button type="button" class="close-btn" title="关闭" aria-label="关闭过滤面板" onclick={close}><X size={20} /></button>
    </header>

    <div class="panel-body">
      <section class="filter-section">
        <div class="section-copy"><b>Tag</b><span>按资料库 Tag 过滤</span></div>
        <select bind:value={draft.tagMode}>
          <option value="any">任何 Tag 状态</option><option value="hasAll">拥有全部所选 Tag</option>
          <option value="hasAny">拥有任意所选 Tag</option><option value="hasNone">不拥有所选 Tag</option><option value="isEmpty">没有任何 Tag</option>
        </select>
        {#if draft.tagMode !== "any" && draft.tagMode !== "isEmpty"}
          <div class="choice-box">
            <input class="choice-search" placeholder="搜索 Tag" bind:value={draft.tagSearch} />
            <div class="choice-list">
              {#each filteredTags as tag (tag.name)}
                <label class="choice"><input type="checkbox" checked={draft.tagValues.includes(tag.name)} onchange={() => toggleTag(tag.name)} /><span>{tag.name}</span><small>{tag.rowCount}</small></label>
              {/each}
            </div>
          </div>
        {/if}
      </section>

      <section class="filter-section">
        <div class="section-copy"><b>分组</b><span>按图片所属分组过滤</span></div>
        <div class="inline-fields">
          <select bind:value={draft.groupMode}><option value="any">任何分组状态</option><option value="is">属于</option><option value="isNot">不属于</option><option value="isEmpty">尚未分组</option></select>
          {#if draft.groupMode === "is" || draft.groupMode === "isNot"}
            <select bind:value={draft.groupId}><option value={null}>选择分组</option>{#each groupStore.list as group}<option value={group.id}>{group.name}</option>{/each}</select>
          {/if}
        </div>
      </section>

      <section class="filter-section">
        <div class="section-copy"><b>画师</b><span>按画师名称或画师数量过滤</span></div>
        <select bind:value={draft.artistMode}><option value="any">任何画师状态</option><option value="containsAny">包含任意指定画师</option><option value="containsNone">不包含指定画师</option><option value="isSingle">单画师</option><option value="isMultiple">多画师</option><option value="isEmpty">没有画师</option></select>
        {#if draft.artistMode === "containsAny" || draft.artistMode === "containsNone"}
          <input list="filter-artist-options" placeholder="输入画师名；多个名称用逗号分隔" bind:value={draft.artistText} />
          <datalist id="filter-artist-options">{#each artists as artist}<option value={artist}></option>{/each}</datalist>
        {/if}
      </section>

      <section class="filter-section">
        <div class="section-copy"><b>VIBE</b><span>按 NovelAI VIBE 引用过滤</span></div>
        <select bind:value={draft.vibeMode}><option value="any">任何 VIBE 状态</option><option value="hasAny">存在 VIBE</option><option value="hasNone">不存在 VIBE</option><option value="count">按 VIBE 数量</option></select>
        {#if draft.vibeMode === "count"}
          <div class="numeric-row">
            <select value={draft.vibeComparison.operator} onchange={e => patchComparison("vibe", { operator: (e.currentTarget as HTMLSelectElement).value as FilterNumericOperator })}><option value="equal">等于</option><option value="notEqual">不等于</option><option value="greaterThan">大于</option><option value="greaterOrEqual">大于等于</option><option value="lessThan">小于</option><option value="lessOrEqual">小于等于</option><option value="between">介于</option></select>
            <input type="number" min="0" value={draft.vibeComparison.value} oninput={e => patchComparison("vibe", { value: (e.currentTarget as HTMLInputElement).valueAsNumber })} />
            {#if draft.vibeComparison.operator === "between"}<input type="number" min="0" placeholder="到" value={draft.vibeComparison.secondValue ?? ""} oninput={e => patchComparison("vibe", { secondValue: (e.currentTarget as HTMLInputElement).valueAsNumber })} />{/if}
          </div>
        {/if}
      </section>

      <section class="filter-section">
        <div class="section-copy"><b>备注</b><span>按备注是否存在或包含内容过滤</span></div>
        <select bind:value={draft.noteMode}><option value="any">任何备注状态</option><option value="contains">备注包含</option><option value="isNotEmpty">有备注</option><option value="isEmpty">无备注</option></select>
        {#if draft.noteMode === "contains"}<input placeholder="输入要查找的备注文字" bind:value={draft.noteText} />{/if}
      </section>

      <section class="filter-section compact-pair">
        <div><div class="section-copy"><b>元数据</b><span>NovelAI 元数据解析状态</span></div><select bind:value={draft.metadataMode}><option value="any">任何状态</option><option value="parsed">解析成功</option><option value="failed">解析失败</option></select></div>
        <div><div class="section-copy"><b>构图</b><span>根据图片宽高判断</span></div><select bind:value={draft.orientation}><option value="any">任何构图</option><option value="landscape">横图</option><option value="portrait">竖图</option><option value="square">正方形</option></select></div>
      </section>

      <section class="filter-section">
        <div class="section-copy"><b>图片尺寸</b><span>按宽度、高度或宽高比过滤</span></div>
        <div class="numeric-row">
          <select bind:value={draft.dimensionField}><option value="any">不限尺寸</option><option value="width">宽度</option><option value="height">高度</option><option value="aspectRatio">宽高比</option></select>
          {#if draft.dimensionField !== "any"}
            <select value={draft.dimensionComparison.operator} onchange={e => patchComparison("dimension", { operator: (e.currentTarget as HTMLSelectElement).value as FilterNumericOperator })}><option value="equal">等于</option><option value="notEqual">不等于</option><option value="greaterThan">大于</option><option value="greaterOrEqual">大于等于</option><option value="lessThan">小于</option><option value="lessOrEqual">小于等于</option><option value="between">介于</option></select>
            <input type="number" min="0" step={draft.dimensionField === "aspectRatio" ? "0.01" : "1"} value={draft.dimensionComparison.value} oninput={e => patchComparison("dimension", { value: (e.currentTarget as HTMLInputElement).valueAsNumber })} />
            {#if draft.dimensionComparison.operator === "between"}<input type="number" min="0" placeholder="到" value={draft.dimensionComparison.secondValue ?? ""} oninput={e => patchComparison("dimension", { secondValue: (e.currentTarget as HTMLInputElement).valueAsNumber })} />{/if}
          {/if}
        </div>
      </section>

      <section class="filter-section">
        <div class="section-copy"><b>生成参数（文字）</b><span>模型、采样器、噪声调度或种子</span></div>
        <div class="inline-fields">
          <select bind:value={draft.generationTextField}><option value="any">不限文字参数</option><option value="model">模型</option><option value="sampler">采样器</option><option value="noiseSchedule">噪声调度</option><option value="seed">种子</option></select>
          {#if draft.generationTextField !== "any"}<select bind:value={draft.generationTextOperator}><option value="contains">包含</option><option value="equals">完全一致</option></select>{/if}
        </div>
        {#if draft.generationTextField !== "any"}<input placeholder="输入要匹配的内容" bind:value={draft.generationTextValue} />{/if}
      </section>

      <section class="filter-section">
        <div class="section-copy"><b>生成参数（数值）</b><span>步数、Prompt Guidance 或 CFG Rescale</span></div>
        <div class="numeric-row">
          <select bind:value={draft.generationNumberField}><option value="any">不限数值参数</option><option value="steps">步数</option><option value="scale">Prompt Guidance</option><option value="cfgRescale">CFG Rescale</option></select>
          {#if draft.generationNumberField !== "any"}
            <select value={draft.generationNumberComparison.operator} onchange={e => patchComparison("generation", { operator: (e.currentTarget as HTMLSelectElement).value as FilterNumericOperator })}><option value="equal">等于</option><option value="notEqual">不等于</option><option value="greaterThan">大于</option><option value="greaterOrEqual">大于等于</option><option value="lessThan">小于</option><option value="lessOrEqual">小于等于</option><option value="between">介于</option></select>
            <input type="number" min="0" step="any" value={draft.generationNumberComparison.value} oninput={e => patchComparison("generation", { value: (e.currentTarget as HTMLInputElement).valueAsNumber })} />
            {#if draft.generationNumberComparison.operator === "between"}<input type="number" min="0" placeholder="到" value={draft.generationNumberComparison.secondValue ?? ""} oninput={e => patchComparison("generation", { secondValue: (e.currentTarget as HTMLInputElement).valueAsNumber })} />{/if}
          {/if}
        </div>
      </section>

      {#if error}<p class="panel-error" role="alert">{error}</p>{/if}
    </div>

    <footer class="panel-footer">
      <button type="button" class="clear-btn" onclick={clearFilters}>清除过滤</button>
      <span></span>
      <button type="button" class="secondary-btn" onclick={close}>取消</button>
      <button type="button" class="apply-btn" onclick={apply}>应用过滤</button>
    </footer>
  </div>
</Modal>

<style>
  .filter-panel { min-height: 0; display: flex; flex-direction: column; background: var(--surface); }
  .panel-head { flex: none; display: flex; align-items: flex-start; justify-content: space-between; padding: 20px 22px 16px; border-bottom: 1px solid var(--border); }
  .panel-head h2 { margin: 0; font-size: 21px; }
  .panel-head p { margin: 4px 0 0; color: var(--text-3); font-size: var(--font-sm); }
  .close-btn { width: 34px; height: 34px; display: grid; place-items: center; border: 0; border-radius: var(--radius-s); background: transparent; color: var(--text-3); }
  .close-btn:hover { background: var(--surface-2); color: var(--text); }
  .panel-body { min-height: 0; overflow-y: auto; padding: 4px 22px 18px; }
  .filter-section { display: grid; gap: 9px; padding: 15px 0; border-bottom: 1px solid var(--border); }
  .section-copy { display: grid; gap: 2px; }
  .filter-section b { color: var(--text); font-size: var(--font-md); }
  .section-copy > span { color: var(--text-3); font-size: var(--font-xs); }
  select, input { width: 100%; min-height: 38px; padding: 0 10px; border: 1px solid var(--border-strong); border-radius: var(--radius-s); background: var(--surface-2); color: var(--text); font: inherit; }
  select:focus, input:focus { border-color: var(--primary); outline: none; }
  .inline-fields, .numeric-row { display: flex; gap: 8px; }
  .inline-fields > *, .numeric-row > * { flex: 1; min-width: 0; }
  .numeric-row input { max-width: 112px; }
  .compact-pair { grid-template-columns: 1fr 1fr; gap: 16px; }
  .compact-pair > div { display: grid; gap: 9px; }
  .choice-box { overflow: hidden; border: 1px solid var(--border); border-radius: var(--radius-s); background: var(--surface-2); }
  .choice-search { border: 0; border-bottom: 1px solid var(--border); border-radius: 0; background: var(--surface); }
  .choice-list { max-height: 150px; overflow-y: auto; padding: 5px; }
  .choice { min-height: 30px; padding: 0 6px; display: flex; align-items: center; gap: 8px; border-radius: 5px; color: var(--text-2); font-size: var(--font-sm); }
  .choice:hover { background: var(--surface-3); }
  .choice input { width: 15px; min-height: 15px; padding: 0; }
  .choice span { flex: 1; }
  .choice small { color: var(--text-4); }
  .panel-error { margin: 14px 0 0; padding: 9px 11px; border-radius: var(--radius-s); background: var(--danger-soft); color: var(--danger); font-size: var(--font-sm); }
  .panel-footer { flex: none; display: grid; grid-template-columns: auto 1fr auto auto; align-items: center; gap: 8px; padding: 14px 22px 18px; border-top: 1px solid var(--border); }
  .panel-footer button { min-height: 36px; padding: 0 15px; border: 0; border-radius: var(--radius-s); font: inherit; font-weight: 650; }
  .clear-btn { background: transparent; color: var(--accent); padding-left: 0 !important; }
  .secondary-btn { background: var(--surface-3); color: var(--text); }
  .apply-btn { background: var(--primary); color: white; }
  .apply-btn:hover { filter: brightness(1.06); }
  @media (max-width: 620px) { .compact-pair { grid-template-columns: 1fr; } .inline-fields, .numeric-row { flex-wrap: wrap; } .numeric-row input { max-width: none; } }
</style>
