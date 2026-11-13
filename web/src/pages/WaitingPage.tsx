import { Link, useNavigate } from "@tanstack/react-router";
import { useEffect, useRef, useState, type ComponentProps } from "react";

import type { HeapRef, Question, WaitingRow } from "../api/client";
import { useHeap, useItem, useMe, useWaiting } from "../api/queries";
import { BlockerGoneRow, blockerGoneWord } from "../components/BlockerGoneRow";
import { kindWord, QuestionRow } from "../components/ItemQuestions";
import { proposalSummary } from "../components/proposalSummary";
import { Markdown } from "../components/Markdown";
import { StuckRow } from "../components/StuckRow";
import { titleTeam } from "../components/teamName";

/** A stuck turn's or a gone blocker's row has its item's ID and its kind for a key; a question's row has the question's number. */
const rowKey = (row: WaitingRow) =>
  row.kind === "question" ? `${row.item.id}:${row.question?.number}` : `${row.item.id}:${row.kind}`;

/** The attribute that marks a row's element, under its `data-item`. */
const rowMark = (row: WaitingRow) =>
  row.kind === "stuck"
    ? "[data-stuck]"
    : row.kind === "blocker_gone"
      ? "[data-blocker-gone]"
      : `[data-question="${row.question?.number}"]`;
const heapOf = (row: WaitingRow): HeapRef => ({ team: row.heap.team_slug, heap: row.heap.slug });

/**
 * Every open question across the person's heaps, every item whose agent turn is stuck, and every blocked item whose
 * blocker is gone, most important first, each answered or let go in its row (ux.md, Waiting for me). The rows are the item's open-question rows in Questions, with a head line
 * that says which item and heap they are on.
 */
export function WaitingPage() {
  const { data: me } = useMe();
  const waiting = useWaiting();
  const navigate = useNavigate();
  const heading = useRef<HTMLHeadingElement>(null);
  const list = useRef<HTMLUListElement>(null);
  const [message, setMessage] = useState("");
  const [elsewhere, setElsewhere] = useState<string[]>([]);
  // A stuck turn that someone else took back or retried first, or a blocked item someone else changed first.
  const [stuckChanged, setStuckChanged] = useState(false);
  // Replies refused because the item was closed meanwhile, with the comment that was lost, to copy.
  const [refused, setRefused] = useState<{ text: string; comment: string | null }[]>([]);
  // The row just replied to and where it was: once it has left, the focus goes on to the row that took its place.
  const repliedTo = useRef<{ key: string; index: number } | null>(null);
  // Questions, stuck turns and gone blockers; a kind this client doesn't know is left out. The heading keeps the
  // server's count.
  const rows = (waiting.data?.rows ?? []).filter(
    (row) =>
      (row.kind === "question" && row.question) ||
      (row.kind === "stuck" && row.stuck) ||
      (row.kind === "blocker_gone" && row.blocker),
  );

  useEffect(() => {
    const replied = repliedTo.current;
    if (replied === null || rows.some((row) => rowKey(row) === replied.key)) return;
    repliedTo.current = null;
    // Only when the focus was lost with the row; if the person has moved on, it stays where they put it.
    const active = document.activeElement;
    if (active && active !== document.body && active.isConnected) return;
    const next = rows[replied.index];
    const first = next
      ? list.current?.querySelector<HTMLElement>(
          `[data-item="${next.item.id}"]${rowMark(next)} :is(input, textarea, button, a[aria-label]):not([data-details]):not(:disabled)`,
        )
      : undefined;
    (first ?? heading.current)?.focus();
  });

  if (!me) return null;
  const count = waiting.data?.count ?? 0;
  // A person belongs to one team or several; the team's name helps only when there are several.
  const manyTeams = me.teams.length > 1;
  const track = (row: WaitingRow) => {
    repliedTo.current = { key: rowKey(row), index: rows.indexOf(row) };
  };

  return (
    <main className="flex flex-col gap-3.5 text-sm max-sm:text-[15px]">
      <span role="status" className="sr-only">
        {message}
      </span>
      <div className="mt-6 flex flex-col gap-1">
        <h1 ref={heading} tabIndex={-1} className="text-[22px] font-semibold tracking-[-0.015em] outline-none">
          Waiting for me{count > 0 && ` · ${count}`}
        </h1>
        <p className="text-[13px] text-ink-soft max-sm:text-sm">All heaps, most important first</p>
      </div>
      {refused.map((entry, index) => (
        <p key={index} role="alert" className="text-sm whitespace-pre-wrap text-overdue-ink">
          The item was closed before your reply to “{entry.text}” was sent.
          {entry.comment && `\nYour comment: ${entry.comment}`}
        </p>
      ))}
      {elsewhere.map((text, index) => (
        <p key={index} role="alert" className="text-sm text-overdue-ink">
          Someone else replied to “{text}” meanwhile.
        </p>
      ))}
      {stuckChanged && (
        <p role="alert" className="text-sm text-overdue-ink">
          Someone else changed this meanwhile.
        </p>
      )}
      {waiting.isPending ? (
        <p className="py-16 text-center text-muted">Loading…</p>
      ) : waiting.data === undefined ? (
        <p role="alert" className="py-16 text-center text-overdue-ink">
          {waiting.error?.message}
        </p>
      ) : rows.length === 0 ? (
        <div className="py-16 text-center">
          <p>Nothing waits for you.</p>
          <p className="mt-1 text-[13px] text-muted">
            Questions from agents and teammates, and work whose blocker was closed, show up here.
          </p>
        </div>
      ) : (
        <>
          <ul ref={list} className="flex flex-col divide-y divide-line">
            {rows.map((row) =>
              row.kind === "blocker_gone" && row.blocker ? (
                <BlockerGoneRow
                  key={rowKey(row)}
                  heapRef={heapOf(row)}
                  item={row.item}
                  blocker={row.blocker}
                  open
                  line={<RowLine row={row} team={manyTeams ? titleTeam(me, heapOf(row)) : undefined} />}
                  onUnblocked={() => {
                    track(row);
                    setStuckChanged(false);
                    setMessage(`Unblocked “${row.item.title}”.`);
                  }}
                  onElsewhere={() => {
                    track(row);
                    setStuckChanged(true);
                  }}
                />
              ) : row.kind === "stuck" && row.stuck ? (
                <StuckRow
                  key={rowKey(row)}
                  heapRef={heapOf(row)}
                  item={row.item}
                  stuck={row.stuck}
                  line={<RowLine row={row} team={manyTeams ? titleTeam(me, heapOf(row)) : undefined} />}
                  onRetried={() => {
                    track(row);
                    setStuckChanged(false);
                    setMessage(`Retried “${row.item.title}”.`);
                  }}
                  onTakenBack={() => {
                    track(row);
                    setStuckChanged(false);
                    setMessage(`Took back “${row.item.title}”.`);
                  }}
                  onElsewhere={() => {
                    track(row);
                    setStuckChanged(true);
                  }}
                />
              ) : (
                row.question && (
                  <QuestionOrProposal
                    key={rowKey(row)}
                    heapRef={heapOf(row)}
                    item={row.item}
                    question={row.question}
                    line={<RowLine row={row} team={manyTeams ? titleTeam(me, heapOf(row)) : undefined} />}
                    context={<RowContext heapRef={heapOf(row)} itemKey={row.item.key} heapName={row.heap.name} />}
                    title={row.item.title}
                    openTo={() =>
                      void navigate({
                        to: "/teams/$team/heaps/$heap/items/$item",
                        params: { ...heapOf(row), item: row.item.key },
                        state: (prev) => ({ ...prev, closesBack: true }),
                      })
                    }
                    onReplied={(question: Question) => {
                      track(row);
                      setElsewhere([]);
                      setRefused([]);
                      setMessage(`Replied to “${question.text}”.`);
                    }}
                    onElsewhere={(question: Question) => {
                      track(row);
                      setElsewhere((current) => [...current, question.text]);
                    }}
                    onRefused={(question: Question, comment: string | null) => {
                      track(row);
                      setRefused((current) => [...current, { text: question.text, comment }]);
                    }}
                  />
                )
              ),
            )}
          </ul>
          {count > rows.length && (
            <p className="text-[13px] text-muted">
              Showing the {rows.length} most important of {count}.
            </p>
          )}
        </>
      )}
    </main>
  );
}

