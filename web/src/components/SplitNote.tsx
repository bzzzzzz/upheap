import { Check, X } from "lucide-react";
import { useRef, useState, type FormEvent, type ReactNode } from "react";
import {
  Dialog,
  Heading,
  ListBox,
  ListBoxItem,
  Modal,
  ModalOverlay,
  ToggleButton,
  ToggleButtonGroup,
  type Selection,
} from "react-aria-components";

import { ApiProblem, type HeapRef } from "../api/client";
import type { Age, Heap, Item, Need } from "../api/schema";
import { useCreateHeap } from "../api/queries/heaps";
import { useCachedItem, useCreateItem } from "../api/queries/items";
import { useBlockerCandidates } from "../api/queries/lists";
import { useBecame, useNoteToStep } from "../api/queries/moves";
import { useTeam } from "../api/queries/teams";
import { compactButtonClass } from "./buttons";
import { choiceButtonClass, ChoiceButtons } from "./ChoiceButtons";
import { itemProblem } from "./itemProblems";
import { isClosed } from "./labels";
import { labelClass } from "./ItemQuestions";
import { PHONE, useMedia } from "./media";
import { NeedPicker } from "./NeedPicker";
import { problemMessage } from "./ProblemText";
import { inputClass } from "../pages/settingsForm";
import { useFocusAfterRender } from "./useFocusAfterRender";
import { useToast } from "./UndoToast";

type Outcome = "tasks" | "step" | "heap";

const OUTCOMES: readonly { id: Outcome; label: string }[] = [
  { id: "tasks", label: "Tasks" },
  { id: "step", label: "A step" },
  { id: "heap", label: "A heap" },
];

const AGE_OPTIONS: readonly { id: Age; label: string }[] = [
  { id: "keep", label: "Keep the note's age" },
  { id: "new", label: "Start new" },
];

type RowKind = "task" | "note";

const KIND_OPTIONS: readonly { id: RowKind; label: string }[] = [
  { id: "task", label: "Task" },
  { id: "note", label: "Note" },
];

/** One item to make from the note. `made` rows are locked: the server has them. */
type Row = {
  id: number;
  title: string;
  need: Need | null;
  age: Age | null;
  kind: RowKind;
  made: boolean;
  error: string | null;
};

const newRow = (id: number): Row => ({ id, title: "", need: null, age: null, kind: "task", made: false, error: null });

const isComplete = (row: Row) => row.title.trim() !== "" && row.need !== null && row.age !== null;
const isTouched = (row: Row) => row.title !== "" || row.need !== null || row.age !== null;

const NAME_LENGTH = 100;

const isProblem = (error: unknown, code: string) => error instanceof ApiProblem && error.code === code;

/**
 * Splits an open note into tasks, a step of an open task of its heap, or a new heap with its first items (ux.md, The
 * item §6). A sheet on a phone, a centred dialog from a tablet up. Items are made one at a time and the note is closed
 * last, so a failure part-way leaves what was made, locked, and the note open; Make the rest carries on from there.
 * Each outcome keeps its own draft while the dialog is open, and every opening starts a fresh one. `onDone` runs once
 * the note has closed, `onClose` when the dialog is left without that.
 */
export function SplitNoteDialog({
  heapRef,
  item,
  isOpen,
  onClose,
  onDone,
}: {
  heapRef: HeapRef;
  item: Item;
  isOpen: boolean;
  onClose: () => void;
  onDone: () => void;
}) {
  return isOpen ? <SplitNote heapRef={heapRef} item={item} onClose={onClose} onDone={onDone} /> : null;
}

