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
import { localDatabase } from "#local-database";
import { useNavigate } from "@tanstack/react-router";
import { useRef } from "react";

import {
  api,
  ApiProblem,
  ensureOk,
  unwrap,
  type AuthorizationRequest,
  type ClientSignIn,
  type Heap,
  type HeapRef,
  type Item,
  type ItemPatch,
  type ListViewQuery,
  type Me,
  type NewItem,
  type NewParameter,
  type NewStep,
  type NewTag,
  type Parameter,
  type ParameterList,
  type ParameterOption,
  type RankedList,
  type ParameterPatch,
  type RankingPreviewRequest,
  type Role,
  type Question,
  type QuestionReply,
  type Step,
  type StepPatch,
  type Tag,
  type TagPatch,
  type Team,
  type User,
  type Waiting,
  type WeightsPatch,
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
const parametersKey = (heap: HeapRef) => ["parameters", heap.team, heap.heap] as const;
const itemKey = (heap: HeapRef, key: string) => ["item", heap.team, heap.heap, key] as const;
const teamKey = (team: string) => ["team", team] as const;
/** Under the team's key, so invalidating the team refreshes its links too. */
const inviteLinksKey = (team: string) => [...teamKey(team), "invite-links"] as const;
const heapKey = (heap: HeapRef) => ["heap", heap.team, heap.heap] as const;
const waitingKey = ["waiting"] as const;
const usersKey = (query: string) => ["users", query] as const;

/**
 * The ranked list in one view, kept stable within a session: the first response's `as_of` is sent back on every
 * refetch and view change, so only edited items move; reloading the page ranks at the current time. While a new
 * view loads, the previous one stays on screen.
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
  return query;
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
 * What waits for the person across their heaps (ux.md, Waiting for me): its count is in the navigation, so it's fetched
 * for every page, once signed in, and not at all in the local build, where nothing can ask a question. Kept fresh for
 * 30 s and fetched again on window focus and after writes (no polling).
 */
export function useWaiting() {
  return useQuery({
    queryKey: waitingKey,
    queryFn: async () => unwrap(await api.GET("/api/v1/me/waiting")),
    staleTime: 30_000,
    enabled: !localDatabase,
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
    // An item's state or order changes what waits for the person, and in what order.
    queryClient.invalidateQueries({ queryKey: waitingKey }),
  ]);
}

/** `/me` and what waits for the person, which counts the heaps they are in. */
function invalidateMe(queryClient: QueryClient) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: meQuery.queryKey }),
    queryClient.invalidateQueries({ queryKey: waitingKey }),
  ]);
}

/**
 * The heaps' open counts in `/me`, which the sidebar shows. Only adding an item or changing its state changes them,
 * so only those refetch `/me`, and nothing waits for it.
 */
function invalidateOpenCounts(queryClient: QueryClient) {
  void invalidateMe(queryClient);
}

/**
 * Gives back a view of a heap's ranked list, the one on screen, as fetched from now on, so Add can say where an item
 * the server has just taken landed, from the list itself rather than from a second request. A fetch already on
 * its way, as after the add invalidated the heap, is waited for rather than started again; if it was cancelled
 * meanwhile (another change invalidated the heap), the list is fetched once more. If fetching fails, it's the list as
 * cached, which may not have the item yet.
 */
export function useRefetchRankedList() {
  const queryClient = useQueryClient();
  return async (heap: HeapRef, view: ListViewQuery): Promise<RankedList | undefined> => {
    const since = Date.now();
    const queryKey = rankedViewKey(heap, view);
    await queryClient.refetchQueries({ queryKey, exact: true }, { cancelRefetch: false });
    if ((queryClient.getQueryState(queryKey)?.dataUpdatedAt ?? 0) < since) {
      await queryClient.refetchQueries({ queryKey, exact: true });
    }
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
    onSettled: (_item, _error, { heap }) => {
      invalidateOpenCounts(queryClient);
      void invalidateHeap(queryClient, heap);
    },
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
    onSettled: (_saved, _error, { patch }) => {
      if (patch.state !== undefined) invalidateOpenCounts(queryClient);
      return invalidateHeap(queryClient, heap);
    },
  });
}

