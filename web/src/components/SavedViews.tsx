import { Check, Ellipsis, Plus } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent } from "react";
import { Button, Menu, MenuTrigger } from "react-aria-components";

import { requestIdOf, type HeapRef } from "../api/client";
import type { SavedView } from "../api/schema";
import {
  useArchiveSavedView,
  useCreateSavedView,
  useSavedViews,
  useUnarchiveSavedView,
  useUpdateSavedView,
} from "../api/queries/views";
import { compactButtonClass } from "./buttons";
import { holdsSettings, isCurrentView, searchOfView, viewInputOf, type ListSearch } from "./listView";
import { MenuEntry, MenuPopover } from "./Menu";
import { problemMessage, ProblemText } from "./ProblemText";
import { Section } from "./Section";
import { useToast } from "./UndoToast";
import { useFocusAfterRender } from "./useFocusAfterRender";
import { Saving } from "./Saving";

/** The row of a view's button, or of the "Save as view…" button. */
const rowClass =
  "flex min-h-9 min-w-0 flex-1 items-center gap-2 rounded-lg px-2 text-left text-sm outline-offset-2 hover:bg-sunken pointer-coarse:min-h-11";

/**
 * The heap's saved views (ux.md, The list §6): tap one to apply it, name the list's settings as a new one, and rename,
 * update or archive a view from its ⋯. The server's `allowed_actions` say which of those show. Applying calls
 * `onChange` with the view's settings, which replace the list's, so its agent filters are cleared, and then `onApplied`.
 * Updating and archiving call `onLeave` first, which closes the panel: the open panel keeps the page out of reach,
 * the toast that says how it went too.
 */
export function ViewsSection({
  heapRef,
  view,
  onChange,
  onApplied,
  onLeave,
}: {
  heapRef: HeapRef;
  view: ListSearch;
  onChange: (view: ListSearch) => void;
  onApplied: () => void;
  onLeave: () => void;
}) {
  const views = useSavedViews(heapRef);
  const create = useCreateSavedView(heapRef);
  const rename = useUpdateSavedView(heapRef);
  const update = useUpdateSavedView(heapRef);
  const archive = useArchiveSavedView(heapRef);
  const unarchive = useUnarchiveSavedView(heapRef);
  const setToast = useToast();
  const focusAfterRender = useFocusAfterRender();
  // The view whose name is being edited, or "new" for the name of a view to save. Local, so a refetch keeps it.
  const [editing, setEditing] = useState<string | null>(null);
  const list = useRef<HTMLDivElement>(null);
  const saveRef = useRef<HTMLButtonElement>(null);

  const open = (id: string) => {
    create.reset();
    rename.reset();
    setEditing(id);
  };
  const focusView = (id: string) =>
    focusAfterRender(() => list.current?.querySelector<HTMLElement>(`[data-view-id="${id}"]`));
  const failed = (error: Error) =>
    setToast({ message: problemMessage(error), error: true, requestId: requestIdOf(error) });

  // `mutateAsync`, not `mutate` with callbacks: the panel closes first, and TanStack drops a call's callbacks once
  // the component that made it has unmounted.
  const refresh = (saved: SavedView) => {
    onLeave();
    void update
      .mutateAsync({ view: saved, patch: { view: viewInputOf(view) } })
      .then(() => setToast({ message: `Updated ${saved.name}.` }), failed);
  };
  const remove = (saved: SavedView) => {
    onLeave();
    void archive.mutateAsync(saved).then(
      (archived) =>
        setToast({
          message: `Archived ${saved.name}.`,
          undo: archived.allowed_actions.includes("unarchive")
            ? () => void unarchive.mutateAsync(archived).then(undefined, failed)
            : undefined,
        }),
      failed,
    );
  };

  if (!views.data) {
    return (
      <Section title="Views">
        <p className="text-sm text-muted">{views.isError ? "Couldn't load views." : "Loading views…"}</p>
      </Section>
    );
  }
  const canCreate = views.data.allowed_actions.includes("create");

  return (
    <Section title="Views">
      <div ref={list} className="flex flex-col gap-0.5">
        {views.data.views.length === 0 && !canCreate && <p className="text-sm text-muted">No saved views yet.</p>}
        {views.data.views.map((saved) =>
          editing === saved.id ? (
            <NameField
              key={saved.id}
              initial={saved.name}
              pending={rename.isPending}
              error={rename.error}
              onSave={(name) =>
                rename.mutate(
                  { view: saved, patch: { name } },
                  {
                    onSuccess: () => {
                      setEditing(null);
                      focusView(saved.id);
                    },
                  },
                )
              }
              onCancel={() => {
                setEditing(null);
                focusView(saved.id);
              }}
            />
          ) : (
            <ViewRow
              key={saved.id}
              saved={saved}
              current={isCurrentView(saved.view, view)}
              holdsSettings={holdsSettings(saved.view, view)}
              onApply={() => {
                onChange(searchOfView(saved.view));
                onApplied();
              }}
              onRename={() => open(saved.id)}
              onUpdate={() => refresh(saved)}
              onArchive={() => remove(saved)}
            />
          ),
        )}
        {canCreate &&
          (editing === "new" ? (
            <NameField
              initial=""
              pending={create.isPending}
              error={create.error}
              onSave={(name) =>
                create.mutate(
                  { name, view: viewInputOf(view) },
                  {
                    onSuccess: () => {
                      setEditing(null);
                      focusAfterRender(() => saveRef.current);
                    },
                  },
                )
              }
              onCancel={() => {
                setEditing(null);
                focusAfterRender(() => saveRef.current);
              }}
            />
          ) : (
            <button ref={saveRef} type="button" onClick={() => open("new")} className={`${rowClass} text-ink-soft`}>
              <Plus aria-hidden="true" className="size-4 shrink-0" />
              Save as view…
            </button>
          ))}
      </div>
    </Section>
  );
}

