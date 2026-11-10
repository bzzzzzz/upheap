import { Link, Outlet } from "@tanstack/react-router";
import { localDatabase } from "#local-database";
import { useEffect, useRef, useState, type FormEvent } from "react";

import { ApiProblem, type Heap, type HeapRef } from "../api/client";
import { useHeap, useSetHeapArchived, useUpdateHeap } from "../api/queries";
import { BASE } from "../basePath";
import { HistoryDisclosure } from "../components/AuditHistory";
import { compactButtonClass } from "../components/buttons";
import { NameForm } from "../components/NameForm";
import { ProblemText } from "../components/ProblemText";
import { cardClass, cardTitleClass, inputClass, nameFormClasses } from "./settingsForm";

// A tab is a link, so each one has its own address and Back steps between them. The active one is underlined in the
// accent, as the router marks it.
const tabClass =
  "-mb-px flex min-h-11 items-center border-b-2 border-transparent px-3 text-sm font-medium text-ink-soft hover:text-ink data-[status=active]:border-accent data-[status=active]:text-ink";

/**
 * The heap's settings (ux.md, Heap settings): General (name, link name, archive), Tags, Parameters and Ranking, each a tab
 * with its own path under `/settings`. The tab's page renders below them.
 */
export function HeapSettingsLayout({ heap: heapRef }: { heap: HeapRef }) {
  const { data: heap } = useHeap(heapRef);
  return (
    <main>
      <h1 className="mt-6 mb-3 text-[22px] font-semibold tracking-[-0.015em] sm:mb-1">Settings</h1>
      {/* A phone's top bar already names the heap. */}
      {heap && <p className="mb-3 hidden text-sm text-muted sm:block">{heap.name}</p>}
      <nav aria-label="Settings" className="mb-6 flex gap-1 overflow-x-auto border-b border-line">
        <Link
          to="/teams/$team/heaps/$heap/settings"
          params={heapRef}
          activeOptions={{ exact: true }}
          className={tabClass}
        >
          General
        </Link>
        <Link to="/teams/$team/heaps/$heap/settings/tags" params={heapRef} className={tabClass}>
          Tags
        </Link>
        <Link to="/teams/$team/heaps/$heap/settings/parameters" params={heapRef} className={tabClass}>
          Parameters
        </Link>
        <Link to="/teams/$team/heaps/$heap/settings/ranking" params={heapRef} className={tabClass}>
          Ranking
        </Link>
      </nav>
      <Outlet />
    </main>
  );
}

/**
 * The heap's name, its link name (the last part of its path), its context and whether it's archived. Those who may
 * change them, as `allowed_actions` says, edit them in place; everyone else reads them. The local build has no
 * context card.
 */
