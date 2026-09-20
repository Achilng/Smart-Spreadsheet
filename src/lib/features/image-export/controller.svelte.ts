import { errorText, formatCount } from "../../utils/format";
import { setNotice } from "../../stores/notices.svelte";
import { emitTo, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { onMount } from "svelte";
import {
  collectExportImages,
  exportSelectedImages,
  getImageExportSettings,
  setImageExportSettings,
  type ExportProgress,
  type ImageFileRenameMode,
  type ImageFilesExportResult,
  type RowSelection,
} from "../../api";
import { focusMainWindow, type ToolboxSelectionSnapshot } from "../../windows/toolbox";


/** Per-instance state and operations for the ImageExportTool view. */
export function createImageExportController(isActive: () => boolean) {

  let selectionSnapshot = $state<ToolboxSelectionSnapshot | null>(null);
  let destination = $state<string | null>(null);
  let renameEnabled = $state(false);
  let renameMode = $state<"random" | "custom">("random");
  let customName = $state("");
  let stripMetadata = $state(false);
  let exporting = $state(false);
  let progress = $state<ExportProgress | null>(null);
  let lastResult = $state<ImageFilesExportResult | null>(null);
  let localError = $state<string | null>(null);
  let selectionListenerReady = $state(false);
  let settingsReady = $state(false);
  let addedPaths = $state<string[]>([]);
  let scanning = $state(false);
  let draggingOverSource = $state(false);
  let sourceDropZone: HTMLButtonElement;

  const mainSelectedCount = $derived(selectionSnapshot?.count ?? 0);
  const addedCount = $derived(addedPaths.length);
  const selectedCount = $derived(mainSelectedCount + addedCount);
  const effectiveRenameMode = $derived<ImageFileRenameMode>(
    renameEnabled ? renameMode : "original",
  );
  const customNameValid = $derived(
    effectiveRenameMode !== "custom" || customName.trim().length > 0,
  );
  const canExport = $derived(
    !exporting &&
      !scanning &&
      selectedCount > 0 &&
      Boolean(destination) &&
      customNameValid,
  );

  onMount(() => {
    let disposed = false;
    let unlistenSelection: UnlistenFn | null = null;
    let unlistenDragDrop: UnlistenFn | null = null;
    void getImageExportSettings()
      .then(settings => {
        if (disposed) return;
        destination = settings.destination;
        renameEnabled = settings.renameEnabled;
        renameMode = settings.renameMode;
        customName = settings.customName;
        stripMetadata = settings.stripMetadata;
        settingsReady = true;
      })
      .catch(cause => {
        if (disposed) return;
        localError = `无法读取已保存的导出设置：${errorText(cause)}`;
      });
    void listen<ToolboxSelectionSnapshot>("main://selection-changed", event => {
      selectionSnapshot = event.payload;
    }).then(unlisten => {
      if (disposed) {
        unlisten();
      } else {
        unlistenSelection = unlisten;
        selectionListenerReady = true;
        void requestSelection();
      }
    });
    void getCurrentWebview().onDragDropEvent(event => {
      if (!isActive() || exporting || scanning) {
        draggingOverSource = false;
        return;
      }
      if (event.payload.type === "enter" || event.payload.type === "over") {
        draggingOverSource = isInsideSourceDropZone(event.payload.position);
      } else if (event.payload.type === "leave") {
        draggingOverSource = false;
      } else {
        const shouldAdd = isInsideSourceDropZone(event.payload.position);
        draggingOverSource = false;
        if (shouldAdd) {
          void addSourcePaths(event.payload.paths);
        }
      }
    }).then(unlisten => {
      if (disposed) unlisten();
      else unlistenDragDrop = unlisten;
    });
    return () => {
      disposed = true;
      unlistenSelection?.();
      unlistenDragDrop?.();
    };
  });

  $effect(() => {
    if (isActive() && selectionListenerReady) {
      void requestSelection();
    }
  });

  $effect(() => {
    const settings = {
      destination,
      renameEnabled,
      renameMode,
      customName,
      stripMetadata,
    };
    if (!settingsReady) return;
    const timer = window.setTimeout(() => {
      void setImageExportSettings(settings).catch(cause => {
        localError = `无法保存导出设置：${errorText(cause)}`;
      });
    }, 250);
    return () => window.clearTimeout(timer);
  });

  async function requestSelection(): Promise<void> {
    try {
      await emitTo("main", "toolbox://request-selection");
    } catch {
      localError = "无法读取主窗口选区，请确认主窗口仍在运行。";
    }
  }

  async function chooseDestination(): Promise<void> {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "选择图片导出文件夹",
    });
    if (typeof selected !== "string") {
      return;
    }
    destination = selected;
    lastResult = null;
    localError = null;
  }

  async function chooseSourceFolder(): Promise<void> {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "选择需要导出的图片文件夹",
    });
    if (typeof selected === "string") {
      await addSourcePaths([selected]);
    }
  }

  async function addSourcePaths(paths: string[]): Promise<void> {
    if (paths.length === 0 || scanning || exporting) return;
    scanning = true;
    localError = null;
    lastResult = null;
    try {
      addedPaths = await collectExportImages([...addedPaths, ...paths]);
    } catch (cause) {
      localError = errorText(cause);
    } finally {
      scanning = false;
    }
  }

  function removeAddedPath(path: string): void {
    addedPaths = addedPaths.filter(candidate => candidate !== path);
    lastResult = null;
  }

  function clearAddedPaths(): void {
    addedPaths = [];
    lastResult = null;
  }

  function isInsideSourceDropZone(position: { x: number; y: number }): boolean {
    if (!sourceDropZone) return false;
    const scale = window.devicePixelRatio || 1;
    const x = position.x / scale;
    const y = position.y / scale;
    const rect = sourceDropZone.getBoundingClientRect();
    return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
  }

  async function returnToMain(): Promise<void> {
    try {
      await focusMainWindow();
    } catch (cause) {
      localError = `无法切换到主窗口：${errorText(cause)}`;
    }
  }

  async function runExport(): Promise<void> {
    if (!canExport || !destination) {
      return;
    }
    const selection: RowSelection = selectionSnapshot?.selection ?? {
      kind: "explicit",
      rowIds: [],
    };
    const target = destination;
    const custom = effectiveRenameMode === "custom" ? customName.trim() : null;
    exporting = true;
    progress = null;
    lastResult = null;
    localError = null;
    const unlisten = await listen<ExportProgress>("export://progress", event => {
      progress = event.payload;
    });
    try {
      const result = await exportSelectedImages(
        selection,
        addedPaths,
        target,
        effectiveRenameMode,
        custom,
        stripMetadata,
      );
      lastResult = result;
      const missing = result.missing > 0
        ? `，${formatCount(result.missing)} 张源文件不可用`
        : "";
      setNotice({
        tone: "success",
        text: `已导出 ${formatCount(result.exported)} 张图片${missing}。`,
      });
      await requestSelection();
    } catch (cause) {
      localError = errorText(cause);
    } finally {
      unlisten();
      progress = null;
      exporting = false;
    }
  }

  function folderName(path: string): string {
    return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
  }

  function fileName(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }
  return {
    get destination() { return destination; },
    set destination(value: typeof destination) { destination = value; },
    get renameEnabled() { return renameEnabled; },
    set renameEnabled(value: typeof renameEnabled) { renameEnabled = value; },
    get renameMode() { return renameMode; },
    set renameMode(value: typeof renameMode) { renameMode = value; },
    get customName() { return customName; },
    set customName(value: typeof customName) { customName = value; },
    get stripMetadata() { return stripMetadata; },
    set stripMetadata(value: typeof stripMetadata) { stripMetadata = value; },
    get exporting() { return exporting; },
    set exporting(value: typeof exporting) { exporting = value; },
    get progress() { return progress; },
    set progress(value: typeof progress) { progress = value; },
    get lastResult() { return lastResult; },
    set lastResult(value: typeof lastResult) { lastResult = value; },
    get localError() { return localError; },
    set localError(value: typeof localError) { localError = value; },
    get settingsReady() { return settingsReady; },
    set settingsReady(value: typeof settingsReady) { settingsReady = value; },
    get addedPaths() { return addedPaths; },
    set addedPaths(value: typeof addedPaths) { addedPaths = value; },
    get scanning() { return scanning; },
    set scanning(value: typeof scanning) { scanning = value; },
    get draggingOverSource() { return draggingOverSource; },
    set draggingOverSource(value: typeof draggingOverSource) { draggingOverSource = value; },
    get sourceDropZone() { return sourceDropZone; },
    set sourceDropZone(value: typeof sourceDropZone) { sourceDropZone = value; },
    get mainSelectedCount() { return mainSelectedCount; },
    get addedCount() { return addedCount; },
    get selectedCount() { return selectedCount; },
    get customNameValid() { return customNameValid; },
    get canExport() { return canExport; },
    chooseDestination,
    chooseSourceFolder,
    removeAddedPath,
    clearAddedPaths,
    returnToMain,
    runExport,
    folderName,
    fileName,
  };
}
