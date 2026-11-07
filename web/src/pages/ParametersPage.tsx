import { useState, type FormEvent } from "react";

import type { ItemTag, Parameter, ParameterOption } from "../api/client";
import {
  useArchiveOption,
  useArchiveParameter,
  useChangeParameter,
  useCreateParameter,
  useParameters,
  useTags,
} from "../api/queries";
import { inputClass, NameForm, primaryButtonClass, quietButtonClass } from "../components/NameForm";
import { ProblemText } from "../components/ProblemText";
import { TagPicker } from "../components/TagPicker";
import { useConfirmation } from "../components/useConfirmation";

/**
 * Manage parameters (Req Views: Manage parameters). Every member sees the parameters, since they set their values on
 * items; only admins get the controls, as `allowed_actions` says. Parameters and options are archived, never deleted.
 */
export function ParametersPage({ heapId }: { heapId: string }) {
  const parameters = useParameters(heapId);

  if (parameters.isPending) {
    return <p className="py-16 text-center text-stone-500">Loading…</p>;
  }
  if (parameters.isError) {
    return (
      <p role="alert" className="py-16 text-center text-red-700 dark:text-red-400">
        {parameters.error.message}
      </p>
    );
  }

  const { heap, allowed_actions, archived } = parameters.data;
  return (
    <main>
      <h1 className="mt-6 mb-1 text-xl font-semibold tracking-tight">Parameters</h1>
      <p className="mb-4 text-sm text-stone-500">
        Parameters rank the items of {heap.name} next to age, deadline and value. An item's option counts more the
        higher it is in the list, and the weight sets how much the parameter counts overall.
      </p>
      {allowed_actions.includes("create") && <NewParameterForm heapId={heapId} />}
      {parameters.data.parameters.length === 0 ? (
        <p className="py-12 text-center text-stone-500">No parameters yet.</p>
      ) : (
        <ul className="flex flex-col gap-4">
          {parameters.data.parameters.map((parameter) => (
            <ParameterCard key={parameter.id} parameter={parameter} heapId={heapId} />
          ))}
        </ul>
      )}
      {archived.length > 0 && (
        <section className="mt-8">
          <h2 className="mb-2 text-sm font-medium text-stone-500">Archived</h2>
          <ul className="text-sm text-stone-500">
            {archived.map((parameter) => (
              <li key={parameter.id}>
                {parameter.name}: {parameter.options.map((option) => option.label).join(", ")}
              </li>
            ))}
          </ul>
        </section>
      )}
    </main>
  );
}

function NewParameterForm({ heapId }: { heapId: string }) {
  const create = useCreateParameter(heapId);
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
    create.mutate(
      { name, weight, options: labels },
      {
        onSuccess: () => {
          setName("");
          setWeight(10);
          setOptions("");
        },
      },
    );
  };

  return (
    <form
      onSubmit={submit}
      className="mb-6 flex flex-col gap-2 rounded-lg border border-stone-200 bg-white p-3 dark:border-stone-800 dark:bg-stone-900"
    >
      <div className="flex flex-wrap items-center gap-2">
        <input
          aria-label="New parameter name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Parameter name, such as Customer"
          maxLength={50}
          className={`${inputClass} min-w-0 flex-1`}
        />
        <WeightInput value={weight} onChange={setWeight} label="Weight of the new parameter" />
      </div>
      <textarea
        aria-label="Options of the new parameter, one per line, highest priority first"
        value={options}
        onChange={(e) => setOptions(e.target.value)}
        placeholder={"Options, one per line, highest priority first\nEnterprise\nPro\nFree"}
        rows={3}
        className={inputClass}
      />
      <div className="flex items-center gap-2">
        <button type="submit" disabled={create.isPending || name.trim() === ""} className={primaryButtonClass}>
          Add parameter
        </button>
        <ProblemText error={create.error} />
      </div>
    </form>
  );
}

function WeightInput({ value, onChange, label }: { value: number; onChange: (value: number) => void; label: string }) {
  return (
    <label className="inline-flex items-center gap-1 text-xs text-stone-500">
      Weight
      <input
        type="number"
        min={0}
        max={100}
        aria-label={label}
        value={value}
        onChange={(e) => onChange(Math.max(0, Math.min(100, Math.round(Number(e.target.value) || 0))))}
        className={`${inputClass} w-16`}
      />
    </label>
  );
}

