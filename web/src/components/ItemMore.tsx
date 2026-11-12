import { Ellipsis } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import {
  Button,
  Dialog,
  Heading,
  Menu,
  MenuSection,
  MenuTrigger,
  Modal,
  ModalOverlay,
  ToggleButtonGroup,
  type Key,
  type Selection,
} from "react-aria-components";

import type { HeapRef, Item, ItemPatch, ItemState, Stage } from "../api/client";
import { useBlockerCandidates } from "../api/queries";
import { BlockerSelect } from "./BlockerSelect";
import { blockerCandidatesFor } from "./blockerCandidates";
import { compactButtonClass } from "./buttons";
import { shortLink, useCopyText } from "./copyLink";
import { SendToAgent } from "./ItemTurn";
import { changeLabel, isClosed, NEXT_STATES, STAGE_LABELS } from "./labels";
import { MenuChoice, MenuEntry, MenuHeading, MenuPopover } from "./Menu";
import { PopoverSheet } from "./PopoverSheet";
import { isToCome } from "./reminders";
import { Sheet, SheetChoice, sheetRowClass } from "./Sheet";
import { useToast } from "./UndoToast";
import { useFocusAfterRender } from "./useFocusAfterRender";

/** The main state button is filled with the accent, the next action's colour, with dark text for contrast in both themes. */
const stateButtonClass =
  "h-9 rounded-lg bg-accent px-3 text-sm font-medium text-stone-950 hover:brightness-95 pointer-coarse:h-11";

/** The same, as the phone's bar shows it: 52 px tall and wide. */
const barButtonClass = "h-13 min-w-0 flex-1 rounded-[14px] bg-accent text-[15px] font-semibold text-stone-950";

/** The state the main button moves the item to, or the kind it makes it. */
type Main = { label: string; state: ItemState } | { label: "Make it a task"; state?: undefined };

/**
 * What the item's main button and ⋯ offer (ux.md, The item §6), each only when the server allows it. The main button
 * is the likeliest next state, Make it a task for a reminder that has arrived, or Reopen on a closed item. ⋯ holds the
 * other states, Won't do, Make it a task…, Send to an agent…, Change blocker…, Copy link and the Stage choices.
 */
function actionsOf(item: Item) {
  const allowed = item.allowed_actions;
  const state = item.state;
  const canChange = allowed.includes("change_state");
  const arrivedReminder = item.kind === "reminder" && !isToCome(item) && allowed.includes("make_task");
  let main: Main | null = null;
  let others: ItemState[] = [];
  if (state === "done" || state === "wont_do") {
    if (allowed.includes("reopen")) main = { label: changeLabel(state, "open"), state: "open" };
  } else if (canChange) {
    const [first, ...rest] = NEXT_STATES[state];
    if (arrivedReminder) {
      main = { label: "Make it a task" };
      others = [first, ...rest];
    } else {
      main = { label: changeLabel(state, first), state: first };
      others = rest;
    }
  }
  return {
    main,
    others,
    wontDo: canChange && !isClosed(state),
    makeTask: allowed.includes("make_task") && !arrivedReminder,
    goal: allowed.includes("send_to_agent") ? item.next_goal : null,
    changeBlocker: item.state === "blocked" && canChange,
    stages: item.stage_choices,
  };
}

/**
 * The item's main state button and its ⋯ (More), beside the title on a wide screen and a tablet and in the bar at the
 * bottom of a phone's sheet (ux.md, The item §6). ⋯ is a menu on a wide screen and a tablet, a sheet on a phone; what it
 * opens, the Send to an agent form and Change blocker, opens under it as a popover, or as a sheet on a phone. Picking
 * a stage saves at once and silently, like every change made in the item. Making a note a task asks first, since a
 * task can't go back, and focus comes back to whatever asked, whichever way the question is answered. A state change
 * goes through `onState`, which may ask whether to take the item back from its agent first.
 */
