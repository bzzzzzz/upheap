import { ApiProblem } from "../api/client";

/** What to tell the user for problem codes they can act on. Anything else shows the server's title. */
const MESSAGES: Record<string, string> = {
  last_admin: "A team needs an admin. Make someone else an admin first.",
  version_conflict: "Someone else changed this. It now shows their version.",
  user_not_found: "That user no longer exists.",
  heap_archived: "This heap is archived, so it can't be changed. Unarchive it first.",
  field_empty: "A name can't be empty.",
  saved_view_archived: "Someone archived this view, so it can't be changed.",
  parameter_name_taken: "This heap already has a parameter with that name.",
  option_label_taken: "Two options can't have the same label.",
  options_mismatch: "Someone changed the options meanwhile. They now show the latest version.",
  replacement_required: "Pick another option in use for the items to switch to.",
  parameter_in_use: "Open items still have a value for this parameter. Clear their values first.",
  parameter_archived: "Someone archived this parameter, so it can't be changed.",
  weight_out_of_range: "The weight goes from 0 to 100.",
  too_many_options: "A parameter can have at most 50 options.",
  handle_taken: "That's taken. Pick another.",
  invalid_handle: "Use lowercase letters, digits and single dashes, starting with a letter.",
  personal_team: "A personal team is yours alone, so nobody can join it and its link stays the same.",
};

export function problemMessage(error: Error): string {
  const message = (error instanceof ApiProblem && MESSAGES[error.code]) || error.message;
  // A handle the server would take, such as `my-team` for `My-Team`.
  return error instanceof ApiProblem && error.hint ? `${message} Try “${error.hint}”.` : message;
}

export function ProblemText({ error }: { error: Error | null }) {
  if (!error) return null;
  return (
    <p role="alert" className="basis-full text-sm text-red-700 dark:text-red-400">
      {problemMessage(error)}
    </p>
  );
}
