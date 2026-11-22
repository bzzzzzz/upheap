import { Link } from "@tanstack/react-router";
import {
  ArrowDown,
  ArrowUp,
  Check,
  ChevronRight,
  CircleMinus,
  Ellipsis,
  GripVertical,
  Minus,
  Plus,
  Undo2,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button, Checkbox, Menu, MenuTrigger } from "react-aria-components";

import type { HeapRef } from "../api/client";
import type { Item, Step, StepPatch } from "../api/schema";
import { useAddStep, useUpdateStep } from "../api/queries/steps";
import { compactButtonClass } from "./buttons";
import { itemProblem } from "./itemProblems";
import { LazyMarkdownEditor } from "./LazyMarkdownEditor";
import { Markdown } from "./Markdown";
import { MenuEntry, MenuPopover } from "./Menu";
import { useOverListLink } from "./overList";
import { RequestId } from "./RequestId";
import { Saving } from "./Saving";
import { sectionHeadingClass } from "./sectionHeading";
import { useDragToReorder, type GripProps } from "./useDragToReorder";

/**
 * A task's steps (ux.md, The item point 11): a checklist, inline at every width, each step saving as it's changed. An
 * editor moves a step by dragging its grip or with Move up and Move down in its ⋯ menu; the move shows at once, from a
 * pending order, until the server has answered. Only the server's `allowed_actions` decide what can be done: ticking,
 * editing, moving and dropping need `edit` on the step, adding needs `add_step` on the item, and the server offers that
 * only for a task, so this renders nothing for a note. A step that is dropped stays in the list, struck through, and can
 * be restored. `item` is a single-item response, the only kind that carries the steps; a list row has `steps: null`,
 * which reads as none.
 */
export function ItemSteps({ heapRef, item }: { heapRef: HeapRef; item: Item }) {
  const add = useAddStep(heapRef, item.key);
  const update = useUpdateStep(heapRef, item.key);
  // The write that is on its way, which is the latest one: a step shows it saving when the write is a tick, a drop, or
  // a new title or context of its own.
  const writing = update.isPending ? update.variables : undefined;
  // Which steps show their context: local to this view, closed to begin with.
  const [shown, setShown] = useState<ReadonlySet<number>>(new Set());
  const canAdd = item.allowed_actions.includes("add_step");

  const served = item.steps ?? [];
  // The order a move has asked for, while the server saves it; the list shows it until the latest move has settled.
  const [pending, setPending] = useState<readonly number[] | null>(null);
  const moves = useRef(0);
  const [moved, setMoved] = useState({ move: 0, text: "" });
  const steps = pending
    ? (() => {
        const place = (step: Step) => {
          const index = pending.indexOf(step.number);
          return index < 0 ? pending.length : index;
        };
        return [...served].sort((a, b) => place(a) - place(b));
      })()
    : served;
  const listRef = useRef<HTMLUListElement>(null);
  const rows = useRef(new Map<number, HTMLLIElement>());

  // Puts a step at an index of the shown order. A move to where it is writes nothing.
  const moveTo = (number: number, index: number) => {
    const order = steps.map((step) => step.number);
    const from = order.indexOf(number);
    if (from < 0 || from === index) return;
    order.splice(from, 1);
    order.splice(index, 0, number);
    const step = steps[from];
    if (!step) return;
    const move = ++moves.current;
    setPending(order);
    setMoved({ move, text: `Moved to place ${index + 1} of ${order.length}` });
    const settled = () => {
      if (move === moves.current) setPending(null);
    };
    update
      .mutateAsync({ step, patch: { after: index === 0 ? null : (order[index - 1] ?? null) } })
      .then(settled, settled);
  };
  const drag = useDragToReorder({
    listRef,
    rows,
    ids: steps.map((step) => step.number),
    onDrop: moveTo,
  });

  if (steps.length === 0 && !canAdd) return null;

  const toggleContext = (number: number) =>
    setShown((current) => {
      const next = new Set(current);
      if (!next.delete(number)) next.add(number);
      return next;
    });
  // The latest failure of either kind of write, worded for the person.
  const failed = [add, update].filter((write) => write.isError).sort((a, b) => b.submittedAt - a.submittedAt)[0];
  const progress = item.step_progress;

  return (
    <section aria-label="Steps" className="flex flex-col gap-1.5 border-t border-line pt-3">
      <h3 className={sectionHeadingClass}>{progress ? `Steps ${progress.ticked}/${progress.total}` : "Steps"}</h3>
      {steps.length > 0 && (
        <ul ref={listRef} className={`relative flex flex-col ${drag.active ? "select-none" : ""}`}>
          {steps.map((step, index) => (
            <StepRow
              key={step.id}
              step={step}
              rowRef={(row) => {
                if (row) rows.current.set(step.number, row);
                else rows.current.delete(step.number);
              }}
              canMoveUp={index > 0}
              canMoveDown={index < steps.length - 1}
              onMove={(delta) => moveTo(step.number, index + delta)}
              grip={drag.gripProps(step.number)}
              lift={drag.lifted === step.number ? drag.dy : null}
              saving={writing?.step.number === step.number ? writing.patch : null}
              contextShown={shown.has(step.number)}
              onToggleContext={() => toggleContext(step.number)}
              onSave={(patch) =>
                update.mutateAsync({ step, patch }).then(
                  () => true,
                  () => false,
                )
              }
            />
          ))}
          {drag.lineY !== null && (
            <li
              aria-hidden="true"
              className="pointer-events-none absolute inset-x-0 h-0.5 bg-accent"
              style={{ top: drag.lineY - 1 }}
            />
          )}
        </ul>
      )}
      <span role="status" className="sr-only">
        {/* Keyed by the move, so a second move to the same place is announced again. */}
        <span key={moved.move}>{moved.text}</span>
      </span>
      {canAdd && (
        <AddStep
          saving={add.isPending}
          onAdd={(title) =>
            add.mutateAsync({ title }).then(
              () => true,
              () => false,
            )
          }
        />
      )}
      {failed?.error && (
        <>
          <p role="alert" className="text-sm text-overdue-ink">
            {itemProblem(failed.error)}
          </p>
          <RequestId error={failed.error} />
        </>
      )}
    </section>
  );
}

