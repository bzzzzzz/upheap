import { Link } from "@tanstack/react-router";
import { useEffect, useRef, useState } from "react";

import type { HeapRef, Question, WaitingRow } from "../api/client";
import { useHeap, useItem, useMe, useWaiting } from "../api/queries";
import { kindWord, QuestionRow } from "../components/ItemQuestions";
import { titleTeam } from "../components/teamName";

const rowKey = (row: WaitingRow) => `${row.item.id}:${row.question.number}`;
const heapOf = (row: WaitingRow): HeapRef => ({ team: row.heap.team_slug, heap: row.heap.slug });

/**
 * Every open question across the person's heaps, most important first, each answered in its row (ux.md, Waiting for
 * me). The rows are the item's Waits for you rows, with a head line that says which item and heap they are on.
 */
export function WaitingPage() {
  const { data: me } = useMe();
  const waiting = useWaiting();
  const heading = useRef<HTMLHeadingElement>(null);
  const list = useRef<HTMLUListElement>(null);
  const [message, setMessage] = useState("");
  const [elsewhere, setElsewhere] = useState<string[]>([]);
  // Replies refused because the item was closed meanwhile, with the comment that was lost, to copy.
  const [refused, setRefused] = useState<{ text: string; comment: string | null }[]>([]);
  // The row just replied to and where it was: once it has left, the focus goes on to the row that took its place.
  const repliedTo = useRef<{ key: string; index: number } | null>(null);
  // Only questions for now; a kind this client doesn't know is left out. The heading keeps the server's count.
  const rows = (waiting.data?.rows ?? []).filter((row) => row.kind === "question");

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
          `[data-item="${next.item.id}"][data-question="${next.question.number}"] :is(input, textarea, button):not([data-details]):not(:disabled)`,
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
      {waiting.isPending ? (
        <p className="py-16 text-center text-muted">Loading…</p>
      ) : waiting.data === undefined ? (
        <p role="alert" className="py-16 text-center text-overdue-ink">
          {waiting.error?.message}
        </p>
      ) : rows.length === 0 ? (
        <div className="py-16 text-center">
          <p>Nothing waits for you.</p>
          <p className="mt-1 text-[13px] text-muted">Questions from agents and teammates show up here.</p>
        </div>
      ) : (
        <>
          <ul ref={list} className="flex flex-col divide-y divide-line">
            {rows.map((row) => (
              <QuestionRow
                key={rowKey(row)}
                heapRef={heapOf(row)}
                item={row.item}
                question={row.question}
                line={<RowLine row={row} team={manyTeams ? titleTeam(me, heapOf(row)) : undefined} />}
                context={<RowContext heapRef={heapOf(row)} itemKey={row.item.key} heapName={row.heap.name} />}
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
            ))}
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

/** "Question · <item> · <heap> · <team> · step “…”": which item the question is on, and where. */
function RowLine({ row, team }: { row: WaitingRow; team: string | undefined }) {
  return (
    <p className="text-xs break-words text-muted">
      {kindWord(row.question)}
      {" · "}
      <Link
        to="/teams/$team/heaps/$heap/items/$item"
        params={{ ...heapOf(row), item: row.item.key }}
        // Closing the item goes back here, not to the heap's list.
        state={(prev) => ({ ...prev, closesBack: true })}
        // A finger gets a 44 px target; the negative margin keeps the line as tall as it was.
        className="font-medium text-accent-ink underline underline-offset-2 outline-none focus-visible:outline-2 focus-visible:outline-accent pointer-coarse:-my-4 pointer-coarse:inline-flex pointer-coarse:min-h-11 pointer-coarse:items-center"
      >
        {row.item.title}
      </Link>
      {` · ${row.heap.name}`}
      {team && ` · ${team}`}
      {row.step_title && ` · step “${row.step_title}”`}
    </p>
  );
}

/** The item's context and its heap's, as written. Fetched when this is shown, not with the page. */
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
        <p className="text-sm break-words whitespace-pre-wrap text-ink-soft">{text}</p>
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
