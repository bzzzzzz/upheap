import { Link, useNavigate } from "@tanstack/react-router";
import { Check, ChevronDown, Plus, X } from "lucide-react";
import {
  useEffect,
  useId,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
} from "react";
import { Dialog, Modal, ModalOverlay, ToggleButton, ToggleButtonGroup } from "react-aria-components";

import { requestIdOf, type HeapRef } from "../api/client";
import type { DueInput, Goal, Item, ItemTag, Me, Need, Parameter, Size } from "../api/schema";
import { useHeap } from "../api/queries/heaps";
import { useCreateItem } from "../api/queries/items";
import { useRefetchRankedList } from "../api/queries/lists";
import { useMe } from "../api/queries/me";
import { useParameters } from "../api/queries/parameters";
import { heapTags, useTags } from "../api/queries/tags";
import { useAddAnother } from "./addAnother";
import { ChoiceButtons, FieldFrame, FieldLabel, choiceButtonClass } from "./ChoiceButtons";
import { DateField } from "./DateField";
import { DuePicker } from "./DuePicker";
import { RequestId } from "./RequestId";
import { itemProblem } from "./itemProblems";
import { fieldClass } from "./ItemTurn";
import { GOAL_LABELS, SIZE_LABELS, SIZES } from "./labels";
import { itemAddress, listQueryFor, matchesSearch, placeOf, splitList, type ListSearch } from "./listView";
import { PHONE, useMedia } from "./media";
import { Kbd, MAC, WIDE_HINT } from "./Kbd";
import { NeedPicker } from "./NeedPicker";
import { ParameterPicker } from "./ParameterPicker";
import { PopoverSheet } from "./PopoverSheet";
import { formatDate, isToCome } from "./reminders";
import { Sheet, SheetChoice } from "./Sheet";
import { TagPicker } from "./TagPicker";
import { tagChipColor } from "./tagColor";
import { teamName } from "./teamName";
import { useToast, useToastCount } from "./UndoToast";

/** What is on screen over the page, so a key that opens another dialog waits: a dialog, or a confirmation. */
export const OVERLAY = "[role=dialog], [role=alertdialog]";

const SIZE_OPTIONS = SIZES.map((size) => ({ id: size, label: SIZE_LABELS[size] }));

/** Whether a new note goes to an agent as it's added, and how far (ux.md, Adding an item §6). */
type AgentChoice = "none" | "with_context" | "defined" | "all_the_way";

/** The choices, with the name the wide row's buttons give each and the name the phone's sheet does. */
const AGENT_CHOICES: readonly { id: AgentChoice; label: string; sheetLabel: string }[] = [
  { id: "none", label: "Don't send", sheetLabel: "Don't send" },
  { id: "with_context", label: "With context", sheetLabel: "To with context" },
  { id: "defined", label: "Defined", sheetLabel: "To defined" },
  { id: "all_the_way", label: "All the way", sheetLabel: "All the way" },
];

/** The longest line the server takes for an agent. */
const MAX_AGENT_LINE = 2000;

/**
 * The way in to adding an item (ux.md, Adding an item): the Add item button beside the heap name, or `N`, opens
 * the Add dialog. It's a modal over the list on a wide screen and a tablet, and a sheet from the bottom on a phone, where the
 * + in the middle of the tabs opens it instead of this button (ux.md, Layout). `view` is the
 * list's, for the dialog to start from, and `search` the text the list is narrowed by.
 */
