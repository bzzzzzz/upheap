// What the heap switcher remembers about this browser (design/decisions.md, 32): the heaps opened most recently and
// the teams the user folded. Both are about this device's habits, so they live in localStorage like Add another and
// the theme. Storage can be blocked or hold anything, so every read and write is guarded: a failed read is an empty
// list, a failed write just isn't remembered. Ids that no longer match anything are the reader's to skip.

const RECENT_HEAPS_KEY = "upheap.recentHeaps";
const FOLDED_TEAMS_KEY = "upheap.foldedTeams";

/** How many opened heaps are kept; the switcher shows fewer, as the heap on screen is left out. */
const RECENT_KEPT = 5;

/** The string ids stored under `key`, in order, or none when storage is unavailable or holds something else. */
function readIds(key: string): string[] {
  try {
    const value: unknown = JSON.parse(window.localStorage.getItem(key) ?? "[]");
    return Array.isArray(value) ? value.filter((id): id is string => typeof id === "string") : [];
  } catch {
    return [];
  }
}

function writeIds(key: string, ids: string[]) {
  try {
    window.localStorage.setItem(key, JSON.stringify(ids));
  } catch {
    // Not remembered.
  }
}

/** Heap ids, the one opened last first. */
export function readRecentHeaps(): string[] {
  return readIds(RECENT_HEAPS_KEY);
}

/** Moves the heap to the front of the recent ones. */
export function recordRecentHeap(id: string) {
  writeIds(RECENT_HEAPS_KEY, [id, ...readRecentHeaps().filter((other) => other !== id)].slice(0, RECENT_KEPT));
}

/** Ids of the teams the user folded. */
export function readFoldedTeams(): Set<string> {
  return new Set(readIds(FOLDED_TEAMS_KEY));
}

export function writeFoldedTeams(ids: Iterable<string>) {
  writeIds(FOLDED_TEAMS_KEY, [...ids]);
}
