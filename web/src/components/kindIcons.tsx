import { CalendarClock, CircleCheck, ClipboardCheck, FileText, ListChecks } from "lucide-react";

import type { Goal, Kind, Stage } from "../api/schema";

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

/** The icon of where a turn goes: the stage icons for with context and defined, a check for done. */
export function TargetIcon({ goal, className }: { goal: Goal; className: string }) {
  switch (goal) {
    case "with_context":
      return <FileText aria-hidden="true" className={className} />;
    case "defined":
      return <ClipboardCheck aria-hidden="true" className={className} />;
    case "done":
      return <CircleCheck aria-hidden="true" className={className} />;
  }
}
