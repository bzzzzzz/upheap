// The URL is the state: the heap and the page live in the route, so links and reloads work. Paths name teams, heaps
// and items by their handles (design/architecture.md, Handles), and `/h/…` short links expand to them.
import type { QueryClient } from "@tanstack/react-query";
import {
  createRootRouteWithContext,
  createRoute,
  createRouter,
  Link,
  Outlet,
  redirect,
  useLocation,
  useParams,
  useRouter,
} from "@tanstack/react-router";

import { isUnauthenticated, type AuthorizationRequest } from "./api/client";
import { meQuery, useAuthorizationCheck, useHeap, useSignOut } from "./api/queries";
import { BASE } from "./basePath";
import { parseListSearch } from "./components/listView";
import { AccountPage } from "./pages/AccountPage";
import { HeapActivityPage, TeamActivityPage } from "./pages/ActivityPage";
import { AppLayout } from "./pages/AppLayout";
import { AuthorizePage } from "./pages/AuthorizePage";
import { AuthPage } from "./pages/AuthPage";
import { ClosedPage } from "./pages/ClosedPage";
import { HeapPage } from "./pages/HeapPage";
import { HeapContextSettings, HeapGeneralSettings, HeapSettingsLayout } from "./pages/HeapSettingsPage";
import { InvitePage } from "./pages/InvitePage";
import { ParametersPage } from "./pages/ParametersPage";
import { ProviderStepPage } from "./pages/ProviderStepPage";
import { HeapRankingSettings } from "./pages/RankingPage";
import { ResetPage } from "./pages/ResetPage";
import { TagsPage } from "./pages/TagsPage";
import { TeamPage } from "./pages/TeamPage";
import { TeamsPage } from "./pages/TeamsPage";
import { WaitingPage } from "./pages/WaitingPage";

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
 * `href`, the router's address of the page the browser shows, as the browser has it, when the browser does show
 * that page. The router reads each query value as JSON and writes it back, so `state=1e3` comes back as
 * `state=1000` and `state="abc"` as `state=abc`. An OAuth client's `state` has to come back exactly as it sent it.
 * While a navigation is pending, the browser still shows the page it's leaving, so `href` stays as it is.
 */