export function HeapGeneralSettings({ heap: heapRef }: { heap: HeapRef }) {
  const heap = useHeap(heapRef);
  const update = useUpdateHeap(heapRef.team);
  const setArchived = useSetHeapArchived(heapRef.team);
  // What the link name field holds, for the new path under it while it's being changed.
  const [slugDraft, setSlugDraft] = useState("");

  // A failed refetch keeps the data, so what is on screen (and any draft in it) stays; the error shows only without it.
  if (!heap.data) {
    if (heap.isError) {
      return (
        <p role="alert" className="py-16 text-center text-overdue-ink">
          {heap.error.message}
        </p>
      );
    }
    return <p className="py-16 text-center text-muted">Loading…</p>;
  }

  const data = heap.data;
  const can = (action: (typeof data.allowed_actions)[number]) => data.allowed_actions.includes(action);
  const archived = data.archived_at !== null;
  const archiveAction = archived ? "unarchive" : "archive";
  const save = (patch: { name: string } | { slug: string }) => update.mutate({ heap: data, patch });
  const newSlug = can("rename") ? slugDraft.trim() : "";
  const slugChanging = newSlug !== "" && newSlug !== data.slug;

  return (
    <div className="flex max-w-[640px] flex-col gap-4 sm:gap-6">
      <section aria-labelledby="heap-details" className={cardClass}>
        <h2 id="heap-details" className={cardTitleClass}>
          Details
        </h2>
        <dl className="mt-4 flex flex-col gap-[18px]">
          <div>
            <dt className="mb-1.5 text-sm font-medium text-ink">Name</dt>
            <dd className="flex sm:max-w-[360px]">
              {can("rename") ? (
                <NameForm
                  label="Heap name"
                  value={data.name}
                  pending={update.isPending}
                  error={update.variables && "name" in update.variables.patch ? update.error : null}
                  onSave={(name) => save({ name })}
                  classes={nameFormClasses}
                />
              ) : (
                <span className="text-sm text-ink-soft">{data.name}</span>
              )}
            </dd>
          </div>
          <div>
            <dt className="mb-1.5 text-sm font-medium text-ink">Link name</dt>
            <dd className="flex min-w-0 items-start gap-1.5 text-sm text-muted">
              {/* Level with the input, also when Save and Cancel wrap under it on a phone. */}
              <span aria-hidden="true" className="flex min-h-[38px] items-center pointer-coarse:min-h-11">
                /heaps/
              </span>
              {can("rename") ? (
                <NameForm
                  label="Heap link name"
                  value={data.slug}
                  maxLength={39}
                  pending={update.isPending}
                  error={update.variables && "slug" in update.variables.patch ? update.error : null}
                  onSave={(slug) => save({ slug: slug.trim() })}
                  onDraftChange={setSlugDraft}
                  classes={nameFormClasses}
                />
              ) : (
                <span className="flex min-h-[38px] items-center text-ink-soft pointer-coarse:min-h-11">
                  {data.slug}
                </span>
              )}
            </dd>
            {/* Only those who may change the link name are warned about changing it. */}
            {can("rename") && (
              <p className="mt-2 text-xs/normal text-muted max-sm:text-[13px]">
                {slugChanging && (
                  <>
                    New link:{" "}
                    <span className="font-mono break-all text-ink-soft">
                      {BASE}/teams/{heapRef.team}/heaps/{newSlug}
                    </span>
                    .{" "}
                  </>
                )}
                Changing it changes the heap&apos;s links; old links stop working.
              </p>
            )}
          </div>
        </dl>
        {can("view_history") && <HistoryDisclosure scope={{ team: heapRef.team }} entityId={data.id} />}
      </section>

      {!localDatabase && <HeapContextCard team={heapRef.team} heap={data} canEdit={can("edit_context")} />}

      {can(archiveAction) && (
        <section aria-labelledby="heap-archive" className={cardClass}>
          <h2 id="heap-archive" className={cardTitleClass}>
            {archived ? "Archived" : "Archive"}
          </h2>
          <p className="mt-2 text-sm/normal text-ink-soft">
            {archived
              ? "This heap is read-only. Unarchive it to add and change items again."
              : "An archived heap keeps its items but becomes read-only. It can be unarchived at any time."}
          </p>
          <div className="mt-4 flex flex-wrap items-center gap-3">
            <button
              type="button"
              disabled={setArchived.isPending}
              onClick={() => setArchived.mutate({ heap: data, archived: !archived })}
              className={`${archived ? compactButtonClass.bordered : compactButtonClass.borderedDanger} max-sm:w-full`}
            >
              {archived ? "Unarchive heap" : "Archive heap"}
            </button>
            <ProblemText error={setArchived.error} className="text-overdue-ink" />
          </div>
        </section>
      )}
    </div>
  );
}

const CONTEXT_MAX_CHARS = 20_000;

/**
 * The heap's context, as written: read in place, with an Edit button that turns the card into a form (ux.md, Heap
 * settings). The draft is local state from the press of Edit, so a refetch of the heap doesn't touch it; a failed
 * save keeps the form and the draft open. Escape does nothing, so a long draft is never lost to a stray key.
 */
