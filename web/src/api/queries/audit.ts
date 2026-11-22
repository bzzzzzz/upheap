import { useInfiniteQuery } from "@tanstack/react-query";
import { api, unwrap, type HeapRef } from "../client";
import { apiKey, heapPath } from "./keys";

/** Whose history to read: a heap's or a team's, or one entity's within it. */
export type AuditScope = { heap: HeapRef } | { team: string };

/**
 * A page-by-page history, newest first. Every write is audited, so `main.tsx` invalidates these after any mutation.
 */
export function useAuditLog(scope: AuditScope, entityId?: string) {
  const query = { entity_id: entityId };
  return useInfiniteQuery({
    queryKey:
      "heap" in scope
        ? apiKey("/api/v1/teams/{team}/heaps/{heap}/audit", { path: heapPath(scope.heap), query })
        : apiKey("/api/v1/teams/{team}/audit", { path: { team: scope.team }, query }),
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam }) => {
      const page = { ...query, cursor: pageParam };
      const result =
        "heap" in scope
          ? await api.GET("/api/v1/teams/{team}/heaps/{heap}/audit", { params: { path: scope.heap, query: page } })
          : await api.GET("/api/v1/teams/{team}/audit", { params: { path: { team: scope.team }, query: page } });
      return unwrap(result);
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  });
}

/**
 * The signed-in user's account history (password events and their heap priority changes), newest first, a page at a
 * time. `main.tsx` refreshes it after any mutation, a password change included. It loads once mounted, which the History
 * disclosure does only when it is opened.
 */
export function useAccountHistory() {
  return useInfiniteQuery({
    queryKey: apiKey("/api/v1/me/history"),
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam }) =>
      unwrap(await api.GET("/api/v1/me/history", { params: { query: { cursor: pageParam } } })),
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  });
}
