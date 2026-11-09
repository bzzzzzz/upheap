// Token-styled buttons from the first-set mockups (WideAccount), for pages fitted to the first-set palette. Touch
// keeps 44 px targets.
export const buttonClass =
  "inline-flex items-center justify-center gap-2 text-sm font-medium disabled:opacity-40 pointer-coarse:min-h-11";
export const borderedClass = "border border-line-strong bg-surface text-ink hover:bg-sunken";
/** For buttons without a visible border, so they stand as tall as bordered ones beside them. */
export const unborderedClass = "border border-transparent";

/**
 * Buttons 38 px tall, as tall as a 38 px input beside them, or in a dialog: the wide replace confirm, and Save and
 * Cancel on Account, and Sign out there.
 */
const compactClass = `${buttonClass} rounded-[9px] py-2`;
export const compactButtonClass = {
  primary: `${compactClass} ${unborderedClass} bg-ink px-3.5 font-semibold text-surface`,
  quiet: `${compactClass} ${unborderedClass} px-3 text-ink-soft hover:text-ink`,
  bordered: `${compactClass} px-3 ${borderedClass}`,
  danger: `${compactClass} ${unborderedClass} bg-overdue-ink px-3.5 font-semibold text-surface`,
  /** Bordered, in red: an action that ends something but needs no confirm, such as Sign out. */
  borderedDanger: `${compactClass} px-3 border border-line-strong bg-surface text-overdue-ink hover:bg-sunken`,
  /** Unbordered, in red: an action that asks first, such as Archive parameter. */
  quietDanger: `${compactClass} ${unborderedClass} px-3 text-overdue-ink hover:bg-overdue-soft`,
  /** A square button with only an icon, which carries the `aria-label`: 36 px, 44 px to a finger. */
  icon: `${buttonClass} ${unborderedClass} size-9 shrink-0 rounded-[9px] text-ink-soft hover:bg-sunken hover:text-ink pointer-coarse:size-11`,
};