export function AddItem({
  heapRef,
  view,
  search,
  acrossHeaps,
  disabled,
}: {
  heapRef: HeapRef;
  view: ListSearch;
  search: string;
  /** The list is the one across heaps: `view` is for the prefill only, and the item opens over that list. */
  acrossHeaps?: boolean;
  disabled: boolean;
}) {
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (disabled) return;
    const onKey = (event: globalThis.KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing = target?.closest("input, textarea, select, [contenteditable]");
      // Not over another dialog or sheet, where the new one would open underneath it.
      const overlaid = document.querySelector(OVERLAY) !== null;
      if (event.key === "n" && !typing && !overlaid && !event.metaKey && !event.ctrlKey && !event.altKey) {
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
        className="inline-flex h-9 shrink-0 items-center gap-2 rounded-[10px] bg-ink px-3.5 text-sm font-semibold text-canvas outline-offset-2 focus-visible:outline-2 focus-visible:outline-accent disabled:opacity-40 max-sm:hidden"
      >
        <Plus aria-hidden="true" className="size-4 shrink-0" strokeWidth={2.25} />
        Add item
        <kbd aria-hidden="true" className="font-sans text-xs font-medium opacity-70">
          N
        </kbd>
      </button>
      {open && (
        <AddItemDialog
          heapRef={heapRef}
          view={view}
          search={search}
          acrossHeaps={acrossHeaps}
          onClose={() => setOpen(false)}
        />
      )}
    </>
  );
}

/** A new item as the dialog holds it before it's added. */
type Draft = {
  /** A note, or a reminder, which has a date instead of a due time. */
  kind: "note" | "reminder";
  title: string;
  description: string;
  /** Required: Add stays off until one is picked. */
  need: Need | null;
  effort: Size | null;
  impact: Size | null;
  due: DueInput | null;
  /** The reminder's date, `YYYY-MM-DD`. Required for a reminder, and never sent for a note. */
  date: string | null;
  tags: ItemTag[];
  /** Option ID by parameter ID. */
  values: Record<string, string | null>;
  /** Whether the note is sent to an agent as it's added, and how far. Never sent for a reminder. */
  agent: AgentChoice;
  /** What to tell the agent with it; only with an agent picked. */
  agentLine: string;
};

const EMPTY: Draft = {
  kind: "note",
  title: "",
  description: "",
  need: null,
  effort: null,
  impact: null,
  due: null,
  date: null,
  tags: [],
  values: {},
  agent: "none",
  agentLine: "",
};

/** An item just added, for saying where it landed. `place` is its place in the list on screen, if it shows there. */
type Added = {
  title: string;
  key: string;
  heap: HeapRef;
  heapName: string;
  place: { position: number; section: string | undefined } | undefined;
  /** The date a reminder to come joins the list, as people say it; it has no place until then. */
  joins: string | undefined;
  /** Where the agent it was sent to is going, when it was sent with the add. */
  sentTo: Goal | undefined;
};

/**
 * "Added “Title” at #4 in Up next.", or where it went when the list on screen doesn't show it. A reminder to come says
 * when it joins the list instead.
 */
const addedMessage = ({ title, heapName, place, joins, sentTo }: Added) => {
  if (joins) return `Added. It joins the list on ${joins}.`;
  const sent = sentTo ? `, sent to an agent going to: ${GOAL_LABELS[sentTo]}` : "";
  return place
    ? `Added “${title}” at #${place.position}${place.section ? ` in ${place.section}` : ""}${sent}.`
    : `Added “${title}” to ${heapName}${sent}.`;
};

/** The same, short enough for the corner of a phone's sheet. */
const addedNote = ({ title, heapName, place, joins, sentTo }: Added) => {
  if (joins) return `“${title}” joins the list on ${joins}`;
  const sent = sentTo ? " · sent" : "";
  return place ? `“${title}” added at #${place.position}${sent}` : `“${title}” added to ${heapName}${sent}`;
};

const sameHeap = (a: HeapRef, b: HeapRef) => a.team === b.team && a.heap === b.heap;

/** An add on its way: where it goes, whether Add another was on, the edits and title it was made from, its number. */
type SentAdd = {
  heap: HeapRef;
  heapName: string;
  onScreen: boolean;
  addAnother: boolean;
  /** Whether it was sent to an agent with the add, rather than left to the heap's prepare turn. */
  sending: boolean;
  edits: Partial<Draft>;
  title: string;
  number: number;
  /** How many toasts had been shown when it was sent. */
  toasts: number;
};

/** The edits changed since `sent`, each change making a new value, so the rest can go. */
function changedSince(edits: Partial<Draft>, sent: Partial<Draft>): Partial<Draft> {
  return Object.fromEntries(
    Object.entries(edits).filter(([key, value]) => sent[key as keyof Draft] !== value),
  ) as Partial<Draft>;
}

/**
 * Every field Add item takes, the title focused (product.md, Add item). The title and the need are required. Picking
 * a tag brings up the parameters linked to it. Enter in the title or ⌘↵ adds; with Add another ticked the dialog
 * clears for the next item instead of closing. Closing with a title typed asks first, so a stray tap outside doesn't
 * lose it.
 *
 * It starts from the list's view (ux.md, Adding an item §3): the heap on screen, and the tags, effort, impact and
 * parameter options the filters pick out one of, without a label to say so. The heap can be changed from the title.
 * Once added, it says where the item landed in the list, from the list as the server ranks it.
 */
export function AddItemDialog({
  heapRef,
  view,
  search = "",
  acrossHeaps = false,
  title: initialTitle,
  onClose,
}: {
  heapRef: HeapRef;
  /** The list's view, when the dialog opens over the list. */
  view: ListSearch | undefined;
  /** The text the list on screen is narrowed by, which may hide the new item. */
  search?: string;
  /**
   * The list the dialog opens over is the one across heaps: `view` only fills the form in. The item's place in its
   * heap's list isn't its place there, so none is looked up, and Open it opens the item over that list.
   */
  acrossHeaps?: boolean;
  /** The title to start with, which counts as typed. */
  title?: string;
  onClose: () => void;
}) {
  const phone = useMedia(PHONE);
  const { data: me } = useMe();
  const navigate = useNavigate();
  const showToast = useToast();
  const toastCount = useToastCount();
  const refetchList = useRefetchRankedList();
  // For an add the server answers later: whether the dialog is still open (if not, it answers in a toast), which add
  // came last (only its message shows in the dialog), the heap now chosen, and the edits now made.
  const open = useRef(true);
  const latest = useRef(0);
  const chosenHeap = useRef(heapRef);
  const currentEdits = useRef<Partial<Draft>>({});
  useEffect(() => {
    open.current = true;
    return () => {
      open.current = false;
    };
  }, []);
  // The heap the item goes into, the one on screen unless another is chosen.
  const [target, setTarget] = useState(heapRef);
  const onScreen = sameHeap(target, heapRef);
  const create = useCreateItem();
  const parameters = useParameters(target);
  const targetHeap = useHeap(target);
  const prefill = usePrefill(heapRef, view);
  const [addAnother, setAddAnother] = useAddAnother();
  // What the user has set, over the view's prefill, which belongs to the heap on screen.
  const [edits, setEdits] = useState<Partial<Draft>>(initialTitle === undefined ? {} : { title: initialTitle });
  // The edits of the add on its way, until the server answers. While they're untouched there's nothing unsaved.
  const [sent, setSent] = useState<Partial<Draft> | null>(null);
  const draft: Draft = { ...EMPTY, ...(onScreen ? prefill : {}), ...edits };
  // A new key remounts the due picker, so a cleared draft doesn't keep Exact date picked.
  const [round, setRound] = useState(0);
  const [added, setAdded] = useState<Added | null>(null);
  // The add that failed last, with what went wrong.
  const [failed, setFailed] = useState<{ title: string; error: Error } | null>(null);
  const [confirmingDiscard, setConfirmingDiscard] = useState(false);
  // The item to open once the draft is discarded, when Open it asked first.
  const [openAfterDiscard, setOpenAfterDiscard] = useState<Added | null>(null);
  // On a phone, the description and the parameters are under More fields: shown from the start when the view gave a
  // parameter its value, so it's in sight.
  const [moreFields, setMoreFields] = useState<boolean | null>(null);
  const moreId = useId();
  const agentLabelId = useId();
  const titleRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    currentEdits.current = edits;
  });
  const set = <K extends keyof Draft>(key: K, value: Draft[K]) => setEdits((d) => ({ ...d, [key]: value }));
  const heapName = heapNameOf(me, target);
  const showMore = moreFields ?? Object.keys(draft.values).length > 0;

  // The parameters linked to a chosen tag, and any already given a value.
  const suggested = (parameters.data?.parameters ?? []).filter(
    (parameter) =>
      parameter.tag_ids.some((id) => draft.tags.some((tag) => tag.id === id)) || draft.values[parameter.id],
  );
  const askedBecause = (parameter: Parameter) => {
    const names = draft.tags.filter((tag) => parameter.tag_ids.includes(tag.id)).map((tag) => tag.name);
    if (names.length === 0) return undefined;
    return `Asked because of the ${names.length === 1 ? "tag" : "tags"} ${names.join(", ")}`;
  };

  const reminder = draft.kind === "reminder";
  // A reminder is never sent: its choice stays in the draft, hidden, for when it is a note again.
  const sending = !reminder && draft.agent !== "none";
  const prepares = targetHeap.data?.agents_prepare === true && draft.agent === "none";
  const canAdd =
    draft.title.trim() !== "" && draft.need !== null && (!reminder || draft.date !== null) && !create.isPending;
  const unsaved = draft.title.trim() !== "" && edits !== sent;

  const requestClose = () => {
    if (unsaved && !confirmingDiscard) {
      setConfirmingDiscard(true);
    } else {
      onClose();
    }
  };

  const openItem = (item: Added) => {
    if (acrossHeaps) {
      void navigate({
        to: "/all",
        search: (prev) => ({ ...prev, item: itemAddress(item.heap, item.key) }),
        resetScroll: false,
        state: (prev) => ({ ...prev, selecting: undefined, closesBack: true }),
      });
      return;
    }
    // Added to the heap of the list on screen; the phone's Add also opens on the heap's other pages, with no view.
    const overList = view !== undefined && sameHeap(item.heap, heapRef);
    void navigate({
      to: "/teams/$team/heaps/$heap/items/$item",
      params: { ...item.heap, item: item.key },
      // Over the list, the item opens over it as it is by then, and the page keeps its place; elsewhere, the list
      // starts afresh at the top.
      search: overList ? (prev) => prev : {},
      resetScroll: !overList,
      state: (prev) => ({ ...prev, selecting: undefined, closesBack: true }),
    });
  };

  // Open it in the dialog leaves the draft behind, so with a title typed it asks first.
  const onOpenAdded = (event: MouseEvent, item: Added) => {
    // A new tab or window leaves the dialog as it is.
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    if (unsaved) {
      setOpenAfterDiscard(item);
      setConfirmingDiscard(true);
      return;
    }
    onClose();
    openItem(item);
  };

  const chooseHeap = (next: HeapRef) => {
    if (sameHeap(next, target)) return;
    chosenHeap.current = next;
    setAdded(null);
    // Tags and parameter values belong to a heap; the rest carries over.
    setEdits({
      kind: draft.kind,
      title: draft.title,
      description: draft.description,
      need: draft.need,
      effort: draft.effort,
      impact: draft.impact,
      due: draft.due,
      date: draft.date,
      agent: draft.agent,
      agentLine: draft.agentLine,
    });
    setTarget(next);
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const need = draft.need;
    if (!canAdd || need === null) return;
    const add: SentAdd = {
      heap: target,
      heapName,
      onScreen,
      addAnother,
      sending,
      edits,
      title: draft.title,
      number: ++latest.current,
      toasts: toastCount(),
    };
    setSent(edits);
    setFailed(null);
    // Through the promise rather than `onSuccess`, which is dropped once the dialog has closed. It settles as soon as
    // the server has the item, so the next one can be typed and added while the list catches up.
    create
      .mutateAsync({
        heap: target,
        body: {
          title: draft.title,
          need,
          description: draft.description.trim() === "" ? null : draft.description,
          effort: draft.effort,
          impact: draft.impact,
          // A reminder's date is its deadline; the server makes it firm, so it says so too.
          ...(reminder && draft.date !== null
            ? { kind: "reminder" as const, due: { date: draft.date, firm: true } }
            : { due: draft.due }),
          ...(sending && {
            send: {
              reach: draft.agent === "all_the_way" ? ("all_the_way" as const) : ("one" as const),
              goal: draft.agent === "defined" ? ("defined" as const) : undefined,
              line: draft.agentLine.trim() === "" ? null : draft.agentLine.trim(),
            },
          }),
          tag_ids: draft.tags.map((tag) => tag.id),
          parameter_values: Object.fromEntries(
            suggested.flatMap((parameter) => {
              const option = draft.values[parameter.id];
              return option ? [[parameter.id, option]] : [];
            }),
          ),
        },
      })
      .then(
        (item) => void onAdded(item, add),
        (error: Error) => onFailed(error, add),
      );
  };

  /**
   * Once the server has the item: the dialog clears for the next one, or closes, at once; then, with the list on
   * screen fetched again, it says where the item landed, in the dialog or, without Add another or once the dialog is
   * gone, in a toast.
   */
  const onAdded = async (item: Item, add: SentAdd) => {
    setSent(null);
    if (!add.addAnother) {
      // Only this dialog: once it's gone, the parent's state may already belong to the next one the user opened.
      if (open.current) onClose();
    } else if (open.current) {
      // What was changed since Enter belongs to the next item, and stays.
      const next = changedSince(currentEdits.current, add.edits);
      setEdits(next);
      // A discard asked about the title just added; one typed since is still to be asked about.
      if (!next.title?.trim()) {
        setConfirmingDiscard(false);
        setOpenAfterDiscard(null);
      }
      setRound((n) => n + 1);
      titleRef.current?.focus();
    }

    // A reminder still to come is not in the ranked list, so there is no place to read from it.
    const joins = isToCome(item) && item.due ? formatDate(item.due.date) : undefined;
    const list =
      add.onScreen && view && !acrossHeaps && !joins
        ? await refetchList(heapRef, listQueryFor(view, search))
        : undefined;
    const place = list && placeOf(list, item.key);
    // Not a position the search on screen hides.
    const shown = place && matchesSearch(place.entry, search) ? place : undefined;
    const result: Added = {
      title: item.title,
      key: item.key,
      heap: add.heap,
      heapName: add.heapName,
      place: shown && { position: shown.entry.position, section: shown.section },
      joins,
      // Only a send the person chose: the heap's prepare turn was already said in the dialog.
      sentTo: add.sending ? item.turn?.going_to : undefined,
    };
    if (!add.addAnother || !open.current) {
      // Not over a toast about something done since, such as a row marked done.
      if (toastCount() === add.toasts) {
        showToast({ message: addedMessage(result), action: { label: "Open it", run: () => openItem(result) } });
      }
    } else if (latest.current === add.number) {
      // Under another heap's title, it says which heap it went into.
      setAdded(sameHeap(add.heap, chosenHeap.current) ? result : { ...result, place: undefined });
    }
  };

  /**
   * When the server refuses the item: the dialog says so, with the draft still there to try again, as nothing clears
   * it before the server has the item; once the dialog is gone, a toast says so.
   */
  const onFailed = (error: Error, add: SentAdd) => {
    if (!open.current) {
      showToast({
        message: `“${add.title}” wasn't added: ${itemProblem(error)}`,
        error: true,
        requestId: requestIdOf(error),
      });
      return;
    }
    setSent(null);
    setFailed({ title: add.title, error });
  };

  const onKeyDown = (event: KeyboardEvent<HTMLFormElement>) => {
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      event.currentTarget.requestSubmit();
    }
  };

  const openAdded = added && (
    <Link
      to="/teams/$team/heaps/$heap/items/$item"
      params={{ ...added.heap, item: added.key }}
      onClick={(event) => onOpenAdded(event, added)}
      className="shrink-0 font-medium underline"
    >
      Open it
    </Link>
  );

  const chooser = <HeapChooser me={me} value={target} name={heapName} onChange={chooseHeap} />;

  const kindChooser = (
    <ToggleButtonGroup
      aria-label="Kind"
      selectionMode="single"
      disallowEmptySelection
      selectedKeys={[draft.kind]}
      onSelectionChange={(keys) => {
        const [picked] = [...keys];
        if (picked !== undefined) set("kind", picked === "reminder" ? "reminder" : "note");
      }}
      className="flex gap-1"
    >
      <ToggleButton id="note" className={`${choiceButtonClass} rounded-lg px-4`}>
        Note
      </ToggleButton>
      <ToggleButton id="reminder" className={`${choiceButtonClass} rounded-lg px-4`}>
        Reminder
      </ToggleButton>
    </ToggleButtonGroup>
  );

  const title = (
    <label className="flex flex-col">
      <span className="sr-only">Title</span>
      <input
        ref={titleRef}
        autoFocus
        value={draft.title}
        onChange={(e) => set("title", e.target.value)}
        placeholder="What needs doing?"
        autoComplete="off"
        className={`min-w-0 border-b-2 border-accent bg-transparent font-medium outline-none placeholder:text-muted ${
          phone ? "py-1.5 text-[19px]" : "pt-1 pb-2 text-xl"
        }`}
      />
    </label>
  );

  const description = (
    <label className="flex flex-col gap-1.5 text-[13px] text-ink-soft">
      Context
      <textarea
        value={draft.description}
        onChange={(e) => set("description", e.target.value)}
        rows={2}
        placeholder="Optional"
        className="rounded-lg border border-line bg-surface px-2.5 py-2 text-sm text-ink placeholder:text-muted"
      />
    </label>
  );

  const parameterPickers = (layout: "stack" | "grid") =>
    suggested.length > 0 && (
      <div
        role="group"
        aria-label="Parameters for these tags"
        className={layout === "grid" ? "contents" : "flex flex-col gap-3"}
      >
        {suggested.map((parameter) => (
          <ParameterPicker
            key={parameter.id}
            parameter={parameter}
            layout={layout}
            hint={askedBecause(parameter)}
            value={draft.values[parameter.id] ?? null}
            onChange={(option) => set("values", { ...draft.values, [parameter.id]: option })}
          />
        ))}
      </div>
    );

  const error = failed && (
    <>
      <p role="alert" className="text-sm text-overdue-ink">
        “{failed.title}” wasn't added: {itemProblem(failed.error)}
      </p>
      <RequestId error={failed.error} />
    </>
  );

  const prepareNote = prepares && (
    <p className="text-xs text-muted">This heap's agents still take it one step, to with context.</p>
  );
  const agentLine = (
    <label className="flex flex-col gap-1.5 text-[13px] text-ink-soft">
      A line for the agent (optional)
      <textarea
        value={draft.agentLine}
        onChange={(e) => set("agentLine", e.target.value)}
        maxLength={MAX_AGENT_LINE}
        className={fieldClass}
      />
    </label>
  );
  const buttonClass = "h-11 rounded-xl px-4 text-sm font-medium";
  const discardRow = (
    <div role="alert" className="flex flex-wrap items-center gap-2">
      <span className="mr-auto text-sm">Discard this item?</span>
      <button
        type="button"
        autoFocus
        onClick={() => {
          setConfirmingDiscard(false);
          setOpenAfterDiscard(null);
        }}
        className={`${buttonClass} text-ink-soft hover:bg-sunken`}
      >
        Keep editing
      </button>
      <button
        type="button"
        onClick={() => {
          onClose();
          if (openAfterDiscard) openItem(openAfterDiscard);
        }}
        className={`${buttonClass} bg-overdue-ink font-semibold text-surface`}
      >
        Discard
      </button>
    </div>
  );
  const addAnotherBox = (
    <label className="inline-flex min-h-11 items-center gap-2 text-sm text-ink-soft">
      <input
        type="checkbox"
        checked={addAnother}
        onChange={(e) => setAddAnother(e.target.checked)}
        className="size-4 accent-ink pointer-coarse:size-5"
      />
      Add another
    </label>
  );
  const submitKeys = MAC ? "⌘↵" : "Ctrl ↵";
  const addButton = (
    <button
      type="submit"
      disabled={!canAdd}
      title={submitKeys}
      aria-keyshortcuts={MAC ? "Meta+Enter" : "Control+Enter"}
      className="inline-flex h-11 items-center gap-2 rounded-xl bg-ink px-5 text-sm font-semibold text-canvas disabled:opacity-40"
    >
      {sending ? "Add and send" : "Add"}
      <Kbd className={`${WIDE_HINT} items-center border-canvas/30 bg-transparent text-canvas/70`}>{submitKeys}</Kbd>
    </button>
  );

  return (
    <ModalOverlay
      isOpen
      isDismissable
      onOpenChange={(open) => !open && requestClose()}
      className="fixed inset-x-0 top-0 bottom-(--keyboard-inset) z-30 flex items-end justify-center bg-black/40 pt-[max(env(safe-area-inset-top),1rem)] sm:bottom-0 sm:items-start sm:px-4 sm:pt-12 sm:pb-4"
    >
      <Modal className="flex max-h-full w-full flex-col overflow-hidden rounded-t-[22px] bg-surface text-ink shadow-xl outline-none sm:max-w-[600px] sm:rounded-2xl">
        <Dialog aria-label={`Add an item to ${heapName}`} className="flex min-h-0 flex-col outline-none">
          <form onSubmit={submit} onKeyDown={onKeyDown} className="flex min-h-0 flex-col">
            {phone ? (
              <>
                {/* Above the keyboard: the title and the one-tap chips, with Add in reach at the bottom. */}
                <div className="flex shrink-0 flex-col gap-2 px-4 pt-2.5">
                  <span aria-hidden="true" className="mx-auto h-1 w-10 rounded-full bg-line" />
                  <div className="flex items-center gap-2 text-[13px] text-ink-soft">
                    <h2 className="shrink-0">Add to {chooser}</h2>
                    <div role="status" className="flex min-w-0 flex-1 items-center justify-end gap-1.5 text-done">
                      {added && (
                        <>
                          <Check aria-hidden="true" className="size-3.5 shrink-0" strokeWidth={3} />
                          <span className="min-w-0 truncate">{addedNote(added)}</span>
                          {openAdded}
                        </>
                      )}
                    </div>
                  </div>
                </div>
                <div className="flex min-h-0 flex-col gap-3 overflow-y-auto px-4 pt-1 pb-3">
                  {kindChooser}
                  {title}
                  <NeedPicker layout="chips" value={draft.need} onChange={(need) => set("need", need)} />
                  <PhoneChips heapRef={target} draft={draft} set={set} />
                  {reminder ? (
                    <DateField value={draft.date} onChange={(date) => set("date", date)} />
                  ) : (
                    <>
                      <DuePicker key={round} layout="chips" value={draft.due} onChange={(due) => set("due", due)} />
                      <div className="flex flex-col gap-1.5">
                        <AgentChip draft={draft} set={set} line={agentLine} />
                        {prepareNote}
                      </div>
                    </>
                  )}
                  {showMore && (
                    <div id={moreId} className="flex flex-col gap-3">
                      {description}
                      {parameterPickers("stack")}
                    </div>
                  )}
                  {error}
                </div>
                <div className="flex shrink-0 items-center gap-2.5 border-t border-line px-4 pt-1 pb-[max(env(safe-area-inset-bottom),0.75rem)]">
                  {confirmingDiscard ? (
                    <div className="flex-1">{discardRow}</div>
                  ) : (
                    <>
                      {addAnotherBox}
                      <button
                        type="button"
                        aria-expanded={showMore}
                        aria-controls={showMore ? moreId : undefined}
                        onClick={() => setMoreFields(!showMore)}
                        className="min-h-11 text-sm text-ink-soft"
                      >
                        More fields
                      </button>
                      <span className="flex-1" />
                      {/* With no ✕ in the sheet, for a screen reader, which can't tap outside it. */}
                      <button type="button" onClick={requestClose} className="sr-only">
                        Cancel
                      </button>
                      {addButton}
                    </>
                  )}
                </div>
              </>
            ) : (
              <>
                <div className="flex shrink-0 items-center gap-2.5 px-6 pt-5.5">
                  <h2 className="flex-1 text-base font-semibold">Add to {chooser}</h2>
                  <button
                    type="button"
                    onClick={requestClose}
                    aria-label="Close"
                    className="flex size-8 items-center justify-center rounded-lg bg-sunken text-ink-soft hover:text-ink pointer-coarse:size-11"
                  >
                    <X aria-hidden="true" className="size-4" />
                  </button>
                </div>
                <div className="flex min-h-0 flex-col gap-4 overflow-y-auto px-6 pt-4 pb-4">
                  <div role="status">
                    {added && (
                      <p className="flex items-center gap-2.5 rounded-[10px] bg-done/10 px-3 py-2 text-[13px] text-done">
                        <Check aria-hidden="true" className="size-4 shrink-0" strokeWidth={2.5} />
                        <span className="min-w-0 flex-1">{addedMessage(added)}</span>
                        {openAdded}
                      </p>
                    )}
                  </div>
                  {kindChooser}
                  {title}
                  {description}
                  <div className="grid grid-cols-[84px_minmax(0,1fr)] items-start gap-x-3 gap-y-3.5">
                    <NeedPicker layout="grid" value={draft.need} onChange={(need) => set("need", need)} />
                    <ChoiceButtons
                      label="Effort"
                      layout="grid"
                      options={SIZE_OPTIONS}
                      value={draft.effort}
                      onChange={(v) => set("effort", v)}
                    />
                    <ChoiceButtons
                      label="Impact"
                      layout="grid"
                      options={SIZE_OPTIONS}
                      value={draft.impact}
                      onChange={(v) => set("impact", v)}
                    />
                    {reminder ? (
                      <DateField layout="grid" value={draft.date} onChange={(date) => set("date", date)} />
                    ) : (
                      <DuePicker
                        key={round}
                        layout="grid"
                        noneChoice
                        value={draft.due}
                        onChange={(due) => set("due", due)}
                      />
                    )}
                    <TagPicker
                      heapRef={target}
                      layout="grid"
                      value={draft.tags}
                      onChange={(tags) => set("tags", tags)}
                    />
                    {parameterPickers("grid")}
                    {!reminder && (
                      <FieldFrame layout="grid" label={<FieldLabel id={agentLabelId}>Agent</FieldLabel>}>
                        <ToggleButtonGroup
                          aria-labelledby={agentLabelId}
                          selectionMode="single"
                          disallowEmptySelection
                          selectedKeys={[draft.agent]}
                          onSelectionChange={(keys) => {
                            const [picked] = [...keys];
                            const choice = AGENT_CHOICES.find((c) => c.id === picked);
                            if (choice) set("agent", choice.id);
                          }}
                          className="flex flex-wrap gap-1"
                        >
                          {AGENT_CHOICES.map((choice) => (
                            <ToggleButton key={choice.id} id={choice.id} className={`${choiceButtonClass} rounded-lg`}>
                              {choice.label}
                            </ToggleButton>
                          ))}
                        </ToggleButtonGroup>
                        {sending && agentLine}
                        {prepareNote}
                      </FieldFrame>
                    )}
                  </div>
                  {error}
                </div>
                <div className="mx-6 flex shrink-0 items-center gap-2.5 border-t border-line pt-3 pb-5">
                  {confirmingDiscard ? (
                    <div className="flex-1">{discardRow}</div>
                  ) : (
                    <>
                      {addAnotherBox}
                      <span className="flex-1" />
                      <button
                        type="button"
                        onClick={requestClose}
                        className={`${buttonClass} text-ink-soft hover:bg-sunken`}
                      >
                        Cancel
                      </button>
                      {addButton}
                    </>
                  )}
                </div>
              </>
            )}
          </form>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}

