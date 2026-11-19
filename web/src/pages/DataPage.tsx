import { ChevronDown } from "lucide-react";
import { useRef, useState } from "react";
import { ToggleButton, ToggleButtonGroup, type Key } from "react-aria-components";

import { isNotFound } from "../api/client";
import type { DataDays, Me, TeamData } from "../api/schema";
import { useMe, useTeamData } from "../api/queries";
import { choiceButtonClass } from "../components/ChoiceButtons";
import { DataChart, type DataSeries } from "../components/DataChart";
import { usd } from "../components/labels";
import { NotFound } from "../components/NotFound";
import { PopoverSheet } from "../components/PopoverSheet";
import { ProblemText } from "../components/ProblemText";
import { SheetChoice } from "../components/Sheet";
import { teamName } from "../components/teamName";

export type DataSearch = { heap?: string; days?: 7 | 30 | 90 };

const PERIODS: readonly { id: DataDays; label: string }[] = [
  { id: "7", label: "7 days" },
  { id: "30", label: "30 days" },
  { id: "90", label: "90 days" },
];

/** The id of "All heaps" among the heap choices. A slug can't hold "~", so it names no heap. */
const ALL_HEAPS = "~all";

const SPEND: DataSeries[] = [{ key: "spend", label: "Spend", color: "var(--agent)", value: (b) => b.spend_usd }];
const ADDED: DataSeries[] = [{ key: "added", label: "Added", color: "var(--ink-soft)", value: (b) => b.added }];
const CLOSED: DataSeries[] = [
  { key: "done", label: "Done", color: "var(--done)", value: (b) => b.closed.done },
  { key: "wont_do", label: "Won't do", color: "var(--muted)", value: (b) => b.closed.wont_do },
  { key: "became", label: "Became", color: "var(--param-ink)", value: (b) => b.closed.became },
];
const RUNS: DataSeries[] = [
  { key: "reached", label: "Reached", color: "var(--done)", value: (b) => b.runs.reached },
  { key: "asked", label: "Asked", color: "var(--accent)", value: (b) => b.runs.asked },
  { key: "failed", label: "Failed", color: "var(--overdue-ink)", value: (b) => b.runs.failed },
  { key: "other", label: "Other", color: "var(--muted)", value: (b) => b.runs.other },
];

const sum = (counts: Record<string, number>) => Object.values(counts).reduce((a, b) => a + b, 0);

/**
 * What a team's heaps, or one of them, spent and did over the last 7, 30 or 90 days (ux.md, Data). Every choice is in
 * the address, so `onChange` navigates.
 */
export function DataPage({
  team: slug,
  search,
  onChange,
}: {
  team: string;
  search: DataSearch;
  onChange: (next: DataSearch) => void;
}) {
  const { data: me } = useMe();
  const days: DataDays = `${search.days ?? 30}` as DataDays;
  const data = useTeamData(slug, { heap: search.heap, days });
  const team = me?.teams.find((t) => t.slug === slug);
  if (data.isError && !data.data && isNotFound(data.error))
    return <NotFound heading="This team isn't here" line="It doesn't exist, or you're not in it." teamsLink />;

  return (
    <main>
      <h1 className="mt-6 mb-1 text-[22px] font-semibold tracking-[-0.015em]">Data</h1>
      <p className="mb-4 min-h-5 text-[13px] break-words text-ink-soft">
        {me && team && `${teamName(team, me)} · what happened, by ${days === "90" ? "week" : "day"}.`}
      </p>
      <div className="mb-4 flex flex-wrap items-center gap-3">
        {team && (
          <HeapFilter heaps={team.heaps} value={search.heap} onChange={(heap) => onChange({ ...search, heap })} />
        )}
        <ToggleButtonGroup
          aria-label="Period"
          selectionMode="single"
          disallowEmptySelection
          selectedKeys={[days]}
          onSelectionChange={(keys: Set<Key>) => {
            const [picked] = [...keys];
            if (picked !== undefined) onChange({ ...search, days: Number(picked) as 7 | 30 | 90 });
          }}
          className="flex gap-1"
        >
          {PERIODS.map((period) => (
            <ToggleButton key={period.id} id={period.id} className={`${choiceButtonClass} rounded-lg`}>
              {period.label}
            </ToggleButton>
          ))}
        </ToggleButtonGroup>
      </div>
      {data.isError && !data.data ? (
        <div className="flex flex-col items-center gap-3 py-12">
          <ProblemText error={data.error} className="text-overdue-ink" />
          <button type="button" onClick={() => void data.refetch()} className={choiceButtonClass + " rounded-lg"}>
            Try again
          </button>
        </div>
      ) : (
        <div aria-busy={data.isPlaceholderData} className={data.isPlaceholderData ? "opacity-60" : undefined}>
          <Figures data={data.data} days={days} />
        </div>
      )}
    </main>
  );
}

