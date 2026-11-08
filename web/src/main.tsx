import { MutationCache, QueryCache, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { MotionConfig } from "motion/react";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { ApiProblem, isUnauthenticated } from "./api/client";
import { LocalDatabaseGate } from "./components/LocalDatabase";
import { createAppRouter, isSignInPath, signInSearch } from "./routes";
import "./index.css";

/**
 * Sends a user whose session has ended to sign-in, which returns to the page, as the guard (`appRoute`) does. While a
 * navigation is pending, `location` is already where it's going, so the two send the user to the same place. It
 * replaces the page, so Back doesn't return to it signed out. The cache goes first, so neither Back nor a sign-in
 * from another tab lets the guard pass on the last user's `me`, or shows their rows. Sign-in has no queries and its
 * wrong password is `invalid_credentials`, so nothing there gets here and it can't loop; but another request of the
 * page that fails after the first has moved to sign-in mustn't replace its `redirect` with one to sign-in.
 */
function onUnauthenticated(error: Error) {
  if (!isUnauthenticated(error)) return;
  queryClient.removeQueries();
  const { location } = router.state;
  if (isSignInPath(router, location.pathname)) return;
  void router.navigate({ to: "/login", search: signInSearch(location.href), replace: true });
}

const queryClient = new QueryClient({
  // Every write records an audit entry, so any open history is out of date after one, even a failed one that the
  // server may have applied. Not awaited: a returned promise would hold every mutation until the histories reload.
  mutationCache: new MutationCache({
    onError: onUnauthenticated,
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: ["audit"] });
    },
  }),
  queryCache: new QueryCache({ onError: onUnauthenticated }),
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
