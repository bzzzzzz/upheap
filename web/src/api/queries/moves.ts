import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api, unwrap, type HeapRef } from "../client";
import type { Item, ItemToStepRequest } from "../schema";
import {
  rankedKey,
  closedKey,
  itemKey,
  waitingKey,
  decisionsKey,
  acrossHeapsKey,
  invalidateHeap,
  invalidateOpenCounts,
  itemEditsScope,
} from "./keys";

/**
 * Closes the note `key` as became, once items were made from it. Its answer, the closed note, is written into the
 * cache, and the lists and open counts catch up.
 */
export function useBecame(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (key: string) =>
      unwrap(
        await api.POST("/api/v1/teams/{team}/heaps/{heap}/items/{item}/became", {
          params: { path: { ...heap, item: key } },
        }),
      ),
    onSuccess: (saved) => queryClient.setQueryData(itemKey(heap, saved.key), saved),
    onSettled: () => {
      invalidateOpenCounts(queryClient);
      return invalidateHeap(queryClient, heap);
    },
  });
}

/**
 * Moves an item to another heap, `to`, of any team the person is in. It runs in the item edits' scope and sends the newest version it
 * knows of, as `useUpdateItem` does. The saved item is written under its new heap's key (it may have a new key there),
 * and both heaps and the open counts are fetched again, whatever the outcome. Nothing waits for those fetches, so the
 * app can go to the item's new address at once; after a move the old address isn't fetched again, as its pane is
 * still open until then and would say the item doesn't exist.
 */
export function useMoveItem(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    scope: itemEditsScope,
    mutationFn: async ({ item, to }: { item: Item; to: HeapRef & { id: string } }) => {
      const version = Math.max(item.version, queryClient.getQueryData<Item>(itemKey(heap, item.key))?.version ?? 0);
      return unwrap(
        await api.POST("/api/v1/teams/{team}/heaps/{heap}/items/{item}/move", {
          params: { path: { ...heap, item: item.key }, header: { "If-Match": `"${version}"` } },
          body: { heap_id: to.id },
        }),
      );
    },
    onSuccess: (saved, { to }) => queryClient.setQueryData(itemKey(to, saved.key), saved),
    onSettled: (saved, _error, { item, to }) => {
      invalidateOpenCounts(queryClient);
      const gone = itemKey(heap, item.key);
      const isGone = (key: readonly unknown[]) => saved !== undefined && gone.every((part, i) => key[i] === part);
      for (const queryKey of [rankedKey(heap), rankedKey(to), closedKey(heap), closedKey(to)]) {
        void queryClient.invalidateQueries({ queryKey });
      }
      void queryClient.invalidateQueries({ queryKey: acrossHeapsKey });
      void queryClient.invalidateQueries({ queryKey: ["item"], predicate: (query) => !isGone(query.queryKey) });
      void queryClient.invalidateQueries({ queryKey: waitingKey });
      void queryClient.invalidateQueries({ queryKey: decisionsKey });
    },
  });
}

/**
 * Moves several items of `heap` to another heap of its team, `to`, in one request. Each carries the newest
 * version known, as `useMoveItem` sends it, and moves on its own: the answer lists the items that moved and those that
 * didn't, with the problem for each. Nothing navigates, as the list stays where it is.
 */
export function useMoveItems(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    scope: itemEditsScope,
    mutationFn: async ({ items, to }: { items: Item[]; to: HeapRef & { id: string } }) =>
      unwrap(
        await api.POST("/api/v1/teams/{team}/items/move", {
          params: { path: { team: heap.team } },
          body: {
            heap_id: to.id,
            items: items.map((item) => ({
              id: item.id,
              version: Math.max(item.version, queryClient.getQueryData<Item>(itemKey(heap, item.key))?.version ?? 0),
            })),
          },
        }),
      ),
    onSettled: (_response, _error, { to }) => {
      invalidateOpenCounts(queryClient);
      for (const queryKey of [rankedKey(heap), rankedKey(to), closedKey(heap), closedKey(to)]) {
        void queryClient.invalidateQueries({ queryKey });
      }
      void queryClient.invalidateQueries({ queryKey: acrossHeapsKey });
      void queryClient.invalidateQueries({ queryKey: ["item"] });
      void queryClient.invalidateQueries({ queryKey: waitingKey });
      void queryClient.invalidateQueries({ queryKey: decisionsKey });
    },
  });
}

/**
 * Makes the note `key` a step of an open task of the same heap. The server closes the note as became; its answer is
 * written into the cache. Settling waits for the lists and items to be fetched again, so a caller reading the cache
 * after a refusal sees what the server has now.
 */
export function useNoteToStep(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ key, body }: { key: string; body: ItemToStepRequest }) =>
      unwrap(
        await api.POST("/api/v1/teams/{team}/heaps/{heap}/items/{item}/to-step", {
          params: { path: { ...heap, item: key } },
          body,
        }),
      ),
    onSuccess: (saved) => queryClient.setQueryData(itemKey(heap, saved.key), saved),
    onSettled: () => {
      invalidateOpenCounts(queryClient);
      return invalidateHeap(queryClient, heap);
    },
  });
}
