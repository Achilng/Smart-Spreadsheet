import { useLayoutEffect, useRef, type RefObject } from "react";
import { useReducedMotionPreference } from "./use-reduced-motion";

type Position = { x: number; y: number };

/** Animate row changes without measuring or animating the entire bookshelf.
 * Positions are relative to the scrolling surface, so sidebar motion and scrolling
 * don't become extra animation offsets. Only the outer frame owns this transform.
 */
export function useAlbumReflow(ref: RefObject<HTMLDivElement | null>) {
  const reducedMotion = useReducedMotionPreference();
  const reduced = useRef(reducedMotion);
  const refresh = useRef<() => void>(() => {});

  useLayoutEffect(() => {
    const root = ref.current;
    if (!root) return;
    const viewport = root.closest<HTMLElement>(".r-section-list, .rm-viewport") ?? root;
    const tracked = new Set<HTMLElement>();
    const nearby = new Set<HTMLElement>();
    const positions = new Map<HTMLElement, Position>();
    const animations = new Map<HTMLElement, Animation>();
    let columns: number | undefined;

    const cancel = (node: HTMLElement) => {
      animations.get(node)?.cancel();
      animations.delete(node);
    };
    const measure = () => {
      const bounds = root.getBoundingClientRect();
      if (!bounds.width || !bounds.height) {
        animations.forEach((_, node) => cancel(node));
        positions.clear();
        columns = undefined;
        return;
      }
      const nextColumns = root.dataset.albumColumns
        ? Number(root.dataset.albumColumns)
        : getComputedStyle(root).gridTemplateColumns.split(" ").length;
      const rearranged = columns !== undefined && columns !== nextColumns;
      columns = nextColumns;
      // Batch layout reads before any animation writes. IO keeps this set bounded
      // by the viewport; in-flight items remain tracked until their motion ends.
      const measured = [...new Set([...nearby, ...animations.keys()])].map(node => {
        const box = node.getBoundingClientRect();
        const transform = animations.has(node) ? new DOMMatrixReadOnly(getComputedStyle(node).transform) : null;
        const tx = transform?.m41 ?? 0, ty = transform?.m42 ?? 0;
        const next = { x: box.left - bounds.left + root.scrollLeft - tx, y: box.top - bounds.top + root.scrollTop - ty };
        const previous = positions.get(node);
        return { node, next, dx: previous ? previous.x + tx - next.x : 0, dy: previous ? previous.y + ty - next.y : 0 };
      });
      for (const { node, next, dx, dy } of measured) {
        positions.set(node, next);
        if (reduced.current) { cancel(node); continue; }
        if (!rearranged) continue;
        cancel(node);
        if (Math.abs(dx) < .5 && Math.abs(dy) < .5) continue;
        const animation = node.animate([
          { transform: `translate(${dx}px, ${dy}px)` },
          { transform: "translate(0px, 0px)" },
        ], { duration: 280, easing: "cubic-bezier(.22, 1, .36, 1)" });
        animations.set(node, animation);
        animation.onfinish = () => {
          if (animations.get(node) !== animation) return;
          animations.delete(node);
          if (!nearby.has(node)) positions.delete(node);
        };
      }
    };
    const visibility = new IntersectionObserver(entries => {
      for (const entry of entries) {
        const node = entry.target as HTMLElement;
        if (!tracked.has(node)) continue;
        if (entry.isIntersecting) nearby.add(node);
        else {
          nearby.delete(node);
          if (!animations.has(node)) positions.delete(node);
        }
      }
      measure();
    }, { root: viewport, rootMargin: "160px" });
    const resize = new ResizeObserver(measure);
    resize.observe(root);
    refresh.current = () => {
      // Only run registration on React commits, never on each resize frame.
      const nodes = new Set(root.querySelectorAll<HTMLElement>("[data-album-reflow]"));
      for (const node of tracked) {
        if (nodes.has(node)) continue;
        visibility.unobserve(node);
        tracked.delete(node);
        nearby.delete(node);
        positions.delete(node);
        cancel(node);
      }
      for (const node of nodes) {
        if (tracked.has(node)) continue;
        tracked.add(node);
        visibility.observe(node);
      }
      measure();
    };
    return () => {
      refresh.current = () => {};
      resize.disconnect();
      visibility.disconnect();
      animations.forEach((_, node) => cancel(node));
    };
  }, [ref]);

  // Materials commit new column positions and CSS geometry before this hook runs.
  // Also cancel immediately if the OS motion preference changes during an animation.
  useLayoutEffect(() => {
    reduced.current = reducedMotion;
    refresh.current();
  });
}
