import { useEffect, useRef, type ReactNode, type PointerEvent } from "react";
import { Images } from "lucide-react";
import "./album.css";

/** Transform only: opening an album never moves neighboring cards. Front image first. */
export function AlbumStack({ images, empty = "暂无图片" }: { images: ReactNode[]; empty?: string }) {
  const root = useRef<HTMLSpanElement>(null), frame = useRef(0);
  function reset() {
    cancelAnimationFrame(frame.current);
    root.current?.style.setProperty("--album-rx", "0deg");
    root.current?.style.setProperty("--album-ry", "0deg");
  }
  useEffect(() => {
    const reduced = matchMedia("(prefers-reduced-motion: reduce)");
    window.addEventListener("blur", reset); reduced.addEventListener("change", reset);
    return () => { cancelAnimationFrame(frame.current); window.removeEventListener("blur", reset); reduced.removeEventListener("change", reset); };
  }, []);
  function move(event: PointerEvent<HTMLSpanElement>) {
    if (event.pointerType === "touch" || matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const box = event.currentTarget.getBoundingClientRect();
    const x = Math.max(-1, Math.min(1, (event.clientX - box.left) / box.width * 2 - 1));
    const y = Math.max(-1, Math.min(1, (event.clientY - box.top) / box.height * 2 - 1));
    cancelAnimationFrame(frame.current);
    frame.current = requestAnimationFrame(() => {
      root.current?.style.setProperty("--album-rx", `${-y * 4}deg`);
      root.current?.style.setProperty("--album-ry", `${x * 5}deg`);
    });
  }
  return <span className="r-album-stack" ref={root} onPointerMove={move} onPointerLeave={reset} onPointerCancel={reset}>
    {images.length ? images.slice(0, 3).map((image, index) => <span key={index} className={`r-album-sheet r-album-sheet-${index}`}>{image}</span>) : <span className="r-album-sheet r-album-sheet-0 r-album-empty"><Images size={28} /><span>{empty}</span></span>}
  </span>;
}
