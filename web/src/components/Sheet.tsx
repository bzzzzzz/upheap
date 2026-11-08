import { Check, X } from "lucide-react";
import type { ReactNode } from "react";
import { Dialog, Heading, Modal, ModalOverlay, ToggleButton, type Key } from "react-aria-components";

/**
 * A sheet from the bottom of a phone's screen, over a dimmed page (ux-overview.md: overlays on the phone are bottom
 * sheets). It sits above the on-screen keyboard, or with `aboveTabs` above the phone's tabs as a card, for More.
 * Escape, ✕ or a tap on the dimmed page closes it. Without `showTitle` the title only names the dialog. `role` is
 * `alertdialog` for a confirm that needs an answer.
 */
export function Sheet({
  title,
  isOpen,
  onClose,
  aboveTabs = false,
  showTitle = true,
  role = "dialog",
  describedBy,
  aside,
  children,
}: {
  title: string;
  isOpen: boolean;
  onClose: () => void;
  aboveTabs?: boolean;
  showTitle?: boolean;
  role?: "dialog" | "alertdialog";
  /** The id of what describes the dialog, such as a confirm's question. */
  describedBy?: string;
  /** Beside the title: what the sheet is for, such as the item's title. */
  aside?: ReactNode;
  children: ReactNode;
}) {
  return (
    <ModalOverlay
      isOpen={isOpen}
      isDismissable
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      className={`fixed inset-x-0 top-0 z-50 flex items-end bg-black/40 pt-[max(env(safe-area-inset-top),1rem)] ${
        aboveTabs ? "bottom-0 pb-[calc(var(--phone-tabs-height)+0.5rem)]" : "bottom-(--keyboard-inset)"
      }`}
    >
      <Modal
        className={`max-h-full w-full overflow-y-auto bg-surface text-ink shadow-xl outline-none ${
          aboveTabs ? "mx-2 rounded-2xl p-2" : "rounded-t-2xl px-4 pt-2.5 pb-[max(env(safe-area-inset-bottom),1rem)]"
        }`}
      >
        <Dialog
          aria-label={title}
          aria-describedby={describedBy}
          role={role}
          className="flex flex-col gap-1 outline-none"
        >
          {!aboveTabs && <span aria-hidden="true" className="mx-auto h-1 w-10 rounded-full bg-line" />}
          {showTitle && (
            <div className="flex items-center gap-2 pt-1 pb-1">
              <Heading slot="title" className="min-w-0 text-[17px] font-semibold break-words">
                {title}
              </Heading>
              <span className="min-w-0 flex-1 truncate text-right text-[13px] text-muted">{aside}</span>
              <button
                type="button"
                onClick={onClose}
                aria-label="Close"
                className="-mr-2 flex size-11 shrink-0 items-center justify-center rounded-lg text-ink-soft hover:bg-sunken"
              >
                <X aria-hidden="true" className="size-4" />
              </button>
            </div>
          )}
          {children}
          {!showTitle && (
            // Without the title bar there's no ✕, and a screen reader can't tap outside.
            <button type="button" onClick={onClose} className="sr-only">
              Close
            </button>
          )}
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}

/**
 * A row of a list in a sheet (ux-overview.md, What comes first §3): 52 px tall, with what it is on the left, a note or
 * value on the right, and a check on the current choice.
 */
export const sheetRowClass =
  "flex min-h-13 w-full items-center gap-3 rounded-xl px-3 text-left text-[15px] text-ink outline-none hover:bg-sunken focus-visible:outline-2 focus-visible:outline-accent data-[focus-visible]:outline-2 data-[focus-visible]:outline-accent aria-[current=page]:bg-accent-soft aria-[current=page]:font-semibold data-[selected]:bg-accent-soft data-[selected]:font-semibold disabled:opacity-50";

/**
 * One choice among the rows of a sheet, inside a single-selection `ToggleButtonGroup`, which makes them a radio group
 * the arrows move through. `note` sits before the check and describes the choice without naming it.
 */
export function SheetChoice({
  id,
  muted = false,
  note,
  children,
}: {
  id: Key;
  muted?: boolean;
  note?: { id: string; text: string };
  children: ReactNode;
}) {
  return (
    <ToggleButton id={id} aria-describedby={note?.id} className={sheetRowClass}>
      {({ isSelected }) => (
        <>
          <span className={`flex-1 ${muted && !isSelected ? "text-ink-soft" : ""}`}>{children}</span>
          {note && (
            <span id={note.id} aria-hidden="true" className="text-[13px] font-normal text-muted">
              {note.text}
            </span>
          )}
          <span aria-hidden="true" className="w-5 text-accent-ink">
            {isSelected && <Check className="size-4" strokeWidth={3} />}
          </span>
        </>
      )}
    </ToggleButton>
  );
}