function ParameterCard({ parameter, heapId }: { parameter: Parameter; heapId: string }) {
  const change = useChangeParameter(heapId);
  const canEdit = parameter.allowed_actions.includes("edit");

  return (
    <li className="rounded-lg border border-stone-200 bg-white p-3 dark:border-stone-800 dark:bg-stone-900">
      {canEdit ? (
        <div className="flex flex-wrap items-center gap-3">
          <NameForm
            label={`Name of parameter ${parameter.name}`}
            value={parameter.name}
            version={parameter.version}
            maxLength={50}
            pending={change.isPending}
            error={null}
            onSave={(name) => change.mutate({ parameter, change: { kind: "patch", patch: { name } } })}
            className="font-medium"
          />
          <WeightForm
            parameter={parameter}
            onSave={(weight) => change.mutate({ parameter, change: { kind: "patch", patch: { weight } } })}
          />
        </div>
      ) : (
        <h2 className="font-medium">
          {parameter.name} <span className="text-xs font-normal text-stone-500">weight {parameter.weight}</span>
        </h2>
      )}
      <ProblemText error={change.error} />

      <OptionsEditor parameter={parameter} heapId={heapId} canEdit={canEdit} />
      <LinkedTags parameter={parameter} heapId={heapId} canEdit={canEdit} />
      {parameter.allowed_actions.includes("archive") && <ArchiveParameter parameter={parameter} heapId={heapId} />}
    </li>
  );
}

/** The weight, saved on its own once it changes. A newer version from the server resets it. */
function WeightForm({ parameter, onSave }: { parameter: Parameter; onSave: (weight: number) => void }) {
  const [weight, setWeight] = useState(parameter.weight);
  const [shownVersion, setShownVersion] = useState(parameter.version);
  if (shownVersion !== parameter.version) {
    setShownVersion(parameter.version);
    setWeight(parameter.weight);
  }
  return (
    <form
      className="flex items-center gap-1"
      onSubmit={(event) => {
        event.preventDefault();
        if (weight !== parameter.weight) onSave(weight);
      }}
    >
      <WeightInput value={weight} onChange={setWeight} label={`Weight of parameter ${parameter.name}`} />
      {weight !== parameter.weight && (
        <button type="submit" className={quietButtonClass}>
          Save
        </button>
      )}
    </form>
  );
}

/**
 * The options in use, highest priority first. Labels and order are edited together and saved as one arrangement;
 * a new option is added last. Archiving an option asks which option open items should switch to.
 */
function OptionsEditor({ parameter, heapId, canEdit }: { parameter: Parameter; heapId: string; canEdit: boolean }) {
  const change = useChangeParameter(heapId);
  const [options, setOptions] = useState<ParameterOption[]>(parameter.options);
  const [shownVersion, setShownVersion] = useState(parameter.version);
  const [newLabel, setNewLabel] = useState("");
  if (shownVersion !== parameter.version) {
    setShownVersion(parameter.version);
    setOptions(parameter.options);
  }
  const changed = JSON.stringify(options) !== JSON.stringify(parameter.options);

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
    <div className="mt-3">
      <h3 className="mb-1 text-xs text-stone-500">Options, highest priority first</h3>
      {options.length === 0 && <p className="text-sm text-stone-500">No options yet.</p>}
      <ol className="flex flex-col gap-1">
        {options.map((option, index) => (
          <li key={option.id} className="flex flex-wrap items-center gap-1">
            {canEdit ? (
              <>
                <input
                  aria-label={`Label of option ${index + 1}`}
                  value={option.label}
                  maxLength={50}
                  onChange={(e) =>
                    setOptions(options.map((o) => (o.id === option.id ? { ...o, label: e.target.value } : o)))
                  }
                  className={`${inputClass} w-48`}
                />
                <button
                  type="button"
                  disabled={index === 0}
                  onClick={() => move(index, -1)}
                  aria-label={`Move ${option.label} up`}
                  className={quietButtonClass}
                >
                  ↑
                </button>
                <button
                  type="button"
                  disabled={index === options.length - 1}
                  onClick={() => move(index, 1)}
                  aria-label={`Move ${option.label} down`}
                  className={quietButtonClass}
                >
                  ↓
                </button>
                {!changed && options.length > 1 && (
                  <ArchiveOption option={option} others={options.filter((o) => o.id !== option.id)} heapId={heapId} />
                )}
              </>
            ) : (
              <span className="text-sm">{option.label}</span>
            )}
          </li>
        ))}
      </ol>
      {canEdit && changed && (
        <div className="mt-2 flex items-center gap-2">
          <button
            type="button"
            disabled={change.isPending || options.some((o) => o.label.trim() === "")}
            onClick={() => change.mutate({ parameter, change: { kind: "arrange_options", options } })}
            className={primaryButtonClass}
          >
            Save options
          </button>
          <button type="button" onClick={() => setOptions(parameter.options)} className={quietButtonClass}>
            Cancel
          </button>
        </div>
      )}
      {canEdit && !changed && (
        <form onSubmit={add} className="mt-2 flex items-center gap-2">
          <input
            aria-label={`New option of ${parameter.name}`}
            value={newLabel}
            onChange={(e) => setNewLabel(e.target.value)}
            placeholder="New option"
            maxLength={50}
            className={`${inputClass} w-48`}
          />
          <button type="submit" disabled={change.isPending || newLabel.trim() === ""} className={quietButtonClass}>
            Add option
          </button>
        </form>
      )}
      <ProblemText error={change.error} />
    </div>
  );
}

