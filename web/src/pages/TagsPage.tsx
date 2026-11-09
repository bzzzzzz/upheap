import { Pencil } from "lucide-react";
import { useState, type FormEvent } from "react";

import { ApiProblem, type Tag, type HeapRef } from "../api/client";
import { useCreateTag, useTags, useUpdateTag } from "../api/queries";
import { HistoryDisclosure } from "../components/AuditHistory";
import { compactButtonClass } from "../components/buttons";
import { useReturnFocus } from "../components/useConfirmation";
import { plural } from "../components/words";
import { cardClass, cardTitleClass, inputClass, introClass } from "./settingsForm";

const chipClass =
  "inline-flex h-6 max-w-full shrink-0 items-center truncate rounded-full border border-line bg-sunken px-[9px] text-[13px] font-medium text-ink max-sm:h-[26px] max-sm:px-2.5 max-sm:text-sm";

/**
 * Any member can add tags, rename them and edit their descriptions, as `allowed_actions` says (not in an archived
 * heap). Tags can't be deleted. Each tag reads as a row; its edit button turns the row into a form.
 */
export function TagsPage({ heap: heapRef }: { heap: HeapRef }) {
  const tags = useTags(heapRef);

  // A failed refetch keeps the data, so what is on screen (and any draft in it) stays; the error shows only without it.
  if (!tags.data) {
    if (tags.isError) {
      return (
        <p role="alert" className="py-16 text-center text-overdue-ink">
          {tags.error.message}
        </p>
      );
    }
    return <p className="py-16 text-center text-muted">Loading…</p>;
  }

  const { heap, allowed_actions } = tags.data;
  const count = tags.data.tags.length;
  return (
    <section className="flex flex-col gap-4 sm:gap-6">
      <h2 className="sr-only">Tags</h2>
      <p className={introClass}>Tags group similar items in {heap.name}. They can be renamed but not deleted.</p>
      {allowed_actions.includes("create") && <NewTagForm heapRef={heapRef} />}
      {count === 0 ? (
        <p className="py-12 text-center text-muted">No tags yet.</p>
      ) : (
        <section aria-labelledby="all-tags" className="overflow-hidden rounded-[14px] border border-line bg-surface">
          <h3
            id="all-tags"
            className={`${cardTitleClass} border-b border-line px-3.5 pt-3.5 pb-2.5 sm:px-4 sm:pt-4 sm:pb-3`}
          >
            {plural(count, "tag")}
          </h3>
          <ul className="divide-y divide-line">
            {tags.data.tags.map((tag) => (
              <TagRow key={tag.id} tag={tag} heapRef={heapRef} />
            ))}
          </ul>
        </section>
      )}
    </section>
  );
}

function NewTagForm({ heapRef }: { heapRef: HeapRef }) {
  const create = useCreateTag(heapRef);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (name.trim() === "") return;
    create.mutate(
      { name, description: description || null },
      {
        onSuccess: () => {
          setName("");
          setDescription("");
        },
      },
    );
  };

  return (
    <section aria-labelledby="new-tag" className={cardClass}>
      <h3 id="new-tag" className={`${cardTitleClass} mb-3`}>
        New tag
      </h3>
      <form onSubmit={submit} className="flex flex-col gap-2.5 sm:flex-row sm:flex-wrap sm:items-end">
        <label className="flex flex-col gap-1.5 text-sm font-medium sm:max-w-[220px] sm:flex-[1_1_180px]">
          <span className="max-sm:sr-only">Name</span>
          <input
            aria-label="New tag name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g. compliance"
            maxLength={50}
            className={`${inputClass} font-normal`}
          />
        </label>
        <label className="flex flex-col gap-1.5 text-sm font-medium sm:flex-[999_1_240px]">
          <span className="max-sm:sr-only">Description</span>
          <input
            aria-label="New tag description"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="Optional: what belongs under this tag"
            className={`${inputClass} font-normal`}
          />
        </label>
        <button type="submit" disabled={create.isPending || name.trim() === ""} className={compactButtonClass.primary}>
          Add tag
        </button>
        <TagProblem error={create.error} />
      </form>
    </section>
  );
}

/**
 * A tag as a row: its name, its description and, for those who may edit it, an edit button that turns the row into
 * the form. Those who may not still find its history in the row; editors find it in the form.
 */