function SplitNote({
  heapRef,
  item,
  onClose,
  onDone,
}: {
  heapRef: HeapRef;
  item: Item;
  onClose: () => void;
  onDone: () => void;
}) {
  const phone = useMedia(PHONE);
  const showToast = useToast();
  const focusAfterRender = useFocusAfterRender();
  const team = useTeam(heapRef.team);
  const createItem = useCreateItem();
  const createHeap = useCreateHeap(heapRef.team);
  const became = useBecame(heapRef);
  const toStep = useNoteToStep(heapRef);
  const cachedItem = useCachedItem(heapRef);

  const nextId = useRef(4);
  const titleInputs = useRef(new Map<number, HTMLInputElement>());
  const [outcome, setOutcome] = useState<Outcome>("tasks");
  const candidates = useBlockerCandidates(heapRef, outcome === "step");
  const [taskRows, setTaskRows] = useState<Row[]>([newRow(1), newRow(2)]);
  const [heapRows, setHeapRows] = useState<Row[]>([newRow(3)]);
  const initialName = item.title.slice(0, NAME_LENGTH);
  const initialContext = item.description ?? "";
  const [name, setName] = useState(initialName);
  const [context, setContext] = useState(initialContext);
  const [madeHeap, setMadeHeap] = useState<Heap | null>(null);
  const [heapError, setHeapError] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [taskId, setTaskId] = useState<string | null>(null);
  const [stepError, setStepError] = useState<string | null>(null);
  const [becameError, setBecameError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [closedMeanwhile, setClosedMeanwhile] = useState(false);
  const [confirmingDiscard, setConfirmingDiscard] = useState(false);

  const canMakeHeaps = team.data?.allowed_actions.includes("create_heap") ?? false;
  const rows = outcome === "heap" ? heapRows : taskRows;
  const setRows = outcome === "heap" ? setHeapRows : setTaskRows;
  const anyMade = madeHeap !== null || taskRows.some((row) => row.made) || heapRows.some((row) => row.made);

  const dirty =
    taskRows.some(isTouched) ||
    heapRows.some(isTouched) ||
    name !== initialName ||
    context !== initialContext ||
    taskId !== null;

  const openTasks = (candidates.data ?? []).map((entry) => entry.item).filter((entry) => entry.kind === "task");
  const needle = filter.trim().toLowerCase();
  const shownTasks = openTasks.filter(
    (task) => task.title.toLowerCase().includes(needle) || task.key.toLowerCase().includes(needle),
  );

  const canMake =
    !busy &&
    (outcome === "step" ? taskId !== null : rows.every(isComplete) && (outcome === "tasks" || name.trim() !== ""));

  const patchRow = (id: number, patch: Partial<Row>) =>
    setRows((current) => current.map((row) => (row.id === id ? { ...row, ...patch } : row)));

  const addRow = () => {
    const id = nextId.current++;
    setRows((current) => [...current, newRow(id)]);
    focusAfterRender(() => titleInputs.current.get(id));
  };

  const requestClose = () => {
    if (busy) return;
    if (closedMeanwhile || !(dirty || anyMade)) onClose();
    else setConfirmingDiscard(true);
  };

  const done = (message: string) => {
    showToast({ message: `“${item.title}” ${message}` });
    onDone();
  };

  /** An error from a call that needs the note open: the note being closed ends the split, whatever else was made. */
  const noteClosed = (error: unknown) => isProblem(error, "origin_closed") || isProblem(error, "item_closed");

  const makeStep = async () => {
    if (taskId === null) return;
    setStepError(null);
    try {
      await toStep.mutateAsync({ key: item.key, body: { task_id: taskId } });
      done("became a step.");
    } catch (error) {
      if (isProblem(error, "item_closed")) {
        // Settling fetched the note and the tasks again: closed, it was the note; else the task.
        const note = cachedItem(item.key);
        if (note && isClosed(note.state)) {
          setClosedMeanwhile(true);
        } else {
          setTaskId(null);
          setStepError("That task was closed meanwhile. Pick another.");
        }
      } else {
        setStepError(itemProblem(error as Error));
      }
    }
  };

  const makeItems = async () => {
    setBecameError(null);
    let target = heapRef;
    if (outcome === "heap") {
      let heap = madeHeap;
      if (heap === null) {
        setHeapError(null);
        try {
          heap = await createHeap.mutateAsync({ name: name.trim(), context, origin_id: item.id });
        } catch (error) {
          if (isProblem(error, "origin_closed")) setClosedMeanwhile(true);
          else setHeapError(problemMessage(error as Error));
          return;
        }
        setMadeHeap(heap);
      }
      target = { team: heapRef.team, heap: heap.slug };
    }
    for (const row of rows) {
      if (row.made) continue;
      patchRow(row.id, { error: null });
      try {
        await createItem.mutateAsync({
          heap: target,
          body: {
            title: row.title.trim(),
            need: row.need ?? "idea",
            kind: outcome === "heap" ? row.kind : "task",
            origin: { item_id: item.id, age: row.age ?? "keep" },
          },
        });
      } catch (error) {
        if (isProblem(error, "origin_closed")) setClosedMeanwhile(true);
        else patchRow(row.id, { error: itemProblem(error as Error) });
        return;
      }
      patchRow(row.id, { made: true });
    }
    try {
      await became.mutateAsync(item.key);
    } catch (error) {
      if (noteClosed(error)) setClosedMeanwhile(true);
      else setBecameError(itemProblem(error as Error));
      return;
    }
    done(outcome === "heap" ? "became a heap." : `became ${rows.length === 1 ? "1 task" : `${rows.length} tasks`}.`);
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!canMake) return;
    setBusy(true);
    void (outcome === "step" ? makeStep() : makeItems()).finally(() => setBusy(false));
  };

  const count = rows.length;
  const mainLabel = busy
    ? "Making…"
    : anyMade
      ? "Make the rest"
      : outcome === "step"
        ? "Make it a step"
        : outcome === "tasks"
          ? `Make ${count} ${count === 1 ? "task" : "tasks"}`
          : `Make the heap and ${count} ${count === 1 ? "item" : "items"}`;

  const rowList = (withKind: boolean) => (
    <div className="flex flex-col gap-3">
      {rows.map((row, index) => (
        <RowFields
          key={row.id}
          row={row}
          number={index + 1}
          max={item.need}
          phone={phone}
          withKind={withKind}
          disabled={busy}
          removable={rows.length > 1 && !row.made}
          autoFocus={outcome === "tasks" && index === 0}
          inputRef={(input) => {
            if (input) titleInputs.current.set(row.id, input);
            else titleInputs.current.delete(row.id);
          }}
          onChange={(patch) => patchRow(row.id, patch)}
          onRemove={() => setRows((current) => current.filter(({ id }) => id !== row.id))}
        />
      ))}
      <button type="button" onClick={addRow} disabled={busy} className={`${compactButtonClass.quiet} self-start`}>
        Add another
      </button>
    </div>
  );

  const stepPicker = (
    <div className="flex flex-col gap-2">
      <input
        aria-label="Filter by title or key"
        placeholder="Filter by title or key"
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
        disabled={busy}
        autoComplete="off"
        className={inputClass}
      />
      <ListBox
        aria-label="Open tasks"
        selectionMode="single"
        selectedKeys={taskId === null ? [] : [taskId]}
        onSelectionChange={(keys: Selection) => {
          const [picked] = keys === "all" ? [] : [...keys];
          setTaskId(picked === undefined ? null : String(picked));
          setStepError(null);
        }}
        disabledKeys={busy ? shownTasks.map((task) => task.id) : []}
        items={shownTasks}
        renderEmptyState={() => (
          <p className="px-3 py-2 text-sm text-muted">
            {candidates.isPending
              ? "Loading…"
              : candidates.isError
                ? candidates.error.message
                : openTasks.length === 0
                  ? "There are no open tasks in this heap."
                  : "No open task matches."}
          </p>
        )}
        className="flex max-h-64 flex-col overflow-y-auto rounded-lg border border-line outline-none"
      >
        {(task) => (
          <ListBoxItem
            id={task.id}
            textValue={task.title}
            className="flex min-h-9 cursor-default flex-col px-3 py-1.5 text-sm outline-none data-[focus-visible]:outline-2 data-[focus-visible]:-outline-offset-2 data-[focus-visible]:outline-accent data-[hovered]:bg-sunken data-[selected]:bg-accent-soft data-[selected]:font-semibold pointer-coarse:min-h-11"
          >
            <span className="break-words">{task.title}</span>
            <span className="text-xs font-normal text-muted">{task.key}</span>
          </ListBoxItem>
        )}
      </ListBox>
      {stepError && (
        <p role="alert" className="text-sm text-overdue-ink">
          {stepError}
        </p>
      )}
      <p className="text-xs text-muted">
        The note's title and context become the step's, at the end of the task's steps.
      </p>
    </div>
  );

  const heapFields = (
    <div className="flex flex-col gap-3">
      <div className="flex flex-col gap-1.5">
        <label className="flex flex-col gap-1.5">
          <span className={labelClass}>
            Name{madeHeap && <span className="ml-2 font-normal text-muted">Heap made</span>}
          </span>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            maxLength={NAME_LENGTH}
            disabled={busy || madeHeap !== null}
            autoComplete="off"
            className={inputClass}
          />
        </label>
        {heapError && (
          <p role="alert" className="text-sm text-overdue-ink">
            {heapError}
          </p>
        )}
      </div>
      <label className="flex flex-col gap-1.5">
        <span className={labelClass}>Context</span>
        <textarea
          value={context}
          onChange={(e) => setContext(e.target.value)}
          rows={3}
          disabled={busy || madeHeap !== null}
          className={inputClass}
        />
      </label>
      <h3 className="pt-1 text-sm font-semibold">First items</h3>
      {rowList(true)}
    </div>
  );

  const body = closedMeanwhile ? (
    <p className="text-sm">This note was closed meanwhile.</p>
  ) : (
    <>
      <div className="flex flex-col gap-1.5">
        <span id="split-into" className={labelClass}>
          Into
        </span>
        <ToggleButtonGroup
          aria-labelledby="split-into"
          selectionMode="single"
          disallowEmptySelection
          selectedKeys={[outcome]}
          onSelectionChange={(keys) => {
            const [picked] = [...keys];
            if (picked !== undefined) setOutcome(String(picked) as Outcome);
          }}
          isDisabled={busy || anyMade}
          className="grid auto-cols-fr grid-flow-col gap-1"
        >
          {OUTCOMES.filter(({ id }) => id !== "heap" || canMakeHeaps).map(({ id, label }) => (
            <ToggleButton key={id} id={id} className={`${choiceButtonClass} rounded-lg px-1`}>
              {label}
            </ToggleButton>
          ))}
        </ToggleButtonGroup>
      </div>
      {outcome === "tasks" && rowList(false)}
      {outcome === "step" && stepPicker}
      {outcome === "heap" && heapFields}
    </>
  );

  const footerButton = (label: string, onClick: () => void, className: string, autoFocus = false) => (
    <button type="button" autoFocus={autoFocus} onClick={onClick} className={className}>
      {label}
    </button>
  );

  let footer: ReactNode;
  if (closedMeanwhile) {
    footer = footerButton("Close", onClose, compactButtonClass.primary, true);
  } else if (confirmingDiscard) {
    footer = (
      <div role="alert" className="flex flex-1 flex-wrap items-center justify-end gap-2">
        <span className="mr-auto text-sm">Discard this split?</span>
        {footerButton("Keep editing", () => setConfirmingDiscard(false), compactButtonClass.quiet, true)}
        {footerButton("Discard", onClose, compactButtonClass.danger)}
      </div>
    );
  } else {
    footer = (
      <>
        {!phone && footerButton("Cancel", requestClose, compactButtonClass.quiet)}
        <button
          type="submit"
          disabled={!canMake}
          className={`${compactButtonClass.primary} ${phone ? "h-13 flex-1 rounded-[14px]" : ""}`}
        >
          {mainLabel}
        </button>
      </>
    );
  }

  return (
    <ModalOverlay
      isOpen
      isDismissable={!busy}
      isKeyboardDismissDisabled={busy}
      onOpenChange={(open) => !open && requestClose()}
      className="fixed inset-x-0 top-0 bottom-(--keyboard-inset) z-50 flex items-end justify-center bg-black/40 pt-[max(env(safe-area-inset-top),1rem)] sm:bottom-0 sm:items-start sm:px-4 sm:pt-12 sm:pb-4"
    >
      <Modal className="flex h-full w-full flex-col overflow-hidden rounded-t-[22px] bg-surface text-ink shadow-xl outline-none sm:h-auto sm:max-h-[85vh] sm:max-w-[560px] sm:rounded-2xl">
        <Dialog className="flex min-h-0 flex-1 flex-col outline-none">
          <form onSubmit={submit} className="flex min-h-0 flex-1 flex-col">
            <div className="flex shrink-0 items-start gap-2.5 px-4 pt-4 sm:px-6 sm:pt-5.5">
              <Heading slot="title" className="flex-1 text-base font-semibold break-words">
                Split “{item.title}”
              </Heading>
              {phone && !closedMeanwhile && (
                <button
                  type="button"
                  onClick={requestClose}
                  disabled={busy}
                  aria-label="Cancel"
                  className="flex size-11 shrink-0 items-center justify-center rounded-lg bg-sunken text-ink-soft hover:text-ink disabled:opacity-40"
                >
                  <X aria-hidden="true" className="size-4" />
                </button>
              )}
            </div>
            <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-4 pt-4 pb-4 sm:px-6">{body}</div>
            <div className="flex shrink-0 flex-col gap-2 border-t border-line px-4 pt-3 pb-[max(env(safe-area-inset-bottom),1rem)] sm:px-6 sm:pb-5">
              {becameError && !confirmingDiscard && !closedMeanwhile && (
                <p role="alert" className="text-sm text-overdue-ink">
                  {becameError}
                </p>
              )}
              <div className="flex items-center justify-end gap-2.5">{footer}</div>
            </div>
          </form>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}

/**
 * One item to make: a title, a need, an age, and for a heap's first items the kind. A made row shows "Made" and its
 * inputs are off; a failed one says why under it.
 */
function RowFields({
  row,
  number,
  max,
  phone,
  withKind,
  disabled,
  removable,
  autoFocus,
  inputRef,
  onChange,
  onRemove,
}: {
  row: Row;
  number: number;
  max: Need;
  phone: boolean;
  withKind: boolean;
  disabled: boolean;
  removable: boolean;
  autoFocus: boolean;
  inputRef: (input: HTMLInputElement | null) => void;
  onChange: (patch: Partial<Row>) => void;
  onRemove: () => void;
}) {
  const off = disabled || row.made;
  return (
    <div
      role="group"
      aria-label={`Row ${number}`}
      className={`flex flex-col gap-3 ${number > 1 ? "border-t border-line pt-3" : ""}`}
    >
      <div className="flex items-center gap-2">
        <input
          ref={inputRef}
          aria-label="Title"
          placeholder="Title"
          value={row.title}
          onChange={(e) => onChange({ title: e.target.value })}
          disabled={off}
          autoFocus={autoFocus}
          autoComplete="off"
          className={`${inputClass} flex-1`}
        />
        {row.made && (
          <span className="flex shrink-0 items-center gap-1 text-xs text-muted">
            <Check aria-hidden="true" className="size-3.5" strokeWidth={3} />
            Made
          </span>
        )}
        {removable && (
          <button
            type="button"
            aria-label={`Remove row ${number}`}
            onClick={onRemove}
            disabled={disabled}
            className={compactButtonClass.icon}
          >
            <X aria-hidden="true" className="size-4" />
          </button>
        )}
      </div>
      <div
        className={phone ? "flex flex-col gap-3" : "grid grid-cols-[84px_minmax(0,1fr)] items-start gap-x-3 gap-y-3.5"}
      >
        <NeedPicker
          layout={phone ? "chips" : "grid"}
          value={row.need}
          onChange={(need) => onChange({ need })}
          disabled={off}
          max={max}
        />
        <ChoiceButtons
          label="Age"
          layout={phone ? "stack" : "grid"}
          options={AGE_OPTIONS}
          value={row.age}
          onChange={(age) => onChange({ age })}
          disabled={off}
        />
        {withKind && (
          <ChoiceButtons
            label="Kind"
            layout={phone ? "stack" : "grid"}
            options={KIND_OPTIONS}
            value={row.kind}
            // The kind is always one or the other: pressing the picked one again keeps it.
            onChange={(kind) => kind && onChange({ kind })}
            disabled={off}
          />
        )}
      </div>
      {row.error && (
        <p role="alert" className="text-sm text-overdue-ink">
          {row.error}
        </p>
      )}
    </div>
  );
}
