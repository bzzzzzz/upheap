// The app shell at each width (design/app-ux.md, Layout): a sidebar on a wide screen, a rail on a tablet, and on a
// phone the heap name at the top with tabs at the bottom. Each part shows at its own widths through CSS alone, so
// the layout never waits for script to measure the screen.
import { Link, Outlet, useNavigate, useParams, type LinkProps } from "@tanstack/react-router";
import {
  Activity,
  Check,
  ChevronDown,
  CircleCheck,
  Ellipsis,
  Layers,
  List,
  LogOut,
  SlidersHorizontal,
  Tag,
  User,
  Users,
  type LucideIcon,
} from "lucide-react";
import type { ReactNode } from "react";
import { Button, Header, Menu, MenuItem, MenuSection, MenuTrigger, Popover } from "react-aria-components";

import type { Me } from "../api/client";
import { soleTeam, useHeap, useMe, useSetHeapArchived, useSignOut } from "../api/queries";
import { StorageNote } from "../components/LocalDatabase";
import { ProblemText } from "../components/ProblemText";

export function AppLayout() {
  const { data: me } = useMe();
  const { heapId } = useParams({ strict: false });

  return (
    // On a wide screen the shell is the sidebar plus the content column (15rem + 48rem + padding), centred, so the
    // menu stays next to what it opens. The left half behind it takes the sidebar's colour, so the sidebar reaches
    // the edge of the screen.
    <div className="lg:bg-[linear-gradient(to_right,var(--sunken)_50%,var(--canvas)_50%)]">
      <div className="min-h-dvh sm:flex lg:mx-auto lg:max-w-[66rem]">
        {me && <Sidebar me={me} heapId={heapId} />}
        <div className="min-w-0 flex-1 bg-canvas">
          {me && <PhoneTopBar me={me} heapId={heapId} />}
          {/* On a phone, room at the bottom for the tabs. */}
          <div className="mx-auto max-w-3xl px-4 pb-[calc(6rem+env(safe-area-inset-bottom))] sm:px-6 sm:pb-24">
            {heapId && <ArchivedNotice key={heapId} heapId={heapId} />}
            <Outlet />
            <div className="lg:hidden">
              <StorageNote />
            </div>
          </div>
        </div>
        {me && heapId && <PhoneTabs me={me} heapId={heapId} />}
      </div>
    </div>
  );
}

/** Where the shell's links go, for the signed-in user and the heap on screen. */
function useShellLinks(me: Me, heapId: string | undefined) {
  const heapTeam = me.teams.find((team) => team.heaps.some((heap) => heap.id === heapId));
  // With one team and no Teams list, as in the local build, the team link goes straight to it, so the heap's own
  // Team link would repeat it.
  const onlyTeam = soleTeam(me);
  return {
    heap: heapTeam?.heaps.find((heap) => heap.id === heapId),
    heapTeam: onlyTeam ? undefined : heapTeam,
    teams: onlyTeam
      ? ({ to: "/teams/$teamId", params: { teamId: onlyTeam.id }, label: onlyTeam.name } as const)
      : ({ to: "/teams", label: "Teams" } as const),
  };
}

// The shared look of a navigation entry: icon over a short label in the rail, icon beside the label in the sidebar,
// and at least 44 px tall everywhere. Links get `data-status="active"` from the router.
const navItemClass =
  "flex min-h-11 flex-col items-center justify-center gap-1 rounded-xl py-1.5 text-[11px] font-medium text-ink-soft hover:bg-line/60 hover:text-ink data-[status=active]:bg-surface data-[status=active]:text-ink data-[status=active]:shadow-sm lg:flex-row lg:justify-start lg:gap-3 lg:px-3 lg:text-sm";

function NavLink({ icon: Icon, label, ...link }: LinkProps & { icon: LucideIcon; label: string }) {
  return (
    <Link {...link} className={navItemClass}>
      <Icon aria-hidden="true" className="size-5 shrink-0 lg:size-4" />
      <span className="max-w-full truncate">{label}</span>
    </Link>
  );
}

/** The heap's views, from both the sidebar and the phone's tabs. */
function heapViews(heapId: string) {
  return {
    open: { to: "/heaps/$heapId", params: { heapId }, activeOptions: { exact: true, includeSearch: false } },
    closed: { to: "/heaps/$heapId/closed", params: { heapId } },
    activity: { to: "/heaps/$heapId/activity", params: { heapId } },
    tags: { to: "/heaps/$heapId/tags", params: { heapId } },
    parameters: { to: "/heaps/$heapId/parameters", params: { heapId } },
  } as const;
}

