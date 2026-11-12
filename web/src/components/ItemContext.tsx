import { useEffect, useRef, useState, type Dispatch, type KeyboardEvent, type SetStateAction } from "react";

import type { Item } from "../api/client";
import { compactButtonClass } from "./buttons";
import { useRegisterDraft, type FlushDraft } from "./contextDraft";
import { itemProblem } from "./itemProblems";
import { Markdown } from "./Markdown";
import { PHONE, useMedia } from "./media";
import { Sheet } from "./Sheet";
import { useFocusAfterRender } from "./useFocusAfterRender";

/** A blank text is no text, as the save stores it. */
const normal = (text: string) => (text.trim() === "" ? "" : text);

/**
 * The item's context, its description, read as Markdown with an Edit button (ux.md, The item §17). Edit swaps the text
 * in place for a form on a wide screen and a tablet, and opens the same form in a sheet on a phone. With no `edit` the
 * text is only read, and with none and no text the section is absent. `error` is the last failed save, said under the
 * text when no form is open to say it: a save made as the form went away has nowhere else to.
 */
export function ItemContext({
  item,
  canEdit,
  error,
  onSave,
  onReset,
}: {
  item: Item;
  canEdit: boolean;
  error: Error | null;
  onSave: (text: string) => Promise<void>;
  /** Forgets `error`, once the form that had it is closed. */
  onReset: () => void;
}) {
  const phone = useMedia(PHONE);
  const focusAfterRender = useFocusAfterRender();
  const register = useRegisterDraft();
  const editButton = useRef<HTMLButtonElement>(null);
  const section = useRef<HTMLElement>(null);
  const [editing, setEditing] = useState(false);
  // The open form's save, as a change that closes the item or the phone's sheet runs it first.
  const [saver, setSaver] = useState<FlushDraft | null>(null);
  useEffect(() => {
    if (!register) return;
    register(() => saver);
    return () => register(null);
  }, [register, saver]);
  const text = item.description ?? "";
  if (!canEdit && text === "" && !editing) return null;

  const done = () => {
    setEditing(false);
    onReset();
    // The Edit button, or the section where there is none to come back to (the item can't be edited any more).
    focusAfterRender(() => editButton.current ?? section.current);
  };
  const empty = text === "";
  const form = <ContextForm context={text} canEdit={canEdit} onSave={onSave} onSaver={setSaver} onDone={done} />;

  return (
    <section
      ref={section}
      tabIndex={-1}
      aria-label="Context"
      className="flex flex-col gap-2 border-t border-line pt-3 outline-none"
    >
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
        form
      ) : empty ? (
        <p className="text-sm text-muted">No context yet.</p>
      ) : (
        <Markdown text={text} />
      )}
      {!editing && error !== null && (
        <p role="alert" className="text-sm text-overdue-ink">
          {itemProblem(error)}
        </p>
      )}
      {phone && (
        <Sheet
          title="Context"
          isOpen={editing}
          // Dismissing the sheet saves a changed draft like Save does, and stays open if that doesn't land.
          onClose={() => {
            if (saver === null) done();
            else void saver().then((saved) => saved && done());
          }}
          aside={item.title}
        >
          {form}
        </Sheet>
      )}
    </section>
  );
}

/**
 * The form that edits the context, in place on a wide screen and a tablet and in the sheet on a phone; it reads and
 * behaves as the heap's Context card does (ux.md, Heap settings). The draft is local, started from the saved text when
 * Edit was pressed, so a refetch or someone else's save while typing leaves it alone. Save, or ⌘ or Ctrl with Enter,
 * saves and goes back to the text, but only once the save has landed: a failure is said under the text and the draft
 * stays. The text is read-only while a save is in flight, so what was typed is what was sent. If the saved text is no
 * longer the one Edit opened, someone else changed it: Save shows it above the draft under "Saved meanwhile" and
 * becomes Save again, which replaces it. Cancel throws the draft away. Escape does nothing, so a long draft is never
 * lost to a stray key (and it doesn't close the item). If the item can't be edited any more, the draft stays on
 * screen to read and select, unsaved.
 *
 * Leaving with a changed draft (the item closed, the pane walked to the next) saves it too, as the form goes away, and
 * can't say if that fails here; the section says it. It skips the save when the text moved since Edit, or the item
 * closed: losing the draft that way is better than silently overwriting someone else's text, or sending a write the
 * server refuses.
 */
