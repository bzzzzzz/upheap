import type { ItemTag } from "../api/client";

/** An item's tags, in the order the server sends them. With `onRemove`, each tag gets a remove button. */
export function TagChips({ tags, onRemove }: { tags: ItemTag[]; onRemove?: (tag: ItemTag) => void }) {
  if (tags.length === 0) return null;
  return (
    <ul className="inline-flex flex-wrap gap-1" aria-label="Tags">
      {tags.map((tag) => (
        <li
          key={tag.id}
          className="inline-flex items-center gap-1 rounded bg-sky-100 px-1.5 text-xs text-sky-900 dark:bg-sky-950 dark:text-sky-200"
        >
          {tag.name}
          {onRemove && (
            <button
              type="button"
              onClick={() => onRemove(tag)}
              aria-label={`Remove tag ${tag.name}`}
              className="text-sky-700 hover:text-sky-950 dark:text-sky-400 dark:hover:text-sky-100"
            >
              ×
            </button>
          )}
        </li>
      ))}
    </ul>
  );
}
