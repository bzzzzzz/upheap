// The URL is the state: the heap and the page live in the route, so links and reloads work.
import type { QueryClient } from "@tanstack/react-query";
import {
  createRootRouteWithContext,
  createRoute,
  createRouter,
  Link,
  Outlet,
  redirect,
  useParams,
} from "@tanstack/react-router";

import { isUnauthenticated } from "./api/client";
import { meQuery, soleTeam, useMe } from "./api/queries";
import { parseListSearch } from "./components/listView";
import { AccountPage } from "./pages/AccountPage";
import { HeapActivityPage, TeamActivityPage } from "./pages/ActivityPage";
import { AppLayout } from "./pages/AppLayout";
import { AuthPage } from "./pages/AuthPage";
import { ClosedPage } from "./pages/ClosedPage";
import { HeapPage } from "./pages/HeapPage";
import { ParametersPage } from "./pages/ParametersPage";
import { TagsPage } from "./pages/TagsPage";
import { TeamPage } from "./pages/TeamPage";
import { TeamsPage } from "./pages/TeamsPage";

const linkClass = "text-sky-700 hover:underline dark:text-sky-400";

const rootRoute = createRootRouteWithContext<{ queryClient: QueryClient }>()({ component: Outlet });

const loginRoute = createRoute({ getParentRoute: () => rootRoute, path: "/login", component: AuthPage });

const appRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: "app",
  beforeLoad: async ({ context }) => {
    try {
      await context.queryClient.ensureQueryData(meQuery);
    } catch (error) {
      if (isUnauthenticated(error)) {
        throw redirect({ to: "/login" });
      }
      throw error;
    }
  },
  component: AppLayout,
});

const indexRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/",
  beforeLoad: async ({ context }) => {
    const me = await context.queryClient.ensureQueryData(meQuery);
    const heap = me.teams.flatMap((team) => team.heaps).find((h) => !h.archived);
    if (heap) {
      throw redirect({ to: "/heaps/$heapId", params: { heapId: heap.id } });
    }
  },
  component: function NoHeaps() {
    const { data: me } = useMe();
    const team = me && soleTeam(me);
    return (
      <p className="py-16 text-center text-stone-500">
        You don't have any heaps yet.{" "}
        {team ? (
          <>
            <Link to="/teams/$teamId" params={{ teamId: team.id }} className={linkClass}>
              Start one
            </Link>{" "}
            in {team.name}.
          </>
        ) : (
          <>
            <Link to="/teams" className={linkClass}>
              Create a team
            </Link>{" "}
            to start one.
          </>
        )}
      </p>
    );
  },
});

const heapRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/heaps/$heapId",
  validateSearch: parseListSearch,
  component: function HeapRoute() {
    const { heapId } = heapRoute.useParams();
    // The item route is a child, so the list stays mounted, and keeps its place, while an item opens over it.
    const { itemId } = useParams({ strict: false });
    const view = heapRoute.useSearch();
    const navigate = heapRoute.useNavigate();
    // A new key per heap resets the session-stable ranking time. View changes replace the history entry, so Back
    // leaves the list instead of stepping through filter clicks.
    return (
      <HeapPage
        key={heapId}
        heapId={heapId}
        itemId={itemId}
        view={view}
        onViewChange={(next) => void navigate({ search: parseListSearch(next), replace: true })}
      />
    );
  },
});

/** An item, open over its heap's list (app-ux.md, The item). `HeapPage` shows it. */
const itemRoute = createRoute({ getParentRoute: () => heapRoute, path: "items/$itemId" });

const closedRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/heaps/$heapId/closed",
  component: function ClosedRoute() {
    const { heapId } = closedRoute.useParams();
    return <ClosedPage heapId={heapId} />;
  },
});

const tagsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/heaps/$heapId/tags",
  component: function TagsRoute() {
    const { heapId } = tagsRoute.useParams();
    return <TagsPage heapId={heapId} />;
  },
});

const parametersRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/heaps/$heapId/parameters",
  component: function ParametersRoute() {
    const { heapId } = parametersRoute.useParams();
    return <ParametersPage heapId={heapId} />;
  },
});

const heapActivityRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/heaps/$heapId/activity",
  component: function HeapActivityRoute() {
    const { heapId } = heapActivityRoute.useParams();
    return <HeapActivityPage key={heapId} heapId={heapId} />;
  },
});

const teamsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/teams",
  // Without team creation and with one team, as in the local build, the list would hold only that team.
  beforeLoad: async ({ context }) => {
    const team = soleTeam(await context.queryClient.ensureQueryData(meQuery));
    if (team) {
      throw redirect({ to: "/teams/$teamId", params: { teamId: team.id }, replace: true });
    }
  },
  component: TeamsPage,
});

const teamRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/teams/$teamId",
  component: function TeamRoute() {
    const { teamId } = teamRoute.useParams();
    return <TeamPage key={teamId} teamId={teamId} />;
  },
});

const teamActivityRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/teams/$teamId/activity",
  component: function TeamActivityRoute() {
    const { teamId } = teamActivityRoute.useParams();
    return <TeamActivityPage key={teamId} teamId={teamId} />;
  },
});

const accountRoute = createRoute({ getParentRoute: () => appRoute, path: "/account", component: AccountPage });

const routeTree = rootRoute.addChildren([
  loginRoute,
  appRoute.addChildren([
    indexRoute,
    heapRoute.addChildren([itemRoute]),
    closedRoute,
    tagsRoute,
    parametersRoute,
    heapActivityRoute,
    teamsRoute,
    teamRoute,
    teamActivityRoute,
    accountRoute,
  ]),
]);

export function createAppRouter(queryClient: QueryClient) {
  // The local build may be served below the root, such as `/upheap/` on GitHub Pages.
  return createRouter({
    routeTree,
    context: { queryClient },
    defaultPreload: "intent",
    basepath: import.meta.env.BASE_URL,
  });
}

declare module "@tanstack/react-router" {
  interface Register {
    router: ReturnType<typeof createAppRouter>;
  }
}

declare module "@tanstack/history" {
  interface HistoryState {
    /** Set on the way into an item from a page of the app, so closing the item goes back there. */
    closesBack?: boolean;
  }
}