function ViewRow({
  saved,
  current,
  holdsSettings,
  onApply,
  onRename,
  onUpdate,
  onArchive,
}: {
  saved: SavedView;
  current: boolean;
  /** Whether the view already holds the list's settings, so updating it would change nothing. */
  holdsSettings: boolean;
  onApply: () => void;
  onRename: () => void;
  onUpdate: () => void;
  onArchive: () => void;
}) {
  const can = (action: SavedView["allowed_actions"][number]) => saved.allowed_actions.includes(action);
  return (
    <div className="flex items-center gap-1">
      <button
        type="button"
        data-view-id={saved.id}
        aria-label={current ? `${saved.name}, current view` : saved.name}
        onClick={onApply}
        className={rowClass}
      >
        <span aria-hidden="true" className="flex size-4 shrink-0 items-center justify-center text-accent-ink">
          {current && <Check className="size-4" strokeWidth={3} />}
        </span>
        <span className="min-w-0 truncate">{saved.name}</span>
      </button>
      {saved.allowed_actions.length > 0 && (
        <MenuTrigger>
          <Button
            aria-label={`More for ${saved.name}`}
            className="flex size-9 shrink-0 items-center justify-center rounded-lg text-ink-soft outline-none hover:bg-sunken data-[focus-visible]:outline-2 data-[focus-visible]:outline-accent pointer-coarse:size-11"
          >
            <Ellipsis aria-hidden="true" className="size-4" />
          </Button>
          <MenuPopover placement="bottom end">
            <Menu aria-label={`More for ${saved.name}`} className="outline-none">
              {can("edit") && (
                <MenuEntry id="rename" onAction={onRename}>
                  Rename…
                </MenuEntry>
              )}
              {can("edit") && !holdsSettings && (
                <MenuEntry id="update" onAction={onUpdate}>
                  Update to current settings
                </MenuEntry>
              )}
              {can("archive") && (
                <MenuEntry id="archive" onAction={onArchive}>
                  Archive
                </MenuEntry>
              )}
            </Menu>
          </MenuPopover>
        </MenuTrigger>
      )}
    </div>
  );
}

/**
 * A view's name in place of its row: empty to name a new view, or the view's own to rename it, selected. Save needs
 * a name; Enter saves; Escape or Cancel closes it, and Escape stops there instead of closing the panel.
 */
function NameField({
  initial,
  pending,
  error,
  onSave,
  onCancel,
}: {
  initial: string;
  pending: boolean;
  error: Error | null;
  onSave: (name: string) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState(initial);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    // Focused in an effect, not with `autoFocus`: the field mounts in the render that closes the ⋯ menu it may have
    // been opened from (useFocusAfterRender explains why that order matters).
    input.current?.focus();
    input.current?.select();
  }, []);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (name.trim() !== "") onSave(name);
  };
  return (
    <form
      onSubmit={submit}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          onCancel();
        }
      }}
      className="flex flex-wrap items-center gap-2"
    >
      <input
        ref={input}
        aria-label="View name"
        value={name}
        onChange={(e) => setName(e.target.value)}
        maxLength={100}
        className="h-9 min-w-0 flex-1 rounded-lg border border-line bg-surface px-3 text-sm max-sm:text-base pointer-coarse:h-11"
      />
      <button
        type="submit"
        disabled={pending || name.trim() === ""}
        aria-busy={pending || undefined}
        className={compactButtonClass.primary}
      >
        <Saving pending={pending} />
        Save
      </button>
      <button type="button" onClick={onCancel} className={compactButtonClass.quiet}>
        Cancel
      </button>
      <ProblemText error={error} />
    </form>
  );
}
