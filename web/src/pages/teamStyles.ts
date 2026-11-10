// What the Team page and its invite links share: the section title and card, and the small buttons of a row.

export const sectionTitleClass = "min-w-0 flex-1 text-[15px] font-semibold outline-none";
export const cardClass =
  "divide-y divide-line rounded-xl border border-line bg-surface p-1 max-sm:rounded-[14px] max-sm:p-0";
/** The ⋯ that opens a row's or the title's actions: 30 px on a wide screen, 44 px to a finger. */
export const moreButtonClass =
  "flex size-[30px] shrink-0 items-center justify-center rounded-[7px] text-ink-soft outline-none hover:bg-sunken data-[focus-visible]:outline-2 data-[focus-visible]:outline-accent focus-visible:outline-2 focus-visible:outline-accent pointer-coarse:size-11 max-sm:size-11 max-sm:rounded-xl";
/** Make admin, Make member and Copy: bordered and small (WideTeam). */
export const roleButtonClass =
  "shrink-0 rounded-[7px] border border-line bg-surface px-2.5 py-[5px] text-xs text-ink-soft hover:bg-sunken hover:text-ink disabled:opacity-40 pointer-coarse:min-h-11";
export const dangerTextButtonClass =
  "shrink-0 rounded-[7px] px-2.5 py-[5px] text-xs text-overdue-ink hover:bg-overdue-soft disabled:opacity-40 pointer-coarse:min-h-11";
