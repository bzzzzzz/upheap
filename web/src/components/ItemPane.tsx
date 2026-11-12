import { Link } from "@tanstack/react-router";
import { Check, ChevronDown, ChevronRight, ChevronUp, Link2, X } from "lucide-react";
import { motion, useDragControls, type PanInfo } from "motion/react";
import { Fragment, useEffect, useId, useRef, useState, type ReactNode } from "react";
import { ToggleButtonGroup } from "react-aria-components";

import { ApiProblem, type Item, type ItemPatch, type ItemState, type RankedItem, type HeapRef } from "../api/client";
import { useBlockerCandidates, useItem, useParameters, useUpdateItem } from "../api/queries";
import { gerund, takesBack } from "./agentConfirm";
import { AuditHistory } from "./AuditHistory";
import { BlockerSelect } from "./BlockerSelect";
import { useCopyLink } from "./copyLink";
import { blockerCandidatesFor } from "./blockerCandidates";
import { ChoiceButtons, FieldLabel } from "./ChoiceButtons";
import { DateField } from "./DateField";
import { DuePicker, DueRows } from "./DuePicker";
import { InlineConfirm } from "./InlineConfirm";
import { itemProblem } from "./itemProblems";
import { ItemMore, MakeTaskButton } from "./ItemMore";
import { AnsweredQuestions, WaitsForYou } from "./ItemQuestions";
import { ItemSteps } from "./ItemSteps";
import { ItemTurn } from "./ItemTurn";
import { KindIcon, StageIcon } from "./kindIcons";
import {
  changeLabel,
  isClosed,
  KIND_LABELS,
  NEXT_STATES,
  SIZE_LABELS,
  SIZES,
  STAGE_LABELS,
  STATE_LABELS,
} from "./labels";
import { PHONE, TABLET, TOUCH, useMedia } from "./media";
import { formatDate, isToCome } from "./reminders";
import { NeedPicker } from "./NeedPicker";
import { ParameterPicker } from "./ParameterPicker";
import { Sheet, SheetChoice } from "./Sheet";
import { useFocusAfterRender } from "./useFocusAfterRender";
import { TagPicker } from "./TagPicker";
import { useUserName } from "./userName";

const SIZE_OPTIONS = SIZES.map((size) => ({ id: size, label: SIZE_LABELS[size] }));

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
          <h2 id={headingId} className="flex-1 text-xs font-medium text-muted">
            <span className="sr-only">Item</span>
            {entry && (
              <span aria-hidden="true">
                #{entry.position}
                {section && ` · ${section}`}
              </span>
            )}
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

