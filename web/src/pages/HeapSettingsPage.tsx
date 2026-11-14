import { Link, Outlet } from "@tanstack/react-router";
import { useEffect, useId, useRef, useState, type FormEvent } from "react";
import { Switch, ToggleButton, ToggleButtonGroup } from "react-aria-components";

import { ApiProblem, type Heap, type HeapRef } from "../api/client";
import { useHeap, useSetHeapPriority, useUpdateHeap } from "../api/queries";
import { BASE } from "../basePath";
import { useArchiveHeap } from "../components/archiveHeap";
import { HistoryDisclosure } from "../components/AuditHistory";
import { compactButtonClass } from "../components/buttons";
import { choiceButtonClass } from "../components/ChoiceButtons";
import { Markdown } from "../components/Markdown";
import { NameForm } from "../components/NameForm";
import { ProblemText } from "../components/ProblemText";
import { cardClass, cardTitleClass, inputClass, nameFormClasses } from "./settingsForm";

// A tab is a link, so each one has its own address and Back steps between them. The active one is underlined in the
// accent, as the router marks it.
// On a phone the five tabs are a little tighter, so they fit a 375 px screen and none hides off its edge.
const tabClass =
  "-mb-px flex min-h-11 shrink-0 items-center border-b-2 border-transparent px-3 text-sm font-medium max-sm:px-2 max-sm:text-[13px] text-ink-soft hover:text-ink data-[status=active]:border-accent data-[status=active]:text-ink";

/**
 * The heap's settings (ux.md, Heap settings): General (name, link name, agents, archive), Context, Tags, Parameters and
 * Ranking, each a tab with its own path under `/settings`. The tab's page renders below them.
 */
