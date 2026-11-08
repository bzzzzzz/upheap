import { Plus } from "lucide-react";
import { useEffect, useId, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { Dialog, Heading, Modal, ModalOverlay } from "react-aria-components";

import type { DueInput, HeapRef, ItemTag, Need, Size } from "../api/client";
import { useCreateItem, useParameters } from "../api/queries";
import { ChoiceButtons } from "./ChoiceButtons";
import { DuePicker } from "./DuePicker";
import { SIZE_LABELS, SIZES } from "./labels";
import { NeedPicker } from "./NeedPicker";
import { ParameterPicker } from "./ParameterPicker";
import { TagPicker } from "./TagPicker";

const SIZE_OPTIONS = SIZES.map((size) => ({ id: size, label: SIZE_LABELS[size] }));

/**
 * The way in to adding an item (app-ux.md, Adding an item): a field-like button above the list, or `N`, opens the Add
 * dialog. It's a modal over the list on a wide screen and a tablet, and a sheet from the bottom on a phone.
 */
export function AddItem({ heapRef, disabled }: { heapRef: HeapRef; disabled: boolean }) {
  const [open, setOpen] = useState(false);
  // Kept while the page is open, for entering several items in a row over more than one visit to the dialog.
  const [addAnother, setAddAnother] = useState(false);

  useEffect(() => {
    if (disabled) return;
    const onKey = (event: globalThis.KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing = target?.closest("input, textarea, select, [contenteditable]");
      if (event.key === "n" && !typing && !event.metaKey && !event.ctrlKey && !event.altKey) {
        event.preventDefault();
        setOpen(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [disabled]);

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        disabled={disabled}
        aria-keyshortcuts="n"
        className="my-4 flex min-h-12 w-full items-center gap-2 rounded-xl border border-line bg-surface px-4 text-left text-muted shadow-sm hover:border-muted disabled:opacity-50"
      >
        <Plus aria-hidden="true" className="size-4 shrink-0" />
        <span className="flex-1">Add an item</span>
        <kbd
          aria-hidden="true"
          className="hidden rounded border border-line px-1.5 text-xs font-sans text-muted sm:inline"
        >
          N
        </kbd>
      </button>
      {open && (
        <AddItemDialog
          heapRef={heapRef}
          addAnother={addAnother}
          onAddAnother={setAddAnother}
          onClose={() => setOpen(false)}
        />
      )}
    </>
  );
}

/** A new item as the dialog holds it before it's added. */
type Draft = {
  title: string;
  description: string;
  /** Required: Add stays off until one is picked. */
  need: Need | null;
  effort: Size | null;
  impact: Size | null;
  due: DueInput | null;
  tags: ItemTag[];
  /** Option ID by parameter ID. */
  values: Record<string, string | null>;
};

const EMPTY: Draft = {
  title: "",
  description: "",
  need: null,
  effort: null,
  impact: null,
  due: null,
  tags: [],
  values: {},
};

/**
 * Every field Add item takes, the title focused (Req Views: Add item). The title and the need are required. Picking
 * a tag brings up the parameters linked to it. Enter in the title or ⌘↵ adds; with Add another ticked the dialog
 * clears for the next item instead of closing. Closing with a title typed asks first, so a stray tap outside doesn't
 * lose it.
 */
function AddItemDialog({
  heapRef,
  addAnother,
  onAddAnother,
  onClose,
}: {
  heapRef: HeapRef;
  addAnother: boolean;
  onAddAnother: (addAnother: boolean) => void;
  onClose: () => void;
}) {
  const create = useCreateItem(heapRef);
  const parameters = useParameters(heapRef);
  const [draft, setDraft] = useState(EMPTY);
  // A new key remounts the due picker, so a cleared draft doesn't keep Exact date picked.
  const [round, setRound] = useState(0);
  const [confirmingDiscard, setConfirmingDiscard] = useState(false);
  const titleRef = useRef<HTMLInputElement>(null);
  const titleId = useId();
  const set = <K extends keyof Draft>(key: K, value: Draft[K]) => setDraft((d) => ({ ...d, [key]: value }));

  // The parameters linked to a chosen tag, and any already given a value.
  const suggested = (parameters.data?.parameters ?? []).filter(
    (parameter) =>
      parameter.tag_ids.some((id) => draft.tags.some((tag) => tag.id === id)) || draft.values[parameter.id],
  );

  const canAdd = draft.title.trim() !== "" && draft.need !== null && !create.isPending;

  const requestClose = () => {
    if (draft.title.trim() !== "" && !confirmingDiscard) {
      setConfirmingDiscard(true);
    } else {
      onClose();
    }
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const need = draft.need;
    if (!canAdd || need === null) return;
    create.mutate(
      {
        title: draft.title,
        need,
        description: draft.description.trim() === "" ? null : draft.description,
        effort: draft.effort,
        impact: draft.impact,
        due: draft.due,
        tag_ids: draft.tags.map((tag) => tag.id),
        parameter_values: Object.fromEntries(
          suggested.flatMap((parameter) => {
            const option = draft.values[parameter.id];
            return option ? [[parameter.id, option]] : [];
          }),
        ),
      },
      {
        onSuccess: () => {
          if (!addAnother) {
            onClose();
            return;
          }
          setDraft(EMPTY);
          setRound((n) => n + 1);
          titleRef.current?.focus();
        },
      },
    );
  };

  const onKeyDown = (event: KeyboardEvent<HTMLFormElement>) => {
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      event.currentTarget.requestSubmit();
    }
  };

  return (
    <ModalOverlay
      isOpen
      isDismissable
      onOpenChange={(open) => !open && requestClose()}
      className="fixed inset-x-0 top-0 bottom-(--keyboard-inset) z-30 flex items-end justify-center bg-black/40 pt-[max(env(safe-area-inset-top),1rem)] sm:bottom-0 sm:items-start sm:p-4 sm:pt-[10dvh]"
    >
      <Modal className="max-h-full w-full overflow-y-auto rounded-t-2xl bg-surface text-ink shadow-xl outline-none sm:max-h-[80dvh] sm:max-w-lg sm:rounded-2xl">
        <Dialog aria-labelledby={titleId} className="outline-none">
          <form
            onSubmit={submit}
            onKeyDown={onKeyDown}
            className="flex flex-col gap-4 p-4 pb-[max(env(safe-area-inset-bottom),1rem)] sm:p-5"
          >
            <Heading slot="title" id={titleId} className="sr-only">
              Add an item
            </Heading>
            <label className="flex flex-col">
              <span className="sr-only">Title</span>
              <input
                ref={titleRef}
                autoFocus
                value={draft.title}
                onChange={(e) => set("title", e.target.value)}
                placeholder="What needs doing?"
                autoComplete="off"
                className="bg-transparent py-1 text-lg font-medium outline-none placeholder:text-muted"
              />
            </label>
            <label className="flex flex-col gap-1.5">
              <span className="text-xs font-medium text-ink-soft">Description</span>
              <textarea
                value={draft.description}
                onChange={(e) => set("description", e.target.value)}
                rows={2}
                className="rounded-lg border border-line bg-surface px-2 py-1.5 text-sm"
              />
            </label>
            <NeedPicker value={draft.need} onChange={(need) => set("need", need)} />
            <div className="grid grid-cols-2 gap-3">
              <ChoiceButtons
                label="Effort"
                options={SIZE_OPTIONS}
                value={draft.effort}
                onChange={(v) => set("effort", v)}
              />
              <ChoiceButtons
                label="Impact"
                options={SIZE_OPTIONS}
                value={draft.impact}
                onChange={(v) => set("impact", v)}
              />
            </div>
            <DuePicker key={round} value={draft.due} onChange={(due) => set("due", due)} />
            <TagPicker heapRef={heapRef} value={draft.tags} onChange={(tags) => set("tags", tags)} />
            {suggested.length > 0 && (
              <div className="flex flex-col gap-3" role="group" aria-label="Parameters for these tags">
                {suggested.map((parameter) => (
                  <ParameterPicker
                    key={parameter.id}
                    parameter={parameter}
                    value={draft.values[parameter.id] ?? null}
                    onChange={(option) => set("values", { ...draft.values, [parameter.id]: option })}
                  />
                ))}
              </div>
            )}

            {create.error && (
              <p role="alert" className="text-sm text-overdue-ink">
                {create.error.message}
              </p>
            )}

            {confirmingDiscard ? (
              <div role="alert" className="flex flex-wrap items-center gap-2 border-t border-line pt-4">
                <span className="mr-auto text-sm">Discard this item?</span>
                <button
                  type="button"
                  autoFocus
                  onClick={() => setConfirmingDiscard(false)}
                  className="h-11 rounded-xl px-4 text-sm font-medium text-ink-soft hover:bg-sunken"
                >
                  Keep editing
                </button>
                <button
                  type="button"
                  onClick={onClose}
                  className="h-11 rounded-xl bg-overdue-ink px-4 text-sm font-semibold text-surface"
                >
                  Discard
                </button>
              </div>
            ) : (
              <div className="flex flex-wrap items-center gap-2 border-t border-line pt-4">
                <label className="mr-auto inline-flex min-h-11 items-center gap-2 text-sm text-ink-soft">
                  <input
                    type="checkbox"
                    checked={addAnother}
                    onChange={(e) => onAddAnother(e.target.checked)}
                    className="size-4 accent-ink"
                  />
                  Add another
                </label>
                <button
                  type="button"
                  onClick={requestClose}
                  className="h-11 rounded-xl px-4 text-sm font-medium text-ink-soft hover:bg-sunken"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={!canAdd}
                  title="⌘↵"
                  className="h-11 rounded-xl bg-ink px-5 text-sm font-semibold text-canvas disabled:opacity-40"
                >
                  Add
                </button>
              </div>
            )}
          </form>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
