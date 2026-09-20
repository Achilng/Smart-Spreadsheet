import { restoreScrollPosition, savedScrollPosition } from "./view-state";

interface ViewportScrollOptions {
  key: string;
  active: () => boolean;
  viewport: () => HTMLElement | null;
  resetToken: () => number;
  loading: () => boolean;
  layoutReady: () => boolean;
  extent: () => number;
  apply: (top: number) => void;
}

/** Keep the virtual range and DOM position in sync when a retained view returns. */
export function createViewportScroll(options: ViewportScrollOptions) {
  let restored = $state(false);
  let restoring = false;
  let seenReset = options.resetToken();

  $effect.pre(() => {
    const reset = options.resetToken();
    if (reset !== seenReset) {
      seenReset = reset;
      restored = false;
    }
    if (!options.active()) {
      restored = false;
    } else if (!restored && !options.loading()) {
      options.apply(savedScrollPosition(options.key));
    }
  });

  $effect(() => {
    if (restored || !options.active()) return;
    const viewport = options.viewport();
    if (!viewport || options.loading() || !options.layoutReady()) return;
    // Wait for the spacer to reflect the loaded result before restoring the DOM.
    void options.extent();
    restored = true;
    restoring = true;
    restoreScrollPosition(viewport, options.key, 60, options.apply, () => {
      restoring = false;
    });
  });

  return {
    get restoring() { return restoring; },
    markPositioned() {
      restored = true;
      restoring = false;
    },
  };
}
