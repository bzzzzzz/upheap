import { useNavigate } from "@tanstack/react-router";
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

import type { HeapRef } from "../api/client";
import type { Item, ItemPatchRequest, ItemState, Stage } from "../api/schema";
import { useUpdateItem } from "../api/queries/items";
import { useBlockerCandidates } from "../api/queries/lists";
import { useMe } from "../api/queries/me";
import { useMoveItem } from "../api/queries/moves";
import { BlockerSelect } from "./BlockerSelect";
import { BlockReasonEditor } from "./BlockReasonEditor";
import { blockerCandidatesFor } from "./blockerCandidates";
import { compactButtonClass } from "./buttons";
import { changeLabel, isClosed, NEXT_STATES, STAGE_LABELS } from "./labels";
import { MenuChoice, MenuEntry, MenuHeading, MenuPopover } from "./Menu";
import { MoveToHeap } from "./MoveToHeap";
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
 * other states, Won't do, Make it a task…, Split… on an open note, Change blocker…, Add reason… or Change reason…, the
 * Stage choices, and Move to another heap… when the team has another heap to move it to (`canMove`); with none of
 * them, as for a reader, there is no ⋯.
 */
function actionsOf(item: Item, canMove: boolean) {
  const allowed = item.allowed_actions;
  const move = allowed.includes("move") && canMove;
  const state = item.state;
  const canChange = allowed.includes("change_state");
  const arrivedReminder = item.kind === "reminder" && !isToCome(item) && allowed.includes("make_task");
  let main: Main | null = null;
  let others: ItemState[] = [];
  if (isClosed(state)) {
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
    split: allowed.includes("became"),
    changeBlocker: item.state === "blocked" && canChange,
    reasonLabel: item.blocked_reason ? "Change reason…" : "Add reason…",
    stages: item.stage_choices,
    move,
    more:
      others.length > 0 ||
      (canChange && !isClosed(state)) ||
      (allowed.includes("make_task") && !arrivedReminder) ||
      allowed.includes("became") ||
      (item.state === "blocked" && canChange) ||
      item.stage_choices.length > 0 ||
      move,
  };
}

/**
 * The item's main state button and its ⋯ (More), beside the title on a wide screen and a tablet and in the bar at the
 * bottom of a phone's sheet (ux.md, The item §6). ⋯ is a menu on a wide screen and a tablet, a sheet on a phone; what it
 * opens, Change blocker and the block reason's editor, open under it as a popover, or as a sheet on a phone. Picking
 * a stage saves at once and silently, like every change made in the item. Making a note a task asks first, since a
 * task can't go back, and focus comes back to whatever asked, whichever way the question is answered. Split… asks the
 * pane, through `onSplit`, to open the dialog that splits a note: the pane owns it, so a draft survives the bar and
 * the inline actions swapping at the phone width, and it gets the two buttons' refs to give focus back. Move to
 * another heap… opens a picker under ⋯ that confirms before it moves, and lands on the item in its new heap. A state
 * change goes through `onState`, which may ask whether to take the item back from its agent first.
 */
