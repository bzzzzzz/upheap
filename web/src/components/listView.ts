// The list view lives in the URL, so a view can be bookmarked, shared and kept on reload. The search params are
// the API's query parameters, with lists comma-separated as the API takes them. Anything else in the URL is dropped.
import type {
  BlockedLayout,
  ItemState,
  ListSort,
  ListViewQuery,
  RankedItem,
  RankedList,
  Size,
  StateMode,
} from "../api/client";
import { SIZES } from "./labels";

export type ListSearch = {
  open?: StateMode;
  in_progress?: StateMode;
  blocked?: StateMode;
  blocked_layout?: BlockedLayout;
  sort?: ListSort;
  /** The parameter ID the list is sorted by; set exactly when `sort` is `parameter`. */
  sort_parameter?: string;
  /** Tag IDs, comma-separated. */
  tag?: string;
  /** Sizes, comma-separated. */
  effort?: string;
  impact?: string;
  /** Option IDs of custom parameters, comma-separated. */
  param?: string;
};

/** The states whose place in the list the user picks. */
export type ShownState = Extract<ItemState, "open" | "in_progress" | "blocked">;
export const SHOWN_STATES: readonly ShownState[] = ["open", "in_progress", "blocked"];

/** What the server does when a parameter is left out, so controls show the effective value. */
export const DEFAULT_MODES: Record<ShownState, StateMode> = { open: "shown", in_progress: "pinned", blocked: "hidden" };

export const STATE_MODES: readonly StateMode[] = ["pinned", "shown", "hidden"];
export const MODE_LABELS: Record<StateMode, string> = { pinned: "Pinned", shown: "Shown", hidden: "Hidden" };

/** The sorts that need no parameter. Sorting by a parameter is offered once per parameter. */
export const SORTS: readonly Exclude<ListSort, "parameter">[] = ["priority", "due", "effort", "impact"];
export const SORT_LABELS: Record<Exclude<ListSort, "parameter">, string> = {
  priority: "Priority",
  due: "Due soonest",
  effort: "Smallest effort",
  impact: "Largest impact",
};

const oneOf =
  <T extends string>(values: readonly T[]) =>
  (raw: unknown): T | undefined =>
    values.includes(raw as T) ? (raw as T) : undefined;

const mode = oneOf(STATE_MODES);
const layout = oneOf<BlockedLayout>(["grouped", "mixed"]);
const sort = oneOf<ListSort>([...SORTS, "parameter"]);
const list = (raw: unknown) => (typeof raw === "string" && raw !== "" ? raw : undefined);

/**
 * Keeps only the known list view params of a URL, and drops values that equal the server's defaults, so equal
 * views have equal URLs and query keys. Lists are sorted for the same reason. Unknown sizes are dropped too; tag IDs
 * go to the server, which refuses malformed or foreign ones (the page then offers to reset the view).
 */
export function parseListSearch(raw: Record<string, unknown>): ListSearch {
  const modeOf = (state: ShownState) => {
    const value = mode(raw[state]);
    return value === DEFAULT_MODES[state] ? undefined : value;
  };
  const sizes = (value: unknown) =>
    joinList(
      splitList(list(value))
        .filter((size, index, all) => SIZES.includes(size as Size) && all.indexOf(size) === index)
        .sort((a, b) => SIZES.indexOf(a as Size) - SIZES.indexOf(b as Size)),
    );
  const blockedLayout = layout(raw.blocked_layout);
  const sortParameter = list(raw.sort_parameter);
  // A parameter sort needs its parameter, and no other sort takes one, so a half of the pair alone is dropped.
  const sortBy = sort(raw.sort) === "parameter" && sortParameter === undefined ? undefined : sort(raw.sort);
  const search: ListSearch = {
    open: modeOf("open"),
    in_progress: modeOf("in_progress"),
    blocked: modeOf("blocked"),
    blocked_layout: blockedLayout === "grouped" ? undefined : blockedLayout,
    sort: sortBy === "priority" ? undefined : sortBy,
    sort_parameter: sortBy === "parameter" ? sortParameter : undefined,
    tag: ids(raw.tag),
    effort: sizes(raw.effort),
    impact: sizes(raw.impact),
    param: ids(raw.param),
  };
  // Unset keys stay out of the URL.
  return Object.fromEntries(Object.entries(search).filter(([, value]) => value !== undefined)) as ListSearch;
}

const ids = (raw: unknown) => joinList([...new Set(splitList(list(raw)))].sort());

export const splitList = (raw: string | undefined): string[] => (raw ? raw.split(",").filter(Boolean) : []);
export const joinList = (values: readonly string[]): string | undefined =>
  values.length > 0 ? values.join(",") : undefined;

/** The API query for a view from [`parseListSearch`]. */
export function toListQuery(search: ListSearch): ListViewQuery {
  const query: ListViewQuery = {
    open: search.open,
    in_progress: search.in_progress,
    blocked: search.blocked,
    blocked_layout: search.blocked_layout,
    sort: search.sort,
    sort_parameter: search.sort_parameter,
    tag: splitList(search.tag),
    effort: splitList(search.effort) as Size[],
    impact: splitList(search.impact) as Size[],
    param: splitList(search.param),
  };
  // Left-out keys keep the query key, and so the cache, the same for equal views.
  return Object.fromEntries(
    Object.entries(query).filter(([, value]) => value !== undefined && !(Array.isArray(value) && value.length === 0)),
  ) as ListViewQuery;
}

/** How many filters are on. */
export const filterCount = (search: ListSearch) =>
  splitList(search.tag).length +
  splitList(search.effort).length +
  splitList(search.impact).length +
  splitList(search.param).length;

/** The same view with no filters on. */
export const withoutFilters = (search: ListSearch): ListSearch => ({
  ...search,
  tag: undefined,
  effort: undefined,
  impact: undefined,
  param: undefined,
});

/**
 * The text search, the one filter the client runs: every word must appear in the title or description, ignoring
 * case. Matches keep the order the server sent.
 */
export function matchesSearch(entry: RankedItem, text: string): boolean {
  const words = text.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return true;
  const haystack = `${entry.item.title}\n${entry.item.description ?? ""}`.toLowerCase();
  return words.every((word) => haystack.includes(word));
}

/**
 * Where an item sits in the list, as the server ranked it: its entry, with its position within its group, and the
 * group's title, if it shows one. For the item pane's "#3 · Up next" and Add's "Added at #4 in Up next".
 */
export function placeOf(
  list: RankedList,
  itemKey: string,
): { entry: RankedItem; section: string | undefined } | undefined {
  const sections: [RankedItem[], string | undefined][] = [
    [list.pinned, pinnedTitle(list.pinned)],
    [list.ranked, list.pinned.length > 0 ? "Up next" : undefined],
    [list.blocked, "Blocked"],
  ];
  for (const [entries, section] of sections) {
    const entry = entries.find((e) => e.item.key === itemKey);
    if (entry) return { entry, section };
  }
  return undefined;
}

/** The title of the pinned group: In progress while that's all it holds. */
export const pinnedTitle = (pinned: RankedItem[]) =>
  pinned.every((entry) => entry.item.state === "in_progress") ? "In progress" : "Pinned";
