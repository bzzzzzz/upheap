import { useNavigate, useRouter, useRouterState } from "@tanstack/react-router";
import { ListChecks } from "lucide-react";

import type { HeapRef } from "../api/client";
import { useMe } from "../api/queries";

/**
 * Select mode on a heap's list (ux.md, The list §19). `canSelect` says whether the heap offers it: the heap isn't
 * archived and its team has another heap to move to, as `ItemMore` tests for Move. Select mode is a history entry,
 * `state.selecting`, so Back leaves it; `enter` goes to the list, which also closes an open item, and `leave` goes back.
 */
export function useSelectMode(heapRef: HeapRef | undefined) {
  const { data: me } = useMe();
  const router = useRouter();
  const navigate = useNavigate();
  const selecting = useRouterState({ select: (state) => state.location.state.selecting === true });
  const team = me?.teams.find((t) => t.slug === heapRef?.team);
  const heap = team?.heaps.find((h) => h.slug === heapRef?.heap);
  const canSelect = !!team && !!heap && !heap.archived && team.heaps.some((h) => !h.archived && h.slug !== heap.slug);
  return {
    selecting,
    canSelect,
    enter: () => {
      if (!heapRef) return;
      void navigate({
        to: "/teams/$team/heaps/$heap",
        params: heapRef,
        search: true,
        state: { selecting: true },
        resetScroll: false,
      });
    },
    leave: () => {
      if (selecting) router.history.back();
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
