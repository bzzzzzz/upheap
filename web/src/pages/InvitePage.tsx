import { Link, useRouter } from "@tanstack/react-router";
import { useEffect, useRef, type ReactNode, type RefObject } from "react";

import { ApiProblem, type Invitation, type Team } from "../api/client";
import {
  isInvitationChange,
  useInvitation,
  useJoinByInvite,
  useMe,
  useRegisterByInvite,
  useSignOut,
  useTeam,
} from "../api/queries";
import { problemMessage } from "../components/ProblemText";
import { AuthCard, AuthPage, authButtonClass, type RegisterRequest } from "./AuthPage";

/** Whether the page has shown a state already, so a heading that mounts now replaces one and takes focus. */
type MoveFocus = RefObject<boolean>;

type Land = (team: Team, line: string) => void;

/**
 * Goes to where a new member lands (ux.md, Invite page): the team's heap when it has exactly one that isn't
 * archived, otherwise the Team page. It replaces the invite page, so Back doesn't return to it, and carries `line`
 * for the shell to show once.
 */
function landOn(router: ReturnType<typeof useRouter>, team: Team, line: string) {
  const open = team.heaps.filter((heap) => heap.archived_at == null);
  const heap = open.length === 1 ? open[0] : undefined;
  if (heap) {
    void router.navigate({
      to: "/teams/$team/heaps/$heap",
      params: { team: team.slug, heap: heap.slug },
      replace: true,
      state: { joined: line },
    });
  } else {
    void router.navigate({ to: "/teams/$team", params: { team: team.slug }, replace: true, state: { joined: line } });
  }
}

/**
 * The page an invite link opens (ux.md, Invite page), outside the app's shell like sign-in: create an account, or
 * sign in and join, or learn why the link can't be used. The server decides which, through the invitation's state,
 * the viewer's standing and `allowed_actions`; this only shows it.
 */
export function InvitePage({ token }: { token: string }) {
  const router = useRouter();
  const invitation = useInvitation(token);
  // Focus moves to the heading when the page changes state, not when it first loads. `moveFocus` turns on after the
  // first state shows (a child's effect runs before this one), and a heading reads it as it mounts.
  const moveFocus = useRef(false);
  const loading = invitation.isPending;
  useEffect(() => {
    if (!loading) moveFocus.current = true;
  }, [loading]);
  const land: Land = (team, line) => landOn(router, team, line);

  const { data, error } = invitation;
  if (invitation.isPending) {
    return (
      <AuthCard>
        <Checking />
      </AuthCard>
    );
  }
  if (error instanceof ApiProblem && error.status === 404) {
    return (
      <AuthCard>
        <Notice moveFocus={moveFocus} heading="This isn't an invite link.">
          <p className="text-sm text-ink-soft">Check that the whole link was copied, or ask the team for a new one.</p>
        </Notice>
      </AuthCard>
    );
  }
  if (!data) {
    // A failed refresh with a link on screen keeps it; this is only when there is none.
    return (
      <AuthCard>
        <Retry error={error} onRetry={() => void invitation.refetch()} />
      </AuthCard>
    );
  }
  return (
    <Opened
      invitation={data}
      token={token}
      land={land}
      moveFocus={moveFocus}
      onChanged={() => void invitation.refetch()}
    />
  );
}

/** A read that failed: the problem where the content would be, and the way to try again. */
function Retry({ error, onRetry }: { error: Error | null; onRetry: () => void }) {
  return (
    <div className="flex flex-col gap-4">
      <p role="alert" className="text-sm font-medium text-overdue-ink">
        {error ? problemMessage(error) : "The link couldn't be read."}
      </p>
      <button type="button" onClick={onRetry} className={authButtonClass}>
        Try again
      </button>
    </div>
  );
}

function Checking() {
  return (
    <p role="status" className="py-6 text-center text-sm text-muted">
      Checking the link…
    </p>
  );
}

/**
 * The link's state, in the order the page decides it (ux.md, Invite page). Signed out has the sign-in page's card
 * already; the rest sit in one.
 */
function Opened({
  invitation,
  token,
  land,
  moveFocus,
  onChanged,
}: {
  invitation: Invitation;
  token: string;
  land: Land;
  moveFocus: MoveFocus;
  onChanged: () => void;
}) {
  const { team_name: team, created_by_name: maker, state, viewer, allowed_actions: actions } = invitation;
  if (state === "working" && !viewer && actions.includes("create_account")) {
    return <SignedOut invitation={invitation} token={token} land={land} moveFocus={moveFocus} onChanged={onChanged} />;
  }
  return (
    <AuthCard>
      {viewer?.status === "member" && viewer.team_slug ? (
        <AlreadyIn slug={viewer.team_slug} land={land} />
      ) : state !== "working" ? (
        <Notice
          moveFocus={moveFocus}
          heading={state === "ended" ? "This invite link has ended." : "This invite link was revoked."}
        >
          <p className="text-sm text-ink-soft">
            It was for {team}, from {maker}.
          </p>
          <p className="text-sm text-ink-soft">Ask the team for a new one.</p>
        </Notice>
      ) : viewer?.status === "removed" ? (
        <Notice moveFocus={moveFocus} heading={`You were removed from ${team}.`}>
          <p className="text-sm text-ink-soft">This link can't add you back. Ask one of the team's admins.</p>
        </Notice>
      ) : viewer && actions.includes("join") ? (
        <Confirm invitation={invitation} token={token} land={land} moveFocus={moveFocus} />
      ) : (
        <Notice moveFocus={moveFocus} heading="This invite link can't be used." />
      )}
    </AuthCard>
  );
}

