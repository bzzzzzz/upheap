// The parts of a tag's settings that a heap's Tags tab and the Team page's Tags section share: the New tag card, a tag
// as a row, and the form that edits it. What differs between a heap's tags and a team's comes in as `TagSettings`.
import { Ellipsis, Pencil, Users } from "lucide-react";
import { useState, type FormEvent } from "react";
import { Button, Label, Menu, MenuTrigger, Radio, RadioGroup } from "react-aria-components";

import { ApiProblem } from "../api/client";
import type { HeapSummary, MergeReach, Tag, TagColor } from "../api/schema";
import { type AuditScope } from "../api/queries/audit";
import { useCreateTagIn, useUpdateTagIn, type TagScope } from "../api/queries/tags";
import { HistoryDisclosure } from "../components/AuditHistory";
import { compactButtonClass } from "../components/buttons";
import { MenuEntry, MenuPopover } from "../components/Menu";
import { tagChipColor, tagColors, tagDotColor } from "../components/tagColor";
import { useToast } from "../components/UndoToast";
import { useReturnFocus } from "../components/useConfirmation";
import { RequestId } from "../components/RequestId";
import { problemMessage } from "../components/ProblemText";
import { plural } from "../components/words";
import { MergeConfirm } from "./MergeConfirm";
import { cardClass, cardTitleClass, inputClass } from "./settingsForm";

const chipShape =
  "inline-flex h-6 max-w-full shrink-0 items-center truncate rounded-full border px-[9px] text-[13px] font-medium max-sm:h-[26px] max-sm:px-2.5 max-sm:text-sm";

/** A name in a chip, grey. A tag's own chip is in its colour (`tagChipClass`). */
export const chipClass = `${chipShape} border-line bg-sunken text-ink`;

/** A tag's chip: its border and text in the tag's colour, with no fill. */
const tagChipClass = (color: TagColor) => `${chipShape} bg-transparent ${tagChipColor(color)}`;

/** The card of tag rows. */
export const tagCardClass = "overflow-hidden rounded-[14px] border border-line bg-surface";

/** What differs between a heap's tags and a team's. */
export type TagSettings = {
  scope: TagScope;
  /** The team's name as the page shows it, for the messages. */
  teamName: string;
};

/** The reach a team tag's write was refused with because it would merge heap tags, or nothing for any other error. */
const mergeReachOf = (error: unknown): MergeReach | undefined =>
  error instanceof ApiProblem && error.code === "tag_merge_unconfirmed" ? error.mergeReach : undefined;

/** The toast after a confirmed merge: how many items in how many heaps carry the team tag. */
export const carriedBy = (reach: MergeReach) =>
  `${plural(
    reach.heaps.reduce((sum, reached) => sum + reached.items, 0),
    "item",
  )} in ${plural(reach.heaps.length, "heap")} carry it.`;

/** "New tag" for a heap's tags, "New team tag" for a team's; the inputs' names follow it. */
export function NewTagForm({ settings }: { settings: TagSettings }) {
  const create = useCreateTagIn(settings.scope);
  const team = settings.scope.kind === "team";
  const title = team ? "New team tag" : "New tag";
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [reach, setReach] = useState<MergeReach | null>(null);
  const showToast = useToast();
  const body = { name, description: description || null };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (name.trim() === "") return;
    create.mutate(
      { body },
      {
        onSuccess: () => {
          setName("");
          setDescription("");
        },
        onError: (error) => setReach(mergeReachOf(error) ?? null),
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
        {!reach && <TagProblem error={create.error} typed={name} settings={settings} />}
      </form>
      {reach && (
        <MergeConfirm
          mutation={create}
          variables={{ body }}
          act={{ kind: "make", name }}
          reach={reach}
          description={body.description}
          settings={settings}
          onDone={(confirmed) => {
            showToast({ message: `“${name}” is a team tag now: ${carriedBy(confirmed)}` });
            setName("");
            setDescription("");
            setReach(null);
          }}
          onClose={() => setReach(null)}
        />
      )}
    </section>
  );
}

/**
 * A tag as a row: its name, its description and, for those who may edit it, an edit button that turns the row into
 * the form. Those who may not still find the tag's history in the row; editors find it in the form. In a heap, an
 * editor's row also has a ⋯ with Make a team tag; `heap` is the heap the row is in.
 */
export function TagRow({ tag, settings, heap }: { tag: Tag; settings: TagSettings; heap?: HeapSummary }) {
  const historyScope: AuditScope =
    settings.scope.kind === "heap" ? { heap: settings.scope.heap } : { team: settings.scope.team };
  const showEntity = settings.scope.kind === "team";
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
        <HistoryDisclosure scope={historyScope} entityId={tag.id} showEntity={showEntity} className="mt-2" />
      </li>
    );
  }
  return (
    <li className="py-2.5 pr-1 pl-3.5 sm:px-4 sm:py-3.5">
      <div className="flex items-center gap-2 sm:gap-4">
        <div className="flex min-w-0 flex-1 flex-col items-start gap-1.5 sm:flex-row sm:items-center sm:gap-4">
          <span className={tagChipClass(tag.color)}>{tag.name}</span>
          {tag.description ? (
            <span className="min-w-0 text-sm text-ink-soft">{tag.description}</span>
          ) : (
            <span className="text-sm text-muted italic">No description</span>
          )}
        </div>
        {canEdit && heap && settings.scope.kind === "heap" && (
          <MakeTeamTag tag={tag} heap={heap} team={settings.scope.heap.team} teamName={settings.teamName} />
        )}
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
      {!canEdit && <HistoryDisclosure scope={historyScope} entityId={tag.id} showEntity={showEntity} />}
    </li>
  );
}

