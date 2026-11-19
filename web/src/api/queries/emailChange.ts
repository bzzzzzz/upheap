import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api, ensureOk, unwrap } from "../client";
import { meKey } from "./keys";

/** Asks to change the signed-in user's email address: the server mails a link to the new one, and nothing changes yet. */
export function useRequestEmailChange() {
  return useMutation({
    mutationFn: async (body: { email: string; current_password: string }) =>
      ensureOk(await api.POST("/api/v1/me/email-changes", { body })),
  });
}

/**
 * Applies an email change link, which changes the address of the account it was asked for, signed in or not. A
 * signed-in tab reads `me` again for the new address.
 */
export function useApplyEmailChange(token: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async () => unwrap(await api.POST("/api/v1/email-changes/{token}", { params: { path: { token } } })),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: meKey }),
  });
}