function ArchiveOption({
  option,
  others,
  heapId,
}: {
  option: ParameterOption;
  others: ParameterOption[];
  heapId: string;
}) {
  const archive = useArchiveOption(heapId);
  const { confirming, ask, cancel, buttonRef } = useConfirmation();
  const [replacement, setReplacement] = useState(others[0]?.id ?? "");

  if (!confirming) {
    return (
      <button ref={buttonRef} type="button" onClick={ask} className={quietButtonClass}>
        Archive
      </button>
    );
  }
  return (
    <span className="inline-flex flex-wrap items-center gap-1 text-sm">
      <label className="inline-flex items-center gap-1">
        Open items with {option.label} switch to
        <select
          autoFocus
          value={replacement}
          onChange={(e) => setReplacement(e.target.value)}
          className={`${inputClass} py-0.5`}
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
        className="rounded px-1.5 py-1 font-medium text-red-700 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950"
      >
        Archive
      </button>
      <button type="button" onClick={cancel} className={quietButtonClass}>
        Cancel
      </button>
      <ProblemText error={archive.error} />
    </span>
  );
}

/** The tags whose items the parameter is suggested for. Each change is saved at once. */
function LinkedTags({ parameter, heapId, canEdit }: { parameter: Parameter; heapId: string; canEdit: boolean }) {
  const tags = useTags(heapId);
  const change = useChangeParameter(heapId);
  const linked: ItemTag[] = (tags.data?.tags ?? [])
    .filter((tag) => parameter.tag_ids.includes(tag.id))
    .map((tag) => ({ id: tag.id, name: tag.name }));

  if (!canEdit) {
    if (linked.length === 0) return null;
    return (
      <p className="mt-3 text-xs text-stone-500">Suggested for items tagged {linked.map((t) => t.name).join(", ")}</p>
    );
  }
  return (
    <div className="mt-3">
      <TagPicker
        heapId={heapId}
        value={linked}
        disabled={change.isPending}
        onChange={(next) => change.mutate({ parameter, change: { kind: "link_tags", tagIds: next.map((t) => t.id) } })}
      />
      <p className="mt-1 text-xs text-stone-500">Adding one of these tags to an item suggests this parameter.</p>
      <ProblemText error={change.error} />
    </div>
  );
}

function ArchiveParameter({ parameter, heapId }: { parameter: Parameter; heapId: string }) {
  const archive = useArchiveParameter(heapId);
  const { confirming, ask, cancel, buttonRef } = useConfirmation();
  return (
    <div className="mt-3 flex flex-wrap items-center gap-1 border-t border-stone-100 pt-2 text-sm dark:border-stone-800">
      {confirming ? (
        <>
          <span>Archive {parameter.name}? It stops counting in the ranking; closed items keep their values.</span>
          <button
            type="button"
            autoFocus
            disabled={archive.isPending}
            onClick={() => archive.mutate(parameter, { onError: cancel })}
            className="rounded px-1.5 py-1 font-medium text-red-700 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950"
          >
            Archive
          </button>
          <button type="button" onClick={cancel} className={quietButtonClass}>
            Cancel
          </button>
        </>
      ) : (
        <button ref={buttonRef} type="button" onClick={ask} className={quietButtonClass}>
          Archive parameter
        </button>
      )}
      <ProblemText error={archive.error} />
    </div>
  );
}
