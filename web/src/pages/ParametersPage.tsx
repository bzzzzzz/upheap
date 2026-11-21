import { Link } from "@tanstack/react-router";
import { ArrowDown, ArrowUp, Ellipsis, List, Plus, Users } from "lucide-react";
import { useState, type FormEvent } from "react";
import { Button, Menu, MenuTrigger } from "react-aria-components";

import type { HeapRef } from "../api/client";
import type {
  ItemTag,
  Parameter,
  ParameterList,
  ParameterMergeReach,
  ParameterOption,
  TeamParameter,
} from "../api/schema";
import { useMe } from "../api/queries/me";
import {
  useArchiveOption,
  useArchiveParameter,
  useChangeParameter,
  useCreateParameter,
  useMakeTeamParameter,
  useParameters,
  useSetUsingTeamParameter,
  type ParameterScope,
} from "../api/queries/parameters";
import { heapTags, useTags } from "../api/queries/tags";
import { HistoryDisclosure } from "../components/AuditHistory";
import { compactButtonClass } from "../components/buttons";
import { MenuEntry, MenuPopover } from "../components/Menu";
import { NameForm } from "../components/NameForm";
import { ProblemText } from "../components/ProblemText";
import { TagPicker } from "../components/TagPicker";
import { TeamMark } from "../components/TeamMark";
import { teamName } from "../components/teamName";
import { useToast } from "../components/UndoToast";
import { useConfirmation, useReturnFocus } from "../components/useConfirmation";
import { listOf, plural } from "../components/words";
import {
  cardClass,
  cardTitleClass,
  clampWeight,
  inputClass,
  introClass,
  labelClass,
  nameFormClasses,
} from "./settingsForm";
import { RequestId } from "../components/RequestId";
import { keptValues, ParameterMergeConfirm, parameterMergeReachOf } from "./ParameterMergeConfirm";

const problemClass = "text-overdue-ink";

/** A parameter as a card shows it: a heap's (which may be its team's, in use there) or, on the Team page, a team's. */
type CardParameter = Parameter | TeamParameter;

/** The history of what the scope owns. */
const historyScope = (scope: ParameterScope) => (scope.kind === "heap" ? { heap: scope.heap } : { team: scope.team });
/** An option's place in the list, in the parameter colour. */
const rankClass =
  "inline-flex size-[22px] shrink-0 items-center justify-center rounded-md bg-param-soft text-xs font-semibold text-param-ink max-sm:size-6";

/**
 * Manage parameters (product.md, Manage parameters). Every member sees the parameters, since they set their values on
 * items; only admins get the controls, as `allowed_actions` says. Parameters and options are archived, never deleted.
 */
export function ParametersPage({ heap: heapRef }: { heap: HeapRef }) {
  const parameters = useParameters(heapRef);
  const scope: ParameterScope = { kind: "heap", heap: heapRef };
  const [adding, setAdding] = useState(false);
  const { ref: newButton, returnFocus } = useReturnFocus<HTMLButtonElement>(adding);

  // A failed refetch keeps the data, so what is on screen (and any draft in it) stays; the error shows only without it.
  if (!parameters.data) {
    if (parameters.isError) {
      return (
        <>
          <p role="alert" className="py-16 text-center text-overdue-ink">
            {parameters.error.message}
          </p>
          <RequestId error={parameters.error} className="justify-center" />
        </>
      );
    }
    return <p className="py-16 text-center text-muted">Loading…</p>;
  }

  const { heap, allowed_actions, archived } = parameters.data;
  const canCreate = allowed_actions.includes("create");
  const closeForm = () => {
    returnFocus();
    setAdding(false);
  };
  return (
    <section className="flex flex-col gap-4 sm:gap-5">
      <h2 className="sr-only">Parameters</h2>
      <div className="flex flex-wrap items-start justify-between gap-3 sm:gap-4">
        <p className={`${introClass} sm:max-w-[520px]`}>
          Parameters rank the items of {heap.name} next to need, age, deadline and value. Order each parameter's options
          from highest priority to lowest; the weight sets how much the parameter counts overall.
        </p>
        {canCreate && !adding && (
          <button
            ref={newButton}
            type="button"
            onClick={() => setAdding(true)}
            className={`${compactButtonClass.bordered} max-sm:w-full`}
          >
            <Plus aria-hidden="true" className="size-4" />
            New parameter
          </button>
        )}
      </div>
      {canCreate && adding && <NewParameterForm scope={scope} onClose={closeForm} />}
      {parameters.data.parameters.length === 0 ? (
        <p className="py-12 text-center text-muted">No parameters yet.</p>
      ) : (
        <ul className="flex flex-col gap-4 sm:gap-5">
          {parameters.data.parameters.map((parameter) => (
            <ParameterCard key={parameter.id} parameter={parameter} scope={scope} />
          ))}
        </ul>
      )}
      <TeamParametersCard heapRef={heapRef} list={parameters.data} />
      {archived.length > 0 && (
        <section aria-labelledby="archived-parameters" className="mt-2">
          <h3 id="archived-parameters" className="mb-2 text-sm font-medium text-muted">
            Archived
          </h3>
          <ul className="flex flex-col gap-2">
            {archived.map((parameter) => (
              <li
                key={parameter.id}
                className="rounded-xl border border-line px-3.5 py-2 text-sm text-ink-soft sm:px-4"
              >
                <div className="flex flex-wrap items-baseline gap-x-2 py-1.5">
                  <span className="font-medium text-ink">{parameter.name}</span>
                  <span>{parameter.options.map((option) => option.label).join(", ")}</span>
                </div>
                {parameter.allowed_actions.includes("view_history") && (
                  <HistoryDisclosure scope={{ heap: heapRef }} entityId={parameter.id} className="" />
                )}
              </li>
            ))}
          </ul>
        </section>
      )}
    </section>
  );
}

