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
import { useNavigate } from "@tanstack/react-router";
import { useRef } from "react";

import {
  api,
  ensureOk,
  unwrap,
  type Heap,
  type Item,
  type ItemPatch,
  type ListViewInput,
  type ListViewQuery,
  type Me,
  type NewItem,
  type NewParameter,
  type NewTag,
  type Parameter,
  type ParameterOption,
  type ParameterPatch,
  type Role,
  type SavedView,
  type Tag,
  type TagPatch,
  type Team,
  type User,
} from "./client";

export const meQuery = queryOptions({
  queryKey: ["me"],
  queryFn: async () => unwrap(await api.GET("/api/v1/me")),
  staleTime: 60_000,
});

export const useMe = () => useQuery(meQuery);

/**
 * The caller's one team when there's no Teams list to show: they can't start another, as in the local build. Links
 * then go straight to its page.
 */
export function soleTeam(me: Me): Me["teams"][number] | undefined {
  return !me.allowed_actions.includes("create_team") && me.teams.length === 1 ? me.teams[0] : undefined;
}

// Every list query of a heap sits under `["ranked", heapId]`, so one invalidation covers them all. The blocker
// candidates have a key of their own, so they never share a cache entry (and its `as_of`) with a list view.
const rankedKey = (heapId: string) => ["ranked", heapId] as const;
const rankedViewKey = (heapId: string, view: ListViewQuery) => ["ranked", heapId, "view", view] as const;
const blockerCandidatesKey = (heapId: string) => ["ranked", heapId, "blocker-candidates"] as const;
const closedKey = (heapId: string) => ["closed", heapId] as const;
const tagsKey = (heapId: string) => ["tags", heapId] as const;
const savedViewsKey = (heapId: string) => ["saved-views", heapId] as const;
const parametersKey = (heapId: string) => ["parameters", heapId] as const;
const itemKey = (itemId: string) => ["item", itemId] as const;
const teamKey = (teamId: string) => ["team", teamId] as const;
const heapKey = (heapId: string) => ["heap", heapId] as const;
const usersKey = (query: string) => ["users", query] as const;

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

/** Whose history to read: a heap's or a team's, or one entity's within it. */
export type AuditScope = { heapId: string } | { teamId: string };

/**
 * A page-by-page history, newest first. Every write is audited, so `main.tsx` invalidates these after any mutation.
 */
export function useAuditLog(scope: AuditScope, entityId?: string) {
  const owner = "heapId" in scope ? ["heap", scope.heapId] : ["team", scope.teamId];
  return useInfiniteQuery({
    queryKey: ["audit", ...owner, entityId ?? null],
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam }) => {
      const query = { entity_id: entityId, cursor: pageParam };
      const result =
        "heapId" in scope
          ? await api.GET("/api/v1/heaps/{heap_id}/audit", { params: { path: { heap_id: scope.heapId }, query } })
          : await api.GET("/api/v1/teams/{team_id}/audit", { params: { path: { team_id: scope.teamId }, query } });
      return unwrap(result);
    },
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

/**
 * The rough due times a due picker offers, from the server, so a picked choice reads back with the label it was
 * picked by. Their dates move with the day, so they're fetched again after a while.
 */
export function useDueChoices() {
  return useQuery({
    queryKey: ["due-choices"],
    queryFn: async () => unwrap(await api.GET("/api/v1/due-choices")).choices,
    staleTime: 10 * 60_000,
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

/** The heap's parameters: those in use and the archived ones, each by name. */
export function useParameters(heapId: string) {
  return useQuery({
    queryKey: parametersKey(heapId),
    queryFn: async () =>
      unwrap(await api.GET("/api/v1/heaps/{heap_id}/parameters", { params: { path: { heap_id: heapId } } })),
    staleTime: 30_000,
  });
}

/** Parameter changes move items (ranking) and change what they show (labels, switched values). */
/** Refetches the heap's parameters and, unless only they changed (`itemsToo` false), the items that show them. */
function invalidateParameters(queryClient: QueryClient, heapId: string, itemsToo = true) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: parametersKey(heapId) }),
    itemsToo && invalidateHeap(queryClient, heapId),
  ]);
}

export function useCreateParameter(heapId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: NewParameter) =>
      unwrap(await api.POST("/api/v1/heaps/{heap_id}/parameters", { params: { path: { heap_id: heapId } }, body })),
    onSettled: () => invalidateParameters(queryClient, heapId),
  });
}

/** Every change to one parameter that takes its version: name and weight, options, tags. */
export type ParameterChange =
  | { kind: "patch"; patch: ParameterPatch }
  | { kind: "add_option"; label: string }
  | { kind: "arrange_options"; options: ParameterOption[] }
  | { kind: "link_tags"; tagIds: string[] };