function ContextForm({
  context,
  canEdit,
  onSave,
  onSaver,
  onDone,
}: {
  context: string;
  canEdit: boolean;
  onSave: (text: string) => Promise<void>;
  /** Hands out the form's save while it is open, and takes it back as it goes away. */
  onSaver: Dispatch<SetStateAction<FlushDraft | null>>;
  onDone: () => void;
}) {
  const [text, setText] = useState(context);
  // The saved text the draft was started from, or last saved over: what a different saved text is compared with.
  const [opened, setOpened] = useState(context);
  const [conflict, setConflict] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const field = useRef<HTMLTextAreaElement>(null);
  // What the cleanup and the registered save read, as they outlive the render they were made in.
  const latest = useRef({ text, opened, context, canEdit, onSave, pending: false, settled: false });
  useEffect(() => {
    latest.current.text = text;
    latest.current.context = context;
    latest.current.canEdit = canEdit;
    latest.current.onSave = onSave;
  });
  const rebase = (to: string) => {
    latest.current.opened = to;
    setOpened(to);
  };

  // Saves the draft and answers whether it is safe to go on: true when it landed or there was nothing to save, false
  // when the form stays, saying why. Reads only `latest` and state setters, so the first one made serves all along.
  const save = async (): Promise<boolean> => {
    const now = latest.current;
    if (now.pending) return false;
    const draft = normal(now.text);
    if (draft === now.opened || draft === now.context) return true;
    if (now.context !== now.opened) {
      // Someone else saved meanwhile: show it, and the next save replaces it.
      rebase(now.context);
      setConflict(true);
      return false;
    }
    const sent = now.text;
    now.pending = true;
    setPending(true);
    setError(null);
    try {
      await now.onSave(sent);
    } catch (e) {
      setError(itemProblem(e instanceof Error ? e : new Error(String(e))));
      return false;
    } finally {
      now.pending = false;
      setPending(false);
    }
    rebase(normal(sent));
    setConflict(false);
    return true;
  };
  const saveAndClose = () => {
    void save().then((saved) => {
      if (!saved) return;
      latest.current.settled = true;
      onDone();
    });
  };
  const discard = () => {
    latest.current.settled = true;
    onDone();
  };

  useEffect(() => {
    onSaver(() => save);
    return () => onSaver(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(
    () => () => {
      const now = latest.current;
      if (
        !now.settled &&
        !now.pending &&
        now.canEdit &&
        normal(now.text) !== now.opened &&
        now.context === now.opened
      ) {
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
  // An item that can't be edited any more, with nothing typed that isn't saved, has nothing left to hold open.
  const kept = !canEdit && (normal(text) === opened || normal(text) === context);
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
          This item can&apos;t be edited any more; your text wasn&apos;t saved.
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
    } else if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      saveAndClose();
    }
  };

  return (
    <div className="flex flex-col gap-2">
      {conflict && (
        <div>
          <h4 className="text-sm font-medium text-ink">Saved meanwhile</h4>
          <div className="mt-1 rounded-md border border-line bg-canvas p-3">
            {context === "" ? <p className="text-sm text-muted">No context.</p> : <Markdown text={context} />}
          </div>
        </div>
      )}
      <textarea
        ref={field}
        aria-label="Context"
        value={text}
        readOnly={pending}
        rows={8}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={onKeyDown}
        className="field-sizing-content min-h-40 w-full rounded-lg border border-line bg-surface px-2 py-1.5 text-sm outline-none focus:border-accent pointer-coarse:text-base"
      />
      <div className="flex gap-2 max-sm:[&>button]:flex-1">
        <button type="button" disabled={pending} onClick={saveAndClose} className={compactButtonClass.primary}>
          {conflict ? "Save again" : "Save"}
        </button>
        <button type="button" onClick={discard} className={compactButtonClass.quiet}>
          Cancel
        </button>
      </div>
      {conflict && (
        <p role="alert" className="text-sm text-overdue-ink">
          Someone else changed the context while you were editing. Their version is above; Save again to replace it with
          yours, or Cancel to keep theirs.
        </p>
      )}
      {error !== null && (
        <p role="alert" className="text-sm text-overdue-ink">
          {error}
        </p>
      )}
    </div>
  );
}
