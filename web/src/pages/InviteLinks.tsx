import { Ellipsis, Link as LinkIcon } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";

import type { InviteLink, Team } from "../api/client";
import { useCreateInviteLink, useInviteLinks, useMe, useRevokeInviteLink } from "../api/queries";
import { compactButtonClass } from "../components/buttons";
import { inviteLink, useCopyText } from "../components/copyLink";
import { ProblemText } from "../components/ProblemText";
import { Sheet, sheetRowClass } from "../components/Sheet";
import { useReturnFocus } from "../components/useConfirmation";
import { InlineConfirm } from "./InlineConfirm";
import { cardClass, dangerTextButtonClass, moreButtonClass, roleButtonClass } from "./teamStyles";

const REVOKE_QUESTION = "Revoke this link? It stops working at once. People who joined through it stay.";

/** Whether the caller may make and revoke the team's invite links, from what the server allows (ux.md, Teams and Team). */
export function canManageInviteLinks(team: Team): boolean {
  return team.allowed_actions.includes("manage_invite_links");
}

/**
 * Makes invite links, for the Members section: the **Invite link** button and the link made last, which the list marks
 * as New until another is made or the page goes. The button sits by the member search, the list below it.
 */
export function useMakeInviteLink(team: string) {
  const create = useCreateInviteLink(team);
  const [newId, setNewId] = useState<string | null>(null);
  return {
    newId,
    isPending: create.isPending,
    error: create.error,
    // Runs once the list has been refetched, so the new link's row is there to take the mark.
    make: () => create.mutate(undefined, { onSuccess: (link) => setNewId(link.id) }),
  };
}

type MakeInviteLink = ReturnType<typeof useMakeInviteLink>;

/** The Invite link button (ux.md, Teams and Team): bordered and beside the search, or full width on a phone. */
export function InviteLinkButton({ making, phone }: { making: MakeInviteLink; phone: boolean }) {
  return (
    <button
      type="button"
      disabled={making.isPending}
      onClick={making.make}
      className={`${compactButtonClass.bordered} shrink-0 gap-1.5 py-[9px] text-[13px] ${
        phone ? "min-h-11 w-full" : "ml-auto"
      }`}
    >
      <LinkIcon aria-hidden="true" className="size-3.5" />
      Invite link
    </button>
  );
}

const madeBy = (link: InviteLink, you: boolean) => `Made by ${you ? "you" : link.created_by.display_name}`;

/**
 * The team's working invite links, newest first, with Copy and Revoke (ux.md, Teams and Team). Revoke asks first: inline
 * on a wide screen, in the ⋯ sheet on a phone. Only admins see it, and the list is fetched only for them. A copy the
 * browser refuses shows the link in a field to copy by hand.
 */
export function InviteLinks({ team, making, phone }: { team: Team; making: MakeInviteLink; phone: boolean }) {
  const allowed = canManageInviteLinks(team);
  const { data: me } = useMe();
  const links = useInviteLinks(team.slug, allowed);
  const revoke = useRevokeInviteLink(team.slug);
  const { copied, copy } = useCopyText();
  const [asking, setAsking] = useState<string | null>(null);
  // The link whose address is shown to copy by hand, after a copy that failed.
  const [manual, setManual] = useState<string | null>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const headingId = useId();
  const working = (links.data ?? []).filter((link) => link.state === "working");

  // A confirm for a link that is no longer listed (revoked, here or by another admin, or ended) goes with its row. Focus
  // moves to the heading only if it was lost with the row, which leaves it on the page's body.
  const gone = asking !== null && !working.some((link) => link.id === asking);
  useEffect(() => {
    if (gone && (!document.activeElement || document.activeElement === document.body)) heading.current?.focus();
  }, [gone]);

  if (!allowed) return null;

  const ask = (id: string) => {
    revoke.reset();
    setAsking(id);
  };
  const cancel = () => {
    revoke.reset();
    setAsking(null);
  };
  // The button that asked is gone with its row, so focus goes to the heading. A phone's sheet gives focus back to its
  // trigger as it closes, which is gone too, so that is let to finish first.
  const revoked = () => {
    cancel();
    requestAnimationFrame(() => heading.current?.focus());
  };
  const copyLink = async (link: InviteLink) => {
    if (link.token === null || link.token === undefined) return false;
    const ok = await copy(inviteLink(link.token));
    setManual(ok ? null : link.id);
    return ok;
  };

  return (
    <section aria-labelledby={headingId} className="flex flex-col gap-1.5">
      <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-0.5">
        <h3 ref={heading} tabIndex={-1} id={headingId} className="text-[13px] font-semibold text-ink-soft outline-none">
          Invite links
        </h3>
        <p className="text-[13px] text-muted">Anyone with a link joins as a member.</p>
      </div>
      <ProblemText error={links.error} className="text-overdue-ink" />
      {working.length > 0 && (
        <ul className={cardClass}>
          {working.map((link) => {
            const props = {
              link,
              you: link.created_by.id === me?.user.id,
              isNew: link.id === making.newId,
              confirming: asking === link.id,
              manual: manual === link.id,
              revoke,
              onAsk: () => ask(link.id),
              onCancel: cancel,
              onRevoked: revoked,
              onCopy: () => copyLink(link),
              onCloseManual: () => setManual(null),
            };
            return phone ? <PhoneLinkRow key={link.id} {...props} /> : <LinkRow key={link.id} {...props} />;
          })}
        </ul>
      )}
      <ProblemText error={making.error} className="text-overdue-ink" />
      <p role="status" className="text-[13px] text-ink-soft empty:sr-only">
        {copied ? "Link copied." : null}
      </p>
    </section>
  );
}

