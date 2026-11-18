import { Check, Search, X } from "lucide-react";
import { useState } from "react";
import { Autocomplete, Button, Input, Label, Menu, MenuItem, MenuSection, SearchField } from "react-aria-components";

import type { HeapRef } from "../api/client";
import type { Me } from "../api/schema";
import { compactButtonClass } from "./buttons";
import { currentRowClass, LONG_LIST, rowClass, SectionTitle } from "./HeapSwitcher";
import { ProblemText } from "./ProblemText";
import { teamName } from "./teamName";

type MoveTeam = Me["teams"][number];
type MoveHeap = MoveTeam["heaps"][number];

/**
 * The body of "Move to" (ux.md, The item §6): the item's team's heaps that aren't archived, the item's own marked and
 * disabled, with a search field from `LONG_LIST` heaps. A pick asks first, since tags the other heap lacks are dropped;
 * Move calls `onMove`. A refusal shows in `error`, above the buttons, and Cancel clears it.
 */
export function MoveToHeap({
  me,
  team,
  heapRef,
  pending,
  error,
  onMove,
  onReset,
}: {
  me: Me;
  team: MoveTeam;
  heapRef: HeapRef;
  pending: boolean;
  error: Error | null;
  onMove: (to: HeapRef & { id: string }, name: string) => void;
  onReset: () => void;
}) {
  const [picked, setPicked] = useState<MoveHeap | null>(null);
  const [text, setText] = useState("");
  const heaps = team.heaps.filter((heap) => !heap.archived);
  const long = heaps.length >= LONG_LIST;
  const query = long ? text.trim().toLowerCase() : "";
  const shown = heaps.filter((heap) => heap.name.toLowerCase().includes(query));
  const label = teamName(team, me);

  if (picked) {
    return (
      <div className="flex flex-col gap-3">
        <h3 className="text-base font-semibold break-words">Move to {picked.name}?</h3>
        <p className="text-sm text-ink-soft">Tags it doesn't have there are dropped.</p>
        <ProblemText error={error} />
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
            onClick={() => onMove({ team: heapRef.team, heap: picked.slug, id: picked.id }, picked.name)}
            className={compactButtonClass.primary}
          >
            {pending ? "Moving…" : "Move"}
          </button>
        </div>
      </div>
    );
  }

  const menu = (
    <Menu aria-label="Move to" autoFocus={!long} className="outline-none">
      <MenuSection aria-label={label} className="flex flex-col">
        <SectionTitle>{label}</SectionTitle>
        {shown.map((heap) => {
          const here = heap.slug === heapRef.heap;
          return (
            <MenuItem
              key={heap.id}
              id={heap.id}
              textValue={heap.name}
              isDisabled={here}
              onAction={() => setPicked(heap)}
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
    </Menu>
  );

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
    </Autocomplete>
  ) : (
    menu
  );
}
