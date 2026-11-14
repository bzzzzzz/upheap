import { Link } from "@tanstack/react-router";

import type { Item, StepState } from "../api/client";
import { becameWords, STATE_LABELS } from "./labels";

const linkClass =
  "text-accent-ink underline underline-offset-2 outline-none focus-visible:outline-2 focus-visible:outline-accent";

const STEP_STATE_LABELS: Record<StepState, string> = {
  open: "Open",
  ticked: "Ticked",
  dropped: "Dropped",
};

/**
 * What a note became: "Became: 2 tasks" with each of them, its state and a link, "Became: a step" with the task
 * the step is on and the step's state, and "Became: a heap" with a link to the heap's list when only heaps were made.
 * Nothing when nothing was made from the item. Where it came from is `ItemFrom`.
 * Each is linked in the heap it is in now, which a move may have changed.
 */
export function ItemBecame({ item }: { item: Item }) {
  const made = item.became ?? [];
  const steps = item.became_steps ?? [];
  const heaps = item.became_heaps ?? [];
  if (made.length === 0 && steps.length === 0 && heaps.length === 0) return null;
  return (
    <section aria-label="Became" className="flex flex-col gap-1 text-sm">
      <p>
        <span className="text-ink-soft">Became: </span>
        {made.length > 0 ? becameWords(made) : steps.length > 0 ? "a step" : "a heap"}
      </p>
      <ul className="flex flex-col gap-1">
        {made.map((child) => (
          <li key={child.id} className="flex items-baseline gap-2 break-words">
            <Link
              to="/teams/$team/heaps/$heap/items/$item"
              params={{ team: child.heap.team_slug, heap: child.heap.slug, item: child.key }}
              search={true}
              className={linkClass}
            >
              {child.title}
            </Link>
            <span className="shrink-0 text-xs text-muted">{STATE_LABELS[child.state]}</span>
          </li>
        ))}
        {steps.map((step) => (
          <li key={`${step.task.id}.${step.number}`} className="flex items-baseline gap-2 break-words">
            <span>
              A step of{" "}
              <Link
                to="/teams/$team/heaps/$heap/items/$item"
                params={{ team: step.task.heap.team_slug, heap: step.task.heap.slug, item: step.task.key }}
                search={true}
                className={linkClass}
              >
                “{step.task.title}”
              </Link>
            </span>
            <span className="shrink-0 text-xs text-muted">{STEP_STATE_LABELS[step.state]}</span>
          </li>
        ))}
        {heaps.map((heap) => (
          <li key={heap.id} className="break-words">
            Heap{" "}
            <Link
              to="/teams/$team/heaps/$heap"
              params={{ team: heap.team_slug, heap: heap.slug }}
              search={true}
              className={linkClass}
            >
              “{heap.name}”
            </Link>
          </li>
        ))}
      </ul>
    </section>
  );
}
