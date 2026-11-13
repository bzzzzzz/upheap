import type { ItemRef, RankedItem } from "../api/client";

/**
 * The items each item may wait for: every listed item except itself and those that already wait for it, directly
 * or down a chain, which the server would refuse as a cycle.
 */
export function blockerCandidatesFor(entries: RankedItem[]): (itemId: string) => ItemRef[] {
  const waitsFor = new Map(entries.map(({ item }) => [item.id, item.blocked_by?.id]));
  const refs: ItemRef[] = entries.map(({ item }) => ({
    id: item.id,
    key: item.key,
    title: item.title,
    kind: item.kind,
    state: item.state,
  }));
  const reaches = (from: string, target: string) => {
    const seen = new Set<string>();
    for (let at: string | undefined = from; at !== undefined && !seen.has(at); at = waitsFor.get(at)) {
      if (at === target) return true;
      seen.add(at);
    }
    return false;
  };
  return (itemId) => refs.filter((ref) => !reaches(ref.id, itemId));
}
