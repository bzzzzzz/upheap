import { useState } from "react";

import type { AuthorizationRequest } from "../api/client";
import { useApproveAuthorization, useAuthorizationCheck, useMe } from "../api/queries";
import { borderedClass } from "../components/buttons";
import { problemMessage } from "../components/ProblemText";
import { AuthCard } from "./AuthPage";

const primaryClass =
  "h-[52px] w-full rounded-[14px] bg-ink text-base font-semibold text-surface disabled:opacity-40 sm:h-11 sm:rounded-[10px] sm:text-sm";
const secondaryClass = `h-[52px] w-full rounded-[14px] text-base font-medium disabled:opacity-40 sm:h-11 sm:rounded-[10px] sm:text-sm ${borderedClass}`;

/**
 * Asks the signed-in user whether a client, such as the CLI, may use their account (OAuth's consent step). The server
 * checks the request first, so nothing is offered, and the browser is sent nowhere, unless the client and its
 * redirect URI are good. Allow has the server approve it and sends the browser on with the code; Cancel sends it on
 * with `access_denied`, as RFC 6749 §4.1.2.1 has the page do for a request the server accepted.
 */
export function AuthorizePage({
  request,
  onUseAnotherAccount,
}: {
  request: AuthorizationRequest;
  /** Signs out and goes to sign-in, which comes back here. */
  onUseAnotherAccount: () => void;
}) {
  const check = useAuthorizationCheck(request);
  const { data: me } = useMe();
  const approve = useApproveAuthorization();
  // Set once the browser is on its way back to the client, which may not leave this page (a listener that's gone).
  const [answered, setAnswered] = useState(false);

  const send = (to: string) => {
    setAnswered(true);
    window.location.assign(to);
  };
  const allow = () => approve.mutate(request, { onSuccess: ({ redirect_to }) => send(redirect_to) });
  const cancel = () => {
    // The redirect URI has no query of its own: the server only accepts loopback ones without.
    const query = new URLSearchParams({ error: "access_denied", state: request.state ?? "" });
    send(`${request.redirect_uri ?? ""}?${query.toString()}`);
  };

  if (check.error) {
    return (
      <AuthCard>
        <h2 className="text-base font-semibold">Can't connect this app</h2>
        <p role="alert" className="text-sm text-overdue-ink">
          {problemMessage(check.error)}
        </p>
      </AuthCard>
    );
  }
  if (!check.data || !me) {
    return (
      <AuthCard>
        <p className="text-sm text-muted">Checking the request…</p>
      </AuthCard>
    );
  }
  const client = check.data.display_name;
  return (
    <AuthCard>
      <h2 className="text-base font-semibold">Allow {client} to use your account?</h2>
      <p className="text-sm text-ink-soft">
        Signed in as {me.user.display_name} · @{me.user.username}
      </p>
      {!answered && (
        <button
          type="button"
          onClick={onUseAnotherAccount}
          className="self-start text-sm text-ink-soft underline-offset-2 hover:text-ink hover:underline"
        >
          Use another account
        </button>
      )}
      {answered ? (
        <p role="status" className="rounded-[10px] bg-accent-soft px-3 py-2.5 text-sm text-accent-ink">
          You can close this tab and go back to the terminal.
        </p>
      ) : (
        <>
          <p className="text-sm text-ink-soft">It can see and change everything you can, until you sign it out.</p>
          {approve.error && (
            <p role="alert" className="text-[13px] font-medium text-overdue-ink max-sm:text-sm">
              {problemMessage(approve.error)}
            </p>
          )}
          <div className="flex flex-col gap-2">
            <button type="button" className={primaryClass} disabled={approve.isPending} onClick={allow}>
              Allow
            </button>
            <button type="button" className={secondaryClass} disabled={approve.isPending} onClick={cancel}>
              Cancel
            </button>
          </div>
        </>
      )}
    </AuthCard>
  );
}
