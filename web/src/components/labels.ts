// Display names for enum values, and which states are closed. Anything computed about an item comes from the server.
import type { ItemState, Need, Size } from "../api/client";

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

export const STATES: readonly ItemState[] = ["open", "in_progress", "blocked", "done", "wont_do"];

/** Done and won't do are the closed states. */
export const isClosed = (state: ItemState) => state === "done" || state === "wont_do";
