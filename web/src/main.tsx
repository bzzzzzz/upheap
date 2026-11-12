import { MutationCache, QueryCache, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider, type NavigateOptions, type ToOptions } from "@tanstack/react-router";
import { MotionConfig } from "motion/react";
import { StrictMode } from "react";
import { RouterProvider as AriaRouterProvider } from "react-aria-components";
import { createRoot } from "react-dom/client";

import { ApiProblem, isUnauthenticated } from "./api/client";
import { followThemeAcrossTabs } from "./components/theme";
import { createAppRouter, handlesOwnSignIn, isSignInPath, pageHref, signInSearch } from "./routes";
import "./index.css";

/**
 * Sends a user whose session has ended to sign-in, which returns to the page, as the guard (`appRoute`) does. While a
 * navigation is pending, `location` is already where it's going, so the two send the user to the same place. It
 * replaces the page, so Back doesn't return to it signed out. The cache goes first, so neither Back nor a sign-in
 * from another tab lets the guard pass on the last user's `me`, or shows their rows. Sign-in has no queries and its
 * wrong password is `invalid_credentials`, so nothing there gets here and it can't loop; but another request of the
 * page that fails after the first has moved to sign-in mustn't replace its `redirect` with one to sign-in.
 *
 * An invite link's page handles its own sign-in, so a `401` there (the session ended while its confirm was open)
 * leaves the user on it and the cache as it is: the page reads the invitation again and shows it signed out. A reset
 * link's page does too: holding the link is the permission, and setting the password signs the visitor in.
 */
function onUnauthenticated(error: Error) {
  if (!isUnauthenticated(error)) return;
  const { location } = router.state;
  if (handlesOwnSignIn(router, location.pathname)) return;
  queryClient.removeQueries();
  if (isSignInPath(router, location.pathname)) return;
  void router.navigate({ to: "/login", search: signInSearch(pageHref(location.href)), replace: true });
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

// A React Aria link's `href` is a typed route, as a TanStack `<Link>`'s is, so a renamed route or param fails the
// type check rather than shipping a broken link.
declare module "react-aria-components" {
  interface RouterConfig {
    href: ToOptions;
    routerOptions: Omit<NavigateOptions, keyof ToOptions>;
  }
}

followThemeAcrossTabs();

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <QueryClientProvider client={queryClient}>
        <MotionConfig reducedMotion="user">
          {/* Links in React Aria components (heap rows) navigate through the router, under its base path. */}
          <AriaRouterProvider
            navigate={(to, options) => void router.navigate({ ...to, ...options })}
            // React Aria asks for an href for every item, with "" where it has none; only links need building.
            useHref={(to) => (to ? router.buildLocation(to).href : "")}
          >
            <RouterProvider router={router} />
          </AriaRouterProvider>
        </MotionConfig>
      </QueryClientProvider>
    </StrictMode>,
  );
}