/** The name of a heap, from the caller's teams. */
function heapNameOf(me: Me | undefined, heap: HeapRef): string {
  return me?.teams.find((team) => team.slug === heap.team)?.heaps.find((h) => h.slug === heap.heap)?.name ?? heap.heap;
}

/**
 * What the dialog starts with from the list's view: the filtered tags, and effort, impact or a parameter's option
 * where the filters pick exactly one. A filter on several values doesn't say which one the new item has.
 */
function usePrefill(heapRef: HeapRef, view: ListSearch | undefined): Partial<Draft> {
  const tags = useTags(heapRef);
  const parameters = useParameters(heapRef);
  if (!view) return {};
  const only = <T extends string>(values: string[]) => (values.length === 1 ? (values[0] as T) : undefined);
  const effort = only<Size>(splitList(view.effort));
  const impact = only<Size>(splitList(view.impact));
  const tagIds = splitList(view.tag);
  const optionIds = splitList(view.param);
  const values = Object.fromEntries(
    (parameters.data?.parameters ?? []).flatMap((parameter) => {
      const options = parameter.options.filter((option) => optionIds.includes(option.id));
      return options.length === 1 && options[0] ? [[parameter.id, options[0].id]] : [];
    }),
  );
  return {
    ...(effort && { effort }),
    ...(impact && { impact }),
    tags: (tags.data ? heapTags(tags.data) : [])
      .filter((tag) => tagIds.includes(tag.id))
      .map(({ id, name, color }) => ({ id, name, color })),
    values,
  };
}

