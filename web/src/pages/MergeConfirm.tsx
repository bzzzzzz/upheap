// The confirm before a team tag merges heap tags (design/ux.md, Tags): a dialog on a wide screen, a sheet on a phone.
import { useId, useState, type ReactNode } from "react";
import { Dialog, Heading, Modal, ModalOverlay } from "react-aria-components";

import { ApiProblem } from "../api/client";
import type { MergeReach } from "../api/schema";
import { compactButtonClass } from "../components/buttons";
import { PHONE, useMedia } from "../components/media";
import { Sheet } from "../components/Sheet";
import { listOf, plural } from "../components/words";
import { chipClass, TagProblem, type TagSettings } from "./TagSettings";

/** What the write does: makes a team tag, or renames one into a name heap tags have. */
export type MergeAct = { kind: "make"; name: string } | { kind: "rename"; from: string; to: string };

/** The part of a mutation the confirm uses. `V` is what the opener sent in its first attempt. */
type Send<V> = {
  mutateAsync: (variables: V & { confirm: string }) => Promise<unknown>;
  reset: () => void;
  isPending: boolean;
  error: Error | null;
};

const badgeClass = "rounded-full bg-sunken px-2 py-0.5 text-xs text-muted";

/**
 * Lists the heap tags a team tag's write would merge and asks first. `reach` is what the write was refused with;
 * confirming sends the same `variables` again with its token. If the reach changed meanwhile, the server refuses again
 * with the new one: the dialog shows it, marks what differs from the one before, and the button confirms the new one.
 * Success calls `onDone` with the reach that was confirmed. Cancel clears the mutation's error and calls `onClose`; a
 * version conflict calls `onClose` and leaves the error with the form.
 */
