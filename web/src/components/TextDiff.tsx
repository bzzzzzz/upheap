// An edited text in history as the server's word diff: removed words struck through in red, added ones in teal, and
// long unchanged stretches folded around the edits (ux.md, The item, point 22).
import { Fragment, useRef, useState } from "react";

import type { AuditTextDiff } from "../api/client";
import { useFocusAfterRender } from "./useFocusAfterRender";

/** An unchanged stretch longer than this many words folds. */
const FOLD_OVER = 24;
/** The words a folded stretch keeps next to each edit. */
const KEEP = 8;

/** The diff's pieces in order, same text plain, with each long unchanged stretch shortened to a button that unfolds it. */
export function TextDiff({ diff }: { diff: AuditTextDiff }) {
  if (diff.spacing_only) {
    return <span className="text-muted">Only spacing changed</span>;
  }
  const { pieces } = diff;
  return (
    <span className="whitespace-pre-wrap">
      {pieces.map((piece, index) => {
        // Spacing changed beside a word edit has nothing to see: the new spacing reads as plain text.
        const blank = piece.text.trim() === "";
        switch (piece.kind) {
          case "added":
            return blank ? (
              <Fragment key={index}>{piece.text}</Fragment>
            ) : (
              <ins key={index} className="rounded-sm bg-added-soft text-added-ink no-underline">
                <span className="sr-only">added </span>
                {piece.text}
              </ins>
            );
          case "removed":
            return blank ? null : (
              <del key={index} className="rounded-sm bg-removed-soft text-removed-ink line-through">
                <span className="sr-only">removed </span>
                {piece.text}
              </del>
            );
          case "same":
            return (
              <SameText key={index} text={piece.text} editBefore={index > 0} editAfter={index < pieces.length - 1} />
            );
        }
      })}
    </span>
  );
}

/**
 * An unchanged stretch, folded when it is long: it keeps {@link KEEP} words on each side that has an edit, and the
 * rest is a button that shows it. Unfolding moves focus to the stretch, since the button goes.
 */
function SameText({ text, editBefore, editAfter }: { text: string; editBefore: boolean; editAfter: boolean }) {
  const [unfolded, setUnfolded] = useState(false);
  const stretch = useRef<HTMLSpanElement>(null);
  const focusAfterRender = useFocusAfterRender();
  // Words and the whitespace between them, so the parts shown keep their spacing and line breaks.
  const tokens = text.split(/(\s+)/);
  const words = tokens.flatMap((token, index) => (token.trim() !== "" ? [index] : []));
  if (unfolded || words.length <= FOLD_OVER) {
    return (
      <span ref={stretch} tabIndex={unfolded ? -1 : undefined} className="outline-none">
        {text}
      </span>
    );
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
        onClick={() => {
          setUnfolded(true);
          focusAfterRender(() => stretch.current);
        }}
        className="rounded-sm text-muted underline decoration-dotted outline-none hover:text-ink focus-visible:outline-2 focus-visible:outline-accent pointer-coarse:inline-flex pointer-coarse:min-h-11 pointer-coarse:items-center"
      >
        … {hidden} more words
      </button>
      {editAfter && " "}
      {tokens.slice(tailStart).join("")}
    </>
  );
}
