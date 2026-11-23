import { keepPreviousData, useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { api, ensureOk, unwrap, type HeapRef } from "../client";
import type { CreateHeapRequest, Heap, HeapWeightsPatchRequest, ParameterList, RankingPreviewRequest } from "../schema";
import {
  apiKey,
  tagsKey,
  parametersKey,
  heapKey,
  heapRecordKeys,
  heapPath,
  meKey,
  waitingKey,
  invalidateHeap,
  invalidateTeam,
} from "./keys";

export function useHeap(heap: HeapRef) {
  return useQuery({
    queryKey: heapKey(heap),
    queryFn: async () => unwrap(await api.GET("/api/v1/teams/{team}/heaps/{heap}", { params: { path: heap } })),
    staleTime: 60_000,
  });
}

export function useCreateHeap(team: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: CreateHeapRequest) =>
      unwrap(await api.POST("/api/v1/teams/{team}/heaps", { params: { path: { team } }, body })),
    onSettled: () => invalidateTeam(queryClient, team),
  });
}

/** Everything that shows a heap's name or archive state. Archiving also changes every item's allowed actions. */
function invalidateHeapRecord(queryClient: QueryClient, heap: HeapRef) {
  return Promise.all([
    ...heapRecordKeys(heap).map((queryKey) => queryClient.invalidateQueries({ queryKey })),
    invalidateTeam(queryClient, heap.team),
    invalidateHeap(queryClient, heap),
    queryClient.invalidateQueries({ queryKey: tagsKey(heap) }),
    queryClient.invalidateQueries({ queryKey: parametersKey(heap) }),
  ]);
}

/**
 * Renames a heap of the team `team`, gives it a new slug or writes its context. A new slug is a new path: when the app
 * is on one of the heap's pages, it moves to the same page at the new path, with the same search.
 */
export function useUpdateHeap(team: string) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  return useMutation({
    mutationFn: async ({
      heap,
      patch,
    }: {
      heap: Heap;
      patch: { name?: string; slug?: string; context?: string | null; agents_prepare?: boolean };
    }) =>
      unwrap(
        await api.PATCH("/api/v1/teams/{team}/heaps/{heap}", {
          params: { path: { team, heap: heap.slug }, header: { "If-Match": `"${heap.version}"` } },
          body: patch,
        }),
      ),
    onSuccess: async (saved, { heap }) => {
      // Cached under its new slug too, so the page at the new path shows it at once rather than loading it.
      queryClient.setQueryData(heapKey({ team, heap: saved.slug }), saved);
      if (saved.slug === heap.slug) return;
      const prefix = `/teams/${team}/heaps/${heap.slug}`;
      const path = window.location.pathname;
      const at = path.indexOf(prefix);
      const end = at + prefix.length;
      if (at >= 0 && (path.length === end || path[end] === "/")) {
        // `search: true` keeps the list's filters and sort, which are in the query string.
        await navigate({ to: `/teams/${team}/heaps/${saved.slug}${path.slice(end)}`, search: true, replace: true });
      }
    },
    onSettled: (saved, _error, { heap }) =>
      Promise.all([
        invalidateHeapRecord(queryClient, { team, heap: heap.slug }),
        saved && invalidateHeapRecord(queryClient, { team, heap: saved.slug }),
      ]),
  });
}

/**
 * Sets the caller's own priority for a heap of the team `team`, 1 to 10. Declarative, so no `If-Match`. Waiting for me
 * is ordered by it, so it is fetched again.
 */
export function useSetHeapPriority(team: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ heap, priority }: { heap: Heap; priority: number }) =>
      unwrap(
        await api.PUT("/api/v1/teams/{team}/heaps/{heap}/priority", {
          params: { path: { team, heap: heap.slug } },
          body: { priority },
        }),
      ),
    onSuccess: (saved) => queryClient.setQueryData(heapKey({ team, heap: saved.slug }), saved),
    onSettled: (_saved, _error, { heap }) =>
      Promise.all([
        ...heapRecordKeys({ team, heap: heap.slug }).map((queryKey) => queryClient.invalidateQueries({ queryKey })),
        // The team's heap list carries the priority too.
        invalidateTeam(queryClient, team),
        queryClient.invalidateQueries({ queryKey: waitingKey }),
      ]),
  });
}

