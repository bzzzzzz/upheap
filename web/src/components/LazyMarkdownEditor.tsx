import { lazy, Suspense, type ComponentProps } from "react";

import type MarkdownEditorType from "./MarkdownEditor";

const MarkdownEditor = lazy(() => import("./MarkdownEditor"));

/**
 * The Markdown editor, loaded when the first field shows (ux.md, Markdown text). Until the chunk is there an empty box
 * of the same size stands in, so nothing moves when the editor mounts; a field that takes focus on mount (`autoFocus`)
 * does so whenever that is.
 */
export function LazyMarkdownEditor(props: ComponentProps<typeof MarkdownEditorType>) {
  return (
    <Suspense fallback={<div className={props.className} />}>
      <MarkdownEditor {...props} />
    </Suspense>
  );
}
