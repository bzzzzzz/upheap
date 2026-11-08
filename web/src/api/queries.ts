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
  type HeapRef,
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
  type RankedList,
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

// Heaps and teams are cached by their slugs, as their paths name them; a new slug is a new path, and the app moves
// there. Every list query of a heap sits under `["ranked", team, heap]`, so one invalidation covers them all. The
// blocker candidates have a key of their own, so they never share a cache entry (and its `as_of`) with a list view.
const rankedKey = (heap: HeapRef) => ["ranked", heap.team, heap.heap] as const;
const rankedViewKey = (heap: HeapRef, view: ListViewQuery) => [...rankedKey(heap), "view", view] as const;
const blockerCandidatesKey = (heap: HeapRef) => [...rankedKey(heap), "blocker-candidates"] as const;
const closedKey = (heap: HeapRef) => ["closed", heap.team, heap.heap] as const;
const tagsKey = (heap: HeapRef) => ["tags", heap.team, heap.heap] as const;
const savedViewsKey = (heap: HeapRef) => ["saved-views", heap.team, heap.heap] as const;
const parametersKey = (heap: HeapRef) => ["parameters", heap.team, heap.heap] as const;
const itemKey = (heap: HeapRef, key: string) => ["item", heap.team, heap.heap, key] as const;
const teamKey = (team: string) => ["team", team] as const;
const heapKey = (heap: HeapRef) => ["heap", heap.team, heap.heap] as const;
const usersKey = (query: string) => ["users", query] as const;

/**
 * The ranked list in one view, kept stable within a session: the first response's `as_of` is sent back on every
 * refetch and view change, so only edited items move. `refresh` drops it and ranks at the current time. While a
 * new view loads, the previous one stays on screen.
 */
