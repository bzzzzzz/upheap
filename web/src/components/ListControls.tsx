import { SlidersHorizontal, X } from "lucide-react";
import { useEffect, useId, useRef, type ReactNode } from "react";

import type { BlockedLayout, HeapRef, Parameter, ParameterList, Size, StateMode } from "../api/client";
import { useParameters, useTags } from "../api/queries";
import { NEED_LABELS, NEEDS, SIZE_LABELS, SIZES, STATE_LABELS } from "./labels";
import { PHONE, useMedia } from "./media";
import { PopoverSheet } from "./PopoverSheet";
import {
  DEFAULT_MODES,
  filterCount,
  isBlockedShown,
  joinList,
  MODE_LABELS,
  SHOWN_STATES,
  SORT_LABELS,
  SORTS,
  splitList,
  STATE_MODES,
  viewChanges,
  withoutFiltersOrViewChanges,
  type ListSearch,
  type ShownState,
  type ViewSetting,
} from "./listView";

export type Panel = "filters" | null;

/** How many items the view lists (`listed`) and would list without its filters (`total`). */
export type ListCounts = { listed: number; total: number };

/**
 * Search, sort, and one Filters panel for the filters and what the list shows (app-ux.md, The list §7). Everything
 * but the search text is a URL param that the server applies; `onChange` gets the whole new view. On a phone the
 * sort moves into the panel. `/` focuses the search box.
 */
