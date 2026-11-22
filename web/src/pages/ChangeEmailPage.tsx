import { Link } from "@tanstack/react-router";

import { ApiProblem } from "../api/client";
import { useApplyEmailChange } from "../api/queries/emailChange";
import { problemMessage } from "../components/ProblemText";
import { RequestId } from "../components/RequestId";
import { AuthCard, authButtonClass } from "./AuthPage";
import { Heading, Notice, useMoveFocus, type MoveFocus } from "./authStates";
import { Saving } from "../components/Saving";

/** The way on from a state of the page: the Account page, which sends a signed-out visitor to sign-in. */
function AccountLink() {
  return (
    <Link to="/account" className={`${authButtonClass} mt-1 flex items-center justify-center`}>
      Go to your account
    </Link>
  );
}

/**
 * The page an email change link opens (ux.md, Change-email page), outside the app's shell like sign-in. Nothing is
 * read when it opens: a mail scanner that fetches the link changes nothing, so the address changes only when the
 * button is pressed. The server decides, through the answer; this only shows it. Holding the link is the permission,
 * so a signed-in visitor sees the same page.
 */
export function ChangeEmailPage({ token }: { token: string }) {
  const apply = useApplyEmailChange(token);
  // Off at first, so the page's first heading doesn't take focus; on for the state the press brings.
  const moveFocus = useMoveFocus(false);
  const { data, error } = apply;

  if (data) {
    return (
      <AuthCard>
        <Notice moveFocus={moveFocus} heading={`Your address is now ${data.email}.`} action={<AccountLink />} />
      </AuthCard>
    );
  }
  if (error instanceof ApiProblem && error.status === 404) {
    return (
      <AuthCard>
        <Notice moveFocus={moveFocus} heading="This isn't a working link.">
          <p className="text-sm text-ink-soft">Check that the whole link was copied.</p>
        </Notice>
      </AuthCard>
    );
  }
  if (error instanceof ApiProblem && error.code === "email_taken") {
    return (
      <AuthCard>
        <Notice
          moveFocus={moveFocus}
          heading="Another account uses this address now, so your address stays the same."
        />
      </AuthCard>
    );
  }
  if (error instanceof ApiProblem && error.status === 409) {
    return (
      <AuthCard>
        <Notice moveFocus={moveFocus} heading={error.message} action={<AccountLink />} />
      </AuthCard>
    );
  }
  return (
    <AuthCard>
      <Confirm moveFocus={moveFocus} pending={apply.isPending} error={error} onApply={() => apply.mutate()} />
    </AuthCard>
  );
}

/** The question and its button, with a problem the page has no state for above the button. */
function Confirm({
  moveFocus,
  pending,
  error,
  onApply,
}: {
  moveFocus: MoveFocus;
  pending: boolean;
  error: Error | null;
  onApply: () => void;
}) {
  return (
    <div className="flex flex-col gap-4">
      <Heading moveFocus={moveFocus}>Use the address this link was mailed to for your Upheap account?</Heading>
      {error && (
        <>
          <p role="alert" className="text-[13px] font-medium text-overdue-ink max-sm:text-sm">
            {problemMessage(error)}
          </p>
          <RequestId error={error} />
        </>
      )}
      <button
        type="button"
        disabled={pending}
        aria-busy={pending || undefined}
        onClick={onApply}
        className={authButtonClass}
      >
        <Saving pending={pending} />
        Use this address
      </button>
    </div>
  );
}
