import type { ItemTag } from "../api/client";

/**
 * An item's tags in a list row, in the order the server sends them, as grey chips (violet is kept for parameters).
 * Each chip is a toggle button that adds the tag to the list's tag filter or, when it is in `active`, removes it;
 * active chips are outlined in the accent (ux.md, The list). The button sits above the row's link, so pressing it
 * doesn't open the item.
 */
export function TagChips({
  tags,
  active,
  onToggle,
}: {
  tags: ItemTag[];
  active: ReadonlySet<string>;
  onToggle: (id: string) => void;
}) {
  if (tags.length === 0) return null;
  return (
    <ul className="inline-flex flex-wrap gap-1" aria-label="Tags">
      {tags.map((tag) => {
        const on = active.has(tag.id);
        return (
          <li key={tag.id} className="flex">
            {/* The pseudo-element makes the hit area 24 px tall without growing the chip: a tag is a secondary
                control, so it keeps the 24 px minimum rather than the 44 px of the primary buttons. A toggle keeps
                one name, and aria-pressed says whether it is on. */}
            <button
              type="button"
              aria-pressed={on}
              aria-label={`Filter by tag ${tag.name}`}
              onClick={() => onToggle(tag.id)}
              className={`relative z-10 cursor-pointer rounded-md border px-1.5 text-xs before:absolute before:inset-x-0 before:-inset-y-1 before:content-[''] ${
                on
                  ? "border-accent bg-transparent text-accent-ink"
                  : "border-transparent bg-sunken text-ink-soft hover:text-ink"
              }`}
            >
              {tag.name}
            </button>
          </li>
        );
      })}
    </ul>
  );
}
