// Server state lives in TanStack Query. Components read it through the hooks in this directory, one file per
// resource, and never copy it. This file holds the query keys and the cache helpers that several resources' hooks
// share, so prefix-based invalidations line up.
import { type QueryClient } from "@tanstack/react-query";
import { type HeapRef, type ListViewQuery } from "../client";
import type { Item, RankedList, Waiting, WaitingRow } from "../schema";

// Heaps and teams are cached by their slugs, as their paths name them; a new slug is a new path, and the app moves
// there. Every list query of a heap sits under `["ranked", team, heap]`, so one invalidation covers them all. The
// blocker candidates have a key of their own, so they never share a cache entry (and its `as_of`) with a list view.
export const rankedKey = (heap: HeapRef) => ["ranked", heap.team, heap.heap] as const;
export const rankedViewKey = (heap: HeapRef, view: ListViewQuery) => [...rankedKey(heap), "view", view] as const;
export const blockerCandidatesKey = (heap: HeapRef) => [...rankedKey(heap), "blocker-candidates"] as const;
export const closedKey = (heap: HeapRef) => ["closed", heap.team, heap.heap] as const;
export const tagsKey = (heap: HeapRef) => ["tags", heap.team, heap.heap] as const;
export const parametersKey = (heap: HeapRef) => ["parameters", heap.team, heap.heap] as const;
export const itemKey = (heap: HeapRef, key: string) => ["item", heap.team, heap.heap, key] as const;
export const teamKey = (team: string) => ["team", team] as const;
/** The team's own tags, under its key, so invalidating the team refreshes them too. */
export const teamTagsKey = (team: string) => [...teamKey(team), "tags"] as const;
/** The team's own parameters, under its key, as its tags are. */
export const teamParametersKey = (team: string) => [...teamKey(team), "parameters"] as const;
/** Under the team's key, so invalidating the team refreshes its links too. */
export const inviteLinksKey = (team: string) => [...teamKey(team), "invite-links"] as const;
export const heapKey = (heap: HeapRef) => ["heap", heap.team, heap.heap] as const;
export const waitingKey = ["waiting"] as const;
export const decisionsKey = ["decisions"] as const;
/** Every view of the list across heaps, by prefix. */
export const acrossHeapsKey = ["across-heaps"] as const;
export const meKey = ["me"] as const;

/**
 * How often what others change on the server reaches a page left open: the ranked list and the Waiting count are
 * fetched again on this interval. TanStack Query skips it while the tab is hidden.
 */
export const POLL_INTERVAL = 30_000;

export function invalidateHeap(queryClient: QueryClient, heap: HeapRef) {
  return Promise.all([
    // Every view of the list, by prefix.
    queryClient.invalidateQueries({ queryKey: rankedKey(heap) }),
    queryClient.invalidateQueries({ queryKey: acrossHeapsKey }),
    queryClient.invalidateQueries({ queryKey: closedKey(heap) }),
    // Any open item may show a changed blocker or tag, so every cached item is refetched.
    queryClient.invalidateQueries({ queryKey: ["item"] }),
    // An item's state or order changes what waits for the person, and in what order.
    queryClient.invalidateQueries({ queryKey: waitingKey }),
    // Starting, closing or sending on an item takes it off Your decision.
    queryClient.invalidateQueries({ queryKey: decisionsKey }),
  ]);
}

/**
 * After a team tag is made (`renamed` false) or edited: the team's tag list and every heap's tag list of the team,
 * and after an edit also everything that shows tag names in the team's heaps. A merge (`merged`) also rewrote the
 * heaps' parameters' tag links and tag orders, so their parameters and the heaps themselves are read again too.
 */
export function invalidateTeamTags(queryClient: QueryClient, team: string, renamed: boolean, merged = false) {
  return Promise.all([
    ...(merged
      ? [
          queryClient.invalidateQueries({ queryKey: ["parameters", team] }),
          queryClient.invalidateQueries({ queryKey: ["heap", team] }),
        ]
      : []),
    queryClient.invalidateQueries({ queryKey: teamTagsKey(team) }),
    queryClient.invalidateQueries({ queryKey: ["tags", team] }),
    ...(renamed
      ? [
          queryClient.invalidateQueries({ queryKey: ["ranked", team] }),
          queryClient.invalidateQueries({ queryKey: ["closed", team] }),
          queryClient.invalidateQueries({ queryKey: ["item"] }),
          queryClient.invalidateQueries({ queryKey: acrossHeapsKey }),
          queryClient.invalidateQueries({ queryKey: waitingKey }),
        ]
      : []),
  ]);
}

