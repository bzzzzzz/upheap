import { useId, useRef, useState, type FormEvent, type RefObject } from "react";
import { Button, Label, Radio, RadioGroup, TextArea, TextField } from "react-aria-components";

import type { Goal, HeapRef, Item, Reach, Turn } from "../api/client";
import { useSendToAgent, useTakeBack } from "../api/queries";
import { TAKE_BACK_QUESTION } from "./agentConfirm";
import { chipClass, compactButtonClass } from "./buttons";
import { InlineConfirm } from "./InlineConfirm";
import { itemProblem } from "./itemProblems";
import { GOAL_LABELS } from "./labels";
import { PHONE, useMedia } from "./media";
import { PopoverSheet } from "./PopoverSheet";
import { Sheet } from "./Sheet";
import { useToast } from "./UndoToast";
import { useConfirmation } from "./useConfirmation";
import { useFocusAfterRender } from "./useFocusAfterRender";

const focusClass = "outline-none data-[focus-visible]:outline-2 data-[focus-visible]:outline-accent";
const pickClass = `group flex min-h-9 items-center gap-2 text-sm ${focusClass} pointer-coarse:min-h-11`;
const pickBoxClass =
  "flex size-5 shrink-0 items-center justify-center rounded-full border border-line-strong bg-surface group-data-[focus-visible]:outline-2 group-data-[focus-visible]:outline-accent group-data-[selected]:border-ink group-data-[selected]:bg-ink";
const labelClass = "text-xs font-medium text-ink-soft";
const fieldClass =
  "field-sizing-content min-h-20 w-full rounded-lg border border-line bg-surface px-2 py-1.5 text-base outline-none focus:border-accent";

/**
 * What the agent chip reads (ux.md, The item §15): where the turn is going, that it waits for the person, or that it is
 * stuck. A turn sent all the way names its reach: its goal is only the first stop, so "to with context" would mislead.
 */
const chipText = (item: Item, turn: Turn) =>
  turn.stuck
    ? "Agent stuck"
    : item.waits_for_you
      ? "Agent · waiting for you"
      : turn.reach === "all_the_way"
        ? "Agent · all the way"
        : `Agent · to ${GOAL_LABELS[turn.goal]}`;

/**
 * Who has the item, when an agent does (ux.md, The item §15): a chip in the status line that opens a popover on a wide
 * screen and a tablet, a sheet on a phone, with since when, where it is going, the person's line and how it began,
 * and Take back, which asks first, inline in the popover or in a sheet on a phone. While the turn is stuck the popover
 * only says so: Retry and Take back are in Waits for you. `onTakenBack` runs once the turn is taken back, which
 * removes the chip, so the caller that stays moves focus on.
 */
