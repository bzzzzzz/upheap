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
import { apiKey, waitingKey, decisionsKey, POLL_INTERVAL, invalidateMe, meKey, teamKeys } from "./keys";

export const meQuery = queryOptions({
  queryKey: meKey,
  queryFn: async () => unwrap(await api.GET("/api/v1/me")),
  staleTime: 60_000,
});

export const useMe = () => useQuery(meQuery);

/**
 * `/me` for the app's shell, which shows its heaps' open counts on every page (ux.md, Layout): fetched again every
 * 30 s like the list and the Waiting count, so an item a teammate or an agent adds or closes reaches the counts with no
 * write of the person's own. Only the shell reads it this way: every observer of a query runs its own interval, so
 * polling in `useMe` would fetch `/me` once per component that reads it.
 */
export const usePolledMe = () => useQuery({ ...meQuery, staleTime: POLL_INTERVAL, refetchInterval: POLL_INTERVAL });

/**
 * The list across the person's heaps in one view, kept stable within a session like `useRankedList`'s: the first
 * response's `as_of` is sent back on every refetch and view change. While a new view loads, the previous one stays on
 * screen.
 */
export function useAcrossHeapsList(view: AcrossHeapsQuery) {
  const asOf = useRef<number | undefined>(undefined);
  return useQuery({
    queryKey: apiKey("/api/v1/me/items", { query: view }),
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
 * What agents finished that waits for the person across their heaps (ux.md, Agents), for the Agents page: fetched
 * again every 30 s like Waiting for me, so what an agent finishes reaches the page left open.
 */
export function useDecisions() {
  return useQuery({
    queryKey: decisionsKey,
    queryFn: async () => unwrap(await api.GET("/api/v1/me/decisions")),
    staleTime: POLL_INTERVAL,
    refetchInterval: POLL_INTERVAL,
  });
}

const resultsKey = apiKey("/api/v1/me/results");

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
    onSettled: () =>
      Promise.all([
        invalidateMe(queryClient),
        ...teamKeys().map((queryKey) => queryClient.invalidateQueries({ queryKey })),
      ]),
  });
}

/**
 * The items whose title or key matches `text`, in every heap the person can view (ux.md, Keyboard). Callers send the
 * trimmed text once it has settled, and nothing for a blank one. The previous answer stays while the next one loads.
 */
export function useSearch(text: string) {
  return useQuery({
    queryKey: apiKey("/api/v1/me/search", { query: { q: text } }),
    queryFn: async () => unwrap(await api.GET("/api/v1/me/search", { params: { query: { q: text } } })),
    enabled: text !== "",
    placeholderData: keepPreviousData,
    staleTime: 30_000,
  });
}
