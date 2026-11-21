// The flow both merge confirms share (design/ux.md, Tags and Parameters): the reach a write was refused with, the
// reach it is refused with again if it changed meanwhile, what differs between the two, and the confirm and cancel
// that follow. The dialogs keep only what they list.
import { useEffect, useId, useRef, useState, type RefObject } from "react";

/** The part of a mutation the confirm uses. `V` is what the opener sent in its first attempt. */
export type Send<V> = {
  mutateAsync: (variables: V & { confirm: string }) => Promise<unknown>;
  reset: () => void;
  isPending: boolean;
  error: Error | null;
};

/** One thing a reach lists: the same `id` in two reaches is the same thing. */
export type ReachPart = { id: string; items: number; label: string };

export type MergeReachConfirm<R> = {
  reach: R;
  /** Whether the reach changed since the dialog opened. */
  changed: boolean;
  bodyId: string;
  /** The band that says the list changed; it takes focus when it appears. */
  band: RefObject<HTMLParagraphElement | null>;
  pending: boolean;
  error: Error | null;
  /** The item count the part `id` had in the reach before, when it differs from `items` now. */
  was: (id: string, items: number) => number | undefined;
  /** Whether the part `id` wasn't in the reach before. */
  isNew: (id: string) => boolean;
  /** The labels of the parts the reach before had and this one doesn't. */
  left: string[];
  confirm: () => void;
  cancel: () => void;
};

/**
 * `refused` is the reach the write was refused with; confirming sends `variables` again with its token. If the reach
 * changed meanwhile, the server refuses again with the new one, which `reachOf` reads from the failure: the hook shows
 * it and marks what differs from the one before. `onFailure` sees any other failure. Success calls `onDone` with the
 * reach that was confirmed. Cancel clears the mutation's error and calls `onClose`.
 */
export function useMergeReachConfirm<R extends { token: string }, V>({
  mutation,
  variables,
  refused,
  partsOf,
  reachOf,
  onFailure,
  onDone,
  onClose,
}: {
  mutation: Send<V>;
  variables: V;
  refused: R;
  partsOf: (reach: R) => ReachPart[];
  reachOf: (failure: unknown) => R | undefined;
  onFailure?: (failure: unknown) => void;
  onDone: (confirmed: R) => void;
  onClose: () => void;
}): MergeReachConfirm<R> {
  const [shown, setShown] = useState<{ reach: R; previous: R | null }>({ reach: refused, previous: null });
  const { reach, previous } = shown;
  const { isPending: pending, error } = mutation;
  const bodyId = useId();
  const band = useRef<HTMLParagraphElement>(null);
  useEffect(() => {
    if (previous) band.current?.focus();
  }, [previous]);
  const before = previous && new Map(partsOf(previous).map((part) => [part.id, part.items]));
  const now = partsOf(reach);
  const left = previous ? partsOf(previous).filter((old) => !now.some((part) => part.id === old.id)) : [];

  return {
    reach,
    changed: previous !== null,
    bodyId,
    band,
    pending,
    error,
    was: (id, items) => {
      const had = before?.get(id);
      return had !== undefined && had !== items ? had : undefined;
    },
    isNew: (id) => Boolean(before) && !before?.has(id),
    left: left.map((old) => old.label),
    confirm: () => {
      mutation.mutateAsync({ ...variables, confirm: reach.token }).then(
        () => onDone(reach),
        (failure: unknown) => {
          const changed = reachOf(failure);
          if (changed) setShown({ reach: changed, previous: reach });
          else onFailure?.(failure);
        },
      );
    },
    cancel: () => {
      if (pending) return;
      mutation.reset();
      onClose();
    },
  };
}