const slotClass = "flex size-9 shrink-0 items-center justify-center pointer-coarse:size-11";
const iconButtonClass =
  "flex size-9 shrink-0 items-center justify-center rounded-lg text-ink-soft outline-none hover:bg-sunken data-[focus-visible]:outline-2 data-[focus-visible]:outline-accent pointer-coarse:size-11";

function StepRow({
  step,
  rowRef,
  canMoveUp,
  canMoveDown,
  onMove,
  grip,
  lift,
  saving,
  contextShown,
  onToggleContext,
  onSave,
}: {
  step: Step;
  rowRef: (row: HTMLLIElement | null) => void;
  canMoveUp: boolean;
  canMoveDown: boolean;
  /** Moves the step by this many places. */
  onMove: (delta: number) => void;
  grip: GripProps;
  /** How far the step is dragged, while it is. */
  lift: number | null;
  /** The change to the step that is being saved, if any. */
  saving: StepPatch | null;
  contextShown: boolean;
  onToggleContext: () => void;
  /** Resolves to whether the save landed; a failure is shown by the list. */
  onSave: (patch: StepPatch) => Promise<boolean>;
}) {
  const canEdit = step.allowed_actions.includes("edit");
  const dropped = step.state === "dropped";
  const hasDisclosure = Boolean(step.context) || canEdit;
  const overList = useOverListLink();
  // Whether the context's editor is open. An editor sees it at once for a step with no context, so adding takes one
  // step; with a context it shows rendered until Edit. Focusing the editor sets the flag, so a context arriving from
  // elsewhere never swaps a draft out from under the typing.
  const [editingContext, setEditingContext] = useState(false);
  // The text in the editor, started from the saved context when Edit was pressed.
  const [contextDraft, setContextDraftState] = useState(step.context ?? "");
  // What the blur reads: on Android the editor hands over its last keystrokes as focus leaves, in the same event the
  // blur is handled in, before a render could give it the state.
  const contextDraftNow = useRef(contextDraft);
  const setContextDraft = (text: string) => {
    contextDraftNow.current = text;
    setContextDraftState(text);
  };
  // Set by Edit, so the editor takes focus as it mounts; a field that is open from the start must not.
  const [focusContext, setFocusContext] = useState(false);
  // Counts the context's saves, so only the latest one closes the field: an earlier save landing while a later one
  // is in flight (and may fail) must not take its draft away.
  const contextSaves = useRef(0);

  const saveTitle = (field: HTMLTextAreaElement) => {
    const title = field.value.trim();
    // A blank title isn't saved, and neither is a title that hasn't changed: the field shows the saved one again.
    if (title !== "" && title !== step.title) void onSave({ title });
    else field.value = step.title;
  };
  // Saves a changed context, and resolves to whether the field may close: at once when nothing changed, else once the
  // save has landed, so a failed save keeps the draft in the field.
  const saveContext = (text: string): Promise<boolean> => {
    const context = text.trim();
    if (context === (step.context ?? "")) return Promise.resolve(true);
    return onSave({ context: context === "" ? null : context });
  };

  return (
    <li
      ref={rowRef}
      style={lift === null ? undefined : { transform: `translateY(${lift}px)` }}
      className={`group flex flex-col ${lift === null ? "" : "relative z-10 bg-surface shadow-lg"}`}
    >
      <div className="flex items-start">
        {canEdit && (
          <span
            aria-hidden="true"
            data-step-grip
            {...grip}
            className={`${slotClass} touch-none text-muted opacity-0 group-focus-within:opacity-100 group-hover:opacity-100 pointer-coarse:opacity-100 ${
              lift === null ? "cursor-grab" : "cursor-grabbing"
            }`}
          >
            <GripVertical className="size-4" />
          </span>
        )}
        {dropped ? (
          <span className={`${slotClass} text-muted`}>
            {saving?.state !== undefined ? <Saving pending /> : <Minus aria-hidden="true" className="size-4" />}
            <span className="sr-only">Dropped</span>
          </span>
        ) : (
          <Checkbox
            isSelected={step.state === "ticked"}
            isDisabled={!canEdit}
            isReadOnly={saving?.state !== undefined}
            onChange={(ticked) => onSave({ state: ticked ? "ticked" : "open" })}
            aria-label={step.title}
            className={`${slotClass} group outline-none`}
          >
            {({ isSelected }) =>
              saving?.state !== undefined ? (
                <Saving pending />
              ) : (
                <span
                  aria-hidden="true"
                  className={`flex size-5 items-center justify-center rounded-md border group-data-[focus-visible]:outline-2 group-data-[focus-visible]:outline-accent ${
                    isSelected ? "border-done bg-done text-surface" : "border-line-strong bg-surface"
                  } ${canEdit ? "" : "opacity-60"}`}
                >
                  {isSelected && <Check className="size-3.5" strokeWidth={3} />}
                </span>
              )
            }
          </Checkbox>
        )}
        <div className="flex min-h-9 min-w-0 flex-1 items-center pointer-coarse:min-h-11">
          {canEdit ? (
            // Keyed by the saved title, so a save from elsewhere shows up here and a draft survives a refetch.
            <textarea
              aria-label="Step title"
              key={step.title}
              defaultValue={step.title}
              rows={1}
              onBlur={(e) => saveTitle(e.target)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  e.currentTarget.blur();
                } else if (e.key === "Escape") {
                  // Undoes the typing, and keeps the item open.
                  e.preventDefault();
                  e.stopPropagation();
                  e.currentTarget.value = step.title;
                  e.currentTarget.blur();
                }
              }}
              className={`field-sizing-content min-w-0 flex-1 resize-none bg-transparent py-1.5 text-base leading-snug outline-none ${
                dropped ? "text-muted line-through" : ""
              }`}
            />
          ) : (
            <span
              className={`min-w-0 flex-1 py-1.5 text-base leading-snug break-words ${
                dropped ? "text-muted line-through" : ""
              }`}
            >
              {step.title}
            </span>
          )}
          <Saving pending={saving?.title !== undefined} />
        </div>
        {hasDisclosure && (
          <Button
            onPress={onToggleContext}
            aria-label={contextShown ? "Hide context" : "Show context"}
            aria-expanded={contextShown}
            className={iconButtonClass}
          >
            <ChevronRight
              aria-hidden="true"
              className={`size-4 transition-transform ${contextShown ? "rotate-90" : ""}`}
            />
          </Button>
        )}
        {canEdit && (
          <MenuTrigger>
            <Button aria-label="Step actions" className={iconButtonClass}>
              <Ellipsis aria-hidden="true" className="size-4" />
            </Button>
            <MenuPopover placement="bottom end">
              <Menu aria-label="Step actions" className="outline-none">
                {canMoveUp && (
                  <MenuEntry id="move-up" icon={ArrowUp} onAction={() => onMove(-1)}>
                    Move up
                  </MenuEntry>
                )}
                {canMoveDown && (
                  <MenuEntry id="move-down" icon={ArrowDown} onAction={() => onMove(1)}>
                    Move down
                  </MenuEntry>
                )}
                {dropped ? (
                  <MenuEntry id="restore" icon={Undo2} onAction={() => onSave({ state: "open" })}>
                    Restore
                  </MenuEntry>
                ) : (
                  <MenuEntry id="drop" icon={CircleMinus} onAction={() => onSave({ state: "dropped" })}>
                    Drop
                  </MenuEntry>
                )}
              </Menu>
            </MenuPopover>
          </MenuTrigger>
        )}
      </div>
      {contextShown && (
        <div className={`pr-1 pb-2 ${canEdit ? "pl-18 pointer-coarse:pl-22" : "pl-9 pointer-coarse:pl-11"}`}>
          <Saving pending={saving?.context !== undefined} className="mb-1" />
          {canEdit && (editingContext || !step.context) ? (
            <LazyMarkdownEditor
              aria-label="Step context"
              value={contextDraft}
              onChange={setContextDraft}
              autoFocus={focusContext}
              placeholder="Background for this step"
              onFocus={() => {
                // A field open from the start remounts with the last draft; start from what is saved.
                if (!editingContext) setContextDraft(step.context ?? "");
                setEditingContext(true);
              }}
              onKeyDown={(e) => {
                // Escape does nothing here, so a draft is never lost to a stray key and the item stays open.
                if (e.key === "Escape") {
                  e.preventDefault();
                  e.stopPropagation();
                }
              }}
              onBlur={(e) => {
                // Focus moving on inside the editor isn't leaving it.
                if (e.currentTarget.contains(e.relatedTarget)) return;
                const box = e.currentTarget;
                const save = ++contextSaves.current;
                void saveContext(contextDraftNow.current).then((landed) => {
                  // Back to the rendered text, unless the save failed, a later one is on its way, or the person came
                  // back to the field meanwhile.
                  if (landed && save === contextSaves.current && !box.contains(document.activeElement)) {
                    setEditingContext(false);
                    setFocusContext(false);
                  }
                });
              }}
              className="min-h-14 w-full rounded-lg border border-line bg-surface focus-within:border-accent"
            />
          ) : (
            <>
              {step.context && <Markdown text={step.context} className="text-ink-soft" />}
              {canEdit && (
                <Button
                  aria-label="Edit step context"
                  onPress={() => {
                    setContextDraft(step.context ?? "");
                    setFocusContext(true);
                    setEditingContext(true);
                  }}
                  className={`${compactButtonClass.quiet} -ml-3 min-h-9 outline-none data-[focus-visible]:outline-2 data-[focus-visible]:outline-accent`}
                >
                  Edit
                </Button>
              )}
            </>
          )}
          {step.origin && (
            <p className="text-xs text-ink-soft">
              From:{" "}
              <Link
                to="/teams/$team/heaps/$heap/items/$item"
                params={{ team: step.origin.heap.team_slug, heap: step.origin.heap.slug, item: step.origin.key }}
                search={true}
                {...overList({ team: step.origin.heap.team_slug, heap: step.origin.heap.slug }, step.origin.key)}
                className="text-accent-ink underline"
              >
                “{step.origin.title}”, a {step.origin.kind}
              </Link>
            </p>
          )}
        </div>
      )}
    </li>
  );
}

