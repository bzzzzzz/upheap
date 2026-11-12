import { useEffect, useRef, useState, type KeyboardEvent } from "react";

import type { Item } from "../api/client";
import { compactButtonClass } from "./buttons";
import { Markdown } from "./Markdown";
import { PHONE, useMedia } from "./media";
import { Sheet } from "./Sheet";
import { useFocusAfterRender } from "./useFocusAfterRender";

/**
 * The item's context, its description, read as Markdown with an Edit button (ux.md, The item §4). Edit swaps the text
 * in place for its source with Save and Cancel on a wide screen and a tablet, and opens a full-height sheet on a
 * phone. With no `edit` the text is only read, and with none and no text the section is absent. Leaving with a changed
 * draft, by closing the item or walking to the next, saves it, so nothing typed is lost; Cancel and Escape are the
 * ways to throw it away.
 */
export function ItemContext({
  item,
  canEdit,
  onSave,
}: {
  item: Item;
  canEdit: boolean;
  onSave: (text: string) => void;
}) {
  const phone = useMedia(PHONE);
  const focusAfterRender = useFocusAfterRender();
  const editButton = useRef<HTMLButtonElement>(null);
  const [editing, setEditing] = useState(false);
  const text = item.description ?? "";
  if (!canEdit && text === "") return null;

  const done = () => {
    setEditing(false);
    focusAfterRender(() => editButton.current);
  };
  const empty = text === "";

  return (
    <section aria-label="Context" className="flex flex-col gap-2 border-t border-line pt-3">
      <div className="flex min-h-9 items-center justify-between gap-2 pointer-coarse:min-h-11">
        <h3 className="text-xs font-semibold text-muted">Context</h3>
        {canEdit && !(editing && !phone) && (
          <button
            ref={editButton}
            type="button"
            aria-label={empty ? "Add context" : "Edit context"}
            aria-haspopup={phone ? "dialog" : undefined}
            onClick={() => setEditing(true)}
            className={compactButtonClass.bordered}
          >
            {empty ? "Add" : "Edit"}
          </button>
        )}
      </div>
      {editing && !phone ? (
        <ContextForm context={text} onSave={onSave} onDone={done} />
      ) : empty ? (
        <p className="text-sm text-muted">No context yet.</p>
      ) : (
        <Markdown text={text} />
      )}
      {phone && (
        <Sheet title="Context" isOpen={editing} onClose={done} aside={item.title}>
          <ContextEditor context={text} onSave={onSave} />
        </Sheet>
      )}
    </section>
  );
}

/**
 * The source of the context on a wide screen and a tablet, in place of the text. The draft is local, started from the
 * saved text when Edit was pressed, so a refetch or someone else's save while typing leaves it alone; if Save then
 * conflicts, the item's conflict message says so. Save, or ⌘ or Ctrl with Enter, saves and goes back to the text;
 * Cancel or Escape throws the draft away. Escape stops here, so it doesn't also close the item.
 */
function ContextForm({
  context,
  onSave,
  onDone,
}: {
  context: string;
  onSave: (text: string) => void;
  onDone: () => void;
}) {
  const [opened] = useState(context);
  const [text, setText] = useState(opened);
  const field = useRef<HTMLTextAreaElement>(null);
  // What the cleanup reads as the form goes away: the text as last typed, the save of that moment, and whether the
  // draft was dealt with already, by Save or Cancel.
  const latest = useRef({ text, onSave, settled: false });
  useEffect(() => {
    latest.current.text = text;
    latest.current.onSave = onSave;
  });
  useEffect(
    () => () => {
      if (!latest.current.settled && latest.current.text !== opened) latest.current.onSave(latest.current.text);
    },
    [opened],
  );
  // With the caret at the end of the text.
  useEffect(() => {
    const element = field.current;
    if (!element) return;
    element.focus();
    element.setSelectionRange(element.value.length, element.value.length);
  }, []);

  const settle = (save: boolean) => {
    latest.current.settled = true;
    if (save) onSave(text);
    onDone();
  };
  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      settle(false);
    } else if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      settle(true);
    }
  };

  return (
    <div className="flex flex-col gap-2">
      <textarea
        ref={field}
        aria-label="Context"
        value={text}
        rows={8}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={onKeyDown}
        className="field-sizing-content min-h-40 w-full rounded-lg border border-line bg-surface px-2 py-1.5 text-sm outline-none focus:border-accent pointer-coarse:text-base"
      />
      <div className="flex gap-2">
        <button type="button" onClick={() => settle(true)} className={compactButtonClass.primary}>
          Save
        </button>
        <button type="button" onClick={() => settle(false)} className={compactButtonClass.quiet}>
          Cancel
        </button>
      </div>
    </div>
  );
}

/**
 * The item's context (its description) in a phone's sheet. It starts from the saved text each time the sheet opens, and
 * saves as the sheet goes away, however it goes (✕, Escape, a tap outside, Back, a turn of the phone), and only if it was
 * changed, so an untouched sheet never writes back what it was opened with.
 */
function ContextEditor({ context, onSave }: { context: string; onSave: (text: string) => void }) {
  const [opened] = useState(context);
  const [text, setText] = useState(opened);
  // What the cleanup reads as the sheet goes away: the text as last typed, and the save of that moment.
  const latest = useRef({ text, onSave });
  useEffect(() => {
    latest.current = { text, onSave };
  });
  useEffect(
    () => () => {
      if (latest.current.text !== opened) latest.current.onSave(latest.current.text);
    },
    [opened],
  );
  return (
    <textarea
      aria-label="Context"
      autoFocus
      value={text}
      onChange={(e) => setText(e.target.value)}
      rows={6}
      className="mt-1 mb-2 min-h-40 rounded-xl border border-line bg-surface px-3 py-2 text-base"
    />
  );
}