/** The item's fields, each saving on its own with the item's version in `If-Match`. */
function ItemFields({ heapRef, item, entry }: { heapRef: HeapRef; item: Item; entry: RankedItem | undefined }) {
  const phone = useMedia(PHONE);
  const update = useUpdateItem(heapRef);
  const parameters = useParameters(heapRef);
  const candidates = useBlockerCandidates(heapRef, item.state === "blocked");
  const descriptionId = useId();
  // The fields a "Held back by" entry takes the user to, and the phone's open sheet, which the Due entry opens.
  const needId = useId();
  const effortId = useId();
  const impactId = useId();
  const dueId = useId();
  const [openSheet, setOpenSheet] = useState<string | null>(null);
  const focusAfterRender = useFocusAfterRender();
  // Set when the Due sheet was opened from "Held back by", whose row goes away once the due date is set.
  const dueFromHeldBack = useRef(false);
  const setSheet = (field: string | null) => {
    setOpenSheet(field);
    if (field === null && dueFromHeldBack.current) {
      dueFromHeldBack.current = false;
      focusAfterRender(() => document.getElementById(dueId));
    }
  };

  const closed = isClosed(item.state);
  const canEdit = item.allowed_actions.includes("edit") && !closed;
  const canReopen = item.allowed_actions.includes("reopen");
  const canChangeState = item.allowed_actions.includes("change_state") || canReopen;
  const save = (patch: ItemPatch) => update.mutate({ item, patch });
  // A change to a state that takes the item back from the agent that has it asks first (ux.md, The item); the server
  // does the take back with the change.
  const [askState, setAskState] = useState<ItemState | null>(null);
  const asker = useRef<HTMLElement | null>(null);
  const saveAsking = (patch: ItemPatch) => {
    if (patch.state != null && takesBack(item, patch.state)) {
      asker.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      setAskState(patch.state);
    } else {
      save(patch);
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

  const fix = (gap: HeldBackGap) => {
    if (gap === "need") focusField(needId, "[data-selected]", "button");
    else if (gap === "size") focusField(item.effort ? impactId : effortId, "button");
    else if (phone) {
      dueFromHeldBack.current = true;
      setOpenSheet("due");
    } else focusField(dueId, "button", "input");
  };

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
      <div className="flex items-start gap-1">
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
          className="field-sizing-content min-w-0 flex-1 resize-none bg-transparent text-lg leading-snug font-semibold outline-none disabled:opacity-100"
        />
        <CopyItemLink heapRef={heapRef} itemKey={item.key} />
      </div>

      <KindLine item={item} />

      {phone ? (
        <StateLine item={item} canReopen={canReopen} />
      ) : (
        <StateActions
          item={item}
          disabled={!canChangeState}
          canReopen={canReopen}
          onChange={(state) => state !== item.state && saveAsking({ state })}
          save={saveAsking}
        />
      )}
      {!phone && confirmAsking}
      {item.state === "blocked" && (
        <BlockerSelect
          value={item.blocked_by}
          candidates={blockerCandidatesFor(candidates.data ?? [])(item.id)}
          disabled={!canChangeState}
          onChange={(blockerId) => save({ blocked_by_item_id: blockerId })}
        />
      )}

      <WaitsForYou heapRef={heapRef} item={item} />

      <ItemTurn heapRef={heapRef} item={item} />

      {entry && (entry.contributors.length > 0 || entry.held_back_by.length > 0) && (
        <WhyItRanks entry={entry} onFix={canEdit ? fix : undefined} phone={phone} />
      )}

      <ItemSteps heapRef={heapRef} item={item} />

      <div id={needId} className="contents">
        <NeedPicker
          value={item.need}
          disabled={!canEdit}
          onChange={(need) => {
            if (need !== item.need) save({ need });
          }}
        />
      </div>

      <div className="grid gap-3 sm:grid-cols-2">
        <div id={effortId} className="contents">
          <ChoiceButtons
            label="Effort"
            options={SIZE_OPTIONS}
            value={item.effort ?? null}
            disabled={!canEdit}
            onChange={(effort) => save({ effort })}
          />
        </div>
        <div id={impactId} className="contents">
          <ChoiceButtons
            label="Impact"
            options={SIZE_OPTIONS}
            value={item.impact ?? null}
            disabled={!canEdit}
            onChange={(impact) => save({ impact })}
          />
        </div>
      </div>

      {phone ? (
        <FieldRows
          heapRef={heapRef}
          item={item}
          canEdit={canEdit}
          save={save}
          saveText={saveText}
          open={openSheet}
          setOpen={setSheet}
          dueRowId={dueId}
        />
      ) : (
        <>
          <div id={dueId} className="contents">
            {item.kind === "reminder" ? (
              // A reminder's date is its deadline: an exact date that can be moved but not cleared.
              <DateField
                value={item.due?.date ?? null}
                disabled={!canEdit}
                onChange={(date) => save({ due: { date, firm: true } })}
              />
            ) : (
              <DuePicker
                value={item.due ? { date: item.due.date, firm: item.due.firm } : null}
                current={item.due ? (item.due.overdue ? `Overdue · ${item.due.label}` : item.due.label) : undefined}
                disabled={!canEdit}
                onChange={(due) => {
                  if (due?.date !== item.due?.date || due?.firm !== item.due?.firm) save({ due });
                }}
              />
            )}
          </div>

          <TagPicker
            heapRef={heapRef}
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
              Context
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
        </>
      )}

      {update.error && (
        <p role="alert" className="text-sm text-overdue-ink">
          {itemProblem(update.error)}
        </p>
      )}

      <AnsweredQuestions item={item} />

      <section aria-label="History" className="flex flex-col gap-1.5 border-t border-line pt-4">
        <div className="flex items-baseline justify-between gap-3">
          <h3 className="text-xs font-medium text-ink-soft">History</h3>
          <AddedBy item={item} />
        </div>
        <AuditHistory scope={{ heap: heapRef }} entityId={item.id} />
      </section>

      {phone && (
        <StateActions
          variant="bar"
          item={item}
          disabled={!canChangeState}
          canReopen={canReopen}
          onChange={(state) => state !== item.state && saveAsking({ state })}
          save={saveAsking}
        />
      )}
      {phone && confirmAsking}
    </div>
  );
}

/**
 * Asks before a state change takes the item back from the agent that has it: inline beside the state buttons on a wide
 * screen, a sheet on a phone.
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

/** The item's longer fields on a phone, as rows of label and value, each opening a sheet to change it. */
function FieldRows({
  heapRef,
  item,
  canEdit,
  save,
  saveText,
  open,
  setOpen,
  dueRowId,
}: {
  heapRef: HeapRef;
  item: Item;
  canEdit: boolean;
  save: (patch: ItemPatch) => void;
  saveText: (field: "title" | "description", text: string) => void;
  /** Which sheet is open, kept by the caller so "Held back by" can open the Due sheet. */
  open: string | null;
  setOpen: (field: string | null) => void;
  /** The id of the Due row, where focus goes when its sheet was opened from "Held back by". */
  dueRowId: string;
}) {
  const parameters = useParameters(heapRef);
  const close = () => setOpen(null);
  const sheet = (field: string) => ({ isOpen: open === field, onClose: close, aside: item.title });

  const due = item.due ? (item.due.overdue ? `Overdue · ${item.due.label}` : item.due.label) : null;
  const reminder = item.kind === "reminder";
  const tags = item.tags.length > 0 ? item.tags.map((tag) => tag.name).join(", ") : null;

  return (
    <div className="-mt-2 flex flex-col border-t border-line/60">
      <FieldRow
        id={dueRowId}
        label={reminder ? "Date" : "Due"}
        value={reminder ? (item.due ? formatDate(item.due.date) : null) : due}
        disabled={!canEdit}
        onOpen={() => setOpen("due")}
      />
      <FieldRow label="Tags" value={tags} disabled={!canEdit} onOpen={() => setOpen("tags")} />
      {item.parameters.map((current) => {
        const parameter = parameters.data?.parameters.find((p) => p.id === current.parameter_id);
        // An archived parameter, or an archived option a closed item keeps, can only be looked at.
        const archivedOption =
          current.option !== null && !parameter?.options.some((option) => option.id === current.option?.id);
        const editable = canEdit && parameter !== undefined && !archivedOption;
        const label = `${current.name}${current.parameter_archived ? " (archived)" : ""}`;
        const value = current.option ? `${current.option.label}${archivedOption ? " (archived)" : ""}` : null;
        return (
          <Fragment key={current.parameter_id}>
            <FieldRow
              label={label}
              value={value}
              empty="Not set"
              tone="param"
              disabled={!editable}
              onOpen={() => setOpen(current.parameter_id)}
            />
            {parameter && (
              <Sheet title={parameter.name} {...sheet(current.parameter_id)}>
                <OptionRows
                  label={parameter.name}
                  options={parameter.options}
                  value={current.option?.id ?? null}
                  onChange={(option) => save({ parameter_values: { [current.parameter_id]: option } })}
                  onDone={close}
                />
              </Sheet>
            )}
          </Fragment>
        );
      })}
      <FieldRow
        label="Context"
        // The first line names it; the sheet, or the read-only row, shows all of it.
        value={(canEdit ? item.description?.split("\n")[0] : item.description) ?? null}
        disabled={!canEdit}
        onOpen={() => setOpen("description")}
      />

      <Sheet title={reminder ? "Date" : "Due"} {...sheet("due")}>
        {reminder ? (
          <div className="pb-2">
            <DateField
              wide
              label="Date"
              value={item.due?.date ?? null}
              onChange={(date) => save({ due: { date, firm: true } })}
            />
          </div>
        ) : (
          <DueRows
            value={item.due ? { date: item.due.date, firm: item.due.firm } : null}
            onChange={(value) => {
              if (value?.date !== item.due?.date || value?.firm !== item.due?.firm) save({ due: value });
            }}
            onDone={close}
          />
        )}
      </Sheet>
      <Sheet title="Tags" {...sheet("tags")}>
        <div className="pt-1 pb-2">
          <TagPicker
            heapRef={heapRef}
            value={item.tags}
            inline
            onChange={(picked) => save({ tag_ids: picked.map((tag) => tag.id) })}
          />
        </div>
      </Sheet>
      <Sheet title="Context" {...sheet("description")}>
        <ContextEditor context={item.description ?? ""} onSave={(text) => saveText("description", text)} />
      </Sheet>
    </div>
  );
}

/**
 * A field as a phone shows it: its label, and its value or what it reads as unset, with a chevron to change it. A
 * field that can't be changed is a plain row showing all of its value, as there's no sheet to read the rest in.
 */
function FieldRow({
  id,
  label,
  value,
  empty = "None",
  tone = "ink",
  disabled,
  onOpen,
}: {
  id?: string;
  label: string;
  value: string | null;
  empty?: string;
  tone?: "ink" | "param";
  disabled: boolean;
  onOpen: () => void;
}) {
  const rowClass = "flex min-h-13 gap-3 border-b border-line/60 text-left text-[15px]";
  const labelClass = `shrink-0 ${tone === "param" ? "text-param-ink" : "text-ink-soft"}`;
  const valueClass = value === null ? "text-muted" : "text-ink";
  if (disabled) {
    return (
      <div className={`${rowClass} items-baseline py-3.5`}>
        <span className={labelClass}>{label}</span>
        <span className={`min-w-0 flex-1 text-right break-words whitespace-pre-line ${valueClass}`}>
          {value ?? empty}
        </span>
      </div>
    );
  }
  return (
    <button
      id={id}
      type="button"
      onClick={onOpen}
      className={`${rowClass} items-center outline-none focus-visible:outline-2 focus-visible:outline-accent`}
    >
      <span className={labelClass}>{label}</span>
      <span className={`min-w-0 flex-1 truncate text-right ${valueClass}`}>{value ?? empty}</span>
      <ChevronRight aria-hidden="true" className="size-4 shrink-0 text-muted" />
    </button>
  );
}

/** The value a choice row stands for when it clears the field. */
const NOT_SET = "";

/**
 * One option out of a list, as 52 px rows with a check on the picked one, and Not set to clear it. A radio group, so
 * the arrows move through it. Picking is the whole job, so `onDone` follows, also when the picked row is tapped again.
 */
function OptionRows({
  label,
  options,
  value,
  onChange,
  onDone,
}: {
  label: string;
  options: readonly { id: string; label: string }[];
  value: string | null;
  onChange: (value: string | null) => void;
  onDone: () => void;
}) {
  return (
    <ToggleButtonGroup
      aria-label={label}
      selectionMode="single"
      orientation="vertical"
      selectedKeys={[value ?? NOT_SET]}
      onSelectionChange={(keys) => {
        const [picked] = [...keys];
        // An empty selection is the picked row tapped again: nothing to change.
        if (picked !== undefined) onChange(picked === NOT_SET ? null : String(picked));
        onDone();
      }}
      className="flex flex-col pb-1"
    >
      {[...options, { id: NOT_SET, label: "Not set" }].map((option) => (
        <SheetChoice key={option.id} id={option.id} muted={option.id === NOT_SET}>
          {option.label}
        </SheetChoice>
      ))}
    </ToggleButtonGroup>
  );
}

/**
 * The item's context (its description) in a phone's sheet. It starts from the saved text each time the sheet opens, and saves as the sheet
 * goes away, however it goes (✕, Escape, a tap outside, Back, a turn of the phone), and only if it was changed, so an
 * untouched sheet never writes back what it was opened with.
 */
function ContextEditor({ context, onSave }: { context: string; onSave: (text: string) => void }) {
  const [opened] = useState(context);
  const [text, setText] = useState(opened);
  // What the cleanup reads as the sheet goes away: the text as last typed, and the save of that moment.
  const latest = useRef({ text, onSave });
  useEffect(() => {
    latest.current = { text, onSave };
  });
  useEffect(
    () => () => {
      if (latest.current.text !== opened) latest.current.onSave(latest.current.text);
    },
    [opened],
  );
  return (
    <textarea
      aria-label="Context"
      autoFocus
      value={text}
      onChange={(e) => setText(e.target.value)}
      rows={6}
      className="mt-1 mb-2 min-h-40 rounded-xl border border-line bg-surface px-3 py-2 text-base"
    />
  );
}

/**
 * What the item is and how far it has been prepared, under its title at every width (ux.md, The item §2): the icons
 * with their words. Closed items show the kind only, as the server sends no stage for them.
 */
function KindLine({ item }: { item: Item }) {
  // A reminder's date goes with its kind; while it's still ahead, a line says what that means.
  const date = item.kind === "reminder" && item.due ? formatDate(item.due.date) : undefined;
  return (
    <div className="-mt-2 flex flex-col gap-0.5">
      <p className="flex items-center gap-1.5 text-sm text-muted">
        <span title={KIND_LABELS[item.kind]} className="flex items-center gap-1.5">
          <KindIcon kind={item.kind} />
          {KIND_LABELS[item.kind]}
        </span>
        {date && (
          <>
            {" "}
            <span aria-hidden="true">·</span> <time dateTime={item.due?.date}>{date}</time>
          </>
        )}
        {item.stage && (
          <>
            {" "}
            <span aria-hidden="true">·</span>{" "}
            <span title={STAGE_LABELS[item.stage]} className="flex items-center gap-1.5">
              <StageIcon stage={item.stage} />
              {STAGE_LABELS[item.stage]}
            </span>
          </>
        )}
      </p>
      {date && isToCome(item) && <p className="text-sm text-ink-soft">Joins the list on {date}</p>}
    </div>
  );
}

/** What state the item is in, as a phone shows it above the fields; its buttons are in the bar at the bottom. */
function StateLine({ item, canReopen }: { item: Item; canReopen: boolean }) {
  const closed = isClosed(item.state);
  return (
    <p className="-mt-2 text-sm text-ink-soft">
      <span className={item.state === "in_progress" ? "font-medium text-accent-ink" : "font-medium text-ink"}>
        {STATE_LABELS[item.state]}
      </span>
      {item.closed_label && <span className="text-muted"> · {item.closed_label}</span>}
      {closed && !canReopen && <span className="text-muted"> · It can't be reopened.</span>}
    </p>
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

type HeldBackGap = RankedItem["held_back_by"][number]["gap"];

/** What each kind of gap offers: the link's text on a wide screen, and what its control is called. */
const FIX_ACTIONS: Partial<Record<HeldBackGap, { link: string; name: string }>> = {
  need: { link: "Change", name: "Change need" },
  due: { link: "Set due", name: "Set due date" },
  size: { link: "Set sizes", name: "Set sizes" },
};

/** Brings the first element inside `id` that matches one of the selectors into view and focuses it. */
function focusField(id: string, ...selectors: string[]) {
  const field = document.getElementById(id);
  const target = selectors.reduce<HTMLElement | null>(
    (found, selector) => found ?? field?.querySelector<HTMLElement>(selector) ?? null,
    null,
  );
  if (!target) return;
  target.scrollIntoView({ block: "nearest" });
  target.focus({ preventScroll: true });
}

/**
 * Why the item ranks where it does (ux.md, The item §9): each contributor, with a bar for its share of the item's
 * priority, then what holds it back, each taking an editor to its field: a link on a wide screen, a row on a phone.
 * Without `onFix` the entries are text.
 */
function WhyItRanks({
  entry,
  onFix,
  phone,
}: {
  entry: RankedItem;
  onFix: ((gap: HeldBackGap) => void) | undefined;
  phone: boolean;
}) {
  const headingId = useId();
  const heldBackId = useId();
  const hasContributors = entry.contributors.length > 0;
  return (
    <section aria-labelledby={headingId} className="flex flex-col gap-2 rounded-lg bg-sunken px-3 py-2.5">
      <h3 id={headingId} className="text-xs font-medium text-ink-soft">
        Why it's #{entry.position}
      </h3>
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
        <div className={hasContributors ? "mt-1 flex flex-col gap-2 border-t border-line pt-2.5" : "contents"}>
          <h4 id={heldBackId} className="text-xs font-medium text-ink-soft">
            Held back by
          </h4>
          <ul
            aria-labelledby={heldBackId}
            className={phone ? "flex flex-col divide-y divide-line" : "flex flex-col gap-1.5"}
          >
            {entry.held_back_by.map((held, index) => {
              const action = FIX_ACTIONS[held.gap];
              if (!onFix || !action) {
                return (
                  <li key={index} className={phone ? "flex min-h-11 items-center py-2 text-sm" : "text-sm"}>
                    {held.label}
                  </li>
                );
              }
              return phone ? (
                <li key={index}>
                  <button
                    type="button"
                    aria-label={`${held.label}. ${action.name}`}
                    onClick={() => onFix(held.gap)}
                    className="flex min-h-11 w-full items-center gap-3 py-2 text-left text-sm outline-none focus-visible:outline-2 focus-visible:outline-accent"
                  >
                    <span className="min-w-0 flex-1">{held.label}</span>
                    <ChevronRight aria-hidden="true" className="size-4 shrink-0 text-muted" />
                  </button>
                </li>
              ) : (
                <li key={index} className="flex items-baseline justify-between gap-3">
                  <span className="min-w-0 text-sm">{held.label}</span>
                  <button
                    type="button"
                    aria-label={action.name}
                    onClick={() => onFix(held.gap)}
                    className="shrink-0 rounded-md px-1.5 py-0.5 text-sm font-medium text-accent-ink hover:bg-accent-soft pointer-coarse:min-h-11 pointer-coarse:px-3"
                  >
                    {action.link}
                  </button>
                </li>
              );
            })}
          </ul>
        </div>
      )}
    </section>
  );
}

/**
 * The item's state and the ways out of it (ux.md, The item §6): the likeliest next state as the main button,
 * the other states beside it, and ⋯ (More) with Won't do, Make it a task and the Stage, as they're rarely the ones wanted. A closed item says when it closed,
 * and offers Reopen while it still can be. As a `bar`, for a phone, the buttons sit at the bottom of the sheet within
 * reach of the thumb, the main one wider, and the state itself is a line under the title (ux.md, The item §2).
 */
function StateActions({
  item,
  disabled,
  canReopen,
  onChange,
  save,
  variant = "inline",
}: {
  item: Item;
  disabled: boolean;
  canReopen: boolean;
  onChange: (state: ItemState) => void;
  save: (patch: ItemPatch) => void;
  variant?: "inline" | "bar";
}) {
  const labelId = useId();
  const bar = variant === "bar";
  const closed = item.state === "done" || item.state === "wont_do";
  if (bar && closed && !canReopen) return null;
  // A reminder that has arrived is most likely made a task, or done: Make it a task leads the state buttons.
  const arrivedReminder = item.kind === "reminder" && !isToCome(item) && item.allowed_actions.includes("make_task");

  const button = (state: ItemState, main: boolean, isDisabled = false) => (
    <button
      key={state}
      type="button"
      disabled={isDisabled}
      onClick={() => onChange(state)}
      className={bar ? barButtonClass(main) : stateButtonClass(main)}
    >
      {changeLabel(item.state, state)}
    </button>
  );

  let actions: ReactNode;
  if (item.state === "done" || item.state === "wont_do") {
    actions = canReopen ? button("open", true) : <p className="text-xs text-muted">It can't be reopened.</p>;
  } else {
    const [main, ...others] = NEXT_STATES[item.state];
    actions = (
      <>
        {arrivedReminder && (
          <MakeTaskButton item={item} save={save} className={bar ? barButtonClass(true) : stateButtonClass(true)} />
        )}
        {[main, ...others].map((state) => button(state, state === main && !arrivedReminder, disabled))}
        <ItemMore item={item} variant={bar ? "bar" : "inline"} save={save} />
      </>
    );
  }

  if (bar) {
    return (
      <div
        role="group"
        aria-label="State"
        data-state-bar=""
        className={`sticky bottom-0 z-10 -mx-4 grid gap-2 border-t border-line bg-surface px-4 pt-3 pb-[max(env(safe-area-inset-bottom),1rem)] ${
          closed
            ? ""
            : arrivedReminder
              ? "grid-cols-[minmax(0,2fr)_minmax(0,1fr)_minmax(0,1fr)_minmax(0,1fr)_auto]"
              : "grid-cols-[minmax(0,2fr)_minmax(0,1fr)_minmax(0,1fr)_auto]"
        }`}
      >
        {actions}
      </div>
    );
  }
  return (
    <div role="group" aria-labelledby={labelId} className="flex flex-col gap-1.5">
      <div className="flex items-baseline gap-2">
        <FieldLabel id={labelId}>State</FieldLabel>
        <span className={`text-sm font-medium ${item.state === "in_progress" ? "text-accent-ink" : "text-ink"}`}>
          {STATE_LABELS[item.state]}
          {item.closed_label && <span className="font-normal text-muted"> · {item.closed_label}</span>}
        </span>
      </div>
      <div className="flex flex-wrap items-center gap-1.5">{actions}</div>
    </div>
  );
}

/** The main state button is filled with the accent, the next action's colour, with dark text for contrast in both themes. */
const stateButtonClass = (main: boolean) =>
  `h-9 rounded-lg px-3 text-sm font-medium disabled:opacity-50 pointer-coarse:h-11 ${
    main ? "bg-accent text-stone-950 hover:brightness-95" : "border border-line bg-surface text-ink hover:border-muted"
  }`;

/** The same, as the phone's bar shows it: 52 px tall, the others on a sunken ground. */
const barButtonClass = (main: boolean) =>
  `h-13 rounded-[14px] text-[15px] disabled:opacity-50 ${
    main ? "bg-accent font-semibold text-stone-950" : "bg-sunken font-medium text-ink"
  }`;

/** The link sign beside the title: copies the item's short link, and says so for a moment. */
function CopyItemLink({ heapRef, itemKey }: { heapRef: HeapRef; itemKey: string }) {
  const { copied, copy } = useCopyLink();
  // The status sits beside the button rather than in it, as a live region inside a button doesn't name it.
  return (
    <>
      <button
        type="button"
        onClick={() => copy(heapRef.team, heapRef.heap, itemKey)}
        aria-label={`Copy the link to ${itemKey}`}
        title={copied ? "Copied" : "Copy link"}
        className="flex size-8 shrink-0 items-center justify-center rounded-lg text-ink-soft hover:bg-sunken hover:text-ink pointer-coarse:size-11"
      >
        {copied ? (
          <Check aria-hidden="true" className="size-4 text-done" />
        ) : (
          <Link2 aria-hidden="true" className="size-4" />
        )}
      </button>
      <span role="status" className="sr-only">
        {copied ? "Link copied" : ""}
      </span>
    </>
  );
}