/** The ⋯ of a heap tag's row: Make a team tag, which asks first when it merges heap tags of the team. */
function MakeTeamTag({ tag, heap, team, teamName }: { tag: Tag; heap: HeapSummary; team: string; teamName: string }) {
  const create = useCreateTagIn({ kind: "team", team });
  const showToast = useToast();
  const [reach, setReach] = useState<MergeReach | null>(null);
  const settings: TagSettings = { scope: { kind: "team", team }, teamName };
  const variables = { body: { name: tag.name, description: tag.description, color: tag.color } };
  const make = () =>
    create.mutate(variables, {
      onError: (error) => {
        const refused = mergeReachOf(error);
        if (refused) setReach(refused);
        else {
          showToast({
            message: tagProblemMessage(error, tag.name, settings),
            error: true,
            requestId: error instanceof ApiProblem ? error.requestId : undefined,
          });
        }
      },
    });

  return (
    <>
      <MenuTrigger>
        <Button aria-label={`More for ${tag.name}`} className={compactButtonClass.icon}>
          <Ellipsis aria-hidden="true" className="size-4" />
        </Button>
        <MenuPopover placement="bottom end">
          <Menu aria-label={`More for ${tag.name}`} className="outline-none">
            <MenuEntry id="make-team-tag" icon={Users} onAction={make}>
              Make a team tag
            </MenuEntry>
          </Menu>
        </MenuPopover>
      </MenuTrigger>
      {reach && (
        <MergeConfirm
          mutation={create}
          variables={variables}
          act={{ kind: "make", name: tag.name }}
          reach={reach}
          description={tag.description ?? null}
          descriptionFrom={heap.name}
          promotingHeapId={heap.id}
          settings={settings}
          onDone={(confirmed) => {
            showToast({ message: `“${tag.name}” is a team tag now: ${carriedBy(confirmed)}` });
            setReach(null);
          }}
          onClose={() => setReach(null)}
        />
      )}
    </>
  );
}

/**
 * The tag's name, description and colour, saved together. Saving, Cancel or Escape close the form; Save with nothing
 * changed just closes it. A newer version from the server (someone else's edit) replaces what's typed.
 */
