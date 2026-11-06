import { useClosedItems, useReopenItem } from "../api/queries";
import { STATE_LABELS } from "../components/labels";
import { TagChips } from "../components/TagChips";

/** Done and won't-do items, most recently closed first. */
export function ClosedPage({ heapId }: { heapId: string }) {
  const closed = useClosedItems(heapId);
  const reopen = useReopenItem(heapId);

  if (closed.isPending) {
    return <p className="py-16 text-center text-stone-500">Loading…</p>;
  }
  if (closed.isError) {
    return (
      <p role="alert" className="py-16 text-center text-red-700 dark:text-red-400">
        {closed.error.message}
      </p>
    );
  }

  const items = closed.data.pages.flatMap((page) => page.items);
  return (
    <main>
      <h1 className="mt-6 mb-4 text-xl font-semibold tracking-tight">Closed</h1>
      {items.length === 0 && <p className="py-12 text-center text-stone-500">Nothing closed yet.</p>}
      {items.length > 0 && (
        <ul className="rounded-lg border border-stone-200 bg-white px-3 dark:border-stone-800 dark:bg-stone-900">
          {items.map((item) => (
            <li
              key={item.id}
              className="flex items-center gap-3 border-b border-stone-200 py-3 last:border-b-0 dark:border-stone-800"
            >
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                  <p className="font-medium text-stone-600 dark:text-stone-300">{item.title}</p>
                  <TagChips tags={item.tags} />
                </div>
                <p className="text-xs text-stone-500">
                  {STATE_LABELS[item.state]} · {item.closed_label}
                </p>
              </div>
              {item.allowed_actions.includes("reopen") && (
                <button
                  type="button"
                  disabled={reopen.isPending}
                  onClick={() => reopen.mutate(item)}
                  className="rounded-md border border-stone-300 px-2 py-1 text-sm hover:bg-stone-100 dark:border-stone-700 dark:hover:bg-stone-800"
                >
                  Reopen
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
      {reopen.error && (
        <p role="alert" className="mt-2 text-sm text-red-700 dark:text-red-400">
          {reopen.error.message}
        </p>
      )}
      {closed.hasNextPage && (
        <button
          type="button"
          onClick={() => void closed.fetchNextPage()}
          disabled={closed.isFetchingNextPage}
          className="mt-4 text-sm text-stone-500 hover:text-stone-900 dark:hover:text-stone-100"
        >
          Load more
        </button>
      )}
    </main>
  );
}
