import { Link } from "@tanstack/react-router";
import { RotateCcw } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent, type KeyboardEvent } from "react";

import type { Heap, HeapRef, Parameter, RankingPreview, WeightsPatch } from "../api/client";
import { useHeap, useParameters, useRankingPreview, useUpdateWeights, type RankingPreviewDraft } from "../api/queries";
import { compactButtonClass } from "../components/buttons";
import { ProblemText } from "../components/ProblemText";
import { cardClass, clampWeight, inputClass, introClass, labelClass } from "./settingsForm";

/** What "Reset all to 10" sets: the weight new heaps and parameters start with. */
const DEFAULT_WEIGHT = 10;
/** How many items the preview lists; a move to a place below it reads "New". */
const PREVIEW_ITEMS = 5;
const PREVIEW_DELAY_MS = 300;

const BUILT_IN = [
  { key: "need", name: "Need", description: "Critical items rise. Ideas add nothing and age slowly." },
  { key: "age", name: "Age", description: "Items rise the longer they wait." },
  {
    key: "deadline",
    name: "Deadline",
    description: "Items rise as their due date nears, and faster past an exact deadline.",
  },
  {
    key: "value",
    name: "Value per effort",
    description: "Small items with high impact rise above big ones with the same impact.",
  },
] as const;

/** One factor of the ranking, as a row: the heap's own, or a parameter in use. */
type Factor = {
  /** `need`, `age`, `deadline`, `value` or `parameter:{id}`: where its draft is kept. */
  key: string;
  name: string;
  description: string;
  /** The weight as last saved. */
  saved: number;
  /** Whether the caller may set it. */
  editable: boolean;
  /** Set for a parameter, which is saved with its version. */
  parameter?: Parameter;
};

/** The weights the user has moved, by factor key. A factor not in it shows its saved weight. */
type Draft = Record<string, number>;

/**
 * The heap's ranking weights (ux.md, Heap settings, Ranking): the heap's own and its parameters', each with its share
 * of the total, saved together. A preview shows how the top of the list would move. Everyone in the team reads them;
 * those with `edit_weights` change them.
 */
export function HeapRankingSettings({ heap: heapRef }: { heap: HeapRef }) {
  const heap = useHeap(heapRef);
  const parameters = useParameters(heapRef);

  // A failed refetch keeps the data, so what is on screen (and any draft in it) stays; the error shows only without it.
  const failure = (!heap.data && heap.error) || (!parameters.data && parameters.error);
  if (failure) {
    return (
      <p role="alert" className="py-16 text-center text-overdue-ink">
        {failure.message}
      </p>
    );
  }
  if (!heap.data || !parameters.data) return <p className="py-16 text-center text-muted">Loading…</p>;
  return <RankingForm heapRef={heapRef} heap={heap.data} parameters={parameters.data.parameters} />;
}

