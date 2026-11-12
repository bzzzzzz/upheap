import { useEffect, useRef, useState, type KeyboardEvent } from "react";

import type { Item } from "../api/client";
import { compactButtonClass } from "./buttons";
import { useRegisterDraft } from "./contextDraft";
import { itemProblem } from "./itemProblems";
import { Markdown } from "./Markdown";
import { PHONE, useMedia } from "./media";
import { Sheet } from "./Sheet";
import { useFocusAfterRender } from "./useFocusAfterRender";

/**
 * The item's context, its description, read as Markdown with an Edit button (ux.md, The item §17). Edit swaps the text
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
  onSave: (text: string) => Promise<void>;
}) {
  const phone = useMedia(PHONE);
  const focusAfterRender = useFocusAfterRender();
  const editButton = useRef<HTMLButtonElement>(null);
  const [editing, setEditing] = useState(false);
  const text = item.description ?? "";
  if (!canEdit && text === "" && !editing) return null;

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
        <ContextForm context={text} canEdit={canEdit} onSave={onSave} onDone={done} />
      ) : empty ? (
        <p className="text-sm text-muted">No context yet.</p>
      ) : (
        <Markdown text={text} />
      )}
      {phone && (
        <Sheet title="Context" isOpen={editing} onClose={done} aside={item.title}>
          <ContextEditor context={text} canEdit={canEdit} onSave={onSave} />
        </Sheet>
      )}
    </section>
  );
}

/**
 * The source of the context on a wide screen and a tablet, in place of the text. The draft is local, started from the
 * saved text when Edit was pressed, so a refetch or someone else's save while typing leaves it alone. Save, or ⌘ or Ctrl
 * with Enter, saves and goes back to the text, but only once the save has landed: a failure is said under the text and
 * the draft stays. If the saved text is no longer the one Edit opened, someone else changed it, and Save asks instead of
 * overwriting: it shows their text and offers Save mine anyway or Use theirs. Cancel or Escape throws the draft away.
 * Escape stops here, so it doesn't also close the item. If the item closes meanwhile, the draft stays on screen to read
 * and select, unsaved.
 *
 * Leaving with a changed draft (the item closed, the pane walked to the next) saves it too, as the form goes away, and
 * can't say if that fails. It skips the save when the text moved since Edit, or the item closed: losing the draft that
 * way is better than silently overwriting someone else's text, or sending a write the server refuses.
 */
