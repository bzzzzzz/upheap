import { ClipboardCheck, FileText, ListChecks } from "lucide-react";

import type { Kind, Stage } from "../api/client";

const ICON_CLASS = "size-3.5 shrink-0 text-muted";

/** The icon of a task; a note has none. The one place that picks the icon for a kind. */
export function KindIcon({ kind }: { kind: Kind }) {
  return kind === "task" ? <ListChecks aria-hidden="true" className={ICON_CLASS} /> : null;
}

/** The icon of a stage that says something: with context or defined. As written has none. */
export function StageIcon({ stage }: { stage: Stage | null | undefined }) {
  switch (stage) {
    case "with_context":
      return <FileText aria-hidden="true" className={ICON_CLASS} />;
    case "defined":
      return <ClipboardCheck aria-hidden="true" className={ICON_CLASS} />;
    case "as_written":
    case null:
    case undefined:
      return null;
  }
}
