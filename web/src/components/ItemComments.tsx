import { useRef, useState } from "react";
import { Button, Label, TextArea, TextField } from "react-aria-components";

import type { HeapRef, Item } from "../api/client";
import { useAddComment } from "../api/queries";
import { compactButtonClass } from "./buttons";
import { itemProblem } from "./itemProblems";
import { fieldClass, focusClass } from "./ItemQuestions";
import { Markdown } from "./Markdown";
import { useFocusAfterRender } from "./useFocusAfterRender";

/**
 * An item's comments (ux.md, The item §18): the thread under the context, oldest first, at every width and always
 * open, and under it a field to add one when the server offers `comment`. A comment is never changed or deleted, so a
 * row has no controls, and its text is shown as Markdown (ux.md, Markdown text). Nothing is fetched here: `item` is a
 * single-item response, the only kind that carries the comments; a list row has `comments: null`, which reads as none.
 * Nothing is rendered when there are none and the caller may not add one.
 *
 * The draft lives in this component's state, apart from the item, so a refetch or another edit saving the item never
 * touches what is typed. It clears only once the server has taken the comment, which then shows from the item the
 * server answered with.
 */
export function ItemComments({ heapRef, item }: { heapRef: HeapRef; item: Item }) {
  const add = useAddComment(heapRef, item.key);
  const [draft, setDraft] = useState("");
  const [message, setMessage] = useState("");
  const fieldRef = useRef<HTMLTextAreaElement>(null);
  const focusAfterRender = useFocusAfterRender();
  const comments = item.comments ?? [];
  // A refusal refetches the item, which may no longer offer `comment` (the heap was archived meanwhile): the form
  // stays while the error does, so the typed text and the reason don't vanish together.
  const canComment = item.allowed_actions.includes("comment") || add.isError;

  if (comments.length === 0 && !canComment) return null;

  const send = () => {
    if (draft.trim() === "" || add.isPending) return;
    setMessage("");
    add.mutate(
      { body: draft },
      {
        onSuccess: () => {
          setDraft("");
          setMessage("Comment added");
          focusAfterRender(() => fieldRef.current);
        },
      },
    );
  };

  return (
    <section aria-label="Comments" className="flex flex-col gap-2">
      <h3 className="text-xs font-medium text-ink-soft">
        {comments.length > 0 ? `Comments ${comments.length}` : "Comments"}
      </h3>
      {comments.length > 0 && (
        <ul className="flex flex-col divide-y divide-line">
          {comments.map((comment) => (
            <li key={comment.number} className="flex flex-col gap-0.5 py-2 first:pt-0 last:pb-0">
              <p className="text-xs text-ink-soft">
                {comment.created_by?.display_name ?? "Someone"} · {comment.created_label}
              </p>
              <Markdown text={comment.body} />
            </li>
          ))}
        </ul>
      )}
      <span role="status" className="sr-only">
        {message}
      </span>
      {canComment && (
        <form
          className="flex flex-col gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            send();
          }}
        >
          <TextField
            value={draft}
            onChange={(value) => {
              setDraft(value);
              // Without the offer the error is what keeps the form, so it stays until the pane closes.
              if (add.isError && item.allowed_actions.includes("comment")) add.reset();
            }}
            className="flex flex-col gap-1.5"
          >
            <Label className="sr-only">Add a comment</Label>
            <TextArea ref={fieldRef} placeholder="Add a comment" rows={2} className={fieldClass} />
          </TextField>
          {add.isError && (
            <p role="alert" className="text-sm text-overdue-ink">
              {itemProblem(add.error)}
            </p>
          )}
          <Button
            type="submit"
            isDisabled={draft.trim() === "" || add.isPending}
            className={`${compactButtonClass.primary} ${focusClass} max-sm:min-h-11 max-sm:w-full sm:self-end`}
          >
            Comment
          </Button>
        </form>
      )}
    </section>
  );
}
