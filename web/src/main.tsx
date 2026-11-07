import { MutationCache, QueryCache, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { MotionConfig } from "motion/react";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { ApiProblem, isUnauthenticated } from "./api/client";
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
      if (isUnauthenticated(error)) {
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
          <RouterProvider router={router} />
        </MotionConfig>
      </QueryClientProvider>
    </StrictMode>,
  );
}
