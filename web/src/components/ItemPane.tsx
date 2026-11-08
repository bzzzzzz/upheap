import { Link } from "@tanstack/react-router";
import { ChevronDown, ChevronUp, Ellipsis, X } from "lucide-react";
import { motion, useDragControls, type PanInfo } from "motion/react";
import { useEffect, useId, useRef, type ReactNode } from "react";
import { Button, Menu, MenuTrigger } from "react-aria-components";

import { ApiProblem, type Item, type ItemPatch, type ItemState, type RankedItem, type HeapRef } from "../api/client";
import { useBlockerCandidates, useItem, useMe, useParameters, useUpdateItem } from "../api/queries";
import { AuditHistory } from "./AuditHistory";
import { BlockerSelect } from "./BlockerSelect";
import { CopyLink } from "./CopyLink";
import { blockerCandidatesFor } from "./blockerCandidates";
import { ChoiceButtons, FieldLabel } from "./ChoiceButtons";
import { DuePicker } from "./DuePicker";
import { itemProblem } from "./itemProblems";
import { changeLabel, isClosed, NEXT_STATES, SIZE_LABELS, SIZES, STATE_LABELS } from "./labels";
import { PHONE, TABLET, TOUCH, useMedia } from "./media";
import { MenuEntry, MenuPopover } from "./Menu";
import { NeedPicker } from "./NeedPicker";
import { ParameterPicker } from "./ParameterPicker";
import { TagPicker } from "./TagPicker";

const SIZE_OPTIONS = SIZES.map((size) => ({ id: size, label: SIZE_LABELS[size] }));

/** How far the sheet (down) or the panel (right) is dragged by its top bar before letting go closes it. */
const CLOSE_DISTANCE = 120;
const CLOSE_SPEED = 600;

/**
 * One item over its list (app-ux.md, The item): a pane beside the list on a wide screen, a panel over the right of
 * the list on a tablet, with the list still live beside it, and a sheet from the bottom on a phone. Each change saves
 * as it's made. `entry` is the item's place in the list on screen, when it's there, for why it ranks where it does.
 * `previousKey` and `nextKey` are the items above and below it on screen, for walking the list without closing the
 * pane (ux-overview.md, The item §2).
 */
