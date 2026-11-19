import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api, unwrap, type HeapRef } from "../client";
import type { AddCommentRequest } from "../schema";
import { itemKey } from "./keys";

/**
 * Adds a comment to the item `key`. Not optimistic: the item the server returns, with the new comment, is written
 * into the cache. A comment needs no version and changes none, and no list row shows one, so only the item's own
 * caches are touched; a refusal fetches the item again, in case its actions changed (its heap was archived).
 */
export function useAddComment(heap: HeapRef, key: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: AddCommentRequest) =>
      unwrap(
        await api.POST("/api/v1/teams/{team}/heaps/{heap}/items/{item}/comments", {
          params: { path: { ...heap, item: key } },
          body,
        }),
      ),
    onSuccess: (saved) => queryClient.setQueryData(itemKey(heap, saved.key), saved),
    onError: () => {
      void queryClient.invalidateQueries({ queryKey: itemKey(heap, key) });
    },
  });
}
