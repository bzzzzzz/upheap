import { Link } from "@tanstack/react-router";
import { ArrowDown, ArrowUp, Ellipsis, RotateCcw } from "lucide-react";
import { Fragment, useEffect, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { Button, Menu, MenuTrigger } from "react-aria-components";

import type { HeapRef } from "../api/client";
import type { Heap, HeapWeightsPatchRequest, Parameter, RankingPreview, Tag } from "../api/schema";
import {
  PREVIEW_ITEMS,
  useHeap,
  useRankingPreview,
  useUpdateWeights,
  type RankingPreviewDraft,
} from "../api/queries/heaps";
import { useParameters } from "../api/queries/parameters";
import { heapTags, useTags } from "../api/queries/tags";
import { compactButtonClass } from "../components/buttons";
import { MenuEntry, MenuPopover } from "../components/Menu";
import { useFocusAfterRender } from "../components/useFocusAfterRender";
import { ProblemText } from "../components/ProblemText";
import { cardClass, clampWeight, inputClass, introClass, labelClass } from "./settingsForm";
import { RequestId } from "../components/RequestId";

/** What "Reset all to 10" sets: the weight new heaps and parameters start with. */
const DEFAULT_WEIGHT = 10;
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

/** The factor row of the heap's tag order, in the Tags card. */
const TAGS_FACTOR = {
  key: "tags",
  name: "Tag order",
  description:
    "An item's highest tag in the order lifts it or holds it back. Items with no tag here sit in the middle.",
} as const;

/** One factor of the ranking, as a row: the heap's own, or a parameter in use. */
type Factor = {
  /** `need`, `age`, `deadline`, `value`, `tags` or `parameter:{id}`: where its draft is kept. */
  key: string;
  name: string;
  description: React.ReactNode;
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
  const tags = useTags(heapRef);

  // A failed refetch keeps the data, so what is on screen (and any draft in it) stays; the error shows only without it.
  const failure = (!heap.data && heap.error) || (!parameters.data && parameters.error) || (!tags.data && tags.error);
  if (failure) {
    return (
      <>
        <p role="alert" className="py-16 text-center text-overdue-ink">
          {failure.message}
        </p>
        <RequestId error={failure} className="justify-center" />
      </>
    );
  }
  if (!heap.data || !parameters.data || !tags.data) {
    return <p className="py-16 text-center text-muted">Loading…</p>;
  }
  return (
    <RankingForm
      heapRef={heapRef}
      heap={heap.data}
      parameters={parameters.data.parameters}
      tags={heapTags(tags.data)}
    />
  );
}

function RankingForm({
  heapRef,
  heap,
  parameters,
  tags,
}: {
  heapRef: HeapRef;
  heap: Heap;
  parameters: Parameter[];
  tags: Tag[];
}) {
  const update = useUpdateWeights(heapRef);
  const [draft, setDraft] = useState<Draft>({});
  // The tag order as the user arranged it; `null` while it is the saved one, so a refetch updates it.
  const [orderDraft, setOrderDraft] = useState<string[] | null>(null);
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
  const tagsFactor: Factor = { ...TAGS_FACTOR, saved: heap.weights.tags ?? 0, editable: canEdit };
  const custom: Factor[] = parameters.map((parameter) => ({
    key: `parameter:${parameter.id}`,
    name: parameter.name,
    description:
      parameter.options.length > 0 ? <OptionLinks heapRef={heapRef} parameter={parameter} /> : "No options yet",
    saved: parameter.weight,
    editable: canEdit && parameter.allowed_actions.includes("edit"),
    parameter,
  }));
  const factors = [...builtIn, tagsFactor, ...custom];

  // A factor the user hasn't moved, or can't, shows its saved weight, so a refetch updates it.
  const valueOf = (factor: Factor) => (factor.editable ? (draft[factor.key] ?? factor.saved) : factor.saved);
  const changed = factors.filter((factor) => valueOf(factor) !== factor.saved);
  const order = canEdit ? (orderDraft ?? heap.tag_order) : heap.tag_order;
  const orderChanged = !sameOrder(order, heap.tag_order);
  // A changed order counts once, however many tags moved.
  const unsaved = changed.length + (orderChanged ? 1 : 0);
  const dirty = unsaved > 0;
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
  const editOrder = (next: string[]) => {
    setJustSaved(false);
    setOrderDraft(next);
  };
  const cancel = () => {
    setDraft({});
    setOrderDraft(null);
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
    const sentOrder = orderChanged ? order : undefined;
    update.mutate(
      { heap, body: toPatch(changed, sent, sentOrder) },
      {
        onSuccess: () => {
          // A weight moved again while this was on its way stays in the draft, and so does an order changed again.
          setDraft((current) => Object.fromEntries(Object.entries(current).filter(([key, v]) => sent[key] !== v)));
          setOrderDraft((current) => (current && sentOrder && !sameOrder(current, sentOrder) ? current : null));
          setJustSaved(true);
        },
      },
    );
  };
  const onEscape = (event: KeyboardEvent) => {
    if (event.key === "Escape") cancel();
  };

  const previewDraft = useSettled(toPreviewDraft(changed, valueOf, orderChanged ? order : undefined), PREVIEW_DELAY_MS);
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
          <FactorCard
            id="ranking-tags"
            title="Tags"
            end={
              orderChanged ? (
                <span className="rounded-full bg-accent-soft px-2 py-px text-xs font-normal tracking-normal text-accent-ink normal-case">
                  Order changed
                </span>
              ) : (
                <Link
                  to="/teams/$team/heaps/$heap/settings/tags"
                  params={heapRef}
                  className="text-[13px] font-normal tracking-normal text-accent-ink normal-case hover:underline max-sm:text-sm"
                >
                  Edit tags
                </Link>
              )
            }
          >
            <FactorRow
              factor={tagsFactor}
              value={valueOf(tagsFactor)}
              total={total}
              onChange={(raw) => edit(tagsFactor, raw)}
              onEscape={onEscape}
            />
            <TagOrder order={order} tags={tags} editable={canEdit} onChange={editOrder} onEscape={onEscape} />
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
                    {unsaved} unsaved
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

/** A parameter's options, highest first, each a link to the heap's list filtered by it. */
function OptionLinks({ heapRef, parameter }: { heapRef: HeapRef; parameter: Parameter }) {
  return parameter.options.map((option, index) => (
    <Fragment key={option.id}>
      {index > 0 && ", then "}
      <Link
        to="/teams/$team/heaps/$heap"
        params={heapRef}
        search={{ param: option.id }}
        aria-label={`Items with ${parameter.name}: ${option.label}`}
        className="text-param-ink hover:underline"
      >
        {option.label}
      </Link>
    </Fragment>
  ));
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
 * The heap's tag order, the highest first: a numbered list with what each tag does next to an untagged item, and for
 * editors ↑ ↓ and Remove on each row (a ⋯ menu on a phone) and a picker of the tags not yet ranked, which adds one at
 * the bottom. A draft, saved with the weights.
 */
function TagOrder({
  order,
  tags,
  editable,
  onChange,
  onEscape,
}: {
  order: string[];
  tags: Tag[];
  editable: boolean;
  onChange: (order: string[]) => void;
  onEscape: (event: KeyboardEvent) => void;
}) {
  const listRef = useRef<HTMLOListElement>(null);
  const pickerRef = useRef<HTMLSelectElement>(null);
  // After a move, focus stays on the moved tag's button, or its other one at the top or bottom; after a remove it
  // goes to the picker. Both are found once the list has re-rendered, and a remove may come from a closing menu.
  const focusAfterRender = useFocusAfterRender();

  const byId = new Map(tags.map((tag) => [tag.id, tag]));
  const ranked = order.flatMap((id) => {
    const tag = byId.get(id);
    return tag ? [tag] : [];
  });
  const unranked = tags.filter((tag) => !order.includes(tag.id));

  const move = (index: number, by: -1 | 1) => {
    const next = [...order];
    const [moved] = next.splice(index, 1);
    if (moved === undefined) return;
    next.splice(index + by, 0, moved);
    onChange(next);
    const direction = by < 0 ? "up" : "down";
    focusAfterRender(() => {
      const buttons = [...(listRef.current?.querySelectorAll<HTMLButtonElement>(`button[data-tag="${moved}"]`) ?? [])];
      return buttons.find((b) => b.dataset.move === direction && !b.disabled) ?? buttons.find((b) => !b.disabled);
    });
  };
  const remove = (id: string) => {
    onChange(order.filter((other) => other !== id));
    focusAfterRender(() => pickerRef.current);
  };

  if (ranked.length === 0 && !editable) {
    return <li className="pb-3.5 text-[13px] text-muted max-sm:text-sm">No tags ranked.</li>;
  }
  return (
    <li className="pb-3.5">
      {ranked.length === 0 ? (
        <p className="py-1.5 text-sm text-muted">No tags ranked yet. Add one to start.</p>
      ) : (
        <>
          <ol ref={listRef} aria-label="Tag order, highest first" className="flex flex-col">
            {ranked.map((tag, index) => (
              <li key={tag.id} className="flex min-h-9 items-center gap-2 max-sm:min-h-11">
                <span className={rankClass}>{index + 1}</span>
                <span className="rounded-md bg-sunken px-1.5 text-[13px] text-ink-soft">{tag.name}</span>
                <span className="text-xs text-muted">{tagEffect(index, ranked.length)}</span>
                <span className="flex-1" />
                {editable && (
                  <>
                    <button
                      type="button"
                      data-tag={tag.id}
                      data-move="up"
                      disabled={index === 0}
                      onClick={() => move(index, -1)}
                      onKeyDown={onEscape}
                      aria-label={`Move ${tag.name} up`}
                      className={compactButtonClass.icon}
                    >
                      <ArrowUp aria-hidden="true" className="size-4" />
                    </button>
                    <button
                      type="button"
                      data-tag={tag.id}
                      data-move="down"
                      disabled={index === ranked.length - 1}
                      onClick={() => move(index, 1)}
                      onKeyDown={onEscape}
                      aria-label={`Move ${tag.name} down`}
                      className={compactButtonClass.icon}
                    >
                      <ArrowDown aria-hidden="true" className="size-4" />
                    </button>
                    <button
                      type="button"
                      onClick={() => remove(tag.id)}
                      onKeyDown={onEscape}
                      aria-label={`Remove ${tag.name} from the order`}
                      className={`${compactButtonClass.quiet} px-2 text-[13px] max-sm:hidden`}
                    >
                      Remove
                    </button>
                    <MenuTrigger>
                      <Button
                        aria-label={`More for ${tag.name}`}
                        className="flex size-11 shrink-0 items-center justify-center rounded-lg text-ink-soft outline-none hover:bg-sunken data-[focus-visible]:outline-2 data-[focus-visible]:outline-accent sm:hidden"
                      >
                        <Ellipsis aria-hidden="true" className="size-4" />
                      </Button>
                      <MenuPopover placement="bottom end">
                        <Menu aria-label={`More for ${tag.name}`} className="outline-none">
                          <MenuEntry id="remove" onAction={() => remove(tag.id)}>
                            Remove from the order
                          </MenuEntry>
                        </Menu>
                      </MenuPopover>
                    </MenuTrigger>
                  </>
                )}
              </li>
            ))}
          </ol>
          <p className="mt-1 text-[13px] text-muted max-sm:text-sm">{untaggedPlace(ranked.length)}</p>
        </>
      )}
      {editable && (
        <div className="mt-2.5 flex flex-wrap items-center gap-x-3 gap-y-1.5">
          {unranked.length > 0 ? (
            <>
              <select
                ref={pickerRef}
                value=""
                aria-label="Add a tag to the order"
                onChange={(e) => {
                  if (e.target.value !== "") onChange([...order, e.target.value]);
                }}
                onKeyDown={onEscape}
                className={`${inputClass} max-sm:min-h-11 max-sm:w-full max-sm:text-base`}
              >
                <option value="">Add a tag to the order</option>
                {unranked.map((tag) => (
                  <option key={tag.id} value={tag.id}>
                    {tag.name}
                  </option>
                ))}
              </select>
              <span className="text-[13px] text-muted max-sm:text-sm">
                {unranked.length === 1 ? "1 tag not ranked" : `${unranked.length} tags not ranked`}
              </span>
            </>
          ) : (
            <span className="text-[13px] text-muted max-sm:text-sm">
              {tags.length === 0 ? "This heap has no tags yet." : "Every tag is ranked."}
            </span>
          )}
        </div>
      )}
    </li>
  );
}

/** A tag's place in the order, like an option's, in grey: violet is kept for parameters. */
const rankClass =
  "inline-flex size-[22px] shrink-0 items-center justify-center rounded-md bg-sunken text-xs font-semibold text-ink-soft max-sm:size-6";

/**
 * What the tag at `index` of `count` does next to an untagged item. A ranked tag scores from 1 at the top to 0 at the
 * bottom, a single one 1, and an item with no ranked tag 0.5 (architecture.md, Priority score).
 */
function tagEffect(index: number, count: number): string {
  if (count === 1) return "lifts";
  // Its score, (count - 1 - index) / (count - 1), against 1/2, in whole numbers.
  const twice = 2 * (count - 1 - index);
  if (twice > count - 1) return "lifts";
  if (twice === count - 1) return "as untagged";
  return "holds back";
}

/** Where an untagged item sits among `count` ranked tags. */
function untaggedPlace(count: number): string {
  if (count === 1) return "Untagged items score below #1.";
  if (count % 2 === 1) return `Untagged items score the same as #${(count + 1) / 2}.`;
  return `Untagged items score between #${count / 2} and #${count / 2 + 1}.`;
}

function sameOrder(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((id, index) => id === b[index]);
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
function toPatch(changed: Factor[], weights: Draft, tagOrder: string[] | undefined): HeapWeightsPatchRequest {
  const patch: HeapWeightsPatchRequest = { parameters: [] };
  if (tagOrder) patch.tag_order = tagOrder;
  for (const factor of changed) {
    const weight = weights[factor.key];
    if (weight === undefined) continue;
    if (factor.parameter) {
      patch.parameters?.push({ id: factor.parameter.id, weight, version: factor.parameter.version });
    } else if (factor.key === TAGS_FACTOR.key) {
      patch.tags = weight;
    } else if (isBuiltIn(factor.key)) {
      patch[factor.key] = weight;
    }
  }
  return patch;
}

/** What a preview ranks with: the weights that differ from the saved ones, and the tag order when it does. */
function toPreviewDraft(
  changed: Factor[],
  valueOf: (factor: Factor) => number,
  tagOrder: string[] | undefined,
): RankingPreviewDraft {
  const draft: RankingPreviewDraft = {};
  if (tagOrder) draft.tag_order = tagOrder;
  const parameters: { id: string; weight: number }[] = [];
  for (const factor of changed) {
    if (factor.parameter) parameters.push({ id: factor.parameter.id, weight: valueOf(factor) });
    else if (factor.key === TAGS_FACTOR.key) draft.tags = valueOf(factor);
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
