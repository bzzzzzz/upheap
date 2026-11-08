// Token-styled buttons from the first-set mockups (WideAccount), for pages fitted to the first-set palette. Touch
// keeps 44 px targets.
export const buttonClass =
  "inline-flex items-center justify-center gap-2 text-sm font-medium disabled:opacity-40 pointer-coarse:min-h-11";
export const borderedClass = "border border-line-strong bg-surface text-ink hover:bg-sunken";

/** Buttons that sit beside a 36 px input or in a dialog: the wide replace confirm, and Save and Cancel on Account. */
const compactClass = `${buttonClass} rounded-[9px] py-2`;
export const compactButtonClass = {
  primary: `${compactClass} bg-ink px-3.5 font-semibold text-surface`,
  quiet: `${compactClass} px-3 text-ink-soft hover:text-ink`,
  bordered: `${compactClass} px-3 ${borderedClass}`,
  danger: `${compactClass} bg-overdue-ink px-3.5 font-semibold text-surface`,
};
