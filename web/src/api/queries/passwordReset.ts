import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, ApiProblem, ensureOk, unwrap } from "../client";

/** Whether the server mails password reset links. Read once: the operator sets mail up with the server's configuration. */
export function usePasswordResetOptions() {
  return useQuery({
    queryKey: ["password-reset-options"],
    queryFn: async () => unwrap(await api.GET("/api/v1/password-resets")),
    staleTime: Infinity,
  });
}

/** Asks for a password reset link to be mailed to an address. The answer is the same whether an account has it. */
export function useRequestPasswordReset() {
  return useMutation({
    mutationFn: async (body: { email: string }) => ensureOk(await api.POST("/api/v1/password-resets", { body })),
  });
}

const passwordResetKey = (token: string) => ["password-reset", token] as const;

/**
 * Reads the password reset link a page was opened with: whose account it is for and whether it works (ux.md, Reset
 * page). Not retried on a 4xx: a link that isn't one stays so.
 */
export function usePasswordReset(token: string) {
  return useQuery({
    queryKey: passwordResetKey(token),
    queryFn: async () => unwrap(await api.GET("/api/v1/password-resets/{token}", { params: { path: { token } } })),
    retry: false,
  });
}

/**
 * Sets a new password with the link; the response signs the browser in as the link's user. A `404` or `409` is the
 * link having changed (another tab used it, it ended), so the page reads it again and becomes the state that says so.
 */
export function useResetPassword(token: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: { password: string }) =>
      unwrap(await api.POST("/api/v1/password-resets/{token}", { params: { path: { token } }, body })),
    // As `useSignIn`: nothing of a previous user's cache survives, and the guard loads `me` afresh. The link stays,
    // so the page doesn't flash "Checking the link…" before it moves on.
    onSuccess: () =>
      queryClient.removeQueries({ predicate: (query) => query.queryKey[0] !== passwordResetKey(token)[0] }),
    onError: (error) => {
      if (error instanceof ApiProblem && (error.status === 404 || error.status === 409)) {
        void queryClient.invalidateQueries({ queryKey: passwordResetKey(token) });
      }
    },
  });
}
