// The confirm before a heap parameter becomes its team's and same-named heap parameters merge into it (design/ux.md,
// Parameters): the shell of the team tag's merge confirm, with the parameters it takes in.
import { useEffect, useId, useRef, useState } from "react";

import { ApiProblem } from "../api/client";
import type { ParameterMergeReach } from "../api/schema";
import { RequestId } from "../components/RequestId";
import { problemMessage } from "../components/ProblemText";
import { listOf, plural } from "../components/words";
import { MergeDialog } from "./MergeConfirm";
import { chipClass } from "./TagSettings";

/** The part of a mutation the confirm uses. */
type Send = {
  mutateAsync: (variables: { parameterId: string; confirm: string }) => Promise<unknown>;
  reset: () => void;
  isPending: boolean;
  error: Error | null;
};

const badgeClass = "rounded-full bg-sunken px-2 py-0.5 text-xs text-muted";

/** The reach a write was refused with because it would merge heap parameters, or nothing for any other error. */
export const parameterMergeReachOf = (error: unknown): ParameterMergeReach | undefined =>
  error instanceof ApiProblem && error.code === "parameter_merge_unconfirmed" ? error.parameterMergeReach : undefined;

/** The toast after a confirmed merge: how many items in how many heaps keep their values. */
export const keptValues = (reach: ParameterMergeReach) =>
  `${plural(
    reach.entries.reduce((sum, entry) => sum + entry.items, 0),
    "item",
  )} in ${plural(reach.entries.length, "heap")} keep their values.`;

/**
 * Lists the heap parameters making `name` a team parameter would merge and asks first. `reach` is what the write was
 * refused with; confirming sends it again with the token. If the reach changed meanwhile, the server refuses again with
 * the new one: the dialog shows it, marks what differs, and the button confirms the new one. Success calls `onDone`
 * with the reach that was confirmed; Cancel clears the mutation's error and calls `onClose`.
 */
export function ParameterMergeConfirm({
  mutation,
  parameterId,
  name,
  reach: refused,
  onDone,
  onClose,
}: {
  mutation: Send;
  parameterId: string;
  name: string;
  reach: ParameterMergeReach;
  onDone: (confirmed: ParameterMergeReach) => void;
  onClose: () => void;
}) {
  const [shown, setShown] = useState<{ reach: ParameterMergeReach; previous: ParameterMergeReach | null }>({
    reach: refused,
    previous: null,
  });
  const { reach, previous } = shown;
  const { isPending: pending, error } = mutation;
  const bodyId = useId();
  // A changed list moves focus to the band that says so, so a screen reader reads it before the button again.
  const band = useRef<HTMLParagraphElement>(null);
  useEffect(() => {
    if (previous) band.current?.focus();
  }, [previous]);
  const items = reach.entries.reduce((sum, entry) => sum + entry.items, 0);
  const before = previous && new Map(previous.entries.map((entry) => [entry.parameter_id, entry.items]));
  const left = previous
    ? previous.entries.filter((old) => !reach.entries.some((entry) => entry.parameter_id === old.parameter_id))
    : [];

  const cancel = () => {
    if (pending) return;
    mutation.reset();
    onClose();
  };
  const confirm = () => {
    mutation.mutateAsync({ parameterId, confirm: reach.token }).then(
      () => onDone(reach),
      (failure: unknown) => {
        const changed = parameterMergeReachOf(failure);
        if (changed) setShown({ reach: changed, previous: reach });
      },
    );
  };

  const body = (
    <div id={bodyId} className="flex min-w-0 flex-col gap-3 text-sm text-ink-soft">
      {previous && (
        <p ref={band} tabIndex={-1} className="rounded-lg bg-accent-soft px-3 py-2 text-accent-ink outline-none">
          This changed while it was open. Check the list again; the button confirms this one.
        </p>
      )}
      <ul aria-label="Heap parameters it merges" className="max-h-[40dvh] divide-y divide-line overflow-y-auto">
        {reach.entries.map((entry) => {
          const was = before?.get(entry.parameter_id);
          return (
            <li
              key={entry.parameter_id}
              className="flex flex-wrap items-center gap-x-2.5 gap-y-1 py-2 max-sm:gap-x-2 sm:min-h-10"
            >
              <span className="min-w-0 font-medium break-words text-ink sm:w-32 sm:shrink-0">{entry.heap.name}</span>
              <span className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 max-sm:order-2 max-sm:basis-full sm:contents">
                <span className={chipClass}>{entry.parameter_name}</span>
                <span>{plural(entry.items, "item")}</span>
                {was !== undefined && was !== entry.items && <span className={badgeClass}>was {was}</span>}
                {entry.added_options.length > 0 && <span>adds {listOf(entry.added_options)}</span>}
              </span>
              <span className="flex gap-1.5 max-sm:order-1 sm:ml-auto">
                {entry.archived && <span className={badgeClass}>archived</span>}
                {entry.starting && <span className={badgeClass}>this heap</span>}
                {before && !before.has(entry.parameter_id) && <span className={badgeClass}>new</span>}
              </span>
            </li>
          );
        })}
      </ul>
      <p className="text-ink">
        {plural(items, "item")} in {plural(reach.entries.length, "heap")} keep their values.
      </p>
      {left.length > 0 && (
        <p>Left the list: {listOf(left.map((old) => `${old.heap.name}'s “${old.parameter_name}”`))}.</p>
      )}
      <p>Each heap keeps its weight. Rankings can shift in a heap whose options were in another order.</p>
      <p className="text-[13px] text-muted">The items keep their history. This can't be undone.</p>
      {error && !parameterMergeReachOf(error) && (
        <>
          <p role="alert" className="text-sm text-overdue-ink">
            {problemMessage(error)}
          </p>
          <RequestId error={error} />
        </>
      )}
    </div>
  );

  return (
    <MergeDialog
      title={`Make “${name}” a team parameter?`}
      body={body}
      bodyId={bodyId}
      actLabel={pending ? "Making…" : "Make team parameter"}
      pending={pending}
      onConfirm={confirm}
      onCancel={cancel}
    />
  );
}