export function pageHref(href: string): string {
  const { pathname, search, hash } = window.location;
  const path = pathname.startsWith(BASE) ? pathname.slice(BASE.length) : pathname;
  const routerPath = href.split(/[?#]/)[0];
  return path === routerPath ? path + search + hash : href;
}

/** Whether `pathname` is sign-in's, by the router's matching, so `/LOGIN` counts too. */
export function isSignInPath(router: ReturnType<typeof useRouter>, pathname: string): boolean {
  return router.matchRoutes(pathname, {}).some((match) => match.routeId === loginRoute.id);
}

/**
 * Whether `pathname` is a page a link opens, an invite link's or a password reset link's, which handles its own
 * sign-in: signed out is one of the invite page's states, and the reset page signs the visitor in as the link's user
 * whoever they are, so a `401` there doesn't send the user to sign-in.
 */
export function handlesOwnSignIn(router: ReturnType<typeof useRouter>, pathname: string): boolean {
  return router
    .matchRoutes(pathname, {})
    .some((match) => match.routeId === inviteRoute.id || match.routeId === resetRoute.id);
}

/**
 * An item key as `domain::handle::ItemKey` makes them: five characters of lowercase Crockford base32, at least one a
 * digit. The notice names an item only when its segment has this shape, so a crafted link can't put its own words
 * there.
 */
const ITEM_KEY = /^(?=.*\d)[0-9a-hjkmnp-tv-z]{5}$/;

/**
 * `redirect` as a page of the app to return to after sign-in, with the router's matches for it and its query, or
 * nothing when it isn't one. A crafted link such as `?redirect=//evil.example` mustn't send a user who signs in to
 * another site, and sign-in itself, or any other path outside the signed-in app, isn't worth returning to; the one
 * page outside it that is, is connecting a client such as the CLI. A path inside it that no page answers, such as
 * `/teams/a/zzz`, still matches loosely and opens the app's not-found, which is harmless. An invite link's page is
 * one too: a sign-in that started on it comes back to it.
 */
function toAppPage(router: ReturnType<typeof useRouter>, redirect: string | undefined) {
  if (!redirect?.startsWith("/") || redirect.startsWith("//") || redirect.startsWith("/\\")) return undefined;
  // The URL parser drops tabs and newlines and resolves dot segments, so `/\t/evil.example` is another origin and
  // `/.//evil.example` comes out as `//evil.example`. Check what it gives, not what came in.
  const url = new URL(redirect, window.location.origin);
  if (url.origin !== window.location.origin || url.pathname.startsWith("//")) return undefined;
  // Only the signed-in app, connecting a client and an invite link send users here; sign-in is outside all of them,
  // whatever the case of `/LOGIN`.
  const matches = router.matchRoutes(url.pathname, {});
  const known = matches.some(
    (match) => match.routeId === appRoute.id || match.routeId === authorizeRoute.id || match.routeId === inviteRoute.id,
  );
  if (!known) return undefined;
  return { href: url.pathname + url.search + url.hash, matches, search: url.searchParams };
}

/**
 * Leaves sign-in for `page`, as `toAppPage` read it from the `redirect` the sign-in started with, or for `/`: the
 * login page's and the username step's way on once signed in.
 */
function goAfterSignIn(router: ReturnType<typeof useRouter>, page: ReturnType<typeof toAppPage>) {
  // The consent page is loaded afresh, so its query arrives exactly as the client wrote it (see `pageHref`).
  if (page?.matches.some((match) => match.routeId === authorizeRoute.id)) window.location.replace(BASE + page.href);
  // With `href`, the router takes the whole location from it; without, it goes to `to`.
  else void router.navigate({ to: "/", href: page?.href, replace: true });
}

/** The OAuth authorization request in a query, its values as they came. The server checks every one. */
function toAuthorizationRequest(search: Record<string, unknown>): AuthorizationRequest {
  const keys = [
    "response_type",
    "client_id",
    "redirect_uri",
    "code_challenge",
    "code_challenge_method",
    "state",
  ] as const;
  const request: AuthorizationRequest = {};
  for (const key of keys) {
    const value = search[key];
    if (typeof value === "string") request[key] = value;
  }
  return request;
}

const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/login",
  // `sign_in_error` and `login` come from the server's callback after a refused sign-in through the provider.
  validateSearch: (search: Record<string, unknown>): { redirect?: string; sign_in_error?: string; login?: string } => ({
    ...(typeof search.redirect === "string" && { redirect: search.redirect }),
    ...(typeof search.sign_in_error === "string" && { sign_in_error: search.sign_in_error }),
    ...(typeof search.login === "string" && { login: search.login }),
  }),
  component: function LoginRoute() {
    const router = useRouter();
    const { redirect, sign_in_error: refusal, login } = loginRoute.useSearch();
    const page = toAppPage(router, redirect);
    // The router's own matching says whether the link is to an item, and which.
    const item = page?.matches.find((match) => match.routeId === itemRoute.id);
    const segment = item && "item" in item.params ? String(item.params.item) : undefined;
    const itemKey = segment && ITEM_KEY.test(segment) ? segment : undefined;
    // Connecting a client names it as the server does, once the server has checked the request.
    const connecting = page?.matches.some((match) => match.routeId === authorizeRoute.id) ?? false;
    const client = useAuthorizationCheck(toAuthorizationRequest(Object.fromEntries(page?.search ?? [])), {
      enabled: connecting,
    }).data;
    let notice: string | undefined;
    if (connecting) notice = `Sign in to connect ${client?.display_name ?? "the app"}.`;
    else if (page) notice = itemKey ? `Sign in to open the link to ${itemKey}.` : "Sign in to open that page.";
    return (
      <AuthPage
        notice={notice}
        providerRedirect={page?.href}
        refusal={refusal}
        initialLogin={login}
        onSignedIn={() => goAfterSignIn(router, page)}
      />
    );
  },
});

