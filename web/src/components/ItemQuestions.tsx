import { ChevronRight } from "lucide-react";
import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { Button, Checkbox, CheckboxGroup, Label, Radio, RadioGroup, TextArea, TextField } from "react-aria-components";

import {
  ApiProblem,
  type HeapRef,
  type Item,
  type Question,
  type QuestionReply,
  type WaitingBlocker,
  type WaitingStuck,
} from "../api/client";
import { useReplyToQuestion } from "../api/queries";
import { BlockerGoneRow, blockerGoneWord } from "./BlockerGoneRow";
import { compactButtonClass } from "./buttons";
import { itemProblem } from "./itemProblems";
import { GOAL_LABELS, isClosed } from "./labels";
import { Markdown } from "./Markdown";
import { StuckRow } from "./StuckRow";
import { useUserName } from "./userName";

export const focusClass = "outline-none data-[focus-visible]:outline-2 data-[focus-visible]:outline-accent";
const detailsButtonClass = `-ml-1 flex min-h-9 items-center gap-1 self-start rounded-lg px-1 text-sm text-ink-soft hover:text-ink ${focusClass} pointer-coarse:min-h-11`;
export const fieldClass =
  "field-sizing-content min-h-14 w-full rounded-lg border border-line bg-surface px-2 py-1.5 text-base outline-none focus:border-accent";
const labelClass = "text-xs font-medium text-ink-soft";
const pickClass = `group flex min-h-9 items-center gap-2 text-base ${focusClass} pointer-coarse:min-h-11`;
const pickBoxClass =
  "flex size-5 shrink-0 items-center justify-center border border-line-strong bg-surface group-data-[focus-visible]:outline-2 group-data-[focus-visible]:outline-accent group-data-[selected]:border-ink group-data-[selected]:bg-ink";

const isClosedMeanwhile = (error: Error) => error instanceof ApiProblem && error.code === "item_closed";

/** The id of a question's short text, which its controls are described by. */
const textId = (itemId: string, question: Question) => `question-text-${itemId}-${question.number}`;

/** The replies that mean the question is answered already, so the item is fetched again and the row leaves. */
const isAnsweredMeanwhile = (error: Error) =>
  error instanceof ApiProblem && (error.code === "version_conflict" || error.code === "question_answered");

/** "Question", "Question · pick any" or "Request": what the question is. */
export function kindWord(question: Question) {
  return `${question.kind === "request" ? "Request" : "Question"}${question.many ? " · pick any" : ""}`;
}

/** "Question", "Question · pick any", "Request", then the step it names. */
function kindLine(item: Item, question: Question) {
  const parts = [kindWord(question)];
  if (question.step !== null && question.step !== undefined) {
    const title = item.steps?.find((step) => step.number === question.step)?.title;
    parts.push(title ? `step “${title}”` : `step ${question.step}`);
  }
  return parts.join(" · ");
}

/** A button that folds `children` open under it. */
function Disclosure({ label, children }: { label: string; children: ReactNode }) {
  const [shown, setShown] = useState(false);
  return (
    <>
      <Button
        onPress={() => setShown((current) => !current)}
        aria-expanded={shown}
        data-details=""
        className={detailsButtonClass}
      >
        <ChevronRight
          aria-hidden="true"
          className={`size-3.5 transition-transform motion-reduce:transition-none ${shown ? "rotate-90" : ""}`}
        />
        {label}
      </Button>
      {shown && children}
    </>
  );
}

/**
 * A question's head line, its short text and its details behind a disclosure: what every row shows. `context`, for a
 * row that sits away from its item, is folded under the details, or alone as "Context" when there are none.
 */
function QuestionHead({
  itemId,
  line,
  question,
  context,
}: {
  itemId: string;
  line: ReactNode;
  question: Question;
  context?: ReactNode;
}) {
  const details = question.details ? <Markdown text={question.details} className="text-ink-soft" /> : null;
  let folded: ReactNode = null;
  if (details && context) {
    folded = (
      <Disclosure label="Details">
        {details}
        <Disclosure label="Context">{context}</Disclosure>
      </Disclosure>
    );
  } else if (details) {
    folded = <Disclosure label="Details">{details}</Disclosure>;
  } else if (context) {
    folded = <Disclosure label="Context">{context}</Disclosure>;
  }
  return (
    <>
      {line}
      <p id={textId(itemId, question)} className="text-base leading-snug font-semibold break-words">
        {question.text}
      </p>
      {folded}
    </>
  );
}

