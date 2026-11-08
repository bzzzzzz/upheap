import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";
import { MenuItem, Popover } from "react-aria-components";

/**
 * A menu's popover. On a phone it's a sheet from the bottom, over a dimmed page (the spread shadow), whatever its
 * anchor. `aboveTabs` puts the sheet above the phone's tabs, for More: a menu opens on mouse down and picks the item
 * the button is released over, so a sheet over its own button would pick its bottom item (Sign out) on a click.
 */
export function MenuPopover({
  placement,
  aboveTabs = false,
  children,
}: {
  placement: "end top" | "bottom start" | "bottom end" | "top end";
  aboveTabs?: boolean;
  children: ReactNode;
}) {
  return (
    <Popover
      placement={placement}
      offset={6}
      className={`max-h-[70dvh] min-w-52 overflow-y-auto rounded-xl border border-line bg-surface p-1 text-ink shadow-lg outline-none max-sm:fixed! max-sm:inset-x-0! max-sm:top-auto! max-sm:max-h-[75dvh]! max-sm:rounded-b-none max-sm:border-x-0 max-sm:border-b-0 max-sm:p-2 max-sm:shadow-[0_0_0_100vmax_rgb(0_0_0/0.4)] ${
        aboveTabs
          ? "max-sm:bottom-(--phone-tabs-height)!"
          : "max-sm:bottom-0! max-sm:pb-[max(env(safe-area-inset-bottom),0.5rem)]"
      }`}
    >
      {children}
    </Popover>
  );
}

export function MenuEntry({
  id,
  icon: Icon,
  textValue,
  children,
}: {
  id: string;
  icon?: LucideIcon | undefined;
  textValue?: string;
  children: ReactNode;
}) {
  return (
    <MenuItem
      id={id}
      textValue={textValue ?? (typeof children === "string" ? children : undefined)}
      className="flex min-h-11 cursor-default items-center gap-3 rounded-lg px-3 text-[15px] outline-none data-[focused]:bg-sunken"
    >
      <span className="flex size-4 shrink-0 items-center justify-center text-ink-soft">
        {Icon && <Icon aria-hidden="true" className="size-4" />}
      </span>
      {children}
    </MenuItem>
  );
}