export function useRankedList(heap: HeapRef, view: ListViewQuery) {
  const asOf = useRef<number | undefined>(undefined);
  const query = useQuery({
    queryKey: rankedViewKey(heap, view),
    queryFn: async () => {
      const data = unwrap(
        await api.GET("/api/v1/teams/{team}/heaps/{heap}/items", {
          params: { path: heap, query: { ...view, as_of: asOf.current } },
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
export function useBlockerCandidates(heap: HeapRef, enabled = true) {
  return useQuery({
    queryKey: blockerCandidatesKey(heap),
    queryFn: async () =>
      unwrap(
        await api.GET("/api/v1/teams/{team}/heaps/{heap}/items", {
          params: { path: heap, query: EVERY_NON_CLOSED_ITEM },
        }),
      ),
    select: (data) => [...data.pinned, ...data.ranked, ...data.blocked],
    enabled,
  });
}

export function useClosedItems(heap: HeapRef) {
  return useInfiniteQuery({
    queryKey: closedKey(heap),
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam }) =>
      unwrap(
        await api.GET("/api/v1/teams/{team}/heaps/{heap}/items/closed", {
          params: { path: heap, query: { cursor: pageParam } },
        }),
      ),
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  });
}

/** Whose history to read: a heap's or a team's, or one entity's within it. */
export type AuditScope = { heap: HeapRef } | { team: string };

/**
 * A page-by-page history, newest first. Every write is audited, so `main.tsx` invalidates these after any mutation.
 */
export function useAuditLog(scope: AuditScope, entityId?: string) {
  const owner = "heap" in scope ? ["heap", scope.heap.team, scope.heap.heap] : ["team", scope.team];
  return useInfiniteQuery({
    queryKey: ["audit", ...owner, entityId ?? null],
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam }) => {
      const query = { entity_id: entityId, cursor: pageParam };
      const result =
        "heap" in scope
          ? await api.GET("/api/v1/teams/{team}/heaps/{heap}/audit", { params: { path: scope.heap, query } })
          : await api.GET("/api/v1/teams/{team}/audit", { params: { path: { team: scope.team }, query } });
      return unwrap(result);
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  });
}

/**
 * One item of `heap`, by its key, for the item pane. Each change saves on its own, so there's no unsaved form for a
 * refetch to reset.
 */
export function useItem(heap: HeapRef, key: string) {
  return useQuery({
    queryKey: itemKey(heap, key),
    queryFn: async () =>
      unwrap(
        await api.GET("/api/v1/teams/{team}/heaps/{heap}/items/{item}", { params: { path: { ...heap, item: key } } }),
      ),
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

function invalidateHeap(queryClient: QueryClient, heap: HeapRef) {
  return Promise.all([
    // Every view of the list, by prefix.
    queryClient.invalidateQueries({ queryKey: rankedKey(heap) }),
    queryClient.invalidateQueries({ queryKey: closedKey(heap) }),
    // Any open item may show a changed blocker or tag, so every cached item is refetched.
    queryClient.invalidateQueries({ queryKey: ["item"] }),
  ]);
}

/**
 * Fetches a view of a heap's ranked list again, the one on screen, and gives it back. A fetch already on its way, as
 * after an add invalidates the heap, is waited for rather than started again, so Add says where an item landed from
 * the list itself rather than from a second request (ux-tracker.md, open question 4). `undefined` if it fails.
 */
export function useRefetchRankedList() {
  const queryClient = useQueryClient();
  return async (heap: HeapRef, view: ListViewQuery): Promise<RankedList | undefined> => {
    const queryKey = rankedViewKey(heap, view);
    await queryClient.refetchQueries({ queryKey, exact: true }, { cancelRefetch: false });
    return queryClient.getQueryData<RankedList>(queryKey);
  };
}

/**
 * Adds an item to `heap`. The heap goes with each call rather than with the hook, as the Add dialog can change heap
 * while an add is on its way, and the views to fetch again are those of the heap the item went into.
 */
export function useCreateItem() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ heap, body }: { heap: HeapRef; body: NewItem }) =>
      unwrap(await api.POST("/api/v1/teams/{team}/heaps/{heap}/items", { params: { path: heap }, body })),
    // Not returned: the mutation is done once the server has the item, and the lists catch up after.
    onSettled: (_item, _error, { heap }) => void invalidateHeap(queryClient, heap),
  });
}

/**
 * Saves one change to an item. The item pane saves each change as it's made, so two can follow each other before
 * the first is back: edits run one at a time (one `scope`), and each sends the newest version it knows of, its own
 * item's or the one the previous edit brought back. A change by someone else still fails with a conflict.
 */
export function useUpdateItem(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    scope: { id: "item-edits" },
    mutationFn: async ({ item, patch }: { item: Item; patch: ItemPatch }) => {
      const version = Math.max(item.version, queryClient.getQueryData<Item>(itemKey(heap, item.key))?.version ?? 0);
      return unwrap(
        await api.PATCH("/api/v1/teams/{team}/heaps/{heap}/items/{item}", {
          params: { path: { ...heap, item: item.key }, header: { "If-Match": `"${version}"` } },
          body: patch,
        }),
      );
    },
    onSuccess: (saved) => queryClient.setQueryData(itemKey(heap, saved.key), saved),
    // Always re-render from the server, also after a conflict, so the row shows the latest version.
    onSettled: () => invalidateHeap(queryClient, heap),
  });
}

export function useReopenItem(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (item: Item) =>
      unwrap(
        await api.POST("/api/v1/teams/{team}/heaps/{heap}/items/{item}/reopen", {
          params: { path: { ...heap, item: item.key }, header: { "If-Match": `"${item.version}"` } },
        }),
      ),
    onSettled: () => invalidateHeap(queryClient, heap),
  });
}

export function useTags(heap: HeapRef) {
  return useQuery({
    queryKey: tagsKey(heap),
    queryFn: async () => unwrap(await api.GET("/api/v1/teams/{team}/heaps/{heap}/tags", { params: { path: heap } })),
    staleTime: 30_000,
  });
}

export function useCreateTag(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: NewTag) =>
      unwrap(await api.POST("/api/v1/teams/{team}/heaps/{heap}/tags", { params: { path: heap }, body })),
    onSettled: () => queryClient.invalidateQueries({ queryKey: tagsKey(heap) }),
  });
}

