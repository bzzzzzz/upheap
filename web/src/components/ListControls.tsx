import { useEffect, useId, useRef } from "react";

import type { BlockedLayout, Parameter, StateMode } from "../api/client";
import { useParameters, useTags } from "../api/queries";
import { SIZE_LABELS, SIZES, STATE_LABELS } from "./labels";
import {
  DEFAULT_MODES,
  filterCount,
  joinList,
  MODE_LABELS,
  SHOWN_STATES,
  SORT_LABELS,
  SORTS,
  splitList,
  STATE_MODES,
  type ListSearch,
} from "./listView";

export type Panel = "filters" | "view" | null;

/**
 * Search, sort, filters and view settings for the ranked list. Everything but the search text is a URL param that
 * the server applies; `onChange` gets the whole new view. `/` focuses the search box.
 */
export function ListControls({
  heapId,
  view,
  onChange,
  search,
  onSearch,
  panel,
  onPanel,
  panelFocus,
}: {
  heapId: string;
  view: ListSearch;
  onChange: (view: ListSearch) => void;
  search: string;
  onSearch: (text: string) => void;
  panel: Panel;
  onPanel: (panel: Panel) => void;
  /** Changing it moves focus into the open panel and scrolls it into view. */
  panelFocus: number;
}) {
  const parameters = useParameters(heapId);
  const searchRef = useRef<HTMLInputElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const filtersId = useId();
  const viewId = useId();

  useEffect(() => {
    if (panelFocus > 0) {
      panelRef.current?.focus();
      panelRef.current?.scrollIntoView({ block: "nearest" });
    }
  }, [panelFocus]);
  const filters = filterCount(view);
  const viewChanged = SHOWN_STATES.some((state) => view[state] !== undefined) || view.blocked_layout !== undefined;

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing = target?.closest("input, textarea, select, [contenteditable]");
      if (event.key === "/" && !typing && !event.metaKey && !event.ctrlKey && !event.altKey) {
        event.preventDefault();
        searchRef.current?.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const toggle = (which: Exclude<Panel, null>) => onPanel(panel === which ? null : which);

  return (
    <div className="mt-6">
      <div className="flex flex-wrap items-center gap-2">
        <input
          ref={searchRef}
          type="search"
          value={search}
          onChange={(e) => onSearch(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape" && search !== "") {
              e.preventDefault();
              onSearch("");
            }
          }}
          placeholder="Search titles and descriptions  /"
          aria-label="Search items"
          className="min-w-0 flex-1 rounded-md border border-stone-300 bg-white px-3 py-1.5 text-sm dark:border-stone-700 dark:bg-stone-900"
        />
        <label className="inline-flex items-center gap-1 text-sm text-stone-500">
          <span>Sort</span>
          <select
            value={view.sort === "parameter" ? `parameter:${view.sort_parameter}` : (view.sort ?? "priority")}
            onChange={(e) => onChange({ ...view, ...sortFrom(e.target.value) })}
            className="rounded border border-stone-300 bg-white px-1 py-1 text-stone-900 dark:border-stone-700 dark:bg-stone-900 dark:text-stone-100"
          >
            {SORTS.map((sort) => (
              <option key={sort} value={sort}>
                {SORT_LABELS[sort]}
              </option>
            ))}
            {parameters.data?.parameters.map((parameter) => (
              <option key={parameter.id} value={`parameter:${parameter.id}`}>
                By {parameter.name}
              </option>
            ))}
            {view.sort === "parameter" &&
              !parameters.data?.parameters.some((parameter) => parameter.id === view.sort_parameter) && (
                <option value={`parameter:${view.sort_parameter}`}>
                  By {archivedName(parameters.data?.archived, view.sort_parameter) ?? "a parameter no longer in use"}{" "}
                  (archived, priority order)
                </option>
              )}
          </select>
        </label>
        <PanelButton
          open={panel === "filters"}
          controls={filtersId}
          onClick={() => toggle("filters")}
          active={filters > 0}
        >
          Filters{filters > 0 && ` (${filters})`}
        </PanelButton>
        <PanelButton open={panel === "view"} controls={viewId} onClick={() => toggle("view")} active={viewChanged}>
          View
        </PanelButton>
      </div>

      {panel === "filters" && (
        <div ref={panelRef} id={filtersId} tabIndex={-1} aria-label="Filters" role="region" className={panelClass}>
          <FiltersPanel heapId={heapId} view={view} onChange={onChange} />
        </div>
      )}
      {panel === "view" && (
        <div ref={panelRef} id={viewId} tabIndex={-1} aria-label="View settings" role="region" className={panelClass}>
          <ViewPanel view={view} onChange={onChange} />
        </div>
      )}
    </div>
  );
}

