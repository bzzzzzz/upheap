import { ApiProblem } from "../api/client";

/**
 * What to tell the user for problem codes they can act on, by `code`, or by `code.field` where the field changes the
 * words. Anything else shows the server's title.
 */
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
  // The sign-in and create-account form. The problems don't carry the limits, so the 10-character minimum is today's
  // `PASSWORD_MIN_CHARS` (domain::user), as the form's hint says.
  email_taken: "There's already an account with this email. Sign in instead.",
  email_invalid: "That doesn't look like an email address.",
  "field_empty.email": "Enter your email address.",
  "field_empty.login": "Enter your email or username.",
  "field_empty.password": "Enter your password.",
  "field_too_long.display_name": "That name is too long.",
  password_too_short: "Use at least 10 characters.",
  password_too_long: "That password is too long.",
  invalid_credentials: "That email or username and password don't match an account.",
  registration_closed: "This server isn't taking new accounts.",
};

export function problemMessage(error: Error): string {
  const message =
    (error instanceof ApiProblem && (MESSAGES[`${error.code}.${error.field}`] ?? MESSAGES[error.code])) ||
    error.message;
  // A handle the server would take, such as `my-team` for `My-Team`.
  return error instanceof ApiProblem && error.hint ? `${message} Try “${error.hint}”.` : message;
}

/** `className` is the text colour, which a page in the design tokens sets. */
export function ProblemText({
  error,
  className = "text-red-700 dark:text-red-400",
}: {
  error: Error | null;
  className?: string;
}) {
  if (!error) return null;
  return (
    <p role="alert" className={`basis-full text-sm ${className}`}>
      {problemMessage(error)}
    </p>
  );
}
