// Display names for enum values, and which states are closed. Anything computed about an item comes from the server.
import type { ItemState, Size } from "../api/client";

export const SIZES: readonly Size[] = ["s", "m", "l", "xl"];

export const SIZE_LABELS: Record<Size, string> = { s: "S", m: "M", l: "L", xl: "XL" };

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
