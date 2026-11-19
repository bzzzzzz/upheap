import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api, ApiProblem, unwrap, type HeapRef } from "../client";
import type { Item, SendToAgentRequest } from "../schema";
import { rankedKey, itemKey, waitingKey, acrossHeapsKey, replaceListedItem, dropStuckRow } from "./keys";

/**
 * Sends the item to an agent. The answer is the whole item, with its turn, and replaces the cached one and the lists'
 * copies; sending needs no version, as a turn is its own row. A send refused because someone else sent the item first,
 * or blocked it, fetches it again.
 */
export function useSendToAgent(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ item, body }: { item: Item; body: SendToAgentRequest }) =>
      unwrap(
        await api.POST("/api/v1/teams/{team}/heaps/{heap}/items/{item}/turn", {
          params: { path: { ...heap, item: item.key } },
          body,
        }),
      ),
    onSuccess: (saved) => {
      queryClient.setQueryData(itemKey(heap, saved.key), saved);
      replaceListedItem(queryClient, heap, saved);
    },
    onError: (error, { item }) => {
      if (error instanceof ApiProblem && (error.code === "turn_open" || error.code === "turn_item_blocked")) {
        void queryClient.invalidateQueries({ queryKey: itemKey(heap, item.key) });
      }
    },
    // Not returned: the mutation is done once the server has the turn, and the lists catch up after. The lists say
    // which items an agent has, and a turn moves an item's allowed actions.
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: rankedKey(heap) });
      void queryClient.invalidateQueries({ queryKey: acrossHeapsKey });
      void queryClient.invalidateQueries({ queryKey: waitingKey });
    },
  });
}

/**
 * Takes the item back from the agent. Taking back what no agent has is not an error: the item comes back as it is. A
 * stuck item's row leaves Waiting for me at once; the refetch below confirms it, and the questions the agent asked.
 */
export function useTakeBack(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (item: Pick<Item, "key">) =>
      unwrap(
        await api.DELETE("/api/v1/teams/{team}/heaps/{heap}/items/{item}/turn", {
          params: { path: { ...heap, item: item.key } },
        }),
      ),
    onSuccess: (saved) => {
      queryClient.setQueryData(itemKey(heap, saved.key), saved);
      replaceListedItem(queryClient, heap, saved);
      dropStuckRow(queryClient, saved.id);
    },
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: rankedKey(heap) });
      void queryClient.invalidateQueries({ queryKey: acrossHeapsKey });
      void queryClient.invalidateQueries({ queryKey: waitingKey });
    },
  });
}

/**
 * Lets the agents try again at an item whose turn is stuck. The answer is the whole item, which replaces the cached one,
 * and its row leaves Waiting for me at once. A retry of a turn that is no longer stuck (someone took it back or retried
 * it first) fetches the item and the lists again.
 */
export function useRetryTurn(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (item: Pick<Item, "key">) =>
      unwrap(
        await api.POST("/api/v1/teams/{team}/heaps/{heap}/items/{item}/turn/retry", {
          params: { path: { ...heap, item: item.key } },
        }),
      ),
    onSuccess: (saved) => {
      queryClient.setQueryData(itemKey(heap, saved.key), saved);
      replaceListedItem(queryClient, heap, saved);
      dropStuckRow(queryClient, saved.id);
    },
    onError: (_error, item) => {
      void queryClient.invalidateQueries({ queryKey: itemKey(heap, item.key) });
    },
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: rankedKey(heap) });
      void queryClient.invalidateQueries({ queryKey: acrossHeapsKey });
      void queryClient.invalidateQueries({ queryKey: waitingKey });
    },
  });
}

/**
 * Sends back the work an agent brought to a stable state short of done, with a line saying what is wrong. The answer is
 * the whole item, with its new turn, which replaces the cached one and the lists' copies. Nothing left to send back
 * (someone else sent the item on first) fetches the item again.
 */
export function useRedoTurn(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ item, line }: { item: Pick<Item, "key">; line: string }) =>
      unwrap(
        await api.POST("/api/v1/teams/{team}/heaps/{heap}/items/{item}/turn/redo", {
          params: { path: { ...heap, item: item.key } },
          body: { line },
        }),
      ),
    onSuccess: (saved) => {
      queryClient.setQueryData(itemKey(heap, saved.key), saved);
      replaceListedItem(queryClient, heap, saved);
    },
    // Returned, so the caller's error handler runs once the item has been fetched again.
    onError: (error, { item }) => {
      if (error instanceof ApiProblem && error.code === "nothing_to_redo") {
        return queryClient.invalidateQueries({ queryKey: itemKey(heap, item.key) });
      }
    },
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: rankedKey(heap) });
      void queryClient.invalidateQueries({ queryKey: acrossHeapsKey });
      void queryClient.invalidateQueries({ queryKey: waitingKey });
    },
  });
}
