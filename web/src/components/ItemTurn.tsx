import { Bot } from "lucide-react";
import { useId, useRef, useState, type FormEvent, type RefObject } from "react";
import { Button, Label, Radio, RadioGroup, TextArea, TextField } from "react-aria-components";

import { ApiProblem, type Goal, type HeapRef, type Item, type Reach, type Turn } from "../api/client";
import { useRedoTurn, useSendToAgent, useTakeBack } from "../api/queries";
import { TAKE_BACK_QUESTION } from "./agentConfirm";
import { chipClass, compactButtonClass } from "./buttons";
import { InlineConfirm } from "./InlineConfirm";
import { itemProblem } from "./itemProblems";
import { GOAL_LABELS, STAGE_LABELS } from "./labels";
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
 * Sends an item to an agent and says so (ux.md, The item §15): a toast naming where the turn is going, with Undo, which
 * takes the turn back (recorded in the item's history, with no second toast), or an error toast. The one place that
 * builds that toast, for the item's form and the list row's quick send. `sendWithUndo` resolves with the item the
 * server answered, or `undefined` when the send failed. The form says a failure in itself instead (`toastErrors` off),
 * from the returned `error`.
 */
export function useSendWithUndo(heapRef: HeapRef, toastErrors = true) {
  const send = useSendToAgent(heapRef);
  const takeBack = useTakeBack(heapRef);
  const setToast = useToast();
  const failed = (error: Error) => setToast({ message: itemProblem(error), error: true });
  const sendWithUndo = (item: Item, reach: Reach, line: string | null) =>
    send.mutateAsync({ item, body: { reach, line } }).then(
      (saved) => {
        setToast({
          message: saved.turn ? `Sent to an agent, going to: ${GOAL_LABELS[saved.turn.goal]}.` : "Sent to an agent.",
          undo: () => void takeBack.mutateAsync(saved).then(undefined, failed),
        });
        return saved;
      },
      (error: Error) => {
        if (toastErrors) failed(error);
        return undefined;
      },
    );
  return { sendWithUndo, pending: send.isPending, error: send.error };
}

/**
 * The button in the item's status line before an agent has the item (ux.md, The item §15), which opens the form that
 * sends it: how far, and a line for the agent. A popover under the button on a wide screen and a tablet, a sheet on a
 * phone. The server decides when sending is allowed (`send_to_agent`) and when there is a goal to offer (`next_goal`);
 * the button shows only then. `onSent` runs once a send has closed the form, for the caller that moves focus on to the
 * agent chip, which replaces this button. Cancelling returns focus to the button.
 */
export function SendToAgentButton({ heapRef, item, onSent }: { heapRef: HeapRef; item: Item; onSent: () => void }) {
  const button = useRef<HTMLButtonElement>(null);
  const focusAfterRender = useFocusAfterRender();
  const [open, setOpen] = useState(false);
  const goal = item.allowed_actions.includes("send_to_agent") ? item.next_goal : null;
  if (item.turn || !goal) return null;
  return (
    <>
      <button
        ref={button}
        type="button"
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen(true)}
        className={`${chipClass} gap-1`}
      >
        <Bot aria-hidden="true" className="size-3.5" />
        Send to an agent
      </button>
      <SendToAgent
        heapRef={heapRef}
        item={item}
        goal={goal}
        isOpen={open}
        onClose={(sent) => {
          setOpen(false);
          if (sent) onSent();
          else focusAfterRender(() => button.current);
        }}
        triggerRef={button}
      />
    </>
  );
}

/**
 * The form that sends the item to an agent (ux.md, The item §15). Send acts at once, closes the form and shows the
 * toast with Undo.
 */
