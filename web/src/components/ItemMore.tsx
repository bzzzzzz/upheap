import { Ellipsis } from "lucide-react";
import { useId, useRef, useState } from "react";
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

import type { Item, ItemPatch, Stage } from "../api/client";
import { compactButtonClass } from "./buttons";
import { MenuChoice, MenuEntry, MenuHeading, MenuPopover } from "./Menu";
import { changeLabel, STAGE_LABELS } from "./labels";
import { Sheet, SheetChoice, sheetRowClass } from "./Sheet";
import { useFocusAfterRender } from "./useFocusAfterRender";

/**
 * What the ⋯ beside the item's state buttons holds, each only when the server allows it (ux.md, The item §6): Won't
 * do, Make it a task… and the Stage choices. `null` when none applies, so the ⋯ is absent rather than disabled.
 */
function moreEntries(item: Item) {
  const wontDo = item.allowed_actions.includes("change_state") && item.state !== "done" && item.state !== "wont_do";
  const makeTask = item.allowed_actions.includes("make_task");
  const stages = item.stage_choices;
  return wontDo || makeTask || stages.length > 0 ? { wontDo, makeTask, stages } : null;
}

/**
 * The item's "More" menu, as a menu on a wide screen and a tablet and as a sheet from the phone's bar. Picking a stage
 * saves at once and silently, like every change made in the item. Making a note a task asks first, since a task can't
 * go back, and focus comes back to the ⋯ whichever way the question is answered.
 */
export function ItemMore({
  item,
  variant,
  save,
}: {
  item: Item;
  variant: "inline" | "bar";
  save: (patch: ItemPatch) => void;
}) {
  const triggerRef = useRef<HTMLButtonElement>(null);
  const [confirming, setConfirming] = useState(false);
  const focusAfterRender = useFocusAfterRender();
  const entries = moreEntries(item);
  if (!entries) return null;

  const closeConfirm = () => {
    setConfirming(false);
    focusAfterRender(() => triggerRef.current);
  };
  const changeStage = (keys: Selection) => {
    const [picked] = keys === "all" ? [] : [...keys];
    if (picked !== undefined && picked !== item.stage) save({ stage: String(picked) as Stage });
  };

  return (
    <>
      {variant === "bar" ? (
        <MoreSheet
          item={item}
          entries={entries}
          triggerRef={triggerRef}
          onWontDo={() => save({ state: "wont_do" })}
          onMakeTask={() => setConfirming(true)}
          onStage={changeStage}
        />
      ) : (
        <MenuTrigger>
          <Button
            ref={triggerRef}
            aria-label="More"
            className="flex size-9 items-center justify-center rounded-lg text-ink-soft outline-none hover:bg-sunken data-[focus-visible]:outline-2 data-[focus-visible]:outline-accent pointer-coarse:size-11"
          >
            <Ellipsis aria-hidden="true" className="size-4" />
          </Button>
          <MenuPopover placement="bottom end">
            <Menu aria-label="More" className="outline-none">
              {entries.wontDo && (
                <MenuEntry id="wont_do" onAction={() => save({ state: "wont_do" })}>
                  {changeLabel(item.state, "wont_do")}
                </MenuEntry>
              )}
              {entries.makeTask && (
                <MenuEntry id="make_task" onAction={() => setConfirming(true)}>
                  Make it a task…
                </MenuEntry>
              )}
              {entries.stages.length > 0 && (
                <MenuSection
                  aria-label="Stage"
                  selectionMode="single"
                  disallowEmptySelection
                  selectedKeys={item.stage ? [item.stage] : []}
                  onSelectionChange={changeStage}
                >
                  <MenuHeading>Stage</MenuHeading>
                  {entries.stages.map((stage) => (
                    <MenuChoice key={stage} id={stage}>
                      {STAGE_LABELS[stage]}
                    </MenuChoice>
                  ))}
                  {!entries.stages.includes("defined") && (
                    <MenuEntry id="defined_needs_task" isDisabled>
                      Defined needs a task
                    </MenuEntry>
                  )}
                </MenuSection>
              )}
            </Menu>
          </MenuPopover>
        </MenuTrigger>
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
    </>
  );
}

/**
 * Make it a task as a button of its own, for a reminder that has arrived: turning it into a task is what is likely
 * next (ux.md, The item §6). Asks first, like the entry in More. Only where the server allows it.
 */
export function MakeTaskButton({
  item,
  save,
  className,
}: {
  item: Item;
  save: (patch: ItemPatch) => void;
  className: string;
}) {
  const [confirming, setConfirming] = useState(false);
  if (!item.allowed_actions.includes("make_task")) return null;
  return (
    <>
      <button type="button" aria-haspopup="dialog" onClick={() => setConfirming(true)} className={className}>
        Make it a task
      </button>
      <MakeTaskDialog
        item={item}
        isOpen={confirming}
        onCancel={() => setConfirming(false)}
        onConfirm={() => {
          save({ kind: "task" });
          setConfirming(false);
        }}
      />
    </>
  );
}

/**
 * The phone's version: a sheet that opens as the finger lifts. A menu opens as it's pressed and picks the entry the
 * finger lifts over, and on a phone its sheet comes up right under the ⋯ at the bottom of the screen.
 */
function MoreSheet({
  item,
  entries,
  triggerRef,
  onWontDo,
  onMakeTask,
  onStage,
}: {
  item: Item;
  entries: NonNullable<ReturnType<typeof moreEntries>>;
  triggerRef: React.RefObject<HTMLButtonElement | null>;
  onWontDo: () => void;
  onMakeTask: () => void;
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
        className="flex size-13 items-center justify-center rounded-[14px] bg-sunken text-ink-soft outline-none focus-visible:outline-2 focus-visible:outline-accent"
      >
        <Ellipsis aria-hidden="true" className="size-5" />
      </button>
      <Sheet title="More" isOpen={open} onClose={() => setOpen(false)} aside={item.title}>
        {entries.wontDo && (
          <button type="button" onClick={then(onWontDo)} className={sheetRowClass}>
            {changeLabel(item.state, "wont_do")}
          </button>
        )}
        {entries.makeTask && (
          <button type="button" onClick={then(onMakeTask)} className={sheetRowClass}>
            Make it a task…
          </button>
        )}
        {entries.stages.length > 0 && (
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
              {entries.stages.map((stage) => (
                <SheetChoice key={stage} id={stage}>
                  {STAGE_LABELS[stage]}
                </SheetChoice>
              ))}
            </ToggleButtonGroup>
            {!entries.stages.includes("defined") && (
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
