import { useNavigate } from "@tanstack/react-router";
import { ChevronLeft, Search, X } from "lucide-react";
import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import {
  Autocomplete,
  Button,
  Dialog,
  Input,
  Label,
  Menu,
  MenuItem,
  MenuSection,
  Modal,
  ModalOverlay,
  SearchField,
} from "react-aria-components";

import { type HeapRef } from "../api/client";
import type { Item, Me, SearchRow } from "../api/schema";
import { useItem, useUpdateItem } from "../api/queries/items";
import { useSearch } from "../api/queries/me";
import { useChangeState } from "../pages/HeapPage";
import { takesBack } from "./agentConfirm";
import { shortLink, useCopyText } from "./copyLink";
import { DuePicker } from "./DuePicker";
import { SectionTitle, rowClass } from "./HeapSwitcher";
import { Kbd, WIDE_HINT } from "./Kbd";
import { KindIcon } from "./kindIcons";
import { STATE_LABELS, isClosed } from "./labels";
import { ProblemText } from "./ProblemText";
import { teamName } from "./teamName";
import { useToast } from "./UndoToast";

type Team = Me["teams"][number];
type Heap = Team["heaps"][number];

/** How long after the last keystroke the search is sent. */
const DEBOUNCE_MS = 150;

const sectionClass = "flex flex-col not-first:mt-3";

/**
 * The palette ⌘K opens (design/ux.md, Keyboard): finds an item by title or key in every heap, goes to a heap, adds the
 * text as a new item in the heap on screen, and with Tab shows the highlighted item's actions. It is a modal at the top
 * of the screen at every width; on a phone it fills the width and ends above the on-screen keyboard. The shell keeps it
 * open as a history entry: going to an item or a heap replaces that entry, which closes it, and `onOpenChange(false)`
 * goes back. `onAdd` is told the heap and title once the palette has closed.
 */