/**
 * The scope that makes one item's step writes run one at a time (ux.md, The item): a tick followed quickly by a title
 * edit doesn't conflict with itself, as each waits for the one before and sends the newest version it left behind.
 */
const stepScope = (heap: HeapRef, key: string) => ({ id: `step-edits:${heap.team}/${heap.heap}/${key}` });

/**
 * Adds a step to the task `taskKey`. Not optimistic: the item the server returns, with the new step, is written into
 * the cache. Adding needs no version.
 */
export function useAddStep(heap: HeapRef, taskKey: string) {
  const queryClient = useQueryClient();
  return useMutation({
    scope: stepScope(heap, taskKey),
    mutationFn: async (body: NewStep) =>
      unwrap(
        await api.POST("/api/v1/teams/{team}/heaps/{heap}/items/{item}/steps", {
          params: { path: { ...heap, item: taskKey } },
          body,
        }),
      ),
    onSuccess: (saved) => queryClient.setQueryData(itemKey(heap, saved.key), saved),
    onSettled: () => invalidateHeap(queryClient, heap),
  });
}

/**
 * Saves one change to a step of the task `taskKey`: its title, context or state. Each sends the newest version of the
 * step it knows of, its own or the one the previous write brought back. A change by someone else still fails with a
 * conflict; either way the heap is invalidated, which fetches the item and the lists again, so the next try has fresh
 * versions and the rows count the steps.
 */
export function useUpdateStep(heap: HeapRef, taskKey: string) {
  const queryClient = useQueryClient();
  return useMutation({
    scope: stepScope(heap, taskKey),
    mutationFn: async ({ step, patch }: { step: Step; patch: StepPatch }) => {
      const cached = queryClient
        .getQueryData<Item>(itemKey(heap, taskKey))
        ?.steps?.find((candidate) => candidate.number === step.number);
      const version = Math.max(step.version, cached?.version ?? 0);
      return unwrap(
        await api.PATCH("/api/v1/teams/{team}/heaps/{heap}/items/{item}/steps/{step}", {
          params: {
            path: { ...heap, item: taskKey, step: String(step.number) },
            header: { "If-Match": `"${version}"` },
          },
          body: patch,
        }),
      );
    },
    onSuccess: (saved) => queryClient.setQueryData(itemKey(heap, saved.key), saved),
    onSettled: () => invalidateHeap(queryClient, heap),
  });
}

/**
 * Replies to a question of the item `itemId` (its key). Not optimistic: the item the server returns, with the question
 * answered, is written into the cache. A failure fetches the item again, so a question someone else answered meanwhile
 * shows as answered. Each question has its own version, which only a reply moves, so replies to different questions
 * don't wait for each other.
 */
export function useReplyToQuestion(heap: HeapRef, itemId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ question, reply }: { question: Question; reply: QuestionReply }) => {
      const cached = queryClient
        .getQueryData<Item>(itemKey(heap, itemId))
        ?.questions?.find((candidate) => candidate.number === question.number);
      const version = Math.max(question.version, cached?.version ?? 0);
      return unwrap(
        await api.PATCH("/api/v1/teams/{team}/heaps/{heap}/items/{item}/questions/{question}", {
          params: {
            path: { ...heap, item: itemId, question: String(question.number) },
            header: { "If-Match": `"${version}"` },
          },
          body: { reply },
        }),
      );
    },
    onSuccess: (saved, { question }) => {
      queryClient.setQueryData(itemKey(heap, saved.key), saved);
      // The Waiting page's row leaves and its count drops at once; the refetch below confirms them.
      queryClient.setQueryData<Waiting>(
        waitingKey,
        (waiting) =>
          waiting && {
            count: Math.max(0, waiting.count - 1),
            rows: waiting.rows.filter((row) => !(row.item.id === saved.id && row.question.number === question.number)),
          },
      );
    },
    // Not returned: the mutation would wait for the refetches, and with it the focus move and the live message of
    // the row replied to.
    onError: () => {
      void queryClient.invalidateQueries({ queryKey: itemKey(heap, itemId) });
    },
    // The lists say which items wait for you, and Waiting for me what does.
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: rankedKey(heap) });
      void queryClient.invalidateQueries({ queryKey: waitingKey });
    },
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
    onSettled: () => {
      invalidateOpenCounts(queryClient);
      return invalidateHeap(queryClient, heap);
    },
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