/**
 * The heap in the dialog's title, which opens the heaps the item can go into: a popover on a tablet or wide screen,
 * a sheet on a phone. Archived heaps are read-only, so they aren't offered. With nowhere else to go, it's the name.
 */
function HeapChooser({
  me,
  value,
  name,
  onChange,
}: {
  me: Me | undefined;
  value: HeapRef;
  name: string;
  onChange: (heap: HeapRef) => void;
}) {
  const [open, setOpen] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const teams = (me?.teams ?? [])
    .map((team) => ({ team, heaps: team.heaps.filter((heap) => !heap.archived) }))
    .filter(({ heaps }) => heaps.length > 0);
  if (teams.flatMap(({ heaps }) => heaps).length < 2) return <span className="font-semibold text-ink">{name}</span>;

  return (
    <>
      <button
        ref={trigger}
        type="button"
        aria-label={`Heap: ${name}`}
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen(true)}
        className="inline-flex max-w-full items-center gap-0.5 border-b border-dashed border-muted font-semibold text-ink outline-none focus-visible:outline-2 focus-visible:outline-accent"
      >
        <span className="truncate">{name}</span>
        <ChevronDown aria-hidden="true" className="size-4 shrink-0" />
      </button>
      <PopoverSheet
        title="Add to which heap?"
        triggerRef={trigger}
        isOpen={open}
        onClose={() => setOpen(false)}
        placement="bottom start"
      >
        <div className="-mx-1 flex flex-col">
          {teams.map(({ team, heaps }) => (
            <section
              key={team.id}
              aria-label={teams.length > 1 && me ? teamName(team, me) : undefined}
              className="flex flex-col"
            >
              {teams.length > 1 && (
                <h3 className="px-3 pt-2.5 pb-0.5 text-xs font-semibold tracking-wide text-muted uppercase">
                  {me && teamName(team, me)}
                </h3>
              )}
              <ToggleButtonGroup
                aria-label={teams.length > 1 && me ? `Heaps of ${teamName(team, me)}` : "Heaps"}
                selectionMode="single"
                orientation="vertical"
                selectedKeys={team.slug === value.team ? [value.heap] : []}
                onSelectionChange={(keys) => {
                  const [picked] = [...keys];
                  // An empty selection is the chosen heap picked again: nothing to change.
                  if (picked !== undefined) onChange({ team: team.slug, heap: String(picked) });
                  setOpen(false);
                }}
                className="flex flex-col"
              >
                {heaps.map((heap) => (
                  <SheetChoice key={heap.id} id={heap.slug}>
                    {heap.name}
                  </SheetChoice>
                ))}
              </ToggleButtonGroup>
            </section>
          ))}
        </div>
      </PopoverSheet>
    </>
  );
}