export function AgentChip({ heapRef, item, onTakenBack }: { heapRef: HeapRef; item: Item; onTakenBack: () => void }) {
  const takeBack = useTakeBack(heapRef);
  const phone = useMedia(PHONE);
  const questionId = useId();
  const chip = useRef<HTMLButtonElement>(null);
  const focusAfterRender = useFocusAfterRender();
  const [open, setOpen] = useState(false);
  const { confirming, ask, cancel, dismiss, buttonRef: takeBackButton } = useConfirmation();
  const turn = item.turn;
  if (!turn) return null;

  const canTakeBack = item.allowed_actions.includes("take_back") && !turn.stuck;
  const confirmingTakeBack = canTakeBack && confirming;
  const close = () => {
    dismiss();
    setOpen(false);
    focusAfterRender(() => chip.current);
  };
  const takeBackNow = () =>
    takeBack.mutateAsync(item).then(
      () => {
        dismiss();
        setOpen(false);
        onTakenBack();
      },
      () => undefined,
    );

  return (
    <>
      <button
        ref={chip}
        type="button"
        data-agent-chip=""
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={`Who has it: ${chipText(item, turn)}`}
        onClick={() => {
          takeBack.reset();
          setOpen(true);
        }}
        className={`${chipClass} ${turn.stuck ? "border-accent bg-accent/10 font-medium text-accent-ink" : ""}`}
      >
        {chipText(item, turn)}
      </button>
      {/* On a phone the question is a sheet of its own in place of the sheet that asked it. */}
      <PopoverSheet
        title="Who has it"
        triggerRef={chip}
        isOpen={open && !(phone && confirmingTakeBack)}
        onClose={close}
        placement="bottom start"
        size="narrow"
      >
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
      </PopoverSheet>
      {phone && (
        <Sheet
          title="Take back"
          role="alertdialog"
          describedBy={questionId}
          isOpen={open && confirmingTakeBack}
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
  );
}

/**
 * The form that sends the item to an agent, from the item's ⋯ (ux.md, The item §15): a popover under the ⋯ on a wide
 * screen and a tablet, a sheet on a phone. Send acts at once, closes the form, gives focus to the agent chip, and
 * shows a toast with Undo, which takes the turn back. The server decides when there is a goal to offer
 * (`next_goal`) and when sending is allowed; the caller shows the entry only then.
 */
export function SendToAgent({
  heapRef,
  item,
  goal,
  isOpen,
  onClose,
  triggerRef,
}: {
  heapRef: HeapRef;
  item: Item;
  goal: Goal;
  isOpen: boolean;
  /** Closes the form; `sent` is whether a send closed it. */
  onClose: (sent: boolean) => void;
  triggerRef: RefObject<HTMLElement | null>;
}) {
  const send = useSendToAgent(heapRef);
  const takeBack = useTakeBack(heapRef);
  const setToast = useToast();
  const turn = item.turn;
  // Someone else's send that arrives while the form is open closes it: the chip shows the agent has the item.
  const formOpen = isOpen && !turn;
  // Set by a send made here. Focus moving to the chip closes the popover as if it was left, which mustn't take it back.
  const sent = useRef(false);

  const failed = (error: Error) => setToast({ message: itemProblem(error), error: true });
  const sendNow = (reach: Reach, line: string) => {
    sent.current = false;
    return send.mutateAsync({ item, body: { reach, line: line.trim() === "" ? null : line } }).then(
      (saved) => {
        sent.current = true;
        onClose(true);
        setToast({
          message: saved.turn ? `Sent to an agent, going to: ${GOAL_LABELS[saved.turn.goal]}.` : "Sent to an agent.",
          // Undo is a take back, recorded in the item's history, with no second toast.
          undo: () => void takeBack.mutateAsync(saved).then(undefined, failed),
        });
      },
      () => undefined,
    );
  };

  return (
    <PopoverSheet
      title="Send to an agent"
      triggerRef={triggerRef}
      isOpen={formOpen}
      onClose={() => {
        if (!sent.current) onClose(false);
      }}
      placement="bottom end"
      size="narrow"
    >
      <SendForm
        goal={goal}
        pending={send.isPending}
        onSend={(reach, line) => void sendNow(reach, line)}
        onCancel={() => onClose(false)}
      />
      {send.error && (
        <p role="alert" className="text-sm text-overdue-ink">
          {itemProblem(send.error)}
        </p>
      )}
    </PopoverSheet>
  );
}

/** What an agent having the item means: since when, where it is going, what the person said and how it began. */
function TurnText({ item, turn }: { item: Item; turn: Turn }) {
  const goal = GOAL_LABELS[turn.goal];
  return (
    <>
      <p className="text-sm">
        {turn.stuck
          ? `An agent is stuck, going to: ${goal}.`
          : item.waits_for_you
            ? `An agent is waiting for your answer, going to: ${goal}.`
            : `With an agent since ${turn.opened_label}, going to: ${goal}${turn.reach === "all_the_way" ? ", all the way" : ""}`}
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
  goal,
  pending,
  onSend,
  onCancel,
}: {
  goal: Goal;
  pending: boolean;
  onSend: (reach: Reach, line: string) => void;
  onCancel: () => void;
}) {
  const [reach, setReach] = useState<Reach>("one");
  const [line, setLine] = useState("");
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
