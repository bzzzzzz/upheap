import { ArrowDown, ArrowUp, Plus } from "lucide-react";
import { useState, type FormEvent } from "react";

import type { HeapRef, ItemTag, Parameter, ParameterOption } from "../api/client";
import {
  useArchiveOption,
  useArchiveParameter,
  useChangeParameter,
  useCreateParameter,
  useParameters,
  useTags,
} from "../api/queries";
import { HistoryDisclosure } from "../components/AuditHistory";
import { compactButtonClass } from "../components/buttons";
import { NameForm } from "../components/NameForm";
import { ProblemText } from "../components/ProblemText";
import { TagPicker } from "../components/TagPicker";
import { useConfirmation, useReturnFocus } from "../components/useConfirmation";
import { cardClass, cardTitleClass, inputClass, introClass, labelClass, nameFormClasses } from "./settingsForm";

const problemClass = "text-overdue-ink";
/** An option's place in the list, in the parameter colour. */
const rankClass =
  "inline-flex size-[22px] shrink-0 items-center justify-center rounded-md bg-param-soft text-xs font-semibold text-param-ink max-sm:size-6";

/**
 * Manage parameters (Req Views: Manage parameters). Every member sees the parameters, since they set their values on
 * items; only admins get the controls, as `allowed_actions` says. Parameters and options are archived, never deleted.
 */
export function ParametersPage({ heap: heapRef }: { heap: HeapRef }) {
  const parameters = useParameters(heapRef);
  const [adding, setAdding] = useState(false);
  const { ref: newButton, returnFocus } = useReturnFocus<HTMLButtonElement>(adding);

  if (parameters.isPending) {
    return <p className="py-16 text-center text-muted">Loading…</p>;
  }
  if (parameters.isError) {
    return (
      <p role="alert" className="py-16 text-center text-overdue-ink">
        {parameters.error.message}
      </p>
    );
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
      {canCreate && adding && <NewParameterForm heapRef={heapRef} onClose={closeForm} />}
      {parameters.data.parameters.length === 0 ? (
        <p className="py-12 text-center text-muted">No parameters yet.</p>
      ) : (
        <ul className="flex flex-col gap-4 sm:gap-5">
          {parameters.data.parameters.map((parameter) => (
            <ParameterCard key={parameter.id} parameter={parameter} heapRef={heapRef} />
          ))}
        </ul>
      )}
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

function NewParameterForm({ heapRef, onClose }: { heapRef: HeapRef; onClose: () => void }) {
  const create = useCreateParameter(heapRef);
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
        New parameter
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
          <WeightInput value={weight} onChange={setWeight} label="Weight of the new parameter" />
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
        onChange={(e) => onChange(Math.max(0, Math.min(100, Math.round(Number(e.target.value) || 0))))}
        className={`${inputClass} w-20 font-normal`}
      />
    </label>
  );
}

/**
 * One parameter: its name with Rename, its weight, its options, the tags that suggest it, and at the foot its history
 * and Archive parameter. Those without `edit` read the name, weight, options and tags.
 */
function ParameterCard({ parameter, heapRef }: { parameter: Parameter; heapRef: HeapRef }) {
  const rename = useChangeParameter(heapRef);
  const weigh = useChangeParameter(heapRef);
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
          </h3>
        )}
        <div className="max-sm:order-last max-sm:basis-full">
          {canEdit ? (
            <WeightForm
              parameter={parameter}
              pending={weigh.isPending}
              onSave={(weight) => weigh.mutate({ parameter, change: { kind: "patch", patch: { weight } } })}
            />
          ) : (
            <span className="inline-flex h-7 items-center rounded-full bg-accent-soft px-2.5 text-[13px] font-medium text-accent-ink">
              Weight {parameter.weight}
            </span>
          )}
        </div>
        {!renaming && (
          <>
            <span className="flex-1 max-sm:hidden" />
            {canEdit && (
              <button
                ref={renameButton}
                type="button"
                onClick={() => setRenaming(true)}
                className={compactButtonClass.quiet}
              >
                Rename
              </button>
            )}
          </>
        )}
      </div>
      <ProblemText error={weigh.error} className={problemClass} />

      <div className="mt-[18px] flex flex-col gap-[18px] sm:flex-row sm:flex-wrap sm:gap-8">
        <OptionsEditor parameter={parameter} heapRef={heapRef} canEdit={canEdit} />
        <LinkedTags parameter={parameter} heapRef={heapRef} canEdit={canEdit} />
      </div>

      {(canViewHistory || canArchive) && (
        <div className="mt-3.5 flex flex-wrap items-start gap-2 border-t border-line pt-1.5 sm:mt-[18px] sm:pt-3">
          {canViewHistory && (
            <HistoryDisclosure scope={{ heap: heapRef }} entityId={parameter.id} className="min-w-0 flex-1" />
          )}
          {canArchive && <ArchiveParameter parameter={parameter} heapRef={heapRef} />}
        </div>
      )}
    </li>
  );
}