/**
 * "Add a step", which opens a one-line field in place. Enter adds the step and keeps the field for the next one; leaving
 * the field with text adds it too and closes the field; leaving it empty, or Escape, closes it without adding, and
 * Escape puts the focus back on the button. Enter clears the field at once, so the next step can be typed while the
 * first is saved; a refused add puts its title back when the field is still empty, so it loses nothing (it shows again
 * when the field is opened again, if the field was closed meanwhile).
 */
function AddStep({ saving, onAdd }: { saving: boolean; onAdd: (title: string) => Promise<boolean> }) {
  const [adding, setAdding] = useState(false);
  const [draft, setDraft] = useState("");
  const buttonRef = useRef<HTMLButtonElement>(null);
  // Escape closes the field, and the blur that may follow it must not add what was typed.
  const cancelled = useRef(false);
  const returnFocus = useRef(false);

  useEffect(() => {
    if (!adding && returnFocus.current) {
      returnFocus.current = false;
      buttonRef.current?.focus();
    }
  }, [adding]);

  if (!adding) {
    return (
      <Button
        ref={buttonRef}
        onPress={() => {
          cancelled.current = false;
          setAdding(true);
        }}
        className="-ml-1 flex min-h-9 items-center gap-2 self-start rounded-lg px-1 text-sm text-ink-soft outline-none hover:text-ink data-[focus-visible]:outline-2 data-[focus-visible]:outline-accent pointer-coarse:min-h-11"
      >
        {saving ? <Saving pending /> : <Plus aria-hidden="true" className="size-4" />}
        Add a step
      </Button>
    );
  }

  const takeDraft = () => {
    const title = draft.trim();
    if (title === "") return;
    setDraft("");
    void onAdd(title).then((added) => {
      // Whatever has been typed since stays; the refused title comes back only into an empty field.
      if (!added) setDraft((current) => (current.trim() === "" ? title : current));
    });
  };

  return (
    <div className="flex items-center gap-2">
      <input
        // The person asked for the field, and expects to type in it.
        autoFocus
        aria-label="New step"
        value={draft}
        maxLength={300}
        placeholder="What needs doing?"
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            takeDraft();
          } else if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            cancelled.current = true;
            returnFocus.current = true;
            setDraft("");
            setAdding(false);
          }
        }}
        onBlur={() => {
          if (cancelled.current) return;
          takeDraft();
          setAdding(false);
        }}
        className="min-h-9 w-full rounded-lg border border-line bg-surface px-2 text-base outline-none focus:border-accent pointer-coarse:min-h-11"
      />
      <Saving pending={saving} />
    </div>
  );
}
