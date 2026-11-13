import { Link } from "@tanstack/react-router";
import { ChevronDown, ChevronUp, X } from "lucide-react";
import { motion, useDragControls, type PanInfo } from "motion/react";
import { useEffect, useId, useRef, useState, type ReactNode } from "react";

import { ApiProblem, type Item, type ItemPatch, type ItemState, type RankedItem, type HeapRef } from "../api/client";
import { useItem, useUpdateItem } from "../api/queries";
import { gerund, takesBack } from "./agentConfirm";
import { HistoryDisclosure } from "./AuditHistory";
import { InlineConfirm } from "./InlineConfirm";
import { ContextDraft, type FlushDraft } from "./contextDraft";
import { ItemContext } from "./ItemContext";
import { ItemFieldList } from "./ItemFieldList";
import { itemProblem } from "./itemProblems";
import { ItemActions } from "./ItemMore";
import { ItemComments } from "./ItemComments";
import { ItemQuestions, useItemWaits, WaitsForYou } from "./ItemQuestions";
import { ItemSteps } from "./ItemSteps";
import { StatusLine } from "./ItemStatus";
import { isClosed } from "./labels";
import { PHONE, TABLET, TOUCH, useMedia } from "./media";
import { Sheet } from "./Sheet";
import { useFocusAfterRender } from "./useFocusAfterRender";
import { useUserName } from "./userName";

/** How far the sheet (down) or the panel (right) is dragged by its top bar before letting go closes it. */
const CLOSE_DISTANCE = 120;
const CLOSE_SPEED = 600;

/**
 * One item over its list (ux.md, The item): a pane beside the list on a wide screen, a panel over the right of
 * the list on a tablet, with the list still live beside it, and a sheet from the bottom on a phone. Each change saves
 * as it's made. `entry` is the item's place in the list on screen, when it's there, for why it ranks where it does.
 * `previousKey` and `nextKey` are the items above and below it on screen, for walking the list without closing the
 * pane (ux.md, The item §7); `listed` is whether the item is a row on screen, a reminder to come included, which
 * has no `entry`.
 */
export function ItemPane({
  heapRef,
  itemKey,
  entry,
  section,
  listed,
  previousKey,
  nextKey,
  onClose,
}: {
  heapRef: HeapRef;
  itemKey: string;
  entry: RankedItem | undefined;
  section: string | undefined;
  listed: boolean;
  previousKey: string | undefined;
  nextKey: string | undefined;
  onClose: () => void;
}) {
  const item = useItem(heapRef, itemKey);
  const paneRef = useRef<HTMLElement>(null);
  const headingId = useId();
  const drag = useDragToClose(onClose);
  // The ↑ or ↓ that walked to this item, so focus stays on it for the next step.
  const walkedBy = useRef<string | undefined>(undefined);

  // Focus moves to the item as it opens, so the keyboard carries on from there; Escape closes it (HeapPage).
  useEffect(() => {
    const label = walkedBy.current;
    walkedBy.current = undefined;
    const walk = label && paneRef.current?.querySelector<HTMLElement>(`[aria-label="${label}"]`);
    (walk || paneRef.current)?.focus({ preventScroll: true });
  }, [itemKey]);

  let body: ReactNode;
  if (item.isPending) {
    body = <p className="py-12 text-center text-muted">Loading…</p>;
  } else if (item.isError) {
    const missing = item.error instanceof ApiProblem && item.error.status === 404;
    body = (
      <p role="alert" className="py-12 text-center text-overdue-ink">
        {missing ? "This item doesn't exist, or it's in a heap you can't see." : item.error.message}
      </p>
    );
  } else {
    body = <ItemFields key={item.data.id} heapRef={heapRef} item={item.data} entry={entry} />;
  }

  return (
    <>
      {/* On a phone the sheet covers the list, and a tap on what shows of it closes the sheet. */}
      <div aria-hidden="true" onClick={onClose} className="fixed inset-0 z-40 bg-black/40 sm:hidden" />
      <motion.section
        ref={paneRef}
        tabIndex={-1}
        aria-labelledby={headingId}
        data-item-pane=""
        {...drag.paneProps}
        className="fixed inset-x-0 bottom-(--keyboard-inset) z-40 max-h-[calc(88dvh-var(--keyboard-inset))] overflow-y-auto rounded-t-2xl bg-surface text-ink shadow-2xl outline-none sm:top-0 sm:bottom-0 sm:left-auto sm:z-20 sm:max-h-none sm:w-[25rem] sm:rounded-none sm:border-l sm:border-line lg:sticky lg:right-auto lg:bottom-auto lg:h-dvh lg:w-[26rem] lg:shrink-0 lg:shadow-none"
      >
        {/* The top bar is what drags: down to close the phone's sheet, right to close the tablet's panel. */}
        <div
          {...drag.handleProps}
          className="sticky top-0 z-10 flex items-center gap-2 bg-surface px-4 pt-3 pb-2 sm:px-5 sm:pt-5"
        >
          {drag.enabled && (
            <span
              aria-hidden="true"
              className="absolute top-1.5 left-1/2 h-1 w-10 -translate-x-1/2 rounded-full bg-line sm:hidden"
            />
          )}
          <h2 id={headingId} className="min-h-6 flex-1 text-xs font-medium text-muted">
            <span className="sr-only">Item</span>
            {entry && section && <span aria-hidden="true">{section}</span>}
          </h2>
          {/* On a phone the sheet closes and the next row is a tap away, so only the pane walks the list. */}
          {listed && (
            <div className="flex max-sm:hidden">
              {(["Previous item", "Next item"] as const).map((label) => (
                <WalkLink
                  key={label}
                  heapRef={heapRef}
                  itemKey={label === "Previous item" ? previousKey : nextKey}
                  label={label}
                  icon={label === "Previous item" ? ChevronUp : ChevronDown}
                  onWalk={() => (walkedBy.current = label)}
                />
              ))}
            </div>
          )}
          <span className="px-2 font-mono text-xs text-ink-soft">{itemKey}</span>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close"
            aria-keyshortcuts="Escape"
            className="flex size-9 items-center justify-center rounded-lg text-ink-soft hover:bg-sunken pointer-coarse:size-11"
          >
            <X aria-hidden="true" className="size-4" />
          </button>
        </div>
        <div className="px-4 pb-[max(env(safe-area-inset-bottom),1.5rem)] has-[[data-state-bar]]:pb-0 sm:px-5">
          {body}
        </div>
      </motion.section>
    </>
  );
}

