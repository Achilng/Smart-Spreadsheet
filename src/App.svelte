<script lang="ts">
  import { app, refreshSnapshot } from "./lib/app-state.svelte";
  import ImportScreen from "./lib/components/ImportScreen.svelte";
  import Notice from "./lib/components/Notice.svelte";
  import SetupScreen from "./lib/components/SetupScreen.svelte";
  import Workspace from "./lib/components/Workspace.svelte";

  void refreshSnapshot();
</script>

{#if !app.loaded}
  <div class="center-screen">
    <p class="muted">正在读取应用状态…</p>
  </div>
{:else if app.snapshot?.startupError}
  <div class="center-screen">
    <div class="flow-card">
      <h2>无法打开已配置的数据目录</h2>
      <p class="muted">{app.snapshot.startupError}</p>
      <p class="flow-hint">定位文件未被自动覆盖，避免误切换到另一份工作区。</p>
    </div>
  </div>
{:else if !app.snapshot?.dataDirectory}
  <SetupScreen />
{:else if !app.snapshot.workbook}
  <ImportScreen />
{:else}
  <Workspace />
{/if}

<Notice />