/** The sidebar on a wide screen, folded into a rail on a tablet; hidden on a phone. */
function Sidebar({ me, heapId }: { me: Me; heapId: string | undefined }) {
  const links = useShellLinks(me, heapId);
  const views = heapId && heapViews(heapId);

  return (
    <aside
      aria-label="Heaps and views"
      className="sticky top-0 hidden h-dvh w-20 shrink-0 flex-col gap-5 overflow-y-auto border-r border-line bg-sunken pt-[max(env(safe-area-inset-top),1rem)] pb-[max(env(safe-area-inset-bottom),1rem)] sm:flex sm:px-1.5 lg:w-60 lg:px-3"
    >
      <div className="hidden items-center gap-2 px-3 font-semibold tracking-tight lg:flex">
        <Layers aria-hidden="true" className="size-5 text-accent" />
        Upheap
      </div>

      <div className="lg:hidden">
        <HeapMenu me={me} heapId={heapId} placement="end top">
          <Button
            aria-label={`Switch heap${links.heap ? `: ${links.heap.name}` : ""}`}
            className="mx-auto flex size-12 items-center justify-center rounded-2xl bg-ink text-lg font-semibold text-canvas outline-offset-2 data-[focus-visible]:outline-2 data-[focus-visible]:outline-accent"
          >
            {(links.heap?.name ?? "Upheap").charAt(0).toUpperCase()}
          </Button>
        </HeapMenu>
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
                to="/heaps/$heapId"
                params={{ heapId: heap.id }}
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
          {links.heapTeam && (
            <NavLink to="/teams/$teamId" params={{ teamId: links.heapTeam.id }} icon={Users} label="Team" />
          )}
        </nav>
      )}

      <div className="mt-auto flex flex-col gap-1 lg:gap-0.5">
        <NavLink {...links.teams} activeOptions={{ exact: true }} icon={Users} />
        <NavLink to="/account" icon={User} label="Account" />
        <SignOutButton me={me} className={navItemClass}>
          <LogOut aria-hidden="true" className="size-5 shrink-0 lg:size-4" />
          <span>Sign out</span>
        </SignOutButton>
        <div className="hidden px-3 lg:block">
          <StorageNote />
        </div>
      </div>
    </aside>
  );
}

/** The heap name on a phone, which switches heap. */
function PhoneTopBar({ me, heapId }: { me: Me; heapId: string | undefined }) {
  const { heap } = useShellLinks(me, heapId);
  return (
    <header className="sticky top-0 z-10 flex items-center bg-canvas/95 px-4 pt-[max(env(safe-area-inset-top),0.5rem)] pb-1 backdrop-blur sm:hidden">
      <HeapMenu me={me} heapId={heapId} placement="bottom start">
        <Button
          aria-label={`Switch heap${heap ? `: ${heap.name}` : ""}`}
          className="-ml-1 flex min-h-11 min-w-0 items-center gap-1.5 rounded-lg px-1 text-xl font-semibold tracking-tight outline-none data-[focus-visible]:outline-2 data-[focus-visible]:outline-accent"
        >
          <span className="truncate">{heap?.name ?? "Upheap"}</span>
          <ChevronDown aria-hidden="true" className="size-5 shrink-0 text-ink-soft" />
        </Button>
      </HeapMenu>
    </header>
  );
}