export function ListControls({
  heapRef,
  view,
  onChange,
  search,
  onSearch,
  panel,
  onPanel,
  counts,
}: {
  heapRef: HeapRef;
  view: ListSearch;
  onChange: (view: ListSearch) => void;
  search: string;
  onSearch: (text: string) => void;
  panel: Panel;
  onPanel: (panel: Panel) => void;
  /** `undefined` while the view loads. */
  counts: ListCounts | undefined;
}) {
  const phone = useMedia(PHONE);
  const parameters = useParameters(heapRef);
  const searchRef = useRef<HTMLInputElement>(null);
  const filtersRef = useRef<HTMLButtonElement>(null);

  const changes = filterCount(view) + viewChanges(view).length;
  // On a phone the sort lives in the panel too, so the button counts a sort other than priority; on a wider screen
  // the sort select shows it. Reset all leaves the sort as it is either way.
  const badge = changes + (phone && sortValue(view) !== "priority" ? 1 : 0);
  const close = () => onPanel(null);

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

  return (
    <div className="mt-6">
      <div className="flex items-center gap-2">
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
          placeholder={phone ? "Search" : "Search titles and descriptions  /"}
          aria-label="Search items"
          className="h-9 min-w-0 flex-1 rounded-lg border border-line bg-surface px-3 text-sm placeholder:text-muted max-sm:h-11 max-sm:text-base pointer-coarse:h-11"
        />
        <label className="inline-flex h-9 shrink-0 items-center rounded-lg border border-line bg-surface pl-3 text-sm max-sm:hidden pointer-coarse:h-11">
          <span className="text-muted">Sort by</span>
          <select
            value={sortValue(view)}
            onChange={(e) => onChange({ ...view, ...sortFrom(e.target.value) })}
            className="h-full max-w-44 cursor-pointer rounded-lg bg-transparent pr-1.5 pl-1 font-medium text-ink outline-offset-2"
          >
            {sortOptions(view, parameters.data).map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </label>
        <button
          ref={filtersRef}
          type="button"
          onClick={() => onPanel(panel === "filters" ? null : "filters")}
          aria-expanded={panel === "filters"}
          aria-haspopup="dialog"
          // A label rather than hidden text: screen-reader-only spans are laid out as blocks, which puts a stray space
          // in the name ("Filters , 2 on").
          aria-label={`${phone ? "Filters and sort" : "Filters"}${badge > 0 ? `, ${badge} on` : ""}`}
          className={`inline-flex h-9 shrink-0 items-center justify-center gap-1.5 rounded-lg border px-3 text-sm font-medium max-sm:relative max-sm:size-11 max-sm:px-0 pointer-coarse:h-11 ${
            badge > 0
              ? "border-accent/40 bg-accent-soft text-accent-ink"
              : "border-line bg-surface text-ink-soft hover:bg-sunken"
          }`}
        >
          <SlidersHorizontal aria-hidden="true" className="size-4 shrink-0" />
          {/* The phone's button is the icon alone, and opens the sort too, which its name says. */}
          <span className="max-sm:hidden">Filters</span>
          {badge > 0 && (
            <span
              aria-hidden="true"
              className="min-w-5 rounded-full bg-accent px-1.5 text-xs leading-5 font-semibold text-surface tabular-nums max-sm:absolute max-sm:-top-1.5 max-sm:-right-1.5"
            >
              {badge}
            </span>
          )}
        </button>
      </div>

      <PopoverSheet
        title={phone ? "Filters & sort" : "Filters"}
        triggerRef={filtersRef}
        isOpen={panel === "filters"}
        onClose={close}
        action={
          changes > 0 && (
            <button
              type="button"
              onClick={() => onChange(withoutFiltersOrViewChanges(view))}
              className="rounded-md px-1.5 py-1 text-sm font-medium text-accent-ink hover:bg-accent-soft"
            >
              Reset all
            </button>
          )
        }
      >
        {phone && <SortSection view={view} parameters={parameters.data} onChange={onChange} />}
        <ShowSection view={view} onChange={onChange} />
        <hr className="border-line" />
        <NarrowSection heapRef={heapRef} view={view} onChange={onChange} />
        <PanelFooter phone={phone} filtered={filterCount(view) > 0} counts={counts} onDone={close} />
      </PopoverSheet>
    </div>
  );
}

/**
 * The filters that are on and the view settings changed from the defaults, as chips that each take one off, so the
 * user always sees why the list looks as it does (ux-overview.md, The list §3). `shown` of `total` says how much the
 * filters leave out; `total` is the view's count without them, `undefined` until it has loaded.
 */
export function FilterChips({
  heapRef,
  view,
  onChange,
  shown,
  total,
}: {
  heapRef: HeapRef;
  view: ListSearch;
  onChange: (view: ListSearch) => void;
  shown: number;
  total: number | undefined;
}) {
  const tags = useTags(heapRef);
  const parameters = useParameters(heapRef);
  const options = [...(parameters.data?.parameters ?? []), ...(parameters.data?.archived ?? [])].flatMap((parameter) =>
    [...parameter.options, ...parameter.archived_options].map((option) => ({
      id: option.id,
      label: `${parameter.name}: ${option.label}`,
    })),
  );
  const filters = [
    ...splitList(view.tag).map((id) => ({
      key: `tag:${id}`,
      label: tags.data?.tags.find((tag) => tag.id === id)?.name ?? "A tag",
      remove: () => onChange({ ...view, tag: toggled(view.tag, id) }),
    })),
    ...splitList(view.need).map((need) => ({
      key: `need:${need}`,
      label: `Need: ${NEED_LABELS[need as keyof typeof NEED_LABELS]}`,
      remove: () => onChange({ ...view, need: toggled(view.need, need) }),
    })),
    ...(["effort", "impact"] as const).flatMap((field) =>
      splitList(view[field]).map((size) => ({
        key: `${field}:${size}`,
        label: `${field === "effort" ? "Effort" : "Impact"} ${SIZE_LABELS[size as Size]}`,
        remove: () => onChange({ ...view, [field]: toggled(view[field], size) }),
      })),
    ),
    ...splitList(view.param).map((id) => ({
      key: `param:${id}`,
      label: options.find((option) => option.id === id)?.label ?? "An option",
      remove: () => onChange({ ...view, param: toggled(view.param, id) }),
    })),
  ];
  const settings = viewChanges(view).map((setting) => {
    const [name, value] = describeSetting(view, setting);
    return { key: setting, name, value, remove: () => onChange({ ...view, [setting]: undefined }) };
  });

  return (
    <div role="group" aria-label="Filters on" className="mt-3 flex flex-wrap items-center gap-1.5">
      {filters.map((chip) => (
        <Chip key={chip.key} label={`Remove filter ${chip.label}`} onClick={chip.remove}>
          {chip.label}
        </Chip>
      ))}
      {settings.map((chip) => (
        <Chip key={chip.key} label={`Remove view setting ${chip.name} ${chip.value}`} onClick={chip.remove}>
          {chip.name}: {chip.value}
        </Chip>
      ))}
      {filters.length > 0 && total !== undefined && (
        <span className="ml-1 text-xs text-muted tabular-nums">
          {shown} of {total}
        </span>
      )}
      <button
        type="button"
        onClick={() => onChange(withoutFiltersOrViewChanges(view))}
        className="ml-1 rounded-md px-1 text-xs font-medium text-accent-ink hover:underline pointer-coarse:h-9"
      >
        Reset all
      </button>
    </div>
  );
}

function Chip({ label, onClick, children }: { label: string; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      className="inline-flex h-7 items-center gap-1 rounded-full bg-sunken pr-1.5 pl-2.5 text-xs text-ink-soft hover:bg-line pointer-coarse:h-9"
    >
      {children}
      <X aria-hidden="true" className="size-3" />
    </button>
  );
}

/** A changed view setting as its chip says it: the state, or Blocked for its layout, and its value in lower case. */
function describeSetting(view: ListSearch, setting: ViewSetting): [string, string] {
  if (setting === "blocked_layout") {
    return [STATE_LABELS.blocked, LAYOUT_LABELS[view.blocked_layout ?? "grouped"].toLowerCase()];
  }
  return [STATE_LABELS[setting], MODE_LABELS[view[setting] ?? DEFAULT_MODES[setting]].toLowerCase()];
}

/** The sort, phone only: the same choices as the sort select of a wider screen. */
function SortSection({
  view,
  parameters,
  onChange,
}: {
  view: ListSearch;
  parameters: ParameterList | undefined;
  onChange: (view: ListSearch) => void;
}) {
  return (
    <Section title="Sort by">
      <ChipChoice
        label="Sort by"
        options={sortOptions(view, parameters)}
        value={sortValue(view)}
        onChange={(value) => onChange({ ...view, ...sortFrom(value) })}
      />
    </Section>
  );
}

/** Where each state goes, and where shown blocked items go. A dot marks a state changed from the default. */
function ShowSection({ view, onChange }: { view: ListSearch; onChange: (view: ListSearch) => void }) {
  const blockedShown = isBlockedShown(view);
  const modes = STATE_MODES.map((mode) => ({ value: mode, label: MODE_LABELS[mode] }));
  return (
    <Section title="Show">
      {SHOWN_STATES.map((state: ShownState) => (
        <Segmented
          key={state}
          label={STATE_LABELS[state]}
          changed={view[state] !== undefined}
          options={modes}
          value={view[state] ?? DEFAULT_MODES[state]}
          onChange={(mode: StateMode) =>
            onChange({ ...view, [state]: mode === DEFAULT_MODES[state] ? undefined : mode })
          }
        />
      ))}
      <Segmented
        label="Shown blocked"
        changed={viewChanges(view).includes("blocked_layout")}
        options={BLOCKED_LAYOUTS.map((layout) => ({ value: layout, label: LAYOUT_LABELS[layout] }))}
        value={view.blocked_layout ?? "grouped"}
        disabled={!blockedShown}
        onChange={(layout: BlockedLayout) =>
          onChange({ ...view, blocked_layout: layout === "grouped" ? undefined : layout })
        }
      />
    </Section>
  );
}

/** The filters: tags, need, effort, impact, then each custom parameter. */
function NarrowSection({
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
  const chosenNeeds = splitList(view.need);
  const chosenOptions = splitList(view.param);
  return (
    <Section title="Narrow down">
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
      <PanelRow label="Need">
        {NEEDS.map((need) => (
          <Toggle
            key={need}
            pressed={chosenNeeds.includes(need)}
            onClick={() => onChange({ ...view, need: toggled(view.need, need) })}
          >
            {NEED_LABELS[need]}
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
      {/* Archived parameters no longer filter; a filter a link still has on one is shown so it can be removed. */}
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
    </Section>
  );
}

/**
 * How many items the panel's settings leave, and the way out: "N of M items" and Done on a wider screen, one
 * "Show N items" button on a phone.
 */
function PanelFooter({
  phone,
  filtered,
  counts,
  onDone,
}: {
  phone: boolean;
  filtered: boolean;
  counts: ListCounts | undefined;
  onDone: () => void;
}) {
  const items = (n: number) => (n === 1 ? "item" : "items");
  const total = filtered ? counts?.total : undefined;
  if (phone) {
    const label = !counts
      ? "Show items"
      : total !== undefined
        ? `Show ${counts.listed} of ${total} ${items(total)}`
        : `Show ${counts.listed} ${items(counts.listed)}`;
    return (
      <div className="sticky bottom-0 -mx-4 mt-1 bg-surface px-4 pt-2">
        <button
          type="button"
          onClick={onDone}
          className="h-12 w-full rounded-xl bg-ink text-[15px] font-semibold text-canvas outline-offset-2 focus-visible:outline-2 focus-visible:outline-accent"
        >
          {label}
        </button>
      </div>
    );
  }
  return (
    <div className="sticky -bottom-4 -mx-4 -mb-4 flex items-center gap-2 border-t border-line bg-surface px-4 py-3">
      {counts && total !== undefined && (
        <span className="text-sm text-muted tabular-nums">
          <b className="font-semibold text-ink">{counts.listed}</b> of {total} {items(total)}
        </span>
      )}
      <button
        type="button"
        onClick={onDone}
        className="ml-auto h-9 rounded-lg bg-ink px-4 text-sm font-medium text-canvas outline-offset-2 focus-visible:outline-2 focus-visible:outline-accent pointer-coarse:h-11"
      >
        Done
      </button>
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  const id = useId();
  return (
    <section aria-labelledby={id} className="flex flex-col gap-3">
      <h3 id={id} className="text-xs font-semibold tracking-wide text-muted uppercase">
        {title}
      </h3>
      {children}
    </section>
  );
}

/** A chip's look, filled once it's on, for the filters' toggles and the phone's sort choices. */
// The pressed look is amber, like the Filters button it answers to. Off and on each set their own text color, as two
// utilities for one property in the same class list would leave the winner to the stylesheet's order.
const chipClass = "inline-flex h-8 items-center rounded-full border px-3 text-sm pointer-coarse:h-10";
const chipOffClass = "border-line text-ink-soft hover:border-muted";
const chipOnClass = "border-accent/40 bg-accent-soft font-semibold text-accent-ink";

function Toggle({ pressed, onClick, children }: { pressed: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      aria-pressed={pressed}
      onClick={onClick}
      className={`${chipClass} ${pressed ? chipOnClass : chipOffClass}`}
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

type ChoiceOption<T extends string> = { value: T; label: string };

/** One choice out of several, as a radio group of chips like the filters' toggles. */
function ChipChoice<T extends string>({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: readonly ChoiceOption<T>[];
  value: T;
  onChange: (value: T) => void;
}) {
  const name = useId();
  return (
    <div role="radiogroup" aria-label={label} className="flex flex-wrap items-center gap-1.5">
      {options.map((option) => (
        <label key={option.value} className="inline-flex">
          <input
            type="radio"
            name={name}
            value={option.value}
            checked={value === option.value}
            onChange={() => onChange(option.value)}
            className="peer sr-only"
          />
          <span
            className={`${chipClass} ${chipOffClass} peer-checked:border-accent/40 peer-checked:bg-accent-soft peer-checked:font-semibold peer-checked:text-accent-ink peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-accent`}
          >
            {option.label}
          </span>
        </label>
      ))}
    </div>
  );
}

/** One choice out of a few, as a labelled row with a segmented control: a radio group. */
function Segmented<T extends string>({
  label,
  changed,
  options,
  value,
  onChange,
  disabled,
}: {
  label: string;
  /** Whether the value differs from the default, which a dot by the label shows. */
  changed: boolean;
  options: readonly ChoiceOption<T>[];
  value: T;
  onChange: (value: T) => void;
  disabled?: boolean;
}) {
  const name = useId();
  return (
    <div role="radiogroup" aria-label={label} className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1">
      <span className={`inline-flex items-center gap-1.5 text-sm ${disabled ? "text-muted" : "text-ink"}`}>
        {label}
        {changed && <span aria-hidden="true" className="size-1.5 rounded-full bg-accent" />}
      </span>
      <div className="inline-flex rounded-lg bg-sunken p-0.5">
        {options.map((option) => (
          <label key={option.value} className="inline-flex">
            <input
              type="radio"
              name={name}
              value={option.value}
              checked={value === option.value}
              disabled={disabled}
              onChange={() => onChange(option.value)}
              className="peer sr-only"
            />
            <span className="inline-flex h-7 cursor-pointer items-center rounded-md px-2.5 text-[13px] text-ink-soft peer-checked:bg-surface peer-checked:font-medium peer-checked:text-ink peer-checked:shadow-sm peer-focus-visible:outline-2 peer-focus-visible:outline-accent peer-disabled:cursor-default peer-disabled:opacity-50 pointer-coarse:h-9">
              {option.label}
            </span>
          </label>
        ))}
      </div>
    </div>
  );
}

/** The sort as one value: a fixed sort, or `parameter:<id>`. */
const sortValue = (view: ListSearch) =>
  view.sort === "parameter" ? `parameter:${view.sort_parameter}` : (view.sort ?? "priority");

/**
 * The sorts on offer: the fixed ones, one per parameter, and an archived parameter the view still sorts by, so it
 * shows as chosen.
 */
function sortOptions(view: ListSearch, parameters: ParameterList | undefined): ChoiceOption<string>[] {
  const options = [
    ...SORTS.map((sort) => ({ value: sort as string, label: SORT_LABELS[sort] })),
    ...(parameters?.parameters ?? []).map((parameter) => ({
      value: `parameter:${parameter.id}`,
      label: `By ${parameter.name}`,
    })),
  ];
  if (view.sort === "parameter" && !parameters?.parameters.some((parameter) => parameter.id === view.sort_parameter)) {
    const name = parameters?.archived.find((parameter) => parameter.id === view.sort_parameter)?.name;
    options.push({
      value: sortValue(view),
      label: `By ${name ?? "a parameter no longer in use"} (archived, priority order)`,
    });
  }
  return options;
}

/** The sort params for a sort value: a fixed sort, or `parameter:<id>`. */
function sortFrom(value: string): Pick<ListSearch, "sort" | "sort_parameter"> {
  if (value.startsWith("parameter:")) {
    return { sort: "parameter", sort_parameter: value.slice("parameter:".length) };
  }
  const sort = SORTS.find((s) => s === value);
  return { sort: sort === "priority" ? undefined : sort, sort_parameter: undefined };
}

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

const BLOCKED_LAYOUTS: readonly BlockedLayout[] = ["grouped", "mixed"];
const LAYOUT_LABELS: Record<BlockedLayout, string> = { grouped: "Own group", mixed: "Mixed in" };