function SendToAgent({
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
  const { sendWithUndo, pending, error } = useSendWithUndo(heapRef, false);
  // Someone else's send that arrives while the form is open closes it: the chip shows the agent has the item.
  const formOpen = isOpen && !item.turn;
  // Set by a send made here. Focus moving to the chip closes the popover as if it was left, which mustn't cancel it.
  const sent = useRef(false);

  const sendNow = (reach: Reach, line: string) => {
    sent.current = false;
    return sendWithUndo(item, reach, line.trim() === "" ? null : line).then((saved) => {
      if (!saved) return;
      sent.current = true;
      onClose(true);
    });
  };

  return (
    <PopoverSheet
      title="Send to an agent"
      triggerRef={triggerRef}
      isOpen={formOpen}
      onClose={() => {
        if (!sent.current) onClose(false);
      }}
      placement="bottom start"
      size="narrow"
    >
      <SendForm
        goal={goal}
        pending={pending}
        onSend={(reach, line) => void sendNow(reach, line)}
        onCancel={() => onClose(false)}
      />
      {error && (
        <p role="alert" className="text-sm text-overdue-ink">
          {itemProblem(error)}
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

/**
 * "Changes asked…" in the item's status line when an agent brought the item to a stable state short of done and the
 * server offers `redo_turn` (ux.md, The item §15): opens a form that says what is wrong and sends the item back, to the
 * same goal. A popover on a wide screen and a tablet, a sheet on a phone. `onSent` runs once the send has closed the
 * form, for the caller that moves focus on to the agent chip, which the new turn brings. Cancelling returns focus to
 * the button.
 */
export function ChangesAskedButton({ heapRef, item, onSent }: { heapRef: HeapRef; item: Item; onSent: () => void }) {
  const button = useRef<HTMLButtonElement>(null);
  const focusAfterRender = useFocusAfterRender();
  const redo = useRedoTurn(heapRef);
  const setToast = useToast();
  const [open, setOpen] = useState(false);
  // Set by a send made here. Focus moving to the chip closes the popover as if it was left, which mustn't cancel it.
  const sent = useRef(false);
  if (item.turn || !item.allowed_actions.includes("redo_turn")) return null;

  const close = () => {
    setOpen(false);
    focusAfterRender(() => button.current);
  };
  const send = (line: string) => {
    sent.current = false;
    return redo.mutateAsync({ item, line }).then(
      () => {
        sent.current = true;
        setOpen(false);
        setToast({ message: "Sent back to the agent." });
        onSent();
      },
      (error: Error) => {
        // The item changed meanwhile and has a turn now, so this button and its form unmount: say it in a toast and
        // move focus to what the new turn brings.
        if (error instanceof ApiProblem && error.code === "nothing_to_redo") {
          setOpen(false);
          setToast({ message: itemProblem(error), error: true });
          focusAfterRender(
            () =>
              document.querySelector<HTMLElement>("[data-item-pane] [data-agent-chip]") ??
              document.querySelector<HTMLElement>("[data-item-pane]"),
          );
        }
      },
    );
  };

  return (
    <>
      <button
        ref={button}
        type="button"
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => {
          redo.reset();
          sent.current = false;
          setOpen(true);
        }}
        className={chipClass}
      >
        Changes asked…
      </button>
      <PopoverSheet
        title="Changes asked"
        triggerRef={button}
        isOpen={open}
        onClose={() => {
          if (!sent.current) close();
        }}
        placement="bottom start"
        size="narrow"
      >
        <ChangesAskedForm
          stage={item.stage}
          pending={redo.isPending}
          error={redo.error}
          onSend={(line) => void send(line)}
          onCancel={close}
        />
      </PopoverSheet>
    </>
  );
}

/** The form that sends the work back: what is wrong, required. Its draft lives here and closing the form drops it. */
function ChangesAskedForm({
  stage,
  pending,
  error,
  onSend,
  onCancel,
}: {
  stage: Item["stage"];
  pending: boolean;
  error: Error | null;
  onSend: (line: string) => void;
  onCancel: () => void;
}) {
  const [line, setLine] = useState("");
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (line.trim() !== "" && !pending) onSend(line);
  };
  return (
    <form onSubmit={submit} className="flex flex-col gap-3">
      {stage && <p className="text-sm">An agent brought this to: {STAGE_LABELS[stage]}.</p>}
      <TextField value={line} onChange={setLine} isRequired className="flex flex-col gap-1">
        <Label className={labelClass}>What's wrong</Label>
        <TextArea rows={3} className={fieldClass} />
      </TextField>
      <div className="flex gap-2 max-sm:grid max-sm:grid-cols-2">
        <Button
          type="submit"
          isDisabled={pending || line.trim() === ""}
          className={`${compactButtonClass.primary} ${focusClass}`}
        >
          Send back
        </Button>
        <Button onPress={onCancel} className={`${compactButtonClass.quiet} ${focusClass}`}>
          Cancel
        </Button>
      </div>
      {error && (
        <p role="alert" className="text-sm text-overdue-ink">
          {itemProblem(error)}
        </p>
      )}
    </form>
  );
}
