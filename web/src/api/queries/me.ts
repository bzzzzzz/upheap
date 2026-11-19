import {
  keepPreviousData,
  queryOptions,
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { useRef } from "react";
import { api, unwrap, type AcrossHeapsQuery } from "../client";
import type { User } from "../schema";
import { acrossHeapsKey, waitingKey, decisionsKey, resultsKey, POLL_INTERVAL, invalidateMe, meKey } from "./keys";

export const meQuery = queryOptions({
  queryKey: meKey,
  queryFn: async () => unwrap(await api.GET("/api/v1/me")),
  staleTime: 60_000,
});

export const useMe = () => useQuery(meQuery);

/**
 * The list across the person's heaps in one view, kept stable within a session like `useRankedList`'s: the first
 * response's `as_of` is sent back on every refetch and view change. While a new view loads, the previous one stays on
 * screen.
 */
export function useAcrossHeapsList(view: AcrossHeapsQuery) {
  const asOf = useRef<number | undefined>(undefined);
  return useQuery({
    queryKey: [...acrossHeapsKey, view] as const,
    queryFn: async () => {
      const data = unwrap(await api.GET("/api/v1/me/items", { params: { query: { ...view, as_of: asOf.current } } }));
      asOf.current = data.as_of;
      return data;
    },
    placeholderData: keepPreviousData,
    refetchInterval: POLL_INTERVAL,
  });
}

/**
 * What waits for the person across their heaps (ux.md, Waiting for me): its count is in the navigation, so it's fetched
 * for every page, once signed in. Fetched again every 30 s like the list, so a question an agent or a teammate asks
 * reaches the count with no write of the person's own, and also on window focus and after writes.
 */
export function useWaiting() {
  return useQuery({
    queryKey: waitingKey,
    queryFn: async () => unwrap(await api.GET("/api/v1/me/waiting")),
    staleTime: POLL_INTERVAL,
    refetchInterval: POLL_INTERVAL,
  });
}

/**
 * What agents finished that waits for the person across their heaps (ux.md, Agents): its count is in the navigation,
 * so it's fetched for every page, once signed in, and again every 30 s like Waiting for me's.
 */
export function useDecisions() {
  return useQuery({
    queryKey: decisionsKey,
    queryFn: async () => unwrap(await api.GET("/api/v1/me/decisions")),
    staleTime: POLL_INTERVAL,
    refetchInterval: POLL_INTERVAL,
  });
}

/** The history of runs that ended as reached in the last 30 days, a page at a time, newest first (ux.md, Agents). */
export function useResults() {
  return useInfiniteQuery({
    queryKey: resultsKey,
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam }) =>
      unwrap(await api.GET("/api/v1/me/results", { params: { query: { cursor: pageParam } } })),
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    staleTime: POLL_INTERVAL,
    refetchInterval: POLL_INTERVAL,
  });
}

export function useUpdateMe() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ user, display_name }: { user: User; display_name: string }) =>
      unwrap(
        await api.PATCH("/api/v1/me", {
          params: { header: { "If-Match": `"${user.version}"` } },
          body: { display_name },
        }),
      ),
    // Teams list members by name.
    onSettled: () => Promise.all([invalidateMe(queryClient), queryClient.invalidateQueries({ queryKey: ["team"] })]),
  });
}
