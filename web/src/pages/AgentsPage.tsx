import { Link } from "@tanstack/react-router";
import { ChevronDown } from "lucide-react";
import { useState } from "react";

import type { HeapRef } from "../api/client";
import type { DecisionRow, Me, ResultRow } from "../api/schema";
import { useDecisions, useMe, useResults } from "../api/queries";
import { readDecisionsFolded, writeDecisionsFolded } from "../components/agentsStorage";
import { KIND_LABELS, STAGE_LABELS } from "../components/labels";
import { titleTeam } from "../components/teamName";

/** How many Your decision rows show before "Show N more". */
const FIRST_DECISIONS = 5;

const heapOf = (row: { heap: { team_slug: string; slug: string } }): HeapRef => ({
  team: row.heap.team_slug,
  heap: row.heap.slug,
});

/** "Rotate the certs" link: opens the item over the page, and closing it comes back here. */
function ItemLink({ row }: { row: DecisionRow | ResultRow }) {
  return (
    <Link
      to="/teams/$team/heaps/$heap/items/$item"
      params={{ ...heapOf(row), item: row.item.key }}
      state={(prev) => ({ ...prev, closesBack: true })}
      data-row-link={row.item.key}
      // A finger gets a 44 px target: padding on an inline link grows what it catches without changing the line.
      className="font-medium text-accent-ink underline underline-offset-2 outline-none focus-visible:outline-2 focus-visible:outline-accent pointer-coarse:py-4"
    >
      {row.item.title}
    </Link>
  );
}

/** The heap's name, with its team's when the person is in more than one team (as Waiting for me names it). */
function heapName(row: DecisionRow | ResultRow, me: Me): string {
  const team = me.teams.length > 1 ? titleTeam(me, heapOf(row)) : undefined;
  return team ? `${row.heap.name} · ${team}` : row.heap.name;
}

/** "note, with context", "task, defined". */
const stageWords = (row: DecisionRow) =>
  `${KIND_LABELS[row.item.kind].toLowerCase()}${row.item.stage ? `, ${STAGE_LABELS[row.item.stage].toLowerCase()}` : ""}`;

/**
 * What agents finished across the person's heaps (ux.md, Agents): Your decision, the items an agent brought to a stable
 * state that wait for the person, then the history of the runs that ended as reached, by day.
 */
export function AgentsPage() {
  const { data: me } = useMe();
  const decisions = useDecisions();
  const results = useResults();
  if (!me) return null;

  const rows = results.data?.pages.flatMap((page) => page.rows) ?? [];
  const decided = decisions.data?.rows ?? [];
  const pending = decisions.isPending || results.isPending;
  const failed = decisions.data === undefined || results.data === undefined;

  return (
    <main className="flex flex-col gap-3.5 text-sm max-sm:text-[15px]">
      <div className="mt-6 flex flex-col gap-1">
        <h1 className="text-[22px] font-semibold tracking-[-0.015em]">Agents</h1>
        <p className="text-[13px] text-ink-soft max-sm:text-sm">What agents finished, across your heaps.</p>
      </div>
      {pending ? (
        <p className="py-16 text-center text-muted">Loading…</p>
      ) : failed ? (
        <p role="alert" className="py-16 text-center text-overdue-ink">
          {(decisions.error ?? results.error)?.message}
        </p>
      ) : decisions.data.count === 0 && rows.length === 0 ? (
        <div className="py-16 text-center">
          <p>No agent has finished anything yet.</p>
          <p className="mt-1 text-[13px] text-muted">Start one in a terminal:</p>
          <pre className="mx-auto mt-2 w-fit rounded-lg border border-line bg-sunken px-3 py-2 text-left font-mono text-[13px]">
            upheap work run
          </pre>
        </div>
      ) : (
        <>
          {decisions.data.count > 0 && <DecisionsSection me={me} count={decisions.data.count} rows={decided} />}
          {rows.length > 0 && (
            <HistorySection
              me={me}
              rows={rows}
              hasMore={results.hasNextPage}
              loading={results.isFetchingNextPage}
              onMore={() => void results.fetchNextPage()}
            />
          )}
        </>
      )}
    </main>
  );
}

