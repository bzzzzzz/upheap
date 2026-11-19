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

/** Where a tag lives: a heap's own, or a team's, which every heap of the team uses. */
export type TagScope = { kind: "heap"; heap: HeapRef } | { kind: "team"; team: string };

/** Creates a tag in the scope. Each form holds its own, so one's error and pending state are not another's. */
export function useCreateTagIn(scope: TagScope) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: CreateTagRequest) =>
      unwrap(
        scope.kind === "heap"
          ? await api.POST("/api/v1/teams/{team}/heaps/{heap}/tags", { params: { path: scope.heap }, body })
          : await api.POST("/api/v1/teams/{team}/tags", { params: { path: { team: scope.team } }, body }),
      ),
    onSettled: () =>
      scope.kind === "heap"
        ? queryClient.invalidateQueries({ queryKey: tagsKey(scope.heap) })
        : invalidateTeamTags(queryClient, scope.team, false),
  });
}

/** Edits a tag in the scope. */
export function useUpdateTagIn(scope: TagScope) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ tag, patch }: { tag: Tag; patch: TagPatchRequest }) => {
      const header = { "If-Match": `"${tag.version}"` };
      return unwrap(
        scope.kind === "heap"
          ? await api.PATCH("/api/v1/teams/{team}/heaps/{heap}/tags/{tag_id}", {
              params: { path: { ...scope.heap, tag_id: tag.id }, header },
              body: patch,
            })
          : await api.PATCH("/api/v1/teams/{team}/tags/{tag_id}", {
              params: { path: { team: scope.team, tag_id: tag.id }, header },
              body: patch,
            }),
      );
    },
    // Items show tag names, so a rename refreshes them too.
    onSettled: () =>
      scope.kind === "heap"
        ? Promise.all([
            queryClient.invalidateQueries({ queryKey: tagsKey(scope.heap) }),
            invalidateHeap(queryClient, scope.heap),
          ])
        : invalidateTeamTags(queryClient, scope.team, true),
  });
}

export function useTeamTags(team: string) {
  return useQuery({
    queryKey: teamTagsKey(team),
    queryFn: async () => unwrap(await api.GET("/api/v1/teams/{team}/tags", { params: { path: { team } } })),
    staleTime: 30_000,
  });
}
