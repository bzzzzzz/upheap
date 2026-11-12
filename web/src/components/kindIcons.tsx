import { CalendarClock, ClipboardCheck, FileText, ListChecks } from "lucide-react";

import type { Kind, Stage } from "../api/client";

const ICON_CLASS = "size-3.5 shrink-0 text-muted";

/** The icon of a task or a reminder; a note has none. The one place that picks the icon for a kind. */
export function KindIcon({ kind }: { kind: Kind }) {
  switch (kind) {
    case "task":
      return <ListChecks aria-hidden="true" className={ICON_CLASS} />;
    case "reminder":
      return <CalendarClock aria-hidden="true" className={ICON_CLASS} />;
    case "note":
      return null;
  }
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
