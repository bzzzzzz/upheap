import { Link } from "@tanstack/react-router";
import { useEffect, useState, type FormEvent, type ReactNode } from "react";
import { Button, Menu, MenuTrigger } from "react-aria-components";

import type { HeapRef, SavedView } from "../api/client";
import { useArchiveSavedView, useCreateSavedView, useSavedViews, useUpdateSavedView } from "../api/queries";
import { HistoryDisclosure } from "./AuditHistory";
import { searchFor, showSameList, toViewInput, type ListSearch } from "./listView";
import { MenuEntry, MenuPopover } from "./Menu";
import { inputClass, primaryButtonClass, quietButtonClass } from "./NameForm";
import { ProblemText } from "./ProblemText";
import { useConfirmation } from "./useConfirmation";

type Naming = "new" | "rename" | null;

/**
 * The heap's saved views, as links, and what can be done with the open one. Opening a view puts its settings and its
 * ID in the URL. Changing the list afterwards keeps the ID, and the view's chip shows a dot: pressing it offers to save
 * the change into the view or as a new one, or to undo it (ux-overview.md, The list §3). Every member sees the same
 * views.
 */
export function SavedViewsBar({
  heapRef,
  view,
  onViewChange,
}: {
  heapRef: HeapRef;
  view: ListSearch;
  onViewChange: (view: ListSearch) => void;
}) {
  const views = useSavedViews(heapRef);
  const update = useUpdateSavedView(heapRef);
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
  const canSave = selected?.allowed_actions.includes("edit") === true;
  const onChangedAction = (key: ChangedAction) => {
    switch (key) {
      case "save":
        if (selected) update.mutate({ saved: selected, patch: { view: toViewInput(view) } });
        break;
      case "save-new":
        setNaming("new");
        break;
      case "undo":
        onViewChange(base);
        break;
    }
  };
  const chip = (saved: SavedView | undefined, name: string) =>
    saved?.id === selected?.id && changed ? (
      <ChangedChip
        key={saved?.id ?? ""}
        name={name}
        canSave={canSave}
        canCreate={canCreate}
        pending={update.isPending}
        onAction={onChangedAction}
      />
    ) : (
      <ViewLink
        key={saved?.id ?? ""}
        heapRef={heapRef}
        search={saved ? searchFor(saved) : {}}
        current={saved?.id === selected?.id}
      >
        {name}
      </ViewLink>
    );

  return (
    <div className="mt-4 flex flex-col gap-2">
      {/* On a phone the chips scroll sideways rather than wrap (app-ux.md, The list §6). */}
      <nav
        aria-label="Saved views"
        className="flex items-center gap-1.5 max-sm:-mx-4 max-sm:overflow-x-auto max-sm:px-4 max-sm:[scrollbar-width:none] sm:flex-wrap [&>*]:shrink-0"
      >
        {chip(undefined, "Default")}
        {views.data.views.map((saved) => chip(saved, saved.name))}
      </nav>
      <ProblemText error={update.error} />
      {naming === "new" && (
        <CreateView
          heapRef={heapRef}
          view={view}
          onCreated={(created) => {
            setNaming(null);
            onViewChange(searchFor(created));
          }}
          onCancel={() => setNaming(null)}
        />
      )}
      {naming === "rename" && selected && (
        <RenameView heapRef={heapRef} saved={selected} onDone={() => setNaming(null)} />
      )}
      {naming === null && !changed && selected && (
        <SelectedActions
          heapRef={heapRef}
          saved={selected}
          onRename={() => setNaming("rename")}
          onArchived={() => onViewChange({ ...view, view: undefined })}
        />
      )}
      {selected && (
        <HistoryDisclosure
          key={selected.id}
          scope={{ heap: heapRef }}
          entityId={selected.id}
          label={`History of ${selected.name}`}
        />
      )}
    </div>
  );
}

const chipClass = "rounded-full px-2.5 py-0.5 text-sm whitespace-nowrap";
const currentChipClass = "bg-stone-900 text-white dark:bg-stone-100 dark:text-stone-900";

function ViewLink({
  heapRef,
  search,
  current,
  children,
}: {
  heapRef: HeapRef;
  search: ListSearch;
  current: boolean;
  children: ReactNode;
}) {
  return (
    <Link
      to="/teams/$team/heaps/$heap"
      params={heapRef}
      search={search}
      // The open view stays marked while its settings are changed, which the router's own matching wouldn't do.
      activeOptions={{ exact: true, includeSearch: true }}
      activeProps={{}}
      aria-current={current ? "page" : undefined}
      className={`${chipClass} ${
        current ? currentChipClass : "text-stone-600 hover:bg-stone-100 dark:text-stone-300 dark:hover:bg-stone-800"
      }`}
    >
      {children}
    </Link>
  );
}

type ChangedAction = "save" | "save-new" | "undo";

/**
 * The open view's chip once the list differs from it: a dot says so, and pressing it offers to save the change into
 * the view (not into Default, which isn't saved), to save it as a new view, or to undo it.
 */
function ChangedChip({
  name,
  canSave,
  canCreate,
  pending,
  onAction,
}: {
  name: string;
  canSave: boolean;
  canCreate: boolean;
  pending: boolean;
  onAction: (action: ChangedAction) => void;
}) {
  return (
    <MenuTrigger>
      <Button
        aria-current="page"
        className={`${chipClass} ${currentChipClass} inline-flex items-center gap-1.5 outline-none data-[focus-visible]:outline-2 data-[focus-visible]:outline-offset-2 data-[focus-visible]:outline-accent`}
      >
        {name}
        <span aria-hidden="true" className="size-1.5 rounded-full bg-accent" />
        <span className="sr-only">(changed)</span>
      </Button>
      <MenuPopover placement="bottom start">
        <Menu
          aria-label={`Changes to ${name}`}
          className="outline-none"
          disabledKeys={pending ? ["save"] : []}
          onAction={(key) => onAction(key as ChangedAction)}
        >
          {canSave && <MenuEntry id="save">{`Save to ${name}`}</MenuEntry>}
          {canCreate && <MenuEntry id="save-new">Save as a new view…</MenuEntry>}
          <MenuEntry id="undo">Undo changes</MenuEntry>
        </Menu>
      </MenuPopover>
    </MenuTrigger>
  );
}

function SelectedActions({
  heapRef,
  saved,
  onRename,
  onArchived,
}: {
  heapRef: HeapRef;
  saved: SavedView;
  onRename: () => void;
  onArchived: () => void;
}) {
  const archive = useArchiveSavedView(heapRef);
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
  heapRef,
  view,
  onCreated,
  onCancel,
}: {
  heapRef: HeapRef;
  view: ListSearch;
  onCreated: (created: SavedView) => void;
  onCancel: () => void;
}) {
  const create = useCreateSavedView(heapRef);
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

function RenameView({ heapRef, saved, onDone }: { heapRef: HeapRef; saved: SavedView; onDone: () => void }) {
  const update = useUpdateSavedView(heapRef);
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
