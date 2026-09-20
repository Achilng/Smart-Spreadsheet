<script lang="ts">
  import { flip } from "svelte/animate";
  import { flipDuration, softFade } from "../../ui/motion";
  import { createPromptDocsController } from "../../features/prompt-docs/controller.svelte";

  const controller = createPromptDocsController();
</script>

<section class="prompt-docs">
  <aside class="doc-list">
    <div class="doc-list-header">
      <h2>提示词文档</h2>
      <button type="button" class="btn btn-primary" disabled={controller.loading} onclick={() => void controller.createNewDoc()}>
        新建
      </button>
    </div>

    <input
      class="doc-search"
      type="text"
      placeholder="搜索标题和正文…"
      bind:value={controller.search}
    />

    <div class="doc-items">
      {#if controller.filteredDocs.length === 0}
        <p class="empty-list">{controller.search.trim() ? "没有匹配的文档" : "还没有提示词文档"}</p>
      {:else}
        {#each controller.filteredDocs as doc (doc.id)}
          <button
            type="button"
            class="doc-item"
            class:is-active={controller.activeDoc?.id === doc.id}
            onclick={() => void controller.selectDoc(doc.id)}
            animate:flip={{ duration: flipDuration(170) }}
            transition:softFade={{ duration: 130 }}
          >
            <span class="doc-title">{doc.title}</span>
            <span class="doc-date">{doc.updatedAt}</span>
            {#if doc.plainText.trim()}
              <span class="doc-snippet">{doc.plainText}</span>
            {/if}
          </button>
        {/each}
      {/if}
    </div>
  </aside>

  <main class="doc-editor">
    {#if controller.switchingDoc}
      <div class="document-progress" aria-hidden="true" transition:softFade={{ duration: 100 }}></div>
    {/if}
    <div class="editor-titlebar">
      <input
        class="title-input"
        type="text"
        value={controller.title}
        placeholder="未命名文档"
        disabled={!controller.activeDoc || controller.loading}
        oninput={controller.onTitleInput}
      />
      <span class:error={controller.saveState === "error"} class="save-status">
        {#key controller.statusText}
          <span transition:softFade={{ duration: 120 }}>
            {#if controller.saveState === "saving"}
              <i class="save-spinner" aria-hidden="true"></i>
            {:else if controller.saveState === "saved"}
              <i class="save-check" aria-hidden="true">✓</i>
            {:else if controller.saveState === "dirty"}
              <i class="save-dot" aria-hidden="true"></i>
            {/if}
            {controller.statusText}
          </span>
        {/key}
      </span>
      <button type="button" class="btn" disabled={!controller.activeDoc || controller.loading} onclick={() => void controller.copyPlainText()}>
        复制全文纯文本
      </button>
      <button type="button" class="btn btn-danger" disabled={!controller.activeDoc || controller.loading} onclick={() => void controller.deleteActiveDoc()}>
        删除
      </button>
    </div>

    <div class="toolbar" aria-label="提示词文档工具栏">
      <button type="button" disabled={!controller.activeDoc} class:is-active={controller.isActive("paragraph")} onclick={() => controller.editor?.chain().focus().setParagraph().run()}>
        正文
      </button>
      <button type="button" disabled={!controller.activeDoc} class:is-active={controller.isActive("heading", { level: 2 })} onclick={() => controller.editor?.chain().focus().toggleHeading({ level: 2 }).run()}>
        标题
      </button>
      <button type="button" disabled={!controller.activeDoc} class:is-active={controller.isActive("bold")} onclick={() => controller.editor?.chain().focus().toggleBold().run()}>
        B
      </button>
      <button type="button" disabled={!controller.activeDoc} class:is-active={controller.isActive("italic")} onclick={() => controller.editor?.chain().focus().toggleItalic().run()}>
        I
      </button>
      <button type="button" disabled={!controller.activeDoc} class:is-active={controller.isActive("bulletList")} onclick={() => controller.editor?.chain().focus().toggleBulletList().run()}>
        列表
      </button>
      <button type="button" disabled={!controller.activeDoc || controller.loading} onclick={() => void controller.chooseImageFiles()}>
        插入图片
      </button>
    </div>

    <div class="editor-shell">
      <div class="editor-surface" class:is-disabled={!controller.activeDoc} class:is-switching={controller.switchingDoc}>
        <div class="editor-content" bind:this={controller.editorElement}></div>
        {#if !controller.activeDoc}
          <div class="editor-empty">
            <h3>创建一个提示词文档</h3>
            <button type="button" class="btn btn-primary" disabled={controller.loading} onclick={() => void controller.createNewDoc()}>
              新建文档
            </button>
          </div>
        {/if}
      </div>
    </div>
  </main>
</section>

<style>
  .prompt-docs {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: grid;
    grid-template-columns: 280px minmax(0, 1fr);
    background: var(--bg);
  }

  .doc-list {
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px;
    background: var(--surface);
    border-right: 1px solid var(--border);
  }

  .doc-list-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .doc-list-header h2 {
    font-size: var(--font-lg);
  }

  .doc-search {
    width: 100%;
    height: 30px;
    padding: 0 9px;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    background: var(--bg);
    color: var(--text);
    outline: none;
  }

  .doc-search:focus {
    border-color: var(--accent);
    box-shadow: var(--focus-ring);
  }

  .doc-items {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .doc-item {
    width: 100%;
    display: grid;
    gap: 3px;
    border: 1px solid transparent;
    border-radius: var(--radius-s);
    background: transparent;
    padding: 9px;
    text-align: left;
    position: relative;
    transition:
      background var(--motion-fast) var(--ease-responsive),
      border-color var(--motion-fast) var(--ease-responsive),
      transform var(--motion-press) var(--ease-responsive);
  }

  .doc-item:active {
    transform: scale(0.99);
  }

  .doc-item:hover {
    background: var(--surface-2);
  }

  .doc-item.is-active {
    background: var(--accent-soft);
    border-color: var(--accent-soft-border);
  }

  .doc-item.is-active::before {
    content: "";
    position: absolute;
    inset: 8px auto 8px 0;
    width: 2px;
    border-radius: var(--radius-full);
    background: var(--accent);
    animation: doc-indicator-in var(--motion-fast) var(--ease-responsive);
  }

  .doc-title {
    font-weight: 600;
    color: var(--text);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .doc-date {
    font-size: var(--font-xs);
    color: var(--text-3);
  }

  .doc-snippet {
    display: -webkit-box;
    overflow: hidden;
    color: var(--text-2);
    font-size: var(--font-sm);
    line-height: 1.45;
    line-clamp: 2;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
  }

  .empty-list {
    padding: 16px 4px;
    color: var(--text-3);
    font-size: var(--font-md);
  }

  .doc-editor {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    position: relative;
  }

  .document-progress {
    position: absolute;
    z-index: var(--z-nav);
    inset: 0 0 auto;
    height: 2px;
    overflow: hidden;
    background: var(--accent-soft);
  }

  .document-progress::after {
    content: "";
    display: block;
    width: 38%;
    height: 100%;
    background: var(--accent);
    animation: document-progress 0.85s linear infinite;
  }

  .editor-titlebar {
    flex: none;
    height: 54px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }

  .title-input {
    flex: 1;
    min-width: 0;
    height: 34px;
    border: 1px solid transparent;
    border-radius: var(--radius-s);
    background: transparent;
    padding: 0 8px;
    font-size: var(--font-xl);
    font-weight: 650;
    color: var(--text);
    outline: none;
  }

  .title-input:focus {
    border-color: var(--border-strong);
    background: var(--surface-2);
  }

  .save-status {
    width: 92px;
    flex: none;
    color: var(--text-3);
    font-size: var(--font-sm);
  }

  .save-status > span {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }

  .save-dot,
  .save-check,
  .save-spinner {
    width: 10px;
    height: 10px;
    display: inline-grid;
    place-items: center;
    flex: none;
    border-radius: 50%;
    font-size: 9px;
    font-style: normal;
  }

  .save-dot {
    background: #c88a1a;
  }

  .save-check {
    color: var(--success);
    animation: save-check-in var(--motion-fast) var(--ease-responsive);
  }

  .save-spinner {
    border: 1.5px solid var(--border-strong);
    border-top-color: var(--accent);
    animation: save-spin 0.8s linear infinite;
  }

  .save-status.error {
    color: var(--danger);
  }

  .toolbar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }

  .toolbar button {
    min-width: 34px;
    height: 30px;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    background: var(--surface);
    color: var(--text-2);
    padding: 0 10px;
    font-size: var(--font-md);
    transition:
      background var(--motion-fast) var(--ease-responsive),
      border-color var(--motion-fast) var(--ease-responsive),
      color var(--motion-fast) var(--ease-responsive),
      transform var(--motion-press) var(--ease-responsive);
  }

  .toolbar button:active:not(:disabled) {
    transform: scale(0.96);
  }

  .toolbar button:hover:not(:disabled),
  .toolbar button.is-active {
    background: var(--accent-soft);
    border-color: var(--accent-soft-border);
    color: var(--accent);
  }

  .toolbar button:nth-child(3),
  .toolbar button:nth-child(4) {
    font-weight: 700;
  }

  .toolbar button:nth-child(4) {
    font-style: italic;
  }

  .editor-shell {
    flex: 1;
    min-height: 0;
    padding: 18px;
    overflow: auto;
  }

  .editor-surface {
    position: relative;
    min-height: 100%;
    max-width: 980px;
    margin: 0 auto;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    box-shadow: var(--shadow-1);
    opacity: 1;
    transition: opacity var(--motion-fast) var(--ease-responsive);
  }

  .editor-surface.is-switching {
    opacity: 0.72;
  }

  .editor-surface.is-disabled {
    display: grid;
    place-items: center;
  }

  .editor-content {
    min-height: calc(100vh - 190px);
  }

  .editor-empty {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    align-content: center;
    gap: 14px;
    background: var(--surface);
    color: var(--text-2);
  }

  .editor-empty h3 {
    font-size: var(--font-lg);
  }

  :global(.ProseMirror) {
    min-height: calc(100vh - 190px);
    padding: 36px 44px;
    outline: none;
    color: var(--text);
    line-height: 1.7;
    font-size: var(--font-lg);
  }

  :global(.ProseMirror p) {
    margin: 0 0 12px;
  }

  :global(.ProseMirror h1),
  :global(.ProseMirror h2),
  :global(.ProseMirror h3) {
    margin: 18px 0 10px;
    line-height: 1.35;
  }

  :global(.ProseMirror h1) {
    font-size: 24px;
  }

  :global(.ProseMirror h2) {
    font-size: var(--font-xl);
  }

  :global(.ProseMirror h3) {
    font-size: var(--font-lg);
  }

  :global(.ProseMirror ul),
  :global(.ProseMirror ol) {
    margin: 0 0 12px;
    padding-left: 24px;
  }

  :global(.ProseMirror img) {
    display: block;
    max-width: 100%;
    height: auto;
    margin: 12px 0;
    border-radius: var(--radius-s);
  }

  :global(.ProseMirror-selectednode) {
    outline: 2px solid var(--accent);
  }

  @keyframes document-progress {
    from { transform: translateX(-100%); }
    to { transform: translateX(365%); }
  }

  @keyframes save-spin {
    to { transform: rotate(360deg); }
  }

  @keyframes save-check-in {
    from { opacity: 0; transform: scale(0.7); }
  }

  @keyframes doc-indicator-in {
    from { opacity: 0; transform: scaleY(0); }
  }

  @media (prefers-reduced-motion: reduce) {
    .document-progress::after,
    .save-spinner,
    .save-check,
    .doc-item.is-active::before {
      animation: none;
    }
  }
</style>