function RankingForm({ heapRef, heap, parameters }: { heapRef: HeapRef; heap: Heap; parameters: Parameter[] }) {
  const update = useUpdateWeights(heapRef);
  const [draft, setDraft] = useState<Draft>({});
  const [justSaved, setJustSaved] = useState(false);
  const firstInput = useRef<HTMLInputElement>(null);
  const status = useRef<HTMLParagraphElement>(null);

  const canEdit = heap.allowed_actions.includes("edit_weights");
  const archived = heap.archived_at !== null;
  const builtIn: Factor[] = BUILT_IN.map((factor) => ({
    ...factor,
    saved: heap.weights[factor.key],
    editable: canEdit,
  }));
  const custom: Factor[] = parameters.map((parameter) => ({
    key: `parameter:${parameter.id}`,
    name: parameter.name,
    description:
      parameter.options.length > 0 ? parameter.options.map((option) => option.label).join(", then ") : "No options yet",
    saved: parameter.weight,
    editable: canEdit && parameter.allowed_actions.includes("edit"),
    parameter,
  }));
  const factors = [...builtIn, ...custom];

  // A factor the user hasn't moved, or can't, shows its saved weight, so a refetch updates it.
  const valueOf = (factor: Factor) => (factor.editable ? (draft[factor.key] ?? factor.saved) : factor.saved);
  const changed = factors.filter((factor) => valueOf(factor) !== factor.saved);
  const dirty = changed.length > 0;
  const total = factors.reduce((sum, factor) => sum + valueOf(factor), 0);
  const atDefaults = factors.every((factor) => !factor.editable || valueOf(factor) === DEFAULT_WEIGHT);

  const savedMessageShown = justSaved && !dirty;
  useEffect(() => {
    // The Save button is gone by now, so focus goes to what replaced it.
    if (savedMessageShown) status.current?.focus();
  }, [savedMessageShown]);

  const edit = (factor: Factor, raw: string) => {
    setJustSaved(false);
    setDraft((current) => ({ ...current, [factor.key]: clampWeight(raw) }));
  };
  const cancel = () => {
    setDraft({});
    setJustSaved(false);
    update.reset();
    firstInput.current?.focus();
  };
  const reset = () => {
    setJustSaved(false);
    setDraft(Object.fromEntries(factors.filter((factor) => factor.editable).map((f) => [f.key, DEFAULT_WEIGHT])));
  };
  const save = (event: FormEvent) => {
    event.preventDefault();
    if (!dirty || update.isPending) return;
    const sent = Object.fromEntries(changed.map((factor) => [factor.key, valueOf(factor)]));
    update.mutate(
      { heap, body: toPatch(changed, sent) },
      {
        onSuccess: () => {
          // A weight moved again while this was on its way stays in the draft.
          setDraft((current) => Object.fromEntries(Object.entries(current).filter(([key, v]) => sent[key] !== v)));
          setJustSaved(true);
        },
      },
    );
  };
  const onEscape = (event: KeyboardEvent) => {
    if (event.key === "Escape") cancel();
  };

  const previewDraft = useSettled(toPreviewDraft(changed, valueOf), PREVIEW_DELAY_MS);
  const barShown = dirty || savedMessageShown;

  return (
    <div className="max-sm:pb-16">
      <p className={`${introClass} mb-5`}>
        Weights set how much each factor counts when {heap.name}&apos;s items are ranked, for everyone in the team. A
        factor&apos;s share is its weight out of the total; 0 turns it off.
      </p>
      {!canEdit && !archived && (
        <p className="mb-4 rounded-[10px] bg-sunken px-3.5 py-2.5 text-sm text-ink-soft max-sm:py-3">
          Only the team&apos;s admins can change the weights.
        </p>
      )}
      <div className="flex flex-wrap items-start gap-4 sm:gap-6">
        <form onSubmit={save} className="flex min-w-0 flex-[999_1_420px] flex-col gap-4">
          <FactorCard id="ranking-built-in" title="Built in" end={<span aria-hidden="true">Share</span>}>
            {builtIn.map((factor, index) => (
              <FactorRow
                key={factor.key}
                factor={factor}
                value={valueOf(factor)}
                total={total}
                onChange={(raw) => edit(factor, raw)}
                onEscape={onEscape}
                inputRef={index === 0 ? firstInput : undefined}
              />
            ))}
          </FactorCard>
          {custom.length > 0 && (
            <FactorCard
              id="ranking-parameters"
              title="Parameters"
              end={
                <Link
                  to="/teams/$team/heaps/$heap/settings/parameters"
                  params={heapRef}
                  className="text-[13px] font-normal tracking-normal text-accent-ink normal-case hover:underline max-sm:text-sm"
                >
                  Edit parameters
                </Link>
              }
            >
              {custom.map((factor) => (
                <FactorRow
                  key={factor.key}
                  factor={factor}
                  value={valueOf(factor)}
                  total={total}
                  onChange={(raw) => edit(factor, raw)}
                  onEscape={onEscape}
                />
              ))}
            </FactorCard>
          )}
          {canEdit && (
            // Wide: under the cards, sticking to the bottom of the screen. Phone: fixed above the tabs, and only
            // while there is something to say.
            <div
              className={`z-10 flex flex-wrap items-center gap-x-2.5 gap-y-2 border-t border-line bg-canvas py-3 sm:sticky sm:bottom-0 max-sm:fixed max-sm:inset-x-0 max-sm:bottom-(--phone-tabs-height) max-sm:bg-surface max-sm:px-4 max-sm:py-2.5 max-sm:shadow-[0_-4px_12px_rgb(28_25_23/0.06)] ${barShown ? "" : "max-sm:hidden"}`}
            >
              {dirty && (
                <>
                  <button
                    type="submit"
                    disabled={update.isPending}
                    className={`${compactButtonClass.primary} max-sm:order-3 max-sm:min-h-11`}
                  >
                    Save weights
                  </button>
                  <button
                    type="button"
                    onClick={cancel}
                    className={`${compactButtonClass.quiet} max-sm:order-2 max-sm:min-h-11`}
                  >
                    Cancel
                  </button>
                  <span className="text-[13px] text-accent-ink max-sm:order-1 max-sm:flex-1 max-sm:text-sm">
                    {changed.length} unsaved
                  </span>
                </>
              )}
              <p
                ref={status}
                role="status"
                tabIndex={-1}
                className="font-medium text-done outline-none max-sm:order-1 max-sm:flex-1"
              >
                {savedMessageShown && `Weights saved. ${heap.name}'s list is re-ranked.`}
              </p>
              <span className="flex-1 max-sm:hidden" />
              <button
                type="button"
                onClick={reset}
                disabled={atDefaults}
                className={`${compactButtonClass.quiet} max-sm:hidden`}
              >
                <RotateCcw aria-hidden="true" className="size-4" />
                Reset all to 10
              </button>
              <ProblemText error={update.error} className="text-overdue-ink" />
            </div>
          )}
        </form>
        <TopOfTheList heapRef={heapRef} draft={previewDraft} dirty={dirty} canEdit={canEdit} />
      </div>
      {canEdit && (
        <button
          type="button"
          onClick={reset}
          disabled={atDefaults}
          className={`${compactButtonClass.quiet} mt-4 min-h-11 pl-0 sm:hidden`}
        >
          <RotateCcw aria-hidden="true" className="size-[18px]" />
          Reset all to 10
        </button>
      )}
    </div>
  );
}