/** The New parameter card. A team's parameter has no weight of its own: each heap weights it. */
export function NewParameterForm({ scope, onClose }: { scope: ParameterScope; onClose: () => void }) {
  const create = useCreateParameter(scope);
  const [name, setName] = useState("");
  const [weight, setWeight] = useState(10);
  const [options, setOptions] = useState("");

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (name.trim() === "") return;
    const labels = options
      .split("\n")
      .map((label) => label.trim())
      .filter(Boolean);
    create.mutate({ name, weight, options: labels }, { onSuccess: onClose });
  };

  return (
    <section aria-labelledby="new-parameter" className={cardClass}>
      <h3 id="new-parameter" className={`${cardTitleClass} mb-3`}>
        {scope.kind === "team" ? "New team parameter" : "New parameter"}
      </h3>
      <form
        onSubmit={submit}
        onKeyDown={(e) => {
          if (e.key !== "Escape") return;
          e.preventDefault();
          onClose();
        }}
        className="flex flex-col gap-3"
      >
        <div className="flex flex-wrap items-end gap-2.5">
          <label className="flex min-w-0 flex-[1_1_220px] flex-col gap-1.5 text-sm font-medium">
            Name
            <input
              aria-label="New parameter name"
              value={name}
              autoFocus
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. Customer"
              maxLength={50}
              className={`${inputClass} font-normal`}
            />
          </label>
          {scope.kind === "heap" && (
            <WeightInput value={weight} onChange={setWeight} label="Weight of the new parameter" />
          )}
        </div>
        <label className="flex flex-col gap-1.5 text-sm font-medium">
          <span>
            Options <span className="font-normal text-muted">one per line, highest priority first</span>
          </span>
          <textarea
            aria-label="Options of the new parameter, one per line, highest priority first"
            value={options}
            onChange={(e) => setOptions(e.target.value)}
            placeholder={"Enterprise\nPro\nFree"}
            rows={3}
            className={`${inputClass} font-normal`}
          />
        </label>
        <div className="flex flex-wrap items-center gap-2">
          <button
            type="submit"
            disabled={create.isPending || name.trim() === ""}
            className={`${compactButtonClass.primary} max-sm:flex-1`}
          >
            Add parameter
          </button>
          <button type="button" onClick={onClose} className={compactButtonClass.quiet}>
            Cancel
          </button>
          <ProblemText error={create.error} className={problemClass} />
        </div>
      </form>
    </section>
  );
}

function WeightInput({ value, onChange, label }: { value: number; onChange: (value: number) => void; label: string }) {
  return (
    <label className="flex flex-col gap-1.5 text-sm font-medium">
      Weight
      <input
        type="number"
        min={0}
        max={100}
        aria-label={label}
        value={value}
        onChange={(e) => onChange(clampWeight(e.target.value))}
        className={`${inputClass} w-20 font-normal`}
      />
    </label>
  );
}

