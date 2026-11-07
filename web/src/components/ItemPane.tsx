import { Navigate } from "@tanstack/react-router";
import { X } from "lucide-react";
import { motion, useDragControls, type PanInfo } from "motion/react";
import { useEffect, useId, useRef, type ReactNode } from "react";
import { ToggleButton, ToggleButtonGroup, type Key } from "react-aria-components";

import { ApiProblem, type Item, type ItemPatch, type ItemState, type RankedItem } from "../api/client";
import { useBlockerCandidates, useItem, useParameters, useUpdateItem } from "../api/queries";
import { AuditHistory } from "./AuditHistory";
import { BlockerSelect } from "./BlockerSelect";
import { blockerCandidatesFor } from "./blockerCandidates";
import { choiceButtonClass, ChoiceButtons, FieldLabel } from "./ChoiceButtons";
import { DuePicker } from "./DuePicker";
import { itemProblem } from "./itemProblems";
import { isClosed, SIZE_LABELS, SIZES, STATE_LABELS } from "./labels";
import { PHONE, TABLET, TOUCH, useMedia } from "./media";
import { ParameterPicker } from "./ParameterPicker";
import { TagPicker } from "./TagPicker";

const SIZE_OPTIONS = SIZES.map((size) => ({ id: size, label: SIZE_LABELS[size] }));

/** The states the segmented control offers. Won't do sits apart, as it's rarely the one wanted. */
const SEGMENTS: readonly ItemState[] = ["open", "in_progress", "blocked", "done"];

/** How far the sheet (down) or the panel (right) is dragged by its top bar before letting go closes it. */
const CLOSE_DISTANCE = 120;
const CLOSE_SPEED = 600;

/**
 * One item over its list (app-ux.md, The item): a pane beside the list on a wide screen, a panel over the right of
 * the list on a tablet, with the list still live beside it, and a sheet from the bottom on a phone. Each change saves
 * as it's made. `entry` is the item's place in the list on screen, when it's there, for why it ranks where it does.
 */
