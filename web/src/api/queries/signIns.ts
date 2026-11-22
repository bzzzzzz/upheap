import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, ensureOk, unwrap } from "../client";
import type { ClientSignIn, PersonalToken, PersonalTokenExpiry } from "../schema";
import { apiKey } from "./keys";

const signInsKey = apiKey("/api/v1/me/sign-ins");

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

const personalTokensKey = apiKey("/api/v1/me/personal-tokens");

/** The signed-in user's live personal tokens, newest first. Only the Command line card asks. */
export function usePersonalTokens() {
  return useQuery({
    queryKey: personalTokensKey,
    queryFn: async () => unwrap(await api.GET("/api/v1/me/personal-tokens")).tokens,
    staleTime: 30_000,
  });
}

/**
 * Makes a personal token. The answer holds the secret, which is shown once: it is kept in the caller's state and the
 * mutation is dropped as soon as nothing watches it (`gcTime: 0`), so it isn't kept in the cache. The list is read
 * again for the new row.
 */
export function useMakePersonalToken() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: { name: string; expires_in: PersonalTokenExpiry }) =>
      unwrap(await api.POST("/api/v1/me/personal-tokens", { body })),
    gcTime: 0,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: personalTokensKey }),
  });
}

/** Revokes a personal token on the server. Revoking one that's gone already succeeds, so it's fine to repeat. */
export function useRevokePersonalToken() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (token: PersonalToken) =>
      ensureOk(await api.DELETE("/api/v1/me/personal-tokens/{id}", { params: { path: { id: token.id } } })),
    onSuccess: (_, token) =>
      queryClient.setQueryData<PersonalToken[]>(personalTokensKey, (list) =>
        list?.filter((other) => other.id !== token.id),
      ),
    // A 404 means the list is out of date, so it is read again either way.
    onSettled: () => queryClient.invalidateQueries({ queryKey: personalTokensKey }),
  });
}

/**
 * Changes the signed-in user's password. The browser is given a new session cookie, so it stays signed in and the
 * cached data stays valid; only the Command line card's lists change, as the change ends every client sign-in and
 * every personal token.
 */
export function useChangePassword() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: { current_password: string; password: string }) =>
      unwrap(await api.POST("/api/v1/me/password", { body })),
    onSuccess: () =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: signInsKey }),
        queryClient.invalidateQueries({ queryKey: personalTokensKey }),
      ]),
  });
}