/** A heading the page moves focus to as it replaces another state of the page. */
function Heading({ children, moveFocus }: { children: ReactNode; moveFocus: MoveFocus }) {
  const ref = useRef<HTMLHeadingElement>(null);
  // Decided as the heading mounts: later renders of the same state leave focus where the user put it.
  useEffect(() => {
    if (moveFocus.current) ref.current?.focus();
  }, [moveFocus]);
  return (
    <h2 ref={ref} tabIndex={-1} className="text-xl font-semibold outline-none sm:text-lg">
      {children}
    </h2>
  );
}

/** A link that can't be used, in words and the way back: Open Upheap sends a signed-out visitor on to sign-in. */
function Notice({ heading, moveFocus, children }: { heading: string; moveFocus: MoveFocus; children?: ReactNode }) {
  return (
    <div className="flex flex-col gap-3">
      <Heading moveFocus={moveFocus}>{heading}</Heading>
      {children}
      <Link to="/" className={`${authButtonClass} mt-1 flex items-center justify-center`}>
        Open Upheap
      </Link>
    </div>
  );
}

/** Already in the team: nothing to show, but the team's page, with a line saying so. */
function AlreadyIn({ slug, land }: { slug: string; land: Land }) {
  const team = useTeam(slug);
  const landed = useRef(false);
  const { data } = team;
  useEffect(() => {
    if (!data || landed.current) return;
    landed.current = true;
    land(data, `You're already in ${data.name}.`);
  });
  if (team.isError && !data) return <Retry error={team.error} onRetry={() => void team.refetch()} />;
  return <Checking />;
}

/** Signed in, and the link admits them: say who they are, and join on their word. */
function Confirm({
  invitation,
  token,
  land,
  moveFocus,
}: {
  invitation: Invitation;
  token: string;
  land: Land;
  moveFocus: MoveFocus;
}) {
  const { data: me } = useMe();
  const join = useJoinByInvite(token);
  const signOut = useSignOut();
  const { team_name: team } = invitation;
  // A change to the invitation shows as the page it makes, not as an error above the button.
  const error = join.error && !isInvitationChange(join.error) ? join.error : null;
  return (
    <div className="flex flex-col gap-3">
      <Heading moveFocus={moveFocus}>Join {team}?</Heading>
      <p className="text-sm text-ink-soft">{invitation.created_by_name} invited you.</p>
      {me && (
        <p className="text-sm text-ink-soft">
          You're signed in as {me.user.display_name} (@{me.user.username}).
        </p>
      )}
      {error && (
        <p role="alert" className="text-[13px] font-medium text-overdue-ink max-sm:text-sm">
          {problemMessage(error)}
        </p>
      )}
      <button
        type="button"
        // Stays disabled once joined, until the landing replaces the page.
        disabled={join.isPending || join.isSuccess}
        onClick={() =>
          join.mutate(undefined, {
            onSuccess: ({ team: joined, created }) =>
              land(joined, created ? `You joined ${joined.name}.` : `You're already in ${joined.name}.`),
          })
        }
        className={authButtonClass}
      >
        {join.isPending || join.isSuccess ? "Joining…" : `Join ${team}`}
      </button>
      <p className="text-center text-sm text-ink-soft">
        Not you?{" "}
        <button
          type="button"
          disabled={signOut.isPending}
          // Signing out drops the whole cache under this page's live queries, so the page is loaded afresh, as the
          // consent page does (`AuthorizeRoute`).
          onClick={() => signOut.mutate(undefined, { onSuccess: () => window.location.replace(window.location.href) })}
          className="font-semibold text-accent-ink underline disabled:opacity-40 pointer-coarse:min-h-11"
        >
          Sign out
        </button>
      </p>
    </div>
  );
}

/** Signed out: create an account in the team, or sign in and come back to the confirm. */
function SignedOut({
  invitation,
  token,
  land,
  moveFocus,
  onChanged,
}: {
  invitation: Invitation;
  token: string;
  land: Land;
  moveFocus: MoveFocus;
  onChanged: () => void;
}) {
  const register = useRegisterByInvite(token);
  const { team_name: team } = invitation;
  const request: RegisterRequest = {
    error: register.error,
    isPending: register.isPending,
    reset: register.reset,
    // The new account lands in the team the response carries, with no call to fetch it.
    mutate: (body) =>
      register.mutate(body, { onSuccess: ({ team: joined }) => land(joined, `You joined ${joined.name}.`) }),
  };
  return (
    <AuthPage
      header={
        <div className="flex flex-col gap-1">
          <Heading moveFocus={moveFocus}>Join {team}</Heading>
          <p className="text-sm text-ink-soft">{invitation.created_by_name} invited you.</p>
        </div>
      }
      initialMode="register"
      register={request}
      registerLabel="Create account and join"
      registerNote={`You'll join ${team} as a member, and get a heap of your own too.`}
      // Signing in drops the cache, this page's invitation with it; it is read again with the session, and the page
      // becomes the confirm, a landing, or what a removal says. It doesn't join by itself.
      onSignedIn={onChanged}
    />
  );
}