export function ItemPane({
  heapRef,
  itemKey,
  entry,
  section,
  previousKey,
  nextKey,
  onClose,
}: {
  heapRef: HeapRef;
  itemKey: string;
  entry: RankedItem | undefined;
  section: string | undefined;
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
          {entry && (
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
          {/* The key is the link's own name, so the button says what it copies (ux-overview.md, The item §3). */}
          <CopyLink
            handles={[heapRef.team, heapRef.heap, itemKey]}
            label={<span className="font-mono">{itemKey}</span>}
            description={`Copy the link to ${itemKey}`}
          />
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
  const update = useUpdateItem(heapRef);
  const parameters = useParameters(heapRef);
  const candidates = useBlockerCandidates(heapRef, item.state === "blocked");
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

      <StateActions
        item={item}
        disabled={!canChangeState}
        canReopen={canReopen}
        onChange={(state) => state !== item.state && save({ state })}
      />
      {item.state === "blocked" && (
        <BlockerSelect
          value={item.blocked_by}
          candidates={blockerCandidatesFor(candidates.data ?? [])(item.id)}
          disabled={!canChangeState}
          onChange={(blockerId) => save({ blocked_by_item_id: blockerId })}
        />
      )}

      {entry && entry.contributors.length > 0 && <WhyItRanks entry={entry} />}

      <NeedPicker
        value={item.need}
        disabled={!canEdit}
        onChange={(need) => {
          if (need !== item.need) save({ need });
        }}
      />

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
        <div className="flex items-baseline justify-between gap-3">
          <h3 className="text-xs font-medium text-ink-soft">History</h3>
          <AddedBy item={item} />
        </div>
        <AuditHistory scope={{ heap: heapRef }} entityId={item.id} />
      </section>
    </div>
  );
}

/**
 * Who added the item and how long ago, as the server labels it: "Added by Sam 4 days ago", or "by you" for the
 * viewer's own, as History says "You".
 */
function AddedBy({ item }: { item: Item }) {
  const { data: me } = useMe();
  const by = item.created_by && (item.created_by.id === me?.user.id ? "you" : item.created_by.display_name);
  return (
    <p className="text-xs text-muted">
      Added{by && ` by ${by}`} <time dateTime={new Date(item.created_at).toISOString()}>{item.created_label}</time>
    </p>
  );
}

/**
 * Why the item ranks where it does (ux-overview.md, The item §4): each contributor, with a bar for its share of the
 * item's priority.
 */
function WhyItRanks({ entry }: { entry: RankedItem }) {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="flex flex-col gap-2 rounded-lg bg-sunken px-3 py-2.5">
      <h3 id={headingId} className="text-xs font-medium text-ink-soft">
        Why it's #{entry.position}
      </h3>
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
    </section>
  );
}

/**
 * The item's state and the ways out of it (ux-overview.md, The item §1): the likeliest next state as the main button,
 * the other states beside it, and Won't do under ⋯, as it's rarely the one wanted. A closed item says when it closed,
 * and offers Reopen while it still can be.
 */
function StateActions({
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
  const header = (
    <div className="flex items-baseline gap-2">
      <FieldLabel id={labelId}>State</FieldLabel>
      <span className={`text-sm font-medium ${item.state === "in_progress" ? "text-accent-ink" : "text-ink"}`}>
        {STATE_LABELS[item.state]}
        {item.closed_label && <span className="font-normal text-muted"> · {item.closed_label}</span>}
      </span>
    </div>
  );

  if (item.state === "done" || item.state === "wont_do") {
    return (
      <div role="group" aria-labelledby={labelId} className="flex flex-col gap-1.5">
        {header}
        {canReopen ? (
          <div>
            <button type="button" onClick={() => onChange("open")} className={stateButtonClass(true)}>
              {changeLabel(item.state, "open")}
            </button>
          </div>
        ) : (
          <p className="text-xs text-muted">It can't be reopened.</p>
        )}
      </div>
    );
  }

  const [main, ...others] = NEXT_STATES[item.state];
  return (
    <div role="group" aria-labelledby={labelId} className="flex flex-col gap-1.5">
      {header}
      <div className="flex flex-wrap items-center gap-1.5">
        {[main, ...others].map((state) => (
          <button
            key={state}
            type="button"
            disabled={disabled}
            onClick={() => onChange(state)}
            className={stateButtonClass(state === main)}
          >
            {changeLabel(item.state, state)}
          </button>
        ))}
        <MenuTrigger>
          <Button
            aria-label="More states"
            isDisabled={disabled}
            className="flex size-9 items-center justify-center rounded-lg text-ink-soft outline-none hover:bg-sunken data-[disabled]:opacity-50 data-[focus-visible]:outline-2 data-[focus-visible]:outline-accent pointer-coarse:size-11"
          >
            <Ellipsis aria-hidden="true" className="size-4" />
          </Button>
          <MenuPopover placement="bottom end">
            <Menu aria-label="More states" className="outline-none" onAction={() => onChange("wont_do")}>
              <MenuEntry id="wont_do">{changeLabel(item.state, "wont_do")}</MenuEntry>
            </Menu>
          </MenuPopover>
        </MenuTrigger>
      </div>
    </div>
  );
}

/** The main state button is filled with the accent, the next action's colour, with dark text for contrast in both themes. */
const stateButtonClass = (main: boolean) =>
  `h-9 rounded-lg px-3 text-sm font-medium disabled:opacity-50 pointer-coarse:h-11 ${
    main ? "bg-accent text-stone-950 hover:brightness-95" : "border border-line bg-surface text-ink hover:border-muted"
  }`;