function TagRow({ tag, heapRef }: { tag: Tag; heapRef: HeapRef }) {
  const canEdit = tag.allowed_actions.includes("edit");
  const [editing, setEditing] = useState(false);
  const { ref: editButton, returnFocus } = useReturnFocus<HTMLButtonElement>(editing);
  const close = () => {
    returnFocus();
    setEditing(false);
  };

  if (editing && canEdit) {
    return (
      <li className="bg-accent-soft p-3.5 sm:px-4 sm:pb-4">
        <TagForm tag={tag} heapRef={heapRef} onClose={close} />
        <HistoryDisclosure scope={{ heap: heapRef }} entityId={tag.id} className="mt-2" />
      </li>
    );
  }
  return (
    <li className="py-2.5 pr-1 pl-3.5 sm:px-4 sm:py-3.5">
      <div className="flex items-center gap-2 sm:gap-4">
        <div className="flex min-w-0 flex-1 flex-col items-start gap-1.5 sm:flex-row sm:items-center sm:gap-4">
          <span className={chipClass}>{tag.name}</span>
          {tag.description ? (
            <span className="min-w-0 text-sm text-ink-soft">{tag.description}</span>
          ) : (
            <span className="text-sm text-muted italic">No description</span>
          )}
        </div>
        {canEdit && (
          <button
            ref={editButton}
            type="button"
            onClick={() => setEditing(true)}
            aria-label={`Edit ${tag.name}`}
            className={compactButtonClass.icon}
          >
            <Pencil aria-hidden="true" className="size-4" />
          </button>
        )}
      </div>
      {!canEdit && <HistoryDisclosure scope={{ heap: heapRef }} entityId={tag.id} />}
    </li>
  );
}

/**
 * The tag's name and description, saved together. Saving, Cancel or Escape close the form; Save with nothing changed
 * just closes it. A newer version from the server (someone else's edit) replaces what's typed.
 */
function TagForm({ tag, heapRef, onClose }: { tag: Tag; heapRef: HeapRef; onClose: () => void }) {
  const update = useUpdateTag(heapRef);
  const [name, setName] = useState(tag.name);
  const [description, setDescription] = useState(tag.description ?? "");
  const [shownVersion, setShownVersion] = useState(tag.version);
  if (shownVersion !== tag.version) {
    setShownVersion(tag.version);
    setName(tag.name);
    setDescription(tag.description ?? "");
  }
  const changed = name !== tag.name || description !== (tag.description ?? "");

  const save = (event: FormEvent) => {
    event.preventDefault();
    if (!changed) {
      onClose();
      return;
    }
    update.mutate(
      {
        tag,
        patch: {
          ...(name !== tag.name && { name }),
          ...(description !== (tag.description ?? "") && { description: description || null }),
        },
      },
      { onSuccess: onClose },
    );
  };

  return (
    <form
      onSubmit={save}
      onKeyDown={(e) => {
        if (e.key !== "Escape") return;
        e.preventDefault();
        onClose();
      }}
      className="flex flex-col gap-2.5 sm:flex-row sm:flex-wrap sm:items-center"
    >
      <input
        aria-label={`Name of tag ${tag.name}`}
        value={name}
        autoFocus
        onChange={(e) => setName(e.target.value)}
        maxLength={50}
        className={`${inputClass} font-medium sm:max-w-[200px] sm:flex-[1_1_160px]`}
      />
      <input
        aria-label={`Description of tag ${tag.name}`}
        value={description}
        onChange={(e) => setDescription(e.target.value)}
        placeholder="No description"
        className={`${inputClass} sm:flex-[999_1_240px]`}
      />
      <div className="flex gap-2">
        <button
          type="submit"
          disabled={update.isPending || name.trim() === ""}
          className={`${compactButtonClass.primary} max-sm:flex-1`}
        >
          Save
        </button>
        <button type="button" onClick={onClose} className={compactButtonClass.quiet}>
          Cancel
        </button>
      </div>
      <TagProblem error={update.error} />
    </form>
  );
}

function TagProblem({ error }: { error: Error | null }) {
  if (!error) return null;
  const message =
    error instanceof ApiProblem && error.code === "tag_name_taken"
      ? "This heap already has a tag with that name."
      : error instanceof ApiProblem && error.code === "version_conflict"
        ? "Someone else changed this tag. It now shows their version."
        : error.message;
  return (
    <p role="alert" className="basis-full text-sm text-overdue-ink">
      {message}
    </p>
  );
}
