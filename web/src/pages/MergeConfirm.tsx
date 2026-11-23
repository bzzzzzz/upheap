// The confirm before a team tag merges heap tags (ux.md, Tags): a dialog on a wide screen, a sheet on a phone.
import type { ReactNode } from "react";
import { Dialog, Heading, Modal, ModalOverlay } from "react-aria-components";

import { ApiProblem } from "../api/client";
import type { MergeReach } from "../api/schema";
import { compactButtonClass } from "../components/buttons";
import { PHONE, useMedia } from "../components/media";
import { Sheet } from "../components/Sheet";
import { listOf, plural } from "../components/words";
import { chipClass, TagProblem, type TagSettings } from "./TagSettings";
import { useMergeReachConfirm, type Send } from "./useMergeReachConfirm";
import { Saving } from "../components/Saving";

/** What the write does: makes a team tag, or renames one into a name heap tags have. */
export type MergeAct = { kind: "make"; name: string } | { kind: "rename"; from: string; to: string };

/** The end of the toast after a confirmed merge: how many items in how many heaps carry the team tag. */
export const carriedBy = (reach: MergeReach) =>
  `${plural(
    reach.heaps.reduce((sum, reached) => sum + reached.items, 0),
    "item",
  )} in ${plural(reach.heaps.length, "heap")} carry it.`;

/** The toast after a confirmed merge made `name` a team tag. */
export const madeTeamTag = (name: string, reach: MergeReach) => `“${name}” is a team tag now: ${carriedBy(reach)}`;

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
  const flow = useMergeReachConfirm({
    mutation,
    variables,
    refused,
    partsOf: (r: MergeReach) =>
      r.heaps.map((reached) => ({
        id: reached.tag_id,
        items: reached.items,
        label: `${reached.heap.name}'s “${reached.tag_name}”`,
      })),
    reachOf: (failure) =>
      failure instanceof ApiProblem && failure.code === "tag_merge_unconfirmed" ? failure.mergeReach : undefined,
    // A version conflict belongs to the form behind the dialog, which keeps the error.
    onFailure: (failure) => {
      if (failure instanceof ApiProblem && failure.code === "version_conflict") onClose();
    },
    onDone,
    onClose,
  });
  const { reach, pending, error, bodyId, band } = flow;
  const make = act.kind === "make";
  const name = make ? act.name : act.to;
  const title = make ? `Make “${name}” a team tag?` : `Rename “${act.from}” to “${act.to}”?`;
  const items = reach.heaps.reduce((sum, reached) => sum + reached.items, 0);

  const actLabel = make ? "Make team tag" : "Rename and merge";
  const body: ReactNode = (
    <div id={bodyId} className="flex min-w-0 flex-col gap-3 text-sm text-ink-soft">
      {flow.changed && (
        <p ref={band} tabIndex={-1} className="rounded-lg bg-accent-soft px-3 py-2 text-accent-ink outline-none">
          This changed while it was open. Check the list again; the button confirms this one.
        </p>
      )}
      <ul aria-label="Heap tags it merges" className="max-h-[40dvh] divide-y divide-line overflow-y-auto">
        {reach.heaps.map((reached) => {
          const was = flow.was(reached.tag_id, reached.items);
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
                {was !== undefined && <span className={badgeClass}>was {was}</span>}
              </span>
              <span className="flex gap-1.5 max-sm:order-1 sm:ml-auto">
                {reached.archived && <span className={badgeClass}>archived</span>}
                {reached.heap.id === promotingHeapId && <span className={badgeClass}>this heap</span>}
                {flow.isNew(reached.tag_id) && <span className={badgeClass}>new</span>}
              </span>
            </li>
          );
        })}
      </ul>
      <p className="text-ink">
        {plural(items, "item")} in {plural(reach.heaps.length, "heap")} will carry the team tag.
      </p>
      {flow.left.length > 0 && <p>Left the list: {listOf(flow.left)}.</p>}
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

  return (
    <MergeDialog
      title={title}
      body={body}
      bodyId={bodyId}
      actLabel={actLabel}
      pending={pending}
      onConfirm={flow.confirm}
      onCancel={flow.cancel}
    />
  );
}

/**
 * The shell of a merge's confirm: a dialog on a wide screen, a sheet on a phone, with the body, Cancel and the button
 * that goes ahead. `bodyId` is the id of the body's element, which the dialog is described by.
 */
export function MergeDialog({
  title,
  body,
  bodyId,
  actLabel,
  pending,
  onConfirm,
  onCancel,
}: {
  title: string;
  body: ReactNode;
  bodyId: string;
  actLabel: string;
  pending: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const phone = useMedia(PHONE);
  if (phone) {
    return (
      <Sheet title={title} role="alertdialog" describedBy={bodyId} isOpen onClose={onCancel}>
        <div className="flex flex-col gap-3 pb-1">
          {body}
          <div className="flex flex-col gap-2">
            <button
              type="button"
              autoFocus
              disabled={pending}
              aria-busy={pending || undefined}
              onClick={onConfirm}
              className="flex min-h-13 items-center justify-center gap-2 rounded-xl bg-accent text-[15px] font-semibold text-stone-950 disabled:not-aria-busy:opacity-40"
            >
              <Saving pending={pending} />
              {actLabel}
            </button>
            <button
              type="button"
              disabled={pending}
              onClick={onCancel}
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
      onOpenChange={(open) => !open && onCancel()}
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 px-4 pt-[14vh]"
    >
      <Modal className="w-full max-w-[480px] rounded-2xl bg-surface p-[22px] text-ink shadow-xl outline-none">
        <Dialog role="alertdialog" aria-describedby={bodyId} className="flex flex-col gap-3.5 outline-none">
          <Heading slot="title" className="text-base font-semibold">
            {title}
          </Heading>
          {body}
          <div className="flex flex-wrap items-center justify-end gap-2">
            <button type="button" autoFocus disabled={pending} onClick={onCancel} className={compactButtonClass.quiet}>
              Cancel
            </button>
            <button
              type="button"
              disabled={pending}
              aria-busy={pending || undefined}
              onClick={onConfirm}
              className={compactButtonClass.primary}
            >
              <Saving pending={pending} />
              {actLabel}
            </button>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
