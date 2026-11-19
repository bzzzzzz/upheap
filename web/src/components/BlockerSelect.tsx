import { Check } from "lucide-react";
import { useState } from "react";
import { Autocomplete, Menu, MenuItem } from "react-aria-components";

import type { ItemRef } from "../api/schema";
import { FindField } from "./FindField";
import { currentRowClass, rowClass } from "./HeapSwitcher";
import { isClosed, STATE_LABELS } from "./labels";

/** The id of the row that clears the blocker; item ids are never empty. */
const NOTHING = "";

/**
 * The body of "Change blocker" (ux.md, The item §6): a search field over the items a blocked item may wait for, as in
 * Move to. `candidates` are the heap's other non-closed items that wouldn't make a cycle, in list order; typing keeps
 * those whose title holds the text. Nothing picked comes first while the field is empty. The current blocker, or
 * Nothing picked when there's none, is marked and can't be picked again. A pick calls `onChange`. Until the
 * candidates first load, `pending`, or when that fails, `error`, the list says so instead.
 *
 * Its panel must scroll the body (`PopoverSheet`'s `scrollBody`), so the field stays put over a long list.
 */
export function BlockerSelect({
  value,
  candidates,
  pending,
  error,
  onChange,
}: {
  value: ItemRef | null | undefined;
  candidates: ItemRef[];
  pending: boolean;
  error: Error | null;
  onChange: (blockerId: string | null) => void;
}) {
  const [text, setText] = useState("");
  // A closed blocker isn't among the candidates, but the list still has to show it.
  const options = value && !candidates.some((c) => c.id === value.id) ? [value, ...candidates] : candidates;
  const query = text.trim().toLowerCase();
  const rows =
    pending || error
      ? []
      : [
          ...(query === "" ? [{ id: NOTHING, title: "Nothing picked", state: undefined }] : []),
          ...options.filter((item) => item.title.toLowerCase().includes(query)),
        ];
  const current = value?.id ?? NOTHING;

  return (
    <Autocomplete inputValue={text} onInputChange={setText}>
      <FindField label="Find an item" text={text} />
      <Menu
        aria-label="Waiting for"
        items={rows}
        disabledKeys={[current]}
        onAction={(id) => onChange(id === NOTHING ? null : String(id))}
        renderEmptyState={() => (
          <p className="px-3 py-2 text-sm text-muted">
            {pending ? "Loading…" : error ? error.message : `No item matches “${text.trim()}”`}
          </p>
        )}
        className="flex min-h-0 flex-1 flex-col overflow-y-auto outline-none"
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
                {here && <span className="font-normal text-muted"> (current)</span>}
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
