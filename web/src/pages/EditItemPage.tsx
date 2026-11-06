import { Link, Navigate, useCanGoBack, useNavigate, useRouter } from "@tanstack/react-router";
import { useId, useState, type FormEvent, type KeyboardEvent } from "react";

import {
  ApiProblem,
  type Item,
  type ItemPatch,
  type ItemRef,
  type ItemState,
  type ItemTag,
  type Size,
} from "../api/client";
import { useBlockerCandidates, useItem, useUpdateItem } from "../api/queries";
import { BlockerSelect } from "../components/BlockerSelect";
import { blockerCandidatesFor } from "../components/blockerCandidates";
import { isClosed, STATE_LABELS, STATES } from "../components/labels";
import { SizeSelect } from "../components/SizeSelect";
import { TagPicker } from "../components/TagPicker";

/** Messages for the errors a save can bring back, by code. */
const PROBLEM_MESSAGES: Record<string, string> = {
  version_conflict: "Someone else changed this item while you were editing. The form now shows their version.",
  blocking_cycle: "That item already waits for this one, so it can't block it.",
  blocker_closed: "That item is closed, so nothing can wait for it.",
  blocker_not_in_heap: "That item isn't in this heap any more.",
  item_closed: "This item is closed, so only its state can change.",
  reopen_window_expired: "This item was closed more than a week ago, so it can't be reopened.",
};

/** Every field of one item in a single form. Saving sends one patch with only the changed fields. */
export function EditItemPage({ heapId, itemId }: { heapId: string; itemId: string }) {
  const item = useItem(itemId);
  const candidates = useBlockerCandidates(heapId);
  const update = useUpdateItem(heapId);
  const leave = useLeave(heapId);

  if (item.isPending) {
    return <p className="py-16 text-center text-stone-500">Loading…</p>;
  }
  if (item.isError) {
    const missing = item.error instanceof ApiProblem && item.error.status === 404;
    return (
      <div className="py-16 text-center">
        <p role="alert" className="text-red-700 dark:text-red-400">
          {missing ? "This item doesn't exist, or it's in a heap you can't see." : item.error.message}
        </p>
        <Link to="/heaps/$heapId" params={{ heapId }} className="mt-4 inline-block text-sm text-stone-500 underline">
          Back to the list
        </Link>
      </div>
    );
  }
  if (item.data.heap_id !== heapId) {
    return <Navigate to="/heaps/$heapId/items/$itemId" params={{ heapId: item.data.heap_id, itemId }} replace />;
  }

  const current = item.data;
  // After a save the item's new state decides which list to go back to, so a reopened item lands in the open list.
  const save = (patch: ItemPatch) =>
    update.mutate({ item: current, patch }, { onSuccess: (saved) => leave(isClosed(saved.state)) });

  return (
    <main>
      <h1 className="mt-6 mb-4 text-xl font-semibold tracking-tight">Edit item</h1>
      {/* A new version (after a conflict, say) starts the form again from the server's copy. */}
      <EditItemForm
        key={`${item.data.id}:${item.data.version}`}
        item={item.data}
        blockerCandidates={blockerCandidatesFor(candidates.data ?? [])(itemId)}
        saving={update.isPending}
        error={update.error}
        onSave={save}
        onCancel={() => leave(isClosed(current.state))}
      />
    </main>
  );
}

/** Goes back to where the user came from, or to the list the item belongs in when there is no history. */
function useLeave(heapId: string) {
  const router = useRouter();
  const navigate = useNavigate();
  const canGoBack = useCanGoBack();
  return (closed: boolean) => {
    if (canGoBack) {
      router.history.back();
    } else {
      void navigate({ to: closed ? "/heaps/$heapId/closed" : "/heaps/$heapId", params: { heapId } });
    }
  };
}

type Draft = {
  title: string;
  description: string;
  effort: Size | null;
  impact: Size | null;
  dueDate: string;
  dueFirm: boolean;
  tags: ItemTag[];
  state: ItemState;
  blockedBy: string | null;
};

function draftOf(item: Item): Draft {
  return {
    title: item.title,
    description: item.description ?? "",
    effort: item.effort ?? null,
    impact: item.impact ?? null,
    dueDate: item.due?.date ?? "",
    dueFirm: item.due?.firm ?? false,
    tags: item.tags,
    state: item.state,
    blockedBy: item.blocked_by?.id ?? null,
  };
}

