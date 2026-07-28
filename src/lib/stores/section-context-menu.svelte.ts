export type SectionMenuTarget =
  | { kind: "group"; groupId: number; name: string }
  | { kind: "dedupe"; mode: "artists" | "positivePrompt" | "vibes"; key: string; displayName: string };

export const sectionMenu = $state({
  open: false,
  x: 0,
  y: 0,
  target: null as SectionMenuTarget | null,
  aliasVersion: 0,
});

export function showSectionMenu(
  target: SectionMenuTarget,
  x: number,
  y: number,
): void {
  sectionMenu.target = target;
  sectionMenu.x = x;
  sectionMenu.y = y;
  sectionMenu.open = true;
}

export function hideSectionMenu(): void {
  sectionMenu.open = false;
}