function Figures({ data, days }: { data: TeamData | undefined; days: DataDays }) {
  const totals = data?.totals;
  const nothing =
    totals !== undefined &&
    totals.spend_usd === 0 &&
    totals.runs_without_cost === 0 &&
    totals.added === 0 &&
    sum(totals.closed) === 0 &&
    sum(totals.runs) === 0;
  const emptyText = nothing ? `Nothing in these ${days === "90" ? "13 weeks" : `${days} days`}.` : null;
  const frame = (title: string, series: DataSeries[], format?: (value: number) => string) =>
    data ? (
      <DataChart
        title={title}
        buckets={data.buckets}
        series={series}
        format={format}
        decimals={format !== undefined}
        empty={emptyText}
      />
    ) : (
      <section aria-label={title} className="rounded-[14px] border border-line bg-surface p-3">
        <h2 className="text-sm font-semibold">{title}</h2>
        <p className="flex h-[180px] items-center justify-center text-sm text-muted">Loading…</p>
      </section>
    );

  return (
    <>
      <div className="mb-4 grid grid-cols-2 gap-3 lg:grid-cols-4">
        <Tile label="Agents' spend" value={totals && usd(totals.spend_usd)}>
          {totals && totals.runs_without_cost > 0 && (
            <>
              {totals.runs_without_cost} {totals.runs_without_cost === 1 ? "run" : "runs"} reported no cost
            </>
          )}
        </Tile>
        <Tile label="Items added" value={totals?.added} />
        <Tile label="Items closed" value={totals && sum(totals.closed)} />
        <Tile label="Agent runs" value={totals && sum(totals.runs)} />
      </div>
      <div className="grid gap-3 sm:grid-cols-2">
        {frame("Agents' spend", SPEND, usd)}
        {frame("Items added", ADDED)}
        {frame("Items closed", CLOSED)}
        {frame("Agent runs", RUNS)}
      </div>
    </>
  );
}

function Tile({
  label,
  value,
  children,
}: {
  label: string;
  value: string | number | undefined;
  children?: React.ReactNode;
}) {
  return (
    <div className="rounded-[14px] border border-line bg-surface p-3">
      <p className="text-xs text-ink-soft">{label}</p>
      <p className="text-xl font-semibold tabular-nums">{value ?? "–"}</p>
      {children && <p className="text-xs text-muted">{children}</p>}
    </div>
  );
}

/** The heap the figures are for: all heaps, then the active ones, then the archived under their own heading. */
function HeapFilter({
  heaps,
  value,
  onChange,
}: {
  heaps: Me["teams"][number]["heaps"];
  value: string | undefined;
  onChange: (heap: string | undefined) => void;
}) {
  const [open, setOpen] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const current = heaps.find((heap) => heap.slug === value);
  const active = heaps.filter((heap) => !heap.archived);
  const archived = heaps.filter((heap) => heap.archived);

  return (
    <>
      <button
        ref={trigger}
        type="button"
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen(true)}
        className="flex h-9 max-w-full items-center gap-2 rounded-lg border border-line-strong bg-surface px-3 text-sm outline-offset-2 hover:border-muted focus-visible:outline-2 focus-visible:outline-accent pointer-coarse:h-11"
      >
        <span className="sr-only">Heap: </span>
        <span className="truncate">{current?.name ?? "All heaps"}</span>
        <ChevronDown aria-hidden="true" className="size-4 shrink-0 text-muted" />
      </button>
      <PopoverSheet
        title="Heaps"
        triggerRef={trigger}
        isOpen={open}
        onClose={() => setOpen(false)}
        placement="bottom start"
        size="narrow"
        scrollBody
      >
        <ToggleButtonGroup
          aria-label="Heap"
          selectionMode="single"
          disallowEmptySelection
          selectedKeys={[current?.slug ?? ALL_HEAPS]}
          onSelectionChange={(keys: Set<Key>) => {
            const [picked] = [...keys];
            if (picked === undefined) return;
            onChange(picked === ALL_HEAPS ? undefined : String(picked));
            setOpen(false);
          }}
          className="flex min-h-0 flex-col overflow-y-auto"
        >
          <SheetChoice id={ALL_HEAPS}>All heaps</SheetChoice>
          {active.map((heap) => (
            <SheetChoice key={heap.id} id={heap.slug}>
              {heap.name}
            </SheetChoice>
          ))}
          {archived.length > 0 && (
            <p className="px-3 pt-3 pb-1 text-xs font-semibold tracking-wide text-muted uppercase">Archived</p>
          )}
          {archived.map((heap) => (
            <SheetChoice key={heap.id} id={heap.slug}>
              {heap.name}
            </SheetChoice>
          ))}
        </ToggleButtonGroup>
      </PopoverSheet>
    </>
  );
}