export function ItemPane({
  heapId,
  itemId,
  entry,
  section,
  onClose,
}: {
  heapId: string;
  itemId: string;
  entry: RankedItem | undefined;
  section: string | undefined;
  onClose: () => void;
}) {
  const item = useItem(itemId);
  const paneRef = useRef<HTMLElement>(null);
  const headingId = useId();
  const drag = useDragToClose(onClose);

  // Focus moves to the item as it opens, so the keyboard carries on from there; Escape closes it (HeapPage).
  useEffect(() => {
    paneRef.current?.focus({ preventScroll: true });
  }, [itemId]);

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
  } else if (item.data.heap_id !== heapId) {
    body = <Navigate to="/heaps/$heapId/items/$itemId" params={{ heapId: item.data.heap_id, itemId }} replace />;
  } else {
    body = <ItemFields key={item.data.id} item={item.data} entry={entry} />;
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
          <h2 id={headingId} className="flex-1 text-xs font-medium text-muted">
            <span className="sr-only">Item</span>
            {entry && (
              <span aria-hidden="true">
                #{entry.position}
                {section && ` · ${section}`}
              </span>
            )}
          </h2>
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
        <div className="px-4 pb-[max(env(safe-area-inset-bottom),1.5rem)] sm:px-5">{body}</div>
      </motion.section>
    </>
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

/** The item's fields, each saving on its own with the item's version in `If-Match`. */
function ItemFields({ item, entry }: { item: Item; entry: RankedItem | undefined }) {
  const update = useUpdateItem(item.heap_id);
  const parameters = useParameters(item.heap_id);
  const candidates = useBlockerCandidates(item.heap_id, item.state === "blocked");
  const descriptionId = useId();

  const closed = isClosed(item.state);
  const canEdit = item.allowed_actions.includes("edit") && !closed;
  const canReopen = item.allowed_actions.includes("reopen");
  const canChangeState = item.allowed_actions.includes("change_state") || canReopen;
  const save = (patch: ItemPatch) => update.mutate({ item, patch });

  const saveText = (field: "title" | "description", text: string) => {
    if (field === "title") {
      if (text.trim() !== "" && text !== item.title) save({ title: text });
    } else {
      const description = text.trim() === "" ? null : text;
      if (description !== (item.description ?? null)) save({ description });
    }
  };

  return (
    <div className="flex flex-col gap-5">
      {/* Keyed by the saved text, so a save from elsewhere shows up here. Labelled by attribute, as a wrapping label
          would take the text typed in as part of its own. */}
      <textarea
        aria-label="Title"
        key={item.title}
        defaultValue={item.title}
        disabled={!canEdit}
        rows={1}
        onBlur={(e) => saveText("title", e.target.value)}
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
        className="field-sizing-content resize-none bg-transparent text-lg leading-snug font-semibold outline-none disabled:opacity-100"
      />

      <StateControl
        item={item}
        disabled={!canChangeState}
        canReopen={canReopen}
        onChange={(state) => state !== item.state && save({ state })}
      />
      {closed && item.closed_label && (
        <p className="-mt-3 text-xs text-muted">
          {STATE_LABELS[item.state]} · {item.closed_label}
          {!canReopen && ". It can't be reopened."}
        </p>
      )}
      {item.state === "blocked" && (
        <BlockerSelect
          value={item.blocked_by}
          candidates={blockerCandidatesFor(candidates.data ?? [])(item.id)}
          disabled={!canChangeState}
          onChange={(blockerId) => save({ blocked_by_item_id: blockerId })}
        />
      )}

      {entry && entry.contributors.length > 0 && (
        <div className="flex flex-col gap-1 rounded-lg bg-sunken px-3 py-2">
          <span className="text-xs font-medium text-ink-soft">Why it's #{entry.position}</span>
          <span className="text-sm">{entry.contributors.map((c) => c.label).join(" · ")}</span>
        </div>
      )}

      <div className="grid grid-cols-2 gap-3">
        <ChoiceButtons
          label="Effort"
          options={SIZE_OPTIONS}
          value={item.effort ?? null}
          disabled={!canEdit}
          onChange={(effort) => save({ effort })}
        />
        <ChoiceButtons
          label="Impact"
          options={SIZE_OPTIONS}
          value={item.impact ?? null}
          disabled={!canEdit}
          onChange={(impact) => save({ impact })}
        />
      </div>

      <DuePicker
        value={item.due ? { date: item.due.date, firm: item.due.firm } : null}
        current={item.due ? (item.due.overdue ? `Overdue · ${item.due.label}` : item.due.label) : undefined}
        disabled={!canEdit}
        onChange={(due) => {
          if (due?.date !== item.due?.date || due?.firm !== item.due?.firm) save({ due });
        }}
      />

      <TagPicker
        heapId={item.heap_id}
        value={item.tags}
        disabled={!canEdit}
        onChange={(tags) => save({ tag_ids: tags.map((tag) => tag.id) })}
      />

      {item.parameters.length > 0 && (
        <div className="flex flex-col gap-3" role="group" aria-label="Parameters">
          {item.parameters.map((current) => {
            const parameter = parameters.data?.parameters.find((p) => p.id === current.parameter_id);
            // An archived parameter, or an archived option a closed item keeps, can only be looked at.
            const archivedOption =
              current.option !== null && !parameter?.options.some((option) => option.id === current.option?.id);
            return parameter && !archivedOption ? (
              <ParameterPicker
                key={current.parameter_id}
                parameter={parameter}
                value={current.option?.id ?? null}
                disabled={!canEdit}
                onChange={(option) => save({ parameter_values: { [current.parameter_id]: option } })}
              />
            ) : (
              <p key={current.parameter_id} className="text-sm">
                <span className="text-xs font-medium text-param-ink">
                  {current.name}
                  {current.parameter_archived && " (archived)"}
                </span>{" "}
                {current.option ? `${current.option.label}${archivedOption ? " (archived)" : ""}` : "Not set"}
              </p>
            );
          })}
        </div>
      )}

      <div className="flex flex-col gap-1.5">
        <label htmlFor={descriptionId} className="text-xs font-medium text-ink-soft">
          Description
        </label>
        <textarea
          id={descriptionId}
          key={item.description ?? ""}
          defaultValue={item.description ?? ""}
          disabled={!canEdit}
          rows={3}
          onBlur={(e) => saveText("description", e.target.value)}
          className="field-sizing-content min-h-20 rounded-lg border border-line bg-surface px-2 py-1.5 text-sm disabled:opacity-60"
        />
      </div>

      {update.error && (
        <p role="alert" className="text-sm text-overdue-ink">
          {itemProblem(update.error)}
        </p>
      )}

      <section aria-label="History" className="flex flex-col gap-1.5 border-t border-line pt-4">
        <h3 className="text-xs font-medium text-ink-soft">History</h3>
        <AuditHistory scope={{ heapId: item.heap_id }} entityId={item.id} />
      </section>
    </div>
  );
}

/**
 * The item's state as a segmented control, with Won't do beside it. A closed item that can be reopened offers the
 * open states; picking one reopens it.
 */
function StateControl({
  item,
  disabled,
  canReopen,
  onChange,
}: {
  item: Item;
  disabled: boolean;
  canReopen: boolean;
  onChange: (state: ItemState) => void;
}) {
  const labelId = useId();
  const closed = isClosed(item.state);
  // From a closed state only the open ones lead anywhere, and only when the item can still be reopened.
  const unavailable = closed ? SEGMENTS.filter((state) => isClosed(state) || !canReopen) : [];
  const onSelectionChange = (keys: Set<Key>) => {
    const [picked] = [...keys];
    if (picked !== undefined) onChange(picked as ItemState);
  };

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center justify-between gap-2">
        <FieldLabel id={labelId}>State</FieldLabel>
        {!closed && (
          <button
            type="button"
            disabled={disabled}
            onClick={() => onChange("wont_do")}
            className="-my-2 h-8 rounded-lg px-2 text-xs text-ink-soft hover:bg-sunken disabled:opacity-50 pointer-coarse:h-11"
          >
            Won't do
          </button>
        )}
      </div>
      <ToggleButtonGroup
        aria-labelledby={labelId}
        selectionMode="single"
        disallowEmptySelection
        selectedKeys={[item.state]}
        onSelectionChange={onSelectionChange}
        isDisabled={disabled}
        className="grid grid-cols-4 gap-0.5 rounded-xl bg-sunken p-0.5"
      >
        {SEGMENTS.map((state) => (
          <ToggleButton
            key={state}
            id={state}
            isDisabled={unavailable.includes(state)}
            className={`${choiceButtonClass} rounded-lg border-transparent bg-transparent px-1 whitespace-nowrap data-[selected]:border-transparent data-[selected]:bg-surface data-[selected]:font-medium data-[selected]:text-ink data-[selected]:shadow-sm ${
              state === "done" ? "data-[selected]:text-done" : ""
            }`}
          >
            {STATE_LABELS[state]}
          </ToggleButton>
        ))}
      </ToggleButtonGroup>
    </div>
  );
}
