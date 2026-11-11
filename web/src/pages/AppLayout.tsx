// The app shell at each width (design/ux.md, Layout): a sidebar on a wide screen, a rail on a tablet, and on a
// phone the heap name at the top with tabs at the bottom. Each part shows at its own widths through CSS alone, so
// the layout never waits for script to measure the screen.
import { localDatabase } from "#local-database";
import {
  Link,
  Outlet,
  useMatch,
  useParams,
  useRouter,
  useRouterState,
  useSearch,
  type LinkProps,
} from "@tanstack/react-router";
import {
  Activity,
  Building2,
  ChevronDown,
  CircleCheck,
  Ellipsis,
  Inbox,
  List,
  Plus,
  Settings,
  User,
  Users,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";

import type { HeapRef, Me } from "../api/client";
import { soleTeam, useHeap, useMe, useSetHeapArchived, useWaiting } from "../api/queries";
import { AddItemDialog } from "../components/AddItem";
import { readRecentHeaps, recordRecentHeap } from "../components/heapSwitcherStorage";
import { HeapSwitcher } from "../components/HeapSwitcher";
import { useListSearch, ListSearchProvider } from "../components/listSearch";
import { StorageNote } from "../components/LocalDatabase";
import { Logo } from "../components/Logo";
import { useKeyboardInset, usePhoneTabsHeight } from "../components/media";
import { ProblemText } from "../components/ProblemText";
import { Sheet, sheetRowClass } from "../components/Sheet";
import { teamName, titleTeam } from "../components/teamName";
import { ToastProvider, useToast } from "../components/UndoToast";

export function AppLayout() {
  const { data: me } = useMe();
  useKeyboardInset();
  const { team, heap } = useParams({ strict: false });
  const heapRef = team !== undefined && heap !== undefined ? { team, heap } : undefined;
  // The heap's list, also with an item open over it.
  const onList = useMatch({ from: "/app/teams/$team/heaps/$heap", shouldThrow: false }) !== undefined;
  // The heap on screen counts as opened once per visit, for the switcher's Recent.
  // Pages with no heap in their address keep the heap opened last, where Waiting for me needs the heap's views.
  const onWaiting = useMatch({ from: "/app/waiting", shouldThrow: false }) !== undefined;
  const shellHeap = heapRef ?? (onWaiting && me ? keptHeap(me) : undefined);
  const heapId = me?.teams.find((t) => t.slug === heapRef?.team)?.heaps.find((h) => h.slug === heapRef?.heap)?.id;
  useEffect(() => {
    if (heapId !== undefined) recordRecentHeap(heapId);
  }, [heapId]);

  return (
    // On a wide screen the shell is the sidebar plus the content column (15rem + 48rem + padding), centred, so the
    // menu stays next to what it opens. The left half behind it takes the sidebar's colour, so the sidebar reaches
    // the edge of the screen. An open item pane (`data-item-pane`) widens the block by its own width and takes the
    // right-hand padding.
    <ToastProvider>
      <JoinedLine />
      <ListSearchProvider listKey={onList && heapRef ? `${heapRef.team}/${heapRef.heap}` : undefined}>
        <div className="lg:bg-[linear-gradient(to_right,var(--sunken)_50%,var(--canvas)_50%)]">
          <div className="min-h-dvh sm:flex lg:mx-auto lg:max-w-[66rem] lg:has-[[data-item-pane]]:max-w-[92rem]">
            {me && <Sidebar me={me} heapRef={shellHeap} />}
            <div className="min-w-0 flex-1 bg-canvas">
              {me && <PhoneTopBar me={me} heapRef={shellHeap} />}
              {/* On a phone, room at the bottom for the tabs. */}
              <div className="mx-auto max-w-3xl px-4 pb-[calc(6rem+env(safe-area-inset-bottom))] sm:px-6 sm:pb-24 lg:has-[[data-item-pane]]:max-w-none lg:has-[[data-item-pane]]:pr-0">
                {heapRef && <ArchivedNotice key={`${heapRef.team}/${heapRef.heap}`} heapRef={heapRef} />}
                <Outlet />
                <div className="lg:hidden">
                  <StorageNote className="mt-16 border-t border-line pt-4" />
                </div>
              </div>
            </div>
            {me && shellHeap && <PhoneTabs me={me} heapRef={shellHeap} />}
          </div>
        </div>
      </ListSearchProvider>
    </ToastProvider>
  );
}

/**
 * The heap the shell keeps on a page without one in its address: the one opened last that is still there and active,
 * else the first active heap, else the first heap.
 */
function keptHeap(me: Me): HeapRef | undefined {
  const entries = me.teams.flatMap((team) => team.heaps.map((heap) => ({ team, heap })));
  const kept =
    readRecentHeaps()
      .map((id) => entries.find((entry) => entry.heap.id === id && !entry.heap.archived))
      .find((entry) => entry !== undefined) ??
    entries.find((entry) => !entry.heap.archived) ??
    entries[0];
  return kept && { team: kept.team.slug, heap: kept.heap.slug };
}

/**
 * The line a new member lands with ("You joined Acme.", from the invite page; ux.md, Invite page), shown once as a toast
 * on the page they land on. The toast belongs to the page it's shown on, so it goes on the next navigation. It is taken
 * out of the history entry as it is shown, since the browser keeps that across a reload.
 */
function JoinedLine() {
  const router = useRouter();
  const show = useToast();
  const joined = useRouterState({ select: (state) => state.location.state.joined });
  useEffect(() => {
    if (joined === undefined) return;
    show({ message: joined });
    void router.navigate({
      to: ".",
      params: true,
      search: true,
      hash: true,
      replace: true,
      state: (previous) => ({ ...previous, joined: undefined }),
    });
  }, [joined, show, router]);
  return null;
}

/** Where the shell's links go, for the signed-in user, the heap on screen and the team in the URL. */
function useShellLinks(me: Me, heapRef: HeapRef | undefined) {
  const { team: teamSlug } = useParams({ strict: false });
  const heapTeam = me.teams.find((team) => team.slug === heapRef?.team);
  const urlTeam = me.teams.find((team) => team.slug === teamSlug);
  // With one team and no Teams list, as in the local build, the team link goes straight to it, so a link to the
  // team in the URL would repeat it.
  const onlyTeam = soleTeam(me);
  return {
    heap: heapTeam?.heaps.find((heap) => heap.slug === heapRef?.heap),
    team: onlyTeam || !urlTeam ? undefined : { slug: urlTeam.slug, label: teamName(urlTeam, me) },
    teams: onlyTeam
      ? ({ to: "/teams/$team", params: { team: onlyTeam.slug }, label: teamName(onlyTeam, me) } as const)
      : ({ to: "/teams", label: "Teams" } as const),
  };
}

// The shared look of a navigation entry: icon over a short label in the rail, icon beside the label in the sidebar,
// and at least 44 px tall everywhere. Links get `data-status="active"` from the router. The active entry is an amber
// pill, as the selected row of the list is amber: a surface-coloured fill vanished on the phone's tabs, which are the
// surface, and barely showed on the dark sidebar. A tint of the accent rather than `accent-soft`, which is too close to
// white to show on the tabs and the sidebar in light mode. Hovering it keeps the pill, so hover and active stay apart.
const navItemClass =
  "flex min-h-11 flex-col items-center justify-center gap-1 rounded-xl py-1.5 text-[11px] font-medium text-ink-soft hover:bg-line/60 hover:text-ink data-[status=active]:bg-accent/20 data-[status=active]:text-accent-ink data-[status=active]:hover:bg-accent/20 data-[status=active]:hover:text-accent-ink lg:flex-row lg:justify-start lg:gap-3 lg:px-3 lg:text-sm";

/**
 * With `current`, the link shows as active also where the router wouldn't say so. `count`, such as the heap's open
 * items, shows on its right in the sidebar only; it stays out of the link's name, which is the view's.
 */
function NavLink({
  icon: Icon,
  label,
  current,
  count,
  ...link
}: LinkProps & { icon: LucideIcon; label: string; current?: boolean; count?: number }) {
  return (
    <Link {...link} {...(current && { "data-status": "active", "aria-current": "page" })} className={navItemClass}>
      <Icon aria-hidden="true" className="size-5 shrink-0 lg:size-4" />
      <span className="max-w-full truncate">{label}</span>
      {count !== undefined && (
        <span aria-hidden="true" className="ml-auto hidden font-normal text-muted tabular-nums lg:inline">
          {count}
        </span>
      )}
    </Link>
  );
}

/**
 * Waiting for me, with the number of questions that wait. The count is a badge on the icon in the rail and the tabs, and
 * on the right in the sidebar; it is part of the link's name, and there is none at zero. Not in the local build.
 */
function WaitingLink() {
  const count = useWaiting().data?.count ?? 0;
  return (
    <Link to="/waiting" className={`${navItemClass} relative`}>
      <Inbox aria-hidden="true" className="size-5 shrink-0 lg:size-4" />
      <span className="max-w-full truncate">
        <span className="lg:hidden">Waiting</span>
        <span className="max-lg:hidden">Waiting for me</span>
      </span>
      {count > 0 && (
        <span className="absolute top-0.5 left-1/2 ml-1 min-w-4 rounded-full bg-accent-ink px-1 text-center text-[10px] leading-4 font-semibold text-canvas tabular-nums lg:static lg:ml-auto lg:min-w-0 lg:bg-transparent lg:px-0 lg:text-sm lg:leading-normal lg:font-normal lg:text-muted">
          {count}
        </span>
      )}
    </Link>
  );
}

/** The heap's views, from both the sidebar and the phone's tabs. An open item sits over the open list. */
function heapViews(heapRef: HeapRef, itemOpen: boolean) {
  return {
    open: {
      to: "/teams/$team/heaps/$heap",
      params: heapRef,
      activeOptions: { exact: true, includeSearch: false },
      current: itemOpen,
    },
    closed: { to: "/teams/$team/heaps/$heap/closed", params: heapRef },
    activity: { to: "/teams/$team/heaps/$heap/activity", params: heapRef },
    // Active on each of its tabs too.
    settings: { to: "/teams/$team/heaps/$heap/settings", params: heapRef },
  } as const;
}

/** The sidebar on a wide screen, folded into a rail on a tablet; hidden on a phone. */
function Sidebar({ me, heapRef }: { me: Me; heapRef: HeapRef | undefined }) {
  const links = useShellLinks(me, heapRef);
  const { item: itemKey } = useParams({ strict: false });
  const views = heapRef && heapViews(heapRef, itemKey !== undefined);
  const heapTeam = me.teams.find((team) => team.slug === heapRef?.team);
  const railTeam = titleTeam(me, heapRef);

  return (
    <aside
      aria-label="Heaps and views"
      className="sticky top-0 hidden h-dvh w-20 shrink-0 flex-col gap-5 overflow-y-auto border-r border-line bg-sunken pt-[max(env(safe-area-inset-top),1rem)] pb-[max(env(safe-area-inset-bottom),1rem)] sm:flex sm:px-1.5 lg:w-60 lg:px-3"
    >
      <div className="hidden items-center gap-2 px-3 font-semibold tracking-tight lg:flex">
        <Logo className="size-5 text-accent" />
        Upheap
      </div>

      {!localDatabase && (
        <div className="lg:border-b lg:border-line lg:pb-4">
          <WaitingLink />
        </div>
      )}

      <div className="lg:hidden">
        <HeapSwitcher me={me} heapRef={heapRef} placement="end top">
          {(props) => (
            <button
              type="button"
              {...props}
              aria-label={`Switch heap${links.heap ? `: ${links.heap.name}${railTeam ? ` · ${railTeam}` : ""}` : ""}`}
              className="mx-auto flex size-12 items-center justify-center rounded-2xl bg-ink text-lg font-semibold text-canvas outline-offset-2 focus-visible:outline-2 focus-visible:outline-accent"
            >
              {(links.heap?.name ?? "Upheap").charAt(0).toUpperCase()}
            </button>
          )}
        </HeapSwitcher>
      </div>

      <div className="hidden lg:block">
        <HeapSwitcher me={me} heapRef={heapRef} placement="bottom start">
          {(props) => (
            <button
              type="button"
              {...props}
              className="flex h-10 w-full items-center gap-2 rounded-[10px] border border-line-strong bg-surface px-3 text-left text-sm outline-offset-2 hover:border-muted focus-visible:outline-2 focus-visible:outline-accent"
            >
              {/* The name is what the button shows, after what it does. */}
              <span className="min-w-0 flex-1 truncate">
                <span className="sr-only">Switch heap: </span>
                {links.heap ? (
                  <>
                    <span className="font-semibold text-ink">{links.heap.name}</span>
                    {links.heap.archived && <span className="text-muted"> (archived)</span>}
                    {heapTeam && <span className="text-muted"> · {teamName(heapTeam, me)}</span>}
                  </>
                ) : (
                  <span className="text-ink-soft">Choose a heap</span>
                )}
              </span>
              <ChevronDown aria-hidden="true" className="size-4 shrink-0 text-muted" />
            </button>
          )}
        </HeapSwitcher>
      </div>

      {views && (
        <nav aria-label="Heap views" className="flex flex-col gap-1 lg:gap-0.5">
          <NavLink {...views.open} icon={List} label="Open" count={links.heap?.open_count} />
          <NavLink {...views.closed} icon={CircleCheck} label="Closed" />
          <NavLink {...views.activity} icon={Activity} label="Activity" />
          <NavLink {...views.settings} icon={Settings} label="Settings" />
        </nav>
      )}

      <div className="mt-auto flex flex-col gap-1 lg:gap-0.5">
        {links.team && (
          <NavLink
            to="/teams/$team"
            params={{ team: links.team.slug }}
            // Not marked from the heap's own pages, which sit under the team's path.
            activeOptions={{ exact: true }}
            icon={Building2}
            label={links.team.label}
          />
        )}
        <NavLink {...links.teams} activeOptions={{ exact: true }} icon={Users} />
        <NavLink to="/account" icon={User} label="Account" />
        <div className="hidden lg:block">
          <StorageNote className="px-2 pt-1.5" />
        </div>
      </div>
    </aside>
  );
}

/** The heap name on a phone, which switches heap. */
function PhoneTopBar({ me, heapRef }: { me: Me; heapRef: HeapRef | undefined }) {
  const { heap } = useShellLinks(me, heapRef);
  const team = titleTeam(me, heapRef);
  return (
    <header className="sticky top-0 z-10 flex items-center bg-canvas/95 px-4 pt-[max(env(safe-area-inset-top),0.5rem)] pb-1 backdrop-blur sm:hidden">
      <HeapSwitcher me={me} heapRef={heapRef}>
        {(props) => (
          <button
            type="button"
            {...props}
            aria-label={`Switch heap${heap ? `: ${heap.name}${team ? ` · ${team}` : ""}` : ""}`}
            className="-ml-1 flex min-h-11 min-w-0 items-center gap-1.5 rounded-lg px-1 text-xl font-semibold tracking-tight outline-none focus-visible:outline-2 focus-visible:outline-accent"
          >
            <span className="truncate">
              {heap?.name ?? "Upheap"}
              {heap && team && <span className="font-normal text-muted"> · {team}</span>}
            </span>
            <ChevronDown aria-hidden="true" className="size-5 shrink-0 text-ink-soft" />
          </button>
        )}
      </HeapSwitcher>
    </header>
  );
}

/**
 * The phone's tabs (ux.md, Layout): Open, Waiting, Add in the middle under the thumb, Closed, and More for the rest,
 * Settings among it. The local build has nothing to wait for, so its tabs are Open, Closed, Add, Settings and More.
 */
function PhoneTabs({ me, heapRef }: { me: Me; heapRef: HeapRef }) {
  const links = useShellLinks(me, heapRef);
  const { item: itemKey } = useParams({ strict: false });
  const views = heapViews(heapRef, itemKey !== undefined);
  const tabs = useRef<HTMLElement>(null);
  usePhoneTabsHeight(tabs);
  const [adding, setAdding] = useState(false);
  // On the list, Add starts from its view; the heap's other pages have none.
  const view = useSearch({ from: "/app/teams/$team/heaps/$heap", shouldThrow: false });
  const { search } = useListSearch();
  const [more, setMore] = useState(false);
  // The heap's pages that only More leads to, so More shows as active on them.
  const onActivity = useMatch({ from: "/app/teams/$team/heaps/$heap/activity", shouldThrow: false }) !== undefined;
  const onSettings = useMatch({ from: "/app/teams/$team/heaps/$heap/settings", shouldThrow: false }) !== undefined;
  const onMorePage = onActivity || (!localDatabase && onSettings);

  return (
    <nav
      ref={tabs}
      aria-label="Heap views"
      className="fixed inset-x-0 bottom-0 z-10 grid grid-cols-5 items-end gap-1 border-t border-line bg-surface px-2 pt-1.5 pb-[max(env(safe-area-inset-bottom),0.375rem)] sm:hidden"
    >
      <NavLink {...views.open} icon={List} label="Open" />
      {localDatabase ? <NavLink {...views.closed} icon={CircleCheck} label="Closed" /> : <WaitingLink />}
      <button
        type="button"
        aria-label="Add item"
        disabled={links.heap?.archived ?? false}
        onClick={() => setAdding(true)}
        className="-mt-6 size-14 justify-self-center rounded-[18px] bg-ink text-canvas shadow-lg outline-offset-2 focus-visible:outline-2 focus-visible:outline-accent disabled:opacity-50"
      >
        <Plus aria-hidden="true" className="mx-auto size-6" strokeWidth={2.25} />
      </button>
      {localDatabase ? (
        <NavLink {...views.settings} icon={Settings} label="Settings" />
      ) : (
        <NavLink {...views.closed} icon={CircleCheck} label="Closed" />
      )}
      <button
        type="button"
        aria-expanded={more}
        {...(onMorePage && { "data-status": "active", "aria-current": "true" })}
        onClick={() => setMore(true)}
        className={`${navItemClass} outline-none focus-visible:outline-2 focus-visible:outline-accent aria-expanded:text-ink`}
      >
        <Ellipsis aria-hidden="true" className="size-5 shrink-0" />
        <span>More</span>
      </button>
      {adding && <AddItemDialog heapRef={heapRef} view={view} search={search} onClose={() => setAdding(false)} />}
      <MoreSheet me={me} heapRef={heapRef} isOpen={more} onClose={() => setMore(false)} />
    </nav>
  );
}

/**
 * More on a phone: the heap's settings (in the server build; the local build has them as a tab), its activity and the team and account pages, as a card of 52 px rows above the tabs. It opens on
 * release, as a button does, so lifting the finger never picks a row.
 */
function MoreSheet({
  me,
  heapRef,
  isOpen,
  onClose,
}: {
  me: Me;
  heapRef: HeapRef;
  isOpen: boolean;
  onClose: () => void;
}) {
  const links = useShellLinks(me, heapRef);
  const views = heapViews(heapRef, false);
  const row = (label: string, note?: string) => (
    <>
      <span className="flex-1">{label}</span>
      {note && <span className="text-[13px] text-muted">{note}</span>}
    </>
  );

  return (
    <Sheet title="More" isOpen={isOpen} onClose={onClose} aboveTabs showTitle={false}>
      <nav aria-label="More" className="flex flex-col">
        {!localDatabase && (
          <Link {...views.settings} onClick={onClose} className={sheetRowClass}>
            {row("Settings")}
          </Link>
        )}
        <Link {...views.activity} onClick={onClose} className={sheetRowClass}>
          {row("Activity")}
        </Link>
        {links.team && (
          <Link
            to="/teams/$team"
            params={{ team: links.team.slug }}
            // Not marked from the heap's own pages, which sit under the team's path.
            activeOptions={{ exact: true }}
            onClick={onClose}
            className={sheetRowClass}
          >
            {row(links.team.label, "Team")}
          </Link>
        )}
        <Link {...links.teams} activeOptions={{ exact: true }} onClick={onClose} className={sheetRowClass}>
          {row(links.teams.label, links.teams.to === "/teams" ? String(me.teams.length) : undefined)}
        </Link>
        <Link to="/account" onClick={onClose} className={sheetRowClass}>
          {row("Account", `@${me.user.username}`)}
        </Link>
      </nav>
    </Sheet>
  );
}

/** Says when the heap is read-only, and lets admins unarchive it. */
function ArchivedNotice({ heapRef }: { heapRef: HeapRef }) {
  const { data: heap } = useHeap(heapRef);
  const setArchived = useSetHeapArchived(heapRef.team);
  if (!heap || heap.archived_at === null) return null;

  return (
    <div
      role="status"
      className="mt-4 flex flex-wrap items-center gap-2 rounded-lg border border-accent/40 bg-accent-soft px-3 py-2 text-sm text-accent-ink"
    >
      This heap is archived and read-only.
      {heap.allowed_actions.includes("unarchive") && (
        <button
          type="button"
          disabled={setArchived.isPending}
          onClick={() => setArchived.mutate({ heap, archived: false })}
          className="font-medium underline disabled:opacity-50"
        >
          Unarchive
        </button>
      )}
      <ProblemText error={setArchived.error} />
    </div>
  );
}
