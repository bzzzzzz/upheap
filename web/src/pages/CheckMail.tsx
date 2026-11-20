import { ApiProblem } from "../api/client";
import { useSignIn } from "../api/queries/auth";
import { problemMessage } from "../components/ProblemText";
import { RequestId } from "../components/RequestId";
import { Heading, type MoveFocus } from "./authStates";

/** The card replaces the forms, so its heading takes focus as it mounts. */
const MOVE_FOCUS: MoveFocus = { current: true };

/** Why the card shows: an account was just made, or a sign-in was refused until the address is confirmed. */
export type CheckMailCause = { kind: "created"; email: string } | { kind: "refused" };

/**
 * What a server with mail set up shows until the address is confirmed (ux.md, Sign in and create account): after
 * Create account, or after a sign-in with the right password. **Send it again** posts the sign-in with the login and
 * password the form held, which makes the server mail a fresh link; its `403 email_not_confirmed` means sent. A
 * sign-in that goes through (confirmed meanwhile, in another tab) is `onSignedIn`. `onWrongAddress` is the way back
 * to Create account's form, only offered after creating the account.
 */
export function CheckMail({
  cause,
  login,
  password,
  onSignedIn,
  onWrongAddress,
}: {
  cause: CheckMailCause;
  login: string;
  password: string;
  onSignedIn: () => void;
  onWrongAddress: () => void;
}) {
  const signIn = useSignIn();
  const { error } = signIn;
  const sent = error instanceof ApiProblem && error.code === "email_not_confirmed";
  return (
    <div className="flex flex-col gap-3">
      <Heading moveFocus={MOVE_FOCUS}>{cause.kind === "created" ? "Check your mail" : "Confirm your address"}</Heading>
      <p className="text-sm text-ink-soft">
        {cause.kind === "created"
          ? `We sent a link to ${cause.email}. It works for a day.`
          : "We mailed you a new link. It works for a day."}
      </p>
      <button
        type="button"
        disabled={signIn.isPending}
        onClick={() => signIn.mutate({ login, password }, { onSuccess: onSignedIn })}
        className="h-11 self-start rounded-[10px] border border-line-strong bg-surface px-4 text-sm font-semibold text-ink disabled:opacity-40 pointer-coarse:min-h-11"
      >
        Send it again
      </button>
      {/* Shown again, and so announced again, for each send: the mutation leaves its error while it is pending. */}
      {sent && (
        <p role="status" className="text-sm text-ink-soft">
          Sent. It can take a minute to arrive.
        </p>
      )}
      {error && !sent && (
        <>
          <p role="alert" className="text-[13px] font-medium text-overdue-ink max-sm:text-sm">
            {problemMessage(error)}
          </p>
          <RequestId error={error} />
        </>
      )}
      {cause.kind === "created" && (
        <p className="text-sm text-ink-soft">
          Wrong address?{" "}
          <button
            type="button"
            onClick={onWrongAddress}
            className="font-semibold text-accent-ink underline pointer-coarse:min-h-11"
          >
            Create the account again.
          </button>
        </p>
      )}
    </div>
  );
}