/** The stuck row an item shows in Waits for you, from its turn, or none when its turn isn't stuck. */
function stuckOf(item: Item): WaitingStuck | null {
  const turn = item.turn;
  if (!turn?.stuck || isClosed(item.state)) return null;
  return {
    failed_runs: turn.failed_runs,
    reason: turn.stuck_reason ?? null,
    goal: turn.goal,
    allowed_actions: item.allowed_actions.filter((action) => action === "retry" || action === "take_back"),
  };
}

/** The gone-blocker row an item shows in Waits for you, or none while its blocker holds it (or it has none). */
function blockerGoneOf(item: Item): WaitingBlocker | null {
  if (item.state !== "blocked" || !item.blocked_by || !item.blocker_gone) return null;
  return {
    item: item.blocked_by,
    gone: item.blocker_gone,
    allowed_actions: item.allowed_actions.filter((action) => action === "change_state"),
  };
}

/**
 * What an item is waiting for the person to answer (ux.md, The item): the open questions, which sit in the Questions
 * section, and the item's stuck turn, when it has one, which sits in Waits for you at the top. The two share what a
 * reply or a retry leaves to say, and where focus goes next, so it is kept here, by the item, for both to use. Both
 * show only while the item is open: closing withdraws its open questions, which {@link AnsweredQuestions} lists.
 */
export function useItemWaits(item: Item) {
  const closed = isClosed(item.state);
  const open = closed ? [] : (item.questions ?? []).filter((question) => question.state === "open");
  // The id of the open questions' list, found by id because the hook's result is passed around and a ref isn't.
  const listId = useId();
  const [message, setMessage] = useState("");
  const [elsewhere, setElsewhere] = useState<string[]>([]);
  // The stuck turn was taken back or retried by someone else first.
  const [stuckChanged, setStuckChanged] = useState(false);
  // Replies refused because the item was closed meanwhile, with the comment that was lost, to copy.
  const [refused, setRefused] = useState<{ text: string; comment: string | null }[]>([]);
  // The number of the question just replied to: once its row has left, focus goes on to the next one.
  const repliedTo = useRef<number | null>(null);
  const stuck = stuckOf(item);
  const blocker = blockerGoneOf(item);
  // Set when the stuck or gone-blocker row was acted on here: once it has left, focus goes to the item, as there is
  // no next row.
  const actedOnStuck = useRef(false);

  useEffect(() => {
    if (!actedOnStuck.current || stuck || blocker) return;
    actedOnStuck.current = false;
    const active = document.activeElement;
    if (active && active !== document.body && active.isConnected) return;
    document.querySelector<HTMLElement>("[data-item-pane]")?.focus();
  });

  useEffect(() => {
    const after = repliedTo.current;
    if (after === null || open.some((question) => question.number === after)) return;
    repliedTo.current = null;
    const next = open.find((question) => question.number > after) ?? open[0];
    // Only when the focus was lost with the row; if the person has moved on, it stays where they put it.
    const active = document.activeElement;
    if (active && active !== document.body && active.isConnected) return;
    const first = next
      ? firstControl(document.getElementById(listId), next.number)
      : document.querySelector<HTMLElement>("[data-item-pane]");
    first?.focus();
  });

  const replied = (question: Question) => {
    repliedTo.current = question.number;
    setElsewhere([]);
    setRefused([]);
    setMessage(`Replied to “${question.text}”.`);
  };
  const repliedElsewhere = (question: Question) => {
    repliedTo.current = question.number;
    setElsewhere((current) => [...current, question.text]);
  };
  const refusedReply = (question: Question, comment: string | null) => {
    repliedTo.current = question.number;
    setRefused((current) => [...current, { text: question.text, comment }]);
  };
  const retried = () => {
    actedOnStuck.current = true;
    setStuckChanged(false);
    setMessage(`Retried “${item.title}”.`);
  };
  const tookBack = () => {
    actedOnStuck.current = true;
    setStuckChanged(false);
    setMessage(`Took back “${item.title}”.`);
  };
  const unblocked = () => {
    actedOnStuck.current = true;
    setStuckChanged(false);
    setMessage(`Unblocked “${item.title}”.`);
  };

  return {
    closed,
    open,
    stuck,
    blocker,
    listId,
    message,
    elsewhere,
    refused,
    stuckChanged,
    setStuckChanged,
    replied,
    repliedElsewhere,
    refusedReply,
    retried,
    tookBack,
    unblocked,
  };
}

