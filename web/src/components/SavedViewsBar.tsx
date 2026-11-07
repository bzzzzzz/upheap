import { Link } from "@tanstack/react-router";
import { useEffect, useState, type FormEvent } from "react";

import type { SavedView } from "../api/client";
import { useArchiveSavedView, useCreateSavedView, useSavedViews, useUpdateSavedView } from "../api/queries";
import { HistoryDisclosure } from "./AuditHistory";
import { searchFor, showSameList, toViewInput, type ListSearch } from "./listView";
import { inputClass, primaryButtonClass, quietButtonClass } from "./NameForm";
import { ProblemText } from "./ProblemText";
import { useConfirmation } from "./useConfirmation";

type Naming = "new" | "rename" | null;

/**
 * The heap's saved views, as links, and what can be done with the open one. Opening a view puts its settings and its
 * ID in the URL. Changing the list afterwards keeps the ID, so the bar can offer to save the change into the view or
 * as a new one. Every member sees the same views.
 */
export function SavedViewsBar({
  heapId,
  view,
  onViewChange,
}: {
  heapId: string;
  view: ListSearch;
  onViewChange: (view: ListSearch) => void;
}) {
  const views = useSavedViews(heapId);
  const [naming, setNaming] = useState<Naming>(null);
  // An ID that names no view in use (archived since, say) leaves the URL, so equal lists keep equal URLs. Not while
  // the list is refetching: a view just created may not be in it yet.
  const deadId =
    view.view !== undefined &&
    views.data !== undefined &&
    !views.isFetching &&
    !views.data.views.some((saved) => saved.id === view.view);
  useEffect(() => {
    if (deadId) onViewChange({ ...view, view: undefined });
  }, [deadId, view, onViewChange]);

  if (!views.data) {
    return <ProblemText error={views.error} />;
  }
  const selected = views.data.views.find((saved) => saved.id === view.view);
  // Until the effect above drops an unknown ID, the list shows as the default view.
  const base: ListSearch = selected ? searchFor(selected) : {};
  const changed = !showSameList(view, base);
  const canCreate = views.data.allowed_actions.includes("create");

  return (
    <div className="mt-4 flex flex-col gap-2">
      <nav aria-label="Saved views" className="flex flex-wrap items-center gap-1.5">
        <ViewLink heapId={heapId} search={{}} current={!selected} changed={changed}>
          Default
        </ViewLink>
        {views.data.views.map((saved) => (
          <ViewLink
            key={saved.id}
            heapId={heapId}
            search={searchFor(saved)}
            current={saved.id === selected?.id}
            changed={changed}
          >
            {saved.name}
          </ViewLink>
        ))}
      </nav>
      {naming === "new" && (
        <CreateView
          heapId={heapId}
          view={view}
          onCreated={(created) => {
            setNaming(null);
            onViewChange(searchFor(created));
          }}
          onCancel={() => setNaming(null)}
        />
      )}
      {naming === "rename" && selected && (
        <RenameView heapId={heapId} saved={selected} onDone={() => setNaming(null)} />
      )}
      {naming === null && changed && (
        <ChangedActions
          heapId={heapId}
          view={view}
          selected={selected}
          canCreate={canCreate}
          onSaveAsNew={() => setNaming("new")}
          onDiscard={() => onViewChange(base)}
        />
      )}
      {naming === null && !changed && selected && (
        <SelectedActions
          heapId={heapId}
          saved={selected}
          onRename={() => setNaming("rename")}
          onArchived={() => onViewChange({ ...view, view: undefined })}
        />
      )}
      {selected && (
        <HistoryDisclosure
          key={selected.id}
          scope={{ heapId }}
          entityId={selected.id}
          label={`History of ${selected.name}`}
        />
      )}
    </div>
  );
}

function ViewLink({
  heapId,
  search,
  current,
  changed,
  children,
}: {
  heapId: string;
  search: ListSearch;
  current: boolean;
  changed: boolean;
  children: React.ReactNode;
}) {
  return (
    <Link
      to="/heaps/$heapId"
      params={{ heapId }}
      search={search}
      // The open view stays marked while its settings are changed, which the router's own matching wouldn't do.
      activeOptions={{ exact: true, includeSearch: true }}
      activeProps={{}}
      aria-current={current ? "page" : undefined}
      className={`rounded-full px-2.5 py-0.5 text-sm ${
        current
          ? "bg-stone-900 text-white dark:bg-stone-100 dark:text-stone-900"
          : "text-stone-600 hover:bg-stone-100 dark:text-stone-300 dark:hover:bg-stone-800"
      }`}
    >
      {children}
      {current && changed && <span className="ml-1 opacity-70">(changed)</span>}
    </Link>
  );
}

