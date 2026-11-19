import { useMatch, useNavigate, useSearch } from "@tanstack/react-router";
import { Archive, Check, ChevronRight, Clock, Plus, Search, X } from "lucide-react";
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
  Separator,
} from "react-aria-components";

import type { HeapRef } from "../api/client";
import type { Me } from "../api/schema";
import { useCreateHeap } from "../api/queries/heaps";
import { useTeam } from "../api/queries/teams";
import { readFoldedTeams, readRecentHeaps, writeFoldedTeams } from "./heapSwitcherStorage";
import { PHONE, TOUCH, useMedia } from "./media";
import { PopoverSheet } from "./PopoverSheet";
import { ProblemText } from "./ProblemText";
import { sheetRowClass } from "./Sheet";
import { teamName } from "./teamName";

type SwitcherTeam = Me["teams"][number];
type Entry = { team: SwitcherTeam; heap: SwitcherTeam["heaps"][number] };

/**
 * How many open items the person's heaps hold in all, archived ones left out: what the list across heaps counts. With
 * `team`, only that team's heaps count.
 */
export const openItemCount = (me: Me, team?: string) =>
  me.teams
    .filter((t) => team === undefined || t.slug === team)
    .reduce(
      (total, team) => total + team.heaps.reduce((sum, heap) => sum + (heap.archived ? 0 : heap.open_count), 0),
      0,
    );

/** From this many unarchived heaps it gets a search field, Recent and folding teams (design/ux.md, Phone details). */
export const LONG_LIST = 8;
/** How many recent heaps show. */
const RECENT_SHOWN = 4;

/**
 * A heap row: 52 px on a phone, 36 px where the pointer is fine on a tablet or wide screen, 44 px where it's coarse.
 * In the popover the rows are denser than a sheet's: smaller text, tighter corners.
 */
export const rowClass = `${sheetRowClass} sm:min-h-9 sm:gap-2.5 sm:rounded-lg sm:px-2.5 sm:text-sm sm:pointer-coarse:min-h-11 data-[focused]:bg-sunken`;
/** Under a team's fold header in the popover, rows start where the header's name does, past its chevron. */
const indentClass = "sm:pl-7";
/** A key's name beside what it does. */
const kbdClass =
  "shrink-0 rounded-[5px] border border-line-strong bg-surface px-1.5 text-[11px] font-normal text-ink-soft";
export const currentRowClass = "bg-accent-soft font-semibold";
/** Recent on a phone: chips in a row that scrolls sideways. */
const chipClass =
  "max-sm:min-h-11 max-sm:w-auto max-sm:shrink-0 max-sm:rounded-full max-sm:border max-sm:border-line max-sm:bg-surface max-sm:px-4";