/** The phone's tabs: the three views used most, and More for the rest. */
function PhoneTabs({ me, heapId }: { me: Me; heapId: string }) {
  const links = useShellLinks(me, heapId);
  const views = heapViews(heapId);
  const navigate = useNavigate();
  const signOut = useSignOut();

  return (
    <nav
      aria-label="Heap views"
      className="fixed inset-x-0 bottom-0 z-10 grid grid-cols-4 gap-1 border-t border-line bg-surface px-2 pt-1.5 pb-[max(env(safe-area-inset-bottom),0.375rem)] sm:hidden"
    >
      <NavLink {...views.open} icon={List} label="Open" />
      <NavLink {...views.closed} icon={CircleCheck} label="Closed" />
      <NavLink {...views.activity} icon={Activity} label="Activity" />
      <MenuTrigger>
        <Button
          className={`${navItemClass} outline-none data-[focus-visible]:outline-2 data-[focus-visible]:outline-accent`}
        >
          <Ellipsis aria-hidden="true" className="size-5 shrink-0" />
          <span>More</span>
        </Button>
        <MenuPopover placement="top end">
          <Menu
            aria-label="More"
            className="outline-none"
            onAction={(key) => {
              if (key === "sign-out") {
                signOut.mutate(undefined, { onSuccess: () => void navigate({ to: "/login" }) });
              } else if (key === "tags") {
                void navigate(views.tags);
              } else if (key === "parameters") {
                void navigate(views.parameters);
              } else if (key === "team" && links.heapTeam) {
                void navigate({ to: "/teams/$teamId", params: { teamId: links.heapTeam.id } });
              } else if (key === "teams") {
                void navigate(links.teams);
              } else if (key === "account") {
                void navigate({ to: "/account" });
              }
            }}
          >
            <MenuEntry id="tags" icon={Tag}>
              Tags
            </MenuEntry>
            <MenuEntry id="parameters" icon={SlidersHorizontal}>
              Parameters
            </MenuEntry>
            {links.heapTeam && (
              <MenuEntry id="team" icon={Users}>
                Team
              </MenuEntry>
            )}
            <MenuEntry id="teams" icon={Users}>
              {links.teams.label}
            </MenuEntry>
            <MenuEntry id="account" icon={User}>
              Account
            </MenuEntry>
            {me.allowed_actions.includes("sign_out") && (
              <MenuEntry id="sign-out" icon={LogOut}>
                Sign out
              </MenuEntry>
            )}
          </Menu>
        </MenuPopover>
      </MenuTrigger>
    </nav>
  );
}

/** A menu of every heap, by team, that opens the one picked. */
function HeapMenu({
  me,
  heapId,
  placement,
  children,
}: {
  me: Me;
  heapId: string | undefined;
  placement: "end top" | "bottom start";
  children: ReactNode;
}) {
  const navigate = useNavigate();
  return (
    <MenuTrigger>
      {children}
      <MenuPopover placement={placement}>
        <Menu
          aria-label="Heaps"
          className="outline-none"
          onAction={(key) => void navigate({ to: "/heaps/$heapId", params: { heapId: String(key) } })}
        >
          {me.teams.map((team) => (
            <MenuSection key={team.id} aria-label={team.name} className="not-first:mt-1">
              {me.teams.length > 1 && (
                <Header className="px-3 pt-2 pb-1 text-xs font-semibold tracking-wide text-muted uppercase">
                  {team.name}
                </Header>
              )}
              {team.heaps.map((heap) => (
                <MenuEntry
                  key={heap.id}
                  id={heap.id}
                  textValue={heap.name}
                  icon={heap.id === heapId ? Check : undefined}
                >
                  {heap.name}
                  {heap.archived && <span className="text-muted"> (archived)</span>}
                </MenuEntry>
              ))}
            </MenuSection>
          ))}
        </Menu>
      </MenuPopover>
    </MenuTrigger>
  );
}

function MenuPopover({
  placement,
  children,
}: {
  placement: "end top" | "bottom start" | "top end";
  children: ReactNode;
}) {
  return (
    <Popover
      placement={placement}
      offset={6}
      className="max-h-[70dvh] min-w-52 overflow-y-auto rounded-xl border border-line bg-surface p-1 text-ink shadow-lg outline-none"
    >
      {children}
    </Popover>
  );
}

function MenuEntry({
  id,
  icon: Icon,
  textValue,
  children,
}: {
  id: string;
  icon?: LucideIcon | undefined;
  textValue?: string;
  children: ReactNode;
}) {
  return (
    <MenuItem
      id={id}
      textValue={textValue ?? (typeof children === "string" ? children : undefined)}
      className="flex min-h-11 cursor-default items-center gap-3 rounded-lg px-3 text-[15px] outline-none data-[focused]:bg-sunken"
    >
      <span className="flex size-4 shrink-0 items-center justify-center text-ink-soft">
        {Icon && <Icon aria-hidden="true" className="size-4" />}
      </span>
      {children}
    </MenuItem>
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
function ArchivedNotice({ heapId }: { heapId: string }) {
  const { data: heap } = useHeap(heapId);
  const setArchived = useSetHeapArchived();
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
