// Counts and lists in a sentence, for the Teams and Team pages.

/** "1 heap", "2 heaps": `noun` takes an "s" for any count but one. */
export function plural(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}

/** "Launch", "Launch and Support", "Launch, Support and Ops". */
export function listOf(names: readonly string[]): string {
  const last = names.at(-1) ?? "";
  return names.length <= 1 ? last : `${names.slice(0, -1).join(", ")} and ${last}`;
}
