import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, ensureOk, unwrap } from "../client";
import type { OperatorAccount } from "../schema";
import { apiKey } from "./keys";

// What an operator reads and does about accounts (ux.md, Accounts). Every endpoint answers `404` to anyone who isn't an
// operator, so these are asked for only once `me.operator` says so.

const accountsKey = apiKey("/api/v1/operator/accounts");

/** Every account on the server, by name. One request: the page searches, filters and pages the list itself. */
export function useOperatorAccounts() {
  return useQuery({
    queryKey: accountsKey,
    queryFn: async () => unwrap(await api.GET("/api/v1/operator/accounts")).accounts,
  });
}

/**
 * The history of one account as an operator reads it, a page at a time. `main.tsx`
 * refreshes it after any mutation, an action on the account included. It loads once mounted.
 */
export function useOperatorAccountHistory(account: OperatorAccount) {
  return useInfiniteQuery({
    queryKey: apiKey("/api/v1/operator/accounts/{user_id}/history", { path: { user_id: account.id } }),
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam }) =>
      unwrap(
        await api.GET("/api/v1/operator/accounts/{user_id}/history", {
          params: { path: { user_id: account.id }, query: { cursor: pageParam } },
        }),
      ),
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  });
}

/** How many sign-ins and personal tokens turning the account off would end, read when Deactivate… is chosen. */
export function useSignInsToEnd() {
  return useMutation({
    mutationFn: async (account: OperatorAccount) =>
      unwrap(
        await api.GET("/api/v1/operator/accounts/{user_id}/sign-ins", { params: { path: { user_id: account.id } } }),
      ),
  });
}

/** Turns a local account off. Repeating it is harmless: an account that is off answers 0 sign-ins ended. */
export function useDeactivateAccount() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (account: OperatorAccount) =>
      unwrap(
        await api.POST("/api/v1/operator/accounts/{user_id}/deactivate", { params: { path: { user_id: account.id } } }),
      ),
    onSettled: () => queryClient.invalidateQueries({ queryKey: accountsKey }),
  });
}

/** Turns a local account back on. Repeating it is harmless. */
export function useReactivateAccount() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (account: OperatorAccount) =>
      ensureOk(
        await api.POST("/api/v1/operator/accounts/{user_id}/reactivate", { params: { path: { user_id: account.id } } }),
      ),
    onSettled: () => queryClient.invalidateQueries({ queryKey: accountsKey }),
  });
}

/**
 * Makes a reset link for the account. The answer holds the link, which is shown once: the caller keeps it in its state
 * and the mutation is dropped as soon as nothing watches it (`gcTime: 0`), so it isn't kept in the cache.
 */
export function useMakeResetLink() {
  return useMutation({
    mutationFn: async (account: OperatorAccount) =>
      unwrap(
        await api.POST("/api/v1/operator/accounts/{user_id}/reset-link", { params: { path: { user_id: account.id } } }),
      ),
    gcTime: 0,
  });
}
