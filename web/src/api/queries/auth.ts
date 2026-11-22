import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, ApiProblem, ensureOk, unwrap } from "../client";
import type { ConfirmationSent, OAuthAuthorizationRequest } from "../schema";
import { apiKey, oidcProviderKey, registrationOptionsKey, removeUserQueries } from "./keys";

export function useSignIn() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: { login: string; password: string }) =>
      unwrap(await api.POST("/api/v1/auth/login", { body })),
    // Another user's data may be cached from before a session ended, so start over, and the guard loads `me`
    // afresh. Queries only: the mutation cache holds this mutation, whose callbacks navigate on.
    onSuccess: () => removeUserQueries(queryClient),
  });
}

/**
 * What registering did: signed the browser in (`201`), or, on a server with mail set up, made the account and mailed
 * a confirmation link to `email` without signing in (`202`).
 */
export type Registered = { kind: "signed_in" } | { kind: "confirmation_sent"; email: string };

/**
 * What a registering endpoint's answer means. Its `201` and `202` bodies differ, and only the status tells them
 * apart: `answer` is a `ConfirmationSent` for a `202`, and is not read for a `201`.
 */
export function registeredFrom(response: Response, answer: ConfirmationSent | object): Registered {
  return response.status === 202
    ? { kind: "confirmation_sent", email: (answer as ConfirmationSent).email }
    : { kind: "signed_in" };
}

export function useRegister() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: { email: string; username: string; display_name: string; password: string }) => {
      const result = await api.POST("/api/v1/auth/register", { body });
      return registeredFrom(result.response, unwrap(result));
    },
    // Another user's data may be cached from before a session ended, so start over, and the guard loads `me`
    // afresh. Queries only: the mutation cache holds this mutation, whose callbacks navigate on. Only for a sign-in:
    // an account waiting for its address to be confirmed has no session.
    onSuccess: (registered) => {
      if (registered.kind === "signed_in") removeUserQueries(queryClient);
    },
  });
}

/**
 * The OpenID Connect provider people may sign in through on this server, or `null` when it has none (a `404`). Read
 * once: the operator sets it with the server's configuration.
 */
export function useOidcProvider() {
  return useQuery({
    queryKey: oidcProviderKey,
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

/**
 * Whether anyone may create an account on this server. Read once: the operator sets it with the server's
 * configuration.
 */
export function useRegistrationOptions() {
  return useQuery({
    queryKey: registrationOptionsKey,
    queryFn: async () => unwrap(await api.GET("/api/v1/auth/registration")),
    staleTime: Infinity,
  });
}

/** Who the provider vouched for, for the username step. A `404` is the step having ended, which stays so. */
export function useOidcPending() {
  return useQuery({
    queryKey: apiKey("/api/v1/auth/oidc/pending"),
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
    onSuccess: () => removeUserQueries(queryClient),
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
    queryKey: apiKey("/api/v1/oauth/authorize", { query: request }),
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
    // The mutations go too: their variables may hold what the user typed.
    onSuccess: () => {
      removeUserQueries(queryClient);
      queryClient.getMutationCache().clear();
    },
  });
}
