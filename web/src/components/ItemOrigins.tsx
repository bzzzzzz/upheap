import { Link } from "@tanstack/react-router";

import type { HeapRef, Item } from "../api/client";
import { becameWords, KIND_LABELS, STATE_LABELS } from "./labels";

const linkClass =
  "text-accent-ink underline underline-offset-2 outline-none focus-visible:outline-2 focus-visible:outline-accent";

/**
 * What the item came from and what it became (ux.md, The item): "From: “<title>”, a note" on an item made from another,
 * and "Became: 3 tasks" with each of them, its state and a link, on a note that was split. Nothing when the item has
 * neither. The items are linked in the heap this one is in: that is where they were made, and a split keeps them in one
 * heap.
 */
export function ItemOrigins({ heapRef, item }: { heapRef: HeapRef; item: Item }) {
  const { origin, became } = item;
  const made = became ?? [];
  if (!origin && made.length === 0) return null;
  return (
    <section aria-label="Origins" className="flex flex-col gap-2 border-t border-line pt-3 text-sm">
      {origin && (
        <p className="break-words">
          <span className="text-ink-soft">From: </span>“
          <Link
            to="/teams/$team/heaps/$heap/items/$item"
            params={{ ...heapRef, item: origin.key }}
            className={linkClass}
          >
            {origin.title}
          </Link>
          ”, a {KIND_LABELS[origin.kind].toLowerCase()}
        </p>
      )}
      {made.length > 0 && (
        <div className="flex flex-col gap-1">
          <p>
            <span className="text-ink-soft">Became: </span>
            {becameWords(made)}
          </p>
          <ul className="flex flex-col gap-1">
            {made.map((child) => (
              <li key={child.id} className="flex items-baseline gap-2 break-words">
                <Link
                  to="/teams/$team/heaps/$heap/items/$item"
                  params={{ ...heapRef, item: child.key }}
                  className={linkClass}
                >
                  {child.title}
                </Link>
                <span className="shrink-0 text-xs text-muted">{STATE_LABELS[child.state]}</span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}