export function useChangeParameter(heapId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ parameter, change }: { parameter: Parameter; change: ParameterChange }) => {
      const params = {
        params: { path: { parameter_id: parameter.id }, header: { "If-Match": `"${parameter.version}"` } },
      };
      switch (change.kind) {
        case "patch":
          return unwrap(await api.PATCH("/api/v1/parameters/{parameter_id}", { ...params, body: change.patch }));
        case "add_option":
          return unwrap(
            await api.POST("/api/v1/parameters/{parameter_id}/options", { ...params, body: { label: change.label } }),
          );
        case "arrange_options":
          return unwrap(
            await api.PATCH("/api/v1/parameters/{parameter_id}/options", {
              ...params,
              body: { options: change.options },
            }),
          );
        case "link_tags":
          return unwrap(
            await api.PUT("/api/v1/parameters/{parameter_id}/tags", { ...params, body: { tag_ids: change.tagIds } }),
          );
      }
    },
    // Also after a conflict, so the page shows the version someone else saved. Tag links don't show on items.
    onSettled: (_data, _error, { change }) => invalidateParameters(queryClient, heapId, change.kind !== "link_tags"),
  });
}

export function useArchiveParameter(heapId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (parameter: Parameter) =>
      unwrap(
        await api.POST("/api/v1/parameters/{parameter_id}/archive", {
          params: { path: { parameter_id: parameter.id } },
        }),
      ),
    onSettled: () => invalidateParameters(queryClient, heapId),
  });
}

/** Archives an option; open items that have it switch to `replacementId`. */
export function useArchiveOption(heapId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ optionId, replacementId }: { optionId: string; replacementId: string }) =>
      unwrap(
        await api.POST("/api/v1/options/{option_id}/archive", {
          params: { path: { option_id: optionId } },
          body: { replacement_option_id: replacementId },
        }),
      ),
    onSettled: () => invalidateParameters(queryClient, heapId),
  });
}

/** The heap's saved views in use, by name. */
export function useSavedViews(heapId: string) {
  return useQuery({
    queryKey: savedViewsKey(heapId),
    queryFn: async () =>
      unwrap(await api.GET("/api/v1/heaps/{heap_id}/views", { params: { path: { heap_id: heapId } } })),
    staleTime: 30_000,
  });
}

export function useCreateSavedView(heapId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: { name: string; view: ListViewInput }) =>
      unwrap(await api.POST("/api/v1/heaps/{heap_id}/views", { params: { path: { heap_id: heapId } }, body })),
    onSettled: () => queryClient.invalidateQueries({ queryKey: savedViewsKey(heapId) }),
  });
}

export function useUpdateSavedView(heapId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ saved, patch }: { saved: SavedView; patch: { name?: string; view?: ListViewInput } }) =>
      unwrap(
        await api.PATCH("/api/v1/views/{view_id}", {
          params: { path: { view_id: saved.id }, header: { "If-Match": `"${saved.version}"` } },
          body: patch,
        }),
      ),
    // Also after a conflict, so the bar shows the version someone else saved.
    onSettled: () => queryClient.invalidateQueries({ queryKey: savedViewsKey(heapId) }),
  });
}

export function useArchiveSavedView(heapId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (saved: SavedView) =>
      unwrap(await api.POST("/api/v1/views/{view_id}/archive", { params: { path: { view_id: saved.id } } })),
    onSettled: () => queryClient.invalidateQueries({ queryKey: savedViewsKey(heapId) }),
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
        queryClient.invalidateQueries({ queryKey: meQuery.queryKey }),
        queryClient.invalidateQueries({ queryKey: ["team"] }),
      ]),
  });
}

export function useTeam(teamId: string) {
  return useQuery({
    queryKey: teamKey(teamId),
    queryFn: async () => unwrap(await api.GET("/api/v1/teams/{team_id}", { params: { path: { team_id: teamId } } })),
  });
}

/** Team changes also change what `/me` lists: teams, roles and heaps. */
function invalidateTeam(queryClient: QueryClient, teamId: string) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: teamKey(teamId) }),
    queryClient.invalidateQueries({ queryKey: meQuery.queryKey }),
  ]);
}

export function useCreateTeam() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (name: string) => unwrap(await api.POST("/api/v1/teams", { body: { name } })),
    onSuccess: (team) => queryClient.setQueryData(teamKey(team.id), team),
    onSettled: () => queryClient.invalidateQueries({ queryKey: meQuery.queryKey }),
  });
}

export function useRenameTeam(teamId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ team, name }: { team: Team; name: string }) =>
      unwrap(
        await api.PATCH("/api/v1/teams/{team_id}", {
          params: { path: { team_id: team.id }, header: { "If-Match": `"${team.version}"` } },
          body: { name },
        }),
      ),
    onSettled: () => invalidateTeam(queryClient, teamId),
  });
}

