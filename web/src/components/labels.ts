// Display names for enum values, and which states are closed. Anything computed about an item comes from the server.
import type { ClosedGroup, ItemState, Need, Size } from "../api/client";

export const SIZES: readonly Size[] = ["s", "m", "l", "xl"];

export const SIZE_LABELS: Record<Size, string> = { s: "S", m: "M", l: "L", xl: "XL" };

/** From least to most needed. */
export const NEEDS: readonly Need[] = ["idea", "nice_to_have", "expected", "critical"];

export const NEED_LABELS: Record<Need, string> = {
  idea: "Idea",
  nice_to_have: "Nice to have",
  expected: "Expected",
  critical: "Critical",
};

/** What happens if the item never gets done, which is what tells the steps apart. */
export const NEED_HINTS: Record<Need, string> = {
  idea: "Kept so it isn't forgotten. Nothing is lost if it never happens.",
  nice_to_have: "Wanted, but nobody is waiting for it.",
  expected: "Someone is counting on it. Skipping it lets them down.",
  critical: "It has to happen. Skipping it does real damage.",
};

export const STATE_LABELS: Record<ItemState, string> = {
  open: "Open",
  in_progress: "In progress",
  blocked: "Blocked",
  done: "Done",
  wont_do: "Won't do",
};

/** The headings of the closed list's sections, by the server's `closed_group`. */
export const CLOSED_GROUP_LABELS: Record<ClosedGroup, string> = {
  today: "Today",
  this_week: "This week",
  earlier: "Earlier",
};

export const STATES: readonly ItemState[] = ["open", "in_progress", "blocked", "done", "wont_do"];

/** Done and won't do are the closed states. */
export const isClosed = (state: ItemState) => state === "done" || state === "wont_do";

/**
 * A button that moves an item from `from` to `to`, named for what it does: Start, Block, Done, Won't do, and for
 * going back to open, Unblock, Reopen or Back to open.
 */
export function changeLabel(from: ItemState, to: ItemState): string {
  if (to === "in_progress") return "Start";
  if (to === "blocked") return "Block";
  if (to !== "open") return STATE_LABELS[to];
  if (from === "blocked") return "Unblock";
  return isClosed(from) ? "Reopen" : "Back to open";
}

/**
 * The likeliest next state of a non-closed item, for the item's main button (ux.md, The item §6), then the
 * other states it can move to besides Won't do, which sits apart.
 */
export const NEXT_STATES: Record<Exclude<ItemState, "done" | "wont_do">, readonly [ItemState, ...ItemState[]]> = {
  open: ["in_progress", "blocked", "done"],
  in_progress: ["done", "blocked", "open"],
  blocked: ["open", "in_progress", "done"],
};
