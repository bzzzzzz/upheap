// Server state lives in TanStack Query. Components read it through the hooks in this directory, one file per
// resource, and never copy it. This file holds the query keys and the cache helpers that several resources' hooks
// share. Every key is built from the path it reads, so prefix-based invalidations line up and a path that isn't in
// the API fails the type check.
import { type QueryClient } from "@tanstack/react-query";
import { type HeapRef, type ListViewQuery } from "../client";
import type { Item, paths, RankedList, Waiting, WaitingRow } from "../schema";

/** The operation a path's query reads: its `get`, or its `post` for a path with no `get`. */
type ReadOperation<P extends keyof paths> = [NonNullable<paths[P]["get"]>] extends [never]
  ? NonNullable<paths[P]["post"]>
  : NonNullable<paths[P]["get"]>;
type OperationParameter<O, K extends "path" | "query"> = O extends { parameters: { [Part in K]?: infer T } }
  ? NonNullable<T>
  : never;
type OperationBody<O> = O extends { requestBody: { content: { "application/json": infer B } } } ? B : never;

/** What a key names of its path, as openapi-fetch's `params` does. Some path parameters are enough, to match several queries. */
export type ApiKeyParams<P extends keyof paths> = {
  path?: Partial<OperationParameter<ReadOperation<P>, "path">>;
  query?: OperationParameter<ReadOperation<P>, "query">;
  body?: Partial<OperationBody<ReadOperation<P>>>;
};

/**
 * The query key of a path and the parameters it was read with. A path that isn't in `paths`, or a parameter it doesn't
 * have, fails the type check. Without `params` the key is `[path]` and not `[path, undefined]`, which would match no
 * query with params; TanStack matches the object part of a key partially, so fewer parameters cover every query that
 * has them. That is what `Partial` path parameters are for: a hook's own key names every one, through a wrapper here
 * or `heapPath`.
 */
export function apiKey<P extends keyof paths>(path: P, params?: ApiKeyParams<P>) {
  return params === undefined ? ([path] as const) : ([path, params] as const);
}

const ITEMS = "/api/v1/teams/{team}/heaps/{heap}/items";

/**
 * A heap's path parameters, and nothing else. A key holds its object part whole, so a `HeapRef` that carries more (the
 * heap moved to also has its `id`) would make a key no hook reads.
 */
export const heapPath = (heap: HeapRef): HeapRef => ({ team: heap.team, heap: heap.heap });

/** Every non-closed item, unfiltered: what an item can be blocked by, whatever the list view shows. */
export const EVERY_NON_CLOSED_ITEM: ListViewQuery = { open: "shown", in_progress: "shown", blocked: "shown" };

// Heaps and teams are cached by their slugs, as their paths name them; a new slug is a new path, and the app moves
// there. Every list query of a heap sits under `apiKey(ITEMS, { path: heap })`, so one invalidation covers them all.
export const rankedKey = (heap: HeapRef) => apiKey(ITEMS, { path: heapPath(heap) });
export const rankedViewKey = (heap: HeapRef, view: ListViewQuery) =>
  apiKey(ITEMS, { path: heapPath(heap), query: view });
// The marker keeps the candidates apart from a list view whose query is `EVERY_NON_CLOSED_ITEM`, which would share
// their cache entry (and its `as_of`).
export const blockerCandidatesKey = (heap: HeapRef) =>
  [...apiKey(ITEMS, { path: heapPath(heap), query: EVERY_NON_CLOSED_ITEM }), "blocker-candidates"] as const;
export const closedKey = (heap: HeapRef) =>
  apiKey("/api/v1/teams/{team}/heaps/{heap}/items/closed", { path: heapPath(heap) });
export const tagsKey = (heap: HeapRef) => apiKey("/api/v1/teams/{team}/heaps/{heap}/tags", { path: heapPath(heap) });
export const savedViewsKey = (heap: HeapRef) =>
  apiKey("/api/v1/teams/{team}/heaps/{heap}/views", { path: heapPath(heap) });
export const parametersKey = (heap: HeapRef) =>
  apiKey("/api/v1/teams/{team}/heaps/{heap}/parameters", { path: heapPath(heap) });
export const itemKey = (heap: HeapRef, key: string) =>
  apiKey("/api/v1/teams/{team}/heaps/{heap}/items/{item}", { path: { ...heapPath(heap), item: key } });
export const teamKey = (team: string) => apiKey("/api/v1/teams/{team}", { path: { team } });
export const teamTagsKey = (team: string) => apiKey("/api/v1/teams/{team}/tags", { path: { team } });
export const teamParametersKey = (team: string) => apiKey("/api/v1/teams/{team}/parameters", { path: { team } });
export const inviteLinksKey = (team: string) => apiKey("/api/v1/teams/{team}/invite-links", { path: { team } });
export const heapKey = (heap: HeapRef) => apiKey("/api/v1/teams/{team}/heaps/{heap}", { path: heapPath(heap) });
export const waitingKey = apiKey("/api/v1/me/waiting");
export const decisionsKey = apiKey("/api/v1/me/decisions");
/** Every view of the list across heaps, by prefix. */
export const acrossHeapsKey = apiKey("/api/v1/me/items");
export const meKey = apiKey("/api/v1/me");
/** Every cached item, in every heap. */
export const everyItemKey = apiKey("/api/v1/teams/{team}/heaps/{heap}/items/{item}");