/** A chip of the phone's Add sheet: filled once it has a value, dashed while it has none. */
const chipClass = (filled: boolean) =>
  `flex h-9 shrink-0 items-center gap-1 rounded-lg border px-3 text-sm outline-none focus-visible:outline-2 focus-visible:outline-accent pointer-coarse:h-11 ${
    filled ? "border-ink bg-ink text-canvas" : "border-dashed border-muted bg-surface text-ink-soft"
  }`;

/**
 * Effort, impact and the tags as one row of chips that scrolls sideways (ux.md, Adding an item §2). Effort
 * and impact each open a sheet of S M L XL; a tag chip takes its tag off, and + Tag opens the tag picker in a sheet.
 */
function PhoneChips({
  heapRef,
  draft,
  set,
}: {
  heapRef: HeapRef;
  draft: Draft;
  set: <K extends keyof Draft>(key: K, value: Draft[K]) => void;
}) {
  const [tagsOpen, setTagsOpen] = useState(false);
  return (
    <div className="-mx-4 flex gap-1.5 overflow-x-auto px-4 [scrollbar-width:none]">
      <SizeChip label="Effort" value={draft.effort} onChange={(effort) => set("effort", effort)} />
      <SizeChip label="Impact" value={draft.impact} onChange={(impact) => set("impact", impact)} />
      {draft.tags.map((tag) => (
        <button
          key={tag.id}
          type="button"
          aria-label={`Remove tag ${tag.name}`}
          onClick={() =>
            set(
              "tags",
              draft.tags.filter((t) => t.id !== tag.id),
            )
          }
          className={`flex h-9 shrink-0 items-center gap-1 rounded-full border bg-transparent px-3 text-sm outline-none focus-visible:outline-2 focus-visible:outline-accent pointer-coarse:h-11 ${tagChipColor(tag.color)}`}
        >
          {tag.name}
          <span aria-hidden="true" className="text-muted">
            ×
          </span>
        </button>
      ))}
      <button type="button" aria-haspopup="dialog" onClick={() => setTagsOpen(true)} className={chipClass(false)}>
        + Tag
      </button>
      <Sheet title="Tags" isOpen={tagsOpen} onClose={() => setTagsOpen(false)}>
        <div className="pt-1 pb-2">
          <TagPicker heapRef={heapRef} value={draft.tags} inline onChange={(tags) => set("tags", tags)} />
        </div>
      </Sheet>
    </div>
  );
}