export function MergeConfirm<V>({
  mutation,
  variables,
  act,
  reach: refused,
  description,
  descriptionFrom,
  promotingHeapId,
  settings,
  onDone,
  onClose,
}: {
  mutation: Send<V>;
  variables: V;
  act: MergeAct;
  reach: MergeReach;
  /** What the team tag will have, as the write sends it or keeps it. */
  description: string | null;
  /** The heap whose tag the description is, for a promote. */
  descriptionFrom?: string;
  /** The heap a promote started from. */
  promotingHeapId?: string;
  settings: TagSettings;
  onDone: (confirmed: MergeReach) => void;
  onClose: () => void;
}) {
  const [shown, setShown] = useState<{ reach: MergeReach; previous: MergeReach | null }>({
    reach: refused,
    previous: null,
  });
  const { reach, previous } = shown;
  const { pending, error } = { pending: mutation.isPending, error: mutation.error };
  const phone = useMedia(PHONE);
  const bodyId = useId();
  const make = act.kind === "make";
  const name = make ? act.name : act.to;
  const title = make ? `Make “${name}” a team tag?` : `Rename “${act.from}” to “${act.to}”?`;
  const items = reach.heaps.reduce((sum, reached) => sum + reached.items, 0);
  const before = previous && new Map(previous.heaps.map((reached) => [reached.tag_id, reached.items]));
  const left = previous
    ? previous.heaps.filter((old) => !reach.heaps.some((reached) => reached.tag_id === old.tag_id))
    : [];

  const cancel = () => {
    if (pending) return;
    mutation.reset();
    onClose();
  };
  const confirm = () => {
    mutation.mutateAsync({ ...variables, confirm: reach.token }).then(
      () => onDone(reach),
      (failure: unknown) => {
        if (!(failure instanceof ApiProblem)) return;
        if (failure.code === "tag_merge_unconfirmed" && failure.mergeReach) {
          setShown({ reach: failure.mergeReach, previous: reach });
        } else if (failure.code === "version_conflict") {
          onClose();
        }
      },
    );
  };

  const actLabel = pending ? (make ? "Making…" : "Renaming…") : make ? "Make team tag" : "Rename and merge";
  const body: ReactNode = (
    <div id={bodyId} className="flex min-w-0 flex-col gap-3 text-sm text-ink-soft">
      {previous && (
        <p role="status" className="rounded-lg bg-accent-soft px-3 py-2 text-accent-ink">
          This changed while it was open. Check the list again; the button confirms this one.
        </p>
      )}
      <ul aria-label="Heap tags it merges" className="max-h-[40dvh] divide-y divide-line overflow-y-auto">
        {reach.heaps.map((reached) => {
          const was = before?.get(reached.tag_id);
          return (
            <li
              key={reached.tag_id}
              className="flex flex-wrap items-center gap-x-2.5 gap-y-1 py-2 max-sm:gap-x-2 sm:min-h-10"
            >
              <span className="min-w-0 font-medium break-words text-ink sm:w-32 sm:shrink-0">{reached.heap.name}</span>
              <span className="flex min-w-0 items-center gap-2 max-sm:order-2 max-sm:basis-full sm:contents">
                <span className={chipClass}>{reached.tag_name}</span>
                <span aria-hidden="true" className="sm:hidden">
                  ·
                </span>
                <span>{plural(reached.items, "item")}</span>
                {was !== undefined && was !== reached.items && <span className={badgeClass}>was {was}</span>}
              </span>
              <span className="flex gap-1.5 max-sm:order-1 sm:ml-auto">
                {reached.archived && <span className={badgeClass}>archived</span>}
                {reached.heap.id === promotingHeapId && <span className={badgeClass}>this heap</span>}
                {before && !before.has(reached.tag_id) && <span className={badgeClass}>new</span>}
              </span>
            </li>
          );
        })}
      </ul>
      <p className="text-ink">
        {plural(items, "item")} in {plural(reach.heaps.length, "heap")} will carry the team tag.
      </p>
      {left.length > 0 && <p>Left the list: {listOf(left.map((old) => `${old.heap.name}'s “${old.tag_name}”`))}.</p>}
      <p>
        Description:{" "}
        {description ? (
          <>
            <span className="text-ink">{description}</span>
            {descriptionFrom && <span className="text-muted"> (from {descriptionFrom}'s tag)</span>}
          </>
        ) : (
          <span className="text-muted italic">No description</span>
        )}
      </p>
      <p className="text-[13px] text-muted">The items keep their history. This can't be undone.</p>
      <TagProblem error={error} typed={name} settings={settings} />
    </div>
  );

  if (phone) {
    return (
      <Sheet title={title} role="alertdialog" describedBy={bodyId} isOpen onClose={cancel}>
        <div className="flex flex-col gap-3 pb-1">
          {body}
          <div className="flex flex-col gap-2">
            <button
              type="button"
              autoFocus
              disabled={pending}
              onClick={confirm}
              className="min-h-13 rounded-xl bg-accent text-[15px] font-semibold text-stone-950 disabled:opacity-40"
            >
              {actLabel}
            </button>
            <button
              type="button"
              disabled={pending}
              onClick={cancel}
              className="min-h-13 rounded-xl border border-line text-[15px] text-ink disabled:opacity-40"
            >
              Cancel
            </button>
          </div>
        </div>
      </Sheet>
    );
  }
  return (
    <ModalOverlay
      isOpen
      onOpenChange={(open) => !open && cancel()}
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 px-4 pt-[14vh]"
    >
      <Modal className="w-full max-w-[480px] rounded-2xl bg-surface p-[22px] text-ink shadow-xl outline-none">
        <Dialog role="alertdialog" aria-describedby={bodyId} className="flex flex-col gap-3.5 outline-none">
          <Heading slot="title" className="text-base font-semibold">
            {title}
          </Heading>
          {body}
          <div className="flex flex-wrap items-center justify-end gap-2">
            <button type="button" disabled={pending} onClick={cancel} className={compactButtonClass.quiet}>
              Cancel
            </button>
            <button type="button" autoFocus disabled={pending} onClick={confirm} className={compactButtonClass.primary}>
              {actLabel}
            </button>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