/**
 * Every history: a heap's, a team's, the account's and an account's as an operator reads it. Every write is audited, so
 * `main.tsx` refreshes them all after any mutation. A new history hook's path goes here too.
 */
export const historyKeys = [
  apiKey("/api/v1/teams/{team}/heaps/{heap}/audit"),
  apiKey("/api/v1/teams/{team}/audit"),
  apiKey("/api/v1/me/history"),
  apiKey("/api/v1/operator/accounts/{user_id}/history"),
];

/**
 * The team's own queries: its record, tags, parameters, invite links and data. With no `team`, every team's. A new
 * hook that reads a team's own path goes here too, or changing or leaving the team leaves it stale.
 */
export function teamKeys(team?: string) {
  const params = team === undefined ? undefined : { path: { team } };
  return [
    apiKey("/api/v1/teams/{team}", params),
    apiKey("/api/v1/teams/{team}/tags", params),
    apiKey("/api/v1/teams/{team}/parameters", params),
    apiKey("/api/v1/teams/{team}/invite-links", params),
    apiKey("/api/v1/teams/{team}/data", params),
  ];
}

/**
 * The heap's record and its ranking previews: what shows its name, archive state and weights. With no `heap`, those of
 * every heap of the team. A new query that shows those goes here too.
 */
export function heapRecordKeys(heap: { team: string; heap?: string }) {
  const path = heap.heap === undefined ? { team: heap.team } : { team: heap.team, heap: heap.heap };
  return [
    apiKey("/api/v1/teams/{team}/heaps/{heap}", { path }),
    apiKey("/api/v1/teams/{team}/heaps/{heap}/ranking/preview", { path }),
  ];
}

/**
 * How often what others change on the server reaches a page left open: the ranked list, the Waiting and Agents counts
 * and the heaps' open counts are fetched again on this interval. TanStack Query skips it while the tab is hidden.
 */
export const POLL_INTERVAL = 30_000;

export function invalidateHeap(queryClient: QueryClient, heap: HeapRef) {
  return Promise.all([
    // Every view of the list, by prefix.
    queryClient.invalidateQueries({ queryKey: rankedKey(heap) }),
    queryClient.invalidateQueries({ queryKey: acrossHeapsKey }),
    queryClient.invalidateQueries({ queryKey: closedKey(heap) }),
    // Any open item may show a changed blocker or tag, so every cached item is refetched.
    queryClient.invalidateQueries({ queryKey: everyItemKey }),
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
          queryClient.invalidateQueries({
            queryKey: apiKey("/api/v1/teams/{team}/heaps/{heap}/parameters", { path: { team } }),
          }),
          ...heapRecordKeys({ team }).map((queryKey) => queryClient.invalidateQueries({ queryKey })),
        ]
      : []),
    queryClient.invalidateQueries({ queryKey: teamTagsKey(team) }),
    queryClient.invalidateQueries({ queryKey: apiKey("/api/v1/teams/{team}/heaps/{heap}/tags", { path: { team } }) }),
    ...(renamed
      ? [
          queryClient.invalidateQueries({ queryKey: apiKey(ITEMS, { path: { team } }) }),
          queryClient.invalidateQueries({
            queryKey: apiKey("/api/v1/teams/{team}/heaps/{heap}/items/closed", { path: { team } }),
          }),
          queryClient.invalidateQueries({ queryKey: everyItemKey }),
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
    queryClient.invalidateQueries({
      queryKey: apiKey("/api/v1/teams/{team}/heaps/{heap}/parameters", { path: { team } }),
    }),
    ...(items
      ? [
          queryClient.invalidateQueries({ queryKey: apiKey(ITEMS, { path: { team } }) }),
          queryClient.invalidateQueries({
            queryKey: apiKey("/api/v1/teams/{team}/heaps/{heap}/items/closed", { path: { team } }),
          }),
          queryClient.invalidateQueries({ queryKey: everyItemKey }),
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
 * The heaps' open counts in `/me`, which the sidebar shows. Of the person's own writes only adding an item or changing
 * its state changes them, so only those refetch `/me`, and nothing waits for it. What others change reaches them on the
 * shell's poll (`usePolledMe`).
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
  return Promise.all([
    ...teamKeys(team).map((queryKey) => queryClient.invalidateQueries({ queryKey })),
    invalidateMe(queryClient),
  ]);
}

export const oidcProviderKey = apiKey("/api/v1/auth/oidc");
export const passwordResetOptionsKey = apiKey("/api/v1/password-resets");
export const registrationOptionsKey = apiKey("/api/v1/auth/registration");
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

/** The key of `useUpdateItem`'s mutations in `heap`, so `useSavingFields` finds those pending, whichever component pressed. */
export const itemEditKey = (heap: HeapRef) => ["item-edit", heapPath(heap)] as const;
