import { useEffect, useId, useRef } from "react";

import { compactButtonClass } from "../components/buttons";
import { ProblemText } from "../components/ProblemText";

/**
 * The inline confirm of a wide screen (ux.md, Rules §1), for removing someone, giving up your admin role or revoking an
 * invite link: focus moves to it, and Escape or Cancel goes back to the button that asked.
 */
export function InlineConfirm({
  question,
  confirmLabel,
  pending,
  error,
  onConfirm,
  onCancel,
}: {
  question: string;
  confirmLabel: string;
  pending: boolean;
  error: Error | null;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const questionId = useId();
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => box.current?.focus(), [question]);
  return (
    <div
      ref={box}
      role="alertdialog"
      aria-labelledby={questionId}
      tabIndex={-1}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          onCancel();
        }
      }}
      className="flex flex-wrap items-center gap-x-3 gap-y-2 rounded-[10px] border border-overdue-ink/25 bg-overdue-soft px-3.5 py-2.5 text-[13px] outline-none focus-visible:outline-2 focus-visible:outline-accent"
    >
      <p id={questionId} className="min-w-0 flex-1 basis-60 text-overdue-ink">
        {question}
      </p>
      <div className="flex items-center gap-2">
        <button type="button" onClick={onCancel} className={`${compactButtonClass.quiet} py-1.5 text-[13px]`}>
          Cancel
        </button>
        <button
          type="button"
          disabled={pending}
          onClick={onConfirm}
          className={`${compactButtonClass.danger} py-1.5 text-[13px]`}
        >
          {confirmLabel}
        </button>
      </div>
      <ProblemText error={error} className="text-overdue-ink" />
    </div>
  );
}
