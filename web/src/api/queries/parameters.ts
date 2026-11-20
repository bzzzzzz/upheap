import { useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { api, unwrap, type HeapRef } from "../client";
import type { CreateParameterRequest, Parameter, ParameterOption, ParameterPatchRequest } from "../schema";
import { parametersKey, teamParametersKey, invalidateHeap, invalidateTeamParameters } from "./keys";

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

/** The team's own parameters, in use and archived, each by name. */
export function useTeamParameters(team: string) {
  return useQuery({
    queryKey: teamParametersKey(team),
    queryFn: async () => unwrap(await api.GET("/api/v1/teams/{team}/parameters", { params: { path: { team } } })),
    staleTime: 30_000,
  });
}

/** Where a parameter lives: a heap's own, or a team's, which each of the team's heaps may use. */
export type ParameterScope = { kind: "heap"; heap: HeapRef } | { kind: "team"; team: string };

/** What a change needs of a parameter: which one, and the version it was seen at. */
type Versioned = Pick<Parameter, "id" | "version">;

/** Parameter changes move items (ranking) and change what they show (labels, switched values). */
/** Refetches the scope's parameters and, unless only they changed (`itemsToo` false), the items that show them. */
function invalidateParameters(queryClient: QueryClient, scope: ParameterScope, itemsToo = true) {
  if (scope.kind === "team") return invalidateTeamParameters(queryClient, scope.team, itemsToo);
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: parametersKey(scope.heap) }),
    itemsToo && invalidateHeap(queryClient, scope.heap),
  ]);
}

export function useCreateParameter(scope: ParameterScope) {
  const queryClient = useQueryClient();
  return useMutation({
    // A team's parameter has no weight, which the form leaves out.
    mutationFn: async (body: CreateParameterRequest) =>
      scope.kind === "heap"
        ? unwrap(await api.POST("/api/v1/teams/{team}/heaps/{heap}/parameters", { params: { path: scope.heap }, body }))
        : unwrap(
            await api.POST("/api/v1/teams/{team}/parameters", {
              params: { path: { team: scope.team } },
              body: { name: body.name, options: body.options },
            }),
          ),
    onSettled: () => invalidateParameters(queryClient, scope, false),
  });
}

/** Every change to one parameter that takes its version: name and weight, options, tags. A team's has no weight or tags. */
export type ParameterChange =
  | { kind: "patch"; patch: ParameterPatchRequest }
  | { kind: "add_option"; label: string }
  | { kind: "arrange_options"; options: ParameterOption[] }
  | { kind: "link_tags"; tagIds: string[] };

export function useChangeParameter(scope: ParameterScope) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ parameter, change }: { parameter: Versioned; change: ParameterChange }) => {
      const header = { "If-Match": `"${parameter.version}"` };
      if (scope.kind === "team") {
        const params = { params: { path: { team: scope.team, parameter_id: parameter.id }, header } };
        switch (change.kind) {
          case "patch":
            return unwrap(
              await api.PATCH("/api/v1/teams/{team}/parameters/{parameter_id}", {
                ...params,
                body: { name: change.patch.name ?? "" },
              }),
            );
          case "add_option":
            return unwrap(
              await api.POST("/api/v1/teams/{team}/parameters/{parameter_id}/options", {
                ...params,
                body: { label: change.label },
              }),
            );
          case "arrange_options":
            return unwrap(
              await api.PATCH("/api/v1/teams/{team}/parameters/{parameter_id}/options", {
                ...params,
                body: { options: change.options },
              }),
            );
          case "link_tags":
            throw new Error("A team parameter has no tag links.");
        }
      }
      const params = { params: { path: { ...scope.heap, parameter_id: parameter.id }, header } };
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
    onSettled: (_data, _error, { change }) => invalidateParameters(queryClient, scope, change.kind !== "link_tags"),
  });
}

export function useArchiveParameter(scope: ParameterScope) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (parameter: Versioned) =>
      scope.kind === "heap"
        ? unwrap(
            await api.POST("/api/v1/teams/{team}/heaps/{heap}/parameters/{parameter_id}/archive", {
              params: { path: { ...scope.heap, parameter_id: parameter.id } },
            }),
          )
        : unwrap(
            await api.POST("/api/v1/teams/{team}/parameters/{parameter_id}/archive", {
              params: { path: { team: scope.team, parameter_id: parameter.id } },
            }),
          ),
    onSettled: () => invalidateParameters(queryClient, scope),
  });
}

/** Archives an option; open items that have it switch to `replacementId`. */
export function useArchiveOption(scope: ParameterScope) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ optionId, replacementId }: { optionId: string; replacementId: string }) =>
      scope.kind === "heap"
        ? unwrap(
            await api.POST("/api/v1/teams/{team}/heaps/{heap}/options/{option_id}/archive", {
              params: { path: { ...scope.heap, option_id: optionId } },
              body: { replacement_option_id: replacementId },
            }),
          )
        : unwrap(
            await api.POST("/api/v1/teams/{team}/options/{option_id}/archive", {
              params: { path: { team: scope.team, option_id: optionId } },
              body: { replacement_option_id: replacementId },
            }),
          ),
    onSettled: () => invalidateParameters(queryClient, scope),
  });
}

/** Makes the heap use one of its team's parameters, or stop (`use` false). The answer is the heap's new list. */
export function useSetUsingTeamParameter(heap: HeapRef, use: boolean) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (parameterId: string) => {
      const params = { params: { path: { ...heap, parameter_id: parameterId } } };
      return unwrap(
        use
          ? await api.POST("/api/v1/teams/{team}/heaps/{heap}/parameters/{parameter_id}/use", params)
          : await api.POST("/api/v1/teams/{team}/heaps/{heap}/parameters/{parameter_id}/stop-using", params),
      );
    },
    // The list comes back in the answer. The team's lists of who uses what, and the team's items, are read again.
    onSuccess: (list) => queryClient.setQueryData(parametersKey(heap), list),
    onSettled: () => invalidateTeamParameters(queryClient, heap.team),
  });
}
