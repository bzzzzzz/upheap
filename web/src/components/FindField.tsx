import { Search, X } from "lucide-react";
import { Button, Input, Label, SearchField } from "react-aria-components";

/**
 * The search field over a picker's menu, inside an `Autocomplete` that holds its text: focused on open, with Clear
 * once something is typed. `label` names it and is its placeholder.
 */
export function FindField({ label, text }: { label: string; text: string }) {
  return (
    <SearchField autoFocus className="relative flex shrink-0 items-center">
      <Label className="sr-only">{label}</Label>
      <Search aria-hidden="true" className="pointer-events-none absolute left-3.5 size-4 text-muted" />
      <Input
        placeholder={label}
        className="h-12 w-full rounded-xl border-2 border-transparent bg-sunken pr-11 pl-10 text-base outline-none placeholder:text-muted focus:border-accent sm:h-10 sm:rounded-[9px] sm:pr-10 sm:pl-9 sm:text-sm [&::-webkit-search-cancel-button]:hidden"
      />
      {text !== "" && (
        <Button
          aria-label="Clear"
          className="absolute right-0 flex size-12 items-center justify-center rounded-xl text-ink-soft outline-none hover:text-ink focus-visible:outline-2 focus-visible:outline-accent sm:size-10"
        >
          <X aria-hidden="true" className="size-4" />
        </Button>
      )}
    </SearchField>
  );
}
