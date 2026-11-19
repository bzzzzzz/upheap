import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, ApiProblem, ensureOk, unwrap } from "../client";
import type { OAuthAuthorizationRequest } from "../schema";

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
 * The OpenID Connect provider people may sign in through on this server, or `null` when it has none (a `404`). Read
 * once: the operator sets it with the server's configuration.
 */
export function useOidcProvider() {
  return useQuery({
    queryKey: ["oidc-provider"],
    queryFn: async () => {
      try {
        return unwrap(await api.GET("/api/v1/auth/oidc"));
      } catch (error) {
        if (error instanceof ApiProblem && error.status === 404) return null;
        throw error;
      }
    },
    staleTime: Infinity,
  });
}

/** Who the provider vouched for, for the username step. A `404` is the step having ended, which stays so. */
export function useOidcPending() {
  return useQuery({
    queryKey: ["oidc-pending"],
    queryFn: async () => unwrap(await api.GET("/api/v1/auth/oidc/pending")),
    retry: false,
  });
}

/** Finishes a first sign-in through the provider with the username picked; the response signs the browser in. */
export function useRegisterOidc() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: { username: string }) => unwrap(await api.POST("/api/v1/auth/oidc/register", { body })),
    // As `useRegister`: nothing of a previous user's cache survives, and the guard loads `me` afresh.
    onSuccess: () => queryClient.removeQueries(),
  });
}

/**
 * The client an OAuth authorization request is from, once the server has checked the request; until then the
 * consent page shows no choice and sends the browser nowhere. Not retried: a bad request stays bad.
 */
export function useAuthorizationCheck(
  request: OAuthAuthorizationRequest,
  { enabled = true }: { enabled?: boolean } = {},
) {
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
    mutationFn: async (body: OAuthAuthorizationRequest) => unwrap(await api.POST("/api/v1/oauth/authorize", { body })),
  });
}

export function useSignOut() {
  const queryClient = useQueryClient();
  return useMutation({
    // The body is the provider's logout URL, or `undefined` on a plain `204`.
    mutationFn: async () => {
      const result = await api.POST("/api/v1/auth/logout");
      ensureOk(result);
      return result.data;
    },
    onSuccess: () => queryClient.clear(),
  });
}
