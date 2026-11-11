// Display names for enum values, and which states are closed. Anything computed about an item comes from the server.
import type { ClosedGroup, Goal, Item, ItemState, Kind, Need, Size, Stage } from "../api/client";

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

export const KIND_LABELS: Record<Kind, string> = { note: "Note", task: "Task" };

export const STAGE_LABELS: Record<Stage, string> = {
  as_written: "As written",
  with_context: "With context",
  defined: "Defined",
};

/** Where a turn goes, in a sentence: "going to: with context". */
export const GOAL_LABELS: Record<Goal, string> = {
  with_context: "with context",
  defined: "defined",
  done: "done",
};

/**
 * Where a turn sent now would go: the next stable state of the item. The server works it out when the turn opens and
 * stores it (`turn.goal`); this only tells the Send form what that will be, by the same rule: a note as written goes
 * to with context, a note with context or a task not yet defined to defined, and a defined task to done.
 */
export function nextGoal(item: Pick<Item, "kind" | "stage">): Goal {
  if (item.kind === "task" && item.stage === "defined") return "done";
  if (item.kind === "note" && item.stage !== "with_context" && item.stage !== "defined") return "with_context";
  return "defined";
}

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