/** The weight, saved on its own once it changes. A newer version from the server resets it. */
function WeightForm({
  parameter,
  pending,
  onSave,
}: {
  parameter: Parameter;
  pending: boolean;
  onSave: (weight: number) => void;
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
          <button type="button" onClick={() => setWeight(parameter.weight)} className={compactButtonClass.quiet}>
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
function OptionsEditor({ parameter, heapRef, canEdit }: { parameter: Parameter; heapRef: HeapRef; canEdit: boolean }) {
  const change = useChangeParameter(heapRef);
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
  const changed = JSON.stringify(options) !== saved;

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
            heapRef={heapRef}
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
  heapRef,
  onLabel,
  onMove,
}: {
  option: ParameterOption;
  index: number;
  canEdit: boolean;
  others: ParameterOption[];
  canArchive: boolean;
  isLast: boolean;
  heapRef: HeapRef;
  onLabel: (label: string) => void;
  onMove: (by: -1 | 1) => void;
}) {
  const archive = useArchiveOption(heapRef);
  const { confirming, ask, cancel, buttonRef } = useConfirmation();
  const [replacement, setReplacement] = useState(others[0]?.id ?? "");
  const rank = <span className={rankClass}>{index + 1}</span>;

  if (confirming && canArchive) {
    return (
      <li className="my-1 rounded-xl border border-overdue-ink/25 bg-overdue-soft p-3 text-sm">
        <div className="flex items-center gap-2">
          {rank}
          <span className="font-medium">Archive {option.label}?</span>
        </div>
        <div className="mt-2.5 flex flex-wrap items-center gap-2">
          <label className="flex flex-wrap items-center gap-1.5 text-ink-soft max-sm:w-full max-sm:flex-col max-sm:items-stretch">
            Open items with {option.label} switch to
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
    return (
      <li className="flex items-center gap-2 py-1.5 max-sm:py-2.5">
        {rank}
        <span className="text-sm font-medium">{option.label}</span>
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
function LinkedTags({ parameter, heapRef, canEdit }: { parameter: Parameter; heapRef: HeapRef; canEdit: boolean }) {
  const tags = useTags(heapRef);
  const change = useChangeParameter(heapRef);
  const linked: ItemTag[] = (tags.data?.tags ?? [])
    .filter((tag) => parameter.tag_ids.includes(tag.id))
    .map((tag) => ({ id: tag.id, name: tag.name }));
  const heading = <h4 className={labelClass}>Suggested by tags</h4>;

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
        {linked.length > 0
          ? `Adding one of these tags to an item suggests ${parameter.name}.`
          : `No tags suggest ${parameter.name} yet.`}
      </p>
      <ProblemText error={change.error} className={problemClass} />
    </div>
  );
}

function ArchiveParameter({ parameter, heapRef }: { parameter: Parameter; heapRef: HeapRef }) {
  const archive = useArchiveParameter(heapRef);
  const { confirming, ask, cancel, buttonRef } = useConfirmation();
  return (
    <div className="ml-auto flex flex-wrap items-center justify-end gap-2 text-sm">
      {confirming ? (
        <>
          <span className="text-ink-soft">
            Archive {parameter.name}? It stops counting in the ranking; closed items keep their values.
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
