import { useId, useState, type KeyboardEvent, type ReactNode } from "react";

import { ApiProblem, type ItemTag, type HeapRef } from "../api/client";
import { useCreateTag, useTags } from "../api/queries";
import { FieldFrame, type FieldLayout } from "./ChoiceButtons";

const MAX_SUGGESTIONS = 6;

/**
 * Picks tags for an item. Typing filters the heap's tags; Enter adds the highlighted one, or creates a new tag
 * when nothing matches the typed name exactly. Backspace in the empty input removes the last tag.
 */
export function TagPicker({
  heapRef,
  value,
  onChange,
  disabled,
  inline = false,
  layout = "stack",
  label = <span className="text-xs font-medium text-ink-soft">Tags</span>,
}: {
  heapRef: HeapRef;
  value: ItemTag[];
  onChange: (tags: ItemTag[]) => void;
  disabled?: boolean;
  /** The matches in the flow, under the field, rather than over what follows; for a phone's sheet, which grows. */
  inline?: boolean;
  layout?: FieldLayout;
  /** The heading above the tags, for a page that names them otherwise, such as a parameter's "Suggested by tags". */
  label?: ReactNode;
}) {
  const tags = useTags(heapRef);
  const create = useCreateTag(heapRef);
  const listId = useId();
  const [text, setText] = useState("");
  const [highlight, setHighlight] = useState(0);

  const canCreate = tags.data?.allowed_actions.includes("create") ?? false;
  // While a new tag is being created the chosen tags are frozen, because the new tag is appended to the list as it
  // was when Enter was pressed. The input stays focusable (read-only, not disabled) so typing can resume at once.
  const busy = create.isPending;
  const query = text.trim();
  // Matching typed text against names the client already has is search, which stays on the client.
  const needle = query.toLowerCase();
  const chosen = new Set(value.map((tag) => tag.id));
  const available = (tags.data?.tags ?? []).filter((tag) => !chosen.has(tag.id));
  const exact = available.find((tag) => tag.name.toLowerCase() === needle);
  // The exact match comes first, so Enter on a full name picks that tag.
  const partial = available.filter((tag) => tag !== exact && tag.name.toLowerCase().includes(needle));
  const suggestions = query ? [...(exact ? [exact] : []), ...partial].slice(0, MAX_SUGGESTIONS) : [];
  const alreadyChosen = value.some((tag) => tag.name.toLowerCase() === needle);
  const offerCreate = canCreate && query !== "" && !exact && !alreadyChosen;
  const optionCount = suggestions.length + (offerCreate ? 1 : 0);
  const open = optionCount > 0;

  const add = (tag: ItemTag) => {
    onChange([...value, { id: tag.id, name: tag.name }]);
    setText("");
    setHighlight(0);
  };

  const createTag = () => {
    if (busy) return;
    // Through the promise rather than `onSuccess`, which is dropped if the picker goes away meanwhile (a phone's
    // sheet closed at once), so the new tag still lands on the item. A failure shows as `create.error`.
    create.mutateAsync({ name: query }).then(add, () => {});
  };

  const choose = (index: number) => {
    if (busy) return;
    const suggestion = suggestions[index];
    if (suggestion) {
      add(suggestion);
    } else if (offerCreate) {
      createTag();
    }
  };

  const onKey = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown" && open) {
      event.preventDefault();
      setHighlight((highlight + 1) % optionCount);
    } else if (event.key === "ArrowUp" && open) {
      event.preventDefault();
      setHighlight((highlight - 1 + optionCount) % optionCount);
    } else if (event.key === "Enter" && query !== "" && !event.metaKey && !event.ctrlKey) {
      // Enter picks a tag here instead of submitting the item. With ⌘ or Ctrl it's left to the form.
      event.preventDefault();
      choose(highlight);
    } else if (event.key === "Backspace" && text === "" && value.length > 0 && !busy) {
      onChange(value.slice(0, -1));
    } else if (event.key === "Escape" && text !== "") {
      // Handled here, so a form or dialog around the picker doesn't treat it as cancel.
      event.preventDefault();
      event.stopPropagation();
      setText("");
    }
  };

  const optionId = (index: number) => `${listId}-${index}`;

  return (
    <FieldFrame layout={layout} label={label}>
      <div className="flex flex-wrap items-center gap-1">
        {value.map((tag) => (
          // To a finger, Remove is 44 px square around its 20 px circle. It stays inside the chip, which widens for it
          // and with its margins takes a 44 px line, so the target never covers the tag's name, the next chip or the
          // input.
          <span
            key={tag.id}
            className="inline-flex h-7 items-center gap-1 rounded-full bg-sunken pr-1 pl-2.5 text-sm text-ink-soft pointer-coarse:my-2 pointer-coarse:pr-0"
          >
            {tag.name}
            <button
              type="button"
              disabled={disabled || busy}
              onClick={() => onChange(value.filter((t) => t.id !== tag.id))}
              aria-label={`Remove tag ${tag.name}`}
              className="group flex size-5 shrink-0 items-center justify-center rounded-full text-muted hover:text-ink pointer-coarse:size-11"
            >
              <span className="flex size-5 items-center justify-center rounded-full group-hover:bg-line">×</span>
            </button>
          </span>
        ))}
        <input
          role="combobox"
          aria-label="Add a tag"
          aria-expanded={open}
          aria-controls={listId}
          aria-autocomplete="list"
          aria-activedescendant={open ? optionId(highlight) : undefined}
          value={text}
          disabled={disabled}
          readOnly={busy}
          aria-busy={busy}
          onChange={(e) => {
            setText(e.target.value);
            setHighlight(0);
          }}
          onKeyDown={onKey}
          placeholder={canCreate ? "Type to add or create" : "Type to add"}
          autoComplete="off"
          className="h-9 min-w-32 flex-1 rounded-lg border border-line bg-surface px-2 text-sm text-ink pointer-coarse:h-11"
        />
      </div>
      {open && (
        <ul
          id={listId}
          role="listbox"
          aria-label="Tags"
          className={`mt-1 rounded-lg border border-line bg-surface py-1 text-sm text-ink ${
            inline ? "" : "absolute top-full right-0 left-0 z-10 shadow-md"
          }`}
        >
          {suggestions.map((tag, index) => (
            <li
              key={tag.id}
              id={optionId(index)}
              role="option"
              aria-selected={index === highlight}
              onMouseDown={(e) => {
                e.preventDefault();
                if (!busy) add(tag);
              }}
              className="cursor-pointer px-3 py-2 aria-selected:bg-sunken"
            >
              {tag.name}
              {tag.description && <span className="ml-2 text-xs text-muted">{tag.description}</span>}
            </li>
          ))}
          {offerCreate && (
            <li
              id={optionId(suggestions.length)}
              role="option"
              aria-selected={highlight === suggestions.length}
              onMouseDown={(e) => {
                e.preventDefault();
                createTag();
              }}
              className="cursor-pointer px-3 py-2 aria-selected:bg-sunken"
            >
              Create tag “{query}”
            </li>
          )}
        </ul>
      )}
      {create.error && (
        <p role="alert" className="text-xs text-overdue-ink">
          {create.error instanceof ApiProblem && create.error.code === "tag_name_taken"
            ? "A tag with that name already exists."
            : create.error.message}
        </p>
      )}
    </FieldFrame>
  );
}