/**
 * Every heap, by team, with a check on the one on screen (ux.md, Phone details): a popover beside the rail on a
 * tablet or under the sidebar's heap button on a wide screen, a sheet on a phone. With many heaps it adds search,
 * Recent and folding teams. On a phone it starts with All heaps, the list across them, which a wider screen reaches
 * from the sidebar. It ends with a new heap in the heap's own team, where the user may create one.
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
        bare
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
  const onAll = useMatch({ from: "/app/all", shouldThrow: false }) !== undefined;
  const allTeam = useSearch({ from: "/app/all", shouldThrow: false })?.team;
  // On All items the kept heap is only where Add goes; only the All heaps row, or its team's, is the current one.
  const current = onAll
    ? undefined
    : entries.find((e) => e.team.slug === heapRef?.team && e.heap.slug === heapRef.heap);
  const long = entries.filter((e) => !e.heap.archived).length >= LONG_LIST;
  const several = me.teams.length > 1;
  const onAllHeaps = onAll && allTeam === undefined;

  const [text, setText] = useState("");
  const [folded, setFolded] = useState(readFoldedTeams);
  const [recentIds] = useState(readRecentHeaps);
  // Team ids whose archived heaps show; an archived heap on screen has its team's revealed, so its check shows.
  const [revealed, setRevealed] = useState<ReadonlySet<string>>(
    () => new Set(current?.heap.archived ? [current.team.id] : []),
  );
  // The new heap's name while its form is open.
  const [draft, setDraft] = useState<string | null>(null);

  // Key hints are for a keyboard: not on a phone, nor where the pointer is a finger.
  const hint = !phone && !coarse;
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
      <Plus aria-hidden="true" className="size-4 shrink-0 text-accent-ink" strokeWidth={2.5} />
      <span className="min-w-0 flex-1 truncate">
        New heap “{query}” <span className="text-muted">in {teamName(createIn, me)}</span>
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
    const indent = canFold ? indentClass : "";
    const heapItem = (heap: Entry["heap"]) => (
      <HeapItem
        key={heap.id}
        id={`heap:${heap.id}`}
        entry={{ team, heap }}
        current={heap.id === current?.heap.id}
        extraClass={indent}
        onOpen={close}
      />
    );
    return (
      <MenuSection key={team.id} aria-label={label} className={sectionClass}>
        {canFold ? (
          <MenuItem
            id={`fold:${team.id}`}
            textValue={label}
            aria-expanded={!isFolded}
            onAction={() => toggleFold(team.id)}
            className={`${rowClass} sticky top-0 z-10 mb-1 bg-sunken text-xs font-semibold tracking-wide text-ink-soft uppercase hover:text-ink data-[focused]:text-ink max-sm:min-h-11 sm:min-h-8 sm:gap-1.5 sm:text-[11px]`}
          >
            <ChevronRight
              aria-hidden="true"
              strokeWidth={2.5}
              className={`size-3.5 shrink-0 max-sm:order-last motion-safe:transition-transform ${isFolded ? "" : "rotate-90"}`}
            />
            <span className="min-w-0 flex-1 truncate">{label}</span>
            <span className="shrink-0 font-normal">{unarchived.length}</span>
          </MenuItem>
        ) : (
          several && <SectionTitle count={unarchived.length}>{label}</SectionTitle>
        )}
        {!isFolded && several && (
          <MenuItem
            id={`all:${team.id}`}
            textValue={`All ${label} items`}
            href={{ to: "/all", search: { team: team.slug } }}
            onAction={close}
            className={`${rowClass} ${indent} ${onAll && allTeam === team.slug ? currentRowClass : ""}`}
          >
            <span className="min-w-0 flex-1 truncate">
              All {label} items
              {onAll && allTeam === team.slug && <span className="sr-only">, current</span>}
            </span>
            <span className="shrink-0 text-[13px] font-normal text-muted tabular-nums">
              {openItemCount(me, team.slug)}
            </span>
            <span aria-hidden="true" className="w-5 shrink-0 text-accent-ink">
              {onAll && allTeam === team.slug && <Check className="size-4" strokeWidth={3} />}
            </span>
          </MenuItem>
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
            className={`${rowClass} ${indent} text-muted sm:text-[13px]`}
          >
            <Archive aria-hidden="true" className="size-4 shrink-0 sm:size-3.5" />
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
              enterHint={hint}
              onOpen={close}
            />
          ))}
          {createRow && results.length > 0 && <Separator className="mx-1 my-1.5 h-px border-0 bg-line" />}
          {createRow}
        </>
      ) : (
        <>
          {phone && (
            <MenuSection aria-label="All heaps" className={sectionClass}>
              <MenuItem
                id="all"
                textValue="All heaps"
                href={{ to: "/all" }}
                onAction={close}
                className={`${rowClass} ${onAllHeaps ? currentRowClass : ""}`}
              >
                <span className="min-w-0 flex-1 truncate">
                  All heaps
                  {onAllHeaps && <span className="sr-only">, current</span>}
                </span>
                <span className="shrink-0 text-[13px] font-normal text-muted tabular-nums">
                  {openItemCount(me, allTeam)}
                </span>
                <span aria-hidden="true" className="w-5 shrink-0 text-accent-ink">
                  {onAllHeaps && <Check className="size-4" strokeWidth={3} />}
                </span>
              </MenuItem>
            </MenuSection>
          )}
          {long && recent.length > 0 && (
            <MenuSection
              aria-label="Recent"
              className={`${sectionClass} max-sm:flex-row max-sm:items-center max-sm:gap-2 max-sm:overflow-x-auto max-sm:pb-2`}
            >
              <SectionTitle>Recent</SectionTitle>

              {recent.map((entry) => (
                <HeapItem
                  key={entry.heap.id}
                  id={`recent:${entry.heap.id}`}
                  entry={entry}
                  teamLabel={teamLabel(entry)}
                  recent
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
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto max-sm:-mx-1 max-sm:px-1 sm:px-1.5 sm:pt-1 sm:pb-2">
      {/* Mounted for as long as the panel is, so a screen reader announces the text when it changes. */}
      {long && (
        <p
          role="status"
          className={
            searching && results.length > 0
              ? "px-3 pt-1 pb-1 text-[13px] text-muted sm:px-2.5 sm:pt-1.5 sm:text-xs"
              : "sr-only"
          }
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

  const footer = (createIn || hint) && (
    <div className="shrink-0 border-t border-line max-sm:pt-3 sm:bg-canvas sm:px-2.5 sm:py-2">
      {createIn && draft !== null ? (
        <NewHeapForm me={me} team={createIn} initialName={draft} onCancel={() => setDraft(null)} onCreated={close} />
      ) : (
        <div className="flex items-center gap-3">
          {/* While searching with a keyboard, the results' own row makes the heap, and the footer says what matched. */}
          {searching && hint ? (
            <p className="min-w-0 truncate text-[11px] text-muted">Archived heaps match too, listed last</p>
          ) : (
            createIn && <NewHeapButton me={me} team={createIn} onClick={() => setDraft("")} />
          )}
          {hint && <p className="ml-auto shrink-0 text-[11px] text-muted">↑↓ move · ↵ open · esc</p>}
        </div>
      )}
    </div>
  );

  return (
    <>
      {long ? (
        <Autocomplete inputValue={text} onInputChange={setText}>
          <SearchField
            autoFocus={hint}
            className="relative flex shrink-0 items-center sm:border-b sm:border-line sm:p-2.5"
          >
            <Label className="sr-only">Find a heap</Label>
            <Search
              aria-hidden="true"
              className="pointer-events-none absolute left-3.5 size-4 text-muted sm:left-5.5"
            />
            <Input
              placeholder="Find a heap"
              className="h-12 w-full rounded-xl border-2 border-transparent bg-sunken pr-11 pl-10 text-base outline-none placeholder:text-muted focus:border-accent sm:h-10 sm:rounded-[9px] sm:pr-10 sm:pl-9 sm:text-sm [&::-webkit-search-cancel-button]:hidden"
            />
            {text !== "" && (
              <Button
                aria-label="Clear"
                className="absolute right-0 flex size-12 items-center justify-center rounded-xl text-ink-soft outline-none hover:text-ink focus-visible:outline-2 focus-visible:outline-accent sm:right-2.5 sm:size-10"
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

/** A section of the list, set apart from the one above; its title is a band a shade off the panel. */
const sectionClass = "flex flex-col not-first:mt-3";

export function SectionTitle({ children, count }: { children: ReactNode; count?: number }) {
  return (
    <Header className="flex items-center px-3 text-xs font-semibold tracking-wide text-ink-soft uppercase max-sm:shrink-0 max-sm:pt-2.5 max-sm:pb-0.5 sm:mb-1 sm:min-h-8 sm:rounded-lg sm:bg-sunken sm:px-2.5 sm:text-[11px]">
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
  recent = false,
  enterHint = false,
  extraClass = "",
  onOpen,
}: {
  id: string;
  entry: Entry;
  current?: boolean;
  teamLabel?: string;
  query?: string;
  /** A row of Recent: a clock before the name in the popover. */
  recent?: boolean;
  /** Shows ↵ on the row while the keyboard is on it. */
  enterHint?: boolean;
  extraClass?: string;
  onOpen: () => void;
}) {
  const { heap } = entry;
  return (
    <MenuItem
      id={id}
      textValue={heap.name}
      href={{ to: "/teams/$team/heaps/$heap", params: { team: entry.team.slug, heap: heap.slug } }}
      onAction={onOpen}
      className={`${rowClass} ${current ? currentRowClass : ""} ${extraClass}`}
    >
      {({ isFocused }) => (
        <>
          {recent && <Clock aria-hidden="true" className="size-3.5 shrink-0 text-muted max-sm:hidden" />}
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
          {teamLabel && <span className="shrink-0 text-[13px] font-normal text-muted sm:text-xs">{teamLabel}</span>}
          {enterHint && isFocused && (
            <kbd aria-hidden="true" className={kbdClass}>
              ↵
            </kbd>
          )}
          <span aria-hidden="true" className="w-5 shrink-0 text-accent-ink sm:w-4">
            {current && <Check className="size-4" strokeWidth={3} />}
          </span>
        </>
      )}
    </MenuItem>
  );
}

const newHeapButtonClass =
  "flex min-h-12 flex-1 items-center justify-center gap-1.5 rounded-xl border border-line bg-surface px-3 text-[15px] hover:border-muted disabled:opacity-50";

/** "New heap in <team>", for those the team lets create heaps. */
function NewHeapButton({ me, team, onClick }: { me: Me; team: SwitcherTeam; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`${newHeapButtonClass} min-w-0 sm:min-h-[34px] sm:flex-none sm:rounded-lg sm:px-2.5 sm:text-[13px] sm:font-medium sm:pointer-coarse:min-h-11`}
    >
      <Plus aria-hidden="true" className="size-4 shrink-0 sm:size-3.5" strokeWidth={2.5} />
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
    create.mutate(
      { name: name.trim() },
      {
        onSuccess: (heap) => {
          onCreated();
          void navigate({ to: "/teams/$team/heaps/$heap", params: { team: team.slug, heap: heap.slug } });
        },
      },
    );
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
