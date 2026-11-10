import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";
import { Check } from "lucide-react";
import { Header, MenuItem, Popover } from "react-aria-components";

/**
 * A menu's popover. On a phone it's a sheet from the bottom, over a dimmed page (the spread shadow), whatever its
 * anchor.
 */
export function MenuPopover({
  placement,
  children,
}: {
  placement: "end top" | "bottom start" | "bottom end" | "top end";
  children: ReactNode;
}) {
  return (
    <Popover
      placement={placement}
      offset={6}
      className={`max-h-[70dvh] min-w-52 overflow-y-auto rounded-xl border border-line bg-surface p-1 text-ink shadow-lg outline-none max-sm:fixed! max-sm:inset-x-0! max-sm:top-auto! max-sm:max-h-[75dvh]! max-sm:rounded-b-none max-sm:border-x-0 max-sm:border-b-0 max-sm:p-2 max-sm:shadow-[0_0_0_100vmax_rgb(0_0_0/0.4)] max-sm:bottom-0! max-sm:pb-[max(env(safe-area-inset-bottom),0.5rem)]`}
    >
      {children}
    </Popover>
  );
}

export function MenuEntry({
  id,
  icon: Icon,
  textValue,
  onAction,
  isDisabled,
  children,
}: {
  id: string;
  icon?: LucideIcon | undefined;
  textValue?: string;
  onAction?: () => void;
  /** A line that only says something, such as why a choice is missing. */
  isDisabled?: boolean;
  children: ReactNode;
}) {
  return (
    <MenuItem
      id={id}
      textValue={textValue ?? (typeof children === "string" ? children : undefined)}
      onAction={onAction}
      isDisabled={isDisabled}
      className="flex min-h-11 cursor-default items-center gap-3 rounded-lg px-3 text-[15px] outline-none data-[disabled]:text-muted data-[focused]:bg-sunken"
    >
      <span className="flex size-4 shrink-0 items-center justify-center text-ink-soft">
        {Icon && <Icon aria-hidden="true" className="size-4" />}
      </span>
      {children}
    </MenuItem>
  );
}

/** One choice of a section with single selection, the current one marked with a check. */
export function MenuChoice({ id, children }: { id: string; children: string }) {
  return (
    <MenuItem
      id={id}
      textValue={children}
      className="flex min-h-11 cursor-default items-center gap-3 rounded-lg px-3 text-[15px] outline-none data-[focused]:bg-sunken"
    >
      {({ isSelected }) => (
        <>
          <span aria-hidden="true" className="flex size-4 shrink-0 items-center justify-center text-accent-ink">
            {isSelected && <Check className="size-4" strokeWidth={3} />}
          </span>
          {children}
        </>
      )}
    </MenuItem>
  );
}

/** The heading of a menu section. */
export function MenuHeading({ children }: { children: ReactNode }) {
  return <Header className="px-3 pt-2 pb-1 text-xs font-medium text-muted">{children}</Header>;
}
