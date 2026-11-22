// The confirm before a heap parameter becomes its team's and same-named heap parameters merge into it (design/ux.md,
// Parameters): the shell of the team tag's merge confirm, with the parameters it takes in.
import { ApiProblem } from "../api/client";
import type { ParameterMergeReach } from "../api/schema";
import { RequestId } from "../components/RequestId";
import { problemMessage } from "../components/ProblemText";
import { listOf, plural } from "../components/words";
import { MergeDialog } from "./MergeConfirm";
import { chipClass } from "./TagSettings";
import { useMergeReachConfirm, type Send } from "./useMergeReachConfirm";

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
 * Lists the heap parameters making `name` a team parameter would merge and asks first, with the flow of the tag
 * merge's confirm (`useMergeReachConfirm`). `reach` is what the write was refused with.
 */
export function ParameterMergeConfirm({
  mutation,
  parameterId,
  name,
  reach: refused,
  onDone,
  onClose,
}: {
  mutation: Send<{ parameterId: string }>;
  parameterId: string;
  name: string;
  reach: ParameterMergeReach;
  onDone: (confirmed: ParameterMergeReach) => void;
  onClose: () => void;
}) {
  const flow = useMergeReachConfirm({
    mutation,
    variables: { parameterId },
    refused,
    partsOf: (r: ParameterMergeReach) =>
      r.entries.map((entry) => ({
        id: entry.parameter_id,
        items: entry.items,
        label: `${entry.heap.name}'s “${entry.parameter_name}”`,
      })),
    reachOf: parameterMergeReachOf,
    onDone,
    onClose,
  });
  const { reach, pending, error, bodyId, band } = flow;
  const items = reach.entries.reduce((sum, entry) => sum + entry.items, 0);

  const body = (
    <div id={bodyId} className="flex min-w-0 flex-col gap-3 text-sm text-ink-soft">
      {flow.changed && (
        <p ref={band} tabIndex={-1} className="rounded-lg bg-accent-soft px-3 py-2 text-accent-ink outline-none">
          This changed while it was open. Check the list again; the button confirms this one.
        </p>
      )}
      <ul aria-label="Heap parameters it merges" className="max-h-[40dvh] divide-y divide-line overflow-y-auto">
        {reach.entries.map((entry) => {
          const was = flow.was(entry.parameter_id, entry.items);
          return (
            <li
              key={entry.parameter_id}
              className="flex flex-wrap items-center gap-x-2.5 gap-y-1 py-2 max-sm:gap-x-2 sm:min-h-10"
            >
              <span className="min-w-0 font-medium break-words text-ink sm:w-32 sm:shrink-0">{entry.heap.name}</span>
              <span className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 max-sm:order-2 max-sm:basis-full sm:contents">
                <span className={chipClass}>{entry.parameter_name}</span>
                <span>{plural(entry.items, "item")}</span>
                {was !== undefined && <span className={badgeClass}>was {was}</span>}
                {entry.added_options.length > 0 && <span>adds {listOf(entry.added_options)}</span>}
                {entry.switched.length > 0 && (
                  <span>moves {listOf(entry.switched.map((switched) => `${switched.from} → ${switched.to}`))}</span>
                )}
                {entry.linked_tags.length > 0 && <span>links {listOf(entry.linked_tags)}</span>}
              </span>
              <span className="flex gap-1.5 max-sm:order-1 sm:ml-auto">
                {entry.archived && <span className={badgeClass}>archived</span>}
                {entry.starting && <span className={badgeClass}>this heap</span>}
                {flow.isNew(entry.parameter_id) && <span className={badgeClass}>new</span>}
              </span>
            </li>
          );
        })}
      </ul>
      <p className="text-ink">
        {plural(items, "item")} in {plural(reach.entries.length, "heap")} keep their values.
      </p>
      {flow.left.length > 0 && <p>Left the list: {listOf(flow.left)}.</p>}
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
      actLabel="Make team parameter"
      pending={pending}
      onConfirm={flow.confirm}
      onCancel={flow.cancel}
    />
  );
}