export function ItemActions({
  heapRef,
  item,
  variant,
  save,
  onState,
  onSplit,
  moreRef,
  mainRef,
}: {
  heapRef: HeapRef;
  item: Item;
  variant: "inline" | "bar";
  save: (patch: ItemPatchRequest) => void;
  onState: (state: ItemState) => void;
  onSplit: () => void;
  moreRef: React.RefObject<HTMLButtonElement | null>;
  mainRef: React.RefObject<HTMLButtonElement | null>;
}) {
  const asker = useRef<"main" | "more">("more");
  const focusAfterRender = useFocusAfterRender();
  const [confirming, setConfirming] = useState(false);
  const [changing, setChanging] = useState(false);
  const [moving, setMoving] = useState(false);
  const [editingReason, setEditingReason] = useState(false);
  // Awaitable, and apart from `save`, so the editor says its own failure and the pane doesn't say it twice.
  const saveReason = useUpdateItem(heapRef);
  const candidates = useBlockerCandidates(heapRef, item.state === "blocked");
  const move = useMoveItem(heapRef);
  const navigate = useNavigate();
  const toast = useToast();
  const { data: me } = useMe();
  const own = me?.teams.find((t) => t.slug === heapRef.team);
  // A note heaps were made from moves only within its team.
  const noteTeamOnly = (item.became_heaps?.length ?? 0) > 0;
  const moveTeams = !me || !own ? [] : noteTeamOnly ? [own] : [own, ...me.teams.filter((t) => t !== own)];
  const opener = item.turn?.opened_by;
  const actions = actionsOf(
    item,
    moveTeams.some((t) =>
      t.heaps.some((heap) => !heap.archived && !(t.slug === heapRef.team && heap.slug === heapRef.heap)),
    ),
  );
  const bar = variant === "bar";
  // Done or Won't do picked from ⋯ closes the item, and a closed item has no ⋯: focus, given back to ⋯ as its menu
  // closed, goes to the main button (Reopen) instead of falling to the page.
  const hadMore = useRef(actions.more);
  useEffect(() => {
    if (hadMore.current && !actions.more && document.activeElement === document.body) mainRef.current?.focus();
    hadMore.current = actions.more;
  }, [actions.more, mainRef]);

  const closeConfirm = () => {
    setConfirming(false);
    focusAfterRender(() => (asker.current === "main" ? mainRef.current : moreRef.current));
  };
  const closeChanging = () => {
    setChanging(false);
    focusAfterRender(() => moreRef.current);
  };
  const closeMoving = () => {
    setMoving(false);
    focusAfterRender(() => moreRef.current);
  };
  const moveTo = async (to: HeapRef & { id: string }, label: string) => {
    try {
      const saved = await move.mutateAsync({ item, to });
      await navigate({
        to: "/teams/$team/heaps/$heap/items/$item",
        params: { team: to.team, heap: to.heap, item: saved.key },
        replace: true,
      });
      // This component belongs to the old heap, whose toast the provider puts away: the toast is the new heap's.
      toast(
        { message: `Moved to ${label}.${saved.key !== item.key ? ` Its key is now ${saved.key}.` : ""}` },
        { heap: to },
      );
    } catch {
      // The mutation holds the error, and the picker shows it.
    }
  };
  const closeEditingReason = () => {
    setEditingReason(false);
    focusAfterRender(() => moreRef.current);
  };
  const changeStage = (keys: Selection) => {
    const [picked] = keys === "all" ? [] : [...keys];
    if (picked !== undefined && picked !== item.stage) save({ stage: String(picked) as Stage });
  };
  const openers = {
    makeTask: () => {
      asker.current = "more";
      setConfirming(true);
    },
    split: onSplit,
    changeBlocker: () => setChanging(true),
    editReason: () => {
      saveReason.reset();
      setEditingReason(true);
    },
    move: () => {
      move.reset();
      setMoving(true);
    },
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

  const more = !actions.more ? null : bar ? (
    <MoreSheet
      item={item}
      actions={actions}
      triggerRef={moreRef}
      onState={onState}
      onWontDo={() => onState("wont_do")}
      openers={openers}
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
          {actions.split && (
            <MenuEntry id="split" onAction={openers.split}>
              Split…
            </MenuEntry>
          )}
          {actions.changeBlocker && (
            <MenuEntry id="change_blocker" onAction={openers.changeBlocker}>
              Change blocker…
            </MenuEntry>
          )}
          {actions.changeBlocker && (
            <MenuEntry id="edit_reason" onAction={openers.editReason}>
              {actions.reasonLabel}
            </MenuEntry>
          )}
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
          {actions.move && (
            <MenuSection aria-label="Move">
              <MenuEntry id="move" onAction={openers.move}>
                Move to another heap…
              </MenuEntry>
            </MenuSection>
          )}
        </Menu>
      </MenuPopover>
    </MenuTrigger>
  );

  return (
    <>
      {(main || more) && (
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
      )}
      <MakeTaskDialog
        item={item}
        isOpen={confirming}
        onCancel={closeConfirm}
        onConfirm={() => {
          save({ kind: "task" });
          closeConfirm();
        }}
      />
      {actions.changeBlocker && (
        <PopoverSheet
          title="Change blocker"
          triggerRef={moreRef}
          isOpen={changing}
          onClose={closeChanging}
          placement="bottom end"
          size="narrow"
          scrollBody
        >
          <BlockerSelect
            value={item.blocked_by}
            candidates={blockerCandidatesFor(candidates.data ?? [])(item.id)}
            pending={candidates.isPending}
            error={candidates.error}
            onChange={(blockerId) => {
              save({ blocked_by_item_id: blockerId });
              closeChanging();
            }}
          />
        </PopoverSheet>
      )}
      {actions.move && me && own && (
        <PopoverSheet
          title="Move to"
          triggerRef={moreRef}
          isOpen={moving}
          onClose={() => !move.isPending && closeMoving()}
          placement="bottom end"
          size="narrow"
        >
          <MoveToHeap
            me={me}
            teams={moveTeams}
            heapRef={heapRef}
            pending={move.isPending}
            error={move.error}
            teamOnly={noteTeamOnly}
            turnBy={
              !item.turn ? undefined : !opener ? null : opener.id !== me.user.id ? opener.display_name : undefined
            }
            onMove={(to, label) => void moveTo(to, label)}
            onReset={move.reset}
          />
        </PopoverSheet>
      )}
      {/* Not tied to the item being blocked: a failed save refetches it, and the editor stays to say why. */}
      <PopoverSheet
        title="Why it's blocked"
        triggerRef={moreRef}
        isOpen={editingReason}
        onClose={() => !saveReason.isPending && closeEditingReason()}
        placement="bottom end"
        size="narrow"
      >
        <BlockReasonEditor
          reason={item.blocked_reason ?? null}
          save={(blocked_reason) => saveReason.mutateAsync({ item, patch: { blocked_reason } })}
          saving={saveReason.isPending}
          error={saveReason.error}
          onDone={closeEditingReason}
        />
      </PopoverSheet>
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
  onStage,
}: {
  item: Item;
  actions: ReturnType<typeof actionsOf>;
  triggerRef: React.RefObject<HTMLButtonElement | null>;
  onState: (state: ItemState) => void;
  onWontDo: () => void;
  openers: {
    makeTask: () => void;
    split: () => void;
    changeBlocker: () => void;
    editReason: () => void;
    move: () => void;
  };
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
        {actions.split && (
          <button type="button" onClick={then(openers.split)} className={sheetRowClass}>
            Split…
          </button>
        )}
        {actions.changeBlocker && (
          <button type="button" onClick={then(openers.changeBlocker)} className={sheetRowClass}>
            Change blocker…
          </button>
        )}
        {actions.changeBlocker && (
          <button type="button" onClick={then(openers.editReason)} className={sheetRowClass}>
            {actions.reasonLabel}
          </button>
        )}
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
        {actions.move && (
          <button type="button" onClick={then(openers.move)} className={sheetRowClass}>
            Move to another heap…
          </button>
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
