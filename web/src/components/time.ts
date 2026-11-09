// Wording for dates by the day, in English and the browser's time zone, like the server's labels. For the Your data
// card and storage note (lastDownload.ts), and the Command line card's sign-ins.
//
// An exception to thin clients (engineering-principles.md) for the local build, whose dates are facts of this browser
// that no server sees. The Command line card words the server's timestamps the same way, since the server sends
// instants and the day depends on this browser's time zone.
import { useEffect, useState } from "react";

export const DAY = 24 * 60 * 60 * 1000;

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