export type ItemWaits = ReturnType<typeof useItemWaits>;

/** The first control of the open question numbered `number` in `list`: a reply's first input or button. */
function firstControl(list: HTMLElement | null, number: number) {
  return list?.querySelector<HTMLElement>(
    `[data-question="${number}"] :is(input, textarea, button):not([data-details]):not(:disabled)`,
  );
}

/**
 * Waits for you, the one tinted, outlined block at the top of the item (ux.md, The item): the gone blocker, when it
 * has one, with Unblock, the stuck turn, when it has one, with Retry and Take back, and one line saying how many questions wait, with Answer to go to the first. The
 * reply controls are in {@link ItemQuestions}, under the Steps. It stays mounted when empty, for the live message
 * after the last reply and the note about a stuck turn someone else dealt with.
 */
export function WaitsForYou({ heapRef, item, waits }: { heapRef: HeapRef; item: Item; waits: ItemWaits }) {
  const { open, stuck, blocker } = waits;
  const answer = () => {
    const first = open[0];
    if (!first) return;
    const list = document.getElementById(waits.listId);
    list?.querySelector(`[data-question="${first.number}"]`)?.scrollIntoView({ block: "nearest" });
    firstControl(list, first.number)?.focus({ preventScroll: true });
  };
  return (
    <>
      <span role="status" className="sr-only">
        {waits.message}
      </span>
      {(blocker || stuck || open.length > 0) && (
        <section
          aria-label="Waits for you"
          className="flex flex-col gap-1.5 rounded-xl border-2 border-accent bg-accent/10 p-3"
        >
          <h3 className="text-xs font-medium text-accent-ink">Waits for you</h3>
          <div className="flex flex-col divide-y divide-accent/30">
            {blocker && (
              <ul className="py-3 first:pt-0 last:pb-0">
                <BlockerGoneRow
                  heapRef={heapRef}
                  item={item}
                  blocker={blocker}
                  line={<p className="text-xs text-muted">{blockerGoneWord(blocker.gone)}</p>}
                  onUnblocked={waits.unblocked}
                  onElsewhere={() => waits.setStuckChanged(true)}
                />
              </ul>
            )}
            {stuck && (
              <ul className="py-3 first:pt-0 last:pb-0">
                <StuckRow
                  heapRef={heapRef}
                  item={item}
                  stuck={stuck}
                  line={<p className="text-xs text-muted">Stuck · going to: {GOAL_LABELS[stuck.goal]}</p>}
                  onRetried={waits.retried}
                  onTakenBack={waits.tookBack}
                  onElsewhere={() => waits.setStuckChanged(true)}
                />
              </ul>
            )}
            {open.length > 0 && (
              <div className="flex items-center justify-between gap-3 py-3 first:pt-0 last:pb-0">
                <p className="text-sm">
                  {open.length === 1 ? "1 question waits for you" : `${open.length} questions wait for you`}
                </p>
                <Button onPress={answer} className={`${compactButtonClass.bordered} shrink-0 ${focusClass}`}>
                  Answer ↓
                </Button>
              </div>
            )}
          </div>
        </section>
      )}
      {!waits.closed && waits.stuckChanged && (
        <p role="alert" className="text-sm text-overdue-ink">
          Someone else changed this meanwhile.
        </p>
      )}
    </>
  );
}

/**
 * The Questions section (ux.md, The item): the open questions with their reply controls in the outlined box, then
 * what {@link AnsweredQuestions} folds. Shown when the item has any question. A row is shown with controls when the
 * server offers `reply` on the question, otherwise (an archived heap) as text. After a reply the row leaves and joins
 * the fold.
 */