export function useUpdateTag(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ tag, patch }: { tag: Tag; patch: TagPatch }) =>
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

/** The heap's parameters: those in use and the archived ones, each by name. */
export function useParameters(heap: HeapRef) {
  return useQuery({
    queryKey: parametersKey(heap),
    queryFn: async () =>
      unwrap(await api.GET("/api/v1/teams/{team}/heaps/{heap}/parameters", { params: { path: heap } })),
    staleTime: 30_000,
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
    mutationFn: async (body: NewParameter) =>
      unwrap(await api.POST("/api/v1/teams/{team}/heaps/{heap}/parameters", { params: { path: heap }, body })),
    onSettled: () => invalidateParameters(queryClient, heap),
  });
}

/** Every change to one parameter that takes its version: name and weight, options, tags. */
export type ParameterChange =
  | { kind: "patch"; patch: ParameterPatch }
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

/** The heap's saved views in use, by name. */
export function useSavedViews(heap: HeapRef) {
  return useQuery({
    queryKey: savedViewsKey(heap),
    queryFn: async () => unwrap(await api.GET("/api/v1/teams/{team}/heaps/{heap}/views", { params: { path: heap } })),
    staleTime: 30_000,
  });
}

export function useCreateSavedView(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: { name: string; view: ListViewInput }) =>
      unwrap(await api.POST("/api/v1/teams/{team}/heaps/{heap}/views", { params: { path: heap }, body })),
    onSettled: () => queryClient.invalidateQueries({ queryKey: savedViewsKey(heap) }),
  });
}

export function useUpdateSavedView(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ saved, patch }: { saved: SavedView; patch: { name?: string; view?: ListViewInput } }) =>
      unwrap(
        await api.PATCH("/api/v1/teams/{team}/heaps/{heap}/views/{view_id}", {
          params: { path: { ...heap, view_id: saved.id }, header: { "If-Match": `"${saved.version}"` } },
          body: patch,
        }),
      ),
    // Also after a conflict, so the bar shows the version someone else saved.
    onSettled: () => queryClient.invalidateQueries({ queryKey: savedViewsKey(heap) }),
  });
}

export function useArchiveSavedView(heap: HeapRef) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (saved: SavedView) =>
      unwrap(
        await api.POST("/api/v1/teams/{team}/heaps/{heap}/views/{view_id}/archive", {
          params: { path: { ...heap, view_id: saved.id } },
        }),
      ),
    onSettled: () => queryClient.invalidateQueries({ queryKey: savedViewsKey(heap) }),
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
    mutationFn: async (body: { email: string; username: string; display_name: string; password: string }) =>
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

/** The team whose slug is `team`. */
export function useTeam(team: string) {
  return useQuery({
    queryKey: teamKey(team),
    queryFn: async () => unwrap(await api.GET("/api/v1/teams/{team}", { params: { path: { team } } })),
  });
}

/** Team changes also change what `/me` lists: teams, roles and heaps. */
function invalidateTeam(queryClient: QueryClient, team: string) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: teamKey(team) }),
    queryClient.invalidateQueries({ queryKey: meQuery.queryKey }),
  ]);
}

export function useCreateTeam() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (name: string) => unwrap(await api.POST("/api/v1/teams", { body: { name } })),
    onSuccess: (team) => queryClient.setQueryData(teamKey(team.slug), team),
    onSettled: () => queryClient.invalidateQueries({ queryKey: meQuery.queryKey }),
  });
}

/**
 * Renames the team or gives it a new slug. A new slug is a new path, so the app moves there before the old one is
 * refetched into a "not found".
 */