export function useSignIn() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: { login: string; password: string }) =>
      unwrap(await api.POST("/api/v1/auth/login", { body })),
    // Another user's data may be cached from before a session ended, so start over, and the guard loads `me`
    // afresh. Queries only: the mutation cache holds this mutation, whose callbacks navigate on.
    onSuccess: () => queryClient.removeQueries(),
  });
}

export function useRegister() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: { email: string; username: string; display_name: string; password: string }) =>
      unwrap(await api.POST("/api/v1/auth/register", { body })),
    // Another user's data may be cached from before a session ended, so start over, and the guard loads `me`
    // afresh. Queries only: the mutation cache holds this mutation, whose callbacks navigate on.
    onSuccess: () => queryClient.removeQueries(),
  });
}

/**
 * The client an OAuth authorization request is from, once the server has checked the request; until then the
 * consent page shows no choice and sends the browser nowhere. Not retried: a bad request stays bad.
 */
export function useAuthorizationCheck(request: AuthorizationRequest, { enabled = true }: { enabled?: boolean } = {}) {
  return useQuery({
    queryKey: ["oauth-authorization", request],
    queryFn: async () => unwrap(await api.GET("/api/v1/oauth/authorize", { params: { query: request } })),
    enabled,
    retry: false,
    staleTime: Infinity,
  });
}

/** Approves an authorization request for the signed-in user; the answer says where to send the browser. */
export function useApproveAuthorization() {
  return useMutation({
    mutationFn: async (body: AuthorizationRequest) => unwrap(await api.POST("/api/v1/oauth/authorize", { body })),
  });
}

const signInsKey = ["sign-ins"] as const;

/**
 * The sign-ins of clients such as the CLI, most recently used first. Only the Command line card asks, and it is shown
 * only when the server says with `manage_sign_ins` that there are some to manage.
 */
export function useSignIns() {
  return useQuery({
    queryKey: signInsKey,
    queryFn: async () => unwrap(await api.GET("/api/v1/me/sign-ins")).sign_ins,
    staleTime: 30_000,
  });
}

/** Ends a client's sign-in on the server. Ending one that's gone already succeeds, so it's fine to repeat. */
export function useEndSignIn() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (signIn: ClientSignIn) =>
      ensureOk(await api.DELETE("/api/v1/me/sign-ins/{id}", { params: { path: { id: signIn.id } } })),
    onSuccess: (_, signIn) =>
      queryClient.setQueryData<ClientSignIn[]>(signInsKey, (list) => list?.filter((other) => other.id !== signIn.id)),
    // A 404 means the list is out of date, so it is read again either way.
    onSettled: () => queryClient.invalidateQueries({ queryKey: signInsKey }),
  });
}

/**
 * Changes the signed-in user's password. The browser is given a new session cookie, so it stays signed in and the
 * cached data stays valid; only the Command line card's list changes, as the change ends every client sign-in.
 */
export function useChangePassword() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: { current_password: string; password: string }) =>
      unwrap(await api.POST("/api/v1/me/password", { body })),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: signInsKey }),
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
    onSettled: () => Promise.all([invalidateMe(queryClient), queryClient.invalidateQueries({ queryKey: ["team"] })]),
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
  return Promise.all([queryClient.invalidateQueries({ queryKey: teamKey(team) }), invalidateMe(queryClient)]);
}

/** Starts a team. Without a `slug`, the server makes one from the name. */
export function useCreateTeam() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: { name: string; slug?: string }) => unwrap(await api.POST("/api/v1/teams", { body })),
    onSuccess: (team) => queryClient.setQueryData(teamKey(team.slug), team),
    onSettled: () => invalidateMe(queryClient),
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