function PanelButton({
  open,
  controls,
  active,
  onClick,
  children,
}: {
  open: boolean;
  controls: string;
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-expanded={open}
      aria-controls={open ? controls : undefined}
      className={`rounded-md border px-2.5 py-1 text-sm ${
        active
          ? "border-sky-400 text-sky-800 dark:border-sky-600 dark:text-sky-300"
          : "border-stone-300 text-stone-600 dark:border-stone-700 dark:text-stone-300"
      } hover:bg-stone-100 dark:hover:bg-stone-800`}
    >
      {children}
    </button>
  );
}

function Toggle({ pressed, onClick, children }: { pressed: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      aria-pressed={pressed}
      onClick={onClick}
      className={`rounded px-1.5 py-0.5 text-xs ${
        pressed
          ? "bg-sky-600 text-white dark:bg-sky-500 dark:text-stone-950"
          : "bg-stone-100 text-stone-700 hover:bg-stone-200 dark:bg-stone-800 dark:text-stone-300 dark:hover:bg-stone-700"
      }`}
    >
      {children}
    </button>
  );
}

function PanelRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div role="group" aria-label={label} className="flex flex-wrap items-center gap-1.5">
      <span className="w-24 shrink-0 text-xs text-stone-500">{label}</span>
      {children}
    </div>
  );
}

const panelClass =
  "mt-2 flex flex-col gap-2 rounded-lg border border-stone-200 bg-white p-3 outline-none focus-visible:ring-2 focus-visible:ring-sky-500 dark:border-stone-800 dark:bg-stone-900";

/** One choice out of a few, as a radio group styled like the toggles. */
function Choice<T extends string>({
  label,
  options,
  value,
  labels,
  onChange,
  disabled,
}: {
  label: string;
  options: readonly T[];
  value: T;
  labels: Record<T, string>;
  onChange: (value: T) => void;
  disabled?: boolean;
}) {
  const name = useId();
  return (
    <div role="radiogroup" aria-label={label} className="flex flex-wrap items-center gap-1.5">
      <span className="w-24 shrink-0 text-xs text-stone-500">{label}</span>
      {options.map((option) => (
        <label key={option} className="inline-flex">
          <input
            type="radio"
            name={name}
            value={option}
            checked={value === option}
            disabled={disabled}
            onChange={() => onChange(option)}
            className="peer sr-only"
          />
          <span className="rounded bg-stone-100 px-1.5 py-0.5 text-xs text-stone-700 peer-checked:bg-sky-600 peer-checked:text-white peer-focus-visible:ring-2 peer-focus-visible:ring-sky-500 peer-disabled:opacity-50 hover:bg-stone-200 dark:bg-stone-800 dark:text-stone-300 dark:peer-checked:bg-sky-500 dark:peer-checked:text-stone-950 dark:hover:bg-stone-700">
            {labels[option]}
          </span>
        </label>
      ))}
    </div>
  );
}

/** The sort params for a value of the sort select: a fixed sort, or `parameter:<id>`. */
function sortFrom(value: string): Pick<ListSearch, "sort" | "sort_parameter"> {
  if (value.startsWith("parameter:")) {
    return { sort: "parameter", sort_parameter: value.slice("parameter:".length) };
  }
  const sort = SORTS.find((s) => s === value);
  return { sort: sort === "priority" ? undefined : sort, sort_parameter: undefined };
}

const archivedName = (archived: { id: string; name: string }[] | undefined, id: string | undefined) =>
  archived?.find((parameter) => parameter.id === id)?.name;

/** Archived options of `parameter` that the filter still names, labelled as archived so they can be removed. */
function chosenArchived(parameter: Parameter, chosen: string[]) {
  return parameter.archived_options
    .filter((option) => chosen.includes(option.id))
    .map((option) => ({ id: option.id, label: `${option.label} (archived)` }));
}

/** Toggles `value` in a comma-separated list param. */
const toggled = (raw: string | undefined, value: string) => {
  const values = splitList(raw);
  return joinList(values.includes(value) ? values.filter((v) => v !== value) : [...values, value]);
};

