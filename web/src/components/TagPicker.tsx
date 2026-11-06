import { useId, useState, type KeyboardEvent } from "react";

import { ApiProblem, type ItemTag } from "../api/client";
import { useCreateTag, useTags } from "../api/queries";

const MAX_SUGGESTIONS = 6;

/**
 * Picks tags for an item. Typing filters the heap's tags; Enter adds the highlighted one, or creates a new tag
 * when nothing matches the typed name exactly. Backspace in the empty input removes the last tag.
 */
export function TagPicker({
  heapId,
  value,
  onChange,
  disabled,
}: {
  heapId: string;
  value: ItemTag[];
  onChange: (tags: ItemTag[]) => void;
  disabled?: boolean;
}) {
  const tags = useTags(heapId);
  const create = useCreateTag(heapId);
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
    create.mutate({ name: query }, { onSuccess: add });
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
    } else if (event.key === "Enter" && query !== "") {
      // Enter picks a tag here instead of submitting the item.
      event.preventDefault();
      choose(highlight);
    } else if (event.key === "Backspace" && text === "" && value.length > 0 && !busy) {
      onChange(value.slice(0, -1));
    } else if (event.key === "Escape") {
      setText("");
    }
  };

  const optionId = (index: number) => `${listId}-${index}`;

  return (
    <div className="relative flex flex-col gap-1 text-xs text-stone-500">
      <div className="flex flex-wrap items-center gap-1">
        <span>Tags</span>
        {value.map((tag) => (
          <span
            key={tag.id}
            className="inline-flex items-center gap-1 rounded bg-sky-100 px-1.5 py-0.5 text-sky-900 dark:bg-sky-950 dark:text-sky-200"
          >
            {tag.name}
            <button
              type="button"
              disabled={disabled || busy}
              onClick={() => onChange(value.filter((t) => t.id !== tag.id))}
              aria-label={`Remove tag ${tag.name}`}
              className="text-sky-700 hover:text-sky-950 dark:text-sky-400 dark:hover:text-sky-100"
            >
              ×
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
          className="min-w-32 flex-1 rounded border border-stone-300 bg-white px-1 py-0.5 text-stone-900 dark:border-stone-700 dark:bg-stone-900 dark:text-stone-100"
        />
      </div>
      {open && (
        <ul
          id={listId}
          role="listbox"
          aria-label="Tags"
          className="absolute top-full right-0 left-0 z-10 mt-1 rounded border border-stone-200 bg-white py-1 text-sm text-stone-900 shadow-md dark:border-stone-700 dark:bg-stone-900 dark:text-stone-100"
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
              className="cursor-pointer px-2 py-1 aria-selected:bg-stone-100 dark:aria-selected:bg-stone-800"
            >
              {tag.name}
              {tag.description && <span className="ml-2 text-xs text-stone-500">{tag.description}</span>}
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
              className="cursor-pointer px-2 py-1 aria-selected:bg-stone-100 dark:aria-selected:bg-stone-800"
            >
              Create tag “{query}”
            </li>
          )}
        </ul>
      )}
      {create.error && (
        <p role="alert" className="text-red-700 dark:text-red-400">
          {create.error instanceof ApiProblem && create.error.code === "tag_name_taken"
            ? "A tag with that name already exists."
            : create.error.message}
        </p>
      )}
    </div>
  );
}