/** A card of factor rows under a small uppercase label, with something at the end of the label's line. */
function FactorCard({
  id,
  title,
  end,
  children,
}: {
  id: string;
  title: string;
  end: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section aria-labelledby={id} className={`${cardClass} pt-0 pb-2 sm:pt-0 sm:pb-2`}>
      <div className={`${labelClass} mt-4 flex items-end justify-between`}>
        <h2 id={id}>{title}</h2>
        {end}
      </div>
      <ul>{children}</ul>
    </section>
  );
}

function FactorRow({
  factor,
  value,
  total,
  onChange,
  onEscape,
  inputRef,
}: {
  factor: Factor;
  value: number;
  total: number;
  onChange: (raw: string) => void;
  onEscape: (event: KeyboardEvent) => void;
  inputRef?: React.Ref<HTMLInputElement>;
}) {
  // Plain arithmetic on what is on screen: the share is the weight out of the total, not a fact about the ranking.
  const share = total === 0 ? 0 : Math.round((value * 100) / total);
  return (
    <li className="border-b border-sunken py-3.5 last:border-b-0">
      <div className="flex items-center gap-2">
        <span className="font-semibold">{factor.name}</span>
        {value !== factor.saved && (
          <span className="rounded-full bg-accent-soft px-2 py-px text-xs text-accent-ink">was {factor.saved}</span>
        )}
        <span className="flex-1" />
        <span aria-hidden="true" className="h-1 w-11 overflow-hidden rounded-xs bg-sunken sm:w-14">
          <span className="block h-1 bg-accent" style={{ width: `${share}%` }} />
        </span>
        <span className="min-w-10 text-right font-semibold tabular-nums">{total === 0 ? "–" : `${share}%`}</span>
      </div>
      <p
        className={`mt-1 text-[13px]/[1.45] text-muted max-sm:text-sm/[1.45] ${factor.parameter ? "line-clamp-2" : ""}`}
      >
        {factor.description}
      </p>
      {factor.editable ? (
        <div className="mt-1.5 flex items-center gap-3 max-sm:mt-1 max-sm:gap-2.5">
          <input
            type="range"
            min={0}
            max={100}
            step={1}
            value={value}
            aria-label={`${factor.name} weight`}
            onChange={(e) => onChange(e.target.value)}
            onKeyDown={onEscape}
            className="h-7 min-w-0 flex-1 accent-accent max-sm:h-11 pointer-coarse:h-11"
          />
          <input
            ref={inputRef}
            type="number"
            inputMode="numeric"
            min={0}
            max={100}
            value={value}
            aria-label={`${factor.name} weight, 0 to 100`}
            onChange={(e) => onChange(e.target.value)}
            onKeyDown={onEscape}
            className={`${inputClass} w-[68px] text-right tabular-nums max-sm:min-h-11 max-sm:w-16 max-sm:text-base`}
          />
        </div>
      ) : (
        <p className="mt-1.5 text-[13px] text-ink-soft max-sm:text-sm">Weight {value}</p>
      )}
    </li>
  );
}

/**
 * The first items of the list as the draft weights would rank them, each with how far it moves from where it is now.
 * Everyone sees it; without a draft it is the current top of the list.
 */