function ContextForm({
  context,
  canEdit,
  onSave,
  onDone,
}: {
  context: string;
  canEdit: boolean;
  onSave: (text: string) => Promise<void>;
  onDone: () => void;
}) {
  const register = useRegisterDraft();
  const [opened, setOpened] = useState(context);
  const [text, setText] = useState(opened);
  const [conflict, setConflict] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const field = useRef<HTMLTextAreaElement>(null);
  // What the cleanup and the registered flush read: the text as last typed, the text Edit opened with and the text saved
  // now, the save of that moment, and whether the draft was dealt with already, by Save or Cancel.
  const latest = useRef({ text, opened, context, canEdit, onSave, settled: false });
  useEffect(() => {
    latest.current.text = text;
    latest.current.context = context;
    latest.current.canEdit = canEdit;
    latest.current.onSave = onSave;
  });

  // Writes the draft and answers whether it landed, keeping the form and saying why when it didn't.
  const commit = async (): Promise<boolean> => {
    setPending(true);
    setError(null);
    try {
      await latest.current.onSave(latest.current.text);
    } catch (e) {
      setError(itemProblem(e instanceof Error ? e : new Error(String(e))));
      return false;
    } finally {
      setPending(false);
    }
    latest.current.settled = true;
    return true;
  };
  // The draft as a save goes: nothing to write for an unchanged one, a question for one over someone else's change.
  const attempt = async (): Promise<boolean> => {
    const now = latest.current;
    if (now.text === now.opened) return true;
    if (now.context !== now.opened) {
      setConflict(true);
      return false;
    }
    return commit();
  };
  const saveAndClose = () => {
    if (pending) return;
    void attempt().then((saved) => saved && onDone());
  };
  const saveAnyway = () => {
    latest.current.opened = latest.current.context;
    setOpened(latest.current.context);
    setConflict(false);
    void commit().then((saved) => saved && onDone());
  };
  const discard = () => {
    latest.current.settled = true;
    onDone();
  };

  // A change that closes the item saves the draft first, through here.
  useEffect(() => {
    if (!register) return;
    register(() => async () => {
      const saved = await attempt();
      // Saved, the draft is what Edit opened with now, so cancelling the change that follows leaves nothing to ask.
      if (saved) {
        latest.current.opened = latest.current.text;
        setOpened(latest.current.text);
      }
      return saved;
    });
    return () => register(null);
    // `attempt` reads only `latest` and state setters, so the first one registered serves for the form's life.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [register]);
  useEffect(
    () => () => {
      const now = latest.current;
      if (!now.settled && now.canEdit && now.text !== now.opened && now.context === now.opened) {
        now.onSave(now.text).catch(() => undefined);
      }
    },
    [],
  );
  // With the caret at the end of the text.
  useEffect(() => {
    const element = field.current;
    if (!element) return;
    element.focus();
    element.setSelectionRange(element.value.length, element.value.length);
  }, []);
  // An item that closed after the draft was saved (by the flush above) has nothing left to hold open.
  const kept = !canEdit && text === context;
  useEffect(() => {
    if (kept) onDone();
    // `onDone` is a fresh function each render and only closes the form.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [kept]);

  if (!canEdit) {
    if (kept) return null;
    return (
      <div className="flex flex-col gap-2">
        <p role="status" className="text-sm text-muted">
          The item was closed; your text wasn't saved.
        </p>
        <textarea
          aria-label="Context"
          readOnly
          value={text}
          rows={8}
          className="field-sizing-content min-h-40 w-full rounded-lg border border-line bg-sunken px-2 py-1.5 text-sm outline-none pointer-coarse:text-base"
        />
        <div>
          <button type="button" onClick={discard} className={compactButtonClass.quiet}>
            Close
          </button>
        </div>
      </div>
    );
  }

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      discard();
    } else if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      saveAndClose();
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
      {error !== null && (
        <p role="alert" className="text-sm text-overdue-ink">
          {error}
        </p>
      )}
      {conflict && context !== opened && (
        <div role="alert" className="flex flex-col gap-2 rounded-lg border border-line bg-sunken p-2">
          <p className="text-sm font-medium text-ink">Someone else changed the context meanwhile.</p>
          <details>
            <summary className="cursor-pointer text-sm text-muted">Their text</summary>
            <div className="pt-2">
              {context === "" ? <p className="text-sm text-muted">Empty.</p> : <Markdown text={context} />}
            </div>
          </details>
          <div className="flex gap-2">
            <button type="button" disabled={pending} onClick={saveAnyway} className={compactButtonClass.primary}>
              Save mine anyway
            </button>
            <button type="button" onClick={discard} className={compactButtonClass.quiet}>
              Use theirs
            </button>
          </div>
        </div>
      )}
      <div className="flex gap-2">
        <button type="button" disabled={pending} onClick={saveAndClose} className={compactButtonClass.primary}>
          Save
        </button>
        <button type="button" onClick={discard} className={compactButtonClass.quiet}>
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
function ContextEditor({
  context,
  canEdit,
  onSave,
}: {
  context: string;
  canEdit: boolean;
  onSave: (text: string) => Promise<void>;
}) {
  const [opened] = useState(context);
  const [text, setText] = useState(opened);
  // What the cleanup reads as the sheet goes away: the text as last typed, and the save of that moment.
  const latest = useRef({ text, context, canEdit, onSave });
  useEffect(() => {
    latest.current = { text, context, canEdit, onSave };
  });
  // Not when the text moved since the sheet opened or the item closed: see ContextForm.
  useEffect(
    () => () => {
      const now = latest.current;
      if (now.canEdit && now.text !== opened && now.context === opened) now.onSave(now.text).catch(() => undefined);
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
