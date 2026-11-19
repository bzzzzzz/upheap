import { useNavigate, useParams, useRouter, useRouterState } from "@tanstack/react-router";
import { ListChecks } from "lucide-react";

import type { HeapRef } from "../api/client";
import { useMe } from "../api/queries/me";

/**
 * Select mode on a heap's list (ux.md, The list §19). `canSelect` says whether the heap offers it: the heap isn't
 * archived and its team has another heap to move to, as `ItemMore` tests for Move. Select mode is a history entry,
 * `state.selecting`, so Back leaves it; `enter` goes to the list, and `leave` goes back. With an item open, the entry it leaves is replaced by the list
 * first, so Back lands on the list and not on the item.
 */
export function useSelectMode(heapRef: HeapRef | undefined) {
  const { data: me } = useMe();
  const router = useRouter();
  const navigate = useNavigate();
  const itemOpen = useParams({ strict: false, select: (params) => params.item !== undefined });
  const selecting = useRouterState({ select: (state) => state.location.state.selecting === true });
  const team = me?.teams.find((t) => t.slug === heapRef?.team);
  const heap = team?.heaps.find((h) => h.slug === heapRef?.heap);
  const canSelect = !!team && !!heap && !heap.archived && team.heaps.some((h) => !h.archived && h.slug !== heap.slug);
  return {
    selecting,
    canSelect,
    enter: () => {
      if (!heapRef) return;
      const go = async () => {
        const toList = { to: "/teams/$team/heaps/$heap", params: heapRef, search: true, resetScroll: false } as const;
        // The entry under select mode must be the list, or leaving reopens the item, which may have moved by then.
        if (itemOpen) await navigate({ ...toList, replace: true, state: {} });
        await navigate({ ...toList, state: { selecting: true } });
      };
      void go();
    },
    leave: () => {
      // Read when called: a run that was going when select mode was left must not go back a second time.
      if (router.state.location.state.selecting) router.history.back();
    },
  };
}

/** The toolbar's Select, as quiet as Filters with nothing on. Not on a phone, whose top bar has its own. */
export function SelectButton({ heapRef }: { heapRef: HeapRef }) {
  const { canSelect, enter } = useSelectMode(heapRef);
  if (!canSelect) return null;
  return (
    <button
      type="button"
      onClick={enter}
      className="inline-flex h-9 shrink-0 items-center justify-center gap-1.5 rounded-lg border border-line bg-surface px-3 text-sm font-medium text-ink-soft hover:bg-sunken max-sm:hidden pointer-coarse:h-11"
    >
      <ListChecks aria-hidden="true" className="size-4 shrink-0" />
      Select
    </button>
  );
}
