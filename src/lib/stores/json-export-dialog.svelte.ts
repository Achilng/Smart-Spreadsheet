import type { RowSelection } from "../api";

export const jsonExportDialog = $state<{
  selection: RowSelection | null;
  scopeLabel: string;
}>({
  selection: null,
  scopeLabel: "",
});

export function requestJsonExport(selection: RowSelection, scopeLabel: string): void {
  jsonExportDialog.selection = selection;
  jsonExportDialog.scopeLabel = scopeLabel;
}

export function closeJsonExportDialog(): void {
  jsonExportDialog.selection = null;
  jsonExportDialog.scopeLabel = "";
}
