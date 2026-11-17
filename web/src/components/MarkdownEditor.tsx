import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { defineLanguageFacet, HighlightStyle, Language, syntaxHighlighting } from "@codemirror/language";
import { Compartment, EditorSelection, EditorState, Transaction } from "@codemirror/state";
import { EditorView, keymap, placeholder as placeholderText } from "@codemirror/view";
import { tags } from "@lezer/highlight";
import { GFM, parser } from "@lezer/markdown";
import {
  useEffect,
  useLayoutEffect,
  useImperativeHandle,
  useRef,
  type FocusEventHandler,
  type KeyboardEventHandler,
  type Ref,
} from "react";

export type MarkdownEditorHandle = { focus: () => void };

/**
 * The markers stay visible but muted, and the text they mark is styled as the rendered `Markdown` styles it. Colours
 * are theme variables, so the theme switch needs no code here.
 */
const markdownStyle = HighlightStyle.define([
  { tag: tags.heading1, fontWeight: "600", fontSize: "1.07em" },
  { tag: [tags.heading2, tags.heading3, tags.heading4, tags.heading5, tags.heading6], fontWeight: "600" },
  { tag: tags.strong, fontWeight: "600" },
  { tag: tags.emphasis, fontStyle: "italic" },
  { tag: tags.strikethrough, textDecoration: "line-through" },
  { tag: tags.monospace, fontFamily: "var(--font-mono)", fontSize: "0.93em" },
  { tag: tags.link, color: "var(--accent-ink)", textDecoration: "underline" },
  { tag: tags.quote, color: "var(--ink-soft)" },
  { tag: [tags.processingInstruction, tags.url, tags.contentSeparator], color: "var(--muted)" },
]);

/**
 * GFM straight from the Lezer parser, which carries its own highlight tags. Not `@codemirror/lang-markdown`: it bundles
 * the HTML, CSS and JavaScript parsers for HTML blocks and code, about two thirds of the chunk, and gives nothing here
 * but autocompletion and list keys this editor leaves out.
 */
const markdownLanguage = new Language(defineLanguageFacet(), parser.configure(GFM), [], "markdown");

const theme = EditorView.theme({
  "&": { fontSize: "14px", color: "var(--ink)", flex: "1 1 auto" },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": { fontFamily: "inherit", lineHeight: "1.5", alignItems: "stretch" },
  // The padding is in the content, so a click anywhere in the box lands in the text.
  ".cm-content": { padding: "6px 8px", caretColor: "var(--ink)" },
  ".cm-line": { padding: "0" },
  ".cm-cursor": { borderLeftColor: "var(--ink)" },
  ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": { background: "var(--sunken)" },
  ".cm-placeholder": { color: "var(--muted)" },
});

/**
 * A Markdown source editor (ux.md, Markdown text): CodeMirror 6 keeping the text as typed and styling it as it goes. It
 * is a thin wrapper of our own rather than a React binding, as few props are needed. `value` is the text and is
 * controlled: a change from outside replaces the document, a change from the editor itself leaves the caret alone.
 * `placeholder` and `aria-label` are read once. `className` styles the outer box, which grows with the text. Load it
 * through `LazyMarkdownEditor`, so pages that only read don't carry CodeMirror.
 */
export default function MarkdownEditor({
  value,
  onChange,
  "aria-label": label,
  placeholder,
  readOnly = false,
  onSubmit,
  autoFocus = false,
  className,
  onKeyDown,
  onFocus,
  onBlur,
  ref,
}: {
  value: string;
  onChange: (text: string) => void;
  "aria-label": string;
  placeholder?: string;
  /** Text stays selectable and focus stays where it is. */
  readOnly?: boolean;
  /** Called on ⌘ or Ctrl with Enter. */
  onSubmit?: () => void;
  /** Takes focus with the caret at the end when it mounts. */
  autoFocus?: boolean;
  className?: string;
  onKeyDown?: KeyboardEventHandler<HTMLDivElement>;
  onFocus?: FocusEventHandler<HTMLDivElement>;
  onBlur?: FocusEventHandler<HTMLDivElement>;
  ref?: Ref<MarkdownEditorHandle>;
}) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const readOnlyCompartment = useRef(new Compartment());
  // What the view's handlers read, as the view is made once.
  const latest = useRef({ onChange, onSubmit });
  useEffect(() => {
    latest.current = { onChange, onSubmit };
  });

  useImperativeHandle(ref, () => ({
    focus: () => {
      const editor = view.current;
      if (!editor) return;
      editor.dispatch({ selection: EditorSelection.cursor(editor.state.doc.length) });
      editor.focus();
    },
  }));

  useEffect(() => {
    const editor = new EditorView({
      parent: host.current ?? undefined,
      state: EditorState.create({
        doc: value,
        selection: EditorSelection.cursor(value.length),
        extensions: [
          history(),
          keymap.of([
            {
              key: "Mod-Enter",
              run: () => {
                const submit = latest.current.onSubmit;
                if (!submit) return false;
                submit();
                return true;
              },
            },
            ...defaultKeymap,
            ...historyKeymap,
          ]),
          markdownLanguage,
          syntaxHighlighting(markdownStyle),
          EditorView.lineWrapping,
          // So iOS corrects and capitalises as it does in a textarea.
          EditorView.contentAttributes.of({
            "aria-label": label,
            spellcheck: "true",
            autocorrect: "on",
            autocapitalize: "sentences",
          }),
          ...(placeholder ? [placeholderText(placeholder)] : []),
          readOnlyCompartment.current.of(EditorState.readOnly.of(readOnly)),
          theme,
          EditorView.updateListener.of((update) => {
            if (update.docChanged) latest.current.onChange(update.state.doc.toString());
          }),
        ],
      }),
    });
    view.current = editor;
    if (autoFocus) editor.focus();
    return () => {
      editor.destroy();
      view.current = null;
    };
    // Made once: what changes afterwards is handled below or read through `latest`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    view.current?.dispatch({ effects: readOnlyCompartment.current.reconfigure(EditorState.readOnly.of(readOnly)) });
  }, [readOnly]);

  // A layout effect, so a stale value can't replace the text between a commit and its effect while typing. The
  // replacement stays out of the undo history: undoing after a comment was sent must not bring the sent text back.
  useLayoutEffect(() => {
    const editor = view.current;
    if (!editor || editor.state.doc.toString() === value) return;
    editor.dispatch({
      annotations: Transaction.addToHistory.of(false),
      changes: { from: 0, to: editor.state.doc.length, insert: value },
      selection: EditorSelection.cursor(value.length),
    });
  }, [value]);

  return (
    <div
      ref={host}
      onKeyDown={onKeyDown}
      onFocus={onFocus}
      onBlur={onBlur}
      className={`flex flex-col ${className ?? ""}`}
    />
  );
}
