<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { confirm as confirmDialog, open } from "@tauri-apps/plugin-dialog";
  import { Editor, type JSONContent } from "@tiptap/core";
  import FileHandler from "@tiptap/extension-file-handler";
  import Image from "@tiptap/extension-image";
  import StarterKit from "@tiptap/starter-kit";
  import { flip } from "svelte/animate";
  import { onMount, tick } from "svelte";

  import {
    createPromptDoc,
    deletePromptDoc,
    importPromptDocImageBytes,
    importPromptDocImageFromPath,
    listPromptDocs,
    loadPromptDoc,
    savePromptDoc,
    type PromptDocAsset,
    type PromptDocDetail,
    type PromptDocSummary,
  } from "../../api";
  import { errorText, setNotice } from "../../stores/app-state.svelte";
  import { registerCloseGuard } from "../../stores/close-guard";
  import { lastPromptDoc, rememberPromptDoc } from "../../stores/view-state";
  import { softFade } from "../../ui/motion";

  const IMAGE_MIME_TYPES = [
    "image/png",
    "image/jpeg",
    "image/webp",
    "image/gif",
    "image/bmp",
    "image/tiff",
  ];
  const IMAGE_EXTENSIONS = new Set(["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff"]);

  let docs = $state<PromptDocSummary[]>([]);
  let activeDoc = $state<PromptDocDetail | null>(null);
  let title = $state("");
  let search = $state("");
  let loading = $state(false);
  let switchingDoc = $state(false);
  let saving = $state(false);
  let saveState = $state<"idle" | "dirty" | "saving" | "saved" | "error">("idle");
  let editorElement = $state<HTMLDivElement | null>(null);
  let editor = $state<Editor | null>(null);
  let editorVersion = $state(0);

  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let suppressUpdate = false;
  const displayToPersistent = new Map<string, string>();
  const persistentToDisplay = new Map<string, string>();

  const filteredDocs = $derived(
    docs.filter(doc => {
      const keyword = search.trim().toLowerCase();
      if (!keyword) return true;
      return (
        doc.title.toLowerCase().includes(keyword) ||
        doc.plainText.toLowerCase().includes(keyword)
      );
    }),
  );

  const statusText = $derived.by(() => {
    if (!activeDoc) return "";
    if (saving || saveState === "saving") return "保存中…";
    if (saveState === "dirty") return "有未保存修改";
    if (saveState === "error") return "保存失败";
    return "已保存";
  });

  onMount(() => {
    const pathDropHandler = (event: Event) => {
      const paths = (event as CustomEvent<string[]>).detail ?? [];
      void importImagePaths(paths);
    };

    window.addEventListener("prompt-doc-path-drop", pathDropHandler);
    void initialize();

    // 关窗前：未保存的文档先落盘；保存不掉的情况下拦截确认
    const unregisterGuard = registerCloseGuard(async () => {
      if (saveState === "dirty" || saveState === "saving") {
        await flushSave();
      }
      if (saveState === "dirty" || saveState === "error") {
        return `提示词文档「${activeDoc?.title ?? "未命名"}」有未保存的修改`;
      }
      return null;
    });

    return () => {
      window.removeEventListener("prompt-doc-path-drop", pathDropHandler);
      window.clearTimeout(saveTimer);
      unregisterGuard();
      void flushSave();
      editor?.destroy();
    };
  });

  async function initialize(): Promise<void> {
    await tick();
    createEditor();
    await reloadDocs();
    // 优先恢复上次打开的文档，不存在（已删/换库）则回退第一篇
    const remembered = lastPromptDoc();
    const target = remembered && docs.some(doc => doc.id === remembered) ? remembered : docs[0]?.id;
    if (target) {
      await selectDoc(target);
    } else {
      clearEditor();
    }
  }

  function createEditor(): void {
    if (!editorElement || editor) return;
    editor = new Editor({
      element: editorElement,
      editable: false,
      content: emptyDocContent(),
      extensions: [
        StarterKit.configure({
          heading: { levels: [1, 2, 3] },
        }),
        Image.configure({
          allowBase64: false,
          resize: {
            enabled: true,
            minWidth: 80,
            minHeight: 80,
            alwaysPreserveAspectRatio: true,
          },
        }),
        FileHandler.configure({
          allowedMimeTypes: IMAGE_MIME_TYPES,
          onPaste: (_editor, files) => {
            void importImageFiles(files);
          },
          onDrop: (_editor, files, pos) => {
            void importImageFiles(files, pos);
          },
        }),
      ],
      onUpdate: () => {
        editorVersion += 1;
        if (!suppressUpdate) {
          scheduleSave();
        }
      },
      onSelectionUpdate: () => {
        editorVersion += 1;
      },
    });
  }

  async function reloadDocs(): Promise<void> {
    try {
      docs = await listPromptDocs();
    } catch (error) {
      setNotice({ tone: "error", text: errorText(error) });
    }
  }

  async function createNewDoc(): Promise<void> {
    if (!(await flushSaveOrConfirm())) return;
    loading = true;
    try {
      const detail = await createPromptDoc("未命名文档");
      upsertDocSummary(detail);
      applyDoc(detail);
    } catch (error) {
      setNotice({ tone: "error", text: errorText(error) });
    } finally {
      loading = false;
    }
  }

  async function selectDoc(docId: string): Promise<void> {
    if (activeDoc?.id === docId) return;
    if (!(await flushSaveOrConfirm())) return;
    loading = true;
    switchingDoc = true;
    try {
      applyDoc(await loadPromptDoc(docId));
    } catch (error) {
      setNotice({ tone: "error", text: errorText(error) });
    } finally {
      loading = false;
      switchingDoc = false;
    }
  }

  /**
   * 保存当前文档；保存失败时阻塞询问用户，避免带着未保存内容切换文档后被
   * 旧内容静默覆盖。返回 false 表示用户选择留在当前文档处理。
   */
  async function flushSaveOrConfirm(): Promise<boolean> {
    await flushSave();
    if (saveState !== "error") return true;
    return window.confirm(
      "当前文档保存失败，此时切换将丢失未保存的修改。确定放弃这些修改吗？（取消可留在本文档重试保存）",
    );
  }

  function applyDoc(detail: PromptDocDetail): void {
    activeDoc = detail;
    rememberPromptDoc(detail.id);
    title = detail.title;
    saveState = "saved";
    const content = toDisplayContent(detail.content, detail.assets);
    suppressUpdate = true;
    editor?.commands.setContent(content as JSONContent, { emitUpdate: false });
    editor?.setEditable(true);
    suppressUpdate = false;
    editorVersion += 1;
  }

  function clearEditor(): void {
    activeDoc = null;
    rememberPromptDoc(null);
    title = "";
    saveState = "idle";
    displayToPersistent.clear();
    persistentToDisplay.clear();
    suppressUpdate = true;
    editor?.commands.setContent(emptyDocContent(), { emitUpdate: false });
    editor?.setEditable(false);
    suppressUpdate = false;
    editorVersion += 1;
  }

  function onTitleInput(event: Event): void {
    title = (event.target as HTMLInputElement).value;
    scheduleSave();
  }

  function scheduleSave(): void {
    if (!activeDoc || !editor) return;
    saveState = "dirty";
    window.clearTimeout(saveTimer);
    saveTimer = window.setTimeout(() => {
      void flushSave();
    }, 800);
  }

  async function flushSave(): Promise<void> {
    window.clearTimeout(saveTimer);
    if (!activeDoc || !editor || saveState !== "dirty") return;

    saving = true;
    saveState = "saving";
    try {
      const content = toPersistentContent(editor.getJSON());
      const plainText = editor.getText({ blockSeparator: "\n" });
      const saved = await savePromptDoc(activeDoc.id, title, content, plainText);
      activeDoc = { ...saved, content };
      title = saved.title;
      upsertDocSummary(saved);
      saveState = "saved";
    } catch (error) {
      saveState = "error";
      setNotice({ tone: "error", text: errorText(error) });
    } finally {
      saving = false;
    }
  }

  async function deleteActiveDoc(): Promise<void> {
    if (!activeDoc) return;
    const docId = activeDoc.id;
    const confirmed = await confirmDialog(
      `将删除「${activeDoc.title}」及其中插入的图片。此操作不可撤销，是否继续？`,
      { title: "删除提示词文档", kind: "warning", okLabel: "确认删除", cancelLabel: "取消" },
    );
    if (!confirmed) return;

    loading = true;
    try {
      await deletePromptDoc(docId);
      docs = docs.filter(doc => doc.id !== docId);
      const next = docs[0];
      if (next) {
        activeDoc = null;
        await selectDoc(next.id);
      } else {
        clearEditor();
      }
      setNotice({ tone: "success", text: "提示词文档已删除。" });
    } catch (error) {
      setNotice({ tone: "error", text: errorText(error) });
    } finally {
      loading = false;
    }
  }

  async function chooseImageFiles(): Promise<void> {
    if (!activeDoc) {
      setNotice({ tone: "error", text: "请先创建或选择一个提示词文档。" });
      return;
    }
    const selection = await open({
      multiple: true,
      directory: false,
      title: "选择要插入的图片",
      filters: [{ name: "图片", extensions: Array.from(IMAGE_EXTENSIONS) }],
    });
    if (typeof selection === "string") {
      await importImagePaths([selection]);
    } else if (Array.isArray(selection)) {
      await importImagePaths(selection);
    }
  }

  async function importImagePaths(paths: string[]): Promise<void> {
    if (!activeDoc || !editor) {
      if (paths.length > 0) {
        setNotice({ tone: "error", text: "请先创建或选择一个提示词文档。" });
      }
      return;
    }
    const imagePaths = paths.filter(isImagePath);
    if (imagePaths.length === 0) return;

    loading = true;
    try {
      for (const path of imagePaths) {
        const asset = await importPromptDocImageFromPath(activeDoc.id, path);
        insertImage(registerAsset(asset));
      }
      scheduleSave();
    } catch (error) {
      setNotice({ tone: "error", text: errorText(error) });
    } finally {
      loading = false;
    }
  }

  async function importImageFiles(files: File[], position?: number): Promise<void> {
    if (!activeDoc || !editor) return;
    const images = files.filter(file => file.type.startsWith("image/"));
    if (images.length === 0) return;

    loading = true;
    try {
      let insertPosition = position;
      for (const file of images) {
        const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
        const asset = await importPromptDocImageBytes(
          activeDoc.id,
          file.name || "clipboard.png",
          bytes,
        );
        insertImage(registerAsset(asset), insertPosition);
        if (typeof insertPosition === "number") {
          insertPosition += 1;
        }
      }
      scheduleSave();
    } catch (error) {
      setNotice({ tone: "error", text: errorText(error) });
    } finally {
      loading = false;
    }
  }

  function insertImage(src: string, position?: number): void {
    if (!editor) return;
    if (typeof position === "number") {
      editor.chain().focus().insertContentAt(position, { type: "image", attrs: { src } }).run();
    } else {
      editor.chain().focus().setImage({ src }).run();
    }
  }

  async function copyPlainText(): Promise<void> {
    if (!editor) return;
    const text = editor.getText({ blockSeparator: "\n" });
    try {
      await navigator.clipboard.writeText(text);
      setNotice({ tone: "success", text: "提示词文本已复制。" });
    } catch (error) {
      setNotice({ tone: "error", text: `复制失败：${errorText(error)}` });
    }
  }

  function registerAsset(asset: PromptDocAsset): string {
    const displaySrc = convertFileSrc(asset.path);
    persistentToDisplay.set(asset.src, displaySrc);
    displayToPersistent.set(displaySrc, asset.src);
    return displaySrc;
  }

  function toDisplayContent(content: unknown, assets: PromptDocAsset[]): unknown {
    displayToPersistent.clear();
    persistentToDisplay.clear();
    for (const asset of assets) {
      registerAsset(asset);
    }
    return mapImageSrc(content, src => persistentToDisplay.get(src) ?? src);
  }

  function toPersistentContent(content: unknown): unknown {
    return mapImageSrc(content, src => displayToPersistent.get(src) ?? src);
  }

  function mapImageSrc(value: unknown, mapper: (src: string) => string): unknown {
    if (Array.isArray(value)) {
      return value.map(item => mapImageSrc(item, mapper));
    }
    if (!value || typeof value !== "object") {
      return value;
    }

    const source = value as Record<string, unknown>;
    const next: Record<string, unknown> = {};
    for (const [key, child] of Object.entries(source)) {
      next[key] = mapImageSrc(child, mapper);
    }

    if (source.type === "image" && next.attrs && typeof next.attrs === "object") {
      const attrs = { ...(next.attrs as Record<string, unknown>) };
      if (typeof attrs.src === "string") {
        attrs.src = mapper(attrs.src);
      }
      next.attrs = attrs;
    }

    return next;
  }

  function emptyDocContent(): JSONContent {
    return { type: "doc", content: [{ type: "paragraph" }] };
  }

  function upsertDocSummary(detail: PromptDocDetail): void {
    const summary: PromptDocSummary = {
      id: detail.id,
      title: detail.title,
      createdAt: detail.createdAt,
      updatedAt: detail.updatedAt,
      plainText: detail.plainText,
    };
    docs = [summary, ...docs.filter(doc => doc.id !== summary.id)].sort(compareDocs);
  }

  function compareDocs(left: PromptDocSummary, right: PromptDocSummary): number {
    return (
      right.updatedAt.localeCompare(left.updatedAt) ||
      left.title.localeCompare(right.title) ||
      left.id.localeCompare(right.id)
    );
  }

  function isImagePath(path: string): boolean {
    const extension = path.split(".").pop()?.toLowerCase() ?? "";
    return IMAGE_EXTENSIONS.has(extension);
  }

  function isActive(name: string, attrs?: Record<string, unknown>): boolean {
    void editorVersion;
    return Boolean(editor?.isActive(name, attrs));
  }