/**
 * The username step of a first sign-in through the provider, outside the app's shell like sign-in. It has no guard:
 * the browser holds no session yet, and the page makes no request that a `401` could answer.
 */
const providerStepRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/login/provider",
  component: function ProviderStepRoute() {
    const router = useRouter();
    return <ProviderStepPage onSignedIn={(redirect) => goAfterSignIn(router, toAppPage(router, redirect))} />;
  },
});

/**
 * The page an invite link opens (ux.md, Invite page), outside the app's shell like sign-in. It has no guard: signed
 * out is a normal state of it, and the page decides what to show.
 */
const inviteRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/invite/$token",
  component: function InviteRoute() {
    return <InvitePage token={inviteRoute.useParams().token} />;
  },
});

/**
 * The page a password reset link opens (ux.md, Reset page), outside the app's shell like sign-in. It has no guard:
 * holding the link is the permission, whether signed in or not.
 */
const resetRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/reset/$token",
  component: function ResetRoute() {
    return <ResetPage token={resetRoute.useParams().token} />;
  },
});

/**
 * Connecting a client such as the CLI to the user's account (OAuth's consent page), outside the app's shell like
 * sign-in. A signed-out visit signs in first and comes back.
 */
const authorizeRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/oauth/authorize",
  beforeLoad: async ({ context, location }) => {
    try {
      await context.queryClient.ensureQueryData(meQuery);
    } catch (error) {
      if (isUnauthenticated(error)) throw redirect({ to: "/login", search: signInSearch(pageHref(location.href)) });
      throw error;
    }
  },
  component: function AuthorizeRoute() {
    const router = useRouter();
    const signOut = useSignOut();
    // Re-rendered on every change of the address, but read from the browser's, as the router's isn't exact.
    useLocation();
    const request = toAuthorizationRequest(Object.fromEntries(new URLSearchParams(window.location.search)));
    // Signing out drops the whole cache while this page's queries still watch it, so sign-in is loaded afresh
    // rather than reached in place, as the way back here is.
    const toSignIn = `${BASE}/login?${new URLSearchParams(signInSearch(pageHref(router.state.location.href))).toString()}`;
    return (
      <AuthorizePage
        request={request}
        onUseAnotherAccount={() => signOut.mutate(undefined, { onSuccess: () => window.location.replace(toSignIn) })}
      />
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
  beforeLoad: async ({ context, location }) => {
    const me = await context.queryClient.ensureQueryData(meQuery);
    for (const team of me.teams) {
      const heap = team.heaps.find((h) => !h.archived);
      if (heap) {
        // The state goes along, so the line the reset page lands with (`notice`) reaches the heap.
        throw redirect({
          to: "/teams/$team/heaps/$heap",
          params: { team: team.slug, heap: heap.slug },
          state: location.state,
        });
      }
    }
  },
  component: function NoHeaps() {
    return (
      <p className="py-16 text-center text-stone-500">
        You don't have any heaps yet.{" "}
        <Link to="/teams" className={linkClass}>
          Create a team
        </Link>{" "}
        to start one.
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

/** An item, by its key, open over its heap's list (ux.md, The item). `HeapPage` shows it. */
const itemRoute = createRoute({ getParentRoute: () => heapRoute, path: "items/$item" });

const closedRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/teams/$team/heaps/$heap/closed",
  component: function ClosedRoute() {
    const { team, heap } = closedRoute.useParams();
    return <ClosedPage heap={{ team, heap }} />;
  },
});

/** The heap's settings, a tab per child route (ux.md, Heap settings). */
const heapSettingsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/teams/$team/heaps/$heap/settings",
  component: function HeapSettingsRoute() {
    const { team, heap } = heapSettingsRoute.useParams();
    const record = useHeap({ team, heap });
    // Another heap starts afresh, but a new link name for this one keeps the page as it is, with focus where it was.
    // So it's keyed by the heap's ID, and waits for it rather than rendering once under another key. A new link
    // name's heap is already cached under it, so it doesn't load again.
    // A failed refetch keeps the heap it had, so the page stays; the error shows only when there's no heap to show.
    if (record.data) return <HeapSettingsLayout key={record.data.id} heap={{ team, heap }} />;
    if (record.isError) {
      return (
        <p role="alert" className="py-16 text-center text-overdue-ink">
          {record.error.message}
        </p>
      );
    }
    return <p className="py-16 text-center text-muted">Loading…</p>;
  },
});