export function FindPalette({
  isOpen,
  onOpenChange,
  me,
  shellHeap,
  onAdd,
}: {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  me: Me;
  shellHeap: HeapRef | undefined;
  onAdd: (heap: HeapRef, title: string) => void;
}) {
  return (
    <ModalOverlay
      isOpen={isOpen}
      onOpenChange={onOpenChange}
      isDismissable
      className="fixed inset-x-0 top-0 bottom-(--keyboard-inset) z-50 flex items-start justify-center bg-black/40 px-2 pt-[max(env(safe-area-inset-top),0.5rem)] pb-2 sm:bottom-0 sm:px-4 sm:pt-[10vh] sm:pb-0"
    >
      <Modal className="flex max-h-full w-full sm:max-h-[80dvh] max-w-[560px] flex-col overflow-hidden rounded-2xl bg-surface text-ink shadow-xl outline-none">
        <Dialog aria-label="Find" className="flex min-h-0 flex-1 flex-col outline-none">
          <FindBody me={me} shellHeap={shellHeap} close={() => onOpenChange(false)} onAdd={onAdd} />
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}

/** The palette's content, mounted while it is open, so each opening starts with an empty field. */
function FindBody({
  me,
  shellHeap,
  close,
  onAdd,
}: {
  me: Me;
  shellHeap: HeapRef | undefined;
  close: () => void;
  onAdd: (heap: HeapRef, title: string) => void;
}) {
  const navigate = useNavigate();
  const [text, setText] = useState("");
  // The text the search is sent for, once typing has stopped.
  const [settled, setSettled] = useState("");
  // The item whose actions show in place of the results; the text is kept for the way back.
  const [acting, setActing] = useState<SearchRow | null>(null);
  const query = text.trim();
  const searching = query !== "";
  useEffect(() => {
    const timer = window.setTimeout(() => setSettled(query), DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [query]);
  const search = useSearch(searching ? settled : "");
  // The results are the answer to what is typed now, not the previous text's rows that stay while the next load. Until
  // then Enter and Tab do nothing, so they never act on a row the person didn't see for this text.
  const asked = searching && settled === query;
  const current = asked && !search.isPlaceholderData && search.data !== undefined;
  const error = asked ? search.error : null;
  const pending = searching && !current && !error;

  const several = me.teams.length > 1;
  const rows = current ? search.data.rows : [];
  const heaps = me.teams.flatMap((team) =>
    team.heaps
      .filter((heap) => !heap.archived && (!searching || heap.name.toLowerCase().includes(query.toLowerCase())))
      .map((heap) => ({ team, heap })),
  );
  const addHeap = shellHeap && heapOf(me, shellHeap);

  // In place of the palette's entry, so Back from there goes to where Find was opened.
  const goToHeap = (team: string, heap: string) =>
    void navigate({ to: "/teams/$team/heaps/$heap", params: { team, heap }, replace: true });
  const goToItem = (team: string, heap: string, item: string) =>
    void navigate({ to: "/teams/$team/heaps/$heap/items/$item", params: { team, heap, item }, replace: true });

  // Before the field's own handlers (the menu's Enter among them), which would act on the highlighted row.
  const holdKeys = (event: KeyboardEvent<HTMLInputElement>) => {
    if (pending && (event.key === "Enter" || event.key === "Tab")) {
      event.preventDefault();
      event.stopPropagation();
    }
  };

  // The row the field's highlight is on, as the input names it in `aria-activedescendant`.
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    // The field would clear its text first; here Escape always closes.
    if (event.key === "Escape") close();
    if (event.key !== "Tab" || event.shiftKey) return;
    event.preventDefault();
    const id = event.currentTarget.getAttribute("aria-activedescendant");
    const at = id ? document.getElementById(id)?.dataset.item : undefined;
    const row = at === undefined ? undefined : rows[Number(at)];
    if (row) setActing(row);
  };

  if (acting) return <Actions row={acting} me={me} close={close} onBack={() => setActing(null)} />;

  return (
    <>
      <Autocomplete inputValue={text} onInputChange={setText}>
        <SearchField autoFocus className="relative flex shrink-0 items-center border-b border-line p-2.5">
          <Label className="sr-only">Find</Label>
          <Search aria-hidden="true" className="pointer-events-none absolute left-5 size-4 text-muted" />
          <Input
            placeholder="Find an item, or go to a heap"
            onKeyDownCapture={holdKeys}
            onKeyDown={onKeyDown}
            className="h-10 w-full rounded-[9px] border-2 border-transparent bg-sunken pr-10 pl-9 text-base outline-none placeholder:text-muted focus:border-accent sm:text-sm [&::-webkit-search-cancel-button]:hidden"
          />
          {/* A phone has no Escape, so there ✕ stays and closes the palette once the field is empty. */}
          <Button
            aria-label={text === "" ? "Close" : "Clear"}
            onPress={text === "" ? close : undefined}
            className={`${text === "" ? "sm:hidden" : ""} absolute right-2.5 flex size-10 items-center justify-center rounded-xl text-ink-soft outline-none hover:text-ink focus-visible:outline-2 focus-visible:outline-accent`}
          >
            <X aria-hidden="true" className="size-4" />
          </Button>
        </SearchField>
        <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-1.5 pt-1 pb-2">
          {pending && <p className="px-2.5 py-2 text-sm text-muted">Searching…</p>}
          {error && (
            <div className="px-2.5 py-1">
              <ProblemText error={error} />
            </div>
          )}
          {/* New for each answer, so the highlight starts on the first row again, as it does when typing. */}
          <Menu
            key={search.dataUpdatedAt}
            autoFocus="first"
            aria-label="Results"
            className="outline-none"
            renderEmptyState={() =>
              current ? <p className="px-2.5 py-2 text-sm text-muted">Nothing matches “{query}”.</p> : null
            }
          >
            {rows.length > 0 && (
              <MenuSection aria-label="Items" className={sectionClass}>
                <SectionTitle>Items</SectionTitle>
                {rows.map((row, at) => (
                  <ItemResult
                    key={row.item.id}
                    row={row}
                    at={at}
                    me={me}
                    onAction={() => goToItem(row.heap.team_slug, row.heap.slug, row.item.key)}
                  />
                ))}
              </MenuSection>
            )}
            {heaps.length > 0 && (
              <MenuSection aria-label="Go to" className={sectionClass}>
                <SectionTitle>Go to</SectionTitle>
                {heaps.map(({ team, heap }) => (
                  <MenuItem
                    key={heap.id}
                    id={`heap:${heap.id}`}
                    textValue={heap.name}
                    onAction={() => goToHeap(team.slug, heap.slug)}
                    className={rowClass}
                  >
                    <span className="min-w-0 flex-1 truncate">
                      {heap.name}
                      {several && <span className="text-muted"> · {teamName(team, me)}</span>}
                    </span>
                  </MenuItem>
                ))}
              </MenuSection>
            )}
            {searching && shellHeap && addHeap && !addHeap.heap.archived && (
              <MenuSection aria-label="Add" className={sectionClass}>
                <MenuItem
                  id="add"
                  textValue={`Add “${query}” as a new item`}
                  onAction={() => {
                    close();
                    onAdd(shellHeap, query);
                  }}
                  className={rowClass}
                >
                  <span className="min-w-0 flex-1 truncate">
                    Add “{query}” as a new item <span className="text-muted">in {addHeap.heap.name}</span>
                  </span>
                </MenuItem>
              </MenuSection>
            )}
          </Menu>
          {current && search.data.truncated && (
            <p className="px-2.5 py-2 text-[13px] text-muted">More items match. Keep typing to narrow them.</p>
          )}
        </div>
      </Autocomplete>
      <p
        className={`${WIDE_HINT} shrink-0 border-t border-line bg-canvas px-3 py-2 text-[11px] text-muted`}
        aria-hidden="true"
      >
        ↑↓ move · ↵ open · ⇥ actions · esc
      </p>
    </>
  );
}

/** The team and heap a heap reference names, in what `me` lists. */
function heapOf(me: Me, ref: HeapRef): { team: Team; heap: Heap } | undefined {
  const team = me.teams.find((t) => t.slug === ref.team);
  const heap = team?.heaps.find((h) => h.slug === ref.heap);
  return team && heap && { team, heap };
}

/** "heap · team", the team only for someone in several. */
function placeLabel(me: Me, row: SearchRow): string {
  const team = me.teams.find((t) => t.slug === row.heap.team_slug);
  return me.teams.length > 1 && team ? `${row.heap.name} · ${teamName(team, me)}` : row.heap.name;
}

/** One found item: its kind, title and key, where it is, and its state when it isn't open. `at` is its place in the results. */
function ItemResult({ row, at, me, onAction }: { row: SearchRow; at: number; me: Me; onAction: () => void }) {
  const { item } = row;
  return (
    <MenuItem
      id={`item:${item.id}`}
      textValue={`${item.title} ${item.key}`}
      data-item={at}
      onAction={onAction}
      className={`${rowClass} sm:gap-2`}
    >
      <KindIcon kind={item.kind} />
      <span className="min-w-0 flex-1 truncate">{item.title}</span>
      <span className="shrink-0 font-mono text-xs text-muted">{item.key}</span>
      <span className="hidden max-w-[40%] shrink-0 truncate text-[13px] text-muted sm:inline">
        {placeLabel(me, row)}
        {item.state !== "open" && ` · ${STATE_LABELS[item.state]}`}
      </span>
    </MenuItem>
  );
}

/**
 * What can be done to the item the results had highlighted, from the server's `allowed_actions`. Shift+Tab goes back a
 * level: from Due to the actions, from the actions to the results.
 */
function Actions({ row, me, close, onBack }: { row: SearchRow; me: Me; close: () => void; onBack: () => void }) {
  const heapRef: HeapRef = { team: row.heap.team_slug, heap: row.heap.slug };
  const { data: item, error } = useItem(heapRef, row.item.key);
  const [due, setDue] = useState(false);
  const panel = useRef<HTMLDivElement>(null);
  // Focus stays in the panel while the item loads, so Shift+Tab has somewhere to be heard.
  useEffect(() => panel.current?.focus(), []);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "Tab" || !event.shiftKey) return;
    event.preventDefault();
    if (due) setDue(false);
    else onBack();
  };

  return (
    <div ref={panel} tabIndex={-1} onKeyDown={onKeyDown} className="flex min-h-0 flex-1 flex-col outline-none">
      <div className="flex shrink-0 flex-col border-b border-line px-4 py-3">
        <span className="flex items-center gap-1.5 text-sm text-muted">
          <ChevronLeft aria-hidden="true" className="size-4" />
          <Kbd className={WIDE_HINT}>⇧⇥</Kbd>
        </span>
        <h2 className="truncate text-base font-semibold">{row.item.title}</h2>
        <p className="truncate text-[13px] text-muted">{placeLabel(me, row)}</p>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-1.5 py-2">
        {error ? (
          <ProblemText error={error} />
        ) : !item ? (
          <p className="px-2.5 py-2 text-sm text-muted">Loading…</p>
        ) : due ? (
          <DuePanel item={item} heapRef={heapRef} close={close} />
        ) : (
          <ActionMenu item={item} heapRef={heapRef} close={close} onDue={() => setDue(true)} />
        )}
      </div>
    </div>
  );
}

