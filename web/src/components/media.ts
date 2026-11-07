// Which layout and input the screen has, for the few behaviours CSS can't switch on its own: gestures, and whether
// a panel is a popover or a sheet. Breakpoints are Tailwind's (app-ux.md, Layout).
import { useEffect, useSyncExternalStore } from "react";

export const PHONE = "(max-width: 639.98px)";
export const TABLET = "(min-width: 640px) and (max-width: 1023.98px)";
/** A finger rather than a mouse: swipe actions and drag-to-close are offered only then. */
export const TOUCH = "(pointer: coarse)";

/** Whether the media query matches, kept current as the window changes. */
export function useMedia(query: string): boolean {
  return useSyncExternalStore(
    (onChange) => {
      const list = window.matchMedia(query);
      list.addEventListener("change", onChange);
      return () => list.removeEventListener("change", onChange);
    },
    () => window.matchMedia(query).matches,
  );
}

/**
 * Keeps `--keyboard-inset` on the root at the height the on-screen keyboard covers, so sheets can sit above it. iOS
 * Safari doesn't shrink the layout for the keyboard, only the visual viewport; Android Chrome resizes the layout
 * (`interactive-widget=resizes-content`), and there the inset stays 0.
 */
export function useKeyboardInset() {
  useEffect(() => {
    const viewport = window.visualViewport;
    if (!viewport) return;
    const update = () => {
      const inset = Math.max(0, window.innerHeight - viewport.height - viewport.offsetTop);
      document.documentElement.style.setProperty("--keyboard-inset", `${Math.round(inset)}px`);
    };
    update();
    viewport.addEventListener("resize", update);
    viewport.addEventListener("scroll", update);
    return () => {
      viewport.removeEventListener("resize", update);
      viewport.removeEventListener("scroll", update);
    };
  }, []);
}
