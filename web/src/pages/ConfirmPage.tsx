import { useRouter } from "@tanstack/react-router";

import { ApiProblem } from "../api/client";
import { useConfirmEmail } from "../api/queries/emailConfirmation";
import { problemMessage } from "../components/ProblemText";
import { RequestId } from "../components/RequestId";
import { AuthCard, authButtonClass } from "./AuthPage";
import { Heading, Notice, useMoveFocus } from "./authStates";

/** The line the app shows once the user lands (`NoticeLine`). */
const CONFIRMED_NOTICE = "Address confirmed. You're signed in.";

/**
 * The page an email confirmation link opens (ux.md, Confirm page), outside the app's shell like sign-in. Nothing is
 * sent when it opens: a mail scanner that fetches the link confirms nothing, so the address is confirmed, and the
 * browser signed in, only when the button is pressed. The server decides, through the answer; this only shows it.
 * Holding the link is the permission, so a signed-in visitor sees the same page.
 */
export function ConfirmPage({ token }: { token: string }) {
  const router = useRouter();
  const confirm = useConfirmEmail(token);
  // Off at first, so the page's first heading doesn't take focus; on for the state the press brings.
  const moveFocus = useMoveFocus(false);
  const { error } = confirm;

  if (error instanceof ApiProblem && error.status === 404) {
    return (
      <AuthCard>
        <Notice moveFocus={moveFocus} heading="This isn't a working link.">
          <p className="text-sm text-ink-soft">Check that the whole link was copied.</p>
        </Notice>
      </AuthCard>
    );
  }
  if (error instanceof ApiProblem && error.status === 409) {
    return (
      <AuthCard>
        <Notice moveFocus={moveFocus} heading={error.message} />
      </AuthCard>
    );
  }
  // Disabled once confirmed too, until the landing replaces the page.
  const busy = confirm.isPending || confirm.isSuccess;
  return (
    <AuthCard>
      <div className="flex flex-col gap-4">
        <Heading moveFocus={moveFocus}>Confirm your address and sign in?</Heading>
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
          disabled={busy}
          onClick={() =>
            confirm.mutate(undefined, {
              onSuccess: () => void router.navigate({ to: "/", replace: true, state: { notice: CONFIRMED_NOTICE } }),
            })
          }
          className={authButtonClass}
        >
          Confirm and sign in
        </button>
      </div>
    </AuthCard>
  );
}