/** The list differs from the open view: save it into that view, save it as a new one, or go back. */
function ChangedActions({
  heapId,
  view,
  selected,
  canCreate,
  onSaveAsNew,
  onDiscard,
}: {
  heapId: string;
  view: ListSearch;
  selected: SavedView | undefined;
  canCreate: boolean;
  onSaveAsNew: () => void;
  onDiscard: () => void;
}) {
  const update = useUpdateSavedView(heapId);
  const canSave = selected?.allowed_actions.includes("edit") === true;
  return (
    <div className="flex flex-wrap items-center gap-1 text-sm">
      {selected && canSave && (
        <button
          type="button"
          disabled={update.isPending}
          onClick={() => update.mutate({ saved: selected, patch: { view: toViewInput(view) } })}
          className={quietButtonClass}
        >
          Save to {selected.name}
        </button>
      )}
      {canCreate && (
        <button type="button" onClick={onSaveAsNew} className={quietButtonClass}>
          Save as a new view…
        </button>
      )}
      <button type="button" onClick={onDiscard} className={quietButtonClass}>
        {selected ? `Back to ${selected.name}` : "Back to the default view"}
      </button>
      <ProblemText error={update.error} />
    </div>
  );
}

function SelectedActions({
  heapId,
  saved,
  onRename,
  onArchived,
}: {
  heapId: string;
  saved: SavedView;
  onRename: () => void;
  onArchived: () => void;
}) {
  const archive = useArchiveSavedView(heapId);
  const { confirming, ask, cancel, buttonRef } = useConfirmation();
  const can = (action: SavedView["allowed_actions"][number]) => saved.allowed_actions.includes(action);
  if (!can("edit") && !can("archive")) return null;

  return (
    <div className="flex flex-wrap items-center gap-1 text-sm">
      {can("edit") && !confirming && (
        <button type="button" onClick={onRename} className={quietButtonClass}>
          Rename
        </button>
      )}
      {can("archive") && !confirming && (
        <button ref={buttonRef} type="button" onClick={ask} className={quietButtonClass}>
          Archive
        </button>
      )}
      {confirming && (
        <>
          <span>Archive {saved.name} for everyone in the team?</span>
          <button
            type="button"
            autoFocus
            disabled={archive.isPending}
            onClick={() => archive.mutate(saved, { onSuccess: onArchived, onError: cancel })}
            className="rounded px-1.5 py-1 font-medium text-red-700 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950"
          >
            Archive
          </button>
          <button type="button" onClick={cancel} className={quietButtonClass}>
            Cancel
          </button>
        </>
      )}
      <ProblemText error={archive.error} />
    </div>
  );
}

function CreateView({
  heapId,
  view,
  onCreated,
  onCancel,
}: {
  heapId: string;
  view: ListSearch;
  onCreated: (created: SavedView) => void;
  onCancel: () => void;
}) {
  const create = useCreateSavedView(heapId);
  return (
    <NameEntry
      label="Name of the new view"
      initial=""
      submitLabel="Save view"
      pending={create.isPending}
      error={create.error}
      onSubmit={(name) => create.mutate({ name, view: toViewInput(view) }, { onSuccess: onCreated })}
      onCancel={onCancel}
    />
  );
}

function RenameView({ heapId, saved, onDone }: { heapId: string; saved: SavedView; onDone: () => void }) {
  const update = useUpdateSavedView(heapId);
  return (
    <NameEntry
      label={`New name for ${saved.name}`}
      initial={saved.name}
      submitLabel="Rename"
      pending={update.isPending}
      error={update.error}
      onSubmit={(name) => update.mutate({ saved, patch: { name } }, { onSuccess: onDone })}
      onCancel={onDone}
    />
  );
}

/** A name to type, focused on open. Escape or Cancel closes it. */
function NameEntry({
  label,
  initial,
  submitLabel,
  pending,
  error,
  onSubmit,
  onCancel,
}: {
  label: string;
  initial: string;
  submitLabel: string;
  pending: boolean;
  error: Error | null;
  onSubmit: (name: string) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState(initial);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (name.trim() !== "") onSubmit(name);
  };
  return (
    <form onSubmit={submit} className="flex flex-wrap items-center gap-2">
      <input
        aria-label={label}
        value={name}
        autoFocus
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.preventDefault();
            onCancel();
          }
        }}
        placeholder="View name"
        maxLength={100}
        className={`${inputClass} w-56`}
      />
      <button type="submit" disabled={pending || name.trim() === ""} className={primaryButtonClass}>
        {submitLabel}
      </button>
      <button type="button" onClick={onCancel} className={quietButtonClass}>
        Cancel
      </button>
      <ProblemText error={error} />
    </form>
  );
}
