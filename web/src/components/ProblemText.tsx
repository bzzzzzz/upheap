import { ApiProblem } from "../api/client";

/** What to tell the user for problem codes they can act on. Anything else shows the server's title. */
const MESSAGES: Record<string, string> = {
  last_admin: "A team needs an admin. Make someone else an admin first.",
  version_conflict: "Someone else changed this. It now shows their version.",
  user_not_found: "That user no longer exists.",
  heap_archived: "This heap is archived, so it can't be changed. Unarchive it first.",
  field_empty: "A name can't be empty.",
};

export function problemMessage(error: Error): string {
  return (error instanceof ApiProblem && MESSAGES[error.code]) || error.message;
}

export function ProblemText({ error }: { error: Error | null }) {
  if (!error) return null;
  return (
    <p role="alert" className="basis-full text-sm text-red-700 dark:text-red-400">
      {problemMessage(error)}
    </p>
  );
}
