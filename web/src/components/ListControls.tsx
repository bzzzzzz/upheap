import { useEffect, useId, useRef, type ReactNode } from "react";

import type { BlockedLayout, HeapRef, Parameter, StateMode } from "../api/client";
import { useParameters, useTags } from "../api/queries";
import { SIZE_LABELS, SIZES, STATE_LABELS } from "./labels";
import { PopoverSheet } from "./PopoverSheet";
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
  heapRef,
  view,
  onChange,
  search,
  onSearch,
  panel,
  onPanel,
  panelFocus,
}: {
  heapRef: HeapRef;
  view: ListSearch;
  onChange: (view: ListSearch) => void;
  search: string;
  onSearch: (text: string) => void;
  panel: Panel;
  onPanel: (panel: Panel) => void;
  /** Changing it brings the panel's button into view, as the panel opens from it. */
  panelFocus: number;
}) {
  const parameters = useParameters(heapRef);
  const searchRef = useRef<HTMLInputElement>(null);
  const filtersRef = useRef<HTMLButtonElement>(null);
  const viewRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (panelFocus > 0) viewRef.current?.scrollIntoView({ block: "nearest" });
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
          className="h-9 min-w-0 flex-1 basis-48 rounded-lg border border-line bg-surface px-3 text-sm placeholder:text-muted pointer-coarse:h-11"
        />
        <label className="inline-flex items-center gap-1 text-sm text-muted">
          <span>Sort</span>
          <select
            value={view.sort === "parameter" ? `parameter:${view.sort_parameter}` : (view.sort ?? "priority")}
            onChange={(e) => onChange({ ...view, ...sortFrom(e.target.value) })}
            className="h-9 max-w-44 rounded-lg border border-line bg-surface px-1.5 text-ink pointer-coarse:h-11"
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
          buttonRef={filtersRef}
          open={panel === "filters"}
          onClick={() => toggle("filters")}
          active={filters > 0}
        >
          Filters{filters > 0 && ` (${filters})`}
        </PanelButton>
        <PanelButton buttonRef={viewRef} open={panel === "view"} onClick={() => toggle("view")} active={viewChanged}>
          View
        </PanelButton>
      </div>

      <PopoverSheet title="Filters" triggerRef={filtersRef} isOpen={panel === "filters"} onClose={() => onPanel(null)}>
        <FiltersPanel heapRef={heapRef} view={view} onChange={onChange} />
      </PopoverSheet>
      <PopoverSheet title="View settings" triggerRef={viewRef} isOpen={panel === "view"} onClose={() => onPanel(null)}>
        <ViewPanel view={view} onChange={onChange} />
      </PopoverSheet>
    </div>
  );
}

function PanelButton({
  buttonRef,
  open,
  active,
  onClick,
  children,
}: {
  buttonRef: React.RefObject<HTMLButtonElement | null>;
  open: boolean;
  active: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      ref={buttonRef}
      type="button"
      onClick={onClick}
      aria-expanded={open}
      aria-haspopup="dialog"
      className={`h-9 rounded-lg border px-3 text-sm hover:bg-sunken pointer-coarse:h-11 ${
        active ? "border-accent text-accent-ink" : "border-line text-ink-soft"
      }`}
    >
      {children}
    </button>
  );
}

/** A chip's look, filled once it's on, for the filters' toggles and the view's choices. */
const chipClass =
  "inline-flex h-8 items-center rounded-full border border-line px-3 text-sm text-ink-soft hover:border-muted pointer-coarse:h-10";
const chipOnClass = "border-ink bg-ink text-canvas hover:border-ink";

function Toggle({ pressed, onClick, children }: { pressed: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      aria-pressed={pressed}
      onClick={onClick}
      className={`${chipClass} ${pressed ? chipOnClass : ""}`}
    >
      {children}
    </button>
  );
}

function PanelRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div role="group" aria-label={label} className="flex flex-col gap-1.5">
      <span className="text-xs font-medium text-ink-soft">{label}</span>
      <div className="flex flex-wrap items-center gap-1.5">{children}</div>
    </div>
  );
}

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
    <div role="radiogroup" aria-label={label} className="flex flex-col gap-1.5">
      <span className="text-xs font-medium text-ink-soft">{label}</span>
      <div className="flex flex-wrap items-center gap-1.5">
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
            <span
              className={`${chipClass} peer-checked:border-ink peer-checked:bg-ink peer-checked:text-canvas peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-accent peer-disabled:opacity-50`}
            >
              {labels[option]}
            </span>
          </label>
        ))}
      </div>
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
  heapRef,
  view,
  onChange,
}: {
  heapRef: HeapRef;
  view: ListSearch;
  onChange: (view: ListSearch) => void;
}) {
  const tags = useTags(heapRef);
  const parameters = useParameters(heapRef);
  const chosenTags = splitList(view.tag);
  const chosenOptions = splitList(view.param);
  return (
    <>
      <PanelRow label="Tags (any)">
        {tags.data?.tags.length === 0 && <span className="text-sm text-muted">This heap has no tags yet.</span>}
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
          className="self-start text-sm text-muted underline hover:text-ink"
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
        className="self-start text-sm text-muted underline hover:text-ink"
      >
        Reset to the default view
      </button>
    </>
  );
}

const BLOCKED_LAYOUTS: readonly BlockedLayout[] = ["grouped", "mixed"];
const LAYOUT_LABELS: Record<BlockedLayout, string> = { grouped: "In their own group", mixed: "Mixed in" };