/**
 * The changes from `item` to `draft`. Only the state and blocker change when the item ends up closed: the server
 * refuses field edits on an item that stays closed (`item_closed`), so the form disables the fields rather than
 * send a save that is bound to fail. The server stays the authority; this only keeps the form honest.
 */
function patchFor(item: Item, draft: Draft): ItemPatch {
  const patch: ItemPatch = {};
  if (draft.state !== item.state) patch.state = draft.state;
  if (draft.state === "blocked" && draft.blockedBy !== (item.blocked_by?.id ?? null)) {
    patch.blocked_by_item_id = draft.blockedBy;
  }
  if (isClosed(draft.state)) return patch;

  if (draft.title !== item.title) patch.title = draft.title;
  const description = draft.description.trim() === "" ? null : draft.description;
  if (description !== (item.description ?? null)) patch.description = description;
  if (draft.effort !== (item.effort ?? null)) patch.effort = draft.effort;
  if (draft.impact !== (item.impact ?? null)) patch.impact = draft.impact;
  const due = draft.dueDate ? { date: draft.dueDate, firm: draft.dueFirm } : null;
  if (due?.date !== item.due?.date || due?.firm !== item.due?.firm) patch.due = due;
  const tagIds = draft.tags.map((tag) => tag.id);
  const sameTags = tagIds.length === item.tags.length && item.tags.every((tag) => tagIds.includes(tag.id));
  if (!sameTags) patch.tag_ids = tagIds;
  return patch;
}

