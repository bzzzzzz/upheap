import { useNavigate } from "@tanstack/react-router";
import { Archive, Check, ChevronRight, Plus, Search, X } from "lucide-react";
import { useCallback, useRef, useState, type FormEvent, type ReactNode, type RefObject } from "react";
import {
  Autocomplete,
  Button,
  Header,
  Input,
  Label,
  Menu,
  MenuItem,
  MenuSection,
  SearchField,
} from "react-aria-components";

import type { HeapRef, Me } from "../api/client";
import { useCreateHeap, useTeam } from "../api/queries";
import { readFoldedTeams, readRecentHeaps, writeFoldedTeams } from "./heapSwitcherStorage";
import { PHONE, TOUCH, useMedia } from "./media";
import { PopoverSheet } from "./PopoverSheet";
import { ProblemText } from "./ProblemText";
import { sheetRowClass } from "./Sheet";
import { teamName } from "./teamName";

type SwitcherTeam = Me["teams"][number];
type Entry = { team: SwitcherTeam; heap: SwitcherTeam["heaps"][number] };

/** From this many unarchived heaps the switcher gets a search field, Recent and folding teams (decisions.md, 32). */
const LONG_LIST = 8;
/** How many recent heaps show. */
const RECENT_SHOWN = 4;

/** A heap row: 52 px on a phone, 36 px where the pointer is fine on a tablet or wide screen, 44 px where it's coarse. */
const rowClass = `${sheetRowClass} sm:min-h-9 sm:pointer-coarse:min-h-11 data-[focused]:bg-sunken`;
const currentRowClass = "bg-accent-soft font-semibold";
/** Recent on a phone: chips in a row that scrolls sideways. */
const chipClass =
  "max-sm:min-h-11 max-sm:w-auto max-sm:shrink-0 max-sm:rounded-full max-sm:border max-sm:border-line max-sm:bg-surface max-sm:px-4";

/**
 * Every heap, by team, with a check on the one on screen (ux.md, Phone details): a popover beside the rail on a
 * tablet or under the sidebar's heap button on a wide screen, a sheet on a phone. With many heaps it adds search,
 * Recent and folding teams. It ends with a new heap in the heap's own team, where the user may create one.
 */
