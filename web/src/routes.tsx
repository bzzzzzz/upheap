// The URL is the state: the heap and the page live in the route, so links and reloads work. Paths name teams, heaps
// and items by their handles (design/readable-ids.md), and `/h/…` short links expand to them.
import type { QueryClient } from "@tanstack/react-query";
import {
  createRootRouteWithContext,
  createRoute,
  createRouter,
  Link,
  Outlet,
  redirect,
  useParams,
  useRouter,
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

/**
 * Sign-in's search for a user sent there from `href`, a path in the app with its search and hash, so signing in
 * returns there. Nothing for `/`, where signing in lands anyway.
 */
export function signInSearch(href: string): { redirect?: string } {
  return href === "/" ? {} : { redirect: href };
}

/**
 * `redirect` as a path in this app, or nothing when it isn't one: a crafted link such as `?redirect=//evil.example`
 * mustn't send a user who signs in to another site. Sign-in itself doesn't count, so it can't loop.
 */
function toInAppPath(redirect: string | undefined): string | undefined {
  if (!redirect?.startsWith("/") || redirect.startsWith("//") || redirect.startsWith("/\\")) return undefined;
  // The URL parser drops tabs and newlines, so `/\t/evil.example` is another origin too.
  const url = new URL(redirect, window.location.origin);
  if (url.origin !== window.location.origin || url.pathname.replace(/\/+$/, "") === "/login") return undefined;
  return url.pathname + url.search + url.hash;
}

const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/login",
  validateSearch: (search: Record<string, unknown>): { redirect?: string } =>
    typeof search.redirect === "string" ? { redirect: search.redirect } : {},
  component: function LoginRoute() {
    const target = toInAppPath(loginRoute.useSearch().redirect);
    const router = useRouter();
    let notice: string | undefined;
    if (target) {
      // The router's own matching says whether the link is to an item, and which.
      const [, params, route] = router.getMatchedRoutes(new URL(target, window.location.origin).pathname);
      const isItem = route?.id === itemRoute.id || route?.id === shortItemRoute.id;
      notice = isItem && params.item ? `Sign in to open the link to ${params.item}.` : "Sign in to open that page.";
    }
    // With `href`, the router takes the whole location from it; without, it goes to `to`.
    return (
      <AuthPage notice={notice} onSignedIn={() => void router.navigate({ to: "/", href: target, replace: true })} />
    );
  },
});

const appRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: "app",
  beforeLoad: async ({ context, location }) => {
    try {
      await context.queryClient.ensureQueryData(meQuery);
    } catch (error) {
      if (isUnauthenticated(error)) {
        // `href` is the router's, without the base path, as `redirect` wants it.
        throw redirect({ to: "/login", search: signInSearch(location.href) });
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
    for (const team of me.teams) {
      const heap = team.heaps.find((h) => !h.archived);
      if (heap) {
        throw redirect({ to: "/teams/$team/heaps/$heap", params: { team: team.slug, heap: heap.slug } });
      }
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
            <Link to="/teams/$team" params={{ team: team.slug }} className={linkClass}>
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
  path: "/teams/$team/heaps/$heap",
  validateSearch: parseListSearch,
  component: function HeapRoute() {
    const { team, heap } = heapRoute.useParams();
    // The item route is a child, so the list stays mounted, and keeps its place, while an item opens over it.
    const { item } = useParams({ strict: false });
    const view = heapRoute.useSearch();
    const navigate = heapRoute.useNavigate();
    // A new key per heap resets the session-stable ranking time. View changes replace the history entry, so Back
    // leaves the list instead of stepping through filter clicks.
    return (
      <HeapPage
        key={`${team}/${heap}`}
        heap={{ team, heap }}
        itemKey={item}
        view={view}
        onViewChange={(next) => void navigate({ search: parseListSearch(next), replace: true })}
      />
    );
  },
});

/** An item, by its key, open over its heap's list (app-ux.md, The item). `HeapPage` shows it. */
const itemRoute = createRoute({ getParentRoute: () => heapRoute, path: "items/$item" });

const closedRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/teams/$team/heaps/$heap/closed",
  component: function ClosedRoute() {
    const { team, heap } = closedRoute.useParams();
    return <ClosedPage heap={{ team, heap }} />;
  },
});

const tagsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/teams/$team/heaps/$heap/tags",
  component: function TagsRoute() {
    const { team, heap } = tagsRoute.useParams();
    return <TagsPage heap={{ team, heap }} />;
  },
});

const parametersRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/teams/$team/heaps/$heap/parameters",
  component: function ParametersRoute() {
    const { team, heap } = parametersRoute.useParams();
    return <ParametersPage heap={{ team, heap }} />;
  },
});

const heapActivityRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/teams/$team/heaps/$heap/activity",
  component: function HeapActivityRoute() {
    const { team, heap } = heapActivityRoute.useParams();
    return <HeapActivityPage key={`${team}/${heap}`} heap={{ team, heap }} />;
  },
});

const teamsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/teams",
  // Without team creation and with one team, as in the local build, the list would hold only that team.
  beforeLoad: async ({ context }) => {
    const team = soleTeam(await context.queryClient.ensureQueryData(meQuery));
    if (team) {
      throw redirect({ to: "/teams/$team", params: { team: team.slug }, replace: true });
    }
  },
  component: TeamsPage,
});

const teamRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/teams/$team",
  component: function TeamRoute() {
    const { team } = teamRoute.useParams();
    return <TeamPage key={team} team={team} />;
  },
});

const teamActivityRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/teams/$team/activity",
  component: function TeamActivityRoute() {
    const { team } = teamActivityRoute.useParams();
    return <TeamActivityPage key={team} team={team} />;
  },
});

// Short links (design/readable-ids.md): the handles alone, in fixed positions, swapped for the full path as they open.
// The router does it, so they work the same in the local build, which has no server to redirect.
const shortTeamRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/h/$team",
  beforeLoad: ({ params }) => {
    throw redirect({ to: "/teams/$team", params, replace: true });
  },
});

const shortHeapRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/h/$team/$heap",
  beforeLoad: ({ params }) => {
    throw redirect({ to: "/teams/$team/heaps/$heap", params, replace: true });
  },
});

const shortItemRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/h/$team/$heap/$item",
  beforeLoad: ({ params }) => {
    throw redirect({ to: "/teams/$team/heaps/$heap/items/$item", params, replace: true });
  },
});

const accountRoute = createRoute({ getParentRoute: () => appRoute, path: "/account", component: AccountPage });

const routeTree = rootRoute.addChildren([
  loginRoute,
  shortTeamRoute,
  shortHeapRoute,
  shortItemRoute,
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
