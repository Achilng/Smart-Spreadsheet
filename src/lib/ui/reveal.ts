/**
 * Reveal 光晕：跟随鼠标的径向高光。
 *
 * 全局事件委托：pointerover 命中 [data-reveal] 元素后，才把 rAF 节流的
 * pointermove 监听绑到该元素自身，pointerleave 时解绑。光晕的绘制由
 * app.css 中 [data-reveal]::after 的径向渐变完成，这里只负责更新圆心
 * （--mx/--my）与直径（--reveal-size = 最大边长 × 1.5）。
 */
export function initReveal(): void {
  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
    return;
  }

  document.addEventListener("pointerover", (event) => {
    const target = event.target;
    if (!(target instanceof Element)) {
      return;
    }
    const el = target.closest<HTMLElement>("[data-reveal]");
    if (!el || el.dataset.revealBound) {
      return;
    }
    el.dataset.revealBound = "1";

    let raf = 0;
    const onMove = (move: PointerEvent) => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => {
        const rect = el.getBoundingClientRect();
        el.style.setProperty("--mx", `${move.clientX - rect.left}px`);
        el.style.setProperty("--my", `${move.clientY - rect.top}px`);
        el.style.setProperty(
          "--reveal-size",
          `${Math.max(rect.width, rect.height) * 1.5}px`,
        );
      });
    };
    const onLeave = () => {
      cancelAnimationFrame(raf);
      el.removeEventListener("pointermove", onMove);
      delete el.dataset.revealBound;
    };

    el.addEventListener("pointermove", onMove);
    el.addEventListener("pointerleave", onLeave, { once: true });
    onMove(event as PointerEvent);
  });
}