function EditItemForm({
  item,
  blockerCandidates,
  saving,
  error,
  onSave,
  onCancel,
}: {
  item: Item;
  blockerCandidates: ItemRef[];
  saving: boolean;
  error: Error | null;
  onSave: (patch: ItemPatch) => void;
  onCancel: () => void;
}) {
  const [draft, setDraft] = useState(() => draftOf(item));
  const set = <K extends keyof Draft>(key: K, value: Draft[K]) => setDraft((d) => ({ ...d, [key]: value }));

  const closed = isClosed(item.state);
  const canEdit = item.allowed_actions.includes("edit");
  const canReopen = item.allowed_actions.includes("reopen");
  const canChangeState = item.allowed_actions.includes("change_state") || canReopen;
  // A closed item can be reopened and edited in the same save.
  const stateOptions = closed
    ? canReopen
      ? [item.state, ...STATES.filter((s) => !isClosed(s))]
      : [item.state]
    : STATES;
  const fieldsEditable = (canEdit || canReopen) && !isClosed(draft.state);
  const readOnly = !canEdit && !canChangeState;
  const patch = patchFor(item, draft);
  const titleMissing = fieldsEditable && draft.title.trim() === "";
  // Disabled fields show the saved values, not edits that this save won't send. The draft keeps the edits in case
  // the state goes back to an open one.
  const shown = fieldsEditable ? draft : { ...draftOf(item), state: draft.state, blockedBy: draft.blockedBy };
  const titleHintId = useId();

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (titleMissing) return;
    if (Object.keys(patch).length === 0) {
      onCancel();
    } else {
      onSave(patch);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLFormElement>) => {
    // The tag picker handles Escape and plain Enter itself while it has text, and marks them handled.
    if (event.defaultPrevented) return;
    if (event.key === "Escape") {
      event.preventDefault();
      onCancel();
    } else if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      event.currentTarget.requestSubmit();
    }
  };

  const inputClass =
    "rounded border border-stone-300 bg-white px-2 py-1 text-stone-900 disabled:opacity-60 dark:border-stone-700 dark:bg-stone-900 dark:text-stone-100";

  return (
    <form
      onSubmit={submit}
      onKeyDown={onKeyDown}
      className="flex flex-col gap-4 rounded-lg border border-stone-200 bg-white p-4 shadow-sm dark:border-stone-800 dark:bg-stone-900"
    >
      {readOnly && <p className="text-sm text-stone-500">You can't change this item.</p>}
      {closed && item.closed_label && (
        <p className="text-xs text-stone-500">
          {STATE_LABELS[item.state]} · {item.closed_label}
        </p>
      )}

      <label className="flex flex-col gap-1 text-xs text-stone-500">
        Title
        <input
          autoFocus={fieldsEditable}
          value={shown.title}
          disabled={!fieldsEditable}
          onChange={(e) => set("title", e.target.value)}
          aria-invalid={titleMissing}
          aria-describedby={titleMissing ? titleHintId : undefined}
          className={`${inputClass} text-base font-medium`}
        />
        {titleMissing && (
          <span id={titleHintId} className="text-red-700 dark:text-red-400">
            An item needs a title.
          </span>
        )}
      </label>

      <label className="flex flex-col gap-1 text-xs text-stone-500">
        Description
        <textarea
          value={shown.description}
          disabled={!fieldsEditable}
          onChange={(e) => set("description", e.target.value)}
          rows={5}
          className={`${inputClass} text-sm`}
        />
      </label>

      <div className="flex flex-wrap items-center gap-4">
        <SizeSelect label="Effort" value={shown.effort} disabled={!fieldsEditable} onChange={(v) => set("effort", v)} />
        <SizeSelect label="Impact" value={shown.impact} disabled={!fieldsEditable} onChange={(v) => set("impact", v)} />
        <label className="inline-flex items-center gap-1 text-xs text-stone-500">
          Due
          <input
            type="date"
            value={shown.dueDate}
            disabled={!fieldsEditable}
            onChange={(e) => set("dueDate", e.target.value)}
            className={`${inputClass} py-0.5`}
          />
        </label>
        <label className="inline-flex items-center gap-1 text-xs text-stone-500">
          <input
            type="checkbox"
            checked={shown.dueFirm}
            disabled={!fieldsEditable || !shown.dueDate}
            onChange={(e) => set("dueFirm", e.target.checked)}
          />
          Exact deadline
        </label>
      </div>

      <TagPicker
        heapId={item.heap_id}
        value={shown.tags}
        onChange={(tags) => set("tags", tags)}
        disabled={!fieldsEditable}
      />

      <div className="flex flex-wrap items-center gap-4">
        <label className="inline-flex items-center gap-1 text-xs text-stone-500">
          State
          <select
            value={draft.state}
            disabled={!canChangeState}
            onChange={(e) => set("state", e.target.value as ItemState)}
            className={`${inputClass} py-0.5 text-sm`}
          >
            {stateOptions.map((state) => (
              <option key={state} value={state}>
                {STATE_LABELS[state]}
              </option>
            ))}
          </select>
        </label>
        {draft.state === "blocked" && (
          <BlockerSelect
            value={
              draft.blockedBy === null
                ? null
                : (blockerCandidates.find((c) => c.id === draft.blockedBy) ??
                  (item.blocked_by?.id === draft.blockedBy ? item.blocked_by : null))
            }
            candidates={blockerCandidates}
            disabled={!canChangeState}
            onChange={(id) => set("blockedBy", id)}
          />
        )}
      </div>
      {!closed && isClosed(draft.state) && (
        <p className="text-xs text-stone-500">A closed item can't be edited, so saving now only closes it.</p>
      )}
      {closed && canReopen && isClosed(draft.state) && (
        <p className="text-xs text-stone-500">A closed item can't be edited. Pick another state to reopen it.</p>
      )}

      {error && (
        <p role="alert" className="text-sm text-red-700 dark:text-red-400">
          {(error instanceof ApiProblem && PROBLEM_MESSAGES[error.code]) || error.message}
        </p>
      )}

      <div className="flex items-center gap-3 border-t border-stone-100 pt-4 dark:border-stone-800">
        {!readOnly && (
          <button
            type="submit"
            disabled={saving || titleMissing}
            className="rounded-md bg-stone-900 px-3 py-1 text-sm font-medium text-white disabled:opacity-40 dark:bg-stone-100 dark:text-stone-900"
          >
            {saving ? "Saving…" : "Save"}
          </button>
        )}
        <button
          type="button"
          onClick={onCancel}
          className="text-sm text-stone-500 hover:text-stone-900 dark:hover:text-stone-100"
        >
          {readOnly ? "Back" : "Cancel"}
        </button>
        {!readOnly && <span className="ml-auto text-xs text-stone-400">⌘↵ to save · Esc to cancel</span>}
      </div>
    </form>
  );
}
