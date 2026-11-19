import { Check, Search, X } from "lucide-react";
import { useState } from "react";
import { Autocomplete, Button, Input, Label, Menu, MenuItem, SearchField } from "react-aria-components";

import type { ItemRef } from "../api/schema";
import { currentRowClass, rowClass } from "./HeapSwitcher";
import { isClosed, STATE_LABELS } from "./labels";

/** The id of the row that clears the blocker; item ids are never empty. */
const NOTHING = "";

/**
 * The body of "Change blocker" (ux.md, The item §6): a search field over the items a blocked item may wait for, as in
 * Move to. `candidates` are the heap's other non-closed items that wouldn't make a cycle, in list order; typing keeps
 * those whose title holds the text. Nothing picked comes first while the field is empty. The current blocker, or
 * Nothing picked when there's none, is marked and can't be picked again. A pick calls `onChange`.
 */
export function BlockerSelect({
  value,
  candidates,
  onChange,
}: {
  value: ItemRef | null | undefined;
  candidates: ItemRef[];
  onChange: (blockerId: string | null) => void;
}) {
  const [text, setText] = useState("");
  // A closed blocker isn't among the candidates, but the list still has to show it.
  const options = value && !candidates.some((c) => c.id === value.id) ? [value, ...candidates] : candidates;
  const query = text.trim().toLowerCase();
  const rows = [
    ...(query === "" ? [{ id: NOTHING, title: "Nothing picked", state: undefined }] : []),
    ...options.filter((item) => item.title.toLowerCase().includes(query)),
  ];
  const current = value?.id ?? NOTHING;

  return (
    <Autocomplete inputValue={text} onInputChange={setText}>
      <SearchField autoFocus className="relative flex shrink-0 items-center">
        <Label className="sr-only">Find an item</Label>
        <Search aria-hidden="true" className="pointer-events-none absolute left-3.5 size-4 text-muted" />
        <Input
          placeholder="Find an item"
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
      <Menu
        aria-label="Waiting for"
        items={rows}
        disabledKeys={[current]}
        onAction={(id) => onChange(id === NOTHING ? null : String(id))}
        renderEmptyState={() => <p className="px-3 py-2 text-sm text-muted">No item matches “{text.trim()}”</p>}
        className="mt-2 flex flex-col outline-none"
      >
        {(row) => {
          const here = row.id === current;
          return (
            <MenuItem id={row.id} textValue={row.title} className={`${rowClass} ${here ? currentRowClass : ""}`}>
              <span className="min-w-0 flex-1 truncate">
                {row.title}
                {row.state && isClosed(row.state) && (
                  <span className="font-normal text-muted"> ({STATE_LABELS[row.state].toLowerCase()})</span>
                )}
              </span>
              <span aria-hidden="true" className="w-5 shrink-0 text-accent-ink sm:w-4">
                {here && <Check className="size-4" strokeWidth={3} />}
              </span>
            </MenuItem>
          );
        }}
      </Menu>
    </Autocomplete>
  );
}
