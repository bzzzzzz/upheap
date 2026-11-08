import { X } from "lucide-react";
import type { ReactNode, RefObject } from "react";
import { Dialog, Heading, Popover } from "react-aria-components";

import { PHONE, useMedia } from "./media";
import { Sheet } from "./Sheet";

/**
 * A panel that opens from a button (app-ux.md, The list §7): a popover under the button on a tablet or wide screen,
 * and a sheet from the bottom on a phone, above the keyboard. Escape or a tap outside closes it.
 */
export function PopoverSheet({
  title,
  triggerRef,
  isOpen,
  onClose,
  placement = "bottom end",
  children,
}: {
  title: string;
  triggerRef: RefObject<HTMLElement | null>;
  isOpen: boolean;
  onClose: () => void;
  placement?: "bottom end" | "bottom start" | "end top";
  children: ReactNode;
}) {
  const phone = useMedia(PHONE);
  if (phone) {
    return (
      <Sheet title={title} isOpen={isOpen} onClose={onClose}>
        <div className="flex flex-col gap-3 pt-1">{children}</div>
      </Sheet>
    );
  }
  return (
    <Popover
      triggerRef={triggerRef}
      isOpen={isOpen}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      placement={placement}
      offset={6}
      className="max-h-[70dvh] w-[min(34rem,calc(100vw-2rem))] overflow-y-auto rounded-xl border border-line bg-surface p-4 text-ink shadow-lg outline-none"
    >
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
    </Popover>
  );
}
