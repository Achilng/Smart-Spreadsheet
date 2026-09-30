import { useSyncExternalStore } from "react";

let preference: MediaQueryList | undefined;
const media = () => preference ??= window.matchMedia("(prefers-reduced-motion: reduce)");
const snapshot = () => media().matches;
const serverSnapshot = () => false;
const subscribe = (notify: () => void) => {
  const query = media();
  query.addEventListener("change", notify);
  return () => query.removeEventListener("change", notify);
};

/** Keep JS animations in sync with live OS changes, just like the CSS media query. */
export function useReducedMotionPreference() {
  return useSyncExternalStore(subscribe, snapshot, serverSnapshot);
}
