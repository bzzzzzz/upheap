// The colour theme the user picked on the Account page (design/app-ux.md, Toolkit): System, the default, follows the
// device; Light and Dark set `data-theme` on <html>, which index.css reads. Remembered per browser, in localStorage,
// like Add another, since it's about the screen in front of the user rather than their account. The inline script in
// index.html applies it before the first paint, so it reads the same key; keep the two in step. Storage can be blocked
// or unavailable, so every read and write is guarded, and then the theme is System.
import { useSyncExternalStore } from "react";

export type Theme = "system" | "light" | "dark";

const KEY = "upheap.theme";

const listeners = new Set<() => void>();

function readTheme(): Theme {
  try {
    const value = window.localStorage.getItem(KEY);
    return value === "light" || value === "dark" ? value : "system";
  } catch {
    return "system";
  }
}

/** Sets or clears `data-theme` on <html>. System needs nothing there: index.css follows the device then. */
function applyTheme(theme: Theme) {
  const root = document.documentElement;
  if (theme === "system") delete root.dataset.theme;
  else root.dataset.theme = theme;
}

/** Applies `theme` now and remembers it for this browser. */
function setTheme(theme: Theme) {
  try {
    if (theme === "system") window.localStorage.removeItem(KEY);
    else window.localStorage.setItem(KEY, theme);
  } catch {
    // Not remembered, but it still holds until the page reloads.
  }
  applyTheme(theme);
  listeners.forEach((listener) => listener());
}

/**
 * Follows a theme picked in another tab, which reaches this one as a `storage` event; a `null` key means the other
 * tab cleared all storage. Called once, at startup.
 */
export function followThemeAcrossTabs() {
  window.addEventListener("storage", (event) => {
    if (event.key !== KEY && event.key !== null) return;
    applyTheme(readTheme());
    listeners.forEach((listener) => listener());
  });
}

/** The theme picked in this browser, System when none is, and the way to change it. */
export function useTheme(): [Theme, (theme: Theme) => void] {
  const theme = useSyncExternalStore((listener) => {
    listeners.add(listener);
    return () => listeners.delete(listener);
  }, readTheme);
  return [theme, setTheme];
}
