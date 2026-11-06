// The URL is the state: the heap and the page live in the route, so links and reloads work.
import type { QueryClient } from "@tanstack/react-query";
import { createRootRouteWithContext, createRoute, createRouter, Link, Outlet, redirect } from "@tanstack/react-router";

import { isUnauthenticated } from "./api/client";
import { meQuery } from "./api/queries";
import { parseListSearch } from "./components/listView";
import { AccountPage } from "./pages/AccountPage";
import { AppLayout } from "./pages/AppLayout";
import { AuthPage } from "./pages/AuthPage";
import { ClosedPage } from "./pages/ClosedPage";
import { EditItemPage } from "./pages/EditItemPage";
import { HeapPage } from "./pages/HeapPage";
import { TagsPage } from "./pages/TagsPage";
import { TeamPage } from "./pages/TeamPage";
import { TeamsPage } from "./pages/TeamsPage";

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
  component: () => (
    <p className="py-16 text-center text-stone-500">
      You don't have any heaps yet.{" "}
      <Link to="/teams" className="text-sky-700 hover:underline dark:text-sky-400">
        Create a team
      </Link>{" "}
      to start one.
    </p>
  ),
});

const heapRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/heaps/$heapId",
  validateSearch: parseListSearch,
  component: function HeapRoute() {
    const { heapId } = heapRoute.useParams();
    const view = heapRoute.useSearch();
    const navigate = heapRoute.useNavigate();
    // A new key per heap resets the session-stable ranking time. View changes replace the history entry, so Back
    // leaves the list instead of stepping through filter clicks.
    return (
      <HeapPage
        key={heapId}
        heapId={heapId}
        view={view}
        onViewChange={(next) => void navigate({ search: parseListSearch(next), replace: true })}
      />
    );
  },
});

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

const editItemRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/heaps/$heapId/items/$itemId",
  component: function EditItemRoute() {
    const { heapId, itemId } = editItemRoute.useParams();
    return <EditItemPage key={itemId} heapId={heapId} itemId={itemId} />;
  },
});

const teamsRoute = createRoute({ getParentRoute: () => appRoute, path: "/teams", component: TeamsPage });

const teamRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/teams/$teamId",
  component: function TeamRoute() {
    const { teamId } = teamRoute.useParams();
    return <TeamPage key={teamId} teamId={teamId} />;
  },
});

const accountRoute = createRoute({ getParentRoute: () => appRoute, path: "/account", component: AccountPage });

const routeTree = rootRoute.addChildren([
  loginRoute,
  appRoute.addChildren([
    indexRoute,
    heapRoute,
    closedRoute,
    tagsRoute,
    editItemRoute,
    teamsRoute,
    teamRoute,
    accountRoute,
  ]),
]);

export function createAppRouter(queryClient: QueryClient) {
  return createRouter({ routeTree, context: { queryClient }, defaultPreload: "intent" });
}

declare module "@tanstack/react-router" {
  interface Register {
    router: ReturnType<typeof createAppRouter>;
  }
}