export function useUpdateTeam() {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  return useMutation({
    mutationFn: async ({ team, patch }: { team: Team; patch: { name?: string; slug?: string } }) =>
      unwrap(
        await api.PATCH("/api/v1/teams/{team}", {
          params: { path: { team: team.slug }, header: { "If-Match": `"${team.version}"` } },
          body: patch,
        }),
      ),
    onSuccess: async (saved, { team }) => {
      queryClient.setQueryData(teamKey(saved.slug), saved);
      if (saved.slug !== team.slug) {
        await navigate({ to: "/teams/$team", params: { team: saved.slug }, replace: true });
      }
    },
    onSettled: (saved, _error, { team }) => invalidateTeam(queryClient, saved?.slug ?? team.slug),
  });
}

export function useAddMember(team: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (userId: string) =>
      unwrap(await api.POST("/api/v1/teams/{team}/members", { params: { path: { team } }, body: { user_id: userId } })),
    onSettled: () => invalidateTeam(queryClient, team),
  });
}

export function useSetMemberRole(team: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ username, role }: { username: string; role: Role }) =>
      unwrap(
        await api.PATCH("/api/v1/teams/{team}/members/{username}", {
          params: { path: { team, username } },
          body: { role },
        }),
      ),
    onSettled: () => invalidateTeam(queryClient, team),
  });
}

export function useRemoveMember(team: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (username: string) =>
      ensureOk(await api.DELETE("/api/v1/teams/{team}/members/{username}", { params: { path: { team, username } } })),
    onSettled: () => invalidateTeam(queryClient, team),
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
      ensureOk(await api.POST("/api/v1/teams/{team}/leave", { params: { path: { team: team.slug } } })),
    onSuccess: async () => {
      await navigate({ to: "/teams" });
      queryClient.removeQueries({ queryKey: teamKey(team.slug) });
      for (const { slug } of team.heaps) {
        const heap = { team: team.slug, heap: slug };
        for (const key of [
          heapKey(heap),
          rankedKey(heap),
          closedKey(heap),
          tagsKey(heap),
          savedViewsKey(heap),
          parametersKey(heap),
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
/**
 * Whether the server takes `query` as a user lookup: 3 characters, or a whole username shorter than that (`ab`), which
 * then matches only that username.
 */
export function isUserQuery(query: string): boolean {
  const text = query.trim().toLowerCase();
  return [...text].length >= 3 || /^[a-z][a-z0-9]$/.test(text);
}

export function useFindUsers(query: string) {
  const text = query.trim();
  return useQuery({
    queryKey: usersKey(text),
    queryFn: async () => unwrap(await api.GET("/api/v1/users", { params: { query: { query: text } } })),
    enabled: isUserQuery(text),
    placeholderData: keepPreviousData,
    staleTime: 30_000,
  });
}

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
    mutationFn: async (name: string) =>
      unwrap(await api.POST("/api/v1/teams/{team}/heaps", { params: { path: { team } }, body: { name } })),
    onSettled: () => invalidateTeam(queryClient, team),
  });
}

/** Everything that shows a heap's name or archive state. Archiving also changes every item's allowed actions. */
function invalidateHeapRecord(queryClient: QueryClient, heap: HeapRef) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: heapKey(heap) }),
    invalidateTeam(queryClient, heap.team),
    invalidateHeap(queryClient, heap),
    queryClient.invalidateQueries({ queryKey: tagsKey(heap) }),
    queryClient.invalidateQueries({ queryKey: savedViewsKey(heap) }),
    queryClient.invalidateQueries({ queryKey: parametersKey(heap) }),
  ]);
}

/**
 * Renames a heap of the team `team` or gives it a new slug. A new slug is a new path: when the app is on one of the
 * heap's pages, it moves to the same page at the new path, with the same search.
 */
export function useUpdateHeap(team: string) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  return useMutation({
    mutationFn: async ({ heap, patch }: { heap: Heap; patch: { name?: string; slug?: string } }) =>
      unwrap(
        await api.PATCH("/api/v1/teams/{team}/heaps/{heap}", {
          params: { path: { team, heap: heap.slug }, header: { "If-Match": `"${heap.version}"` } },
          body: patch,
        }),
      ),
    onSuccess: async (saved, { heap }) => {
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
