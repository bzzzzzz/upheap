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