export function ItemQuestions({ heapRef, item, waits }: { heapRef: HeapRef; item: Item; waits: ItemWaits }) {
  if ((item.questions ?? []).length === 0) return null;
  return (
    <section aria-label="Questions" className="flex flex-col gap-2 border-t border-line pt-3">
      <h3 className="text-xs font-semibold text-muted">Questions</h3>
      {waits.open.length > 0 && (
        <div role="group" aria-label="Open questions" className="rounded-xl border-2 border-accent bg-accent/10 p-3">
          <ul id={waits.listId} className="flex flex-col divide-y divide-accent/30">
            {waits.open.map((question) => (
              <QuestionRow
                key={question.number}
                heapRef={heapRef}
                item={item}
                line={<p className="text-xs text-muted">{kindLine(item, question)}</p>}
                question={question}
                onReplied={waits.replied}
                onElsewhere={waits.repliedElsewhere}
                onRefused={waits.refusedReply}
              />
            ))}
          </ul>
        </div>
      )}
      {waits.refused.map((entry, index) => (
        <p key={index} role="alert" className="text-sm whitespace-pre-wrap text-overdue-ink">
          The item was closed before your reply to “{entry.text}” was sent.
          {entry.comment && `\nYour comment: ${entry.comment}`}
        </p>
      ))}
      {!waits.closed &&
        waits.elsewhere.map((text, index) => (
          <p key={index} role="alert" className="text-sm text-overdue-ink">
            Someone else replied to “{text}” meanwhile.
          </p>
        ))}
      <AnsweredQuestions item={item} />
    </section>
  );
}

/**
 * One open question with its reply controls, in the item's Questions and on the Waiting for me page. `line` is the
 * row's own first line, and `context` what the page folds under the details. The reply and its failures are the
 * same in both: the caller says what to do once the row has been replied to, or turns out to be answered already or
 * on a closed item.
 */
export function QuestionRow({
  heapRef,
  item,
  line,
  question,
  context,
  onReplied,
  onElsewhere,
  onRefused,
}: {
  heapRef: HeapRef;
  item: { id: string; key: string };
  line: ReactNode;
  question: Question;
  context?: ReactNode;
  onReplied: (question: Question) => void;
  onElsewhere: (question: Question) => void;
  onRefused: (question: Question, comment: string | null) => void;
}) {
  const reply = useReplyToQuestion(heapRef, item.key);
  const canReply = question.allowed_actions.includes("reply");

  const send = (body: QuestionReply) =>
    reply.mutateAsync({ question, reply: body }).then(
      () => onReplied(question),
      (error: Error) => {
        if (isAnsweredMeanwhile(error)) onElsewhere(question);
        else if (isClosedMeanwhile(error)) onRefused(question, body.comment ?? null);
      },
    );

  return (
    <li data-item={item.id} data-question={question.number} className="flex flex-col gap-1.5 py-3 first:pt-0 last:pb-0">
      <QuestionHead itemId={item.id} line={line} question={question} context={context} />
      {canReply && (
        <ReplyControls
          describedBy={textId(item.id, question)}
          question={question}
          pending={reply.isPending}
          onSend={send}
        />
      )}
      {reply.error && !isAnsweredMeanwhile(reply.error) && !isClosedMeanwhile(reply.error) && (
        <p role="alert" className="text-sm text-overdue-ink">
          {itemProblem(reply.error)}
        </p>
      )}
    </li>
  );
}