type RowProps = {
  link: InviteLink;
  you: boolean;
  isNew: boolean;
  confirming: boolean;
  /** The address is shown to copy by hand. */
  manual: boolean;
  revoke: { isPending: boolean; error: Error | null; mutate: (id: string, options: { onSuccess: () => void }) => void };
  onAsk: () => void;
  onCancel: () => void;
  onRevoked: () => void;
  onCopy: () => Promise<boolean>;
  onCloseManual: () => void;
};

const newBadge = (
  <span className="shrink-0 rounded-md bg-accent-soft px-2 py-0.5 text-xs font-medium text-accent-ink">New</span>
);

/** The address of a link as a read-only field, selected, with "Copy it from here.": for where the clipboard is out of reach. */
function ManualLink({ link, onClose }: { link: InviteLink; onClose: () => void }) {
  const field = useRef<HTMLInputElement>(null);
  useEffect(() => {
    field.current?.focus();
    field.current?.select();
  }, []);
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5 px-2 pb-2 max-sm:px-3">
      <input
        ref={field}
        type="text"
        readOnly
        aria-label="Invite link"
        value={inviteLink(link.token ?? "")}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            event.stopPropagation();
            onClose();
          }
        }}
        className="min-w-0 flex-1 basis-60 rounded-lg border border-line-strong bg-surface px-2.5 py-1.5 text-sm text-ink max-sm:text-base pointer-coarse:min-h-11"
      />
      <span className="text-[13px] text-muted">Copy it from here.</span>
    </div>
  );
}

/** A link on a wide screen or a tablet: who made it and when it ends, New, Copy and Revoke. */
function LinkRow({
  link,
  you,
  isNew,
  confirming,
  manual,
  revoke,
  onAsk,
  onCancel,
  onRevoked,
  onCopy,
  onCloseManual,
}: RowProps) {
  const { ref: copyButton, returnFocus: returnToCopy } = useReturnFocus<HTMLButtonElement>(manual);
  const { ref: revokeButton, returnFocus: returnToRevoke } = useReturnFocus<HTMLButtonElement>(confirming);
  // The link just made takes focus on its Copy, once.
  useEffect(() => {
    if (isNew) copyButton.current?.focus();
  }, [isNew, copyButton]);

  if (confirming) {
    return (
      <li className="p-1">
        <InlineConfirm
          question={REVOKE_QUESTION}
          confirmLabel="Revoke"
          pending={revoke.isPending}
          error={revoke.error}
          onConfirm={() => revoke.mutate(link.id, { onSuccess: onRevoked })}
          onCancel={() => {
            returnToRevoke();
            onCancel();
          }}
        />
      </li>
    );
  }
  return (
    <li className="flex flex-col">
      <div className="flex items-center gap-3 px-2 py-[9px]">
        <span className="min-w-0 flex-1 break-words">
          {madeBy(link, you)} · ends {link.ends_label}
        </span>
        {isNew && newBadge}
        {link.token != null && (
          <button
            ref={copyButton}
            type="button"
            onClick={() => void onCopy()}
            className={
              isNew ? `${compactButtonClass.primary} shrink-0 rounded-[7px] px-2.5 py-[5px] text-xs` : roleButtonClass
            }
          >
            Copy
          </button>
        )}
        {link.allowed_actions.includes("revoke") && (
          <button ref={revokeButton} type="button" onClick={onAsk} className={dangerTextButtonClass}>
            Revoke
          </button>
        )}
      </div>
      {manual && (
        <ManualLink
          link={link}
          onClose={() => {
            returnToCopy();
            onCloseManual();
          }}
        />
      )}
    </li>
  );
}

