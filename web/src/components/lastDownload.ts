// When this browser last downloaded a copy of the local build's database (design/local-build.md, Keeping the data),
// for the storage note and the Your data card.
//
// An exception to thin clients (engineering-principles.md): the date and its wording are worked out here, not by the
// server, because the fact lives only in this browser's localStorage and no server ever sees it. Storage can be
// unavailable or full, so every read and write is guarded, and then the date is unknown, the same as never.
import { useEffect, useState, useSyncExternalStore } from "react";

const KEY = "upheap.lastDownloadAt";
const DAY = 24 * 60 * 60 * 1000;
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

/**
 * The time now, in epoch milliseconds, for wording dates by the day. It moves when the page becomes visible again and
 * once a day, so a tab left open for weeks still nudges.
 */
export function useNow(): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const update = () => setNow(Date.now());
    const onVisible = () => {
      if (document.visibilityState === "visible") update();
    };
    document.addEventListener("visibilitychange", onVisible);
    const daily = setInterval(update, DAY);
    return () => {
      document.removeEventListener("visibilitychange", onVisible);
      clearInterval(daily);
    };
  }, []);
  return now;
}

/** Whether the storage note should say when the last copy was downloaded: never, or over two weeks before `now`. */
export function isDownloadStale(lastAt: number | null, now: number): boolean {
  return lastAt === null || now - lastAt > STALE_AFTER;
}

/** "3 weeks ago", for a time in the past. Day-month dates and English text, like the server's labels. */
export function formatAgo(at: number, now: number): string {
  const days = Math.max(0, Math.floor((now - at) / DAY));
  const format = new Intl.RelativeTimeFormat("en", { numeric: "auto" });
  if (days < 7) return format.format(-days, "day");
  if (days < 30) return format.format(-Math.floor(days / 7), "week");
  if (days < 365) return format.format(-Math.floor(days / 30), "month");
  return format.format(-Math.floor(days / 365), "year");
}

/** "26 Oct", or "26 Oct 2025" outside the year of `now`, in the browser's time zone. */
export function formatDay(at: number, now: number): string {
  const sameYear = new Date(at).getFullYear() === new Date(now).getFullYear();
  return new Intl.DateTimeFormat("en-GB", {
    day: "numeric",
    month: "short",
    year: sameYear ? undefined : "numeric",
  }).format(at);
}
