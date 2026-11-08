import { useLayoutEffect, useRef } from "react";
import { galleryTransition } from "./GalleryMotion";
import { useReducedMotionPreference } from "./use-reduced-motion";

/** Animate CSS wrapping, which can happen between React renders as panels resize. */
export function useCanvasHeaderMotion(view: string) {
  const frame = useRef<HTMLDivElement>(null);
  const header = useRef<HTMLElement>(null);
  const reducedMotion = useReducedMotionPreference();

  useLayoutEffect(() => {
    const shell = frame.current, content = header.current;
    if (!shell || !content) return;
    // CanvasHeader is followed by FilterLayout in the gallery/table shell.
    const viewport = shell.nextElementSibling as HTMLElement | null;
    if (!viewport?.classList.contains("r-filter-layout")) return;
    const actions = content.querySelector<HTMLElement>(".r-page-actions");
    const title = content.querySelector<HTMLElement>(".r-page-title");
    if (!actions || !title) return;
    const items = [title, ...Array.from(actions.children) as HTMLElement[]];
    const positions = new Map<HTMLElement, { x: number; y: number }>();
    const movements = new Map<HTMLElement, Animation>();
    let height: number | null = null;
    let resizing: Animation | null = null;
    const timing = {
      duration: galleryTransition.duration * 1000,
      easing: `cubic-bezier(${galleryTransition.ease.join(",")})`,
    };
    const start = (animation: Animation) => {
      // A simultaneous column reflow can occupy the current frame. Hold the
      // inverted pose until the next frame so that work cannot eat the tween.
      animation.pause();
      animation.currentTime = 0;
      requestAnimationFrame(() => { if (animation.playState === "paused") animation.play(); });
      return animation;
    };

    const measure = () => {
      // The positioned header is the offset parent, so animation transforms and
      // the sliding sidebar do not contaminate these natural layout positions.
      const nextPositions = items.map(item => ({ item, x: item.offsetLeft, y: item.offsetTop }));
      const nextHeight = content.getBoundingClientRect().height;
      if (height !== nextHeight) {
        const reserved = shell.getBoundingClientRect().height;
        const offset = parseFloat(getComputedStyle(viewport).translate.split(" ")[1]) || 0;
        resizing?.cancel();
        resizing = null;
        if (height === null || reducedMotion) {
          shell.style.height = `${nextHeight}px`;
          viewport.style.translate = "none";
        } else {
          // Reserve the smaller height and move the canvas on the compositor.
          // Resize the virtual viewport only at the endpoints, not every frame.
          const base = Math.min(reserved, nextHeight);
          shell.style.height = `${base}px`;
          viewport.style.translate = `0px ${nextHeight - base}px`;
          const animation = start(viewport.animate([
            { translate: `0px ${reserved + offset - base}px` },
            { translate: `0px ${nextHeight - base}px` },
          ], timing));
          resizing = animation;
          animation.onfinish = () => {
            if (resizing !== animation) return;
            resizing = null;
            shell.style.height = `${nextHeight}px`;
            viewport.style.translate = "none";
          };
        }
        height = nextHeight;
      }
      for (const { item, x, y } of nextPositions) {
        const previous = positions.get(item);
        // Follow continuous horizontal resizing directly. Only a row change
        // needs FLIP, including an interrupted transition back to the first row.
        if (previous && previous.y !== y && !reducedMotion) {
          const translate = getComputedStyle(item).translate;
          const [currentX = 0, currentY = 0] = translate === "none" ? [] : translate.split(" ").map(parseFloat);
          movements.get(item)?.cancel();
          const animation = start(item.animate([
            { translate: `${previous.x - x + currentX}px ${previous.y - y + currentY}px` },
            { translate: "0px 0px" },
          ], timing));
          movements.set(item, animation);
          animation.onfinish = () => { if (movements.get(item) === animation) movements.delete(item); };
        }
        positions.set(item, { x, y });
      }
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(content);
    observer.observe(actions);
    observer.observe(title);
    return () => {
      observer.disconnect();
      resizing?.cancel();
      viewport.style.translate = "none";
      movements.forEach(animation => animation.cancel());
    };
  }, [view, reducedMotion]);

  return { frame, header };
}
