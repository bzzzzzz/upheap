// When this browser last downloaded a copy of the local build's database (design/architecture.md, Local build),
// for the storage note and the Your data card.
//
// An exception to thin clients (engineering-principles.md): the date and its wording are worked out here, not by the
// server, because the fact lives only in this browser's localStorage and no server ever sees it. Storage can be
// unavailable or full, so every read and write is guarded, and then the date is unknown, the same as never.
import { useSyncExternalStore } from "react";

import { DAY } from "./time";

const KEY = "upheap.lastDownloadAt";
/** How old the last copy may be before the storage note mentions it. */
const STALE_AFTER = 14 * DAY;

const listeners = new Set<() => void>();

function readLastDownload(): number | null {
  try {
    const value = Number(window.localStorage.getItem(KEY) ?? NaN);
    return Number.isFinite(value) && value > 0 ? value : null;
  } catch {
    return null;
  }
}

/** Remembers that a copy was downloaded at `at` (epoch milliseconds). */
export function recordDownload(at: number): void {
  try {
    window.localStorage.setItem(KEY, String(at));
  } catch {
    // Unavailable storage leaves the date unknown, which the note treats as never.
  }
  listeners.forEach((listener) => listener());
}

/** When a copy was last downloaded in this browser, in epoch milliseconds, or null if never or unknown. */
export function useLastDownloadAt(): number | null {
  return useSyncExternalStore((listener) => {
    listeners.add(listener);
    return () => listeners.delete(listener);
  }, readLastDownload);
}

/** Whether the storage note should say when the last copy was downloaded: never, or over two weeks before `now`. */
export function isDownloadStale(lastAt: number | null, now: number): boolean {
  return lastAt === null || now - lastAt > STALE_AFTER;
}