/**
 * A header button that opens the item above or below this one, in place of it. Disabled at either end of the list,
 * and still focusable there, so focus that walked to the end stays put and a screen reader still finds it.
 */
function WalkLink({
  heapRef,
  itemKey,
  label,
  icon: Icon,
  onWalk,
}: {
  heapRef: HeapRef;
  itemKey: string | undefined;
  label: string;
  icon: typeof ChevronUp;
  onWalk: () => void;
}) {
  const className =
    "flex size-9 items-center justify-center rounded-lg text-ink-soft hover:bg-sunken aria-disabled:opacity-40 aria-disabled:hover:bg-transparent";
  if (itemKey === undefined) {
    return (
      <span role="link" tabIndex={0} aria-disabled="true" aria-label={label} className={className}>
        <Icon aria-hidden="true" className="size-4" />
      </span>
    );
  }
  return (
    <Link
      to="/teams/$team/heaps/$heap/items/$item"
      params={{ ...heapRef, item: itemKey }}
      search={true}
      // In place of this item, so Back still closes the pane rather than walking back up the list.
      replace
      state={(prev) => prev}
      aria-label={label}
      onClick={onWalk}
      className={className}
    >
      <Icon aria-hidden="true" className="size-4" />
    </Link>
  );
}

/**
 * Closing by drag, with a finger: the phone's sheet comes up from the bottom and drags down, the tablet's panel drags
 * right. Only the top bar starts a drag, so the fields scroll as usual. On a wide screen the pane stays put.
 */
function useDragToClose(onClose: () => void) {
  const phone = useMedia(PHONE);
  const tablet = useMedia(TABLET);
  const touch = useMedia(TOUCH);
  const controls = useDragControls();
  const axis = phone ? "y" : tablet && touch ? "x" : null;

  const onDragEnd = (_event: unknown, info: PanInfo) => {
    const distance = axis === "y" ? info.offset.y : info.offset.x;
    const speed = axis === "y" ? info.velocity.y : info.velocity.x;
    if (distance > CLOSE_DISTANCE || speed > CLOSE_SPEED) onClose();
  };

  if (axis === null) return { enabled: false, paneProps: {}, handleProps: {} };
  return {
    enabled: true,
    paneProps: {
      drag: axis,
      dragControls: controls,
      dragListener: false,
      dragConstraints: { top: 0, bottom: 0, left: 0, right: 0 },
      dragElastic: axis === "y" ? { top: 0, bottom: 1 } : { left: 0, right: 1 },
      dragSnapToOrigin: true,
      onDragEnd,
      ...(phone && {
        initial: { y: "100%" },
        animate: { y: 0 },
        transition: { type: "spring" as const, bounce: 0, duration: 0.3 },
      }),
    },
    handleProps: {
      onPointerDown: (event: React.PointerEvent) => controls.start(event),
      style: { touchAction: "none" },
    },
  } as const;
}

