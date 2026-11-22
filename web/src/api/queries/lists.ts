import { keepPreviousData, useInfiniteQuery, useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef } from "react";
import { api, unwrap, type HeapRef, type ListViewQuery } from "../client";
import type { RankedList } from "../schema";
import { apiKey, rankedViewKey, blockerCandidatesKey, closedKey, EVERY_NON_CLOSED_ITEM, POLL_INTERVAL } from "./keys";

/**
 * The ranked list in one view, kept stable within a session: the first response's `as_of` is sent back on every
 * refetch and view change, so only edited items move; reloading the page ranks at the current time. While a new
 * view loads, the previous one stays on screen.
 */
export function useRankedList(heap: HeapRef, view: ListViewQuery) {
  const asOf = useRef<number | undefined>(undefined);
  const query = useQuery({
    queryKey: rankedViewKey(heap, view),
    queryFn: async () => {
      const data = unwrap(
        await api.GET("/api/v1/teams/{team}/heaps/{heap}/items", {
          params: { path: heap, query: { ...view, as_of: asOf.current } },
        }),
      );
      asOf.current = data.as_of;
      return data;
    },
    placeholderData: keepPreviousData,
    refetchInterval: POLL_INTERVAL,
  });
  return query;
}

/** The heap's non-closed items as blocker candidates. Fetched only while `enabled`. */
export function useBlockerCandidates(heap: HeapRef, enabled = true) {
  return useQuery({
    queryKey: blockerCandidatesKey(heap),
    queryFn: async () =>
      unwrap(
        await api.GET("/api/v1/teams/{team}/heaps/{heap}/items", {
          params: { path: heap, query: EVERY_NON_CLOSED_ITEM },
        }),
      ),
    select: (data) => [...data.pinned, ...data.ranked, ...data.blocked],
    enabled,
  });
}

export function useClosedItems(heap: HeapRef) {
  return useInfiniteQuery({
    queryKey: closedKey(heap),
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam }) =>
      unwrap(
        await api.GET("/api/v1/teams/{team}/heaps/{heap}/items/closed", {
          params: { path: heap, query: { cursor: pageParam } },
        }),
      ),
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  });
}

/** The closed items of all the person's heaps, or of `team`'s, most recently closed first, a page at a time. */
export function useClosedAcrossHeaps(team: string | undefined) {
  const query = team === undefined ? {} : { team };
  return useInfiniteQuery({
    queryKey: apiKey("/api/v1/me/items/closed", { query }),
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam }) =>
      unwrap(await api.GET("/api/v1/me/items/closed", { params: { query: { ...query, cursor: pageParam } } })),
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  });
}

/**
 * Gives back a view of a heap's ranked list, the one on screen, as fetched from now on, so Add can say where an item
 * the server has just taken landed, from the list itself rather than from a second request. A fetch already on
 * its way, as after the add invalidated the heap, is waited for rather than started again; if it was cancelled
 * meanwhile (another change invalidated the heap), the list is fetched once more. If fetching fails, it's the list as
 * cached, which may not have the item yet.
 */
export function useRefetchRankedList() {
  const queryClient = useQueryClient();
  return async (heap: HeapRef, view: ListViewQuery): Promise<RankedList | undefined> => {
    const since = Date.now();
    const queryKey = rankedViewKey(heap, view);
    await queryClient.refetchQueries({ queryKey, exact: true }, { cancelRefetch: false });
    if ((queryClient.getQueryState(queryKey)?.dataUpdatedAt ?? 0) < since) {
      await queryClient.refetchQueries({ queryKey, exact: true });
    }
    return queryClient.getQueryData<RankedList>(queryKey);
  };
}
