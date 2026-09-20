<script lang="ts">
  import { formatCount } from "../../utils/format";
  import { history, redoLastAction, undoLastAction } from "../../stores/history.svelte";
  import Thumbnail from "../../ui/Thumbnail.svelte";
  import { createQuickEditController } from "../../features/quick-edit/controller.svelte";

  const controller = createQuickEditController();
</script>

<div class="quick-edit-page">
  <div class="operation-bar">
    <div class="operation-switcher" aria-label="快速整理操作类型">
      <button
        type="button"
        class:is-active={controller.operation === "tag"}
        onclick={() => controller.setOperation("tag")}
      >添加 Tag</button>
      <button
        type="button"
        class:is-active={controller.operation === "group"}
        onclick={() => controller.setOperation("group")}
      >批量分组</button>
      <button
        type="button"
        class:is-active={controller.operation === "artist"}
        onclick={() => controller.setOperation("artist")}
      >提示词操作</button>
    </div>
    <div class="history-actions">
      <button
        type="button"
        class="btn btn-ghost"
        disabled={history.undoCount === 0 || history.busy || controller.applying || controller.previewing}
        title={history.undoLabel ? `撤回：${history.undoLabel}` : "没有可撤回的快速整理"}
        onclick={() => void undoLastAction()}
      >
        ↶ 撤回
      </button>
      <button
        type="button"
        class="btn btn-ghost"
        disabled={history.redoCount === 0 || history.busy || controller.applying || controller.previewing}
        title={history.redoLabel ? `重做：${history.redoLabel}` : "没有可重做的快速整理"}
        onclick={() => void redoLastAction()}
      >
        ↷ 重做
      </button>
    </div>
  </div>

  <div class="editor-layout">
    <div class="rule-column">
      <section class="rule-card tool-card">
        <div class="step-heading">
          <span class="step-badge">1</span>
          <div>
            <h3>{controller.operation === "artist" ? "输入需要修正的画师名" : "输入提示词组合"}</h3>
            <p>{controller.operation === "artist"
              ? "一次处理一个画师名，不需要填写 artist: 前缀。"
              : "组合中的每一项都必须存在，顺序和位置不限。"}</p>
          </div>
        </div>

        {#if controller.operation === "artist"}
          <input
            class="artist-input"
            type="text"
            value={controller.artistName}
            maxlength="240"
            placeholder="例如：parsley_f"
            aria-label="需要添加 artist 前缀的画师名"
            oninput={controller.updateArtistName}
          />
        {:else}
          <textarea
            value={controller.promptText}
            rows="4"
            placeholder="例如：genshin, hutao（用逗号或换行分隔）"
            aria-label="必须同时存在的提示词组合，支持半角逗号、全角逗号或换行分隔"
            oninput={controller.updatePromptText}
          ></textarea>

          {#if controller.requiredTokens.length > 0}
            <div class="token-list" aria-label="已识别的提示词条件">
              {#each controller.requiredTokens as token (token)}
                <span>{token}</span>
              {/each}
            </div>
          {/if}
        {/if}

        <div class="match-rules">
          <span>扫描范围：整个资料库</span>
          <span>{controller.operation === "artist"
            ? "处理正向、角色与负向提示词；严格匹配完整 Tag 并保留权重格式"
            : "忽略大小写与 NovelAI 权重；girl / 1girl / 1 girl 视为同一项"}</span>
          {#if controller.operation === "artist"}
            <span>已带 artist: 前缀或仅名称相似的 Tag 不会修改</span>
          {/if}
        </div>
      </section>

      {#if controller.operation === "tag"}
        <section class="rule-card tag-card tool-card">
          <div class="step-heading">
            <span class="step-badge">2</span>
            <div>
              <h3>选择要添加的 Tag</h3>
              <p>可以多选；图片原有 Tag 不会被移除。</p>
            </div>
          </div>

          <div class="target-toolbar">
            <input
              class="target-search"
              type="search"
              bind:value={controller.tagSearch}
              placeholder="搜索现有 Tag"
              aria-label="搜索现有 Tag"
            />
            <button
              type="button"
              class="btn btn-ghost"
              aria-expanded={controller.tagCreatorOpen}
              disabled={controller.creatingTag || history.busy}
              onclick={() => controller.tagCreatorOpen ? controller.closeTagCreator() : void controller.openTagCreator()}
            >
              {controller.tagCreatorOpen ? "收起" : "＋ 新建"}
            </button>
          </div>

          {#if controller.tagCreatorOpen}
            <form class="create-target-panel" onsubmit={controller.createNewTag}>
              <input
                bind:this={controller.newTagInput}
                type="text"
                bind:value={controller.newTagName}
                maxlength="120"
                placeholder="输入新 Tag 名称"
                aria-label="新建 Tag 名称"
                disabled={controller.creatingTag}
                onkeydown={event => {
                  if (event.key === "Escape") {
                    event.preventDefault();
                    controller.closeTagCreator();
                  }
                }}
              />
              <div class="create-target-actions">
                <button
                  type="button"
                  class="btn btn-ghost"
                  disabled={controller.creatingTag}
                  onclick={controller.closeTagCreator}
                >取消</button>
                <button
                  type="submit"
                  class="btn"
                  disabled={!controller.newTagName.trim() || controller.creatingTag || history.busy}
                >
                  {controller.creatingTag ? "创建中…" : "创建并选中"}
                </button>
              </div>
            </form>
          {/if}

          <div class="target-list tag-list" aria-label="现有 Tag 列表">
            {#if controller.tagsLoading}
              <p class="list-state">正在读取 Tag 库…</p>
            {:else if controller.tags.length === 0}
              <p class="list-state">Tag 库为空，可以在上方直接新建。</p>
            {:else if controller.visibleTags.length === 0}
              <p class="list-state">没有匹配的 Tag。</p>
            {:else}
              {#each controller.visibleTags as tag (tag.name)}
                <button
                  type="button"
                  class:is-selected={controller.selectedTags.includes(tag.name)}
                  aria-pressed={controller.selectedTags.includes(tag.name)}
                  onclick={() => controller.toggleTag(tag.name)}
                >
                  <span class="check" aria-hidden="true"></span>
                  <strong title={tag.name}>{tag.name}</strong>
                  <small>{formatCount(tag.rowCount)}</small>
                </button>
              {/each}
            {/if}
          </div>

          {#if controller.selectedTags.length > 0}
            <div class="selected-summary">已选择 {formatCount(controller.selectedTags.length)} 个 Tag</div>
          {/if}
        </section>
      {:else if controller.operation === "group"}
        <section class="rule-card group-card tool-card">
          <div class="step-heading">
            <span class="step-badge">2</span>
            <div>
              <h3>选择目标分组</h3>
              <p>命中图片会统一移入这个分组；原分组关系将被替换。</p>
            </div>
          </div>

          <div class="target-toolbar">
            <input
              class="target-search"
              type="search"
              bind:value={controller.groupSearch}
              placeholder="搜索现有分组"
              aria-label="搜索现有分组"
            />
            <button
              type="button"
              class="btn btn-ghost"
              aria-expanded={controller.groupCreatorOpen}
              disabled={controller.creatingGroup || history.busy}
              onclick={() => controller.groupCreatorOpen ? controller.closeGroupCreator() : void controller.openGroupCreator()}
            >
              {controller.groupCreatorOpen ? "收起" : "＋ 新建"}
            </button>
          </div>

          {#if controller.groupCreatorOpen}
            <form class="create-target-panel" onsubmit={controller.createNewGroup}>
              <input
                bind:this={controller.newGroupInput}
                type="text"
                bind:value={controller.newGroupName}
                maxlength="120"
                placeholder="输入新分组名称"
                aria-label="新建分组名称"
                disabled={controller.creatingGroup}
                onkeydown={event => {
                  if (event.key === "Escape") {
                    event.preventDefault();
                    controller.closeGroupCreator();
                  }
                }}
              />
              <div class="create-target-actions">
                <button
                  type="button"
                  class="btn btn-ghost"
                  disabled={controller.creatingGroup}
                  onclick={controller.closeGroupCreator}
                >取消</button>
                <button
                  type="submit"
                  class="btn"
                  disabled={!controller.newGroupName.trim() || controller.creatingGroup || history.busy}
                >
                  {controller.creatingGroup ? "创建中…" : "创建并选中"}
                </button>
              </div>
            </form>
          {/if}

          <div class="target-list group-list" aria-label="现有分组列表">
            {#if controller.groupsLoading}
              <p class="list-state">正在读取分组…</p>
            {:else if controller.groups.length === 0}
              <p class="list-state">还没有分组，可以在上方直接新建。</p>
            {:else if controller.visibleGroups.length === 0}
              <p class="list-state">没有匹配的分组。</p>
            {:else}
              {#each controller.visibleGroups as group (group.id)}
                <button
                  type="button"
                  class:is-selected={controller.selectedGroupId === group.id}
                  aria-pressed={controller.selectedGroupId === group.id}
                  onclick={() => controller.selectGroup(group.id)}
                >
                  <span class="check" aria-hidden="true"></span>
                  <strong title={group.name}>{group.name}</strong>
                  <small>{formatCount(group.memberCount)} 张</small>
                </button>
              {/each}
            {/if}
          </div>

          {#if controller.selectedGroupId !== null}
            <div class="selected-summary">
              目标：{controller.groups.find(group => group.id === controller.selectedGroupId)?.name ?? "已删除的分组"}
            </div>
          {/if}

          <label class="group-scope-option">
            <input
              type="checkbox"
              bind:checked={controller.onlyUngrouped}
              onchange={controller.invalidatePreview}
            />
            <span>
              <strong>仅处理未分组的图片</strong>
              <small>已有任意分组的命中图片会跳过，不会从原分组移出。</small>
            </span>
          </label>
        </section>
      {/if}
    </div>

    <section class="preview-card tool-card">
      <div class="preview-heading">
        <div>
          <h3>执行预览</h3>
          <p>先扫描并确认影响范围，再执行修改。</p>
        </div>
        <button
          type="button"
          class="btn"
          disabled={!controller.canPreview}
          onclick={() => void controller.runPreview()}
        >
          {controller.previewing ? "扫描中…" : "预览匹配结果"}
        </button>
      </div>

      {#if controller.error}
        <p class="error-message">{controller.error}</p>
      {:else if controller.preview}
        <div class="metrics" class:is-artist={controller.isArtistPreview(controller.preview)}>
          {#if controller.isArtistPreview(controller.preview)}
            <div>
              <strong>{formatCount(controller.preview.scannedRows)}</strong>
              <span>扫描图片</span>
            </div>
            <div class:is-highlight={controller.preview.rowsNeedingChanges > 0}>
              <strong>{formatCount(controller.preview.rowsNeedingChanges)}</strong>
              <span>需要修正</span>
            </div>
            <div>
              <strong>{formatCount(controller.preview.promptFieldsNeedingChanges)}</strong>
              <span>涉及提示词字段</span>
            </div>
          {:else}
            <div>
              <strong>{formatCount(controller.preview.scannedRows)}</strong>
              <span>扫描图片</span>
            </div>
            <div>
              <strong>{formatCount(controller.preview.matchedRows)}</strong>
              <span>命中组合</span>
            </div>
            <div class:is-highlight={controller.preview.rowsNeedingChanges > 0}>
              <strong>{formatCount(controller.preview.rowsNeedingChanges)}</strong>
              <span>需要修改</span>
            </div>
            <div>
              <strong>
                {formatCount(
                  controller.isTagPreview(controller.preview)
                    ? controller.preview.alreadyTaggedRows
                    : controller.preview.onlyUngrouped
                      ? controller.preview.skippedGroupedRows
                      : controller.preview.alreadyInGroupRows
                )}
              </strong>
              <span>{controller.isTagPreview(controller.preview)
                ? "已有全部 Tag"
                : controller.preview.onlyUngrouped
                  ? "跳过已分组"
                  : "已在目标分组"}</span>
            </div>
          {/if}
        </div>

        {#if controller.sampleRows.length > 0}
          <div class="sample-heading">
            <strong>命中示例</strong>
            <span>最多展示 12 张，点击可在主窗口定位</span>
          </div>
          <div class="sample-grid">
            {#each controller.sampleRows as row (row.id)}
              <button
                type="button"
                title={controller.rowName(row)}
                disabled={controller.openingRowId !== null}
                onclick={() => void controller.openInMain(row.id)}
              >
                <span class="sample-image">
                  <Thumbnail
                    rowId={row.id}
                    hasImage={Boolean(row.imagePath || row.storedImagePath)}
                    alt={controller.rowName(row)}
                  />
                </span>
                <span>{controller.rowName(row)}</span>
              </button>
            {/each}
          </div>
        {:else}
          <div class="empty-preview">
            <strong>{controller.isArtistPreview(controller.preview)
              ? "没有找到需要修正的画师 Tag"
              : controller.isGroupPreview(controller.preview) &&
                  controller.preview.onlyUngrouped &&
                  controller.preview.skippedGroupedRows > 0
                ? "命中图片均已有分组"
                : "没有图片命中这个提示词组合"}</strong>
            <span>{controller.isArtistPreview(controller.preview)
              ? "已带 artist: 前缀的 Tag 会自动跳过。"
              : controller.isGroupPreview(controller.preview) &&
                  controller.preview.onlyUngrouped &&
                  controller.preview.skippedGroupedRows > 0
                ? "已按“仅处理未分组的图片”全部跳过。"
                : "除已列出的泛用别名外，空格和下划线会被严格区分。"}</span>
          </div>
        {/if}

        <div class="apply-panel">
          <div>
            {#if controller.isArtistPreview(controller.preview)}
              {#if controller.preview.rowsNeedingChanges > 0}
                将修正 {formatCount(controller.preview.rowsNeedingChanges)} 张图片中的
                {formatCount(controller.preview.promptFieldsNeedingChanges)} 个提示词字段
              {:else}
                整个资料库中没有需要修正的对应画师 Tag
              {/if}
            {:else if controller.isTagPreview(controller.preview)}
              {#if controller.preview.associationsToAdd > 0}
                将为 {formatCount(controller.preview.rowsNeedingChanges)} 张图片新增
                {formatCount(controller.preview.associationsToAdd)} 个 Tag 关联
              {:else if controller.preview.matchedRows > 0}
                命中图片已经拥有所选 Tag
              {:else}
                当前规则没有可执行的修改
              {/if}
            {:else}
              {#if controller.preview.onlyUngrouped}
                {#if controller.preview.rowsNeedingChanges > 0}
                  将把 {formatCount(controller.preview.rowsNeedingChanges)} 张未分组图片加入
                  「{controller.preview.targetGroupName}」；跳过
                  {formatCount(controller.preview.skippedGroupedRows)} 张已有分组图片
                {:else if controller.preview.skippedGroupedRows > 0}
                  命中图片均已有分组，将全部跳过
                {:else}
                  当前规则没有可执行的修改
                {/if}
              {:else}
                {#if controller.preview.rowsNeedingChanges > 0}
                  将把 {formatCount(controller.preview.rowsNeedingChanges)} 张图片移入
                  「{controller.preview.targetGroupName}」
                {:else if controller.preview.matchedRows > 0}
                  命中图片已经位于「{controller.preview.targetGroupName}」
                {:else}
                  当前规则没有可执行的修改
                {/if}
              {/if}
            {/if}
          </div>
          <button
            type="button"
            class="btn btn-primary"
            disabled={controller.preview.rowsNeedingChanges === 0 || controller.applying || history.busy}
            onclick={() => void controller.runApply()}
          >
            {controller.applying
              ? "正在应用…"
              : controller.operation === "tag"
                ? "执行打标"
                : controller.operation === "group"
                  ? "执行分组"
                  : "修正前缀"}
          </button>
        </div>

        {#if controller.lastResult}
          <p class="result-message">
            {#if controller.isArtistResult(controller.lastResult)}
              已修正 {formatCount(controller.lastResult.changedRows)} 张图片中的
              {formatCount(controller.lastResult.promptFieldsChanged)} 个提示词字段。
            {:else if controller.isTagResult(controller.lastResult)}
              已修改 {formatCount(controller.lastResult.changedRows)} 张图片，共新增
              {formatCount(controller.lastResult.associationsChanged)} 个 Tag 关联。
            {:else}
              已将 {formatCount(controller.lastResult.changedRows)} 张图片移入目标分组。{#if controller.lastResult.onlyUngrouped}
                跳过 {formatCount(controller.lastResult.skippedGroupedRows)} 张已有分组图片。
              {/if}
            {/if}
          </p>
        {/if}
      {:else}
        <div class="preview-placeholder">
          <span class="preview-icon">⌕</span>
          <strong>等待预览</strong>
          <p>{controller.operation === "tag"
            ? "输入提示词组合并选择目标 Tag 后，扫描整个资料库。"
            : controller.operation === "group"
              ? "输入提示词组合并选择目标分组后，扫描整个资料库。"
              : "输入一个画师名后，扫描整个资料库的三类提示词。"}</p>
        </div>
      {/if}
    </section>
  </div>
</div>

<style>
  .quick-edit-page {
    min-height: 100%;
    padding: 20px 24px 30px;
  }

  .operation-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 14px;
  }

  .operation-switcher {
    display: inline-flex;
    padding: 3px;
    gap: 2px;
    border-radius: var(--radius-full);
    background: var(--surface-3);
  }

  .operation-switcher button {
    min-width: 92px;
    height: 28px;
    padding: 0 16px;
    border: 0;
    border-radius: var(--radius-full);
    background: transparent;
    color: var(--text-2);
    font-size: 12.5px;
  }

  .operation-switcher button:hover:not(.is-active) {
    color: var(--text);
  }

  .operation-switcher button.is-active {
    background: var(--surface);
    color: var(--text);
    font-weight: 600;
    box-shadow: 0 1px 4px rgb(0 0 0 / 10%);
  }

  .history-actions {
    display: flex;
    gap: 2px;
  }

  .history-actions .btn {
    padding-inline: 10px;
    font-size: var(--font-sm);
  }

  .editor-layout {
    display: grid;
    grid-template-columns: minmax(300px, 0.82fr) minmax(340px, 1.18fr);
    gap: 16px;
    align-items: start;
  }

  .rule-column {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
  }

  .rule-card,
  .preview-card {
    min-width: 0;
  }

  .rule-card {
    padding: 18px;
  }

  .step-heading {
    display: flex;
    align-items: flex-start;
    gap: 11px;
    margin-bottom: 14px;
  }

  .step-heading h3,
  .preview-heading h3 {
    font-size: var(--font-lg);
  }

  .step-heading p,
  .preview-heading p {
    margin-top: 2px;
    color: var(--text-3);
    font-size: var(--font-sm);
  }

  textarea,
  .artist-input,
  .target-search,
  .create-target-panel input {
    width: 100%;
  }

  textarea {
    min-height: 88px;
    resize: vertical;
    padding: 10px 11px;
    line-height: 1.55;
  }

  textarea:focus,
  .artist-input:focus,
  .target-search:focus,
  .create-target-panel input:focus {
    border-color: var(--accent);
    box-shadow: var(--focus-ring);
  }

  .token-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 10px;
  }

  .artist-input {
    width: 100%;
    height: 39px;
    padding: 0 11px;
  }

  .token-list span {
    max-width: 100%;
    overflow: hidden;
    padding: 3px 8px;
    border-radius: var(--radius-full);
    background: var(--accent-soft);
    color: var(--accent);
    font-size: var(--font-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .match-rules {
    display: flex;
    flex-direction: column;
    gap: 3px;
    margin-top: 13px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
    color: var(--text-3);
    font-size: var(--font-xs);
  }

  .tag-card {
    padding-bottom: 13px;
  }

  .target-toolbar {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 7px;
  }

  .target-search {
    height: 35px;
    padding: 0 10px;
  }

  .target-toolbar .btn {
    min-width: 74px;
    padding-inline: 11px;
    font-size: var(--font-sm);
  }

  .create-target-panel {
    display: grid;
    gap: 9px;
    margin-top: 9px;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    background: var(--surface-2);
  }

  .create-target-panel input {
    width: 100%;
    height: 35px;
    padding: 0 10px;
  }

  .create-target-actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }

  .create-target-actions .btn {
    padding-inline: 11px;
    font-size: var(--font-sm);
  }

  .target-list {
    max-height: 210px;
    min-height: 72px;
    margin-top: 9px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
  }

  .target-list button {
    width: 100%;
    min-height: 36px;
    display: grid;
    grid-template-columns: 20px minmax(0, 1fr) auto;
    align-items: center;
    gap: 7px;
    padding: 5px 9px;
    border: 0;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    text-align: left;
  }

  .target-list button:last-child {
    border-bottom: 0;
  }

  .target-list button:hover {
    background: var(--surface-2);
  }

  .target-list button.is-selected {
    color: var(--text);
  }

  .target-list button.is-selected strong {
    font-weight: 700;
  }

  .target-list .check {
    width: 16px;
    height: 16px;
    border: 1.5px solid var(--border-strong);
    border-radius: 5px;
    background: var(--surface);
    transition:
      background var(--motion-fast) var(--ease-responsive),
      border-color var(--motion-fast) var(--ease-responsive);
  }

  .target-list button.is-selected .check {
    background-color: var(--primary);
    background-image: url('data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M4 8.5 6.8 11 12 5.5" fill="none" stroke="white" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>');
    background-position: center;
    background-size: 12px;
    background-repeat: no-repeat;
    border-color: var(--primary);
  }

  .target-list strong {
    overflow: hidden;
    font-size: var(--font-sm);
    font-weight: 550;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .target-list small {
    color: var(--text-3);
    font-size: var(--font-xs);
  }

  .list-state {
    display: grid;
    min-height: 72px;
    place-items: center;
    padding: 12px;
    color: var(--text-3);
    font-size: var(--font-sm);
    text-align: center;
  }

  .selected-summary {
    margin-top: 7px;
    color: var(--accent);
    font-size: var(--font-xs);
  }

  .group-scope-option {
    display: flex;
    align-items: flex-start;
    gap: 9px;
    margin-top: 12px;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    background: var(--surface-2);
    color: var(--text-2);
    cursor: pointer;
  }

  .group-scope-option input {
    flex: none;
    margin-top: 1px;
  }

  .group-scope-option span,
  .group-scope-option strong,
  .group-scope-option small {
    display: block;
  }

  .group-scope-option strong {
    font-size: var(--font-sm);
  }

  .group-scope-option small {
    margin-top: 2px;
    color: var(--text-3);
    font-size: var(--font-xs);
    line-height: 1.4;
  }

  .preview-card {
    min-height: 480px;
    display: flex;
    flex-direction: column;
    padding: 18px;
  }

  .preview-heading {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 14px;
  }

  .preview-heading .btn {
    padding-inline: 12px;
    font-size: var(--font-sm);
  }

  .metrics {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 8px;
    margin-top: 17px;
  }

  .metrics.is-artist {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .metrics > div {
    min-width: 0;
    padding: 10px;
    border-radius: var(--radius-s);
    background: var(--surface-2);
  }

  .metrics > div.is-highlight {
    background: var(--accent-soft);
  }

  .metrics strong,
  .metrics span {
    display: block;
  }

  .metrics strong {
    overflow: hidden;
    font-size: var(--font-lg);
    text-overflow: ellipsis;
  }

  .metrics span {
    margin-top: 1px;
    color: var(--text-3);
    font-size: var(--font-xs);
    white-space: nowrap;
  }

  .sample-heading {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
    margin: 17px 1px 8px;
    font-size: var(--font-sm);
  }

  .sample-heading span {
    color: var(--text-3);
    font-size: var(--font-xs);
  }

  .sample-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 7px;
  }

  .sample-grid button {
    min-width: 0;
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    background: var(--surface);
    text-align: left;
  }

  .sample-grid button:hover:not(:disabled) {
    border-color: var(--accent);
  }

  .sample-image {
    height: 72px;
    display: block;
    overflow: hidden;
    background: var(--surface-3);
  }

  .sample-grid button > span:last-child {
    display: block;
    overflow: hidden;
    padding: 5px 6px;
    color: var(--text-2);
    font-size: var(--font-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .apply-panel {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    margin-top: auto;
    padding-top: 16px;
    border-top: 1px solid var(--border);
    color: var(--text-2);
    font-size: var(--font-sm);
  }

  .result-message,
  .error-message {
    margin-top: 10px;
    padding: 9px 11px;
    border-radius: var(--radius-s);
    font-size: var(--font-sm);
  }

  .result-message {
    background: var(--success-soft);
    color: var(--success);
  }

  .error-message {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .preview-placeholder,
  .empty-preview {
    flex: 1;
    min-height: 230px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: var(--text-3);
    text-align: center;
  }

  .preview-placeholder strong,
  .empty-preview strong {
    color: var(--text-2);
    font-size: var(--font-md);
  }

  .preview-placeholder p,
  .empty-preview span {
    max-width: 300px;
    margin-top: 3px;
    font-size: var(--font-sm);
  }

  .preview-icon {
    width: 40px;
    height: 40px;
    display: grid;
    place-items: center;
    margin-bottom: 8px;
    border-radius: var(--radius-m);
    background: var(--surface-3);
    color: var(--text-3);
    font-size: 20px;
  }

  @media (max-width: 840px) {
    .quick-edit-page {
      padding: 16px;
    }

    .editor-layout {
      grid-template-columns: 1fr;
    }

    .preview-card {
      min-height: 430px;
    }
  }

  @media (max-width: 680px) {
    .operation-bar {
      align-items: flex-start;
      flex-direction: column;
    }

    .metrics {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