function ActionMenu({
  item,
  heapRef,
  close,
  onDue,
}: {
  item: Item;
  heapRef: HeapRef;
  close: () => void;
  onDue: () => void;
}) {
  const { run } = useChangeState(heapRef, undefined);
  const showToast = useToast();
  const { copy } = useCopyText();
  const allowed = (action: Item["allowed_actions"][number]) => item.allowed_actions.includes(action);
  const closed = isClosed(item.state);
  const canChange = allowed("change_state");

  return (
    <Menu aria-label="Actions" autoFocus="first" className="outline-none">
      {canChange && (item.state === "open" || item.state === "blocked") && (
        <MenuItem
          id="start"
          onAction={() => {
            close();
            run(item, "in_progress");
          }}
          className={rowClass}
        >
          {takesBack(item, "in_progress") ? "Take back and start" : "Start"}
        </MenuItem>
      )}
      {canChange && !closed && (
        <MenuItem
          id="done"
          onAction={() => {
            close();
            run(item, "done");
          }}
          className={rowClass}
        >
          {takesBack(item, "done") ? "Take back and mark done" : "Mark done"}
        </MenuItem>
      )}
      {allowed("edit") && !closed && (
        <MenuItem id="due" onAction={onDue} className={rowClass}>
          Due…
        </MenuItem>
      )}
      <MenuItem
        id="copy"
        onAction={() => {
          void copy(shortLink(heapRef.team, heapRef.heap, item.key)).then((copied) => {
            if (copied) {
              close();
              showToast({ message: "Link copied." });
            } else {
              showToast({ message: "Couldn't copy the link.", error: true });
            }
          });
        }}
        className={rowClass}
      >
        Copy link
      </MenuItem>
    </Menu>
  );
}

/** The item's due time, as the item pane's field offers it; a change saves and closes the palette. */
function DuePanel({ item, heapRef, close }: { item: Item; heapRef: HeapRef; close: () => void }) {
  const update = useUpdateItem(heapRef);
  const showToast = useToast();
  return (
    <div className="flex flex-col gap-2 px-2.5">
      <DuePicker
        value={item.due ? { date: item.due.date, firm: item.due.firm } : null}
        disabled={update.isPending}
        onChange={(value) => {
          if (value?.date === item.due?.date && value?.firm === item.due?.firm) return;
          update.mutateAsync({ item, patch: { due: value } }).then(
            () => {
              close();
              showToast({
                message: value ? `Due date saved for “${item.title}”.` : `Due date cleared for “${item.title}”.`,
              });
            },
            () => undefined,
          );
        }}
      />
      <ProblemText error={update.error} />
    </div>
  );
}
