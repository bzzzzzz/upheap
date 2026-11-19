import { useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { api, unwrap, type HeapRef } from "../client";
import type { CreateItemRequest, Item, ItemPatchRequest, ItemState } from "../schema";
import {
  itemKey,
  invalidateHeap,
  invalidateOpenCounts,
  replaceListedItem,
  dropWaitingRows,
  itemEditsScope,
} from "./keys";

/**
 * One item of `heap`, by its key, for the item pane. Each change saves on its own, so there's no unsaved form for a
 * refetch to reset.
 */
export function useItem(heap: HeapRef, key: string) {
  return useQuery({
    queryKey: itemKey(heap, key),
    queryFn: async () =>
      unwrap(
        await api.GET("/api/v1/teams/{team}/heaps/{heap}/items/{item}", { params: { path: { ...heap, item: key } } }),
      ),
  });
}

/**
 * The rough due times a due picker offers, from the server, so a picked choice reads back with the label it was
 * picked by. Their dates move with the day, so they're fetched again after a while.
 */
export function useDueChoices() {
  return useQuery({
    queryKey: ["due-choices"],
    queryFn: async () => unwrap(await api.GET("/api/v1/due-choices")).choices,
    staleTime: 10 * 60_000,
  });
}

/**
 * Adds an item to `heap`. The heap goes with each call rather than with the hook, as the Add dialog can change heap
 * while an add is on its way, and the views to fetch again are those of the heap the item went into.
 */
export function useCreateItem() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ heap, body }: { heap: HeapRef; body: CreateItemRequest }) =>
      unwrap(await api.POST("/api/v1/teams/{team}/heaps/{heap}/items", { params: { path: heap }, body })),
    // Not returned: the mutation is done once the server has the item, and the lists catch up after.
    onSettled: (_item, _error, { heap }) => {
      invalidateOpenCounts(queryClient);
      void invalidateHeap(queryClient, heap);
    },
  });
}

/**
 * Saves `patch` to `item` at the newest version known: its own, or the one an earlier save brought into the cache.
 * Callers run it in the item edits' scope, so saves go one at a time.
 */
async function patchAtNewestVersion(
  queryClient: QueryClient,
  heap: HeapRef,
  item: Pick<Item, "key" | "version">,
  patch: ItemPatchRequest,
) {
  const version = Math.max(item.version, queryClient.getQueryData<Item>(itemKey(heap, item.key))?.version ?? 0);
  return unwrap(
    await api.PATCH("/api/v1/teams/{team}/heaps/{heap}/items/{item}", {
      params: { path: { ...heap, item: item.key }, header: { "If-Match": `"${version}"` } },
      body: patch,
    }),
  );
}

/**
 * Saves one change to an item. The item pane saves each change as it's made, so two can follow each other before
 * the first is back: edits run one at a time (one `scope`), and each sends the newest version it knows of, its own
 * item's or the one the previous edit brought back. A change by someone else still fails with a conflict.
 */
export function useUpdateItem(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    scope: itemEditsScope,
    mutationFn: ({ item, patch }: { item: Item; patch: ItemPatchRequest }) =>
      patchAtNewestVersion(queryClient, heap, item, patch),
    onSuccess: (saved) => queryClient.setQueryData(itemKey(heap, saved.key), saved),
    // Always re-render from the server, also after a conflict, so the row shows the latest version.
    onSettled: (_saved, _error, { patch }) => {
      if (patch.state !== undefined) invalidateOpenCounts(queryClient);
      return invalidateHeap(queryClient, heap);
    },
  });
}

/**
 * Moves several items to `state`, one after another in the order given, each as `useUpdateItem` saves one: in the item
 * edits' scope, at the newest version known. An item the server refuses, or a request that fails, is listed with its
 * error and the run goes on. Each saved item is written into the cache; the lists and open counts are fetched again
 * once, after the last.
 */
export function useChangeStates(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    scope: itemEditsScope,
    mutationFn: async ({ items, state }: { items: Item[]; state: ItemState }) => {
      const changed: Item[] = [];
      const refused: { item: Item; error: Error }[] = [];
      for (const item of items) {
        try {
          const saved = await patchAtNewestVersion(queryClient, heap, item, { state });
          queryClient.setQueryData(itemKey(heap, saved.key), saved);
          changed.push(saved);
        } catch (error) {
          refused.push({ item, error: error instanceof Error ? error : new Error(String(error)) });
        }
      }
      return { changed, refused };
    },
    onSettled: () => {
      invalidateOpenCounts(queryClient);
      return invalidateHeap(queryClient, heap);
    },
  });
}

/** Reads an item of `heap` as it is cached now, without fetching it. */
export function useCachedItem(heap: HeapRef) {
  const queryClient = useQueryClient();
  return (key: string) => queryClient.getQueryData<Item>(itemKey(heap, key));
}

/**
 * Unblocks an item whose blocker is gone, from its row in Waiting for me or in the item's Waits for you: moves it to
 * open at the version the row knows, or the newer one an edit of the item brought back, in the item edits' scope so it
 * waits for one on its way. The answer replaces the cached item and the lists' copies, and the row leaves at once. A
 * change by someone else in between fails with a conflict and fetches the item again.
 */
export function useUnblock(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    scope: itemEditsScope,
    mutationFn: (item: Pick<Item, "key" | "version">) =>
      patchAtNewestVersion(queryClient, heap, item, { state: "open" }),
    onSuccess: (saved) => {
      queryClient.setQueryData(itemKey(heap, saved.key), saved);
      replaceListedItem(queryClient, heap, saved);
      dropWaitingRows(queryClient, saved.id, "blocker_gone");
      dropWaitingRows(queryClient, saved.id, "blocker_left_team");
    },
    onError: (_error, item) => {
      void queryClient.invalidateQueries({ queryKey: itemKey(heap, item.key) });
    },
    onSettled: () => {
      invalidateOpenCounts(queryClient);
      return invalidateHeap(queryClient, heap);
    },
  });
}

export function useReopenItem(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (item: Item) =>
      unwrap(
        await api.POST("/api/v1/teams/{team}/heaps/{heap}/items/{item}/reopen", {
          params: { path: { ...heap, item: item.key }, header: { "If-Match": `"${item.version}"` } },
        }),
      ),
    onSettled: () => {
      invalidateOpenCounts(queryClient);
      return invalidateHeap(queryClient, heap);
    },
  });
}
