<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";

  import { app } from "../../stores/app-state.svelte";
  import { runExistingImageUpdate } from "../../stores/import-actions.svelte";
  import Modal from "../../ui/Modal.svelte";

  function close(): void {
    app.updateImportOpen = false;
  }

  async function chooseFolder(): Promise<void> {
    const selection = await open({
      directory: true,
      multiple: false,
      title: "选择原图片文件夹（只更新已入库图片，不新增）",
    });
    if (typeof selection !== "string") return;
    close();
    await runExistingImageUpdate(selection);
  }

  async function chooseArchive(): Promise<void> {
    const selection = await open({
      directory: false,
      multiple: false,
      title: "选择原压缩包（只更新已入库图片，不新增）",
      filters: [{ name: "压缩包", extensions: ["zip", "7z", "rar"] }],
    });
    if (typeof selection !== "string") return;
    close();
    await runExistingImageUpdate(selection);
  }
</script>

<Modal open={app.updateImportOpen} onclose={close} labelledby="update-import-title" width="520px">
  <div class="update-dialog">
    <header>
      <h2 id="update-import-title">更新现有图片</h2>
      <p>重新读取原图中的 NovelAI 元数据，并更新资料库中已经存在的对应记录。</p>
    </header>

    <div class="rules">
      <strong>本操作只更新，不会追加新图片：</strong>
      <ul>
        <li>优先按原图片路径，或“压缩包路径＋包内路径”精确匹配。</li>
        <li>原图搬家后，会继续按完整文件内容或含 seed 的完整 NovelAI 元数据重新关联。</li>
        <li>若同一内容或元数据对应多条记录，为安全起见不会自动覆盖。</li>
        <li>分别更新正向提示词、角色提示词、负向提示词、画师串和图片指纹。</li>
        <li>原有 Tag、分组和行 ID 全部保留。</li>
        <li>来源中的新图片会被忽略，不会追加进资料库。</li>
        <li>来源里缺失的旧图片不会删除；读取失败时保留原数据。</li>
        <li><strong>若有图片被更新，会清空当前的撤销/重做记录。</strong></li>
      </ul>
    </div>

    <p class="tip">请选择图片原来所在的文件夹，或原来的压缩包路径。</p>

    <footer>
      <button type="button" class="btn" disabled={app.busy} onclick={close}>取消</button>
      <button type="button" class="btn" disabled={app.busy} onclick={() => void chooseArchive()}>选择压缩包</button>
      <button type="button" class="btn btn-primary" disabled={app.busy} onclick={() => void chooseFolder()}>选择文件夹</button>
    </footer>
  </div>
</Modal>

<style>
  .update-dialog {
    padding: 20px;
  }

  header h2 {
    margin: 0 0 6px;
    font-size: var(--font-lg);
  }

  header p,
  .tip {
    margin: 0;
    color: var(--text-2);
    font-size: var(--font-md);
    line-height: 1.55;
  }

  .rules {
    margin: 16px 0 12px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-s);
    background: var(--surface-2);
    font-size: var(--font-md);
    line-height: 1.6;
  }

  .rules strong {
    color: var(--text);
  }

  .rules ul {
    margin: 6px 0 0;
    padding-left: 20px;
    color: var(--text-2);
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 18px;
  }
</style>
