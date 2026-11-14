import { useState } from "react";

import { compactButtonClass } from "./buttons";
import { Field } from "./Field";
import { itemProblem } from "./itemProblems";

/** The server's limit, in characters (domain `BlockReason::MAX_CHARS`). */
const MAX_CHARS = 300;
/** The counter shows from here, so a short reason has none. */
const COUNT_FROM = 250;

const length = (text: string) => [...text].length;

/**
 * The form in "Why it's blocked" (ux.md, The item §2): one line of text for what the item waits for outside the heap.
 * Save, or Enter, sends the text and Remove reason clears it; an empty text clears too. `save` resolves once the server
 * has it, and the editor stays open with the draft when it fails, saying why. `onDone` closes it. While it
 * saves it can't be closed, so a failure always shows and a late success can't close an editor opened after it. An
 * input takes one line, so pasted line breaks never reach the text.
 */
export function BlockReasonEditor({
  reason,
  save,
  saving,
  error,
  onDone,
}: {
  reason: string | null;
  save: (reason: string | null) => Promise<unknown>;
  saving: boolean;
  error: Error | null;
  onDone: () => void;
}) {
  const [draft, setDraft] = useState(reason ?? "");
  const text = draft.trim();
  const chars = length(text);
  const tooLong = chars > MAX_CHARS;
  const unchanged = text === (reason ?? "");

  const submit = (next: string | null) =>
    save(next).then(onDone, () => {
      // The mutation holds the error, and the editor shows it.
    });

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        if (!unchanged && !tooLong && !saving) void submit(text === "" ? null : text);
      }}
      className="flex flex-col gap-3"
    >
      <Field
        label="Reason"
        value={draft}
        onChange={setDraft}
        readOnly={saving}
        autoFocus
        onFocus={(event) => event.target.select()}
        hint="What it waits for outside this heap."
        error={
          tooLong
            ? `Keep it to ${MAX_CHARS} characters.`
            : error
              ? `Couldn't save the reason. ${itemProblem(error)}`
              : undefined
        }
      />
      {chars >= COUNT_FROM && (
        <p className={`text-right text-xs tabular-nums ${tooLong ? "text-overdue-ink" : "text-muted"}`}>
          {chars} / {MAX_CHARS}
        </p>
      )}
      <div className="flex items-center justify-end gap-2">
        {reason && (
          <button
            type="button"
            disabled={saving}
            onClick={() => void submit(null)}
            className={`${compactButtonClass.quietDanger} mr-auto`}
          >
            Remove reason
          </button>
        )}
        <button type="button" disabled={saving} onClick={onDone} className={compactButtonClass.quiet}>
          Cancel
        </button>
        <button type="submit" disabled={unchanged || tooLong || saving} className={compactButtonClass.primary}>
          {saving ? "Saving…" : "Save"}
        </button>
      </div>
    </form>
  );
}
