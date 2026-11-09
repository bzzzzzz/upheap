import { Link, Outlet } from "@tanstack/react-router";
import { useState } from "react";

import type { HeapRef } from "../api/client";
import { useHeap, useSetHeapArchived, useUpdateHeap } from "../api/queries";
import { BASE } from "../basePath";
import { HistoryDisclosure } from "../components/AuditHistory";
import { compactButtonClass } from "../components/buttons";
import { NameForm } from "../components/NameForm";
import { ProblemText } from "../components/ProblemText";
import { cardClass, cardTitleClass, nameFormClasses } from "./settingsForm";

// A tab is a link, so each one has its own address and Back steps between them. The active one is underlined in the
// accent, as the router marks it.
const tabClass =
  "-mb-px flex min-h-11 items-center border-b-2 border-transparent px-3 text-sm font-medium text-ink-soft hover:text-ink data-[status=active]:border-accent data-[status=active]:text-ink";

/**
 * The heap's settings (app-ux.md, Heap settings): General (name, link name, archive), Tags and Parameters, each a tab
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
      </nav>
      <Outlet />
    </main>
  );
}

/**
 * The heap's name, its link name (the last part of its path) and whether it's archived. Those who may change them,
 * as `allowed_actions` says, edit them in place; everyone else reads them.
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
