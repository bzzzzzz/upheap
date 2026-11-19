import { ApiProblem } from "../api/client";
import { useCopyText } from "./copyLink";

/**
 * The server's request ID and a Copy button, on one line. `className` sets the colours, which differ between a page
 * and the toast. The ID is plain text, so it can be selected if the write to the clipboard fails.
 */
export function RequestIdLine({ id, label, className = "" }: { id: string; label: string; className?: string }) {
  const { copied, copy } = useCopyText();
  return (
    <div className={`flex basis-full flex-wrap items-center gap-x-2 gap-y-1 text-xs ${className}`}>
      <span className="opacity-70">{label}</span>
      <span className="font-mono break-all">{id}</span>
      <button
        type="button"
        onClick={() => void copy(id)}
        className="rounded-md border border-current/30 px-2 py-0.5 font-medium hover:bg-black/5 pointer-coarse:min-h-11 pointer-coarse:px-3"
      >
        {copied ? "Copied" : "Copy"}
      </button>
    </div>
  );
}

/**
 * The request ID of a server error, under the error's words. Nothing for any other error. `className` adds to the
 * line's layout, such as `justify-center` under a centred message.
 */
export function RequestId({ error, className = "" }: { error: Error | null; className?: string }) {
  if (!(error instanceof ApiProblem) || error.requestId === undefined) return null;
  return <RequestIdLine id={error.requestId} label="Request ID:" className={`text-muted ${className}`} />;
}