function TagForm({ tag, settings, onClose }: { tag: Tag; settings: TagSettings; onClose: () => void }) {
  const update = useUpdateTagIn(settings.scope);
  const { teamName } = settings;
  const team = settings.scope.kind === "team";
  const [name, setName] = useState(tag.name);
  const [description, setDescription] = useState(tag.description ?? "");
  const [color, setColor] = useState(tag.color);
  const [shownVersion, setShownVersion] = useState(tag.version);
  // The refused write, as it was sent: a refetch behind the confirm resets the fields, and the confirm must send, and
  // name, what the person asked for, so a newer version is refused as a conflict.
  const [asked, setAsked] = useState<{ reach: MergeReach; variables: typeof variables } | null>(null);
  const showToast = useToast();
  if (shownVersion !== tag.version) {
    setShownVersion(tag.version);
    setName(tag.name);
    setDescription(tag.description ?? "");
    setColor(tag.color);
  }
  const changed = name !== tag.name || description !== (tag.description ?? "") || color !== tag.color;
  const variables = {
    tag,
    patch: {
      ...(name !== tag.name && { name }),
      ...(description !== (tag.description ?? "") && { description: description || null }),
      ...(color !== tag.color && { color }),
    },
  };
  const save = (event: FormEvent) => {
    event.preventDefault();
    if (!changed) {
      onClose();
      return;
    }
    update.mutate(variables, {
      onSuccess: onClose,
      onError: (error) => {
        const reach = mergeReachOf(error);
        setAsked(reach ? { reach, variables } : null);
      },
    });
  };

  return (
    <>
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
        <ColorPicker color={color} onChange={setColor} name={name.trim() || tag.name} />
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
        {!asked && <TagProblem error={update.error} typed={name} settings={settings} />}
      </form>
      {asked && (
        <MergeConfirm
          mutation={update}
          variables={asked.variables}
          act={{ kind: "rename", from: asked.variables.tag.name, to: asked.variables.patch.name ?? "" }}
          reach={asked.reach}
          description={
            asked.variables.patch.description === undefined
              ? (asked.variables.tag.description ?? null)
              : asked.variables.patch.description
          }
          settings={settings}
          onDone={(confirmed) => {
            showToast({ message: `Renamed to “${asked.variables.patch.name ?? ""}”: ${carriedBy(confirmed)}` });
            onClose();
          }}
          onClose={() => setAsked(null)}
        />
      )}
    </>
  );
}

/**
 * The Colour row of the edit form: the palette as round swatches, the picked one ringed, and under them a chip with
 * the name in the picked colour. On a phone the swatches wrap to two rows of four, each a 44 px target.
 */
function ColorPicker({
  color,
  onChange,
  name,
}: {
  color: TagColor;
  onChange: (color: TagColor) => void;
  name: string;
}) {
  return (
    <RadioGroup
      value={color}
      onChange={(value) => onChange(value as TagColor)}
      orientation="horizontal"
      className="flex basis-full flex-col gap-1.5"
    >
      <Label className="text-sm font-medium">Colour</Label>
      <div className="flex gap-1.5 max-sm:grid max-sm:w-fit max-sm:grid-cols-4 max-sm:gap-0">
        {tagColors.map(({ code, label }) => (
          <Radio
            key={code}
            value={code}
            aria-label={label}
            className="group flex size-7 cursor-pointer items-center justify-center rounded-full outline-none max-sm:size-11"
          >
            <span
              aria-hidden="true"
              className={`size-6 rounded-full ${tagDotColor(code)} group-data-[selected]:ring-2 group-data-[selected]:ring-ink group-data-[selected]:ring-offset-2 group-data-[selected]:ring-offset-accent-soft group-data-[focus-visible]:outline-2 group-data-[focus-visible]:outline-offset-2 group-data-[focus-visible]:outline-accent`}
            />
          </Radio>
        ))}
      </div>
      <span className={`${tagChipClass(color)} self-start`}>{name}</span>
    </RadioGroup>
  );
}

/** Why a write was refused. `typed` is the name in the field. */
function tagProblemMessage(error: Error, typed: string, settings: TagSettings): string {
  const { teamName } = settings;
  const team = settings.scope.kind === "team";
  const code = error instanceof ApiProblem ? error.code : undefined;
  if (code === "version_conflict") return "Someone else changed this tag. It now shows their version.";
  if (!team && code === "tag_name_taken") return "This heap already has a tag with that name.";
  if (!team && code === "tag_name_taken_by_team_tag") {
    return `“${typed}” is a team tag of ${teamName}. Use it on items here instead.`;
  }
  if (team && code === "tag_name_taken") return `${teamName} already has a team tag with that name.`;
  return problemMessage(error);
}

/** Why a write was refused, under the field. A merge that needs confirming is the confirm's to show. */
export function TagProblem({ error, typed, settings }: { error: Error | null; typed: string; settings: TagSettings }) {
  if (!error || mergeReachOf(error)) return null;
  return (
    <>
      <p role="alert" className="basis-full text-sm text-overdue-ink">
        {tagProblemMessage(error, typed, settings)}
      </p>
      <RequestId error={error} />
    </>
  );
}
