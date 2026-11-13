import { Link } from "@tanstack/react-router";

import type { HeapRef, Item } from "../api/client";
import { becameWords, STATE_LABELS } from "./labels";

const linkClass =
  "text-accent-ink underline underline-offset-2 outline-none focus-visible:outline-2 focus-visible:outline-accent";

/**
 * What a note became (work-with-agents, W8): "Became: 2 tasks" with each of them, its state and a link. Nothing when
 * nothing was made from the item. Where it came from is `ItemFrom`. The items are linked in the heap this one is in:
 * that is where they were made, and a split keeps them in one heap.
 */
export function ItemBecame({ heapRef, item }: { heapRef: HeapRef; item: Item }) {
  const made = item.became ?? [];
  if (made.length === 0) return null;
  return (
    <section aria-label="Became" className="flex flex-col gap-1 text-sm">
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
    </section>
  );
}
