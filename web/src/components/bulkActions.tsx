import { useRef, useState, type FormEvent, type ReactNode } from "react";
import { Button, Label, RadioGroup, TextArea, TextField } from "react-aria-components";

import type { HeapRef } from "../api/client";
import type { Item, ItemState, Reach } from "../api/schema";
import { useChangeStates } from "../api/queries/items";
import { useSendToAgent } from "../api/queries/turns";
import { ConfirmDialog, takesBack } from "./agentConfirm";
import { compactButtonClass } from "./buttons";
import { fieldClass, focusClass, HowFar, labelClass } from "./ItemTurn";
import { itemProblem } from "./itemProblems";
import { PopoverSheet } from "./PopoverSheet";
import { RequestId } from "./RequestId";
import { useToast } from "./UndoToast";
import { useFocusAfterRender } from "./useFocusAfterRender";

export const plural = (count: number) => `${count} item${count === 1 ? "" : "s"}`;

/** The actions of select mode that go item by item through the single-item endpoints. */
export type BulkAction = "done" | "wont_do" | "start" | "pause" | "send";

type StateAction = Exclude<BulkAction, "send">;

const STATES: Record<StateAction, ItemState> = {
  done: "done",
  wont_do: "wont_do",
  start: "in_progress",
  pause: "open",
};

/** What each action reports, for `count` as "3 items" when all changed or "2 of 4 items" when some did. */
const REPORT: Record<BulkAction, (count: string) => string> = {
  done: (count) => `Marked ${count} done.`,
  wont_do: (count) => `Closed ${count} as won't do.`,
  start: (count) => `Started ${count}.`,
  pause: (count) => `Paused ${count}.`,
  send: (count) => `Sent ${count} to an agent.`,
};

const NOT_DONE: Record<BulkAction, string> = {
  done: "Not marked done:",
  wont_do: "Not closed:",
  start: "Not started:",
  pause: "Not paused:",
  send: "Not sent:",
};

/** The panel under an action's button after some items were refused: what changed, and each item that didn't, with why. */
export type Refused = {
  headline: string;
  heading: string;
  entries: { id: string; title: string; reason: string; error?: Error }[];
};

export function RefusedPanel({ refused, onClose }: { refused: Refused; onClose: () => void }) {
  return (
    <div className="flex flex-col gap-3">
      <h3 className="text-base font-semibold break-words">{refused.headline}</h3>
      <p className="text-sm font-medium">{refused.heading}</p>
      <ul className="flex flex-col gap-1 text-sm text-ink-soft">
        {refused.entries.map((entry) => (
          <li key={entry.id} className="break-words">
            {entry.title} — {entry.reason}
            {entry.error && <RequestId error={entry.error} />}
          </li>
        ))}
      </ul>
      <div className="flex justify-end">
        <button type="button" autoFocus onClick={onClose} className={compactButtonClass.primary}>
          Close
        </button>
      </div>
    </div>
  );
}

/** The form that sends the selection to an agent: how far, and an optional line for the agent. */
function SendToAgentForm({
  pending,
  onSend,
  onCancel,
}: {
  pending: boolean;
  onSend: (reach: Reach, line: string) => void;
  onCancel: () => void;
}) {
  const [reach, setReach] = useState<Reach>("one");
  const [line, setLine] = useState("");
  const submit = (event: FormEvent) => {
    event.preventDefault();
    onSend(reach, line);
  };
  return (
    <form onSubmit={submit} className="flex flex-col gap-3">
      <RadioGroup
        aria-label="How far"
        value={reach}
        onChange={(value) => setReach(value as Reach)}
        className="flex flex-col"
      >
        <HowFar value="one">One step</HowFar>
        <HowFar value="all_the_way">All the way</HowFar>
      </RadioGroup>
      <TextField value={line} onChange={setLine} className="flex flex-col gap-1">
        <Label className={labelClass}>A line for the agent (optional)</Label>
        <TextArea rows={3} className={fieldClass} />
      </TextField>
      <div className="flex gap-2 max-sm:grid max-sm:grid-cols-2">
        <Button type="submit" isDisabled={pending} className={`${compactButtonClass.primary} ${focusClass}`}>
          Send
        </Button>
        <Button onPress={onCancel} isDisabled={pending} className={`${compactButtonClass.quiet} ${focusClass}`}>
          Cancel
        </Button>
      </div>
    </form>
  );
}

/**
 * Done, Won't do, Start / Pause and Send to an agent… on the selection (ux.md, The list §19). Each goes item by item
 * through the single-item endpoint; an item the server refuses is listed with its reason and stays selected. `chosen`
 * are the selected rows in the list's order. `begin` starts an action from the button that was pressed, which gets the
 * focus back when a question or the send form is cancelled; `overlays` holds the questions, the send form and the
 * panel of refused items, and is rendered once.
 */
