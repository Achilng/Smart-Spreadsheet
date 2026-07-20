import type { TransitionConfig } from "svelte/transition";

interface MotionParams {
  delay?: number;
  duration?: number;
  x?: number;
  y?: number;
  start?: number;
}

function reducedMotion(): boolean {
  return typeof window !== "undefined" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

function responsiveEase(t: number): number {
  return 1 - Math.pow(1 - t, 4);
}

function baseStyle(node: Element): { opacity: number; transform: string } {
  const style = getComputedStyle(node);
  const parsedOpacity = Number.parseFloat(style.opacity);
  return {
    opacity: Number.isFinite(parsedOpacity) ? parsedOpacity : 1,
    transform: style.transform === "none" ? "" : `${style.transform} `,
  };
}

export function softFade(
  node: Element,
  { delay = 0, duration = 150 }: MotionParams = {},
): TransitionConfig {
  const reduced = reducedMotion();
  const style = baseStyle(node);
  return {
    delay: reduced ? 0 : delay,
    duration: reduced ? 0 : duration,
    easing: responsiveEase,
    css: t => `opacity: ${t * style.opacity}`,
  };
}

export function softFly(
  node: Element,
  {
    delay = 0,
    duration = 180,
    x = 0,
    y = 6,
  }: MotionParams = {},
): TransitionConfig {
  const reduced = reducedMotion();
  const style = baseStyle(node);
  const dx = reduced ? 0 : x;
  const dy = reduced ? 0 : y;
  return {
    delay: reduced ? 0 : delay,
    duration: reduced ? 0 : duration,
    easing: responsiveEase,
    css: (t, u) =>
      `opacity: ${t * style.opacity}; transform: ${style.transform}translate3d(${u * dx}px, ${u * dy}px, 0)`,
  };
}

export function softPop(
  node: Element,
  {
    delay = 0,
    duration = 190,
    y = 6,
    start = 0.985,
  }: MotionParams = {},
): TransitionConfig {
  const reduced = reducedMotion();
  const style = baseStyle(node);
  const dy = reduced ? 0 : y;
  const initialScale = reduced ? 1 : start;
  return {
    delay: reduced ? 0 : delay,
    duration: reduced ? 0 : duration,
    easing: responsiveEase,
    css: (t, u) => {
      const scale = initialScale + (1 - initialScale) * t;
      return `opacity: ${t * style.opacity}; transform: ${style.transform}translate3d(0, ${u * dy}px, 0) scale(${scale})`;
    },
  };
}
