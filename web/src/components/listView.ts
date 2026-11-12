// The list view lives in the URL, so a view can be bookmarked, shared and kept on reload. The search params are
// the API's query parameters, with lists comma-separated as the API takes them. Anything else in the URL is dropped.
import type {
  BlockedLayout,
  ItemState,
  ListSort,
  ListViewQuery,
  Need,
  RankedItem,
  RankedList,
  RemindersMode,
  Size,
  StateMode,
} from "../api/client";
import { NEEDS, SIZES } from "./labels";

export type ListSearch = {
  open?: StateMode;
  in_progress?: StateMode;
  blocked?: StateMode;
  blocked_layout?: BlockedLayout;
  /** Whether the reminders to come get a group under the list. */
  reminders?: RemindersMode;
  sort?: ListSort;
  /** The parameter ID the list is sorted by; set exactly when `sort` is `parameter`. */
  sort_parameter?: string;
  /** Tag IDs, comma-separated. */
  tag?: string;
  /** Needs, comma-separated. */
  need?: string;
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

/** The view settings besides the sort, which the Filters panel's Show section sets. */
export type ViewSetting = ShownState | "blocked_layout" | "reminders";
export const VIEW_SETTINGS: readonly ViewSetting[] = [...SHOWN_STATES, "blocked_layout", "reminders"];

export const REMINDERS_MODES: readonly RemindersMode[] = ["hidden", "shown"];
export const REMINDERS_LABELS: Record<RemindersMode, string> = { hidden: "Hidden", shown: "Shown" };
/** What the server does when `reminders` is left out. */
export const DEFAULT_REMINDERS: RemindersMode = "hidden";

/** Whether blocked items are in the list on their own, the one case where their layout matters. */
export const isBlockedShown = (search: ListSearch) => (search.blocked ?? DEFAULT_MODES.blocked) === "shown";

/**
 * The view settings changed from the server's defaults (the URL leaves defaults out). The blocked layout counts only
 * while blocked items are shown, as it has no effect otherwise.
 */
export const viewChanges = (search: ListSearch): ViewSetting[] =>
  VIEW_SETTINGS.filter(
    (setting) => search[setting] !== undefined && (setting !== "blocked_layout" || isBlockedShown(search)),
  );

/** The same view with every state and the blocked layout back at the defaults. */
export const withoutViewChanges = (search: ListSearch): ListSearch => ({
  ...search,
  open: undefined,
  in_progress: undefined,
  blocked: undefined,
  blocked_layout: undefined,
  reminders: undefined,
});

/** The sorts that need no parameter. Sorting by a parameter is offered once per parameter. */
export const SORTS: readonly Exclude<ListSort, "parameter">[] = ["priority", "due", "need", "effort", "impact"];
export const SORT_LABELS: Record<Exclude<ListSort, "parameter">, string> = {
  priority: "Priority",
  due: "Due soonest",
  need: "Most needed",
  effort: "Smallest effort",
  impact: "Largest impact",
};

const oneOf =
  <T extends string>(values: readonly T[]) =>
  (raw: unknown): T | undefined =>
    values.includes(raw as T) ? (raw as T) : undefined;

const mode = oneOf(STATE_MODES);
const layout = oneOf<BlockedLayout>(["grouped", "mixed"]);
const remindersMode = oneOf(REMINDERS_MODES);
const sort = oneOf<ListSort>([...SORTS, "parameter"]);
const list = (raw: unknown) => (typeof raw === "string" && raw !== "" ? raw : undefined);

/**
 * Keeps only the known list view params of a URL, and drops values that equal the server's defaults, so equal
 * views have equal URLs and query keys. Lists are sorted for the same reason. Unknown sizes and needs are dropped too; tag IDs
 * go to the server, which refuses malformed or foreign ones (the page then offers to reset the view).
 */
export function parseListSearch(raw: Record<string, unknown>): ListSearch {
  const modeOf = (state: ShownState) => {
    const value = mode(raw[state]);
    return value === DEFAULT_MODES[state] ? undefined : value;
  };
  // Known values only, once each, in their own order.
  const known = (values: readonly string[], value: unknown) =>
    joinList(
      splitList(list(value))
        .filter((v, index, all) => values.includes(v) && all.indexOf(v) === index)
        .sort((a, b) => values.indexOf(a) - values.indexOf(b)),
    );
  const sizes = (value: unknown) => known(SIZES, value);
  const blocked = modeOf("blocked");
  // The layout only applies to shown blocked items, so a layout left over from them is dropped with them.
  const blockedLayout = isBlockedShown({ blocked }) ? layout(raw.blocked_layout) : undefined;
  const sortParameter = list(raw.sort_parameter);
  // A parameter sort needs its parameter, and no other sort takes one, so a half of the pair alone is dropped.
  const sortBy = sort(raw.sort) === "parameter" && sortParameter === undefined ? undefined : sort(raw.sort);
  const search: ListSearch = {
    open: modeOf("open"),
    in_progress: modeOf("in_progress"),
    blocked,
    blocked_layout: blockedLayout === "grouped" ? undefined : blockedLayout,
    reminders: remindersMode(raw.reminders) === DEFAULT_REMINDERS ? undefined : remindersMode(raw.reminders),
    sort: sortBy === "priority" ? undefined : sortBy,
    sort_parameter: sortBy === "parameter" ? sortParameter : undefined,
    tag: ids(raw.tag),
    need: known(NEEDS, raw.need),
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

/** Toggles `value` in a comma-separated list param. */
export const toggled = (raw: string | undefined, value: string) => {
  const values = splitList(raw);
  return joinList(values.includes(value) ? values.filter((v) => v !== value) : [...values, value]);
};

/** The API query for a view from [`parseListSearch`]. */
export function toListQuery(search: ListSearch): ListViewQuery {
  const query: ListViewQuery = {
    open: search.open,
    in_progress: search.in_progress,
    blocked: search.blocked,
    blocked_layout: search.blocked_layout,
    reminders: search.reminders,
    sort: search.sort,
    sort_parameter: search.sort_parameter,
    tag: splitList(search.tag),
    need: splitList(search.need) as Need[],
    effort: splitList(search.effort) as Size[],
    impact: splitList(search.impact) as Size[],
    param: splitList(search.param),
  };
  // Left-out keys keep the query key, and so the cache, the same for equal views.
  return Object.fromEntries(
    Object.entries(query).filter(([, value]) => value !== undefined && !(Array.isArray(value) && value.length === 0)),
  ) as ListViewQuery;
}

/**
 * The API query for the list on screen: the view's, and while search text is entered, the reminders to come too, so
 * a search finds them whatever the view says (design/ux.md, The list).
 */
export const listQueryFor = (search: ListSearch, text: string): ListViewQuery =>
  toListQuery(text.trim() === "" ? search : { ...search, reminders: "shown" });

/** How many filters are on. */
export const filterCount = (search: ListSearch) =>
  splitList(search.tag).length +
  splitList(search.need).length +
  splitList(search.effort).length +
  splitList(search.impact).length +
  splitList(search.param).length;

/** The same view with no filters on. */
export const withoutFilters = (search: ListSearch): ListSearch => ({
  ...search,
  tag: undefined,
  need: undefined,
  effort: undefined,
  impact: undefined,
  param: undefined,
});

/** Reset all: no filters and the default view settings. The sort stays, as does the search text. */
export const withoutFiltersOrViewChanges = (search: ListSearch): ListSearch =>
  withoutFilters(withoutViewChanges(search));

/**
 * The text search, the one filter the client runs: every word must appear in the title or description, ignoring
 * case. Matches keep the order the server sent.
 */
export function matchesSearch(entry: Pick<RankedItem, "item">, text: string): boolean {
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
