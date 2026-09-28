export function LlmBadge({ source }: { source?: string | null }) {
  if (!source) return null;
  let title = "由 LLM 提取";
  try { const info = JSON.parse(source); title = `由 ${info.model} 提取 · ${info.promptVersion}`; } catch { /* Source flag remains visible. */ }
  return <span className="r-llm-badge" title={title}>LLM</span>;
}
