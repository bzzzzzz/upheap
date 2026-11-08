import { MutationCache, QueryCache, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { MotionConfig } from "motion/react";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { ApiProblem, isUnauthenticated } from "./api/client";
import { LocalDatabaseGate } from "./components/LocalDatabase";
import { createAppRouter, signInSearch } from "./routes";
import "./index.css";

const queryClient = new QueryClient({
  // Every write records an audit entry, so any open history is out of date after one, even a failed one that the
  // server may have applied. Not awaited: a returned promise would hold every mutation until the histories reload.
  mutationCache: new MutationCache({
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: ["audit"] });
    },
  }),
  queryCache: new QueryCache({
    onError: (error) => {
      if (!isUnauthenticated(error)) return;
      // Sign-in returns to the page, as from the guard (`appRoute`). While a navigation is pending, `location` is
      // already where it's going, so the two send the user to the same place.
      const { location } = router.state;
      if (router.matchRoutes(location.pathname, {}).some((match) => match.routeId === "/login")) return;
      void router.navigate({ to: "/login", search: signInSearch(location.href) });
    },
  }),
  defaultOptions: {
    queries: {
      // Client errors won't fix themselves; retry only server and network failures.
      retry: (failures, error) => !(error instanceof ApiProblem && error.status < 500) && failures < 2,
      refetchOnWindowFocus: true,
    },
  },
});

const router = createAppRouter(queryClient);

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <QueryClientProvider client={queryClient}>
        <MotionConfig reducedMotion="user">
          <LocalDatabaseGate>
            <RouterProvider router={router} />
          </LocalDatabaseGate>
        </MotionConfig>
      </QueryClientProvider>
    </StrictMode>,
  );
}