export function useAddMember(teamId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (userId: string) =>
      unwrap(
        await api.POST("/api/v1/teams/{team_id}/members", {
          params: { path: { team_id: teamId } },
          body: { user_id: userId },
        }),
      ),
    onSettled: () => invalidateTeam(queryClient, teamId),
  });
}

export function useSetMemberRole(teamId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ userId, role }: { userId: string; role: Role }) =>
      unwrap(
        await api.PATCH("/api/v1/teams/{team_id}/members/{user_id}", {
          params: { path: { team_id: teamId, user_id: userId } },
          body: { role },
        }),
      ),
    onSettled: () => invalidateTeam(queryClient, teamId),
  });
}

export function useRemoveMember(teamId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (userId: string) =>
      ensureOk(
        await api.DELETE("/api/v1/teams/{team_id}/members/{user_id}", {
          params: { path: { team_id: teamId, user_id: userId } },
        }),
      ),
    onSettled: () => invalidateTeam(queryClient, teamId),
  });
}

/**
 * Leaves the team, then goes to the Teams view. The team and its heaps are out of reach afterwards, so their cache
 * entries are dropped, after navigating away: dropping them while the team page still shows would refetch them into
 * a "not found".
 */
export function useLeaveTeam(team: Team) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  return useMutation({
    mutationFn: async () =>
      ensureOk(await api.POST("/api/v1/teams/{team_id}/leave", { params: { path: { team_id: team.id } } })),
    onSuccess: async () => {
      await navigate({ to: "/teams" });
      queryClient.removeQueries({ queryKey: teamKey(team.id) });
      for (const heap of team.heaps) {
        for (const key of [
          heapKey(heap.id),
          rankedKey(heap.id),
          closedKey(heap.id),
          tagsKey(heap.id),
          savedViewsKey(heap.id),
          parametersKey(heap.id),
        ]) {
          queryClient.removeQueries({ queryKey: key });
        }
      }
      await queryClient.invalidateQueries({ queryKey: meQuery.queryKey });
    },
  });
}

/**
 * Users with a word of their name starting with `query`, or whose email starts with it once it has an `@` and part of
 * the domain. Runs once `query` has the 3 characters the server needs.
 */
export function useFindUsers(query: string) {
  const text = query.trim();
  return useQuery({
    queryKey: usersKey(text),
    queryFn: async () => unwrap(await api.GET("/api/v1/users", { params: { query: { query: text } } })),
    enabled: [...text].length >= 3,
    placeholderData: keepPreviousData,
    staleTime: 30_000,
  });
}

export function useHeap(heapId: string) {
  return useQuery({
    queryKey: heapKey(heapId),
    queryFn: async () => unwrap(await api.GET("/api/v1/heaps/{heap_id}", { params: { path: { heap_id: heapId } } })),
    staleTime: 60_000,
  });
}

export function useCreateHeap(teamId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (name: string) =>
      unwrap(
        await api.POST("/api/v1/teams/{team_id}/heaps", { params: { path: { team_id: teamId } }, body: { name } }),
      ),
    onSettled: () => invalidateTeam(queryClient, teamId),
  });
}

/** Everything that shows a heap's name or archive state. Archiving also changes every item's allowed actions. */
function invalidateHeapRecord(queryClient: QueryClient, heap: Pick<Heap, "id" | "team_id">) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: heapKey(heap.id) }),
    invalidateTeam(queryClient, heap.team_id),
    invalidateHeap(queryClient, heap.id),
    queryClient.invalidateQueries({ queryKey: tagsKey(heap.id) }),
    queryClient.invalidateQueries({ queryKey: savedViewsKey(heap.id) }),
    queryClient.invalidateQueries({ queryKey: parametersKey(heap.id) }),
  ]);
}

export function useRenameHeap() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ heap, name }: { heap: Heap; name: string }) =>
      unwrap(
        await api.PATCH("/api/v1/heaps/{heap_id}", {
          params: { path: { heap_id: heap.id }, header: { "If-Match": `"${heap.version}"` } },
          body: { name },
        }),
      ),
    onSettled: (_data, _error, { heap }) => invalidateHeapRecord(queryClient, heap),
  });
}

export function useSetHeapArchived() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ heap, archived }: { heap: Heap; archived: boolean }) => {
      const params = { params: { path: { heap_id: heap.id } } };
      return unwrap(
        archived
          ? await api.POST("/api/v1/heaps/{heap_id}/archive", params)
          : await api.POST("/api/v1/heaps/{heap_id}/unarchive", params),
      );
    },
    onSettled: (_data, _error, { heap }) => invalidateHeapRecord(queryClient, heap),
  });
}