/**
 * After a team parameter is made or changed, or a heap starts or stops using one: the team's list and every heap's
 * parameters of the team, and what shows a team parameter's name, options and values in its heaps. `items` is false
 * for a change that doesn't touch items, such as making one.
 */
export function invalidateTeamParameters(queryClient: QueryClient, team: string, items = true) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: teamParametersKey(team) }),
    queryClient.invalidateQueries({ queryKey: ["parameters", team] }),
    ...(items
      ? [
          queryClient.invalidateQueries({ queryKey: ["ranked", team] }),
          queryClient.invalidateQueries({ queryKey: ["closed", team] }),
          queryClient.invalidateQueries({ queryKey: ["item"] }),
          queryClient.invalidateQueries({ queryKey: acrossHeapsKey }),
          queryClient.invalidateQueries({ queryKey: waitingKey }),
        ]
      : []),
  ]);
}

/** `/me` and what waits for the person, which counts the heaps they are in. */
export function invalidateMe(queryClient: QueryClient) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: meKey }),
    queryClient.invalidateQueries({ queryKey: waitingKey }),
  ]);
}

/**
 * The heaps' open counts in `/me`, which the sidebar shows. Only adding an item or changing its state changes them,
 * so only those refetch `/me`, and nothing waits for it.
 */
export function invalidateOpenCounts(queryClient: QueryClient) {
  void invalidateMe(queryClient);
}

/**
 * Puts the item a turn write answered with into the heap's cached lists, so a row shows the turn, or its end, as soon as
 * the write lands rather than when the refetch after it does: until then the row would still offer to send. Only the
 * ranked lists are changed; the blocker candidates share their key's prefix with another shape.
 */
export function replaceListedItem(queryClient: QueryClient, heap: HeapRef, saved: Item) {
  const swap = (entries: RankedList["ranked"]) =>
    entries.map((entry) => (entry.item.id === saved.id ? { ...entry, item: saved } : entry));
  queryClient.setQueriesData<RankedList>({ queryKey: rankedKey(heap) }, (list) =>
    list && "pinned" in list
      ? { ...list, pinned: swap(list.pinned), ranked: swap(list.ranked), blocked: swap(list.blocked) }
      : list,
  );
}

/** Takes the stuck row of the item `itemId` out of the cached Waiting for me, and out of its count. */
export function dropStuckRow(queryClient: QueryClient, itemId: string) {
  dropWaitingRows(queryClient, itemId, "stuck");
}

/** Takes the rows of `kind` on the item `itemId` out of the cached Waiting for me, and out of its count. */
export function dropWaitingRows(queryClient: QueryClient, itemId: string, kind: WaitingRow["kind"]) {
  queryClient.setQueryData<Waiting>(waitingKey, (waiting) => {
    if (!waiting) return waiting;
    const rows = waiting.rows.filter((row) => !(row.kind === kind && row.item.id === itemId));
    return { ...waiting, rows, count: Math.max(0, waiting.count - (waiting.rows.length - rows.length)) };
  });
}

/** Team changes also change what `/me` lists: teams, roles and heaps. */
export function invalidateTeam(queryClient: QueryClient, team: string) {
  return Promise.all([queryClient.invalidateQueries({ queryKey: teamKey(team) }), invalidateMe(queryClient)]);
}

export const oidcProviderKey = ["oidc-provider"] as const;
export const passwordResetOptionsKey = ["password-reset-options"] as const;
export const registrationOptionsKey = ["registration-options"] as const;
const SERVER_WIDE_KEYS = new Set<unknown>([oidcProviderKey[0], passwordResetOptionsKey[0], registrationOptionsKey[0]]);

/**
 * Drops every cached query of the user whose session ended or changed, so the next one sees none of their data. The
 * server's own settings (its provider, whether it mails reset links, whether registration is open) stay: they are the
 * same for everyone, and a query removed while it loads never answers, which would leave a sign-in page that waits on
 * it with no form.
 * `keep` spares more, such as the reset link a page was opened with.
 */
export function removeUserQueries(queryClient: QueryClient, keep?: (queryKey: readonly unknown[]) => boolean) {
  queryClient.removeQueries({
    predicate: ({ queryKey }) => !SERVER_WIDE_KEYS.has(queryKey[0]) && !keep?.(queryKey),
  });
}

/** The scope that makes edits of items run one at a time. */
export const itemEditsScope = { id: "item-edits" } as const;