/** A question's row; a done proposal's has its summary too. */
function QuestionOrProposal(props: ComponentProps<typeof QuestionRow>) {
  return props.question.kind === "done_proposal" ? <ProposalRow {...props} /> : <QuestionRow {...props} />;
}

/** A done proposal's row, with its summary from the item, which is fetched for it alone (and shared with Context). */
function ProposalRow(props: ComponentProps<typeof QuestionRow>) {
  const item = useItem(props.heapRef, props.item.key);
  return <QuestionRow {...props} summary={item.data ? proposalSummary(item.data) : undefined} />;
}

/**
 * "Question · <item> · <heap> · <team> · step “…”", "Stuck · <item> · <heap> · <team>" or "Blocker closed · <item> ·
 * <heap> · <team>": which item the row is on, and where.
 */
function RowLine({ row, team }: { row: WaitingRow; team: string | undefined }) {
  return (
    <p className="text-xs break-words text-muted">
      {row.kind === "blocker_gone" && row.blocker
        ? blockerGoneWord(row.blocker.gone)
        : row.kind === "stuck" || !row.question
          ? "Stuck"
          : kindWord(row.question)}
      {" · "}
      <Link
        to="/teams/$team/heaps/$heap/items/$item"
        params={{ ...heapOf(row), item: row.item.key }}
        // Closing the item goes back here, not to the heap's list.
        state={(prev) => ({ ...prev, closesBack: true })}
        // What closing the item focuses on the way back (`focusRowOf`), as a list row's link is.
        data-row-link={row.item.key}
        // A finger gets a 44 px target: padding on an inline link grows what it catches without changing the line, so a
        // long title still wraps in place.
        className="font-medium text-accent-ink underline underline-offset-2 outline-none focus-visible:outline-2 focus-visible:outline-accent pointer-coarse:py-4"
      >
        {row.item.title}
      </Link>
      {` · ${row.heap.name}`}
      {team && ` · ${team}`}
      {row.step_title && ` · step “${row.step_title}”`}
    </p>
  );
}

/** The item's context and its heap's, as Markdown text. Fetched when this is shown, not with the page. */
function RowContext({ heapRef, itemKey, heapName }: { heapRef: HeapRef; itemKey: string; heapName: string }) {
  const item = useItem(heapRef, itemKey);
  const heap = useHeap(heapRef);
  const part = (title: string, query: { isPending: boolean; error: Error | null }, text: string | null | undefined) => (
    <div className="flex flex-col gap-0.5">
      <h2 className="text-xs font-medium text-ink-soft">{title}</h2>
      {query.isPending ? (
        <p className="text-sm text-muted">Loading…</p>
      ) : query.error ? (
        <p role="alert" className="text-sm text-overdue-ink">
          {query.error.message}
        </p>
      ) : text ? (
        <Markdown text={text} className="text-ink-soft" />
      ) : (
        <p className="text-sm text-muted">No context</p>
      )}
    </div>
  );
  return (
    <div className="flex flex-col gap-2 pl-1">
      {part("This item", item, item.data?.description)}
      {part(heapName, heap, heap.data?.context)}
    </div>
  );
}
