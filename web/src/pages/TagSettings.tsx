// The parts of a tag's settings that a heap's Tags tab and the Team page's Tags section share: the New tag card, a tag
// as a row, and the form that edits it. What differs between a heap's tags and a team's comes in as `TagSettings`.
import { Pencil } from "lucide-react";
import { useState, type FormEvent } from "react";

import { ApiProblem } from "../api/client";
import type { Tag } from "../api/schema";
import { useCreateTagIn, useUpdateTagIn, type TagScope } from "../api/queries/tags";
import { HistoryDisclosure } from "../components/AuditHistory";
import { compactButtonClass } from "../components/buttons";
import { useReturnFocus } from "../components/useConfirmation";
import { RequestId } from "../components/RequestId";
import { listOf } from "../components/words";
import { cardClass, cardTitleClass, inputClass } from "./settingsForm";

export const chipClass =
  "inline-flex h-6 max-w-full shrink-0 items-center truncate rounded-full border border-line bg-sunken px-[9px] text-[13px] font-medium text-ink max-sm:h-[26px] max-sm:px-2.5 max-sm:text-sm";

/** The card of tag rows. */
export const tagCardClass = "overflow-hidden rounded-[14px] border border-line bg-surface";

/** What differs between a heap's tags and a team's. */
export type TagSettings = {
  scope: TagScope;
  /** The team's name as the page shows it, for the messages. */
  teamName: string;
};

/** "New tag" for a heap's tags, "New team tag" for a team's; the inputs' names follow it. */
export function NewTagForm({ settings }: { settings: TagSettings }) {
  const create = useCreateTagIn(settings.scope);
  const team = settings.scope.kind === "team";
  const title = team ? "New team tag" : "New tag";
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
    <section aria-labelledby={team ? "new-team-tag" : "new-tag"} className={cardClass}>
      <h3 id={team ? "new-team-tag" : "new-tag"} className={`${cardTitleClass} mb-3`}>
        {title}
      </h3>
      <form onSubmit={submit} className="flex flex-col gap-2.5 sm:flex-row sm:flex-wrap sm:items-end">
        <label className="flex flex-col gap-1.5 text-sm font-medium sm:max-w-[220px] sm:flex-[1_1_180px]">
          <span className="max-sm:sr-only">Name</span>
          <input
            aria-label={`${title} name`}
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
            aria-label={`${title} description`}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="Optional: what belongs under this tag"
            className={`${inputClass} font-normal`}
          />
        </label>
        <button type="submit" disabled={create.isPending || name.trim() === ""} className={compactButtonClass.primary}>
          Add tag
        </button>
        <TagProblem error={create.error} typed={name} settings={settings} />
      </form>
    </section>
  );
}

/**
 * A tag as a row: its name, its description and, for those who may edit it, an edit button that turns the row into
 * the form. Those who may not still find a heap tag's history in the row; editors find it in the form.
 */
export function TagRow({ tag, settings }: { tag: Tag; settings: TagSettings }) {
  const historyHeap = settings.scope.kind === "heap" ? settings.scope.heap : undefined;
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
        <TagForm tag={tag} settings={settings} onClose={close} />
        {historyHeap && <HistoryDisclosure scope={{ heap: historyHeap }} entityId={tag.id} className="mt-2" />}
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
      {!canEdit && historyHeap && <HistoryDisclosure scope={{ heap: historyHeap }} entityId={tag.id} />}
    </li>
  );
}

/**
 * The tag's name and description, saved together. Saving, Cancel or Escape close the form; Save with nothing changed
 * just closes it. A newer version from the server (someone else's edit) replaces what's typed.
 */
function TagForm({ tag, settings, onClose }: { tag: Tag; settings: TagSettings; onClose: () => void }) {
  const update = useUpdateTagIn(settings.scope);
  const { teamName } = settings;
  const team = settings.scope.kind === "team";
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
      {team && <p className="basis-full text-[13px] text-muted">Changes in every heap of {teamName}.</p>}
      <TagProblem error={update.error} typed={name} settings={settings} />
    </form>
  );
}

/** Why a write was refused, under the field. `typed` is the name in the field. */
function TagProblem({ error, typed, settings }: { error: Error | null; typed: string; settings: TagSettings }) {
  if (!error) return null;
  const { teamName } = settings;
  const team = settings.scope.kind === "team";
  const code = error instanceof ApiProblem ? error.code : undefined;
  let message = error.message;
  if (code === "version_conflict") {
    message = "Someone else changed this tag. It now shows their version.";
  } else if (!team && code === "tag_name_taken") {
    message = "This heap already has a tag with that name.";
  } else if (!team && code === "tag_name_taken_by_team_tag") {
    message = `“${typed}” is a team tag of ${teamName}. Use it on items here instead.`;
  } else if (team && code === "tag_name_taken") {
    message = `${teamName} already has a team tag with that name.`;
  } else if (team && code === "tag_merge_unconfirmed" && error instanceof ApiProblem && error.mergeReach) {
    const heaps = error.mergeReach.heaps.map((reached) => reached.heap.name);
    message = `${listOf(heaps)} ${heaps.length === 1 ? "has" : "have"} a heap tag “${typed}”. Making it a team tag merges them; that isn't in the web app yet. Use the CLI or pick another name.`;
  }
  return (
    <>
      <p role="alert" className="basis-full text-sm text-overdue-ink">
        {message}
      </p>
      <RequestId error={error} />
    </>
  );
}
