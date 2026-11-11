import { localDatabase } from "#local-database";
import { useEffect, useId, useRef, useState, type FormEvent } from "react";
import { Button, Label, Radio, RadioGroup, TextArea, TextField } from "react-aria-components";

import type { HeapRef, Item, Reach, Turn } from "../api/client";
import { useHeap, useSendToAgent, useTakeBack } from "../api/queries";
import { compactButtonClass } from "./buttons";
import { InlineConfirm } from "./InlineConfirm";
import { itemProblem } from "./itemProblems";
import { GOAL_LABELS, isClosed, nextGoal } from "./labels";
import { PHONE, useMedia } from "./media";
import { PopoverSheet } from "./PopoverSheet";
import { Sheet } from "./Sheet";
import { useToast } from "./UndoToast";
import { useConfirmation } from "./useConfirmation";

const focusClass = "outline-none data-[focus-visible]:outline-2 data-[focus-visible]:outline-accent";
const pickClass = `group flex min-h-9 items-center gap-2 text-sm ${focusClass} pointer-coarse:min-h-11`;
const pickBoxClass =
  "flex size-5 shrink-0 items-center justify-center rounded-full border border-line-strong bg-surface group-data-[focus-visible]:outline-2 group-data-[focus-visible]:outline-accent group-data-[selected]:border-ink group-data-[selected]:bg-ink";
const labelClass = "text-xs font-medium text-ink-soft";
const fieldClass =
  "field-sizing-content min-h-20 w-full rounded-lg border border-line bg-surface px-2 py-1.5 text-base outline-none focus:border-accent";

const TAKE_BACK_QUESTION = "Take this back from the agent? It stops, and what it changed stays.";

/** When the turn opened: the time today, else the date, in the client's time zone. */
function sinceLabel(openedAt: number, now = new Date()) {
  const at = new Date(openedAt);
  const sameDay = at.toDateString() === now.toDateString();
  return new Intl.DateTimeFormat(
    "en-GB",
    sameDay ? { hour: "2-digit", minute: "2-digit" } : { day: "numeric", month: "short" },
  ).format(at);
}

/**
 * Who has the item (ux.md, The item): the person, or an agent, and the person's way to hand it over or take it back.
 * Only on an open item in an active heap, and not in the local build, which has no agents.
 */
export function ItemTurn({ heapRef, item }: { heapRef: HeapRef; item: Item }) {
  const heap = useHeap(heapRef);
  if (localDatabase || isClosed(item.state) || !heap.data || heap.data.archived_at !== null) return null;
  return <WhoHasIt heapRef={heapRef} item={item} />;
}

function WhoHasIt({ heapRef, item }: { heapRef: HeapRef; item: Item }) {
  const send = useSendToAgent(heapRef);
  const takeBack = useTakeBack(heapRef);
  const setToast = useToast();
  const phone = useMedia(PHONE);
  const headingId = useId();
  const questionId = useId();
  const sendButton = useRef<HTMLButtonElement>(null);
  const { confirming, ask, cancel, dismiss, buttonRef: takeBackButton } = useConfirmation();
  const [form, setForm] = useState(false);
  // Which button takes the focus once the block shows the other state: set by a send or a take back made here.
  const focusNext = useRef<"take-back" | "send" | null>(null);
  const turn = item.turn;
  // Someone else's send that arrives while the form is open closes it: the block shows the agent has the item.
  const formOpen = form && !turn;

  useEffect(() => {
    const next = focusNext.current;
    if (next === null || (next === "take-back") !== (turn != null)) return;
    focusNext.current = null;
    // After the form's own focus restore, which would land on a button that is gone.
    const frame = requestAnimationFrame(() =>
      requestAnimationFrame(() => (next === "take-back" ? takeBackButton : sendButton).current?.focus()),
    );
    return () => cancelAnimationFrame(frame);
  }, [turn, takeBackButton]);

  const failed = (error: Error) => setToast({ message: itemProblem(error), error: true });
  const sendNow = (reach: Reach, line: string) =>
    send.mutateAsync({ item, body: { reach, line: line.trim() === "" ? null : line } }).then(
      (saved) => {
        focusNext.current = "take-back";
        setForm(false);
        setToast({
          message: `Sent to an agent, going to: ${GOAL_LABELS[saved.turn?.goal ?? nextGoal(item)]}.`,
          // Undo is a take back, recorded in the item's history, with no second toast.
          undo: () => void takeBack.mutateAsync(saved).then(undefined, failed),
        });
      },
      () => undefined,
    );
  const takeBackNow = () =>
    takeBack.mutateAsync(item).then(
      () => {
        focusNext.current = "send";
        dismiss();
      },
      () => undefined,
    );

  const canSend = item.allowed_actions.includes("send_to_agent");
  const canTakeBack = item.allowed_actions.includes("take_back");
  const confirmingTakeBack = turn != null && confirming;

  return (
    <section aria-labelledby={headingId} className="flex flex-col gap-2 rounded-lg bg-sunken px-3 py-2.5">
      <h3 id={headingId} className="text-xs font-medium text-ink-soft">
        Who has it
      </h3>
      {turn ? (
        <>
          <TurnText item={item} turn={turn} />
          {canTakeBack &&
            (confirmingTakeBack && !phone ? (
              <InlineConfirm
                question={TAKE_BACK_QUESTION}
                confirmLabel="Take back"
                pending={takeBack.isPending}
                error={takeBack.error}
                onConfirm={() => void takeBackNow()}
                onCancel={cancel}
              />
            ) : (
              <Button
                ref={takeBackButton}
                onPress={() => {
                  takeBack.reset();
                  ask();
                }}
                className={`${compactButtonClass.bordered} self-start ${focusClass}`}
              >
                Take back
              </Button>
            ))}
          {phone && (
            <Sheet
              title="Take back"
              role="alertdialog"
              describedBy={questionId}
              isOpen={confirmingTakeBack}
              onClose={cancel}
              aside={item.title}
            >
              <div className="flex flex-col gap-2 pb-1">
                <p id={questionId} className="px-1 py-1 text-[15px] text-ink">
                  {TAKE_BACK_QUESTION}
                </p>
                <button
                  type="button"
                  autoFocus
                  disabled={takeBack.isPending}
                  onClick={() => void takeBackNow()}
                  className="min-h-13 rounded-xl bg-overdue-ink text-[15px] font-semibold text-surface disabled:opacity-40"
                >
                  Take back
                </button>
                <button
                  type="button"
                  disabled={takeBack.isPending}
                  onClick={cancel}
                  className="min-h-13 rounded-xl border border-line text-[15px] text-ink disabled:opacity-40"
                >
                  Cancel
                </button>
                {takeBack.error && (
                  <p role="alert" className="text-sm text-overdue-ink">
                    {itemProblem(takeBack.error)}
                  </p>
                )}
              </div>
            </Sheet>
          )}
        </>
      ) : (
        <div className="flex flex-wrap items-center justify-between gap-2">
          <p className="text-sm">With you.</p>
          {canSend && (
            <Button
              ref={sendButton}
              onPress={() => {
                send.reset();
                setForm(true);
              }}
              aria-haspopup="dialog"
              aria-expanded={formOpen}
              className={`${compactButtonClass.bordered} ${focusClass}`}
            >
              Send to an agent
            </Button>
          )}
        </div>
      )}
      {!turn && (
        <PopoverSheet
          title="Send to an agent"
          triggerRef={sendButton}
          isOpen={formOpen}
          onClose={() => setForm(false)}
          placement="bottom start"
        >
          <SendForm
            item={item}
            pending={send.isPending}
            onSend={(reach, line) => void sendNow(reach, line)}
            onCancel={() => setForm(false)}
          />
        </PopoverSheet>
      )}
      {send.error && (
        <p role="alert" className="text-sm text-overdue-ink">
          {itemProblem(send.error)}
        </p>
      )}
    </section>
  );
}