/** The team's invite links, every one made and past, newest first. Admins only: `enabled` is whether the caller is. */
export function useInviteLinks(team: string, enabled: boolean) {
  return useQuery({
    queryKey: inviteLinksKey(team),
    queryFn: async () =>
      unwrap(await api.GET("/api/v1/teams/{team}/invite-links", { params: { path: { team } } })).links,
    enabled,
  });
}

const invitationKey = (token: string) => ["invitation", token] as const;

/**
 * What the invite link with `token` says: its team, its maker, whether it works and where the viewer stands
 * (ux.md, Invite page). Not retried on a 4xx: a link that isn't one stays so.
 */
export function useInvitation(token: string) {
  return useQuery({
    queryKey: invitationKey(token),
    queryFn: async () => unwrap(await api.GET("/api/v1/invites/{token}", { params: { path: { token } } })),
    retry: false,
  });
}

/**
 * Whether a failed join or registration is the invitation having changed: the link ended or was revoked, the viewer
 * was removed, or the session ended. The page then becomes the state that says so, instead of showing the error.
 */
export function isInvitationChange(error: Error): boolean {
  return (
    error instanceof ApiProblem &&
    (error.status === 401 || error.code === "removed_from_team" || error.code.startsWith("invite_link_"))
  );
}

/** Reads the invitation again after a write that found it changed (`isInvitationChange`). */
function refreshInvitationAfter(queryClient: QueryClient, token: string, error: Error) {
  if (isInvitationChange(error)) {
    void queryClient.invalidateQueries({ queryKey: invitationKey(token) });
  }
}

/** Joins the link's team as the signed-in user. `created` is whether they joined now, rather than being in it already. */
export function useJoinByInvite(token: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async () => {
      const result = await api.POST("/api/v1/invites/{token}/join", { params: { path: { token } } });
      return { team: unwrap(result), created: result.response.status === 201 };
    },
    // The new team shows in the shell. Awaited, so the landing finds it there.
    onSuccess: () => invalidateMe(queryClient),
    onError: (error) => refreshInvitationAfter(queryClient, token, error),
  });
}

/** Creates an account through the link and joins its team in the same act; the response signs the browser in. */
export function useRegisterByInvite(token: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: { email: string; username: string; display_name: string; password: string }) =>
      unwrap(await api.POST("/api/v1/invites/{token}/register", { params: { path: { token } }, body })),
    // As `useRegister`: nothing of a previous user's cache survives.
    onSuccess: () => queryClient.removeQueries(),
    onError: (error) => refreshInvitationAfter(queryClient, token, error),
  });
}

/** Makes an invite link that works for a week. */
export function useCreateInviteLink(team: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async () => unwrap(await api.POST("/api/v1/teams/{team}/invite-links", { params: { path: { team } } })),
    onSettled: () => queryClient.invalidateQueries({ queryKey: inviteLinksKey(team) }),
  });
}

/** Stops the invite link with `id` working at once. */
export function useRevokeInviteLink(team: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) =>
      unwrap(await api.POST("/api/v1/teams/{team}/invite-links/{id}/revoke", { params: { path: { team, id } } })),
    onSettled: () => queryClient.invalidateQueries({ queryKey: inviteLinksKey(team) }),
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
        for (const key of [heapKey(heap), rankedKey(heap), closedKey(heap), tagsKey(heap), parametersKey(heap)]) {
          queryClient.removeQueries({ queryKey: key });
        }
      }
      await invalidateMe(queryClient);
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
      patch: { name?: string; slug?: string; context?: string | null };
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
    mutationFn: async ({ heap, body }: { heap: Heap; body: WeightsPatch }) =>
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
 * in `draft`, so no draft is the current top of the list. While a new draft loads, the last result stays. It sits
 * under the heap's key, so saving or refetching the heap refreshes it.
 */
export function useRankingPreview(heapRef: HeapRef, draft: RankingPreviewDraft) {
  return useQuery({
    queryKey: [...heapKey(heapRef), "ranking-preview", draft],
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