const heapGeneralSettingsRoute = createRoute({
  getParentRoute: () => heapSettingsRoute,
  path: "/",
  component: function HeapGeneralSettingsRoute() {
    const { team, heap } = heapGeneralSettingsRoute.useParams();
    return <HeapGeneralSettings heap={{ team, heap }} />;
  },
});

const heapContextSettingsRoute = createRoute({
  getParentRoute: () => heapSettingsRoute,
  path: "context",
  component: function HeapContextSettingsRoute() {
    const { team, heap } = heapContextSettingsRoute.useParams();
    return <HeapContextSettings heap={{ team, heap }} />;
  },
});

const tagsRoute = createRoute({
  getParentRoute: () => heapSettingsRoute,
  path: "tags",
  component: function TagsRoute() {
    const { team, heap } = tagsRoute.useParams();
    return <TagsPage heap={{ team, heap }} />;
  },
});

const parametersRoute = createRoute({
  getParentRoute: () => heapSettingsRoute,
  path: "parameters",
  component: function ParametersRoute() {
    const { team, heap } = parametersRoute.useParams();
    return <ParametersPage heap={{ team, heap }} />;
  },
});

const rankingRoute = createRoute({
  getParentRoute: () => heapSettingsRoute,
  path: "ranking",
  component: function RankingRoute() {
    const { team, heap } = rankingRoute.useParams();
    return <HeapRankingSettings heap={{ team, heap }} />;
  },
});

// Tags and Parameters were the heap's own pages before they became tabs of its settings; links to them still work.
const oldTagsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/teams/$team/heaps/$heap/tags",
  beforeLoad: ({ params }) => {
    throw redirect({ to: "/teams/$team/heaps/$heap/settings/tags", params, replace: true });
  },
});

const oldParametersRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/teams/$team/heaps/$heap/parameters",
  beforeLoad: ({ params }) => {
    throw redirect({ to: "/teams/$team/heaps/$heap/settings/parameters", params, replace: true });
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

// Short links (design/architecture.md, Handles): the handles alone, in fixed positions, swapped for the full path as they open.
// The router does it, so the server needs no route for them.
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

/** Every open question across the heaps (ux.md, Waiting for me). */
const waitingRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/waiting",
  component: WaitingPage,
});

const accountRoute = createRoute({ getParentRoute: () => appRoute, path: "/account", component: AccountPage });

const routeTree = rootRoute.addChildren([
  loginRoute,
  providerStepRoute,
  inviteRoute,
  resetRoute,
  authorizeRoute,
  shortTeamRoute,
  shortHeapRoute,
  shortItemRoute,
  appRoute.addChildren([
    indexRoute,
    heapRoute.addChildren([itemRoute]),
    closedRoute,
    heapSettingsRoute.addChildren([
      heapGeneralSettingsRoute,
      heapContextSettingsRoute,
      tagsRoute,
      parametersRoute,
      rankingRoute,
    ]),
    oldTagsRoute,
    oldParametersRoute,
    heapActivityRoute,
    teamsRoute,
    teamRoute,
    teamActivityRoute,
    waitingRoute,
    accountRoute,
  ]),
]);

export function createAppRouter(queryClient: QueryClient) {
  // Vite's base, which is the root unless the app is built for another.
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
    /** Set on each click of an agent's mark in a history, so a second click on the same run is a new location. */
    runLinkedAt?: number;
    /**
     * Set on the way to where the user lands after the invite or reset page, holding the line the shell shows once:
     * "You joined Acme." or "Password changed. You're signed in."
     */
    notice?: string;
  }
}
