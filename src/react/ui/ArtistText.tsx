import { Fragment, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { artistHighlightSegments, type ArtistHighlightSegment } from "../../lib/api/local-llm";
import { useLibrary } from "../state/library";
import { Hint } from "./controls";
import { ArtistSelectionEvents, ArtistWhitelistMenu } from "./ArtistWhitelistMenu";
import "./artist-text.css";

const useHighlightRevision = create(() => ({ revision: 0 }));
const cache = new Map<string, Promise<ArtistHighlightSegment[]>>();
type Pending = { text: string; resolve: (parts: ArtistHighlightSegment[]) => void; reject: (error: unknown) => void };
let pending: Pending[] = [];

export function invalidateArtistHighlights(): void {
  cache.clear();
  useHighlightRevision.setState(state => ({ revision: state.revision + 1 }));
}

// Coalesce visible rows into one IPC request and reuse identical artist strings.
function loadSegments(text: string): Promise<ArtistHighlightSegment[]> {
  const existing = cache.get(text);
  if (existing) return existing;
  const revision = useHighlightRevision.getState().revision;
  const result = new Promise<ArtistHighlightSegment[]>((resolve, reject) => {
    pending.push({ text, resolve, reject });
    if (pending.length !== 1) return;
    setTimeout(() => {
      const batch = pending;
      pending = [];
      void artistHighlightSegments(batch.map(item => item.text)).then(parts => {
        batch.forEach((item, index) => item.resolve(parts[index] ?? [{ text: item.text, ignored: false }]));
      }, error => batch.forEach(item => item.reject(error)));
    }, 0);
  }).catch(error => {
    if (revision === useHighlightRevision.getState().revision && cache.get(text) === result) cache.delete(text);
    throw error;
  });
  // Bound memory while scrolling large libraries.
  if (cache.size >= 512) cache.delete(cache.keys().next().value!);
  cache.set(text, result);
  return result;
}

/** One listener set per window; library changes discard old requests and cached rules. */
export function ArtistHighlightEvents() {
  useEffect(() => {
    let disposed = false;
    const cleanups = [useLibrary.subscribe((next, previous) => {
      if (next.snapshot?.dataDirectory !== previous.snapshot?.dataDirectory) invalidateArtistHighlights();
    })];
    const windowType = new URLSearchParams(location.search).get("window") ?? "main";
    const windowEvent = windowType === "toolbox" ? "main://library-changed" : windowType === "compare" ? "main://library-reset" : "toolbox://app-state-changed";
    for (const event of ["artist-matching://changed", windowEvent]) {
      void listen(event, invalidateArtistHighlights).then(stop => {
        if (disposed) stop(); else cleanups.push(stop);
      }).catch(() => {});
    }
    return () => { disposed = true; cleanups.forEach(stop => stop()); invalidateArtistHighlights(); };
  }, []);
  return <ArtistSelectionEvents />;
}

export function ArtistText({ text }: { text: string }) {
  const revision = useHighlightRevision(state => state.revision);
  const [loaded, setLoaded] = useState<{ text: string; revision: number; parts: ArtistHighlightSegment[] } | null>(null);
  useEffect(() => {
    let active = true;
    if (text) void loadSegments(text).then(parts => {
      if (active) setLoaded({ text, revision, parts });
    }).catch(() => { if (active) setLoaded(null); });
    return () => { active = false; };
  }, [text, revision]);
  const content = loaded?.text !== text || loaded.revision !== revision ? text : loaded.parts.map((part, index) => part.ignored
    ? <Hint key={index} text="这些tag会在比较画师串时被忽略"><mark className="r-artist-ignored" tabIndex={0} title="">{part.text}</mark></Hint>
    : <Fragment key={index}>{part.text}</Fragment>);
  return <ArtistWhitelistMenu text={text} revision={revision} onChanged={invalidateArtistHighlights}>{content}</ArtistWhitelistMenu>;
}