</script>

<section class="prompt-docs">
  <aside class="doc-list">
    <div class="doc-list-header">
      <h2>提示词文档</h2>
      <button type="button" class="btn btn-primary" disabled={loading} onclick={() => void createNewDoc()}>
        新建
      </button>
    </div>

    <input
      class="doc-search"
      type="text"
      placeholder="搜索标题和正文…"
      bind:value={search}
    />

    <div class="doc-items">
      {#if filteredDocs.length === 0}
        <p class="empty-list">{search.trim() ? "没有匹配的文档" : "还没有提示词文档"}</p>
      {:else}
        {#each filteredDocs as doc (doc.id)}
          <button
            type="button"
            class="doc-item"
            class:is-active={activeDoc?.id === doc.id}
            onclick={() => void selectDoc(doc.id)}
            animate:flip={{ duration: 170 }}
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
    {#if switchingDoc}
      <div class="document-progress" aria-hidden="true" transition:softFade={{ duration: 100 }}></div>
    {/if}
    <div class="editor-titlebar">
      <input
        class="title-input"
        type="text"
        value={title}
        placeholder="未命名文档"
        disabled={!activeDoc || loading}
        oninput={onTitleInput}
      />
      <span class:error={saveState === "error"} class="save-status">
        {#key statusText}
          <span transition:softFade={{ duration: 120 }}>
            {#if saveState === "saving"}
              <i class="save-spinner" aria-hidden="true"></i>
            {:else if saveState === "saved"}
              <i class="save-check" aria-hidden="true">✓</i>
            {:else if saveState === "dirty"}
              <i class="save-dot" aria-hidden="true"></i>
            {/if}
            {statusText}
          </span>
        {/key}
      </span>
      <button type="button" class="btn" disabled={!activeDoc || loading} onclick={() => void copyPlainText()}>
        复制全文纯文本
      </button>
      <button type="button" class="btn btn-danger" disabled={!activeDoc || loading} onclick={() => void deleteActiveDoc()}>
        删除
      </button>
    </div>

    <div class="toolbar" aria-label="提示词文档工具栏">
      <button type="button" disabled={!activeDoc} class:is-active={isActive("paragraph")} onclick={() => editor?.chain().focus().setParagraph().run()}>
        正文
      </button>
      <button type="button" disabled={!activeDoc} class:is-active={isActive("heading", { level: 2 })} onclick={() => editor?.chain().focus().toggleHeading({ level: 2 }).run()}>
        标题
      </button>
      <button type="button" disabled={!activeDoc} class:is-active={isActive("bold")} onclick={() => editor?.chain().focus().toggleBold().run()}>
        B
      </button>
      <button type="button" disabled={!activeDoc} class:is-active={isActive("italic")} onclick={() => editor?.chain().focus().toggleItalic().run()}>
        I
      </button>
      <button type="button" disabled={!activeDoc} class:is-active={isActive("bulletList")} onclick={() => editor?.chain().focus().toggleBulletList().run()}>
        列表
      </button>
      <button type="button" disabled={!activeDoc || loading} onclick={() => void chooseImageFiles()}>
        插入图片
      </button>
    </div>

    <div class="editor-shell">
      <div class="editor-surface" class:is-disabled={!activeDoc} class:is-switching={switchingDoc}>
        <div class="editor-content" bind:this={editorElement}></div>
        {#if !activeDoc}
          <div class="editor-empty">
            <h3>创建一个提示词文档</h3>
            <button type="button" class="btn btn-primary" disabled={loading} onclick={() => void createNewDoc()}>
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
