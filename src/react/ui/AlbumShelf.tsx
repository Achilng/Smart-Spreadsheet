import { useRef, type ReactNode } from "react";
import { useAlbumReflow } from "./use-album-reflow";

export function AlbumShelf({ label, children }: { label: string; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  useAlbumReflow(ref);
  return <div ref={ref} className="r-group-shelf" role="list" aria-label={label}>{children}</div>;
}
