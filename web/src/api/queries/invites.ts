import { useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { api, ApiProblem, unwrap } from "../client";
import { inviteLinksKey, invalidateMe, removeUserQueries } from "./keys";

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
    onSuccess: () => removeUserQueries(queryClient),
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
