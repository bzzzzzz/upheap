import { Link, Outlet, useNavigate, useParams } from "@tanstack/react-router";

import { useHeap, useMe, useSetHeapArchived, useSignOut } from "../api/queries";
import { ProblemText } from "../components/ProblemText";

const navLink =
  "text-stone-500 hover:text-stone-900 data-[status=active]:font-medium data-[status=active]:text-stone-900 dark:hover:text-stone-100 dark:data-[status=active]:text-stone-100";

export function AppLayout() {
  const { data: me } = useMe();
  const { heapId } = useParams({ strict: false });
  const navigate = useNavigate();
  const signOut = useSignOut();

  const heapCount = me?.teams.reduce((n, team) => n + team.heaps.length, 0) ?? 0;
  const heapTeam = me?.teams.find((team) => team.heaps.some((heap) => heap.id === heapId));

  return (
    <div className="mx-auto max-w-3xl px-4 pb-24">
      <header className="flex flex-wrap items-center gap-x-4 gap-y-2 border-b border-stone-200 py-4 dark:border-stone-800">
        <span className="font-semibold tracking-tight">Upheap</span>
        {me && heapId && heapCount > 1 && (
          <label className="text-sm">
            <span className="sr-only">Heap</span>
            <select
              value={heapId}
              onChange={(e) => void navigate({ to: "/heaps/$heapId", params: { heapId: e.target.value } })}
              className="rounded-md border border-stone-300 bg-white px-2 py-1 dark:border-stone-700 dark:bg-stone-900"
            >
              {me.teams.map((team) => (
                <optgroup key={team.id} label={team.name}>
                  {team.heaps.map((heap) => (
                    <option key={heap.id} value={heap.id}>
                      {heap.name}
                      {heap.archived ? " (archived)" : ""}
                    </option>
                  ))}
                </optgroup>
              ))}
            </select>
          </label>
        )}
        {heapId && (
          <nav className="flex gap-3 text-sm" aria-label="Heap views">
            <Link
              to="/heaps/$heapId"
              params={{ heapId }}
              activeOptions={{ exact: true, includeSearch: false }}
              className={navLink}
            >
              Open
            </Link>
            <Link to="/heaps/$heapId/closed" params={{ heapId }} className={navLink}>
              Closed
            </Link>
            <Link to="/heaps/$heapId/tags" params={{ heapId }} className={navLink}>
              Tags
            </Link>
            <Link to="/heaps/$heapId/parameters" params={{ heapId }} className={navLink}>
              Parameters
            </Link>
            <Link to="/heaps/$heapId/activity" params={{ heapId }} className={navLink}>
              Activity
            </Link>
            {heapTeam && (
              <Link to="/teams/$teamId" params={{ teamId: heapTeam.id }} className={navLink}>
                Team
              </Link>
            )}
          </nav>
        )}
        <nav className="ml-auto flex gap-3 text-sm" aria-label="Account">
          <Link to="/teams" activeOptions={{ exact: true }} className={navLink}>
            Teams
          </Link>
          <Link to="/account" className={navLink}>
            {me?.user.display_name}
          </Link>
        </nav>
        <button
          type="button"
          onClick={() => signOut.mutate(undefined, { onSuccess: () => void navigate({ to: "/login" }) })}
          className="text-sm text-stone-500 hover:text-stone-900 dark:hover:text-stone-100"
        >
          Sign out
        </button>
      </header>
      {heapId && <ArchivedNotice key={heapId} heapId={heapId} />}
      <Outlet />
    </div>
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
      className="mt-4 flex flex-wrap items-center gap-2 rounded-md border border-amber-300 bg-amber-50 px-3 py-2 text-sm text-amber-900 dark:border-amber-800 dark:bg-amber-950 dark:text-amber-200"
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