/**
 * One parameter: its name with Rename, its weight, its options, the tags that suggest it, and at the foot its history
 * and Archive parameter. Those without `edit` read the name, weight, options and tags. A team's parameter in a heap
 * (`kind` team) is read there, with its weight, and edited on the Team page; the heap may stop using it. On the Team
 * page, a team's parameter has no weight, and says which heaps use it.
 */
export function ParameterCard({ parameter, scope }: { parameter: CardParameter; scope: ParameterScope }) {
  const rename = useChangeParameter(scope);
  const weigh = useChangeParameter(scope);
  const canEdit = parameter.allowed_actions.includes("edit");
  const [renaming, setRenaming] = useState(false);
  const { ref: renameButton, returnFocus } = useReturnFocus<HTMLButtonElement>(renaming);
  const stopRenaming = () => {
    returnFocus();
    setRenaming(false);
    // Opened again, the form starts without the last attempt's error.
    rename.reset();
  };
  const headingId = `parameter-${parameter.id}`;
  const canArchive = parameter.allowed_actions.includes("archive");
  const canViewHistory = parameter.allowed_actions.includes("view_history");
  const canStopUsing = parameter.allowed_actions.includes("stop_using");
  const canMakeTeam = parameter.allowed_actions.includes("make_team");
  const inHeap = "weight" in parameter ? parameter : null;
  const usedTeam = inHeap?.kind === "team" && scope.kind === "heap";
  const usedBy = "heaps" in parameter ? parameter.heaps : null;

  return (
    <li aria-labelledby={headingId} className={cardClass}>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
        {/* The weight stays put while the name is being changed, so a weight not yet saved isn't lost. */}
        {renaming && canEdit ? (
          <>
            <h3 id={headingId} className="sr-only">
              {parameter.name}
            </h3>
            <NameForm
              label={`Name of parameter ${parameter.name}`}
              value={parameter.name}
              maxLength={50}
              pending={rename.isPending}
              error={rename.error}
              onSave={(name) =>
                rename.mutate({ parameter, change: { kind: "patch", patch: { name } } }, { onSuccess: stopRenaming })
              }
              onCancel={stopRenaming}
              autoFocus
              className="font-semibold"
              classes={nameFormClasses}
            />
          </>
        ) : (
          <h3 id={headingId} className="min-w-0 text-[17px] font-semibold break-words max-sm:flex-1 sm:text-base">
            {parameter.name}
            {usedTeam && <TeamMark of="parameter" />}
          </h3>
        )}
        {inHeap && (
          <div className="max-sm:order-last max-sm:basis-full">
            {canEdit && scope.kind === "heap" ? (
              <WeightForm
                parameter={inHeap}
                pending={weigh.isPending}
                onSave={(weight) => weigh.mutate({ parameter, change: { kind: "patch", patch: { weight } } })}
                // Cancelled, the weight is back to the saved one, so the last attempt's error no longer applies.
                onCancel={() => weigh.reset()}
              />
            ) : (
              <span className="inline-flex h-7 items-center rounded-full bg-accent-soft px-2.5 text-[13px] font-medium text-accent-ink">
                Weight {inHeap.weight}
              </span>
            )}
          </div>
        )}
        {!renaming && (
          <>
            <span className="flex-1 max-sm:hidden" />
            {canEdit && (
              <button
                ref={renameButton}
                type="button"
                onClick={() => setRenaming(true)}
                aria-label={`Rename ${parameter.name}`}
                className={compactButtonClass.quiet}
              >
                Rename
              </button>
            )}
            {canMakeTeam && scope.kind === "heap" && <MakeTeamParameter parameter={parameter} heapRef={scope.heap} />}
          </>
        )}
      </div>
      <ProblemText error={weigh.error} className={problemClass} />
      {usedBy && (
        <p className="mt-1.5 text-sm break-words text-ink-soft">
          {usedBy.length === 0 ? "Not used in any heap yet" : `Used in ${listOf(usedBy.map((heap) => heap.name))}`}
        </p>
      )}
      {usedTeam && scope.kind === "heap" && (
        <p className="mt-1.5 text-sm text-ink-soft">
          Its weight is set with the heap's other weights, on the{" "}
          <Link to="/teams/$team/heaps/$heap/settings/ranking" params={scope.heap} className="underline hover:text-ink">
            Ranking page
          </Link>
          . Its name and options are edited on the{" "}
          <Link
            to="/teams/$team"
            params={{ team: scope.heap.team }}
            hash="team-parameters"
            className="underline hover:text-ink"
          >
            Team page
          </Link>
          .
        </p>
      )}

      <div className="mt-[18px] flex flex-col gap-[18px] sm:flex-row sm:flex-wrap sm:gap-8">
        <OptionsEditor
          parameter={parameter}
          scope={scope}
          canEdit={canEdit}
          heapNames={usedBy?.map((heap) => heap.name)}
        />
        {scope.kind === "heap" && <LinkedTags parameter={parameter} heapRef={scope.heap} canEdit={canEdit} />}
      </div>

      {(canViewHistory || canArchive || (canStopUsing && scope.kind === "heap")) && (
        <div className="mt-3.5 flex flex-wrap items-start gap-2 border-t border-line pt-1.5 sm:mt-[18px] sm:pt-3">
          {canViewHistory && (
            <HistoryDisclosure scope={historyScope(scope)} entityId={parameter.id} className="min-w-0 flex-1" />
          )}
          {canStopUsing && scope.kind === "heap" && <StopUsing parameter={parameter} heapRef={scope.heap} />}
          {canArchive && <ArchiveParameter parameter={parameter} scope={scope} />}
        </div>
      )}
    </li>
  );
}

