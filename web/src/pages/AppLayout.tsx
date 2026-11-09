// The app shell at each width (design/app-ux.md, Layout): a sidebar on a wide screen, a rail on a tablet, and on a
// phone the heap name at the top with tabs at the bottom. Each part shows at its own widths through CSS alone, so
// the layout never waits for script to measure the screen.
import { Link, Outlet, useMatch, useNavigate, useParams, useSearch, type LinkProps } from "@tanstack/react-router";
import {
  Activity,
  Check,
  ChevronDown,
  CircleCheck,
  Ellipsis,
  List,
  LogOut,
  Plus,
  SlidersHorizontal,
  Tag,
  User,
  Users,
  type LucideIcon,
} from "lucide-react";
import { useRef, useState, type FormEvent, type ReactNode, type RefObject } from "react";

import type { HeapRef, Me } from "../api/client";
import { soleTeam, useCreateHeap, useHeap, useMe, useSetHeapArchived, useSignOut, useTeam } from "../api/queries";
import { AddItemDialog } from "../components/AddItem";
import { useListSearch, ListSearchProvider } from "../components/listSearch";
import { StorageNote } from "../components/LocalDatabase";
import { Logo } from "../components/Logo";
import { useKeyboardInset, usePhoneTabsHeight } from "../components/media";
import { PopoverSheet } from "../components/PopoverSheet";
import { ProblemText } from "../components/ProblemText";
import { Sheet, sheetRowClass } from "../components/Sheet";
import { ToastProvider } from "../components/UndoToast";

export function AppLayout() {
  const { data: me } = useMe();
  useKeyboardInset();
  const { team, heap } = useParams({ strict: false });
  const heapRef = team !== undefined && heap !== undefined ? { team, heap } : undefined;
  // The heap's list, also with an item open over it.
  const onList = useMatch({ from: "/app/teams/$team/heaps/$heap", shouldThrow: false }) !== undefined;

  return (
    // On a wide screen the shell is the sidebar plus the content column (15rem + 48rem + padding), centred, so the
    // menu stays next to what it opens. The left half behind it takes the sidebar's colour, so the sidebar reaches
    // the edge of the screen. An open item pane (`data-item-pane`) widens the block by its own width and takes the
    // right-hand padding.
    <ToastProvider>
      <ListSearchProvider listKey={onList && heapRef ? `${heapRef.team}/${heapRef.heap}` : undefined}>
        <div className="lg:bg-[linear-gradient(to_right,var(--sunken)_50%,var(--canvas)_50%)]">
          <div className="min-h-dvh sm:flex lg:mx-auto lg:max-w-[66rem] lg:has-[[data-item-pane]]:max-w-[92rem]">
            {me && <Sidebar me={me} heapRef={heapRef} />}
            <div className="min-w-0 flex-1 bg-canvas">
              {me && <PhoneTopBar me={me} heapRef={heapRef} />}
              {/* On a phone, room at the bottom for the tabs. */}
              <div className="mx-auto max-w-3xl px-4 pb-[calc(6rem+env(safe-area-inset-bottom))] sm:px-6 sm:pb-24 lg:has-[[data-item-pane]]:max-w-none lg:has-[[data-item-pane]]:pr-0">
                {heapRef && <ArchivedNotice key={`${heapRef.team}/${heapRef.heap}`} heapRef={heapRef} />}
                <Outlet />
                <div className="lg:hidden">
                  <StorageNote className="mt-16 border-t border-line pt-4" />
                </div>
              </div>
            </div>
            {me && heapRef && <PhoneTabs me={me} heapRef={heapRef} />}
          </div>
        </div>
      </ListSearchProvider>
    </ToastProvider>
  );
}

