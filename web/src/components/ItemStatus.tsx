import { Link } from "@tanstack/react-router";
import { useEffect, useId, useRef, useState } from "react";

import type { HeapRef, Item, RankedItem } from "../api/client";
import { chipClass } from "./buttons";
import { AgentChip, ChangesAskedButton, SendToAgentButton } from "./ItemTurn";
import { KindIcon, StageIcon } from "./kindIcons";
import { isClosed, KIND_LABELS, STAGE_LABELS, STATE_LABELS } from "./labels";
import { PopoverSheet } from "./PopoverSheet";
import { formatDate, isToCome } from "./reminders";
import { useFocusAfterRender } from "./useFocusAfterRender";

// With spaces, so the line reads as "Task · With context · Open" to anything that takes its text.
const dot = (
  <span aria-hidden="true" className="text-muted">
    {" · "}
  </span>
);

/**
 * What the item is and where it stands, under its title at every width (ux.md, The item §2): one quiet line that may
 * wrap. The kind and stage with their icons, a reminder's date, the state and, for a closed item, when it closed, or
 * who blocks a blocked one, with a link to that item, and why in words when a reason was given: it wraps, never
 * shortened. Then the chips: the item's place in the list, which opens why,
 * and, who has the item when an agent does, or the button that sends it to one when none does. A reminder that is still ahead says when it joins the list under the
 * line. Closed items show the kind only, as the server sends no stage for them.
 */
export function StatusLine({
  heapRef,
  item,
  entry,
  canReopen,
  onTakenBack,
}: {
  heapRef: HeapRef;
  item: Item;
  entry: RankedItem | undefined;
  canReopen: boolean;
  onTakenBack: () => void;
}) {
  const date = item.kind === "reminder" && item.due ? formatDate(item.due.date) : undefined;
  const closed = isClosed(item.state);
  const blocker = item.state === "blocked" ? item.blocked_by : null;
  const reason = item.state === "blocked" ? item.blocked_reason : null;
  const focusAfterRender = useFocusAfterRender();
  // Set by a send, until the item shows the agent's chip, which then takes the focus: the send button goes away with the
  // send, so the focus is kept here.
  const [sent, setSent] = useState(0);
  const focused = useRef(0);
  const hasTurn = item.turn != null;
  useEffect(() => {
    if (sent === focused.current || !hasTurn) return;
    focused.current = sent;
    focusAfterRender(() => document.querySelector<HTMLElement>("[data-item-pane] [data-agent-chip]"));
  }, [sent, hasTurn, focusAfterRender]);
  return (
    <div className="flex flex-col gap-1">
      <div
        role="group"
        aria-label="Status"
        className="flex flex-wrap items-center gap-x-1.5 gap-y-1 text-sm text-muted"
      >
        <span title={KIND_LABELS[item.kind]} className="flex items-center gap-1.5">
          <KindIcon kind={item.kind} />
          {KIND_LABELS[item.kind]}
        </span>
        {date && (
          <>
            {dot}
            <time dateTime={item.due?.date}>{date}</time>
          </>
        )}
        {item.stage && (
          <>
            {dot}
            <span title={STAGE_LABELS[item.stage]} className="flex items-center gap-1.5">
              <StageIcon stage={item.stage} />
              {STAGE_LABELS[item.stage]}
            </span>
          </>
        )}
        {dot}
        {blocker ? (
          <span className="flex min-w-0 flex-wrap items-center gap-1 font-medium text-ink">
            Blocked by
            <Link
              to="/teams/$team/heaps/$heap/items/$item"
              // A blocker moved to another heap of the team is opened there.
              params={{ ...heapRef, heap: item.blocker_gone?.moved_to?.slug ?? heapRef.heap, item: blocker.key }}
              search={true}
              // In place of this item, so Back still closes the pane rather than walking back through the blockers.
              replace
              state={(prev) => prev}
              className="min-w-0 truncate font-normal text-accent-ink underline"
            >
              <span className="font-mono text-xs">{blocker.key}</span> {blocker.title}
            </Link>
            {reason && <span className="font-normal text-ink break-words">: {reason}</span>}
          </span>
        ) : reason ? (
          <span className="min-w-0 font-medium break-words text-ink">
            Blocked: <span className="font-normal">{reason}</span>
          </span>
        ) : (
          <span className={item.state === "in_progress" ? "font-medium text-accent-ink" : "font-medium text-ink"}>
            {STATE_LABELS[item.state]}
          </span>
        )}
        {item.closed_label && (
          <>
            {dot} <span>{item.closed_label}</span>
          </>
        )}
        {closed && !canReopen && (
          <>
            {dot} <span>It can't be reopened.</span>
          </>
        )}
        {entry && <RankChip entry={entry} />}
        <AgentChip heapRef={heapRef} item={item} onTakenBack={onTakenBack} />
        <SendToAgentButton heapRef={heapRef} item={item} onSent={() => setSent((n) => n + 1)} />
        <ChangesAskedButton heapRef={heapRef} item={item} onSent={() => setSent((n) => n + 1)} />
      </div>
      {date && isToCome(item) && <p className="text-sm text-ink-soft">Joins the list on {date}</p>}
    </div>
  );
}

/**
 * The item's place in the list, "#3", which opens why it ranks there (ux.md, The item §9): the contributors with a bar
 * for each one's share of the item's priority, then what holds it back, as lines of text: the fields that can fix it
 * show it themselves. With nothing to say, as for the item first in its list, it is only text.
 */
function RankChip({ entry }: { entry: RankedItem }) {
  const chip = useRef<HTMLButtonElement>(null);
  const focusAfterRender = useFocusAfterRender();
  const [open, setOpen] = useState(false);
  const heldBackId = useId();
  const title = `Why it's #${entry.position}`;
  const hasContributors = entry.contributors.length > 0;
  if (!hasContributors && entry.held_back_by.length === 0) {
    return <span className={`${chipClass} pointer-coarse:min-h-0 pointer-coarse:px-2`}>#{entry.position}</span>;
  }
  return (
    <>
      <button
        ref={chip}
        type="button"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={title}
        onClick={() => setOpen(true)}
        className={chipClass}
      >
        #{entry.position}
      </button>
      <PopoverSheet
        title={title}
        triggerRef={chip}
        isOpen={open}
        onClose={() => {
          setOpen(false);
          focusAfterRender(() => chip.current);
        }}
        placement="bottom start"
        size="narrow"
      >
        {hasContributors && (
          <ul className="flex flex-col gap-2">
            {entry.contributors.map((contributor, index) => (
              // A factor can come up more than once, such as two parameters.
              <li key={index} className="flex flex-col gap-1">
                <span className="text-sm">
                  {contributor.label}
                  <span className="sr-only">, {contributor.share_percent}% of its priority</span>
                </span>
                <span aria-hidden="true" className="h-1 overflow-hidden rounded-full bg-line">
                  <span
                    className="block h-full rounded-full bg-accent"
                    style={{ width: `${contributor.share_percent}%` }}
                  />
                </span>
              </li>
            ))}
          </ul>
        )}
        {entry.held_back_by.length > 0 && (
          <div className={hasContributors ? "flex flex-col gap-2 border-t border-line pt-3" : "contents"}>
            <h3 id={heldBackId} className="text-xs font-medium text-ink-soft">
              Held back by
            </h3>
            <ul aria-labelledby={heldBackId} className="flex flex-col gap-1.5">
              {entry.held_back_by.map((held, index) => (
                <li key={index} className="text-sm">
                  {held.label}
                </li>
              ))}
            </ul>
          </div>
        )}
      </PopoverSheet>
    </>
  );
}
