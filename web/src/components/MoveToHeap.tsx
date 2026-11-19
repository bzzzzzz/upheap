import { Check, Search, X } from "lucide-react";
import { useState } from "react";
import { Autocomplete, Button, Input, Label, Menu, MenuItem, MenuSection, SearchField } from "react-aria-components";

import { ApiProblem, type HeapRef } from "../api/client";
import type { Me } from "../api/schema";
import { compactButtonClass } from "./buttons";
import { currentRowClass, LONG_LIST, rowClass, SectionTitle } from "./HeapSwitcher";
import { itemProblem } from "./itemProblems";
import { teamName } from "./teamName";
import { RequestId } from "./RequestId";

type MoveTeam = Me["teams"][number];
type MoveHeap = MoveTeam["heaps"][number];

/**
 * The body of "Move to" (ux.md, The item §6): the heaps that aren't archived of each of `teams`, the first being the
 * item's own, one group per team that has any, the item's own heap marked and disabled, with a search field from
 * `LONG_LIST` heaps in all. A pick asks first, since tags the other heap lacks are dropped, and a pick in another team
 * says what else changes. `turnBy` names whoever else sent the item to an agent, whose turn may be taken back there;
 * `null` is a turn nobody can be traced for, which is always taken back. Move calls `onMove` with the picked heap and
 * the label for the toast. With `teamOnly`, for a note heaps were made from, a line under the list says why only its
 * team is listed. A refusal shows in `error`, above the buttons, and Cancel clears it. With `count`, it moves that
 * many items, as the list's selection does (ux.md, The list §19), and the confirm says so.
 */
export function MoveToHeap({
  me,
  teams,
  heapRef,
  pending,
  error,
  count,
  teamOnly,
  turnBy,
  onMove,
  onReset,
}: {
  me: Me;
  teams: MoveTeam[];
  heapRef: HeapRef;
  pending: boolean;
  error: Error | null;
  count?: number;
  teamOnly?: boolean;
  turnBy?: string | null;
  onMove: (to: HeapRef & { id: string }, label: string) => void;
  onReset: () => void;
}) {
  const [picked, setPicked] = useState<{ team: MoveTeam; heap: MoveHeap } | null>(null);
  const [text, setText] = useState("");
  const groups = teams
    .map((team) => ({ team, heaps: team.heaps.filter((heap) => !heap.archived) }))
    .filter((group) => group.heaps.length > 0);
  const long = groups.reduce((total, group) => total + group.heaps.length, 0) >= LONG_LIST;
  const query = long ? text.trim().toLowerCase() : "";
  const shown = groups
    .map(({ team, heaps }) => ({ team, heaps: heaps.filter((heap) => heap.name.toLowerCase().includes(query)) }))
    .filter((group) => group.heaps.length > 0);
  const [first] = teams;
  const own = first ? teamName(first, me) : "";
  const teamOnlyLine = `Heaps were made from this note, so it moves only within ${own}.`;

  if (picked) {
    const to = teamName(picked.team, me);
    const across = count === undefined && picked.team.slug !== heapRef.team;
    const problem =
      error instanceof ApiProblem && error.code === "note_has_heaps" ? teamOnlyLine : error && itemProblem(error);
    return (
      <div className="flex flex-col gap-3">
        <h3 className="text-base font-semibold break-words">
          {count === undefined
            ? `Move to ${picked.heap.name}${across ? ` in ${to}` : ""}?`
            : `Move ${count === 1 ? "1 item" : `${count} items`} to ${picked.heap.name}?`}
        </h3>
        {across ? (
          <>
            <p className="text-sm text-ink-soft">{`Tags and values ${to} doesn't have are dropped.`}</p>
            <p className="text-sm text-ink-soft">
              {`Its history stays in ${own}. People in ${to} see it from now on, and don't see its links to ${own}.`}
            </p>
            {turnBy !== undefined && (
              <p className="text-sm text-ink-soft">
                {turnBy === null
                  ? "Its turn with an agent is taken back."
                  : `${turnBy} sent it to an agent. If ${turnBy} isn't in ${to}, the turn is taken back.`}
              </p>
            )}
          </>
        ) : (
          <p className="text-sm text-ink-soft">
            {count === undefined
              ? "Tags it doesn't have there are dropped."
              : "Tags they don't have there are dropped."}
          </p>
        )}
        {error && (
          <>
            <p role="alert" className="text-sm text-overdue-ink">
              {problem}
            </p>
            <RequestId error={error} />
          </>
        )}
        <div className="flex items-center justify-end gap-2">
          <button
            type="button"
            disabled={pending}
            onClick={() => {
              onReset();
              setPicked(null);
            }}
            className={compactButtonClass.quiet}
          >
            Cancel
          </button>
          <button
            type="button"
            autoFocus
            disabled={pending}
            onClick={() =>
              onMove(
                { team: picked.team.slug, heap: picked.heap.slug, id: picked.heap.id },
                across ? `${picked.heap.name} in ${to}` : picked.heap.name,
              )
            }
            className={compactButtonClass.primary}
          >
            {pending ? "Moving…" : count === undefined ? "Move" : `Move ${count}`}
          </button>
        </div>
      </div>
    );
  }

  const menu = (
    <Menu aria-label="Move to" autoFocus={!long} className="outline-none">
      {shown.map(({ team, heaps }) => {
        const label = teamName(team, me);
        return (
          <MenuSection key={team.slug} aria-label={label} className="flex flex-col">
            <SectionTitle>{label}</SectionTitle>
            {heaps.map((heap) => {
              const here = team.slug === heapRef.team && heap.slug === heapRef.heap;
              return (
                <MenuItem
                  key={heap.id}
                  id={heap.id}
                  textValue={heap.name}
                  isDisabled={here}
                  onAction={() => setPicked({ team, heap })}
                  className={`${rowClass} ${here ? currentRowClass : ""}`}
                >
                  <span className="min-w-0 flex-1 truncate">
                    {heap.name}
                    {here && <span className="font-normal text-muted"> (here)</span>}
                  </span>
                  <span aria-hidden="true" className="w-5 shrink-0 text-accent-ink sm:w-4">
                    {here && <Check className="size-4" strokeWidth={3} />}
                  </span>
                </MenuItem>
              );
            })}
          </MenuSection>
        );
      })}
    </Menu>
  );
  const noteLine = teamOnly && <p className="mt-3 text-sm text-ink-soft">{teamOnlyLine}</p>;

  return long ? (
    <Autocomplete inputValue={text} onInputChange={setText}>
      <SearchField autoFocus className="relative flex shrink-0 items-center">
        <Label className="sr-only">Find a heap</Label>
        <Search aria-hidden="true" className="pointer-events-none absolute left-3.5 size-4 text-muted" />
        <Input
          placeholder="Find a heap"
          className="h-12 w-full rounded-xl border-2 border-transparent bg-sunken pr-11 pl-10 text-base outline-none placeholder:text-muted focus:border-accent sm:h-10 sm:rounded-[9px] sm:pr-10 sm:pl-9 sm:text-sm [&::-webkit-search-cancel-button]:hidden"
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
      {menu}
      {noteLine}
    </Autocomplete>
  ) : (
    <>
      {menu}
      {noteLine}
    </>
  );
}
