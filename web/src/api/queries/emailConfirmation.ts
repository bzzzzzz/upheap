import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api, unwrap } from "../client";
import { removeUserQueries } from "./keys";

/**
 * Confirms an account's address with the link mailed to it; the response signs the browser in as the link's user. The
 * link isn't read first: a mail scanner that opens it confirms nothing.
 */
export function useConfirmEmail(token: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async () =>
      unwrap(await api.POST("/api/v1/email-confirmations/{token}", { params: { path: { token } } })),
    // As `useSignIn`: nothing of a previous user's cache survives, and the guard loads `me` afresh.
    onSuccess: () => removeUserQueries(queryClient),
  });
}

/**
 * Sets the address of an account that hasn't confirmed it, with the account's login and password, and mails a new link
 * there. The answer names the address and the account's username, whether or not the address changed.
 */
export function useChangeConfirmationAddress() {
  return useMutation({
    mutationFn: async (body: { login: string; password: string; email: string }) =>
      unwrap(await api.POST("/api/v1/auth/confirmation-address", { body })),
  });
}