export function ItemActions({
  heapRef,
  item,
  variant,
  save,
  onState,
}: {
  heapRef: HeapRef;
  item: Item;
  variant: "inline" | "bar";
  save: (patch: ItemPatch) => void;
  onState: (state: ItemState) => void;
}) {
  const moreRef = useRef<HTMLButtonElement>(null);
  const mainRef = useRef<HTMLButtonElement>(null);
  const asker = useRef<"main" | "more">("more");
  const focusAfterRender = useFocusAfterRender();
  const setToast = useToast();
  const { copy } = useCopyText();
  const [confirming, setConfirming] = useState(false);
  const [sending, setSending] = useState(false);
  // Set by a send, until the item shows the agent's chip, which then takes the focus. The send form goes away with the
  // send (the item can no longer be sent), so this is kept here.
  const focusChip = useRef(false);
  const hasTurn = item.turn != null;
  useEffect(() => {
    if (!focusChip.current || !hasTurn || sending) return;
    focusChip.current = false;
    focusAfterRender(() => document.querySelector<HTMLElement>("[data-item-pane] [data-agent-chip]"));
  }, [hasTurn, sending, focusAfterRender]);
  const [changing, setChanging] = useState(false);
  const candidates = useBlockerCandidates(heapRef, item.state === "blocked");
  const actions = actionsOf(item);
  const bar = variant === "bar";

  const closeConfirm = () => {
    setConfirming(false);
    focusAfterRender(() => (asker.current === "main" ? mainRef.current : moreRef.current));
  };
  const closeChanging = () => {
    setChanging(false);
    focusAfterRender(() => moreRef.current);
  };
  const changeStage = (keys: Selection) => {
    const [picked] = keys === "all" ? [] : [...keys];
    if (picked !== undefined && picked !== item.stage) save({ stage: String(picked) as Stage });
  };
  // The menu has closed by the time the link is copied, so a toast says it happened.
  const copyLink = () =>
    void copy(shortLink(heapRef.team, heapRef.heap, item.key)).then((copied) =>
      setToast(copied ? { message: "Link copied" } : { message: "Couldn't copy the link.", error: true }),
    );
  const openers = {
    makeTask: () => {
      asker.current = "more";
      setConfirming(true);
    },
    send: () => setSending(true),
    changeBlocker: () => setChanging(true),
  };

  const main = actions.main && (
    <button
      ref={mainRef}
      type="button"
      aria-haspopup={actions.main.state === undefined ? "dialog" : undefined}
      onClick={() => {
        if (actions.main?.state !== undefined) {
          onState(actions.main.state);
        } else {
          asker.current = "main";
          setConfirming(true);
        }
      }}
      className={bar ? barButtonClass : stateButtonClass}
    >
      {actions.main.label}
    </button>
  );

  const more = bar ? (
    <MoreSheet
      item={item}
      actions={actions}
      triggerRef={moreRef}
      onState={onState}
      onWontDo={() => onState("wont_do")}
      openers={openers}
      onCopyLink={copyLink}
      onStage={changeStage}
    />
  ) : (
    <MenuTrigger>
      <Button
        ref={moreRef}
        aria-label="More"
        className="flex size-9 shrink-0 items-center justify-center rounded-lg text-ink-soft outline-none hover:bg-sunken data-[focus-visible]:outline-2 data-[focus-visible]:outline-accent pointer-coarse:size-11"
      >
        <Ellipsis aria-hidden="true" className="size-4" />
      </Button>
      <MenuPopover placement="bottom end">
        <Menu aria-label="More" className="outline-none">
          {actions.others.map((state) => (
            <MenuEntry key={state} id={state} onAction={() => onState(state)}>
              {changeLabel(item.state, state)}
            </MenuEntry>
          ))}
          {actions.wontDo && (
            <MenuEntry id="wont_do" onAction={() => onState("wont_do")}>
              {changeLabel(item.state, "wont_do")}
            </MenuEntry>
          )}
          {actions.makeTask && (
            <MenuEntry id="make_task" onAction={openers.makeTask}>
              Make it a task…
            </MenuEntry>
          )}
          {actions.goal && (
            <MenuEntry id="send" onAction={openers.send}>
              Send to an agent…
            </MenuEntry>
          )}
          {actions.changeBlocker && (
            <MenuEntry id="change_blocker" onAction={openers.changeBlocker}>
              Change blocker…
            </MenuEntry>
          )}
          <MenuEntry id="copy_link" onAction={copyLink}>
            Copy link
          </MenuEntry>
          {actions.stages.length > 0 && (
            <MenuSection
              aria-label="Stage"
              selectionMode="single"
              disallowEmptySelection
              selectedKeys={item.stage ? [item.stage] : []}
              onSelectionChange={changeStage}
            >
              <MenuHeading>Stage</MenuHeading>
              {actions.stages.map((stage) => (
                <MenuChoice key={stage} id={stage}>
                  {STAGE_LABELS[stage]}
                </MenuChoice>
              ))}
              {!actions.stages.includes("defined") && (
                <MenuEntry id="defined_needs_task" isDisabled>
                  Defined needs a task
                </MenuEntry>
              )}
            </MenuSection>
          )}
        </Menu>
      </MenuPopover>
    </MenuTrigger>
  );

  return (
    <>
      <div
        role="group"
        aria-label="State"
        {...(bar && { "data-state-bar": "" })}
        className={
          bar
            ? "sticky bottom-0 z-10 -mx-4 flex gap-2 border-t border-line bg-surface px-4 pt-3 pb-[max(env(safe-area-inset-bottom),1rem)]"
            : "flex shrink-0 items-center gap-1.5"
        }
      >
        {main}
        {more}
      </div>
      <MakeTaskDialog
        item={item}
        isOpen={confirming}
        onCancel={closeConfirm}
        onConfirm={() => {
          save({ kind: "task" });
          closeConfirm();
        }}
      />
      {actions.goal && (
        <SendToAgent
          heapRef={heapRef}
          item={item}
          goal={actions.goal}
          isOpen={sending}
          onClose={(sent) => {
            setSending(false);
            // A send moves focus to the agent chip instead.
            if (sent) focusChip.current = true;
            else focusAfterRender(() => moreRef.current);
          }}
          triggerRef={moreRef}
        />
      )}
      {actions.changeBlocker && (
        <PopoverSheet
          title="Change blocker"
          triggerRef={moreRef}
          isOpen={changing}
          onClose={closeChanging}
          placement="bottom end"
          size="narrow"
        >
          <BlockerSelect
            value={item.blocked_by}
            candidates={blockerCandidatesFor(candidates.data ?? [])(item.id)}
            onChange={(blockerId) => {
              save({ blocked_by_item_id: blockerId });
              closeChanging();
            }}
          />
        </PopoverSheet>
      )}
    </>
  );
}