function FiltersPanel({
  heapId,
  view,
  onChange,
}: {
  heapId: string;
  view: ListSearch;
  onChange: (view: ListSearch) => void;
}) {
  const tags = useTags(heapId);
  const parameters = useParameters(heapId);
  const chosenTags = splitList(view.tag);
  const chosenOptions = splitList(view.param);
  return (
    <>
      <PanelRow label="Tags (any)">
        {tags.data?.tags.length === 0 && <span className="text-xs text-stone-500">This heap has no tags yet.</span>}
        {tags.data?.tags.map((tag) => (
          <Toggle
            key={tag.id}
            pressed={chosenTags.includes(tag.id)}
            onClick={() => onChange({ ...view, tag: toggled(view.tag, tag.id) })}
          >
            {tag.name}
          </Toggle>
        ))}
      </PanelRow>
      {(["effort", "impact"] as const).map((field) => (
        <PanelRow key={field} label={field === "effort" ? "Effort" : "Impact"}>
          {SIZES.map((size) => (
            <Toggle
              key={size}
              pressed={splitList(view[field]).includes(size)}
              onClick={() => onChange({ ...view, [field]: toggled(view[field], size) })}
            >
              {SIZE_LABELS[size]}
            </Toggle>
          ))}
        </PanelRow>
      ))}
      {parameters.data?.parameters.map((parameter) => (
        <PanelRow key={parameter.id} label={`${parameter.name} (any)`}>
          {[...parameter.options, ...chosenArchived(parameter, chosenOptions)].map((option) => (
            <Toggle
              key={option.id}
              pressed={chosenOptions.includes(option.id)}
              onClick={() => onChange({ ...view, param: toggled(view.param, option.id) })}
            >
              {option.label}
            </Toggle>
          ))}
        </PanelRow>
      ))}
      {/* Archived parameters no longer filter; a filter a saved view still has on one is shown so it can be removed. */}
      {parameters.data?.archived.map((parameter) => {
        const chosen = [...parameter.options, ...parameter.archived_options].filter((option) =>
          chosenOptions.includes(option.id),
        );
        return (
          chosen.length > 0 && (
            <PanelRow key={parameter.id} label={`${parameter.name} (archived, ignored)`}>
              {chosen.map((option) => (
                <Toggle
                  key={option.id}
                  pressed
                  onClick={() => onChange({ ...view, param: toggled(view.param, option.id) })}
                >
                  {option.label}
                </Toggle>
              ))}
            </PanelRow>
          )
        );
      })}
      {filterCount(view) > 0 && (
        <button
          type="button"
          onClick={() => onChange({ ...view, tag: undefined, effort: undefined, impact: undefined, param: undefined })}
          className="self-start text-xs text-stone-500 underline hover:text-stone-900 dark:hover:text-stone-100"
        >
          Clear filters
        </button>
      )}
    </>
  );
}

function ViewPanel({ view, onChange }: { view: ListSearch; onChange: (view: ListSearch) => void }) {
  const blockedShown = (view.blocked ?? DEFAULT_MODES.blocked) === "shown";
  return (
    <>
      {SHOWN_STATES.map((state) => (
        <Choice
          key={state}
          label={STATE_LABELS[state]}
          options={STATE_MODES}
          labels={MODE_LABELS}
          value={view[state] ?? DEFAULT_MODES[state]}
          onChange={(mode: StateMode) =>
            onChange({ ...view, [state]: mode === DEFAULT_MODES[state] ? undefined : mode })
          }
        />
      ))}
      <Choice
        label="Shown blocked"
        options={BLOCKED_LAYOUTS}
        labels={LAYOUT_LABELS}
        value={view.blocked_layout ?? "grouped"}
        disabled={!blockedShown}
        onChange={(layout: BlockedLayout) =>
          onChange({ ...view, blocked_layout: layout === "grouped" ? undefined : layout })
        }
      />
      <button
        type="button"
        onClick={() =>
          onChange({ ...view, open: undefined, in_progress: undefined, blocked: undefined, blocked_layout: undefined })
        }
        className="self-start text-xs text-stone-500 underline hover:text-stone-900 dark:hover:text-stone-100"
      >
        Reset to the default view
      </button>
    </>
  );
}

const BLOCKED_LAYOUTS: readonly BlockedLayout[] = ["grouped", "mixed"];
const LAYOUT_LABELS: Record<BlockedLayout, string> = { grouped: "In their own group", mixed: "Mixed in" };