/** What an agent having the item means: since when, where it is going, what the person said and how it began. */
function TurnText({ item, turn }: { item: Item; turn: Turn }) {
  const goal = GOAL_LABELS[turn.goal];
  return (
    <>
      <p className="text-sm">
        {item.waits_for_you
          ? `An agent is waiting for your answer, going to: ${goal}.`
          : `With an agent since ${sinceLabel(turn.opened_at)}, going to: ${goal}${turn.reach === "all_the_way" ? ", all the way" : ""}`}
      </p>
      {turn.line && <p className="text-sm break-words whitespace-pre-wrap text-ink-soft">“{turn.line}”</p>}
      {turn.source === "prepare" && <p className="text-xs text-muted">Prepared on add</p>}
    </>
  );
}

/**
 * The form that sends the item: how far, and an optional line for the agent. Its draft lives here, so a refetch of
 * the item leaves it alone, and closing the form throws it away. A defined task has one way to go, to done.
 */
function SendForm({
  item,
  pending,
  onSend,
  onCancel,
}: {
  item: Item;
  pending: boolean;
  onSend: (reach: Reach, line: string) => void;
  onCancel: () => void;
}) {
  const [reach, setReach] = useState<Reach>("one");
  const [line, setLine] = useState("");
  const goal = nextGoal(item);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    onSend(goal === "done" ? "one" : reach, line);
  };
  return (
    <form onSubmit={submit} className="flex flex-col gap-3">
      {goal === "done" ? (
        <p className="text-sm">To {GOAL_LABELS.done}</p>
      ) : (
        <RadioGroup
          aria-label="How far"
          value={reach}
          onChange={(value) => setReach(value as Reach)}
          className="flex flex-col"
        >
          <Radio value="one" className={pickClass}>
            {({ isSelected }) => (
              <>
                <span aria-hidden="true" className={pickBoxClass}>
                  {isSelected && <span className="size-2 rounded-full bg-surface" />}
                </span>
                To {GOAL_LABELS[goal]}
              </>
            )}
          </Radio>
          <Radio value="all_the_way" className={pickClass}>
            {({ isSelected }) => (
              <>
                <span aria-hidden="true" className={pickBoxClass}>
                  {isSelected && <span className="size-2 rounded-full bg-surface" />}
                </span>
                All the way
              </>
            )}
          </Radio>
        </RadioGroup>
      )}
      <TextField value={line} onChange={setLine} className="flex flex-col gap-1">
        <Label className={labelClass}>A line for the agent (optional)</Label>
        <TextArea rows={3} className={fieldClass} />
      </TextField>
      <div className="flex gap-2 max-sm:grid max-sm:grid-cols-2">
        <Button type="submit" isDisabled={pending} className={`${compactButtonClass.primary} ${focusClass}`}>
          Send
        </Button>
        <Button onPress={onCancel} className={`${compactButtonClass.quiet} ${focusClass}`}>
          Cancel
        </Button>
      </div>
    </form>
  );
}
