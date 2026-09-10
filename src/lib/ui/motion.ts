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

/** animate:flip 的时长：svelte/animate 不读系统偏好，统一从这里取值归零 */
export function flipDuration(duration: number): number {
  return reducedMotion() ? 0 : duration;
}

function responsiveEase(t: number): number {
  return 1 - Math.pow(1 - t, 4);
}

/** 稳定外壳跟随内部自然高度，支持条件增删、换行和动画中途反向。 */
export function animateHeight(node: HTMLElement): { destroy: () => void } {
  const content = node.firstElementChild as HTMLElement;
  const preference = window.matchMedia("(prefers-reduced-motion: reduce)");
  let target = content.getBoundingClientRect().height;
  let animation: Animation | null = null;
  node.style.height = `${target}px`;

  const observer = new ResizeObserver(() => {
    const next = content.getBoundingClientRect().height;
    if (Math.abs(next - target) < 0.5) return;
    // 从当前动画中间帧续接，快速增删时不跳回上一轮起点。
    const current = node.getBoundingClientRect().height;
    animation?.cancel();
    target = next;
    node.style.height = `${target}px`;
    if (preference.matches || Math.abs(current - target) < 0.5) return;
    animation = node.animate(
      [{ height: `${current}px` }, { height: `${target}px` }],
      { duration: 220, easing: "cubic-bezier(0.22, 1, 0.36, 1)" },
    );
  });
  const onPreferenceChange = (): void => {
    if (preference.matches) animation?.cancel();
  };
  observer.observe(content);
  preference.addEventListener("change", onPreferenceChange);
  return {
    destroy: () => {
      observer.disconnect();
      animation?.cancel();
      preference.removeEventListener("change", onPreferenceChange);
    },
  };
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

/** 侧向面板宽度滑入/滑出：从元素当前 CSS 宽度插值到 0，配合内衬层防内容挤压 */
export function panelSlide(
  node: Element,
  { delay = 0, duration = 220 }: MotionParams = {},
): TransitionConfig {
  const reduced = reducedMotion();
  const width = Number.parseFloat(getComputedStyle(node).width) || 0;
  return {
    delay: reduced ? 0 : delay,
    duration: reduced ? 0 : duration,
    easing: responsiveEase,
    css: t =>
      `width: ${(t * width).toFixed(1)}px; opacity: ${Math.min(1, t * 1.6)}; overflow: hidden;`,
  };
}
