// An edited text in history as the server's word diff: removed words struck through in red, added ones in teal, and
// long unchanged stretches folded around the edits (ux.md, History).
import { useState } from "react";

import type { AuditDiffPiece } from "../api/client";

/** An unchanged stretch longer than this many words folds. */
const FOLD_OVER = 24;
/** The words a folded stretch keeps next to each edit. */
const KEEP = 8;

/** `pieces` in order, same text plain, with each long unchanged stretch shortened to a button that unfolds it. */
export function TextDiff({ pieces }: { pieces: AuditDiffPiece[] }) {
  const [unfolded, setUnfolded] = useState<ReadonlySet<number>>(new Set());
  if (pieces.every((piece) => piece.kind === "same" || piece.text.trim() === "")) {
    return <span className="text-muted">Only spacing changed</span>;
  }
  return (
    <span className="whitespace-pre-wrap">
      {pieces.map((piece, index) => {
        switch (piece.kind) {
          case "added":
            return (
              <ins key={index} className="rounded-sm bg-added-soft text-added-ink no-underline">
                <span className="sr-only">added </span>
                {piece.text}
              </ins>
            );
          case "removed":
            return (
              <del key={index} className="rounded-sm bg-overdue-soft text-overdue-ink line-through">
                <span className="sr-only">removed </span>
                {piece.text}
              </del>
            );
          case "same":
            return (
              <SameText
                key={index}
                text={piece.text}
                editBefore={index > 0}
                editAfter={index < pieces.length - 1}
                unfolded={unfolded.has(index)}
                onUnfold={() => setUnfolded((open) => new Set(open).add(index))}
              />
            );
        }
      })}
    </span>
  );
}

/**
 * An unchanged stretch, folded when it is long: it keeps {@link KEEP} words on each side that has an edit, and the
 * rest is a button that shows it.
 */
function SameText({
  text,
  editBefore,
  editAfter,
  unfolded,
  onUnfold,
}: {
  text: string;
  editBefore: boolean;
  editAfter: boolean;
  unfolded: boolean;
  onUnfold: () => void;
}) {
  // Words and the whitespace between them, so the parts shown keep their spacing and line breaks.
  const tokens = text.split(/(\s+)/);
  const words = tokens.flatMap((token, index) => (token !== "" && token.trim() !== "" ? [index] : []));
  if (unfolded || words.length <= FOLD_OVER) {
    return <>{text}</>;
  }
  // Where the shown head ends and the shown tail starts, in tokens.
  const headEnd = editBefore ? (words[KEEP - 1] ?? 0) + 1 : 0;
  const tailStart = editAfter ? (words[words.length - KEEP] ?? tokens.length) : tokens.length;
  const hidden = words.filter((index) => index >= headEnd && index < tailStart).length;
  return (
    <>
      {tokens.slice(0, headEnd).join("")}
      {editBefore && " "}
      <button
        type="button"
        onClick={onUnfold}
        className="rounded-sm text-muted underline decoration-dotted outline-none hover:text-ink focus-visible:outline-2 focus-visible:outline-accent"
      >
        … {hidden} more words
      </button>
      {editAfter && " "}
      {tokens.slice(tailStart).join("")}
    </>
  );
}