/**
 * The phone's ⋯: a sheet that opens as the finger lifts. A menu opens as it's pressed and picks the entry the finger
 * lifts over, and on a phone its sheet comes up right under the ⋯ at the bottom of the screen.
 */
function MoreSheet({
  item,
  actions,
  triggerRef,
  onState,
  onWontDo,
  openers,
  onCopyLink,
  onStage,
}: {
  item: Item;
  actions: ReturnType<typeof actionsOf>;
  triggerRef: React.RefObject<HTMLButtonElement | null>;
  onState: (state: ItemState) => void;
  onWontDo: () => void;
  openers: { makeTask: () => void; send: () => void; changeBlocker: () => void };
  onCopyLink: () => void;
  onStage: (keys: Selection) => void;
}) {
  const [open, setOpen] = useState(false);
  const stageId = useId();
  const then = (action: () => void) => () => {
    setOpen(false);
    action();
  };
  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        aria-label="More"
        onClick={() => setOpen(true)}
        className="flex size-13 shrink-0 items-center justify-center rounded-[14px] bg-sunken text-ink-soft outline-none focus-visible:outline-2 focus-visible:outline-accent"
      >
        <Ellipsis aria-hidden="true" className="size-5" />
      </button>
      <Sheet title="More" isOpen={open} onClose={() => setOpen(false)} aside={item.title}>
        {actions.others.map((state) => (
          <button key={state} type="button" onClick={then(() => onState(state))} className={sheetRowClass}>
            {changeLabel(item.state, state)}
          </button>
        ))}
        {actions.wontDo && (
          <button type="button" onClick={then(onWontDo)} className={sheetRowClass}>
            {changeLabel(item.state, "wont_do")}
          </button>
        )}
        {actions.makeTask && (
          <button type="button" onClick={then(openers.makeTask)} className={sheetRowClass}>
            Make it a task…
          </button>
        )}
        {actions.goal && (
          <button type="button" onClick={then(openers.send)} className={sheetRowClass}>
            Send to an agent…
          </button>
        )}
        {actions.changeBlocker && (
          <button type="button" onClick={then(openers.changeBlocker)} className={sheetRowClass}>
            Change blocker…
          </button>
        )}
        <button type="button" onClick={then(onCopyLink)} className={sheetRowClass}>
          Copy link
        </button>
        {actions.stages.length > 0 && (
          <>
            <h3 id={stageId} className="px-3 pt-2 text-xs font-medium text-muted">
              Stage
            </h3>
            <ToggleButtonGroup
              aria-labelledby={stageId}
              selectionMode="single"
              disallowEmptySelection
              selectedKeys={item.stage ? [item.stage] : []}
              onSelectionChange={(keys: Set<Key>) => {
                setOpen(false);
                onStage(keys);
              }}
              orientation="vertical"
              className="flex flex-col"
            >
              {actions.stages.map((stage) => (
                <SheetChoice key={stage} id={stage}>
                  {STAGE_LABELS[stage]}
                </SheetChoice>
              ))}
            </ToggleButtonGroup>
            {!actions.stages.includes("defined") && (
              <p className="px-3 pb-1 text-[13px] text-muted">Defined needs a task</p>
            )}
          </>
        )}
      </Sheet>
    </>
  );
}

/** Asks before a note or a reminder becomes a task, naming the item. A centred dialog at every width. */
function MakeTaskDialog({
  item,
  isOpen,
  onCancel,
  onConfirm,
}: {
  item: Item;
  isOpen: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const bodyId = useId();
  return (
    <ModalOverlay
      isOpen={isOpen}
      onOpenChange={(open) => !open && onCancel()}
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 px-4 pt-[18vh]"
    >
      <Modal className="w-full max-w-[440px] rounded-2xl bg-surface p-[22px] text-ink shadow-xl outline-none">
        <Dialog role="alertdialog" aria-describedby={bodyId} className="flex flex-col gap-3.5 outline-none">
          <Heading slot="title" className="text-base font-semibold break-words">
            Make “{item.title}” a task?
          </Heading>
          <p id={bodyId} className="text-sm text-ink-soft">
            A task can't go back to a {item.kind === "reminder" ? "reminder" : "note"}.
          </p>
          <div className="flex flex-wrap items-center justify-end gap-2">
            <button type="button" autoFocus onClick={onCancel} className={compactButtonClass.quiet}>
              Cancel
            </button>
            <button type="button" onClick={onConfirm} className={compactButtonClass.primary}>
              Make it a task
            </button>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