/**
 * The ⋯ of a heap parameter's card: Make a team parameter, which asks first. The first try is sent without a token, and
 * the refusal, with the reach, opens the confirm. Any other refusal, such as the linked tags that stop a merge, shows
 * under the card's heading as a failed archive does.
 */
function MakeTeamParameter({ parameter, heapRef }: { parameter: CardParameter; heapRef: HeapRef }) {
  const make = useMakeTeamParameter(heapRef);
  const showToast = useToast();
  const [reach, setReach] = useState<ParameterMergeReach | null>(null);
  const ask = () =>
    make.mutate(
      { parameterId: parameter.id },
      {
        onError: (error) => setReach(parameterMergeReachOf(error) ?? null),
        onSuccess: () => showToast({ message: `“${parameter.name}” is a team parameter already.` }),
      },
    );
  const label = `More for ${parameter.name}`;
  return (
    <>
      <MenuTrigger>
        <Button aria-label={label} className={compactButtonClass.icon}>
          <Ellipsis aria-hidden="true" className="size-4" />
        </Button>
        <MenuPopover placement="bottom end">
          <Menu aria-label={label} className="outline-none">
            <MenuEntry id="make-team-parameter" icon={Users} onAction={ask}>
              Make a team parameter
            </MenuEntry>
          </Menu>
        </MenuPopover>
      </MenuTrigger>
      {!reach && <ProblemText error={parameterMergeReachOf(make.error) ? null : make.error} className={problemClass} />}
      {reach && (
        <ParameterMergeConfirm
          mutation={make}
          parameterId={parameter.id}
          name={parameter.name}
          reach={reach}
          onDone={(confirmed) => {
            showToast({ message: `“${parameter.name}” is a team parameter now: ${keptValues(confirmed)}` });
            setReach(null);
          }}
          onClose={() => setReach(null)}
        />
      )}
    </>
  );
}

/** Stop using: the heap stops counting its team's parameter, which it can use again with the weight it had. */
function StopUsing({ parameter, heapRef }: { parameter: CardParameter; heapRef: HeapRef }) {
  const stop = useSetUsingTeamParameter(heapRef, false);
  return (
    <div className="ml-auto flex flex-wrap items-center justify-end gap-2 text-sm">
      <button
        type="button"
        disabled={stop.isPending}
        onClick={() => stop.mutate(parameter.id)}
        aria-label={`Stop using ${parameter.name}`}
        className={compactButtonClass.quiet}
      >
        Stop using
      </button>
      <ProblemText error={stop.error} className={`${problemClass} text-right`} />
    </div>
  );
}