export function HeapSwitcher({
  me,
  heapRef,
  placement = "bottom end",
  children,
}: {
  me: Me;
  heapRef: HeapRef | undefined;
  placement?: "bottom end" | "bottom start" | "end top";
  children: (props: {
    ref: RefObject<HTMLButtonElement | null>;
    onClick: () => void;
    "aria-expanded": boolean;
    "aria-haspopup": "dialog";
  }) => ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const close = () => setOpen(false);

  return (
    <>
      {children({ ref: trigger, onClick: () => setOpen(true), "aria-expanded": open, "aria-haspopup": "dialog" })}
      <PopoverSheet
        title="Heaps"
        triggerRef={trigger}
        isOpen={open}
        onClose={close}
        placement={placement}
        size="narrow"
        scrollBody
      >
        <SwitcherPanel me={me} heapRef={heapRef} close={close} />
      </PopoverSheet>
    </>
  );
}

/** The panel mounts when it opens, so what it reads from storage and its own state start fresh each time. */
function SwitcherPanel({ me, heapRef, close }: { me: Me; heapRef: HeapRef | undefined; close: () => void }) {
  const team = me.teams.find((t) => t.slug === heapRef?.team);
  return team ? (
    <PanelWithCreate me={me} heapRef={heapRef} close={close} team={team} />
  ) : (
    <SwitcherBody me={me} heapRef={heapRef} close={close} />
  );
}

function PanelWithCreate({
  team,
  ...panel
}: {
  me: Me;
  heapRef: HeapRef | undefined;
  close: () => void;
  team: SwitcherTeam;
}) {
  const { data } = useTeam(team.slug);
  return <SwitcherBody {...panel} createIn={data?.allowed_actions.includes("create_heap") ? team : undefined} />;
}

function SwitcherBody({
  me,
  heapRef,
  close,
  createIn,
}: {
  me: Me;
  heapRef: HeapRef | undefined;
  close: () => void;
  /** The heap's team, when the user may create heaps in it. */
  createIn?: SwitcherTeam;
}) {
  const phone = useMedia(PHONE);
  const coarse = useMedia(TOUCH);
  const entries: Entry[] = me.teams.flatMap((team) => team.heaps.map((heap) => ({ team, heap })));
  const current = entries.find((e) => e.team.slug === heapRef?.team && e.heap.slug === heapRef.heap);
  const long = entries.filter((e) => !e.heap.archived).length >= LONG_LIST;
  const several = me.teams.length > 1;

  const [text, setText] = useState("");
  const [folded, setFolded] = useState(readFoldedTeams);
  const [recentIds] = useState(readRecentHeaps);
  // Team ids whose archived heaps show; an archived heap on screen has its team's revealed, so its check shows.
  const [revealed, setRevealed] = useState<ReadonlySet<string>>(
    () => new Set(current?.heap.archived ? [current.team.id] : []),
  );
  // The new heap's name while its form is open.
  const [draft, setDraft] = useState<string | null>(null);

  const query = long ? text.trim() : "";
  const searching = query !== "";
  const toggleFold = (id: string) => {
    const next = new Set(folded);
    if (!next.delete(id)) next.add(id);
    setFolded(next);
    writeFoldedTeams(next);
  };
  const teamLabel = (entry: Entry) => (several ? teamName(entry.team, me) : undefined);
  const isCurrent = (entry: Entry) => entry.heap.id === current?.heap.id;

  const recent = recentIds
    .map((id) => entries.find((e) => e.heap.id === id))
    .filter((e): e is Entry => e !== undefined && !isCurrent(e))
    .slice(0, RECENT_SHOWN);
  const matches = searching ? entries.filter((e) => e.heap.name.toLowerCase().includes(query.toLowerCase())) : [];
  const results = [...matches.filter((e) => !e.heap.archived), ...matches.filter((e) => e.heap.archived)];

  const createRow = createIn && searching && (
    <MenuItem
      id="create"
      textValue={`New heap “${query}” in ${teamName(createIn, me)}`}
      onAction={() => setDraft(query)}
      className={rowClass}
    >
      <Plus aria-hidden="true" className="size-4 shrink-0 text-ink-soft" />
      <span className="min-w-0 flex-1 truncate">
        New heap “{query}” in {teamName(createIn, me)}
      </span>
    </MenuItem>
  );

  const teamSections = me.teams.map((team) => {
    const unarchived = team.heaps.filter((heap) => !heap.archived);
    const archived = team.heaps.filter((heap) => heap.archived);
    const canFold = long && several;
    const isFolded = canFold && folded.has(team.id);
    const showArchived = revealed.has(team.id);
    const label = teamName(team, me);
    const heapItem = (heap: Entry["heap"]) => (
      <HeapItem
        key={heap.id}
        id={`heap:${heap.id}`}
        entry={{ team, heap }}
        current={heap.id === current?.heap.id}
        onOpen={close}
      />
    );
    return (
      <MenuSection key={team.id} aria-label={label} className="flex flex-col">
        {canFold ? (
          <MenuItem
            id={`fold:${team.id}`}
            textValue={label}
            aria-expanded={!isFolded}
            onAction={() => toggleFold(team.id)}
            className={`${rowClass} text-xs font-semibold tracking-wide text-muted uppercase sm:min-h-8`}
          >
            <ChevronRight
              aria-hidden="true"
              className={`size-4 shrink-0 motion-safe:transition-transform ${isFolded ? "" : "rotate-90"}`}
            />
            <span className="min-w-0 flex-1 truncate">{label}</span>
            <span className="shrink-0 font-normal">{unarchived.length}</span>
          </MenuItem>
        ) : (
          several && <SectionTitle count={unarchived.length}>{label}</SectionTitle>
        )}
        {!isFolded && unarchived.map(heapItem)}
        {!isFolded && showArchived && archived.map(heapItem)}
        {!isFolded && archived.length > 0 && (
          <MenuItem
            id={`archived:${team.id}`}
            textValue={showArchived ? "Hide archived" : `${archived.length} archived`}
            aria-expanded={showArchived}
            onAction={() => {
              const next = new Set(revealed);
              if (!next.delete(team.id)) next.add(team.id);
              setRevealed(next);
            }}
            className={`${rowClass} text-muted`}
          >
            <Archive aria-hidden="true" className="size-4 shrink-0" />
            <span className="min-w-0 flex-1 truncate">
              {showArchived ? "Hide archived" : `${archived.length} archived`}
            </span>
          </MenuItem>
        )}
      </MenuSection>
    );
  });

  const menu = (
    <Menu aria-label="Heaps" autoFocus={!long && !phone && !coarse} className="outline-none">
      {searching ? (
        <>
          {results.map((entry) => (
            <HeapItem
              key={entry.heap.id}
              id={`heap:${entry.heap.id}`}
              entry={entry}
              current={isCurrent(entry)}
              teamLabel={teamLabel(entry)}
              query={query}
              onOpen={close}
            />
          ))}
          {createRow}
        </>
      ) : (
        <>
          {long && recent.length > 0 && (
            <MenuSection
              aria-label="Recent"
              className="flex flex-col max-sm:flex-row max-sm:items-center max-sm:gap-2 max-sm:overflow-x-auto max-sm:pb-1"
            >
              <SectionTitle>Recent</SectionTitle>

              {recent.map((entry) => (
                <HeapItem
                  key={entry.heap.id}
                  id={`recent:${entry.heap.id}`}
                  entry={entry}
                  teamLabel={teamLabel(entry)}
                  extraClass={chipClass}
                  onOpen={close}
                />
              ))}
            </MenuSection>
          )}
          {teamSections}
        </>
      )}
    </Menu>
  );

  const noMatch = `No heap matches “${query}”`;
  const searched = `Searched all ${entries.length} ${entries.length === 1 ? "heap" : "heaps"}${
    several ? ` in ${me.teams.length} teams` : ""
  }, archived ones too.`;
  const scrolling = (
    <div className="-mx-1 flex min-h-0 flex-1 flex-col overflow-y-auto px-1">
      {/* Mounted for as long as the panel is, so a screen reader announces the text when it changes. */}
      {long && (
        <p
          role="status"
          className={searching && results.length > 0 ? "px-3 pt-1 pb-1 text-[13px] text-muted" : "sr-only"}
        >
          {!searching ? "" : results.length > 0 ? resultsSummary(results, me) : `${noMatch}. ${searched}`}
        </p>
      )}
      {searching && results.length === 0 && (
        <div aria-hidden="true" className="flex flex-col items-center gap-1 px-4 py-6 text-center">
          <p className="font-semibold break-words">{noMatch}</p>
          <p className="text-[13px] text-muted">{searched}</p>
        </div>
      )}
      {menu}
    </div>
  );

  const hint = !phone && !coarse;
  const footer = (createIn || hint) && (
    <div className="shrink-0 border-t border-line pt-3">
      {createIn && draft !== null ? (
        <NewHeapForm me={me} team={createIn} initialName={draft} onCancel={() => setDraft(null)} onCreated={close} />
      ) : (
        <div className="flex items-center gap-3">
          {createIn && <NewHeapButton me={me} team={createIn} onClick={() => setDraft("")} />}
          {hint && <p className="ml-auto shrink-0 text-xs text-muted">↑↓ move · ↵ open · esc</p>}
        </div>
      )}
    </div>
  );

  return (
    <>
      {long ? (
        <Autocomplete inputValue={text} onInputChange={setText}>
          <SearchField autoFocus={!phone && !coarse} className="relative flex shrink-0 items-center">
            <Label className="sr-only">Find a heap</Label>
            <Search aria-hidden="true" className="pointer-events-none absolute left-3 size-4 text-muted" />
            <Input
              placeholder="Find a heap"
              className="h-12 w-full rounded-xl border border-line bg-surface pr-11 pl-9 text-base outline-none placeholder:text-muted focus:border-accent sm:h-10 sm:pr-9 sm:text-sm [&::-webkit-search-cancel-button]:hidden"
            />
            {text !== "" && (
              <Button
                aria-label="Clear"
                className="absolute right-0 flex size-12 items-center justify-center rounded-xl text-ink-soft outline-none hover:text-ink focus-visible:outline-2 focus-visible:outline-accent sm:size-10"
              >
                <X aria-hidden="true" className="size-4" />
              </Button>
            )}
          </SearchField>
          {scrolling}
        </Autocomplete>
      ) : (
        scrolling
      )}
      {footer}
    </>
  );
}

function SectionTitle({ children, count }: { children: ReactNode; count?: number }) {
  return (
    <Header className="max-sm:shrink-0 flex items-center px-3 pt-2.5 pb-0.5 text-xs font-semibold tracking-wide text-muted uppercase">
      <span className="min-w-0 flex-1 truncate">{children}</span>
      {count !== undefined && <span className="shrink-0 font-normal">{count}</span>}
    </Header>
  );
}

/** "3 heaps in 2 teams", or "3 heaps" for someone in one team. */
function resultsSummary(results: Entry[], me: Me): string {
  const heaps = `${results.length} ${results.length === 1 ? "heap" : "heaps"}`;
  if (me.teams.length < 2) return heaps;
  const teams = new Set(results.map((e) => e.team.id)).size;
  return `${heaps} in ${teams} ${teams === 1 ? "team" : "teams"}`;
}

/** The part of `text` that matches `query`, marked. */
function Highlighted({ text, query }: { text: string; query: string }) {
  const lower = text.toLowerCase();
  const at = lower.length === text.length ? lower.indexOf(query.toLowerCase()) : -1;
  if (query === "" || at < 0) return text;
  return (
    <>
      {text.slice(0, at)}
      <mark className="rounded-sm bg-accent/25 text-ink">{text.slice(at, at + query.length)}</mark>
      {text.slice(at + query.length)}
    </>
  );
}

/**
 * One heap. In the team's list the name has "(archived)" after it; among search results (with `query`) the match is
 * marked and archived heaps carry a tag. `teamLabel` shows on the right when the user has several teams.
 */
function HeapItem({
  id,
  entry,
  current = false,
  teamLabel,
  query,
  extraClass = "",
  onOpen,
}: {
  id: string;
  entry: Entry;
  current?: boolean;
  teamLabel?: string;
  query?: string;
  extraClass?: string;
  onOpen: () => void;
}) {
  const { heap } = entry;
  return (
    <MenuItem
      id={id}
      textValue={heap.name}
      href={`/teams/${encodeURIComponent(entry.team.slug)}/heaps/${encodeURIComponent(heap.slug)}`}
      onAction={onOpen}
      className={`${rowClass} ${current ? currentRowClass : ""} ${extraClass}`}
    >
      <span className="min-w-0 flex-1 truncate">
        {query === undefined ? heap.name : <Highlighted text={heap.name} query={query} />}
        {heap.archived &&
          (query === undefined ? (
            <span className="font-normal text-muted"> (archived)</span>
          ) : (
            <span className="ml-2 rounded-full border border-line px-1.5 py-px text-[11px] font-normal text-muted">
              archived
            </span>
          ))}
        {/* MenuItem has no aria-current or aria-checked here, so a screen reader hears it from the name. */}
        {current && <span className="sr-only">, current</span>}
      </span>
      {teamLabel && <span className="shrink-0 text-[13px] font-normal text-muted">{teamLabel}</span>}
      <span aria-hidden="true" className="w-5 shrink-0 text-accent-ink">
        {current && <Check className="size-4" strokeWidth={3} />}
      </span>
    </MenuItem>
  );
}

const newHeapButtonClass =
  "flex min-h-12 flex-1 items-center justify-center gap-1.5 rounded-xl border border-line bg-surface px-3 text-[15px] hover:border-muted disabled:opacity-50";

/** "New heap in <team>", for those the team lets create heaps. */
function NewHeapButton({ me, team, onClick }: { me: Me; team: SwitcherTeam; onClick: () => void }) {
  return (
    <button type="button" onClick={onClick} className={`${newHeapButtonClass} min-w-0`}>
      <Plus aria-hidden="true" className="size-4 shrink-0" />
      <span className="truncate">New heap in {teamName(team, me)}</span>
    </button>
  );
}

/** The name field that "New heap" becomes, with `initialName` filled in and focused, the caret at its end. */
function NewHeapForm({
  me,
  team,
  initialName,
  onCancel,
  onCreated,
}: {
  me: Me;
  team: SwitcherTeam;
  initialName: string;
  onCancel: () => void;
  onCreated: () => void;
}) {
  const create = useCreateHeap(team.slug);
  const navigate = useNavigate();
  const [name, setName] = useState(initialName);
  const focusAtEnd = useCallback((input: HTMLInputElement | null) => {
    if (!input) return;
    input.focus();
    input.setSelectionRange(input.value.length, input.value.length);
  }, []);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (name.trim() === "") return;
    create.mutate(name.trim(), {
      onSuccess: (heap) => {
        onCreated();
        void navigate({ to: "/teams/$team/heaps/$heap", params: { team: team.slug, heap: heap.slug } });
      },
    });
  };

  return (
    <form onSubmit={submit} className="flex flex-col gap-2">
      <input
        ref={focusAtEnd}
        aria-label={`Name of the new heap in ${teamName(team, me)}`}
        value={name}
        onChange={(e) => setName(e.target.value)}
        maxLength={100}
        placeholder="Heap name"
        className="h-12 rounded-xl border border-line bg-surface px-3 text-base"
      />
      <div className="flex gap-2">
        <button type="button" onClick={onCancel} className={newHeapButtonClass}>
          Cancel
        </button>
        <button
          type="submit"
          disabled={create.isPending || name.trim() === ""}
          className={`${newHeapButtonClass} border-ink bg-ink font-medium text-canvas`}
        >
          Add heap
        </button>
      </div>
      <ProblemText error={create.error} />
    </form>
  );
}
