export function LlmBadge({ source }: { source?: string | null }) {
  if (!source) return null;
  let title = "由 LLM 提取";
  try { const info = JSON.parse(source); title = `由 ${info.model} 提取 · ${info.promptVersion}`; } catch { /* Source flag remains visible. */ }
  return <span title={title} style={{ display: "inline-block", fontSize: 13, fontWeight: 600, lineHeight: "20px", padding: "0 6px", marginRight: 6, borderRadius: 5, background: "#e8f2eb", color: "#28633f", verticalAlign: "middle" }}>LLM</span>;
}