/** The weight, saved on its own once it changes. A newer version from the server resets it. */
function WeightForm({
  parameter,
  pending,
  onSave,
  onCancel,
}: {
  parameter: Parameter;
  pending: boolean;
  onSave: (weight: number) => void;
  onCancel: () => void;
}) {
  const [weight, setWeight] = useState(parameter.weight);
  // Reseeded when the saved weight changes, not on every new version, so a rename doesn't drop a weight being typed.
  const [shownWeight, setShownWeight] = useState(parameter.weight);
  if (shownWeight !== parameter.weight) {
    setShownWeight(parameter.weight);
    setWeight(parameter.weight);
  }
  return (
    <form
      className="flex items-center gap-2"
      onSubmit={(event) => {
        event.preventDefault();
        if (weight !== parameter.weight) onSave(weight);
      }}
    >
      <label className="inline-flex items-center gap-2 rounded-full bg-accent-soft py-0.5 pr-0.5 pl-2.5 text-[13px] font-medium text-accent-ink">
        Weight
        <input
          type="number"
          min={0}
          max={100}
          aria-label={`Weight of parameter ${parameter.name}`}
          value={weight}
          onChange={(e) => setWeight(Math.max(0, Math.min(100, Math.round(Number(e.target.value) || 0))))}
          className="h-7 w-16 rounded-full border border-line bg-surface px-2.5 text-sm text-ink pointer-coarse:h-10"
        />
      </label>
      {weight !== parameter.weight && (
        <>
          <button type="submit" disabled={pending} className={compactButtonClass.primary}>
            Save
          </button>
          <button
            type="button"
            onClick={() => {
              setWeight(parameter.weight);
              onCancel();
            }}
            className={compactButtonClass.quiet}
          >
            Cancel
          </button>
        </>
      )}
    </form>
  );
}

/**
 * The options in use, highest priority first. Labels and order are edited together and saved as one arrangement;
 * a new option is added last. Archiving an option asks which option open items should switch to.
 */
function OptionsEditor({
  parameter,
  scope,
  canEdit,
  heapNames,
}: {
  parameter: CardParameter;
  scope: ParameterScope;
  canEdit: boolean;
  /** The heaps a team's parameter is used in, which archiving an option names. */
  heapNames: string[] | undefined;
}) {
  const change = useChangeParameter(scope);
  const [options, setOptions] = useState<ParameterOption[]>(parameter.options);
  const [newLabel, setNewLabel] = useState("");
  // Reseeded when the saved options change (added, archived, relabelled or reordered), not on every new version, so
  // saving the weight, a rename or a tag link doesn't drop an arrangement not yet saved.
  const saved = JSON.stringify(parameter.options);
  const [shownOptions, setShownOptions] = useState(saved);
  if (shownOptions !== saved) {
    setShownOptions(saved);
    setOptions(parameter.options);
  }
  // Labels are saved trimmed, so a space before or after one isn't a change to save.
  const changed =
    options.length !== parameter.options.length ||
    options.some((option, index) => {
      const savedOption = parameter.options[index];
      return option.id !== savedOption?.id || option.label.trim() !== savedOption.label.trim();
    });

  const move = (index: number, by: -1 | 1) => {
    const next = [...options];
    const [moved] = next.splice(index, 1);
    if (moved) next.splice(index + by, 0, moved);
    setOptions(next);
  };

  const add = (event: FormEvent) => {
    event.preventDefault();
    if (newLabel.trim() === "") return;
    change.mutate({ parameter, change: { kind: "add_option", label: newLabel } }, { onSuccess: () => setNewLabel("") });
  };

  return (
    <div className="min-w-0 sm:flex-[1_1_300px]">
      <h4 className={`${labelClass} mb-1 sm:mb-1.5`}>Options, highest priority first</h4>
      {options.length === 0 && <p className="py-1.5 text-sm text-muted">No options yet.</p>}
      <ol className="flex flex-col max-sm:divide-y max-sm:divide-line">
        {options.map((option, index) => (
          <OptionRow
            key={option.id}
            option={option}
            index={index}
            canEdit={canEdit}
            others={options.filter((o) => o.id !== option.id)}
            canArchive={!changed && options.length > 1}
            isLast={index === options.length - 1}
            scope={scope}
            heapNames={heapNames}
            // The link filters by the option's id, so it names the option as saved while its label is being edited.
            linkLabel={`Items with ${parameter.name}: ${parameter.options.find((o) => o.id === option.id)?.label ?? option.label}`}
            onLabel={(label) => setOptions(options.map((o) => (o.id === option.id ? { ...o, label } : o)))}
            onMove={(by) => move(index, by)}
          />
        ))}
      </ol>
      {canEdit && changed && (
        <div className="mt-2 flex items-center gap-2">
          <button
            type="button"
            disabled={change.isPending || options.some((o) => o.label.trim() === "")}
            onClick={() => change.mutate({ parameter, change: { kind: "arrange_options", options } })}
            className={`${compactButtonClass.primary} max-sm:flex-1`}
          >
            Save options
          </button>
          <button type="button" onClick={() => setOptions(parameter.options)} className={compactButtonClass.quiet}>
            Cancel
          </button>
        </div>
      )}
      {canEdit && !changed && (
        <form onSubmit={add} className="mt-2 flex gap-2 sm:mt-2.5">
          <input
            aria-label={`New option of ${parameter.name}`}
            value={newLabel}
            onChange={(e) => setNewLabel(e.target.value)}
            placeholder="New option, added last"
            maxLength={50}
            className={`${inputClass} flex-1`}
          />
          <button
            type="submit"
            disabled={change.isPending || newLabel.trim() === ""}
            className={compactButtonClass.bordered}
          >
            Add option
          </button>
        </form>
      )}
      <ProblemText error={change.error} className={problemClass} />
    </div>
  );
}

