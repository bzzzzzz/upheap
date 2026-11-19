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

/**
 * Creates a tag in the scope. Each form holds its own, so one's error and pending state are not another's. `confirm`
 * is the token of the reach a team tag's merge was refused with.
 */
export function useCreateTagIn(scope: TagScope) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ body, confirm }: { body: CreateTagRequest; confirm?: string }) =>
      unwrap(
        scope.kind === "heap"
          ? await api.POST("/api/v1/teams/{team}/heaps/{heap}/tags", { params: { path: scope.heap }, body })
          : await api.POST("/api/v1/teams/{team}/tags", {
              params: { path: { team: scope.team }, query: { ...(confirm && { confirm_reach: confirm }) } },
              body,
            }),
      ),
    // A confirmed merge changes items' tags in many heaps.
    onSettled: (_data, _error, { confirm }) =>
      scope.kind === "heap"
        ? queryClient.invalidateQueries({ queryKey: tagsKey(scope.heap) })
        : invalidateTeamTags(queryClient, scope.team, Boolean(confirm)),
  });
}

/** Edits a tag in the scope. `confirm` is the token of the reach a team tag's merge was refused with. */
export function useUpdateTagIn(scope: TagScope) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ tag, patch, confirm }: { tag: Tag; patch: TagPatchRequest; confirm?: string }) => {
      const header = { "If-Match": `"${tag.version}"` };
      return unwrap(
        scope.kind === "heap"
          ? await api.PATCH("/api/v1/teams/{team}/heaps/{heap}/tags/{tag_id}", {
              params: { path: { ...scope.heap, tag_id: tag.id }, header },
              body: patch,
            })
          : await api.PATCH("/api/v1/teams/{team}/tags/{tag_id}", {
              params: {
                path: { team: scope.team, tag_id: tag.id },
                query: { ...(confirm && { confirm_reach: confirm }) },
                header,
              },
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
