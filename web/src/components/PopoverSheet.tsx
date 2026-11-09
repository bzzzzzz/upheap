import { X } from "lucide-react";
import type { ReactNode, RefObject } from "react";
import { Dialog, Heading, Popover } from "react-aria-components";

import { PHONE, useMedia } from "./media";
import { Sheet } from "./Sheet";

/**
 * A panel that opens from a button (ux.md, The list §7): a popover under the button on a tablet or wide screen,
 * and a sheet from the bottom on a phone, above the keyboard. Escape or a tap outside closes it.
 */
export function PopoverSheet({
  title,
  triggerRef,
  isOpen,
  onClose,
  placement = "bottom end",
  action,
  size = "default",
  scrollBody = false,
  children,
}: {
  title: string;
  triggerRef: RefObject<HTMLElement | null>;
  isOpen: boolean;
  onClose: () => void;
  placement?: "bottom end" | "bottom start" | "end top";
  /** Beside the title, before ✕: an action on the whole panel, such as Reset all. */
  action?: ReactNode;
  /** `narrow` is 24 rem wide on a tablet or wide screen, for a list of short rows. */
  size?: "default" | "narrow";
  /** The body scrolls itself, so its own sticky parts stay put; the title bar stays too. The body is a flex column. */
  scrollBody?: boolean;
  children: ReactNode;
}) {
  const phone = useMedia(PHONE);
  if (phone) {
    return (
      <Sheet title={title} isOpen={isOpen} onClose={onClose} aside={action} scrollBody={scrollBody}>
        <div className={`flex flex-col gap-3 pt-1 ${scrollBody ? "min-h-0 flex-1" : ""}`}>{children}</div>
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
      className={`max-h-[70dvh] ${
        size === "narrow" ? "w-[min(24rem,calc(100vw-2rem))]" : "w-[min(34rem,calc(100vw-2rem))]"
      } rounded-xl border border-line bg-surface p-4 text-ink shadow-lg outline-none ${
        scrollBody ? "flex flex-col overflow-hidden" : "overflow-y-auto"
      }`}
    >
      <Dialog aria-label={title} className={`flex flex-col gap-3 outline-none ${scrollBody ? "min-h-0 flex-1" : ""}`}>
        <div className="flex items-center justify-between gap-2">
          <Heading slot="title" className="text-sm font-semibold">
            {title}
          </Heading>
          <span className="ml-auto">{action}</span>
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