/** Where the shell's links go, for the signed-in user and the heap on screen. */
function useShellLinks(me: Me, heapRef: HeapRef | undefined) {
  const heapTeam = me.teams.find((team) => team.slug === heapRef?.team);
  // With one team and no Teams list, as in the local build, the team link goes straight to it, so the heap's own
  // Team link would repeat it.
  const onlyTeam = soleTeam(me);
  return {
    heap: heapTeam?.heaps.find((heap) => heap.slug === heapRef?.heap),
    heapTeam: onlyTeam ? undefined : heapTeam,
    teams: onlyTeam
      ? ({ to: "/teams/$team", params: { team: onlyTeam.slug }, label: onlyTeam.name } as const)
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

/** With `current`, the link shows as active also where the router wouldn't say so. */
function NavLink({
  icon: Icon,
  label,
  current,
  ...link
}: LinkProps & { icon: LucideIcon; label: string; current?: boolean }) {
  return (
    <Link {...link} {...(current && { "data-status": "active", "aria-current": "page" })} className={navItemClass}>
      <Icon aria-hidden="true" className="size-5 shrink-0 lg:size-4" />
      <span className="max-w-full truncate">{label}</span>
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
    tags: { to: "/teams/$team/heaps/$heap/tags", params: heapRef },
    parameters: { to: "/teams/$team/heaps/$heap/parameters", params: heapRef },
  } as const;
}

/** The sidebar on a wide screen, folded into a rail on a tablet; hidden on a phone. */
function Sidebar({ me, heapRef }: { me: Me; heapRef: HeapRef | undefined }) {
  const links = useShellLinks(me, heapRef);
  const { item: itemKey } = useParams({ strict: false });
  const views = heapRef && heapViews(heapRef, itemKey !== undefined);

  return (
    <aside
      aria-label="Heaps and views"
      className="sticky top-0 hidden h-dvh w-20 shrink-0 flex-col gap-5 overflow-y-auto border-r border-line bg-sunken pt-[max(env(safe-area-inset-top),1rem)] pb-[max(env(safe-area-inset-bottom),1rem)] sm:flex sm:px-1.5 lg:w-60 lg:px-3"
    >
      <div className="hidden items-center gap-2 px-3 font-semibold tracking-tight lg:flex">
        <Logo className="size-5 text-accent" />
        Upheap
      </div>

      <div className="lg:hidden">
        <HeapSwitcher me={me} heapRef={heapRef} placement="end top">
          {(props) => (
            <button
              type="button"
              {...props}
              aria-label={`Switch heap${links.heap ? `: ${links.heap.name}` : ""}`}
              className="mx-auto flex size-12 items-center justify-center rounded-2xl bg-ink text-lg font-semibold text-canvas outline-offset-2 focus-visible:outline-2 focus-visible:outline-accent"
            >
              {(links.heap?.name ?? "Upheap").charAt(0).toUpperCase()}
            </button>
          )}
        </HeapSwitcher>
      </div>

      <nav aria-label="Heaps" className="hidden flex-col gap-0.5 lg:flex">
        {me.teams.map((team) => (
          <div key={team.id} className="flex flex-col gap-0.5">
            <h2 className="px-3 pt-1 pb-1 text-xs font-semibold tracking-wide text-muted uppercase">
              {me.teams.length > 1 ? team.name : "Heaps"}
            </h2>
            {team.heaps.map((heap) => (
              <Link
                key={heap.id}
                to="/teams/$team/heaps/$heap"
                params={{ team: team.slug, heap: heap.slug }}
                activeOptions={{ includeSearch: false }}
                className="flex min-h-9 items-center rounded-lg px-3 text-sm text-ink-soft hover:bg-line/60 hover:text-ink data-[status=active]:font-semibold data-[status=active]:text-ink"
              >
                <span className="truncate">
                  {heap.name}
                  {heap.archived && <span className="text-muted"> (archived)</span>}
                </span>
              </Link>
            ))}
          </div>
        ))}
      </nav>

      {views && (
        <nav aria-label="Heap views" className="flex flex-col gap-1 lg:gap-0.5">
          {links.heap && (
            <p className="hidden truncate px-3 pb-1 text-xs font-semibold tracking-wide text-muted uppercase lg:block">
              {links.heap.name}
            </p>
          )}
          <NavLink {...views.open} icon={List} label="Open" />
          <NavLink {...views.closed} icon={CircleCheck} label="Closed" />
          <NavLink {...views.activity} icon={Activity} label="Activity" />
          <NavLink {...views.tags} icon={Tag} label="Tags" />
          <NavLink {...views.parameters} icon={SlidersHorizontal} label="Parameters" />
        </nav>
      )}

      <div className="mt-auto flex flex-col gap-1 lg:gap-0.5">
        <NavLink {...links.teams} activeOptions={{ exact: true }} icon={Users} />
        <NavLink to="/account" icon={User} label="Account" />
        <SignOutButton me={me} className={navItemClass}>
          <LogOut aria-hidden="true" className="size-5 shrink-0 lg:size-4" />
          <span>Sign out</span>
        </SignOutButton>
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
  return (
    <header className="sticky top-0 z-10 flex items-center bg-canvas/95 px-4 pt-[max(env(safe-area-inset-top),0.5rem)] pb-1 backdrop-blur sm:hidden">
      <HeapSwitcher me={me} heapRef={heapRef}>
        {(props) => (
          <button
            type="button"
            {...props}
            aria-label={`Switch heap${heap ? `: ${heap.name}` : ""}`}
            className="-ml-1 flex min-h-11 min-w-0 items-center gap-1.5 rounded-lg px-1 text-xl font-semibold tracking-tight outline-none focus-visible:outline-2 focus-visible:outline-accent"
          >
            <span className="truncate">{heap?.name ?? "Upheap"}</span>
            <ChevronDown aria-hidden="true" className="size-5 shrink-0 text-ink-soft" />
          </button>
        )}
      </HeapSwitcher>
    </header>
  );
}

/**
 * The phone's tabs (ux-overview.md, What comes first §1): Open, Closed, Add in the middle under the thumb, Activity,
 * and More for the rest.
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
  // The heap's pages that only More leads to, so More shows as active on them. The tabs show only on a heap's pages.
  const onTags = useMatch({ from: "/app/teams/$team/heaps/$heap/tags", shouldThrow: false }) !== undefined;
  const onParameters = useMatch({ from: "/app/teams/$team/heaps/$heap/parameters", shouldThrow: false }) !== undefined;
  const onMorePage = onTags || onParameters;

  return (
    <nav
      ref={tabs}
      aria-label="Heap views"
      className="fixed inset-x-0 bottom-0 z-10 grid grid-cols-5 items-end gap-1 border-t border-line bg-surface px-2 pt-1.5 pb-[max(env(safe-area-inset-bottom),0.375rem)] sm:hidden"
    >
      <NavLink {...views.open} icon={List} label="Open" />
      <NavLink {...views.closed} icon={CircleCheck} label="Closed" />
      <button
        type="button"
        aria-label="Add an item"
        disabled={links.heap?.archived ?? false}
        onClick={() => setAdding(true)}
        className="-mt-6 size-14 justify-self-center rounded-[18px] bg-ink text-canvas shadow-lg outline-offset-2 focus-visible:outline-2 focus-visible:outline-accent disabled:opacity-50"
      >
        <Plus aria-hidden="true" className="mx-auto size-6" strokeWidth={2.25} />
      </button>
      <NavLink {...views.activity} icon={Activity} label="Activity" />
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
 * More on a phone: the heap's other views and the account pages, as a card of 52 px rows above the tabs. It opens on
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
  const navigate = useNavigate();
  const signOut = useSignOut();
  const row = (label: string, note?: string) => (
    <>
      <span className="flex-1">{label}</span>
      {note && <span className="text-[13px] text-muted">{note}</span>}
    </>
  );

  return (
    <Sheet title="More" isOpen={isOpen} onClose={onClose} aboveTabs showTitle={false}>
      <nav aria-label="More" className="flex flex-col">
        {links.heapTeam && (
          <Link
            to="/teams/$team"
            params={{ team: links.heapTeam.slug }}
            // Not marked from the heap's own pages, which sit under the team's path.
            activeOptions={{ exact: true }}
            onClick={onClose}
            className={sheetRowClass}
          >
            {row("Team", links.heapTeam.name)}
          </Link>
        )}
        <Link {...links.teams} activeOptions={{ exact: true }} onClick={onClose} className={sheetRowClass}>
          {row(links.teams.label, links.teams.to === "/teams" ? String(me.teams.length) : undefined)}
        </Link>
        <Link {...views.tags} onClick={onClose} className={sheetRowClass}>
          {row("Tags")}
        </Link>
        <Link {...views.parameters} onClick={onClose} className={sheetRowClass}>
          {row("Parameters")}
        </Link>
        <Link to="/account" onClick={onClose} className={sheetRowClass}>
          {row("Account", `@${me.user.username}`)}
        </Link>
        {me.allowed_actions.includes("sign_out") && (
          <button
            type="button"
            onClick={() => signOut.mutate(undefined, { onSuccess: () => void navigate({ to: "/login" }) })}
            className={`${sheetRowClass} mt-1 rounded-t-none border-t border-line text-overdue-ink`}
          >
            <LogOut aria-hidden="true" className="size-4" />
            Sign out
          </button>
        )}
      </nav>
    </Sheet>
  );
}

/**
 * Every heap, by team, with a check on the one on screen (ux-overview.md, What comes first §3): a popover beside the
 * rail on a tablet, a sheet of 52 px rows on a phone. It ends with a new heap in the heap's own team, where the user
 * may create one.
 */
function HeapSwitcher({
  me,
  heapRef,
  placement = "bottom end",
  children,
}: {
  me: Me;
  heapRef: HeapRef | undefined;
  placement?: "bottom end" | "end top";
  children: (props: {
    ref: RefObject<HTMLButtonElement | null>;
    onClick: () => void;
    "aria-expanded": boolean;
  }) => ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const close = () => setOpen(false);
  const team = me.teams.find((t) => t.slug === heapRef?.team);

  return (
    <>
      {children({ ref: trigger, onClick: () => setOpen(true), "aria-expanded": open })}
      <PopoverSheet title="Heaps" triggerRef={trigger} isOpen={open} onClose={close} placement={placement}>
        <nav aria-label="Heaps" className="-mx-1 flex flex-col">
          {me.teams.map((t) => (
            <section key={t.id} aria-label={t.name} className="flex flex-col">
              {me.teams.length > 1 && (
                <h3 className="px-3 pt-2.5 pb-0.5 text-xs font-semibold tracking-wide text-muted uppercase">
                  {t.name}
                </h3>
              )}
              {t.heaps.map((heap) => {
                const current = t.slug === heapRef?.team && heap.slug === heapRef.heap;
                return (
                  <Link
                    key={heap.id}
                    to="/teams/$team/heaps/$heap"
                    params={{ team: t.slug, heap: heap.slug }}
                    // The heap on screen, on any of its pages.
                    activeOptions={{ includeSearch: false }}
                    onClick={close}
                    className={sheetRowClass}
                  >
                    <span className="min-w-0 flex-1 truncate">
                      {heap.name}
                      {heap.archived && <span className="font-normal text-muted"> (archived)</span>}
                    </span>
                    <span aria-hidden="true" className="w-5 text-accent-ink">
                      {current && <Check className="size-4" strokeWidth={3} />}
                    </span>
                  </Link>
                );
              })}
            </section>
          ))}
        </nav>
        {team && <NewHeap team={team} onCreated={close} />}
      </PopoverSheet>
    </>
  );
}

/** "New heap in <team>", which becomes a name field, for those the team lets create heaps. */
function NewHeap({ team, onCreated }: { team: Me["teams"][number]; onCreated: () => void }) {
  const { data } = useTeam(team.slug);
  const create = useCreateHeap(team.slug);
  const navigate = useNavigate();
  const [naming, setNaming] = useState(false);
  const [name, setName] = useState("");
  if (!data?.allowed_actions.includes("create_heap")) return null;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (name.trim() === "") return;
    create.mutate(name.trim(), {
      onSuccess: (heap) => {
        onCreated();
        void navigate({ to: "/teams/$team/heaps/$heap", params: { team: team.slug, heap: heap.slug } });
      },
    });
  };

  const buttonClass =
    "flex min-h-12 flex-1 items-center justify-center gap-1.5 rounded-xl border border-line bg-surface px-3 text-[15px] hover:border-muted disabled:opacity-50";
  return (
    <div className="border-t border-line pt-3">
      {naming ? (
        <form onSubmit={submit} className="flex flex-col gap-2">
          <input
            aria-label={`Name of the new heap in ${team.name}`}
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
            maxLength={100}
            placeholder="Heap name"
            className="h-12 rounded-xl border border-line bg-surface px-3 text-base"
          />
          <div className="flex gap-2">
            <button type="button" onClick={() => setNaming(false)} className={buttonClass}>
              Cancel
            </button>
            <button
              type="submit"
              disabled={create.isPending || name.trim() === ""}
              className={`${buttonClass} border-ink bg-ink font-medium text-canvas`}
            >
              Add heap
            </button>
          </div>
          <ProblemText error={create.error} />
        </form>
      ) : (
        <button type="button" onClick={() => setNaming(true)} className={buttonClass + " w-full"}>
          <Plus aria-hidden="true" className="size-4" />
          New heap in {team.name}
        </button>
      )}
    </div>
  );
}

function SignOutButton({ me, className, children }: { me: Me; className: string; children: ReactNode }) {
  const navigate = useNavigate();
  const signOut = useSignOut();
  if (!me.allowed_actions.includes("sign_out")) return null;
  return (
    <button
      type="button"
      onClick={() => signOut.mutate(undefined, { onSuccess: () => void navigate({ to: "/login" }) })}
      className={className}
    >
      {children}
    </button>
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