/**
 * The item as it reads (ux.md, The item): the title with its main state button and ⋯, the status line, what waits
 * for the person, the fields, the context, the steps, the questions and the history. Each change saves on its own, with
 * the item's version in `If-Match`.
 */
function ItemFields({ heapRef, item, entry }: { heapRef: HeapRef; item: Item; entry: RankedItem | undefined }) {
  const phone = useMedia(PHONE);
  const update = useUpdateItem(heapRef);
  const waits = useItemWaits(item);
  const focusAfterRender = useFocusAfterRender();

  const closed = isClosed(item.state);
  const canEdit = item.allowed_actions.includes("edit") && !closed;
  const canReopen = item.allowed_actions.includes("reopen");
  const save = (patch: ItemPatch) => update.mutate({ item, patch });
  // A change to a state that takes the item back from the agent that has it asks first (ux.md, The item); the server
  // does the take back with the change.
  const [askState, setAskState] = useState<ItemState | null>(null);
  const asker = useRef<HTMLElement | null>(null);
  // The context form's way to save its draft, while one is open. A change that closes the item goes after it, so text
  // typed for the close isn't lost to the item turning read-only.
  const [flushDraft, setFlushDraft] = useState<FlushDraft | null>(null);
  const changeState = (state: ItemState) => {
    if (state === item.state) return;
    if (flushDraft !== null && (state === "done" || state === "wont_do")) {
      void flushDraft().then((saved) => saved && changeStateNow(state));
    } else {
      changeStateNow(state);
    }
  };
  const changeStateNow = (state: ItemState) => {
    if (takesBack(item, state)) {
      asker.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      setAskState(state);
    } else {
      save({ state });
    }
  };
  const cancelAsking = () => {
    setAskState(null);
    focusAfterRender(() => asker.current);
  };
  const confirmAsking = askState !== null && (
    <TakeBackAndChange
      state={askState}
      phone={phone}
      title={item.title}
      pending={update.isPending}
      onConfirm={() => {
        save({ state: askState });
        setAskState(null);
      }}
      onCancel={cancelAsking}
    />
  );
  // Once the agent chip is gone with its turn, focus goes to the ⋯ that is left.
  const focusMore = () =>
    focusAfterRender(() => document.querySelector<HTMLElement>("[data-item-pane] button[aria-label='More']"));

  const saveTitle = (text: string) => {
    if (text.trim() !== "" && text !== item.title) save({ title: text });
  };
  // Awaitable, and apart from `update`, so the context form says its own failure and the pane doesn't say it twice.
  const saveDescription = useUpdateItem(heapRef);
  const saveContext = async (text: string) => {
    const description = text.trim() === "" ? null : text;
    if (description !== (item.description ?? null)) await saveDescription.mutateAsync({ item, patch: { description } });
  };

  const actions = (variant: "inline" | "bar") => (
    <ItemActions heapRef={heapRef} item={item} variant={variant} save={save} onState={changeState} />
  );

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-1.5">
        <div className="flex items-start gap-2">
          {/* Keyed by the saved text, so a save from elsewhere shows up here. Labelled by attribute, as a wrapping label
            would take the text typed in as part of its own. */}
          <textarea
            aria-label="Title"
            key={item.title}
            defaultValue={item.title}
            disabled={!canEdit}
            rows={1}
            onBlur={(e) => saveTitle(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                e.currentTarget.blur();
              } else if (e.key === "Escape") {
                // Undoes the typing, and keeps the item open.
                e.preventDefault();
                e.currentTarget.value = item.title;
                e.currentTarget.blur();
              }
            }}
            className="field-sizing-content -mx-1.5 min-w-0 flex-1 resize-none rounded-lg bg-transparent px-1.5 py-0.5 text-lg leading-snug font-semibold outline-none enabled:hover:bg-sunken disabled:opacity-100"
          />
          {!phone && actions("inline")}
        </div>
        {!phone && confirmAsking}
        <StatusLine heapRef={heapRef} item={item} entry={entry} canReopen={canReopen} onTakenBack={focusMore} />
      </div>

      <WaitsForYou heapRef={heapRef} item={item} waits={waits} />

      <ItemFieldList heapRef={heapRef} item={item} entry={entry} canEdit={canEdit} save={save} />
      {update.error && (
        <p role="alert" className="text-sm text-overdue-ink">
          {itemProblem(update.error)}
        </p>
      )}

      <ContextDraft.Provider value={setFlushDraft}>
        <ItemContext item={item} canEdit={canEdit} onSave={saveContext} />
      </ContextDraft.Provider>

      <ItemComments heapRef={heapRef} item={item} />

      <ItemSteps heapRef={heapRef} item={item} />

      <ItemFrom item={item} />

      <ItemQuestions heapRef={heapRef} item={item} waits={waits} />

      {/* History starts collapsed and loads once opened; each item opens with it collapsed again. Who added the
          item stays in view, on the same line as History's toggle. */}
      <section aria-label="History" className="relative border-t border-line pt-3">
        <HistoryDisclosure scope={{ heap: heapRef }} entityId={item.id} className="" />
        <div className="absolute top-3 right-0 flex h-9 items-center pointer-coarse:h-11">
          <AddedBy item={item} />
        </div>
      </section>

      {phone && actions("bar")}
      {phone && confirmAsking}
    </div>
  );
}

