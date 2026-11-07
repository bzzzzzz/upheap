import { X } from "lucide-react";
import type { ReactNode, RefObject } from "react";
import { Dialog, Heading, Modal, ModalOverlay, Popover } from "react-aria-components";

import { PHONE, useMedia } from "./media";

/**
 * A panel that opens from a button (app-ux.md, The list §7): a popover under the button on a tablet or wide screen,
 * and a sheet from the bottom on a phone, above the keyboard. Escape or a tap outside closes it.
 */
export function PopoverSheet({
  title,
  triggerRef,
  isOpen,
  onClose,
  children,
}: {
  title: string;
  triggerRef: RefObject<HTMLElement | null>;
  isOpen: boolean;
  onClose: () => void;
  children: ReactNode;
}) {
  const phone = useMedia(PHONE);
  const onOpenChange = (open: boolean) => {
    if (!open) onClose();
  };
  const dialog = (
    <Dialog aria-label={title} className="flex flex-col gap-3 outline-none">
      <div className="flex items-center justify-between gap-2">
        <Heading slot="title" className="text-sm font-semibold">
          {title}
        </Heading>
        <button
          type="button"
          onClick={onClose}
          aria-label="Close"
          className="-my-1 flex size-9 items-center justify-center rounded-lg text-ink-soft hover:bg-sunken pointer-coarse:size-11"
        >
          <X aria-hidden="true" className="size-4" />
        </button>
      </div>
      {children}
    </Dialog>
  );

  if (phone) {
    return (
      <ModalOverlay
        isOpen={isOpen}
        isDismissable
        onOpenChange={onOpenChange}
        className="fixed inset-x-0 top-0 bottom-(--keyboard-inset) z-40 flex items-end bg-black/40 pt-[max(env(safe-area-inset-top),1rem)]"
      >
        <Modal className="max-h-full w-full overflow-y-auto rounded-t-2xl bg-surface p-4 pb-[max(env(safe-area-inset-bottom),1rem)] text-ink shadow-xl outline-none">
          {dialog}
        </Modal>
      </ModalOverlay>
    );
  }
  return (
    <Popover
      triggerRef={triggerRef}
      isOpen={isOpen}
      onOpenChange={onOpenChange}
      placement="bottom end"
      offset={6}
      className="max-h-[70dvh] w-[min(34rem,calc(100vw-2rem))] overflow-y-auto rounded-xl border border-line bg-surface p-4 text-ink shadow-lg outline-none"
    >
      {dialog}
    </Popover>
  );
}