/**
 * An option: its place, its label (an input that reads as text, for those who may edit it), ↑ ↓ and Archive. Archive
 * turns the row into the confirm, which asks which option open items switch to.
 */
function OptionRow({
  option,
  index,
  canEdit,
  others,
  canArchive,
  isLast,
  scope,
  heapNames,
  linkLabel,
  onLabel,
  onMove,
}: {
  option: ParameterOption;
  index: number;
  canEdit: boolean;
  others: ParameterOption[];
  canArchive: boolean;
  isLast: boolean;
  scope: ParameterScope;
  /** The heaps a team's parameter is used in, named by the confirm of archiving an option. */
  heapNames: string[] | undefined;
  /** The accessible name and tooltip of the link to the list filtered by this option. */
  linkLabel: string;
  onLabel: (label: string) => void;
  onMove: (by: -1 | 1) => void;
}) {
  const archive = useArchiveOption(scope);
  const { confirming, ask, cancel, dismiss, buttonRef } = useConfirmation();
  const [picked, setReplacement] = useState("");
  // Editing the options takes Archive away, and the confirm with it: it doesn't come back, stealing focus, once the
  // edit is cancelled.
  if (confirming && !canArchive) dismiss();
  // Options come and go while the row stays mounted, so the pick falls back to the first other option.
  const replacement = others.some((other) => other.id === picked) ? picked : (others[0]?.id ?? "");
  const rank = <span className={rankClass}>{index + 1}</span>;
  // A heap's list can be filtered by the option; a team's parameter is in many heaps, so there is no one list.
  const toList =
    scope.kind === "heap"
      ? ({ to: "/teams/$team/heaps/$heap", params: scope.heap, search: { param: option.id } } as const)
      : null;

  if (confirming) {
    return (
      <li className="my-1 rounded-xl border border-overdue-ink/25 bg-overdue-soft p-3 text-sm">
        <div className="flex items-center gap-2">
          {rank}
          <span className="font-medium">Archive {option.label}?</span>
        </div>
        <div className="mt-2.5 flex flex-wrap items-center gap-2">
          <label className="flex flex-wrap items-center gap-1.5 text-ink-soft max-sm:w-full max-sm:flex-col max-sm:items-stretch">
            Open items {heapNames && heapNames.length > 0 ? `in ${listOf(heapNames)} ` : ""}with {option.label} switch
            to
            <select
              autoFocus
              value={replacement}
              onChange={(e) => setReplacement(e.target.value)}
              className={`${inputClass} py-1.5`}
            >
              {others.map((other) => (
                <option key={other.id} value={other.id}>
                  {other.label}
                </option>
              ))}
            </select>
          </label>
          <button
            type="button"
            disabled={archive.isPending || replacement === ""}
            onClick={() => archive.mutate({ optionId: option.id, replacementId: replacement }, { onError: cancel })}
            className={`${compactButtonClass.danger} max-sm:flex-1`}
          >
            Archive
          </button>
          <button type="button" onClick={cancel} className={compactButtonClass.quiet}>
            Cancel
          </button>
        </div>
      </li>
    );
  }
  if (!canEdit) {
    if (!toList) {
      return (
        <li className="flex items-center gap-2 py-1.5 text-sm font-medium text-param-ink">
          {rank}
          {option.label}
        </li>
      );
    }
    return (
      <li>
        <Link
          {...toList}
          aria-label={linkLabel}
          className="flex items-center gap-2 py-1.5 text-sm font-medium text-param-ink hover:underline max-sm:min-h-11 max-sm:py-2.5"
        >
          {rank}
          {option.label}
        </Link>
      </li>
    );
  }
  return (
    <li className="flex flex-wrap items-center gap-1 py-1 sm:gap-2">
      {rank}
      <input
        aria-label={`Label of option ${index + 1}`}
        value={option.label}
        maxLength={50}
        onChange={(e) => onLabel(e.target.value)}
        className="min-w-0 flex-1 rounded-lg border border-transparent bg-transparent px-2 py-1.5 text-sm font-medium text-ink hover:border-line focus:border-line-strong focus:bg-surface pointer-coarse:min-h-11"
      />
      {toList && (
        <Link {...toList} aria-label={linkLabel} title={linkLabel} className={compactButtonClass.icon}>
          <List aria-hidden="true" className="size-4" />
        </Link>
      )}
      <button
        type="button"
        disabled={index === 0}
        onClick={() => onMove(-1)}
        aria-label={`Move ${option.label} up`}
        className={compactButtonClass.icon}
      >
        <ArrowUp aria-hidden="true" className="size-4" />
      </button>
      <button
        type="button"
        disabled={isLast}
        onClick={() => onMove(1)}
        aria-label={`Move ${option.label} down`}
        className={compactButtonClass.icon}
      >
        <ArrowDown aria-hidden="true" className="size-4" />
      </button>
      {canArchive && (
        <button ref={buttonRef} type="button" onClick={ask} className={`${compactButtonClass.quiet} px-2 text-[13px]`}>
          Archive
        </button>
      )}
      <ProblemText error={archive.error} className={problemClass} />
    </li>
  );
}

