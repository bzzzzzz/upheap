import type { ItemTag } from "../api/client";

/** An item's tags, in the order the server sends them, as grey chips (violet is kept for parameters). */
export function TagChips({ tags }: { tags: ItemTag[] }) {
  if (tags.length === 0) return null;
  return (
    <ul className="inline-flex flex-wrap gap-1" aria-label="Tags">
      {tags.map((tag) => (
        <li key={tag.id} className="rounded-md bg-sunken px-1.5 text-xs text-ink-soft">
          {tag.name}
        </li>
      ))}
    </ul>
  );
}