/** Your decision: folds to its heading, shows the first five rows, and the rest on "Show N more". */
function DecisionsSection({ me, count, rows }: { me: Me; count: number; rows: DecisionRow[] }) {
  const [folded, setFolded] = useState(readDecisionsFolded);
  // Not remembered: a reload starts with the first rows only.
  const [all, setAll] = useState(false);
  const shown = all ? rows : rows.slice(0, FIRST_DECISIONS);

  return (
    <section aria-label="Your decision" className="flex flex-col gap-1">
      <h2 className="text-base font-semibold">
        <button
          type="button"
          aria-expanded={!folded}
          onClick={() => {
            setFolded(!folded);
            writeDecisionsFolded(!folded);
          }}
          className="-ml-1 flex min-h-11 items-center gap-1.5 rounded-lg px-1 outline-none focus-visible:outline-2 focus-visible:outline-accent"
        >
          <ChevronDown aria-hidden="true" className={`size-4 text-ink-soft ${folded ? "-rotate-90" : ""}`} />
          Your decision · {count}
        </button>
      </h2>
      {!folded && (
        <>
          <ul className="flex flex-col divide-y divide-line">
            {shown.map((row) => (
              <li key={`${row.item.id}`} data-item={row.item.id} className="flex flex-col gap-0.5 py-2">
                <p className="flex flex-wrap items-baseline justify-between gap-x-3">
                  <ItemLink row={row} />
                  <span className="text-xs text-muted">{row.arrived}</span>
                </p>
                <p className="text-xs break-words text-muted">
                  {heapName(row, me)} · {row.became_label ?? stageWords(row)}
                </p>
              </li>
            ))}
          </ul>
          {rows.length > FIRST_DECISIONS && (
            <button
              type="button"
              onClick={() => setAll(!all)}
              className="min-h-11 self-start rounded-lg px-1 text-[13px] text-ink-soft underline underline-offset-2 outline-none hover:text-ink focus-visible:outline-2 focus-visible:outline-accent"
            >
              {all ? "Show less" : `Show ${rows.length - FIRST_DECISIONS} more`}
            </button>
          )}
          {count > rows.length && (
            <p className="text-[13px] text-muted">
              Showing the {rows.length} most important of {count}.
            </p>
          )}
        </>
      )}
    </section>
  );
}

/** The runs that ended as reached, grouped by the day they ended on. */
function HistorySection({
  me,
  rows,
  hasMore,
  loading,
  onMore,
}: {
  me: Me;
  rows: ResultRow[];
  hasMore: boolean;
  loading: boolean;
  onMore: () => void;
}) {
  const days: { day: string; label: string; rows: ResultRow[] }[] = [];
  for (const row of rows) {
    const last = days[days.length - 1];
    if (last?.day === row.day) last.rows.push(row);
    else days.push({ day: row.day, label: row.day_label, rows: [row] });
  }

  return (
    <section aria-label="History" className="flex flex-col gap-2">
      <h2 className="text-base font-semibold">History</h2>
      {days.map((group) => (
        <div key={group.day} className="flex flex-col">
          <h3 className="text-xs font-medium text-ink-soft">
            {group.label.charAt(0).toUpperCase() + group.label.slice(1)}
          </h3>
          <ul className="flex flex-col divide-y divide-line">
            {group.rows.map((row) => (
              <li key={row.run_id} data-run={row.run_id} className="flex flex-col gap-0.5 py-2">
                <p className="break-words">
                  <span className="mr-2 text-xs text-muted tabular-nums">{row.time}</span>
                  <ItemLink row={row} />
                  <span className="text-muted">{` · ${heapName(row, me)}`}</span>
                </p>
                <p className="text-xs text-muted">{row.reached_label ? `reached: ${row.reached_label}` : "reached"}</p>
                <p className="line-clamp-2 text-ink-soft">{row.result}</p>
                {row.links.length > 0 && (
                  <p className="flex flex-wrap gap-x-3 text-xs">
                    {row.links.map((link) => (
                      <a
                        key={link.url}
                        href={link.url}
                        target="_blank"
                        rel="noreferrer"
                        className="break-all text-accent-ink underline underline-offset-2 pointer-coarse:py-3"
                      >
                        {link.pull_request ? `↗ ${link.label}` : link.label}
                      </a>
                    ))}
                  </p>
                )}
              </li>
            ))}
          </ul>
        </div>
      ))}
      {hasMore ? (
        <button
          type="button"
          disabled={loading}
          onClick={onMore}
          className="min-h-11 self-start rounded-lg border border-line-strong px-4 text-sm outline-none hover:border-muted focus-visible:outline-2 focus-visible:outline-accent disabled:opacity-50"
        >
          Load more
        </button>
      ) : (
        <p className="text-[13px] text-muted">That's the last 30 days. Older runs are on their items.</p>
      )}
    </section>
  );
}