function HeapContextCard({ team, heap, canEdit }: { team: string; heap: Heap; canEdit: boolean }) {
  // Its own mutation, so resetting its error leaves the name forms' saves alone.
  const update = useUpdateHeap(team);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  // The context when Edit was pressed, to tell whether a conflict changed the context or only the rest of the heap.
  const [startedFrom, setStartedFrom] = useState<string | null>(null);
  // Set when a save finds the heap changed meanwhile; the heap is refetched, so `heap.context` is the saved one.
  const [conflicted, setConflicted] = useState(false);
  const textarea = useRef<HTMLTextAreaElement>(null);
  const editButton = useRef<HTMLButtonElement>(null);
  // Set when the form closes, so focus goes to the Edit button once it is back.
  const refocus = useRef(false);
  useEffect(() => {
    if (editing) {
      const field = textarea.current;
      field?.focus();
      field?.setSelectionRange(field.value.length, field.value.length);
    } else if (refocus.current) {
      refocus.current = false;
      editButton.current?.focus();
    }
  }, [editing]);

  const close = () => {
    refocus.current = true;
    setEditing(false);
    setConflicted(false);
    update.reset();
  };
  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    update.mutate(
      { heap, patch: { context: draft.trim() === "" ? null : draft } },
      {
        onSuccess: close,
        onError: (e) => setConflicted(e instanceof ApiProblem && e.code === "version_conflict"),
      },
    );
  };
  const error = update.error;
  const conflictShown = conflicted && error instanceof ApiProblem && error.code === "version_conflict";
  // A rename or a weights save moves the heap's version too, and leaves nothing of theirs to show.
  const contextChanged = conflictShown && (heap.context ?? null) !== startedFrom;

  return (
    <section aria-labelledby="heap-context" className={cardClass}>
      <h2 id="heap-context" className={cardTitleClass}>
        Context
      </h2>
      {editing ? (
        <form onSubmit={submit} className="mt-3 flex flex-wrap items-center gap-2">
          {contextChanged && (
            <div className="w-full">
              <h3 className="text-sm/normal font-medium text-ink">Saved meanwhile</h3>
              <p className="mt-1 rounded-md border border-line bg-canvas p-3 text-sm/normal break-words whitespace-pre-wrap text-ink-soft">
                {heap.context ?? "No context."}
              </p>
            </div>
          )}
          <textarea
            ref={textarea}
            aria-label="Heap context"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            maxLength={CONTEXT_MAX_CHARS}
            className={`${inputClass} field-sizing-content min-h-48 w-full`}
          />
          <button type="submit" disabled={update.isPending} className={`${compactButtonClass.primary} max-sm:flex-1`}>
            Save
          </button>
          <button type="button" onClick={close} className={`${compactButtonClass.quiet} max-sm:flex-1`}>
            Cancel
          </button>
          {contextChanged ? (
            <p role="alert" className="basis-full text-sm text-overdue-ink">
              Someone else changed the context while you were editing. Their version is above; Save again to replace it
              with yours, or Cancel to keep theirs.
            </p>
          ) : conflictShown ? (
            <p role="alert" className="basis-full text-sm text-overdue-ink">
              The heap&apos;s settings changed while you were editing, but not its context. Save again to save yours.
            </p>
          ) : (
            <ProblemText error={error} className="text-overdue-ink" />
          )}
        </form>
      ) : (
        <>
          {heap.context ? (
            <p className="mt-2 text-sm/normal break-words whitespace-pre-wrap text-ink-soft">{heap.context}</p>
          ) : (
            <p className="mt-2 text-sm/normal text-muted">
              What the project is, where its repository and docs are, where its work is reviewed and what finishing a
              piece means.
            </p>
          )}
          {canEdit && (
            <button
              ref={editButton}
              type="button"
              onClick={() => {
                update.reset();
                setStartedFrom(heap.context ?? null);
                setDraft(heap.context ?? "");
                setEditing(true);
              }}
              className={`${compactButtonClass.bordered} mt-4 max-sm:w-full`}
            >
              Edit
            </button>
          )}
        </>
      )}
    </section>
  );
}
