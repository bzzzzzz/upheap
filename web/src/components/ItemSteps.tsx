import { Check, ChevronRight, CircleMinus, Ellipsis, Minus, Plus, Undo2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button, Checkbox, Menu, MenuTrigger } from "react-aria-components";

import type { HeapRef, Item, Step, StepPatch } from "../api/client";
import { useAddStep, useUpdateStep } from "../api/queries";
import { compactButtonClass } from "./buttons";
import { itemProblem } from "./itemProblems";
import { Markdown } from "./Markdown";
import { MenuEntry, MenuPopover } from "./Menu";

/**
 * A task's steps (ux.md, The item §3): a checklist, inline at every width, each step saving as it's changed. Only the
 * server's `allowed_actions` decide what can be done: ticking, editing and dropping need `edit` on the step, adding
 * needs `add_step` on the item, and the server offers that only for a task, so this renders nothing for a note. A step
 * that is dropped stays in the list, struck through, and can be restored. `item` is a single-item response, the only
 * kind that carries the steps; a list row has `steps: null`, which reads as none.
 */
export function ItemSteps({ heapRef, item }: { heapRef: HeapRef; item: Item }) {
  const add = useAddStep(heapRef, item.key);
  const update = useUpdateStep(heapRef, item.key);
  // Which steps show their context: local to this view, closed to begin with.
  const [shown, setShown] = useState<ReadonlySet<number>>(new Set());
  const canAdd = item.allowed_actions.includes("add_step");

  const steps = item.steps ?? [];

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
    <section aria-label="Steps" className="flex flex-col gap-1.5">
      <h3 className="text-xs font-medium text-ink-soft">
        {progress ? `Steps ${progress.ticked}/${progress.total}` : "Steps"}
      </h3>
      {steps.length > 0 && (
        <ul className="flex flex-col">
          {steps.map((step) => (
            <StepRow
              key={step.id}
              step={step}
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
        </ul>
      )}
      {canAdd && (
        <AddStep
          onAdd={(title) =>
            add.mutateAsync({ title }).then(
              () => true,
              () => false,
            )
          }
        />
      )}
      {failed?.error && (
        <p role="alert" className="text-sm text-overdue-ink">
          {itemProblem(failed.error)}
        </p>
      )}
    </section>
  );
}

const slotClass = "flex size-9 shrink-0 items-center justify-center pointer-coarse:size-11";
const iconButtonClass =
  "flex size-9 shrink-0 items-center justify-center rounded-lg text-ink-soft outline-none hover:bg-sunken data-[focus-visible]:outline-2 data-[focus-visible]:outline-accent pointer-coarse:size-11";

function StepRow({
  step,
  contextShown,
  onToggleContext,
  onSave,
}: {
  step: Step;
  contextShown: boolean;
  onToggleContext: () => void;
  /** Resolves to whether the save landed; a failure is shown by the list. */
  onSave: (patch: StepPatch) => Promise<boolean>;
}) {
  const canEdit = step.allowed_actions.includes("edit");
  const dropped = step.state === "dropped";
  const hasDisclosure = Boolean(step.context) || canEdit;
  // Whether the context's textarea is open. An editor sees it at once for a step with no context, so adding takes one
  // step; with a context it shows rendered until Edit. Focusing the textarea sets the flag, so a context arriving from
  // elsewhere never swaps a draft out from under the typing.
  const [editingContext, setEditingContext] = useState(false);
  const contextField = useRef<HTMLTextAreaElement>(null);
  // Set by Edit, so focus goes into the field once it is there.
  const focusContext = useRef(false);
  useEffect(() => {
    if (editingContext && focusContext.current) {
      focusContext.current = false;
      const field = contextField.current;
      field?.focus();
      field?.setSelectionRange(field.value.length, field.value.length);
    }
  }, [editingContext]);

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
    <li className="flex flex-col">
      <div className="flex items-start">
        {dropped ? (
          <span className={`${slotClass} text-muted`}>
            <Minus aria-hidden="true" className="size-4" />
            <span className="sr-only">Dropped</span>
          </span>
        ) : (
          <Checkbox
            isSelected={step.state === "ticked"}
            isDisabled={!canEdit}
            onChange={(ticked) => onSave({ state: ticked ? "ticked" : "open" })}
            aria-label={step.title}
            className={`${slotClass} group outline-none`}
          >
            {({ isSelected }) => (
              <span
                aria-hidden="true"
                className={`flex size-5 items-center justify-center rounded-md border group-data-[focus-visible]:outline-2 group-data-[focus-visible]:outline-accent ${
                  isSelected ? "border-done bg-done text-surface" : "border-line-strong bg-surface"
                } ${canEdit ? "" : "opacity-60"}`}
              >
                {isSelected && <Check className="size-3.5" strokeWidth={3} />}
              </span>
            )}
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
        <div className="pr-1 pb-2 pl-9 pointer-coarse:pl-11">
          {canEdit && (editingContext || !step.context) ? (
            <textarea
              ref={contextField}
              aria-label="Step context"
              defaultValue={step.context ?? ""}
              rows={2}
              placeholder="Background for this step"
              onFocus={() => setEditingContext(true)}
              onKeyDown={(e) => {
                // Escape does nothing here, so a draft is never lost to a stray key and the item stays open.
                if (e.key === "Escape") {
                  e.preventDefault();
                  e.stopPropagation();
                }
              }}
              onBlur={(e) => {
                const field = e.currentTarget;
                void saveContext(field.value).then((landed) => {
                  // Back to the rendered text, unless the save failed or the person came back to the field meanwhile.
                  if (landed && document.activeElement !== field) setEditingContext(false);
                });
              }}
              className="field-sizing-content min-h-14 w-full rounded-lg border border-line bg-surface px-2 py-1.5 text-base"
            />
          ) : (
            <>
              {step.context && <Markdown text={step.context} className="text-ink-soft" />}
              {canEdit && (
                <Button
                  aria-label="Edit step context"
                  onPress={() => {
                    focusContext.current = true;
                    setEditingContext(true);
                  }}
                  className={`${compactButtonClass.quiet} -ml-3 min-h-9 outline-none data-[focus-visible]:outline-2 data-[focus-visible]:outline-accent`}
                >
                  Edit
                </Button>
              )}
            </>
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
function AddStep({ onAdd }: { onAdd: (title: string) => Promise<boolean> }) {
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
        <Plus aria-hidden="true" className="size-4" />
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
  );
}
