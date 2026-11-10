import { ChevronRight } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button, Checkbox, CheckboxGroup, Label, Radio, RadioGroup, TextArea, TextField } from "react-aria-components";

import { ApiProblem, type HeapRef, type Item, type Question, type QuestionReply } from "../api/client";
import { useReplyToQuestion } from "../api/queries";
import { compactButtonClass } from "./buttons";
import { itemProblem } from "./itemProblems";
import { isClosed } from "./labels";
import { useUserName } from "./userName";

const focusClass = "outline-none data-[focus-visible]:outline-2 data-[focus-visible]:outline-accent";
const detailsButtonClass = `-ml-1 flex min-h-9 items-center gap-1 self-start rounded-lg px-1 text-sm text-ink-soft hover:text-ink ${focusClass} pointer-coarse:min-h-11`;
const fieldClass =
  "field-sizing-content min-h-14 w-full rounded-lg border border-line bg-surface px-2 py-1.5 text-base outline-none focus:border-accent";
const labelClass = "text-xs font-medium text-ink-soft";
const pickClass = `group flex min-h-9 items-center gap-2 text-base ${focusClass} pointer-coarse:min-h-11`;
const pickBoxClass =
  "flex size-5 shrink-0 items-center justify-center border border-line-strong bg-surface group-data-[focus-visible]:outline-2 group-data-[focus-visible]:outline-accent group-data-[selected]:border-ink group-data-[selected]:bg-ink";

const isClosedMeanwhile = (error: Error) => error instanceof ApiProblem && error.code === "item_closed";

/** The id of a question's short text, which its controls are described by. */
const textId = (item: Item, question: Question) => `question-text-${item.id}-${question.number}`;

/** The replies that mean the question is answered already, so the item is fetched again and the row leaves. */
const isAnsweredMeanwhile = (error: Error) =>
  error instanceof ApiProblem && (error.code === "version_conflict" || error.code === "question_answered");

/** "Question", "Question · pick any", "Request", then the step it names. */
function kindLine(item: Item, question: Question) {
  const parts = [question.kind === "request" ? "Request" : "Question"];
  if (question.many) parts.push("pick any");
  if (question.step !== null && question.step !== undefined) {
    const title = item.steps?.find((step) => step.number === question.step)?.title;
    parts.push(title ? `step “${title}”` : `step ${question.step}`);
  }
  return parts.join(" · ");
}

/** A question's kind line, its short text and its details behind a disclosure: what every row shows. */
function QuestionHead({ item, question }: { item: Item; question: Question }) {
  const [shown, setShown] = useState(false);
  return (
    <>
      <p className="text-xs text-muted">{kindLine(item, question)}</p>
      <p id={textId(item, question)} className="text-base leading-snug font-semibold break-words">
        {question.text}
      </p>
      {question.details && (
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
            Details
          </Button>
          {shown && <p className="text-sm break-words whitespace-pre-wrap text-ink-soft">{question.details}</p>}
        </>
      )}
    </>
  );
}

/**
 * What an item is waiting for the person to answer (ux.md, The item): one row per open question, each with its reply
 * controls. Shown only while the item is open; a closed item's open questions are read in {@link AnsweredQuestions}.
 * A row is shown when the server offers `reply` on the question, otherwise (an archived heap) as text. It stays
 * mounted when empty, for the live message after the last reply and the note about a reply someone else made.
 */
