import { useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { api, unwrap, type HeapRef } from "../client";
import type { CreateParameterRequest, Parameter, ParameterOption, ParameterPatchRequest } from "../schema";
import { parametersKey, invalidateHeap } from "./keys";

/** The heap's parameters: those in use and the archived ones, each by name. */
export function useParameters(heap: HeapRef, enabled = true) {
  return useQuery({
    queryKey: parametersKey(heap),
    queryFn: async () =>
      unwrap(await api.GET("/api/v1/teams/{team}/heaps/{heap}/parameters", { params: { path: heap } })),
    staleTime: 30_000,
    enabled,
  });
}

/** Parameter changes move items (ranking) and change what they show (labels, switched values). */
/** Refetches the heap's parameters and, unless only they changed (`itemsToo` false), the items that show them. */
function invalidateParameters(queryClient: QueryClient, heap: HeapRef, itemsToo = true) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: parametersKey(heap) }),
    itemsToo && invalidateHeap(queryClient, heap),
  ]);
}

export function useCreateParameter(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: CreateParameterRequest) =>
      unwrap(await api.POST("/api/v1/teams/{team}/heaps/{heap}/parameters", { params: { path: heap }, body })),
    onSettled: () => invalidateParameters(queryClient, heap),
  });
}

/** Every change to one parameter that takes its version: name and weight, options, tags. */
export type ParameterChange =
  | { kind: "patch"; patch: ParameterPatchRequest }
  | { kind: "add_option"; label: string }
  | { kind: "arrange_options"; options: ParameterOption[] }
  | { kind: "link_tags"; tagIds: string[] };

export function useChangeParameter(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ parameter, change }: { parameter: Parameter; change: ParameterChange }) => {
      const params = {
        params: { path: { ...heap, parameter_id: parameter.id }, header: { "If-Match": `"${parameter.version}"` } },
      };
      switch (change.kind) {
        case "patch":
          return unwrap(
            await api.PATCH("/api/v1/teams/{team}/heaps/{heap}/parameters/{parameter_id}", {
              ...params,
              body: change.patch,
            }),
          );
        case "add_option":
          return unwrap(
            await api.POST("/api/v1/teams/{team}/heaps/{heap}/parameters/{parameter_id}/options", {
              ...params,
              body: { label: change.label },
            }),
          );
        case "arrange_options":
          return unwrap(
            await api.PATCH("/api/v1/teams/{team}/heaps/{heap}/parameters/{parameter_id}/options", {
              ...params,
              body: { options: change.options },
            }),
          );
        case "link_tags":
          return unwrap(
            await api.PUT("/api/v1/teams/{team}/heaps/{heap}/parameters/{parameter_id}/tags", {
              ...params,
              body: { tag_ids: change.tagIds },
            }),
          );
      }
    },
    // Also after a conflict, so the page shows the version someone else saved. Tag links don't show on items.
    onSettled: (_data, _error, { change }) => invalidateParameters(queryClient, heap, change.kind !== "link_tags"),
  });
}

export function useArchiveParameter(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (parameter: Parameter) =>
      unwrap(
        await api.POST("/api/v1/teams/{team}/heaps/{heap}/parameters/{parameter_id}/archive", {
          params: { path: { ...heap, parameter_id: parameter.id } },
        }),
      ),
    onSettled: () => invalidateParameters(queryClient, heap),
  });
}

/** Archives an option; open items that have it switch to `replacementId`. */
export function useArchiveOption(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ optionId, replacementId }: { optionId: string; replacementId: string }) =>
      unwrap(
        await api.POST("/api/v1/teams/{team}/heaps/{heap}/options/{option_id}/archive", {
          params: { path: { ...heap, option_id: optionId } },
          body: { replacement_option_id: replacementId },
        }),
      ),
    onSettled: () => invalidateParameters(queryClient, heap),
  });
}