/** A link on a phone: who made it and when it ends, and a ⋯ whose sheet holds Copy link and Revoke link…. */
function PhoneLinkRow({
  link,
  you,
  isNew,
  confirming,
  manual,
  revoke,
  onAsk,
  onCancel,
  onRevoked,
  onCopy,
  onCloseManual,
}: RowProps) {
  const [open, setOpen] = useState(false);
  const questionId = useId();
  const { ref: revokeRow, returnFocus: returnToRevokeRow } = useReturnFocus<HTMLButtonElement>(confirming);
  const copyButton = useRef<HTMLButtonElement>(null);
  const summary = `${madeBy(link, you)} · ends ${link.ends_label}`;
  useEffect(() => {
    if (isNew) copyButton.current?.focus();
  }, [isNew]);
  // A failed copy from the visible Copy opens the sheet, which holds the address to copy by hand.
  const showSheet = open || manual;
  const close = () => {
    setOpen(false);
    onCloseManual();
    if (confirming) onCancel();
  };

  return (
    <li className="flex min-h-14 items-center gap-2.5 py-1 pr-1.5 pl-3.5">
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="font-medium break-words">{madeBy(link, you)}</span>
        <span className="text-[13px] text-muted">Ends {link.ends_label}</span>
      </div>
      {isNew && newBadge}
      {isNew && link.token != null && (
        <button
          ref={copyButton}
          type="button"
          onClick={() => void onCopy()}
          className={`${compactButtonClass.primary} shrink-0 text-[13px]`}
        >
          Copy
        </button>
      )}
      <button
        type="button"
        aria-label={`Actions for the link ${summary}`}
        onClick={() => setOpen(true)}
        className={moreButtonClass}
      >
        <Ellipsis aria-hidden="true" className="size-4" />
      </button>
      <Sheet
        title="Invite link"
        isOpen={showSheet}
        onClose={close}
        showTitle={false}
        role={confirming ? "alertdialog" : "dialog"}
        describedBy={confirming ? questionId : undefined}
      >
        <p className="px-3 pt-2.5 pb-1.5 text-[13px] break-words text-muted">{summary}</p>
        {confirming ? (
          <div className="flex flex-col gap-2 px-1 pb-1">
            <p id={questionId} className="px-2 pb-1 text-[15px] text-ink">
              {REVOKE_QUESTION}
            </p>
            <button
              type="button"
              autoFocus
              disabled={revoke.isPending}
              onClick={() => revoke.mutate(link.id, { onSuccess: onRevoked })}
              className="min-h-13 rounded-xl bg-overdue-ink text-[15px] font-semibold text-surface disabled:opacity-40"
            >
              Revoke
            </button>
            <button
              type="button"
              onClick={() => {
                // The confirm replaced the row that asked; focus goes back to it.
                returnToRevokeRow();
                onCancel();
              }}
              className="min-h-13 rounded-xl border border-line text-[15px] text-ink"
            >
              Cancel
            </button>
            <ProblemText error={revoke.error} className="px-2 text-overdue-ink" />
          </div>
        ) : (
          <div className="flex flex-col">
            {link.token != null && (
              <button
                type="button"
                onClick={() =>
                  void onCopy().then((ok) => {
                    if (ok) close();
                  })
                }
                className={sheetRowClass}
              >
                Copy link
              </button>
            )}
            {manual && <ManualLink link={link} onClose={onCloseManual} />}
            {link.allowed_actions.includes("revoke") && (
              <button
                ref={revokeRow}
                type="button"
                onClick={onAsk}
                className={`${sheetRowClass} rounded-t-none border-t border-line text-overdue-ink`}
              >
                Revoke link…
              </button>
            )}
          </div>
        )}
      </Sheet>
    </li>
  );
}