/**
 * Makes a heap of the team `team` the caller's default heap, or clears it. Declarative, so no `If-Match`. The default
 * is on `me`, which the card and `/` read, so it is fetched again.
 */
export function useDefaultHeap(team: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ heap, makeDefault }: { heap: Heap; makeDefault: boolean }) => {
      const params = { params: { path: { team, heap: heap.slug } } };
      ensureOk(
        makeDefault
          ? await api.PUT("/api/v1/teams/{team}/heaps/{heap}/default", params)
          : await api.DELETE("/api/v1/teams/{team}/heaps/{heap}/default", params),
      );
    },
    onSettled: () => queryClient.invalidateQueries({ queryKey: meKey }),
  });
}

/** Archives or unarchives a heap of the team `team`. */
export function useSetHeapArchived(team: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ heap, archived }: { heap: Heap; archived: boolean }) => {
      const params = { params: { path: { team, heap: heap.slug } } };
      return unwrap(
        archived
          ? await api.POST("/api/v1/teams/{team}/heaps/{heap}/archive", params)
          : await api.POST("/api/v1/teams/{team}/heaps/{heap}/unarchive", params),
      );
    },
    onSettled: (_data, _error, { heap }) => invalidateHeapRecord(queryClient, { team, heap: heap.slug }),
  });
}

/**
 * Saves the heap's weights and its parameters' weights in one request, sent with the heap's version, which orders
 * weights saves; each parameter's version is checked too. The answer holds the heap and its parameters in use, which replace the cached ones, so the next save
 * has the new versions even before the refetch.
 */
export function useUpdateWeights(heapRef: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ heap, body }: { heap: Heap; body: HeapWeightsPatchRequest }) =>
      unwrap(
        await api.PATCH("/api/v1/teams/{team}/heaps/{heap}/weights", {
          params: { path: heapRef, header: { "If-Match": `"${heap.version}"` } },
          body,
        }),
      ),
    onSuccess: (saved) => {
      queryClient.setQueryData(heapKey(heapRef), saved.heap);
      queryClient.setQueryData<ParameterList>(
        parametersKey(heapRef),
        (list) => list && { ...list, parameters: saved.parameters },
      );
    },
    // Also after a conflict, so the page shows the weights and versions someone else saved. The list, the items'
    // contributors and the preview all move with the weights.
    onSettled: () => invalidateHeapRecord(queryClient, heapRef),
  });
}

/** How many items the Ranking tab's preview lists; a move from below them reads "New". */
export const PREVIEW_ITEMS = 5;

/** The weights a preview ranks with in place of the saved ones. */
export type RankingPreviewDraft = Omit<RankingPreviewRequest, "limit">;

/**
 * The top of the heap's list with the draft weights, none saved. Only the weights that differ from the saved ones go
 * in `draft`, so no draft is the current top of the list. While a new draft loads, the last result stays. `heapRecordKeys`
 * reaches it, so saving or refetching the heap refreshes it.
 */
export function useRankingPreview(heapRef: HeapRef, draft: RankingPreviewDraft) {
  return useQuery({
    queryKey: apiKey("/api/v1/teams/{team}/heaps/{heap}/ranking/preview", { path: heapPath(heapRef), body: draft }),
    queryFn: async () =>
      unwrap(
        await api.POST("/api/v1/teams/{team}/heaps/{heap}/ranking/preview", {
          params: { path: heapRef },
          body: { ...draft, limit: PREVIEW_ITEMS },
        }),
      ),
    placeholderData: keepPreviousData,
  });
}