/** The tags whose items the parameter is suggested for. Each change is saved at once. */
function LinkedTags({ parameter, heapRef, canEdit }: { parameter: CardParameter; heapRef: HeapRef; canEdit: boolean }) {
  const tags = useTags(heapRef);
  const change = useChangeParameter({ kind: "heap", heap: heapRef });
  const heading = <h4 className={labelClass}>Suggested by tags</h4>;

  // Until the tags load, the linked ones can't be named, and a pick would replace the links rather than add to them.
  if (!tags.data) {
    if (!canEdit && parameter.tag_ids.length === 0) return null;
    return (
      <div className="min-w-0 sm:flex-[1_1_220px]">
        {heading}
        {tags.isError ? (
          <ProblemText error={tags.error} className={`${problemClass} mt-1.5`} />
        ) : (
          <p className="mt-1.5 text-sm text-muted">Loading tags…</p>
        )}
      </div>
    );
  }
  const linked: ItemTag[] = heapTags(tags.data)
    .filter((tag) => parameter.tag_ids.includes(tag.id))
    .map((tag) => ({ id: tag.id, name: tag.name }));

  if (!canEdit) {
    if (linked.length === 0) return null;
    return (
      <div className="min-w-0 sm:flex-[1_1_220px]">
        {heading}
        <ul className="mt-1.5 flex flex-wrap gap-1.5" aria-label="Suggested by tags">
          {linked.map((tag) => (
            <li
              key={tag.id}
              className="inline-flex h-[26px] items-center rounded-full border border-line bg-sunken px-2.5 text-[13px] font-medium"
            >
              {tag.name}
            </li>
          ))}
        </ul>
        <p className="mt-2 text-[13px]/[1.45] text-muted">
          Adding one of these tags to an item suggests {parameter.name}.
        </p>
      </div>
    );
  }
  return (
    <div className="min-w-0 sm:flex-[1_1_220px]">
      <TagPicker
        heapRef={heapRef}
        value={linked}
        disabled={change.isPending}
        label={heading}
        inputLabel={`Tags that suggest ${parameter.name}`}
        onChange={(next) => change.mutate({ parameter, change: { kind: "link_tags", tagIds: next.map((t) => t.id) } })}
      />
      <p className="mt-2 text-[13px]/[1.45] text-muted">
        {parameter.tag_ids.length > 0
          ? `Adding one of these tags to an item suggests ${parameter.name}.`
          : `No tags suggest ${parameter.name} yet.`}
      </p>
      <ProblemText error={change.error} className={problemClass} />
    </div>
  );
}