export function HeapSettingsLayout({ heap: heapRef }: { heap: HeapRef }) {
  const { data: heap } = useHeap(heapRef);
  return (
    <main>
      <h1 className="mt-6 mb-3 text-[22px] font-semibold tracking-[-0.015em] sm:mb-1">Settings</h1>
      {/* A phone's top bar already names the heap. */}
      {heap && <p className="mb-3 hidden text-sm text-muted sm:block">{heap.name}</p>}
      <nav aria-label="Settings" className="mb-6 flex gap-1 overflow-x-auto border-b border-line max-sm:gap-0">
        <Link
          to="/teams/$team/heaps/$heap/settings"
          params={heapRef}
          activeOptions={{ exact: true }}
          className={tabClass}
        >
          General
        </Link>
        <Link to="/teams/$team/heaps/$heap/settings/context" params={heapRef} className={tabClass}>
          Context
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
 * The heap's name, its link name (the last part of its path), its agents and whether it's archived. Those who may
 * change them, as `allowed_actions` says, edit them in place; everyone else reads them.
 */
export function HeapGeneralSettings({ heap: heapRef }: { heap: HeapRef }) {
  const heap = useHeap(heapRef);
  const update = useUpdateHeap(heapRef.team);
  const { archive, unarchive, mutation: setArchived } = useArchiveHeap(heapRef.team);
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

      <AgentsCard team={heapRef.team} heap={data} canSet={can("set_agents_prepare")} />

      <PriorityCard team={heapRef.team} heap={data} canSet={can("set_priority")} />

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
              onClick={() => (archived ? unarchive(data) : archive(data))}
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

/**
 * Whether agents prepare the notes added to the heap (ux.md, Heap settings): a switch that saves as it is turned, for
 * those who may set it, as `allowed_actions` says; everyone else, and everyone in an archived heap, reads the value.
 * Its own mutation, so its error doesn't mix with the name forms'.
 */
function AgentsCard({ team, heap, canSet }: { team: string; heap: Heap; canSet: boolean }) {
  const update = useUpdateHeap(team);
  // The value being saved shows at once, and the saved one once the save is over.
  const on = update.isPending ? (update.variables.patch.agents_prepare ?? heap.agents_prepare) : heap.agents_prepare;
  return (
    <section aria-labelledby="heap-agents" className={cardClass}>
      <h2 id="heap-agents" className={cardTitleClass}>
        Agents
      </h2>
      {canSet ? (
        <Switch
          isSelected={on}
          // Read-only while it saves, so a second turn can't send the old version; unlike disabled, it keeps focus.
          isReadOnly={update.isPending}
          onChange={(agentsPrepare) => update.mutate({ heap, patch: { agents_prepare: agentsPrepare } })}
          className="group mt-3 flex min-h-9 items-center gap-3 text-sm font-medium outline-none pointer-coarse:min-h-11"
        >
          <span
            aria-hidden="true"
            className="flex h-6 w-10 shrink-0 items-center rounded-full bg-line-strong px-0.5 transition-colors group-data-[focus-visible]:outline-2 group-data-[focus-visible]:outline-offset-2 group-data-[focus-visible]:outline-accent group-data-[selected]:bg-accent motion-reduce:transition-none"
          >
            <span className="size-5 rounded-full bg-surface shadow-sm transition-transform group-data-[selected]:translate-x-4 motion-reduce:transition-none" />
          </span>
          Agents prepare new notes
        </Switch>
      ) : (
        <p className="mt-3 text-sm font-medium">Agents prepare new notes: {on ? "on" : "off"}</p>
      )}
      <p className="mt-2 text-sm/normal text-ink-soft">
        Each note added here is sent to an agent to fill in its context, or to make it a defined task when it's small
        and clear, then comes back to you.
      </p>
      <ProblemText error={update.error} className="mt-2 text-overdue-ink" />
    </section>
  );
}

const PRIORITIES = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] as const;

/**
 * The caller's own priority for the heap, 1 to 10 (ux.md, Heap settings): a row of numbers that saves as one is
 * pressed, as the Agents switch does. Five a row on a phone, where ten don't fit. Not the Ranking tab's factor weights.
 */
function PriorityCard({ team, heap, canSet }: { team: string; heap: Heap; canSet: boolean }) {
  const setPriority = useSetHeapPriority(team);
  const hintId = useId();
  // The value being saved shows at once, and the saved one once the save is over.
  const priority = setPriority.isPending ? setPriority.variables.priority : heap.priority;
  return (
    <section aria-labelledby="heap-priority" className={cardClass}>
      <h2 id="heap-priority" className={cardTitleClass}>
        Your priority
      </h2>
      {canSet ? (
        <ToggleButtonGroup
          aria-labelledby="heap-priority"
          selectionMode="single"
          disallowEmptySelection
          selectedKeys={[String(priority)]}
          onSelectionChange={(keys) => {
            const [picked] = [...keys];
            if (picked !== undefined && !setPriority.isPending) {
              setPriority.mutate({ heap, priority: Number(picked) });
            }
          }}
          className="mt-3 grid grid-cols-5 gap-1 sm:flex sm:flex-wrap"
        >
          {PRIORITIES.map((value) => (
            // On each button: the group passes no `aria-describedby` on, so a screen reader would never hear the hint.
            <ToggleButton
              key={value}
              id={String(value)}
              aria-describedby={hintId}
              className={`${choiceButtonClass} rounded-lg sm:w-10`}
            >
              {value}
            </ToggleButton>
          ))}
        </ToggleButtonGroup>
      ) : (
        <p className="mt-3 text-sm font-medium">Your priority: {priority}</p>
      )}
      <p id={hintId} className="mt-2 text-sm/normal text-ink-soft">
        How this heap ranks against your other heaps in Waiting for me and the agents&apos; queue: its items&apos;
        scores are multiplied by this. 1 to 10. Only you see it.
      </p>
      <ProblemText error={setPriority.error} className="mt-2 text-overdue-ink" />
    </section>
  );
}

const CONTEXT_MAX_CHARS = 20_000;

/**
 * The Context tab (ux.md, Heap settings): the heap's context card, with the same loading and error handling as General.
 */
export function HeapContextSettings({ heap: heapRef }: { heap: HeapRef }) {
  const heap = useHeap(heapRef);
  // A failed refetch keeps the data; the error shows only without it.
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
  return (
    <div className="flex max-w-[640px] flex-col gap-4 sm:gap-6">
      <HeapContextCard team={heapRef.team} heap={data} canEdit={data.allowed_actions.includes("edit_context")} />
    </div>
  );
}

/**
 * The heap's context, as Markdown text: read in place, with an Edit button that turns the card into a form (ux.md, Heap
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
              <div className="mt-1 rounded-md border border-line bg-canvas p-3">
                {heap.context ? (
                  <Markdown text={heap.context} className="text-ink-soft" />
                ) : (
                  <p className="text-sm/normal text-ink-soft">No context.</p>
                )}
              </div>
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
            <div className="mt-2">
              <Markdown text={heap.context} className="text-ink-soft" />
            </div>
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