function TopOfTheList({
  heapRef,
  draft,
  dirty,
  canEdit,
}: {
  heapRef: HeapRef;
  draft: RankingPreviewDraft;
  dirty: boolean;
  canEdit: boolean;
}) {
  const preview = useRankingPreview(heapRef, draft);
  const hint = dirty
    ? "With your changes. Nothing moves until you save."
    : canEdit
      ? "Move a weight to see how the top of the list would change."
      : "The top of the list as it is ranked now.";
  return (
    <aside
      aria-labelledby="ranking-preview"
      className="min-w-0 flex-[1_1_260px] rounded-[14px] border border-line bg-surface px-4 py-3.5 sm:sticky sm:top-4 sm:px-[18px] sm:py-4"
    >
      <h2 id="ranking-preview" className="text-base font-semibold sm:text-[15px]">
        Top of the list
      </h2>
      <p className="mt-1 mb-2.5 text-sm/[1.45] text-muted sm:mb-3 sm:text-[13px]/[1.45]">{hint}</p>
      <PreviewItems
        heapRef={heapRef}
        preview={preview.data}
        failed={preview.isError}
        dimmed={preview.isPlaceholderData}
      />
    </aside>
  );
}

function PreviewItems({
  heapRef,
  preview,
  failed,
  dimmed,
}: {
  heapRef: HeapRef;
  preview: RankingPreview | undefined;
  failed: boolean;
  dimmed: boolean;
}) {
  if (!preview) {
    return <p className="text-sm text-muted">{failed ? "Couldn't load the preview." : "Loading…"}</p>;
  }
  if (preview.items.length === 0) return <p className="text-sm text-muted">No open items yet.</p>;
  return (
    <ol className={`flex flex-col gap-0.5 ${dimmed ? "opacity-60" : ""}`}>
      {preview.items.map(({ item, position, current_position: current }) => (
        <li key={item.id} className="flex min-h-9 items-center gap-2.5 max-sm:min-h-10">
          <span className="inline-flex size-[22px] shrink-0 items-center justify-center rounded-md bg-sunken text-xs font-semibold text-ink-soft max-sm:size-6">
            {position}
          </span>
          <Link
            to="/teams/$team/heaps/$heap/items/$item"
            params={{ ...heapRef, item: item.key }}
            className="min-w-0 flex-1 truncate hover:underline"
          >
            {item.title}
          </Link>
          <Move position={position} current={current} />
        </li>
      ))}
    </ol>
  );
}

/** How the item moved from where it is now: up or down by how much, or new to the top of the list. Nothing if it didn't. */
function Move({ position, current }: { position: number; current: number }) {
  const className = "text-xs font-semibold max-sm:text-[13px]";
  if (current > PREVIEW_ITEMS) {
    return (
      <span role="img" aria-label={`new in the top ${PREVIEW_ITEMS}`} className={`${className} text-done`}>
        New
      </span>
    );
  }
  if (current > position) {
    return (
      <span role="img" aria-label={`up ${current - position}`} className={`${className} text-done`}>
        ↑{current - position}
      </span>
    );
  }
  if (current < position) {
    return (
      <span role="img" aria-label={`down ${position - current}`} className={`${className} text-muted`}>
        ↓{position - current}
      </span>
    );
  }
  return null;
}

/** The request for the factors whose weight changed: the heap's by name, parameters with the version they were seen at. */
function toPatch(changed: Factor[], weights: Draft): WeightsPatch {
  const patch: WeightsPatch = { parameters: [] };
  for (const factor of changed) {
    const weight = weights[factor.key];
    if (weight === undefined) continue;
    if (factor.parameter) {
      patch.parameters?.push({ id: factor.parameter.id, weight, version: factor.parameter.version });
    } else if (isBuiltIn(factor.key)) {
      patch[factor.key] = weight;
    }
  }
  return patch;
}

/** What a preview ranks with: the weights that differ from the saved ones. */
function toPreviewDraft(changed: Factor[], valueOf: (factor: Factor) => number): RankingPreviewDraft {
  const draft: RankingPreviewDraft = {};
  const parameters: { id: string; weight: number }[] = [];
  for (const factor of changed) {
    if (factor.parameter) parameters.push({ id: factor.parameter.id, weight: valueOf(factor) });
    else if (isBuiltIn(factor.key)) draft[factor.key] = valueOf(factor);
  }
  if (parameters.length > 0) draft.parameters = parameters;
  return draft;
}

function isBuiltIn(key: string): key is (typeof BUILT_IN)[number]["key"] {
  return BUILT_IN.some((factor) => factor.key === key);
}

/** `value` once it has stopped changing for `delay` ms, so a slider being dragged asks the server once, not per step. */
function useSettled<T>(value: T, delay: number): T {
  const key = JSON.stringify(value);
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setSettled(JSON.parse(key) as T), delay);
    return () => clearTimeout(timer);
  }, [key, delay]);
  return settled;
}
