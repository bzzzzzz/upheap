import { Link, Outlet, useNavigate, useParams } from "@tanstack/react-router";

import { useMe, useSignOut } from "../api/queries";

export function AppLayout() {
  const { data: me } = useMe();
  const { heapId } = useParams({ strict: false });
  const navigate = useNavigate();
  const signOut = useSignOut();

  const heapCount = me?.teams.reduce((n, team) => n + team.heaps.length, 0) ?? 0;

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
              activeOptions={{ exact: true }}
              className="text-stone-500 hover:text-stone-900 data-[status=active]:font-medium data-[status=active]:text-stone-900 dark:hover:text-stone-100 dark:data-[status=active]:text-stone-100"
            >
              Open
            </Link>
            <Link
              to="/heaps/$heapId/closed"
              params={{ heapId }}
              className="text-stone-500 hover:text-stone-900 data-[status=active]:font-medium data-[status=active]:text-stone-900 dark:hover:text-stone-100 dark:data-[status=active]:text-stone-100"
            >
              Closed
            </Link>
            <Link
              to="/heaps/$heapId/tags"
              params={{ heapId }}
              className="text-stone-500 hover:text-stone-900 data-[status=active]:font-medium data-[status=active]:text-stone-900 dark:hover:text-stone-100 dark:data-[status=active]:text-stone-100"
            >
              Tags
            </Link>
          </nav>
        )}
        <span className="ml-auto text-sm text-stone-500">{me?.user.display_name}</span>
        <button
          type="button"
          onClick={() => signOut.mutate(undefined, { onSuccess: () => void navigate({ to: "/login" }) })}
          className="text-sm text-stone-500 hover:text-stone-900 dark:hover:text-stone-100"
        >
          Sign out
        </button>
      </header>
      <Outlet />
    </div>
  );
}