function ArchiveParameter({ parameter, scope }: { parameter: CardParameter; scope: ParameterScope }) {
  const archive = useArchiveParameter(scope);
  const { confirming, ask, cancel, buttonRef } = useConfirmation();
  return (
    <div className="ml-auto flex flex-wrap items-center justify-end gap-2 text-sm">
      {confirming ? (
        <>
          <span className="text-ink-soft">
            Archive {parameter.name}?{" "}
            {scope.kind === "team"
              ? "It stops counting in the ranking of every heap that uses it; closed items keep their values."
              : "It stops counting in the ranking; closed items keep their values."}
          </span>
          <button
            type="button"
            autoFocus
            disabled={archive.isPending}
            onClick={() => archive.mutate(parameter, { onError: cancel })}
            className={compactButtonClass.danger}
          >
            Archive
          </button>
          <button type="button" onClick={cancel} className={compactButtonClass.quiet}>
            Cancel
          </button>
        </>
      ) : (
        <button
          ref={buttonRef}
          type="button"
          onClick={ask}
          className={`${compactButtonClass.quietDanger} max-sm:pr-0 max-sm:hover:bg-transparent`}
        >
          Archive parameter
        </button>
      )}
      <ProblemText error={archive.error} className={`${problemClass} text-right`} />
    </div>
  );
}

/**
 * The team's other parameters in use, which this heap doesn't use: each with its options and Use, for those who may
 * (`use_team_parameter`), and a link to the Team page, where they are made and edited.
 */
function TeamParametersCard({ heapRef, list }: { heapRef: HeapRef; list: ParameterList }) {
  const { data: me } = useMe();
  const team = me?.teams.find((t) => t.slug === heapRef.team);
  const name = me && team ? teamName(team, me) : heapRef.team;
  const available = list.team_available;
  const canUse = list.allowed_actions.includes("use_team_parameter");
  const teamPage = (
    <Link
      to="/teams/$team"
      params={{ team: heapRef.team }}
      hash="team-parameters"
      className="text-ink-soft underline hover:text-ink"
    >
      Team page
    </Link>
  );
  return (
    <section
      aria-labelledby="team-parameters-card"
      className="overflow-hidden rounded-[14px] border border-line bg-surface"
    >
      <div className="flex items-baseline justify-between gap-3 px-3.5 pt-3.5 pb-2.5 sm:px-4 sm:pt-4 sm:pb-3">
        <h3 id="team-parameters-card" className={cardTitleClass}>
          {plural(available.length, "parameter")} of the team {name}
        </h3>
        {/* With none, the line below links there instead. */}
        {available.length > 0 && <span className="shrink-0 text-sm">{teamPage}</span>}
      </div>
      {available.length === 0 ? (
        <p className="border-t border-line px-3.5 py-3 text-sm text-muted sm:px-4">
          {name} has no other team parameters. Make them on the {teamPage}.
        </p>
      ) : (
        <ul className="divide-y divide-line border-t border-line">
          {available.map((parameter) => (
            <AvailableParameter key={parameter.id} parameter={parameter} heapRef={heapRef} canUse={canUse} />
          ))}
        </ul>
      )}
    </section>
  );
}

/** A team parameter the heap could use: its name, its options, and Use. */
function AvailableParameter({
  parameter,
  heapRef,
  canUse,
}: {
  parameter: TeamParameter;
  heapRef: HeapRef;
  canUse: boolean;
}) {
  const use = useSetUsingTeamParameter(heapRef, true);
  return (
    <li className="flex flex-wrap items-center gap-x-4 gap-y-1.5 px-3.5 py-2.5 sm:px-4 sm:py-3.5">
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="font-medium break-words">{parameter.name}</span>
        <span className="text-sm break-words text-ink-soft">
          {parameter.options.length === 0 ? "No options yet" : parameter.options.map((o) => o.label).join(", ")}
        </span>
      </div>
      {canUse && (
        <button
          type="button"
          disabled={use.isPending}
          onClick={() => use.mutate(parameter.id)}
          aria-label={`Use ${parameter.name}`}
          className={compactButtonClass.bordered}
        >
          Use
        </button>
      )}
      <ProblemText error={use.error} className={`${problemClass} basis-full`} />
    </li>
  );
}
