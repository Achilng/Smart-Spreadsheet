import { useEffect, useRef } from "react";
import { flushSync } from "react-dom";
import "./album-transition.css";

type Direction = "enter" | "leave";

/** Match real image IDs, never fly a cover into an unrelated photo at a restored scroll position. */
export function useAlbumTransition() {
  const root = useRef<HTMLDivElement>(null);
  const active = useRef<(() => void) | null>(null);
  useEffect(() => () => active.current?.(), []);

  async function navigate(direction: Direction, key: string, update: () => void, prepare?: () => Promise<void>) {
    if (active.current) return;
    const node = root.current;
    if (!node || matchMedia("(prefers-reduced-motion: reduce)").matches) { update(); return; }
    let cancelled = false, applied = false, transition: ViewTransition | undefined;
    const names = new Map<HTMLElement, string>();
    const rules = document.createElement("style");
    const animations: Animation[] = [];
    const mark = (element: HTMLElement | null | undefined, name: string) => {
      if (!element) return;
      if (!names.has(element)) names.set(element, element.style.viewTransitionName);
      element.style.viewTransitionName = name;
    };
    const clean = () => {
      if (cancelled) return;
      cancelled = true;
      transition?.skipTransition();
      animations.forEach(animation => animation.cancel());
      names.forEach((name, element) => { element.style.viewTransitionName = name; });
      rules.remove();
      delete node.dataset.albumMotion;
      delete document.documentElement.dataset.albumMotion;
      active.current = null;
    };
    active.current = clean;
    const album = () => [...node.querySelectorAll<HTMLElement>("[data-album-key]")].find(element => element.dataset.albumKey === key);
    const visible = (element: HTMLElement) => {
      const box = element.getBoundingClientRect(), viewport = node.querySelector(".r-section-list")?.getBoundingClientRect();
      return viewport && box.bottom > viewport.top && box.top < viewport.bottom && box.right > viewport.left && box.left < viewport.right;
    };
    const sheets = () => [...album()?.querySelectorAll<HTMLElement>(".r-album-sheet") ?? []];
    const cards = () => [...node.querySelectorAll<HTMLElement>(".r-section-card")].filter(visible);
    const identify = (element: HTMLElement) => element.querySelector<HTMLElement>("[data-image-row]")?.dataset.imageRow;
    const focusDestination = () => {
      const target = direction === "enter" ? node.querySelector<HTMLElement>(".r-group-detail-title button") : album()?.querySelector<HTMLElement>(".r-album-trigger");
      target?.focus({ preventScroll: true });
    };
    const apply = () => { if (!cancelled && !applied) { applied = true; flushSync(update); } };
    try {
      // Start the request immediately, but never make navigation wait on slow storage.
      if (prepare) {
        let timer: ReturnType<typeof setTimeout> | undefined;
        await Promise.race([prepare(), new Promise<void>(resolve => { timer = setTimeout(resolve, 140); })]).finally(() => clearTimeout(timer));
      }
      if (cancelled || !node.isConnected) return;
      if (typeof document.startViewTransition !== "function") {
        apply(); focusDestination();
        animations.push(node.animate([{ opacity: .35, transform: "translateY(14px) scale(.98)" }, { opacity: 1, transform: "none" }], { duration: 280, easing: "cubic-bezier(.2,.8,.2,1)" }));
        await animations[0].finished.catch(() => {});
        return;
      }
      node.dataset.albumMotion = direction;
      document.documentElement.dataset.albumMotion = direction;
      document.head.append(rules);
      mark(node, "album-page");
      const origin = direction === "enter" ? sheets() : cards();
      const ids = origin.map(identify);
      const sourceTitle = direction === "enter" ? album()?.querySelector<HTMLElement>(".r-album-title") : node.querySelector<HTMLElement>(".r-group-detail-title h1");
      mark(sourceTitle, "album-title");
      // At most three shared covers; other visible tiles spread out in a short wave.
      if (direction === "enter") origin.slice(0, 3).forEach((element, index) => mark(element, `album-photo-${index}`));
      else origin.slice(0, 24).forEach((element, index) => mark(element, `album-tile-${index}`));
      transition = document.startViewTransition(async () => {
        apply();
        if (cancelled) return;
        // Rendering is suspended inside the capture callback; waiting for rAF here deadlocks.
        // A short task delay lets cached cover requests and React effects finish instead.
        await new Promise<void>(resolve => setTimeout(resolve, 40));
        if (cancelled || !node.isConnected) return;
        focusDestination();
        const destination = direction === "enter" ? cards() : sheets();
        const targetTitle = direction === "enter" ? node.querySelector<HTMLElement>(".r-group-detail-title h1") : album()?.querySelector<HTMLElement>(".r-album-title");
        if (direction === "enter" || targetTitle && visible(targetTitle)) mark(targetTitle, "album-title");
        destination.slice(0, direction === "enter" ? 24 : 3).forEach((element, index) => {
          const id = identify(element), match = id ? ids.indexOf(id) : -1;
          const paired = match >= 0 && match < (direction === "enter" ? 3 : 24);
          mark(element, paired ? `album-${direction === "enter" ? "photo" : "tile"}-${match}` : `album-arrival-${index}`);
          if (!paired) rules.sheet?.insertRule(`:root[data-album-motion]::view-transition-new(album-arrival-${index}){animation:album-tile-unfold 360ms cubic-bezier(.2,.8,.2,1) ${Math.min(index * 16, 144)}ms both}`);
        });
        if (direction === "leave") origin.slice(0, 24).forEach((_, index) => {
          if (!ids[index] || !destination.some(element => identify(element) === ids[index])) rules.sheet?.insertRule(`:root[data-album-motion]::view-transition-old(album-tile-${index}){animation:album-tile-fold 260ms cubic-bezier(.4,0,.6,1) ${Math.min(index * 8, 80)}ms both}`);
        });
      });
      // A skipped/unsupported snapshot still runs the state update. Never run it twice.
      void transition.ready.catch(() => {});
      await transition.finished.catch(() => {});
    } catch {
      apply();
    } finally { clean(); }
  }
  return { root, navigate };
}
