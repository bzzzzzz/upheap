import { useInfiniteQuery } from "@tanstack/react-query";
import { api, unwrap, type HeapRef } from "../client";

/** Whose history to read: a heap's or a team's, or one entity's within it. */
export type AuditScope = { heap: HeapRef } | { team: string };

/**
 * A page-by-page history, newest first. Every write is audited, so `main.tsx` invalidates these after any mutation.
 */
export function useAuditLog(scope: AuditScope, entityId?: string) {
  const owner = "heap" in scope ? ["heap", scope.heap.team, scope.heap.heap] : ["team", scope.team];
  return useInfiniteQuery({
    queryKey: ["audit", ...owner, entityId ?? null],
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam }) => {
      const query = { entity_id: entityId, cursor: pageParam };
      const result =
        "heap" in scope
          ? await api.GET("/api/v1/teams/{team}/heaps/{heap}/audit", { params: { path: scope.heap, query } })
          : await api.GET("/api/v1/teams/{team}/audit", { params: { path: { team: scope.team }, query } });
      return unwrap(result);
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  });
}

/**
 * The signed-in user's account history (password events and their heap priority changes), newest first, a page at a
 * time. Keyed under `"audit"` so that `main.tsx` refreshes it after any mutation, a password change included. It loads once mounted, which the History
 * disclosure does only when it is opened.
 */
export function useAccountHistory() {
  return useInfiniteQuery({
    queryKey: ["audit", "account"],
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam }) =>
      unwrap(await api.GET("/api/v1/me/history", { params: { query: { cursor: pageParam } } })),
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  });
}
