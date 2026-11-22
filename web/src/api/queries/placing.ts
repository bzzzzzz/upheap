import { useMutation, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { api, unwrap, type HeapRef, type ListViewQuery } from "../client";
import type { Item, PlaceSide, RankedList } from "../schema";
import { invalidateHeap, itemEditsScope, itemKey, rankedKey } from "./keys";

/** The newest version of `item` known: the one given, or the cached one if a save has landed since. */
const newestVersion = (queryClient: QueryClient, heap: HeapRef, item: Item) =>
  Math.max(item.version, queryClient.getQueryData<Item>(itemKey(heap, item.key))?.version ?? 0);

/**
 * Puts the ranked row `id` on `side` of the row `anchor`, in every cached view of the heap's list in the priority order that has both in its
 * ranked group. The rest of the list stays as it was, and the `as_of` with it.
 */
function reordered(list: RankedList, id: string, anchor: string, side: PlaceSide): RankedList {
  const moved = list.ranked.find((entry) => entry.item.id === id);
  if (!moved || !list.ranked.some((entry) => entry.item.id === anchor)) return list;
  const others = list.ranked.filter((entry) => entry.item.id !== id);
  const at = others.findIndex((entry) => entry.item.id === anchor) + (side === "below" ? 1 : 0);
  return { ...list, ranked: [...others.slice(0, at), moved, ...others.slice(at)] };
}

/** Shared by every placing, so that a refetch waits for the last of a quick run of them. */
const placingKey = ["placing"] as const;

/** Fetches the heap again once no other placing is under way: an earlier one's list would show the older order. */
const settled = (queryClient: QueryClient, heap: HeapRef) =>
  queryClient.isMutating({ mutationKey: placingKey }) === 1 ? invalidateHeap(queryClient, heap) : undefined;

/** Whether a cached list query is the priority order: the one a placing reorders. Other sorts show another order. */
const isPriorityView = (key: readonly unknown[]) => {
  const query = (key[1] as { query?: ListViewQuery } | undefined)?.query;
  return query?.sort === undefined || query.sort === "priority";
};

/**
 * Places an item next to another of its heap's ranked rows, as the list on screen (`asOf`) shows them. The row takes
 * its new place in the cached lists at once; the saved item is written into the cache, and the heap is fetched again
 * whatever the outcome, which brings the server's order and puts the row back if the placing was refused. It runs in
 * the item edits' scope and sends the newest version it knows of, as `useMoveItem` does.
 */
export function usePlaceItem(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    scope: itemEditsScope,
    mutationKey: placingKey,
    mutationFn: async ({ item, anchor, side, asOf }: { item: Item; anchor: string; side: PlaceSide; asOf: number }) =>
      unwrap(
        await api.POST("/api/v1/teams/{team}/heaps/{heap}/items/{item}/place", {
          params: {
            path: { ...heap, item: item.key },
            header: { "If-Match": `"${newestVersion(queryClient, heap, item)}"` },
          },
          body: { anchor, side, as_of: asOf },
        }),
      ),
    onMutate: async ({ item, anchor, side }) => {
      // A fetch still on its way carries the older order and would write it over this one.
      await queryClient.cancelQueries({ queryKey: rankedKey(heap) });
      queryClient.setQueriesData<RankedList>(
        { queryKey: rankedKey(heap), predicate: (query) => isPriorityView(query.queryKey) },
        (list) => list && reordered(list, item.id, anchor, side),
      );
    },
    onSuccess: (saved) => queryClient.setQueryData(itemKey(heap, saved.key), saved),
    onSettled: () => settled(queryClient, heap),
  });
}

/** Takes an item's place by hand away, so it ranks by its fields again. Sends the newest version known. */
export function useResetPlacement(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    scope: itemEditsScope,
    mutationKey: placingKey,
    mutationFn: async ({ item }: { item: Item }) =>
      unwrap(
        await api.DELETE("/api/v1/teams/{team}/heaps/{heap}/items/{item}/place", {
          params: {
            path: { ...heap, item: item.key },
            header: { "If-Match": `"${newestVersion(queryClient, heap, item)}"` },
          },
        }),
      ),
    onSuccess: (saved) => queryClient.setQueryData(itemKey(heap, saved.key), saved),
    onSettled: () => settled(queryClient, heap),
  });
}
