import type { ReactNode, SyntheticEvent } from "react";

import { linkClass } from "./Markdown";

/** A Markdown link to a web address, `[label](https://…)`, or a bare web address up to the next space. */
const LINK = /\[([^\]\n]+)\]\((https?:\/\/[^\s)]+)\)|https?:\/\/[^\s<>]+/g;

/** Punctuation that ends a sentence after a bare address, kept out of the link. */
const TRAILING = /[.,;:!?'"]+$/;

/** A bare address's end: trailing punctuation, and a closing bracket the address didn't open, read as text. */
function trimBare(url: string) {
  let end = url.replace(TRAILING, "");
  while (end.endsWith(")") && end.split("(").length < end.split(")").length) {
    end = end.slice(0, -1).replace(TRAILING, "");
  }
  return end;
}

// A link inside a choice's label opens its page and leaves the choice as it was: the tap stops at the link, before
// React Aria's press handling on the label sees it.
const stop = (event: SyntheticEvent) => event.stopPropagation();

/**
 * One line of text written by people and agents, such as a question's short text or a choice (ux.md, Markdown text):
 * shown as written, except that a web address and a Markdown link to one are links that open a new tab.
 */
export function LinkedText({ text }: { text: string }) {
  const parts: ReactNode[] = [];
  let at = 0;
  for (const match of text.matchAll(LINK)) {
    const [whole, label, target] = match;
    const url = target ?? trimBare(whole);
    parts.push(text.slice(at, match.index));
    parts.push(
      <a
        key={match.index}
        href={url}
        target="_blank"
        rel="noopener noreferrer"
        onClick={stop}
        onPointerDown={stop}
        onPointerUp={stop}
        onKeyDown={stop}
        className={linkClass}
      >
        {label ?? url}
      </a>,
    );
    at = match.index + (label === undefined ? url.length : whole.length);
  }
  parts.push(text.slice(at));
  return <>{parts}</>;
}
