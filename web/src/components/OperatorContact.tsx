import { usePasswordResetOptions } from "../api/queries/passwordReset";

/** The operator's contact line, as a paragraph under a line that says to ask whoever runs the server; nothing when they set none. */
export function OperatorContact({ className }: { className: string }) {
  const contact = usePasswordResetOptions().data?.operator_contact;
  return contact ? <p className={className}>{contact}</p> : null;
}
