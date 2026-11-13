import { useId, useState, type ReactNode } from "react";
import { Dialog, Heading, Modal, ModalOverlay } from "react-aria-components";

import type { Item, ItemState } from "../api/client";
import { compactButtonClass } from "./buttons";
import { isClosed } from "./labels";
import { rowOf, type ChangeState } from "./ItemRow";
import { PHONE, useMedia } from "./media";
import { Sheet } from "./Sheet";
import { useFocusAfterRender } from "./useFocusAfterRender";

/** What Take back asks before it takes an item back from an agent, on the item and on a stuck row. */
export const TAKE_BACK_QUESTION = "Take this back from the agent? It stops, and what it changed stays.";

/**
 * Whether moving `item` to `state` takes it back from the agent that has it (Work with agents, What the person does to
 * a turn): starting it, or closing it. The server does the take back with the change; this tells the client to ask
 * first. Blocking and reopening leave the turn alone.
 */
export const takesBack = (item: Pick<Item, "turn" | "state">, state: ItemState) =>
  item.turn != null && (isClosed(state) || (state === "in_progress" && item.state !== "in_progress"));

/** The verb a change to `state` is named by in the questions: "Starting it" and "Closing it". */
export const gerund = (state: ItemState) => (state === "in_progress" ? "Starting" : "Closing");

/** The word the confirm button ends with, after "Take back and": start, close, or for Done, mark done. */
const doing = (state: ItemState) => (state === "in_progress" ? "start" : state === "done" ? "mark done" : "close");

/** The question the list asks before it moves an item an agent has, naming the item. */
const rowQuestion = (item: Item, state: ItemState) =>
  state === "done"
    ? `An agent has “${item.title}”. Marking it done takes it back from the agent.`
    : `An agent has “${item.title}”. ${gerund(state)} it takes it back from the agent.`;

/**
 * Wraps the list's `ChangeState` so a state change of an item an agent has asks first (ux.md, The list): a dialog on a
 * wide screen and a sheet on a phone, naming the item. Everything that changes a state from the list goes through it,
 * play, the done buttons, the swipes, `x` and `s`. Cancel puts a swiped row back, through the change's `onError`,
 * and gives the focus to the row's link. Render `confirm` once, anywhere.
 */
export function useConfirmAgentStateChange(change: ChangeState): { onChangeState: ChangeState; confirm: ReactNode } {
  const [asking, setAsking] = useState<{ item: Item; state: ItemState; onError: (() => void) | undefined } | null>(
    null,
  );
  const onChangeState: ChangeState = (item, state, onError) => {
    if (takesBack(item, state)) setAsking({ item, state, onError });
    else change(item, state, onError);
  };
  const focusAfterRender = useFocusAfterRender();
  const stop = () => setAsking(null);
  const confirm = asking && (
    <AgentStateConfirm
      question={rowQuestion(asking.item, asking.state)}
      confirmLabel={`Take back and ${doing(asking.state)}`}
      onConfirm={() => {
        stop();
        change(asking.item, asking.state, asking.onError);
      }}
      onCancel={() => {
        stop();
        asking.onError?.();
        focusAfterRender(() => rowOf(asking.item.key));
      }}
    />
  );
  return { onChangeState, confirm };
}

/** Asks whether to take the item back from the agent and carry on: a centred dialog on a wide screen, a sheet on a phone. */
function AgentStateConfirm({
  question,
  confirmLabel,
  onConfirm,
  onCancel,
}: {
  question: string;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const phone = useMedia(PHONE);
  const bodyId = useId();
  if (phone) {
    return (
      <Sheet title="Take back from the agent?" role="alertdialog" describedBy={bodyId} isOpen onClose={onCancel}>
        <div className="flex flex-col gap-2 pb-1">
          <p id={bodyId} className="px-1 py-1 text-[15px] break-words text-ink">
            {question}
          </p>
          <button
            type="button"
            autoFocus
            onClick={onConfirm}
            className="min-h-13 rounded-xl bg-accent text-[15px] font-semibold text-stone-950"
          >
            {confirmLabel}
          </button>
          <button
            type="button"
            onClick={onCancel}
            className="min-h-13 rounded-xl border border-line text-[15px] text-ink"
          >
            Cancel
          </button>
        </div>
      </Sheet>
    );
  }
  return (
    <ModalOverlay
      isOpen
      onOpenChange={(open) => !open && onCancel()}
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 px-4 pt-[18vh]"
    >
      <Modal className="w-full max-w-[440px] rounded-2xl bg-surface p-[22px] text-ink shadow-xl outline-none">
        <Dialog role="alertdialog" aria-describedby={bodyId} className="flex flex-col gap-3.5 outline-none">
          <Heading slot="title" className="text-base font-semibold">
            Take back from the agent?
          </Heading>
          <p id={bodyId} className="text-sm break-words text-ink-soft">
            {question}
          </p>
          <div className="flex flex-wrap items-center justify-end gap-2">
            <button type="button" autoFocus onClick={onCancel} className={compactButtonClass.quiet}>
              Cancel
            </button>
            <button type="button" onClick={onConfirm} className={compactButtonClass.primary}>
              {confirmLabel}
            </button>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