/** The controls for one open question: its drafts (picks and comment) live here, so a refetch leaves them alone. */
function ReplyControls({
  describedBy,
  question,
  pending,
  onSend,
}: {
  describedBy: string;
  question: Question;
  pending: boolean;
  onSend: (reply: QuestionReply) => void;
}) {
  const [comment, setComment] = useState("");
  const [picked, setPicked] = useState<string[]>([]);
  const trimmed = comment.trim();
  const note = trimmed === "" ? null : comment;

  const commentField = (label: string) => (
    <TextField value={comment} onChange={setComment} aria-describedby={describedBy} className="flex flex-col gap-1">
      <Label className={labelClass}>{label}</Label>
      <TextArea rows={2} className={fieldClass} />
    </TextField>
  );

  if (question.kind === "request") {
    return (
      <div className="flex flex-col gap-2">
        {commentField("Comment, optional")}
        <div className="flex gap-2 max-sm:grid max-sm:grid-cols-2">
          {question.choices.map((label, index) => (
            <Button
              key={label}
              aria-describedby={describedBy}
              isDisabled={pending}
              onPress={() => onSend({ picked: [index], comment: note })}
              className={`${index === 0 ? compactButtonClass.primary : compactButtonClass.bordered} ${focusClass}`}
            >
              {label}
            </Button>
          ))}
        </div>
      </div>
    );
  }

  if (question.choices.length === 0) {
    return (
      <div className="flex flex-col gap-2">
        <TextField
          value={comment}
          onChange={setComment}
          isRequired
          aria-describedby={describedBy}
          className="flex flex-col gap-1"
        >
          <Label className={labelClass}>Answer</Label>
          <TextArea rows={2} className={fieldClass} />
        </TextField>
        <Button
          aria-describedby={describedBy}
          isDisabled={pending || trimmed === ""}
          onPress={() => onSend({ picked: [], comment })}
          className={`${compactButtonClass.primary} ${focusClass} self-start max-sm:w-full`}
        >
          Reply
        </Button>
      </div>
    );
  }

  const indexed = (index: number) => ({ value: String(index), label: question.choices[index] });
  const choices = question.choices.map((_, index) => indexed(index));
  const group = question.many ? (
    <CheckboxGroup aria-label={question.text} value={picked} onChange={setPicked} className="flex flex-col">
      {choices.map((choice) => (
        <Checkbox key={choice.value} value={choice.value} className={pickClass}>
          {({ isSelected }) => (
            <>
              <span aria-hidden="true" className={`${pickBoxClass} rounded-md`}>
                {isSelected && <span className="size-2 rounded-sm bg-surface" />}
              </span>
              {choice.label}
            </>
          )}
        </Checkbox>
      ))}
    </CheckboxGroup>
  ) : (
    <RadioGroup
      aria-label={question.text}
      value={picked[0] ?? null}
      onChange={(value) => setPicked([value])}
      className="flex flex-col"
    >
      {choices.map((choice) => (
        <Radio key={choice.value} value={choice.value} className={pickClass}>
          {({ isSelected }) => (
            <>
              <span aria-hidden="true" className={`${pickBoxClass} rounded-full`}>
                {isSelected && <span className="size-2 rounded-full bg-surface" />}
              </span>
              {choice.label}
            </>
          )}
        </Radio>
      ))}
    </RadioGroup>
  );

  return (
    <div className="flex flex-col gap-2">
      {group}
      {commentField("Comment, optional")}
      <Button
        aria-describedby={describedBy}
        isDisabled={pending || picked.length === 0}
        onPress={() => onSend({ picked: picked.map(Number).sort((a, b) => a - b), comment: note })}
        className={`${compactButtonClass.primary} ${focusClass} self-start max-sm:w-full`}
      >
        Reply
      </Button>
    </div>
  );
}

/**
 * The questions that don't wait, folded under the open ones: the answered ones and the ones closing the item withdrew.
 * The summary counts the answered, and the withdrawn when there are no answered, as "2 answered" or "1 closed".
 * Read-only.
 */
function AnsweredQuestions({ item }: { item: Item }) {
  const listed = (item.questions ?? []).filter(
    (question) => question.state === "answered" || question.state === "withdrawn",
  );
  if (listed.length === 0) return null;
  const answered = listed.filter((question) => question.state === "answered").length;
  const closed = listed.length - answered;
  const summary = answered > 0 ? `${answered} answered${closed > 0 ? `, ${closed} closed` : ""}` : `${closed} closed`;
  return (
    <details className="group">
      <summary className="inline-flex min-h-9 cursor-pointer list-none items-center gap-1 text-[13px] font-medium text-ink-soft select-none hover:text-ink pointer-coarse:min-h-11 [&::-webkit-details-marker]:hidden">
        <ChevronRight
          aria-hidden="true"
          className="size-3.5 transition-transform group-open:rotate-90 motion-reduce:transition-none"
        />
        {summary}
      </summary>
      <ul className="mt-1 flex flex-col divide-y divide-line pl-3">
        {listed.map((question) => (
          <li key={question.number} className="flex flex-col gap-1 py-3 first:pt-1 last:pb-1">
            <QuestionHead
              itemId={item.id}
              line={<p className="text-xs text-muted">{kindLine(item, question)}</p>}
              question={question}
            />
            <Outcome question={question} />
          </li>
        ))}
      </ul>
    </details>
  );
}

/** What became of a listed question: the reply and who made it, or that closing the item withdrew it. */
function Outcome({ question }: { question: Question }) {
  const by = useUserName(question.reply?.replied_by);
  const reply = question.reply;
  if (question.state === "withdrawn") return <p className="text-sm text-muted">Withdrawn</p>;
  if (!reply) return null;
  const labels = reply.picked.map((index) => question.choices[index]).join(", ");
  const who = by ? `${by.charAt(0).toUpperCase()}${by.slice(1)}, ` : "";
  return (
    <>
      {labels && <p className="text-sm break-words">→ {labels}</p>}
      {reply.comment && <Markdown text={reply.comment} />}
      <p className="text-xs text-muted">
        {who}
        {reply.replied_label}
      </p>
    </>
  );
}
