/** Trim an owned map, keeping visible pages and the nearest loaded neighbours. */
export function trimPageCache<T>(pages: Map<number, T>, visible: ReadonlySet<number>, limit: number): Map<number, T> {
  const capacity = Math.max(limit, visible.size);
  if (pages.size <= capacity) return pages;
  const anchors = visible.size ? [...visible] : [0];
  const distance = (page: number) => Math.min(...anchors.map(anchor => Math.abs(page - anchor)));
  const candidates = [...pages.keys()].filter(page => !visible.has(page)).sort((a, b) => distance(b) - distance(a));
  for (const page of candidates) {
    if (pages.size <= capacity) break;
    pages.delete(page);
  }
  return pages;
}
