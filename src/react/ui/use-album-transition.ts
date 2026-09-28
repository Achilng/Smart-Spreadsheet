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
    const poses = new Map<HTMLElement, { transform: string; shadow: string }>();
    const rules = document.createElement("style");
    const animations: Animation[] = [];
    const mark = (element: HTMLElement | null | undefined, name: string) => {
      if (!element) return;
      if (!names.has(element)) names.set(element, element.style.viewTransitionName);
      element.style.viewTransitionName = name;
    };
    const freeze = (element: HTMLElement) => {
      if (poses.has(element)) return;
      const style = getComputedStyle(element);
      const transform = style.transform, shadow = style.boxShadow;
      poses.set(element, { transform: element.style.transform, shadow: element.style.boxShadow });
      element.style.transform = transform;
      element.style.boxShadow = shadow;
    };
    const clean = () => {
      if (cancelled) return;
      cancelled = true;
      transition?.skipTransition();
      animations.forEach(animation => animation.cancel());
      names.forEach((name, element) => { element.style.viewTransitionName = name; });
      poses.forEach((pose, element) => { element.style.transform = pose.transform; element.style.boxShadow = pose.shadow; });
      rules.remove();
      delete node.dataset.albumMotion;
      delete document.documentElement.dataset.albumMotion;
      delete document.documentElement.dataset.albumPairs;
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
    // Shared covers travel as bare thumbnails; the file name stays with the page layer.
    const thumb = (card: HTMLElement) => card.querySelector<HTMLElement>(".r-section-thumb") ?? card;
    // Page chrome swaps instantly and stays on top with a solid ground, like a fixed title bar:
    // images move underneath it instead of showing through or covering it.
    const heads = () => [...node.querySelectorAll<HTMLElement>(":scope > .r-page-head, :scope > .r-page-toolbar")];
    const swapHeads = (side: "old" | "new") => heads().forEach((element, index) => {
      const name = `album-chrome-${side}-${index}`;
      mark(element, name);
      if (side === "old") rules.sheet?.insertRule(`:root[data-album-motion]::view-transition-old(${name}){display:none}`);
      else {
        rules.sheet?.insertRule(`:root[data-album-motion]::view-transition-group(${name}){z-index:30;animation:none}`);
        rules.sheet?.insertRule(`:root[data-album-motion]::view-transition-new(${name}){animation:none;background:var(--bg)}`);
      }
    });
    const identify = (element: HTMLElement) => element.querySelector<HTMLElement>("[data-image-row]")?.dataset.imageRow;
    const focusDestination = () => {
      const target = direction === "enter" ? node.querySelector<HTMLElement>(".r-group-detail-title button") : album()?.querySelector<HTMLElement>(".r-album-trigger");
      target?.focus({ preventScroll: true });
    };
    const apply = () => { if (!cancelled && !applied) { applied = true; flushSync(update); } };
    // Transition layers stack in DOM order; keep the fanned sheets' own z-order while they fly.
    const stack = (sheet: HTMLElement, name: string) => {
      const z = Number.parseInt(getComputedStyle(sheet).zIndex, 10);
      if (Number.isFinite(z)) rules.sheet?.insertRule(`:root[data-album-motion]::view-transition-group(${name}){z-index:${8 + z}}`);
    };
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
      // Preserve the exact hover pose before disabling secondary CSS transitions.
      // Otherwise an in-flight fan/tilt can jump before the first snapshot.
      if (direction === "enter") sheets().forEach(freeze);
      node.dataset.albumMotion = direction;
      document.documentElement.dataset.albumMotion = direction;
      document.head.append(rules);
      mark(node, "album-page");
      const origin = direction === "enter" ? sheets() : cards();
      const ids = origin.map(identify);
      swapHeads("old");
      // At most three shared covers; other visible tiles spread out in a short wave.
      if (direction === "enter") origin.slice(0, 3).forEach((element, index) => { mark(element, `album-photo-${index}`); stack(element, `album-photo-${index}`); });
      else origin.slice(0, 24).forEach((element, index) => mark(thumb(element), `album-tile-${index}`));
      transition = document.startViewTransition(async () => {
        apply();
        if (cancelled) return;
        // Rendering is suspended inside the capture callback; waiting for rAF here deadlocks.
        // A short task delay lets cached cover requests and React effects finish instead.
        await new Promise<void>(resolve => setTimeout(resolve, 40));
        if (cancelled || !node.isConnected) return;
        focusDestination();
        const destination = direction === "enter" ? cards() : sheets();
        // Hold the captured pose through handoff; pointer/focus changes must not
        // move the live cover behind its frozen transition snapshot.
        if (direction === "leave") destination.forEach(freeze);
        const images = [...node.querySelectorAll<HTMLImageElement>(".r-section-list img")].filter(visible).slice(0, 72);
        let decodeTimer: ReturnType<typeof setTimeout> | undefined;
        await Promise.race([
          Promise.allSettled(images.map(image => image.decode())),
          new Promise<void>(resolve => { decodeTimer = setTimeout(resolve, 80); }),
        ]).finally(() => clearTimeout(decodeTimer));
        if (cancelled || !node.isConnected) return;
        swapHeads("new");
        const pairedOrigins = new Set<number>();
        const matches = destination.slice(0, direction === "enter" ? 24 : 3).map(element => {
          const id = identify(element), match = id ? ids.indexOf(id) : -1;
          const paired = match >= 0 && match < (direction === "enter" ? 3 : 24) && !pairedOrigins.has(match);
          if (paired) pairedOrigins.add(match);
          return paired ? match : -1;
        });
        // Without a shared cover nothing bridges the two pages, so they cross-fade instead of
        // clearing one before the other (which reads as a blank flash).
        const bridged = pairedOrigins.size > 0;
        if (!bridged) document.documentElement.dataset.albumPairs = "none";
        matches.forEach((match, index) => {
          const element = destination[index];
          if (match >= 0) {
            const name = `album-${direction === "enter" ? "photo" : "tile"}-${match}`;
            mark(direction === "enter" ? thumb(element) : element, name);
            if (direction === "leave") stack(element, name);
          } else {
            mark(element, `album-arrival-${index}`);
            // With a bridge, arrivals wait until the outgoing page has faded so the layouts never double-expose.
            rules.sheet?.insertRule(`:root[data-album-motion]::view-transition-new(album-arrival-${index}){animation:album-tile-unfold 340ms cubic-bezier(.2,.8,.2,1) ${(bridged ? 120 : 40) + Math.min(index * 18, 162)}ms both}`);
            // Without a bridge the arriving images belong in front of the covers that are leaving.
            if (!bridged) rules.sheet?.insertRule(`:root[data-album-motion]::view-transition-group(album-arrival-${index}){z-index:20}`);
          }
        });
        // Old layers without a partner must leave on their own instead of waiting to be covered.
        origin.slice(0, direction === "enter" ? 3 : 24).forEach((_, index) => {
          if (pairedOrigins.has(index)) return;
          const name = `album-${direction === "enter" ? "photo" : "tile"}-${index}`;
          // Unmatched covers leave with their shelf; the back sheets clear first so the fan never looks see-through.
          rules.sheet?.insertRule(bridged
            ? `:root[data-album-motion]::view-transition-old(${name}){animation:album-tile-fold 180ms cubic-bezier(.2,0,0,1) ${Math.min(index * 10, 60)}ms both}`
            : `:root[data-album-motion]::view-transition-old(${name}){animation:album-page-out ${index ? 110 : 160}ms ease-out both}`);
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