/**
 * Effort or impact as a chip, "Effort S", or "Effort ·" while it's not set. It opens a sheet of S M L XL that closes
 * on a pick; picking the size it has again clears it.
 */
function SizeChip({
  label,
  value,
  onChange,
}: {
  label: string;
  value: Size | null;
  onChange: (value: Size | null) => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" aria-haspopup="dialog" onClick={() => setOpen(true)} className={chipClass(value !== null)}>
        {label}{" "}
        {value ? (
          SIZE_LABELS[value]
        ) : (
          <>
            <span aria-hidden="true">·</span>
            <span className="sr-only">not set</span>
          </>
        )}
      </button>
      <Sheet title={label} isOpen={open} onClose={() => setOpen(false)}>
        <ToggleButtonGroup
          aria-label={label}
          selectionMode="single"
          orientation="vertical"
          selectedKeys={value === null ? [] : [value]}
          onSelectionChange={(keys) => {
            const [picked] = [...keys];
            onChange(picked === undefined ? null : (String(picked) as Size));
            setOpen(false);
          }}
          className="flex flex-col pb-1"
        >
          {SIZES.map((size) => (
            <SheetChoice key={size} id={size}>
              {SIZE_LABELS[size]}
            </SheetChoice>
          ))}
        </ToggleButtonGroup>
      </Sheet>
    </>
  );
}

