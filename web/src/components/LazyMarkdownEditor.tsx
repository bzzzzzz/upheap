import {
  Component,
  lazy,
  Suspense,
  useEffect,
  useImperativeHandle,
  useRef,
  type ComponentProps,
  type FocusEventHandler,
  type KeyboardEvent,
  type ReactNode,
} from "react";

import type MarkdownEditorType from "./MarkdownEditor";

const MarkdownEditor = lazy(() => import("./MarkdownEditor"));

type Props = ComponentProps<typeof MarkdownEditorType>;

/** The plain field that stands in when the editor's chunk can't load, with the editor's padding so it reads the same. */
function TextareaFallback({
  value,
  onChange,
  "aria-label": label,
  placeholder,
  readOnly,
  onSubmit,
  autoFocus,
  className,
  onKeyDown,
  onFocus,
  onBlur,
  ref,
}: Props) {
  const field = useRef<HTMLTextAreaElement>(null);
  const focusAtEnd = (area: HTMLTextAreaElement | null) => {
    if (!area) return;
    area.focus();
    area.setSelectionRange(area.value.length, area.value.length);
  };
  useImperativeHandle(ref, () => ({ focus: () => focusAtEnd(field.current) }));
  useEffect(() => {
    if (autoFocus) focusAtEnd(field.current);
    // On mount only.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return (
    <textarea
      ref={field}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      aria-label={label}
      placeholder={placeholder}
      readOnly={readOnly}
      onKeyDown={(e) => {
        if (onSubmit && e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
          e.preventDefault();
          onSubmit();
        }
        onKeyDown?.(e as unknown as KeyboardEvent<HTMLDivElement>);
      }}
      onFocus={onFocus as unknown as FocusEventHandler<HTMLTextAreaElement>}
      onBlur={onBlur as unknown as FocusEventHandler<HTMLTextAreaElement>}
      className={`field-sizing-content resize-none bg-transparent px-2 py-1.5 text-sm leading-normal outline-none ${className ?? ""}`}
    />
  );
}

/**
 * A tab left open across a server upgrade asks for a chunk file that is gone, and the router's error view would replace
 * the page. `lazy` keeps the failure, so a plain `textarea` takes over for good: the text stays writable.
 */
class LoadFailure extends Component<{ fallback: ReactNode; children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  render() {
    return this.state.failed ? this.props.fallback : this.props.children;
  }
}

/**
 * The Markdown editor, loaded when the first field shows (ux.md, Markdown text). Until the chunk is there an empty box
 * of the same size stands in, so nothing moves when the editor mounts; a field that takes focus on mount (`autoFocus`)
 * does so whenever that is.
 */
export function LazyMarkdownEditor(props: Props) {
  return (
    <LoadFailure fallback={<TextareaFallback {...props} />}>
      <Suspense fallback={<div className={props.className} />}>
        <MarkdownEditor {...props} />
      </Suspense>
    </LoadFailure>
  );
}