export function WaitsForYou({ heapRef, item }: { heapRef: HeapRef; item: Item }) {
  const closed = isClosed(item.state);
  const open = closed ? [] : (item.questions ?? []).filter((question) => question.state === "open");
  const listRef = useRef<HTMLUListElement>(null);
  const [message, setMessage] = useState("");
  const [elsewhere, setElsewhere] = useState<string[]>([]);
  // Replies refused because the item was closed meanwhile, with the comment that was lost, to copy.
  const [refused, setRefused] = useState<{ text: string; comment: string | null }[]>([]);
  // The number of the question just replied to: once its row has left, focus goes on to the next one.
  const repliedTo = useRef<number | null>(null);

  useEffect(() => {
    const after = repliedTo.current;
    if (after === null || open.some((question) => question.number === after)) return;
    repliedTo.current = null;
    const next = open.find((question) => question.number > after) ?? open[0];
    // Only when the focus was lost with the row; if the person has moved on, it stays where they put it.
    const active = document.activeElement;
    if (active && active !== document.body && active.isConnected) return;
    const first = next
      ? listRef.current?.querySelector<HTMLElement>(
          `[data-question="${next.number}"] :is(input, textarea, button):not([data-details]):not(:disabled)`,
        )
      : document.querySelector<HTMLElement>("[data-item-pane]");
    first?.focus();
  });

  const onReplied = (question: Question) => {
    repliedTo.current = question.number;
    setElsewhere([]);
    setRefused([]);
    setMessage(`Replied to “${question.text}”.`);
  };
  const onElsewhere = (question: Question) => {
    repliedTo.current = question.number;
    setElsewhere((current) => [...current, question.text]);
  };

  const onRefused = (question: Question, comment: string | null) => {
    repliedTo.current = question.number;
    setRefused((current) => [...current, { text: question.text, comment }]);
  };

  return (
    <>
      <span role="status" className="sr-only">
        {message}
      </span>
      {open.length > 0 && (
        <section
          aria-label="Waits for you"
          className="flex flex-col gap-1.5 rounded-xl border-2 border-accent bg-accent/10 p-3"
        >
          <h3 className="text-xs font-medium text-accent-ink">Waits for you</h3>
          <ul ref={listRef} className="flex flex-col divide-y divide-accent/30">
            {open.map((question) => (
              <QuestionRow
                key={question.number}
                heapRef={heapRef}
                item={item}
                question={question}
                onReplied={onReplied}
                onElsewhere={onElsewhere}
                onRefused={onRefused}
              />
            ))}
          </ul>
        </section>
      )}
      {/* Under the state buttons, where this stays mounted when the item closes and the section goes. */}
      {refused.map((entry, index) => (
        <p key={index} role="alert" className="text-sm whitespace-pre-wrap text-overdue-ink">
          The item was closed before your reply to “{entry.text}” was sent.
          {entry.comment && `\nYour comment: ${entry.comment}`}
        </p>
      ))}
      {!closed &&
        elsewhere.map((text, index) => (
          <p key={index} role="alert" className="text-sm text-overdue-ink">
            Someone else replied to “{text}” meanwhile.
          </p>
        ))}
    </>
  );
}

function QuestionRow({
  heapRef,
  item,
  question,
  onReplied,
  onElsewhere,
  onRefused,
}: {
  heapRef: HeapRef;
  item: Item;
  question: Question;
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
    <li data-question={question.number} className="flex flex-col gap-1.5 py-3 first:pt-0 last:pb-0">
      <QuestionHead item={item} question={question} />
      {canReply && (
        <ReplyControls
          describedBy={textId(item, question)}
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
        <div className="flex gap-2">
          {question.choices.map((label, index) => (
            <Button
              key={label}
              aria-describedby={describedBy}
              isDisabled={pending}
              onPress={() => onSend({ picked: [index], comment: note })}
              className={`${index === 0 ? compactButtonClass.primary : compactButtonClass.bordered} ${focusClass} max-sm:flex-1`}
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
 * The questions that don't wait in {@link WaitsForYou}, folded above History: the answered ones, and on a closed item
 * the open ones too, which can be replied to once the item is reopened. Read-only.
 */
export function AnsweredQuestions({ item }: { item: Item }) {
  const closed = isClosed(item.state);
  const listed = (item.questions ?? []).filter((question) => question.state === "answered" || closed);
  if (listed.length === 0) return null;
  return (
    <details className="group">
      <summary className="inline-flex min-h-9 cursor-pointer list-none items-center gap-1 text-[13px] font-medium text-ink-soft select-none hover:text-ink pointer-coarse:min-h-11 [&::-webkit-details-marker]:hidden">
        <ChevronRight
          aria-hidden="true"
          className="size-3.5 transition-transform group-open:rotate-90 motion-reduce:transition-none"
        />
        Questions {listed.length}
      </summary>
      <ul className="mt-1 flex flex-col divide-y divide-line pl-3">
        {listed.map((question) => (
          <li key={question.number} className="flex flex-col gap-1 py-3 first:pt-1 last:pb-1">
            <QuestionHead item={item} question={question} />
            <Outcome question={question} />
          </li>
        ))}
      </ul>
    </details>
  );
}

/** What became of a listed question: the reply and who made it, or that it is still open. */
function Outcome({ question }: { question: Question }) {
  const by = useUserName(question.reply?.replied_by);
  const reply = question.reply;
  if (!reply) return <p className="text-sm text-muted">Open: reopen the item to reply.</p>;
  const labels = reply.picked.map((index) => question.choices[index]).join(", ");
  const who = by ? `${by.charAt(0).toUpperCase()}${by.slice(1)}, ` : "";
  return (
    <>
      <p className="text-sm break-words">
        → {labels}
        {labels && reply.comment ? ". " : ""}
        <span className="whitespace-pre-wrap">{reply.comment}</span>
      </p>
      <p className="text-xs text-muted">
        {who}
        {reply.replied_label}
      </p>
    </>
  );
}
