// Server state lives in TanStack Query. Components read it through these hooks and never copy it.
import {
  keepPreviousData,
  queryOptions,
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from "@tanstack/react-query";
import { useRef } from "react";

import {
  api,
  ensureOk,
  unwrap,
  type Item,
  type ItemPatch,
  type ListViewQuery,
  type NewItem,
  type NewTag,
  type Tag,
  type TagPatch,
} from "./client";

export const meQuery = queryOptions({
  queryKey: ["me"],
  queryFn: async () => unwrap(await api.GET("/api/v1/me")),
  staleTime: 60_000,
});

export const useMe = () => useQuery(meQuery);

// Every list query of a heap sits under `["ranked", heapId]`, so one invalidation covers them all. The blocker
// candidates have a key of their own, so they never share a cache entry (and its `as_of`) with a list view.
const rankedKey = (heapId: string) => ["ranked", heapId] as const;
const rankedViewKey = (heapId: string, view: ListViewQuery) => ["ranked", heapId, "view", view] as const;
const blockerCandidatesKey = (heapId: string) => ["ranked", heapId, "blocker-candidates"] as const;
const closedKey = (heapId: string) => ["closed", heapId] as const;
const tagsKey = (heapId: string) => ["tags", heapId] as const;
const itemKey = (itemId: string) => ["item", itemId] as const;

/**
 * The ranked list in one view, kept stable within a session: the first response's `as_of` is sent back on every
 * refetch and view change, so only edited items move. `refresh` drops it and ranks at the current time. While a
 * new view loads, the previous one stays on screen.
 */
export function useRankedList(heapId: string, view: ListViewQuery) {
  const asOf = useRef<number | undefined>(undefined);
  const query = useQuery({
    queryKey: rankedViewKey(heapId, view),
    queryFn: async () => {
      const data = unwrap(
        await api.GET("/api/v1/heaps/{heap_id}/items", {
          params: { path: { heap_id: heapId }, query: { ...view, as_of: asOf.current } },
        }),
      );
      asOf.current = data.as_of;
      return data;
    },
    placeholderData: keepPreviousData,
    refetchInterval: 30_000,
  });
  const refresh = () => {
    asOf.current = undefined;
    return query.refetch();
  };
  return { ...query, refresh };
}

/** Every non-closed item, unfiltered: what an item can be blocked by, whatever the list view shows. */
const EVERY_NON_CLOSED_ITEM: ListViewQuery = { open: "shown", in_progress: "shown", blocked: "shown" };

/** The heap's non-closed items as blocker candidates. Fetched only while `enabled`. */
export function useBlockerCandidates(heapId: string, enabled = true) {
  return useQuery({
    queryKey: blockerCandidatesKey(heapId),
    queryFn: async () =>
      unwrap(
        await api.GET("/api/v1/heaps/{heap_id}/items", {
          params: { path: { heap_id: heapId }, query: EVERY_NON_CLOSED_ITEM },
        }),
      ),
    select: (data) => [...data.pinned, ...data.ranked, ...data.blocked],
    enabled,
  });
}

export function useClosedItems(heapId: string) {
  return useInfiniteQuery({
    queryKey: closedKey(heapId),
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam }) =>
      unwrap(
        await api.GET("/api/v1/heaps/{heap_id}/items/closed", {
          params: { path: { heap_id: heapId }, query: { cursor: pageParam } },
        }),
      ),
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  });
}

/**
 * One item, for the Edit item view. It's fetched when the view opens and after each save, but not in the background
 * (focus, reconnect or an interval), because a newer version resets the form and would drop what the user typed.
 */
export function useItem(itemId: string) {
  return useQuery({
    queryKey: itemKey(itemId),
    queryFn: async () => unwrap(await api.GET("/api/v1/items/{item_id}", { params: { path: { item_id: itemId } } })),
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
  });
}

function invalidateHeap(queryClient: QueryClient, heapId: string) {
  return Promise.all([
    // Every view of the list, by prefix.
    queryClient.invalidateQueries({ queryKey: rankedKey(heapId) }),
    queryClient.invalidateQueries({ queryKey: closedKey(heapId) }),
    // Any open item may show a changed blocker or tag, so every cached item is refetched.
    queryClient.invalidateQueries({ queryKey: ["item"] }),
  ]);
}

export function useCreateItem(heapId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: NewItem) =>
      unwrap(await api.POST("/api/v1/heaps/{heap_id}/items", { params: { path: { heap_id: heapId } }, body })),
    onSettled: () => invalidateHeap(queryClient, heapId),
  });
}

export function useUpdateItem(heapId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ item, patch }: { item: Item; patch: ItemPatch }) =>
      unwrap(
        await api.PATCH("/api/v1/items/{item_id}", {
          params: { path: { item_id: item.id }, header: { "If-Match": `"${item.version}"` } },
          body: patch,
        }),
      ),
    // Always re-render from the server, also after a conflict, so the row shows the latest version.
    onSettled: () => invalidateHeap(queryClient, heapId),
  });
}

export function useReopenItem(heapId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (item: Item) =>
      unwrap(
        await api.POST("/api/v1/items/{item_id}/reopen", {
          params: { path: { item_id: item.id }, header: { "If-Match": `"${item.version}"` } },
        }),
      ),
    onSettled: () => invalidateHeap(queryClient, heapId),
  });
}

export function useTags(heapId: string) {
  return useQuery({
    queryKey: tagsKey(heapId),
    queryFn: async () =>
      unwrap(await api.GET("/api/v1/heaps/{heap_id}/tags", { params: { path: { heap_id: heapId } } })),
    staleTime: 30_000,
  });
}

export function useCreateTag(heapId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: NewTag) =>
      unwrap(await api.POST("/api/v1/heaps/{heap_id}/tags", { params: { path: { heap_id: heapId } }, body })),
    onSettled: () => queryClient.invalidateQueries({ queryKey: tagsKey(heapId) }),
  });
}

export function useUpdateTag(heapId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ tag, patch }: { tag: Tag; patch: TagPatch }) =>
      unwrap(
        await api.PATCH("/api/v1/tags/{tag_id}", {
          params: { path: { tag_id: tag.id }, header: { "If-Match": `"${tag.version}"` } },
          body: patch,
        }),
      ),
    // Items show tag names, so a rename refreshes them too.
    onSettled: () =>
      Promise.all([queryClient.invalidateQueries({ queryKey: tagsKey(heapId) }), invalidateHeap(queryClient, heapId)]),
  });
}

export function useSignIn() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: { email: string; password: string }) =>
      unwrap(await api.POST("/api/v1/auth/login", { body })),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: meQuery.queryKey }),
  });
}

export function useRegister() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: { email: string; display_name: string; password: string }) =>
      unwrap(await api.POST("/api/v1/auth/register", { body })),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: meQuery.queryKey }),
  });
}

export function useSignOut() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async () => ensureOk(await api.POST("/api/v1/auth/logout")),
    onSuccess: () => queryClient.clear(),
  });
}