/**
 * Asks before a state change takes the item back from the agent that has it: inline under the title on a wide
 * screen and a tablet, a sheet on a phone.
 */
function TakeBackAndChange({
  state,
  phone,
  title,
  pending,
  onConfirm,
  onCancel,
}: {
  state: ItemState;
  phone: boolean;
  title: string;
  pending: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const questionId = useId();
  const verb = state === "in_progress" ? "start" : "close";
  const question = `An agent has this. ${gerund(state)} it takes it back. Take it back and ${verb} it?`;
  const confirmLabel = `Take back and ${verb}`;
  if (!phone) {
    return (
      <InlineConfirm
        question={question}
        confirmLabel={confirmLabel}
        pending={pending}
        error={null}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );
  }
  return (
    <Sheet
      title="Take back from the agent?"
      role="alertdialog"
      describedBy={questionId}
      isOpen
      onClose={onCancel}
      aside={title}
    >
      <div className="flex flex-col gap-2 pb-1">
        <p id={questionId} className="px-1 py-1 text-[15px] text-ink">
          {question}
        </p>
        <button
          type="button"
          autoFocus
          disabled={pending}
          onClick={onConfirm}
          className="min-h-13 rounded-xl bg-accent text-[15px] font-semibold text-stone-950 disabled:opacity-40"
        >
          {confirmLabel}
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="min-h-13 rounded-xl border border-line text-[15px] text-ink"
        >
          Cancel
        </button>
      </div>
    </Sheet>
  );
}

/**
 * Who added the item and how long ago, as the server labels it: "Added by Sam 4 days ago", or "by you" for the
 * viewer's own, as History says "You".
 */
function AddedBy({ item }: { item: Item }) {
  const by = useUserName(item.created_by);
  return (
    <p className="text-xs text-muted">
      Added{by && ` by ${by}`} <time dateTime={new Date(item.created_at).toISOString()}>{item.created_label}</time>
    </p>
  );
}

/**
 * Where the item came from (work-with-agents, W8): `From: "Rotate the certs", a note`, linking to that item, and
 * " · added by an agent" when a run added it. An item whose origin the viewer can't see shows "Added by an agent"
 * alone. Nothing when it has neither.
 */
function ItemFrom({ item }: { item: Item }) {
  const { origin, added_in_run: addedInRun } = item;
  if (!origin && !addedInRun) return null;
  if (!origin) return <p className="text-sm text-ink-soft">Added by an agent</p>;
  return (
    <p className="text-sm text-ink-soft">
      From:{" "}
      <Link
        to="/teams/$team/heaps/$heap/items/$item"
        params={{ team: origin.heap.team_slug, heap: origin.heap.slug, item: origin.key }}
        search={true}
        className="text-accent-ink underline"
      >
        “{origin.title}”, a {origin.kind}
      </Link>
      {addedInRun && " · added by an agent"}
    </p>
  );
}
