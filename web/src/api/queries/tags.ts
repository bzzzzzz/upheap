import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, unwrap, type HeapRef } from "../client";
import type { CreateTagRequest, Tag, TagList, TagPatchRequest } from "../schema";
import { tagsKey, teamTagsKey, invalidateHeap, invalidateTeamTags } from "./keys";

/** The heap's tags and its team's together, by name. */
export function heapTags(list: TagList): Tag[] {
  return [...list.tags, ...list.team_tags].sort((a, b) =>
    a.name.localeCompare(b.name, undefined, { sensitivity: "base" }),
  );
}

export function useTags(heap: HeapRef, enabled = true) {
  return useQuery({
    queryKey: tagsKey(heap),
    queryFn: async () => unwrap(await api.GET("/api/v1/teams/{team}/heaps/{heap}/tags", { params: { path: heap } })),
    staleTime: 30_000,
    enabled,
  });
}

export function useCreateTag(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: CreateTagRequest) =>
      unwrap(await api.POST("/api/v1/teams/{team}/heaps/{heap}/tags", { params: { path: heap }, body })),
    onSettled: () => queryClient.invalidateQueries({ queryKey: tagsKey(heap) }),
  });
}

export function useUpdateTag(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ tag, patch }: { tag: Tag; patch: TagPatchRequest }) =>
      unwrap(
        await api.PATCH("/api/v1/teams/{team}/heaps/{heap}/tags/{tag_id}", {
          params: { path: { ...heap, tag_id: tag.id }, header: { "If-Match": `"${tag.version}"` } },
          body: patch,
        }),
      ),
    // Items show tag names, so a rename refreshes them too.
    onSettled: () =>
      Promise.all([queryClient.invalidateQueries({ queryKey: tagsKey(heap) }), invalidateHeap(queryClient, heap)]),
  });
}

export function useTeamTags(team: string) {
  return useQuery({
    queryKey: teamTagsKey(team),
    queryFn: async () => unwrap(await api.GET("/api/v1/teams/{team}/tags", { params: { path: { team } } })),
    staleTime: 30_000,
  });
}

export function useCreateTeamTag(team: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: CreateTagRequest) =>
      unwrap(await api.POST("/api/v1/teams/{team}/tags", { params: { path: { team } }, body })),
    onSettled: () => invalidateTeamTags(queryClient, team, false),
  });
}

export function useUpdateTeamTag(team: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ tag, patch }: { tag: Tag; patch: TagPatchRequest }) =>
      unwrap(
        await api.PATCH("/api/v1/teams/{team}/tags/{tag_id}", {
          params: { path: { team, tag_id: tag.id }, header: { "If-Match": `"${tag.version}"` } },
          body: patch,
        }),
      ),
    onSettled: () => invalidateTeamTags(queryClient, team, true),
  });
}