export function useBulkActions({
  heapRef,
  chosen,
  onSelect,
  onDone,
}: {
  heapRef: HeapRef;
  chosen: Item[];
  onSelect: (ids: Set<string>) => void;
  onDone: () => void;
}): {
  running: BulkAction | null;
  pausing: boolean;
  begin: (action: BulkAction, button: HTMLElement) => void;
  overlays: ReactNode;
} {
  const changeStates = useChangeStates(heapRef);
  const send = useSendToAgent(heapRef);
  const toast = useToast();
  const focusAfterRender = useFocusAfterRender();
  const opener = useRef<HTMLElement | null>(null);
  const [running, setRunning] = useState<BulkAction | null>(null);
  const [asking, setAsking] = useState<"done" | "wont_do" | "start" | null>(null);
  const [sending, setSending] = useState(false);
  const [outcome, setOutcome] = useState<(Refused & { action: BulkAction }) | null>(null);

  const pausing = chosen.length > 0 && chosen.every((item) => item.state === "in_progress");
  const touched = (action: BulkAction) =>
    action === "start" ? chosen.filter((item) => item.state !== "in_progress") : chosen;
  const takenBack = (action: "done" | "wont_do" | "start") =>
    touched(action).filter((item) => takesBack(item, STATES[action])).length;

  const asError = (error: unknown) => (error instanceof Error ? error : new Error(String(error)));

  const runSend = async (items: Item[], reach: Reach, text: string) => {
    setRunning("send");
    const line = text.trim() === "" ? null : text;
    const refused: { item: Item; error: Error }[] = [];
    for (const item of items) {
      try {
        await send.mutateAsync({ item, body: { reach, line } });
      } catch (error) {
        refused.push({ item, error: asError(error) });
      }
    }
    finish("send", items, refused);
  };

  const runState = async (action: StateAction, items: Item[]) => {
    setRunning(action);
    const answer = await changeStates.mutateAsync({ items, state: STATES[action] });
    finish(action, items, answer.refused);
  };

  const finish = (action: BulkAction, items: Item[], refused: { item: Item; error: Error }[]) => {
    setRunning(null);
    if (refused.length === 0) {
      toast({ message: REPORT[action](plural(items.length)) });
      setSending(false);
      if (action === "done" || action === "wont_do") onDone();
      return;
    }
    const changed = items.length - refused.length;
    setOutcome({
      action,
      headline:
        changed === 0
          ? `None of the ${plural(items.length)} changed.`
          : REPORT[action](`${changed} of ${plural(items.length)}`),
      heading: NOT_DONE[action],
      entries: refused.map(({ item, error }) => ({
        id: item.id,
        title: item.title,
        reason: itemProblem(error),
        error,
      })),
    });
  };

  const begin = (action: BulkAction, button: HTMLElement) => {
    opener.current = button;
    if (action === "send") setSending(true);
    else if (action === "done" || action === "wont_do") setAsking(action);
    else if (action === "start" && takenBack(action) > 0) setAsking(action);
    else void runState(action, touched(action));
  };

  const cancel = () => {
    setAsking(null);
    setSending(false);
    focusAfterRender(() => opener.current);
  };
  // Close keeps select mode, with only the items that stayed unchanged selected.
  const closeOutcome = () => {
    if (outcome) onSelect(new Set(outcome.entries.map((entry) => entry.id)));
    setOutcome(null);
    setSending(false);
  };

  let question: { title: string; confirmLabel: string; body: string } | null = null;
  if (asking) {
    const n = touched(asking).length;
    const taken = takenBack(asking);
    const body =
      taken > 0
        ? `An agent has ${taken} of them. This takes them back.`
        : asking === "start"
          ? ""
          : "There is no Undo.";
    question = {
      title: {
        done: `Mark ${plural(n)} done?`,
        wont_do: `Close ${plural(n)} as won't do?`,
        start: `Start ${plural(n)}?`,
      }[asking],
      confirmLabel: { done: `Mark ${n} done`, wont_do: `Close ${n}`, start: `Start ${n}` }[asking],
      body,
    };
  }

  const overlays = (
    <>
      {asking && question && (
        <ConfirmDialog
          {...question}
          onConfirm={() => {
            setAsking(null);
            void runState(asking, touched(asking));
          }}
          onCancel={cancel}
        />
      )}
      <PopoverSheet
        title="Send to an agent"
        triggerRef={opener}
        isOpen={sending}
        onClose={() => {
          if (running) return;
          if (outcome) closeOutcome();
          else cancel();
        }}
        placement="bottom end"
        size="narrow"
      >
        {outcome?.action === "send" ? (
          <RefusedPanel refused={outcome} onClose={closeOutcome} />
        ) : (
          <SendToAgentForm
            pending={running === "send"}
            onSend={(reach, line) => void runSend(chosen, reach, line)}
            onCancel={cancel}
          />
        )}
      </PopoverSheet>
      <PopoverSheet
        title="Not changed"
        triggerRef={opener}
        isOpen={outcome !== null && outcome.action !== "send"}
        onClose={closeOutcome}
        placement="bottom end"
        size="narrow"
      >
        {outcome && <RefusedPanel refused={outcome} onClose={closeOutcome} />}
      </PopoverSheet>
    </>
  );

  return { running, pausing, begin, overlays };
}
