import type { Item, ItemRun, RunEnd } from "../api/client";
import { Disclosure } from "./AuditHistory";
import { GOAL_LABELS, STAGE_LABELS } from "./labels";

const END_LABELS: Record<RunEnd, string> = {
  reached: "reached",
  asked: "asked",
  failed: "failed",
  taken_back: "taken back",
  handed_back: "handed back",
};

/** US dollars, as "$0.31". */
const usd = (amount: number) => `$${amount.toFixed(2)}`;

/** How the run ended, in a word; "running" while it hasn't. */
const endWord = (run: ItemRun) => (run.end ? END_LABELS[run.end] : "running");

/** The label's tooltip: the tool's own start when it was recorded, else when the run started, in local time. */
const startTitle = (run: ItemRun) =>
  run.tool_started_at == null
    ? `Started ${new Date(run.started_at).toLocaleString()}`
    : `Tool started ${new Date(run.tool_started_at).toLocaleString()}`;

/**
 * An item's runs (ux.md, The item): a folded section after the questions, shown only when the item has runs. Opened,
 * a row for each run, newest first: when it started, from where to where, how it ended and what it cost, and under
 * it the result or the failure and the links. Read-only for every role, so it has no actions. Nothing is fetched
 * here: `item` is a single-item response, the only kind that carries the runs.
 */
export function ItemRuns({ item }: { item: Item }) {
  const runs = item.runs ?? [];
  if (runs.length === 0) return null;
  const label = `Runs ${runs.length}${item.runs_cost_usd == null ? "" : ` · ${usd(item.runs_cost_usd)}`}`;
  return (
    <section aria-label="Runs" className="border-t border-line pt-1">
      <Disclosure label={label} className="">
        <ul className="flex flex-col divide-y divide-line">
          {runs.map((run) => (
            <li key={run.id} className="flex flex-col gap-0.5 py-2 first:pt-0 last:pb-0">
              <p className="flex flex-wrap items-baseline gap-x-2 text-sm">
                <span title={startTitle(run)} className="font-medium">
                  {run.started_label}
                </span>
                <span className="text-ink-soft">
                  {STAGE_LABELS[run.from_stage].toLowerCase()} → {GOAL_LABELS[run.goal]}
                </span>
                <span className="text-ink-soft">{endWord(run)}</span>
                {run.cost_usd != null && <span className="ml-auto tabular-nums">{usd(run.cost_usd)}</span>}
              </p>
              {(run.result ?? run.failure) && (
                <p className="text-xs break-words text-ink-soft">{run.result ?? run.failure}</p>
              )}
              {run.links.map((link) => (
                <a
                  key={link}
                  href={link}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-xs break-all text-accent-ink underline underline-offset-2 outline-none focus-visible:outline-2 focus-visible:outline-accent pointer-coarse:inline-flex pointer-coarse:min-h-11 pointer-coarse:items-center"
                >
                  ↗ {link.replace(/^https?:\/\//, "")}
                </a>
              ))}
            </li>
          ))}
        </ul>
      </Disclosure>
    </section>
  );
}
