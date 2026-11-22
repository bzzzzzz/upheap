import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, unwrap, type HeapRef } from "../client";
import type { CreateSavedViewRequest, SavedView, SavedViewPatchRequest } from "../schema";
import { savedViewsKey } from "./keys";

/** The heap's saved views in use, by name. */
export function useSavedViews(heap: HeapRef) {
  return useQuery({
    queryKey: savedViewsKey(heap),
    queryFn: async () => unwrap(await api.GET("/api/v1/teams/{team}/heaps/{heap}/views", { params: { path: heap } })),
    staleTime: 30_000,
  });
}

// Each mutation refetches the list when it settles, also after a failure, so what shows is what the server has. The
// refetch is returned so the mutation resolves once the list is current.

export function useCreateSavedView(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: CreateSavedViewRequest) =>
      unwrap(await api.POST("/api/v1/teams/{team}/heaps/{heap}/views", { params: { path: heap }, body })),
    onSettled: () => queryClient.invalidateQueries({ queryKey: savedViewsKey(heap) }),
  });
}

export function useUpdateSavedView(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ view, patch }: { view: SavedView; patch: SavedViewPatchRequest }) =>
      unwrap(
        await api.PATCH("/api/v1/teams/{team}/heaps/{heap}/views/{view_id}", {
          params: { path: { ...heap, view_id: view.id }, header: { "If-Match": `"${view.version}"` } },
          body: patch,
        }),
      ),
    onSettled: () => queryClient.invalidateQueries({ queryKey: savedViewsKey(heap) }),
  });
}

export function useArchiveSavedView(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (view: SavedView) =>
      unwrap(
        await api.POST("/api/v1/teams/{team}/heaps/{heap}/views/{view_id}/archive", {
          params: { path: { ...heap, view_id: view.id } },
        }),
      ),
    onSettled: () => queryClient.invalidateQueries({ queryKey: savedViewsKey(heap) }),
  });
}

export function useUnarchiveSavedView(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (view: SavedView) =>
      unwrap(
        await api.POST("/api/v1/teams/{team}/heaps/{heap}/views/{view_id}/unarchive", {
          params: { path: { ...heap, view_id: view.id } },
        }),
      ),
    onSettled: () => queryClient.invalidateQueries({ queryKey: savedViewsKey(heap) }),
  });
}
