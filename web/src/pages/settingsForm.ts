// The look the heap's settings tabs share (HeapSettingsPage, TagsPage, ParametersPage), in the design tokens.
import { compactButtonClass } from "../components/buttons";
import type { NameFormClasses } from "../components/NameForm";

/** A text input 38 px tall, as tall as a compact button beside it; 44 px to a finger. */
export const inputClass =
  "min-w-0 rounded-lg border border-line bg-surface px-2.5 py-2 text-sm text-ink placeholder:text-muted disabled:opacity-60 pointer-coarse:min-h-11";

/** On a phone the input takes its own line and Save the width under it. */
export const nameFormClasses: NameFormClasses = {
  input: `${inputClass} max-sm:basis-full`,
  save: `${compactButtonClass.primary} max-sm:flex-1`,
  cancel: compactButtonClass.quiet,
  problem: "text-overdue-ink",
};

export const cardClass = "rounded-[14px] border border-line bg-surface p-4 sm:p-5";
export const cardTitleClass = "text-base font-semibold";
/** A small heading inside a card: "Options, highest priority first", "Suggested by tags". */
export const labelClass = "text-xs font-medium tracking-[0.04em] text-muted uppercase";
export const introClass = "max-w-[600px] text-sm/normal text-ink-soft";

/** A weight from a field's text: a whole number from 0 to 100, with anything that isn't a number counting as 0. */
export const clampWeight = (raw: string | number): number => Math.max(0, Math.min(100, Math.round(Number(raw) || 0)));
