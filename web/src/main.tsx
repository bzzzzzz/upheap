import { MutationCache, QueryCache, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { MotionConfig } from "motion/react";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { ApiProblem, isUnauthenticated } from "./api/client";
import { LocalDatabaseGate } from "./components/LocalDatabase";
import { createAppRouter } from "./routes";
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
      // While the router loads a page, its guard (`appRoute`) sends a signed-out user to sign-in itself, with the way
      // back to that page; navigating here as well would race it and lose the way back.
      if (isUnauthenticated(error) && router.state.status !== "pending") {
        void router.navigate({ to: "/login" });
      }
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