/**
 * The phone's Agent choice as a chip in a row of its own, "Agent: Don't send ▾" (ux.md, Adding an item §6). It opens a
 * sheet of the choices, and the line for the agent once one is picked; Done or a tap outside closes it and keeps them.
 */
function AgentChip({
  draft,
  set,
  line,
}: {
  draft: Draft;
  set: <K extends keyof Draft>(key: K, value: Draft[K]) => void;
  line: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const picked = AGENT_CHOICES.find((choice) => choice.id === draft.agent);
  return (
    <>
      <button
        type="button"
        aria-haspopup="dialog"
        onClick={() => setOpen(true)}
        className={`${chipClass(draft.agent !== "none")} w-fit`}
      >
        Agent: {picked?.sheetLabel}
        <ChevronDown aria-hidden="true" className="size-3.5" />
      </button>
      <Sheet title="Agent" isOpen={open} onClose={() => setOpen(false)}>
        <ToggleButtonGroup
          aria-label="Agent"
          selectionMode="single"
          disallowEmptySelection
          orientation="vertical"
          selectedKeys={[draft.agent]}
          onSelectionChange={(keys) => {
            const choice = AGENT_CHOICES.find((c) => c.id === [...keys][0]);
            if (choice) set("agent", choice.id);
          }}
          className="flex flex-col pb-1"
        >
          {AGENT_CHOICES.map((choice) => (
            <SheetChoice key={choice.id} id={choice.id}>
              {choice.sheetLabel}
            </SheetChoice>
          ))}
        </ToggleButtonGroup>
        {draft.agent !== "none" && line}
        <button
          type="button"
          onClick={() => setOpen(false)}
          className="mt-2 h-11 rounded-xl bg-ink px-5 text-sm font-semibold text-canvas"
        >
          Done
        </button>
      </Sheet>
    </>
  );
}
