import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { api, ensureOk, unwrap } from "../client";
import type { DataDays, Role, Team } from "../schema";
import {
  rankedKey,
  closedKey,
  tagsKey,
  parametersKey,
  teamKey,
  heapKey,
  usersKey,
  invalidateMe,
  invalidateTeam,
} from "./keys";

/** The team whose slug is `team`. */
export function useTeam(team: string) {
  return useQuery({
    queryKey: teamKey(team),
    queryFn: async () => unwrap(await api.GET("/api/v1/teams/{team}", { params: { path: { team } } })),
  });
}

/**
 * What happened in the team's heaps over the last `days` days, or in its heap `heap` alone (ux.md, Data). Fetched on
 * open, on focus and on each change of choice, not polled; the figures stay on screen while another choice loads.
 */
export function useTeamData(team: string, choice: { heap: string | undefined; days: DataDays }) {
  return useQuery({
    queryKey: [...teamKey(team), "data", choice.heap ?? null, choice.days] as const,
    queryFn: async () =>
      unwrap(
        await api.GET("/api/v1/teams/{team}/data", {
          params: { path: { team }, query: { heap: choice.heap, days: choice.days } },
        }),
      ),
    placeholderData: keepPreviousData,
  });
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
 * Whether the server takes `query` as a user lookup: 3 characters, or a whole username shorter than that (`ab`), which
 * then matches only that username.
 */
export function isUserQuery(query: string): boolean {
  const text = query.trim().toLowerCase();
  return [...text].length >= 3 || /^[a-z][a-z0-9]$/.test(text);
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
    enabled: isUserQuery(text),
    placeholderData: keepPreviousData,
    staleTime: 30_000,
  });
}
