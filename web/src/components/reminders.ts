// Reminders: an item with a date, out of the ranked list until then (design/ux.md, The list). The server decides what
// is ranked; the client only words the date and tells a reminder still to come from one that has arrived.
import type { Item } from "../api/client";

/** Whether `item` is a reminder whose date is still ahead: out of the ranked list, in the reminders group. */
export const isToCome = (item: Pick<Item, "kind" | "arrives_at">, now = Date.now()): boolean =>
  item.kind === "reminder" && item.arrives_at != null && item.arrives_at > now;

/**
 * A date, `YYYY-MM-DD`, as people say it: "12 March", or "12 March 2028" outside the current year. Read as a calendar
 * date, not as a moment, so no time zone moves it.
 */
export function formatDate(date: string, now = Date.now()): string {
  const [year, month, day] = date.split("-").map(Number);
  if (!year || !month || !day) return date;
  return new Intl.DateTimeFormat("en-GB", {
    day: "numeric",
    month: "long",
    year: year === new Date(now).getFullYear() ? undefined : "numeric",
  }).format(new Date(year, month - 1, day));
}
