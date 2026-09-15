/** Session-only browser history. Entries contain view state, never copies of library data. */
export function createNavigationHistory<Route, Position>(limit = 100) {
  type Entry = { route: Route; position: Position };
  let entries: Entry[] = [];
  let cursor = -1;
  let lastMergeKey: string | undefined;

  return {
    get current() { return entries[cursor]; },
    get back() { return entries[cursor - 1]; },
    get forward() { return entries[cursor + 1]; },
    get length() { return entries.length; },
    reset(entry: Entry) {
      entries = [entry];
      cursor = 0;
      lastMergeKey = undefined;
    },
    save(position: Position) {
      if (entries[cursor]) entries[cursor] = { ...entries[cursor], position };
    },
    record(entry: Entry, mergeKey?: string) {
      if (JSON.stringify(entries[cursor]?.route) === JSON.stringify(entry.route)) return;
      const merge = mergeKey !== undefined && mergeKey === lastMergeKey && cursor === entries.length - 1;
      entries.splice(cursor + 1);
      // Typing and then erasing the same search gesture returns to its origin.
      if (merge && cursor > 0 && JSON.stringify(entries[cursor - 1].route) === JSON.stringify(entry.route)) {
        entries.pop();
        cursor -= 1;
        lastMergeKey = undefined;
        return;
      }
      if (merge) entries[cursor] = entry;
      else {
        entries.push(entry);
        if (entries.length > limit) entries.shift();
        cursor = entries.length - 1;
      }
      lastMergeKey = mergeKey;
    },
    go(direction: -1 | 1) {
      const entry = entries[cursor + direction];
      if (!entry) return undefined;
      cursor += direction;
      lastMergeKey = undefined;
      return entry;
    },
  };
}
