import { Link, useNavigate } from "@tanstack/react-router";
import { ChevronRight, Ellipsis, History, Plus } from "lucide-react";
import { useEffect, useId, useRef, useState, type FormEvent, type KeyboardEvent, type Ref } from "react";
import { Button, Menu, MenuTrigger } from "react-aria-components";

import { ApiProblem, type Heap, type Team, type TeamMember, type UserMatch } from "../api/client";
import {
  isUserQuery,
  soleTeam,
  useAddMember,
  useCreateHeap,
  useFindUsers,
  useLeaveTeam,
  useMe,
  useRemoveMember,
  useSetMemberRole,
  useTeam,
  useUpdateHeap,
  useUpdateTeam,
} from "../api/queries";
import { useArchiveHeap } from "../components/archiveHeap";
import { AuditHistory } from "../components/AuditHistory";
import { compactButtonClass } from "../components/buttons";
import { Field } from "../components/Field";
import { InlineConfirm } from "../components/InlineConfirm";
import { PHONE, useMedia } from "../components/media";
import { MenuEntry, MenuPopover } from "../components/Menu";
import { NameForm, type NameFormClasses } from "../components/NameForm";
import { ProblemText, problemMessage } from "../components/ProblemText";
import { Sheet, sheetRowClass } from "../components/Sheet";
import { useConfirmation, useReturnFocus } from "../components/useConfirmation";
import { namedForOwner, useTeamName } from "../components/teamName";
import { listOf } from "../components/words";
import { canManageInviteLinks, InviteLinkButton, InviteLinks, useMakeInviteLink } from "./InviteLinks";
import { cardClass, dangerTextButtonClass, moreButtonClass, roleButtonClass, sectionTitleClass } from "./teamStyles";

/** Name and link name forms in the design tokens, as on Account. */
const nameFormClasses: NameFormClasses = {
  input: "rounded-lg border border-line-strong bg-surface px-2.5 py-1.5 text-sm text-ink pointer-coarse:min-h-11",
  save: compactButtonClass.primary,
  cancel: compactButtonClass.quiet,
  problem: "text-overdue-ink",
};

/**
 * A team's heaps and members, by its slug `team` (S-team: WideTeam, PhoneTeam). What each person can change comes from
 * `allowed_actions`, so a personal team shows no member controls and no link name.
 */
export function TeamPage({ team: slug }: { team: string }) {
  const team = useTeam(slug);
  const phone = useMedia(PHONE);

  if (team.isPending) {
    return <p className="py-16 text-center text-muted">Loading…</p>;
  }
  if (team.isError) {
    return (
      <p role="alert" className="py-16 text-center text-overdue-ink">
        {team.error.message}
      </p>
    );
  }

  const data = team.data;
  return (
    <main className="flex flex-col gap-7 pt-6 text-sm max-sm:gap-6 max-sm:pt-2 max-sm:text-[15px]">
      <TeamHeader team={data} />
      <HeapsSection team={data} phone={phone} />
      <MembersSection team={data} phone={phone} />
      <LeaveTeam team={data} />
    </main>
  );
}

type TeamEdit = "name" | "slug";

/**
 * "Teams · you're an admin", the name and what a team is. Rename, Link name and Activity are rarer, so they
 * sit under the ⋯ by the name; the name and link name are edited in place when chosen.
 */
function TeamHeader({ team }: { team: Team }) {
  const { data: me } = useMe();
  const label = useTeamName(team);
  const update = useUpdateTeam();
  const navigate = useNavigate();
  const [editing, setEditing] = useState<TeamEdit | null>(null);
  const can = (action: Team["allowed_actions"][number]) => team.allowed_actions.includes(action);
  // In the local build there's no Teams list to go back to: `/teams` leads here.
  const listed = me !== undefined && soleTeam(me) === undefined;
  const role = team.personal ? "your personal team" : team.role === "admin" ? "you're an admin" : "you're a member";
  // After Save, Cancel or Escape, focus goes back to the ⋯ the editor came from.
  const { ref: moreButton, returnFocus } = useReturnFocus<HTMLButtonElement>(editing !== null);
  const close = () => {
    returnFocus();
    setEditing(null);
    update.reset();
  };

  const act = (key: string | number) => {
    if (key === "rename" || key === "slug") setEditing(key === "rename" ? "name" : "slug");
    if (key === "activity") void navigate({ to: "/teams/$team/activity", params: { team: team.slug } });
  };

  return (
    <div className="flex flex-col gap-0.5">
      <p className="text-xs text-muted">
        {listed ? (
          <>
            <Link to="/teams" className="text-ink-soft hover:text-ink hover:underline">
              Teams
            </Link>
            {` · ${role}`}
          </>
        ) : (
          role.charAt(0).toUpperCase() + role.slice(1)
        )}
      </p>
      <div className="flex min-h-9 items-center gap-1.5">
        {editing === "name" ? (
          <>
            <h1 className="sr-only">{team.name}</h1>
            <NameForm
              label="Team name"
              value={team.name}
              pending={update.isPending}
              error={update.error}
              onSave={(name) => update.mutate({ team, patch: { name } }, { onSuccess: close })}
              onCancel={close}
              autoFocus
              classes={nameFormClasses}
              className="text-base font-semibold"
            />
          </>
        ) : (
          <h1 className="min-w-0 text-[22px] font-semibold tracking-[-0.015em] break-words">{label}</h1>
        )}
        {editing === null && (
          <MenuTrigger>
            <Button ref={moreButton} aria-label={`More for ${label}`} className={moreButtonClass}>
              <Ellipsis aria-hidden="true" className="size-4" />
            </Button>
            <MenuPopover placement="bottom start">
              <Menu aria-label={`More for ${label}`} className="outline-none" onAction={act}>
                {/* A team shown as "<name>'s team" whatever it is stored as: renaming it would change nothing. */}
                {can("rename") && !(me && namedForOwner(team, me)) ? <MenuEntry id="rename">Rename</MenuEntry> : null}
                {can("change_slug") ? <MenuEntry id="slug">Link name</MenuEntry> : null}
                <MenuEntry id="activity">Activity</MenuEntry>
              </Menu>
            </MenuPopover>
          </MenuTrigger>
        )}
      </div>
      {editing === "slug" && (
        <div className="flex items-center gap-1.5 text-sm text-muted">
          <span aria-hidden="true">/teams/</span>
          <NameForm
            label="Team link name"
            value={team.slug}
            maxLength={39}
            pending={update.isPending}
            error={update.error}
            onSave={(slug) => update.mutate({ team, patch: { slug: slug.trim() } }, { onSuccess: close })}
            onCancel={close}
            autoFocus
            classes={nameFormClasses}
          />
        </div>
      )}
      <p className="mt-1 text-[13px] text-ink-soft max-sm:text-sm">
        {team.personal
          ? "It's yours alone, and its link uses your username."
          : "Everyone here can see and edit every heap in the team. Admins manage heaps and members."}
      </p>
    </div>
  );
}

function HeapsSection({ team, phone }: { team: Team; phone: boolean }) {
  const [adding, setAdding] = useState(false);
  const { ref: newButton, returnFocus } = useReturnFocus<HTMLButtonElement>(adding);
  const close = () => {
    returnFocus();
    setAdding(false);
  };

  return (
    <section aria-labelledby="team-heaps" className="flex flex-col gap-2 max-sm:gap-2.5">
      <div className="flex items-center gap-3">
        <h2 id="team-heaps" className={sectionTitleClass}>
          Heaps
        </h2>
        {team.allowed_actions.includes("create_heap") && (
          <button
            ref={newButton}
            type="button"
            aria-expanded={adding}
            onClick={() => setAdding(true)}
            className={`${compactButtonClass.primary} py-1.5 text-[13px] max-sm:min-h-10 max-sm:rounded-[10px] max-sm:text-sm`}
          >
            + New heap
          </button>
        )}
      </div>
      {adding && <NewHeapForm team={team.slug} onClose={close} />}
      {team.heaps.length === 0 ? (
        <p className="text-sm text-muted">This team has no heaps yet.</p>
      ) : (
        <ul className={cardClass}>
          {team.heaps.map((heap) => (
            <HeapRow key={heap.id} team={team.slug} heap={heap} phone={phone} />
          ))}
        </ul>
      )}
    </section>
  );
}

/** A name for a new heap. The server makes its link name from the name. */
function NewHeapForm({ team, onClose }: { team: string; onClose: () => void }) {
  const create = useCreateHeap(team);
  const [name, setName] = useState("");
  const { error } = create;
  const nameError = error instanceof ApiProblem && error.field === "name";

  const submit = (event: FormEvent) => {
    event.preventDefault();
    create.mutate(name, { onSuccess: onClose });
  };

  return (
    <form
      aria-label="New heap"
      onSubmit={submit}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          onClose();
        }
      }}
      noValidate
      className="flex flex-col gap-2.5 rounded-xl border border-line bg-surface p-3.5 focus-within:border-accent focus-within:ring-[3px] focus-within:ring-accent-soft"
    >
      <Field
        label="New heap name"
        name="name"
        value={name}
        error={nameError && error ? problemMessage(error) : undefined}
        onChange={(value) => {
          setName(value);
          if (nameError) create.reset();
        }}
        autoFocus
        autoComplete="off"
        maxLength={100}
      />
      {error && !nameError && <ProblemText error={error} className="text-overdue-ink" />}
      <div className="flex items-center justify-end gap-2.5">
        <button type="button" onClick={onClose} className={`${compactButtonClass.quiet} max-sm:min-h-11`}>
          Cancel
        </button>
        <button type="submit" disabled={create.isPending} className={`${compactButtonClass.primary} max-sm:min-h-11`}>
          Add heap
        </button>
      </div>
    </form>
  );
}

type HeapAction = "rename" | "slug" | "archive" | "history";

/**
 * A heap: its name, "14 open", Open, and its actions under ⋯ (a sheet on a phone, where the whole row opens the heap).
 * Rename and Link name edit the row in place.
 */
function HeapRow({ team, heap, phone }: { team: string; heap: Heap; phone: boolean }) {
  const update = useUpdateHeap(team);
  const { archive, unarchive, mutation: setArchived } = useArchiveHeap(team);
  const [editing, setEditing] = useState<TeamEdit | null>(null);
  const [history, setHistory] = useState(false);
  // After Save, Cancel or Escape, focus goes back to the ⋯ the editor came from.
  const { ref: moreButton, returnFocus } = useReturnFocus<HTMLButtonElement>(editing !== null);
  const archived = heap.archived_at !== null;
  const can = (action: Heap["allowed_actions"][number]) => heap.allowed_actions.includes(action);
  const close = () => {
    returnFocus();
    setEditing(null);
    update.reset();
  };

  const actions: { id: HeapAction; label: string }[] = [
    ...(can("rename") ? [{ id: "rename", label: "Rename" } as const, { id: "slug", label: "Link name" } as const] : []),
    ...(can(archived ? "unarchive" : "archive")
      ? [{ id: "archive", label: archived ? "Unarchive" : "Archive" } as const]
      : []),
    ...(can("view_history") ? [{ id: "history", label: history ? "Hide history" : "History" } as const] : []),
  ];
  const act = (id: HeapAction) => {
    if (id === "rename") setEditing("name");
    if (id === "slug") setEditing("slug");
    if (id === "archive") (archived ? unarchive : archive)(heap);
    if (id === "history") setHistory((open) => !open);
  };

  const badge = archived && (
    <span className="shrink-0 rounded-md bg-sunken px-2 py-0.5 text-xs text-ink-soft">Archived</span>
  );
  const params = { team, heap: heap.slug };
  let main;
  if (editing) {
    main = (
      <div className="flex min-w-0 flex-1 items-center gap-1.5 py-1 text-sm text-muted max-sm:px-3">
        {editing === "slug" && <span aria-hidden="true">/heaps/</span>}
        <NameForm
          label={editing === "name" ? `Name of heap ${heap.name}` : `Link name of heap ${heap.name}`}
          value={editing === "name" ? heap.name : heap.slug}
          maxLength={editing === "name" ? 100 : 39}
          pending={update.isPending}
          error={update.error}
          onSave={(value) =>
            update.mutate(
              { heap, patch: editing === "name" ? { name: value } : { slug: value.trim() } },
              { onSuccess: close },
            )
          }
          onCancel={close}
          autoFocus
          classes={nameFormClasses}
        />
      </div>
    );
  } else if (phone) {
    main = (
      <Link
        to="/teams/$team/heaps/$heap"
        params={params}
        className="flex min-h-13 min-w-0 flex-1 items-center gap-3 rounded-[14px] pl-3.5 outline-none focus-visible:outline-2 focus-visible:outline-accent"
      >
        <span className="min-w-0 flex-1 font-medium break-words">{heap.name}</span>
        {badge}
        <span className="flex shrink-0 items-center text-[13px] text-muted">
          {heap.open_count} open
          <ChevronRight aria-hidden="true" className="size-4" />
        </span>
      </Link>
    );
  } else {
    main = (
      <>
        <span className="min-w-0 flex-1 font-medium break-words">{heap.name}</span>
        {badge}
        <span className="shrink-0 text-[13px] text-muted">{heap.open_count} open</span>
        <Link
          to="/teams/$team/heaps/$heap"
          params={params}
          aria-label={`Open ${heap.name}`}
          className="shrink-0 rounded-lg px-2 py-1 text-[13px] text-ink-soft hover:bg-sunken hover:text-ink pointer-coarse:min-h-11 pointer-coarse:content-center"
        >
          Open
        </Link>
      </>
    );
  }

  return (
    <li className="flex flex-col">
      <div className="flex items-center gap-3 px-2 py-2.5 max-sm:gap-1 max-sm:p-0 max-sm:pr-1">
        {main}
        {actions.length > 0 && !editing && (
          <HeapActions heap={heap} actions={actions} buttonRef={moreButton} onAction={act} />
        )}
      </div>
      <ProblemText error={setArchived.error} className="px-2 pb-2 text-overdue-ink max-sm:px-3.5" />
      {history && (
        <div className="px-2 pb-2 max-sm:px-3.5">
          <AuditHistory scope={{ team }} entityId={heap.id} />
        </div>
      )}
    </li>
  );
}

/**
 * The heap's ⋯: a menu, which on a phone is a sheet from the bottom (`MenuPopover`), as the team's ⋯ is. A touch opens
 * it on release, so lifting the finger never picks an entry.
 */
function HeapActions({
  heap,
  actions,
  buttonRef,
  onAction,
}: {
  heap: Heap;
  actions: { id: HeapAction; label: string }[];
  buttonRef: Ref<HTMLButtonElement>;
  onAction: (id: HeapAction) => void;
}) {
  const label = `Actions for ${heap.name}`;
  return (
    <MenuTrigger>
      <Button ref={buttonRef} aria-label={label} className={moreButtonClass}>
        <Ellipsis aria-hidden="true" className="size-4" />
      </Button>
      <MenuPopover placement="bottom end">
        <Menu aria-label={label} className="outline-none" onAction={(key) => onAction(key as HeapAction)}>
          {actions.map(({ id, label }) => (
            <MenuEntry key={id} id={id}>
              {label}
            </MenuEntry>
          ))}
        </Menu>
      </MenuPopover>
    </MenuTrigger>
  );
}

/**
 * "Remove Sam Ortiz from Acme? They lose access to Launch and Support. Their items stay." Only the heaps still in use
 * are named, or counted when there are many. Removing a member deletes only their membership.
 */
function removeQuestion(member: TeamMember, team: Team): string {
  const heaps = team.heaps.filter((heap) => heap.archived_at === null).map((heap) => heap.name);
  const access =
    heaps.length === 0 ? "" : ` They lose access to ${heaps.length > 3 ? `its ${heaps.length} heaps` : listOf(heaps)}.`;
  return `Remove ${member.display_name} from ${team.name}?${access} Their items stay.`;
}

/** "Give up your admin role in Acme? …": the question before an admin makes themselves a member. */
function demoteQuestion(team: Team): string {
  return `Give up your admin role in ${team.name}? You'll no longer manage its members and heaps, and only another admin can make you an admin again.`;
}

/** What the confirm under the member list is asking: to remove `member`, or for you to give up your admin role. */
type Asking = { kind: "remove" | "demote"; member: TeamMember };

function MembersSection({ team, phone }: { team: Team; phone: boolean }) {
  const { data: me } = useMe();
  const remove = useRemoveMember(team.slug);
  const setRole = useSetMemberRole(team.slug);
  const makingLink = useMakeInviteLink(team.slug);
  const canAddMember = team.allowed_actions.includes("add_member");
  const canInvite = canManageInviteLinks(team);
  const [asking, setAsking] = useState<Asking | null>(null);
  const askButtons = useRef(new Map<string, HTMLButtonElement>());
  const heading = useRef<HTMLHeadingElement>(null);
  // Who to give focus back to once the confirm is gone: the button it came from.
  const refocus = useRef<string | null>(null);
  useEffect(() => {
    if (asking || refocus.current === null) return;
    askButtons.current.get(refocus.current)?.focus();
    refocus.current = null;
  }, [asking]);
  // A demote question goes away, without moving focus, once your row no longer offers it (another admin left), unless
  // it holds the error of a refused demote, which would go with it.
  const demoteOffered =
    asking?.kind !== "demote" ||
    team.members.some((m) => m.user_id === asking.member.user_id && m.allowed_actions.includes("demote"));
  const dropAsking = !demoteOffered && !setRole.error;
  if (dropAsking) setAsking(null);

  const buttonKey = (kind: Asking["kind"], member: TeamMember) => `${kind}:${member.user_id}`;
  const registerButton = (kind: Asking["kind"], member: TeamMember) => (button: HTMLButtonElement | null) => {
    if (button) askButtons.current.set(buttonKey(kind, member), button);
    else askButtons.current.delete(buttonKey(kind, member));
  };
  const ask = (kind: Asking["kind"], member: TeamMember) => {
    remove.reset();
    setRole.reset();
    setAsking({ kind, member });
  };
  const cancel = () => {
    refocus.current = asking ? buttonKey(asking.kind, asking.member) : null;
    setAsking(null);
    remove.reset();
    setRole.reset();
  };
  const confirmRemove = (member: TeamMember, onDone?: () => void) =>
    remove.mutate(member.username, {
      onSuccess: () => {
        setAsking(null);
        onDone?.();
        // Their row is gone, so focus goes to the section.
        heading.current?.focus();
      },
    });
  const confirmDemote = (member: TeamMember) =>
    setRole.mutate(
      { username: member.username, role: "member" },
      {
        onSuccess: () => {
          setAsking(null);
          // The button that asked is gone, so focus goes to the section.
          heading.current?.focus();
        },
      },
    );
  const current = asking && !dropAsking ? asking : null;

  return (
    <section aria-labelledby="team-members" className="flex flex-col gap-2 max-sm:gap-2.5">
      <h2 ref={heading} tabIndex={-1} id="team-members" className={sectionTitleClass}>
        Members · {team.members.length}
      </h2>
      {(canAddMember || canInvite) && (
        <div className={phone ? "flex flex-col gap-2.5" : "flex items-start gap-2"}>
          {canAddMember && (
            <div className="min-w-0 flex-1">
              <AddMember team={team} phone={phone} />
            </div>
          )}
          {canInvite && <InviteLinkButton making={makingLink} phone={phone} />}
        </div>
      )}
      <ProblemText error={makingLink.error} className="text-overdue-ink" />
      <InviteLinks team={team} making={makingLink} phone={phone} />
      <ul className={cardClass}>
        {team.members.map((member) => {
          const you = member.user_id === me?.user.id;
          return phone ? (
            <PhoneMemberRow
              key={member.user_id}
              team={team}
              member={member}
              you={you}
              removing={remove}
              onRemove={(onDone) => confirmRemove(member, onDone)}
            />
          ) : (
            <MemberRow
              key={member.user_id}
              team={team}
              member={member}
              you={you}
              asking={current?.member.user_id === member.user_id ? current.kind : null}
              onAskRemove={() => ask("remove", member)}
              onAskDemote={() => ask("demote", member)}
              removeRef={registerButton("remove", member)}
              demoteRef={registerButton("demote", member)}
            />
          );
        })}
      </ul>
      {!phone && current && (
        <InlineConfirm
          question={current.kind === "remove" ? removeQuestion(current.member, team) : demoteQuestion(team)}
          confirmLabel={current.kind === "remove" ? "Remove" : "Give up admin"}
          pending={current.kind === "remove" ? remove.isPending : setRole.isPending}
          error={current.kind === "remove" ? remove.error : setRole.error}
          onConfirm={() => (current.kind === "remove" ? confirmRemove(current.member) : confirmDemote(current.member))}
          onCancel={cancel}
        />
      )}
    </section>
  );
}

/** Initials in a coloured circle, the colour picked by the user so it stays the same. Decorative. */
function Avatar({ member }: { member: TeamMember }) {
  const words = member.display_name.trim().split(/\s+/);
  const initials = [words[0], words.length > 1 ? words.at(-1) : undefined]
    .map((word) => Array.from(word ?? "")[0] ?? "")
    .join("")
    .toUpperCase();
  const hue = [...member.user_id].reduce((sum, char) => sum + char.charCodeAt(0), 0) % AVATAR_CLASSES.length;
  return (
    <span
      aria-hidden="true"
      className={`flex size-[30px] shrink-0 items-center justify-center rounded-full text-xs font-semibold ${AVATAR_CLASSES[hue]}`}
    >
      {initials}
    </span>
  );
}

const AVATAR_CLASSES = [
  "bg-avatar-1 text-avatar-1-ink",
  "bg-avatar-2 text-avatar-2-ink",
  "bg-avatar-3 text-avatar-3-ink",
  "bg-avatar-4 text-avatar-4-ink",
  "bg-avatar-5 text-avatar-5-ink",
];

function RoleBadge({ role }: { role: TeamMember["role"] }) {
  return role === "admin" ? (
    <span className="shrink-0 rounded-md bg-accent-soft px-2 py-0.5 text-xs font-medium text-accent-ink">Admin</span>
  ) : (
    <span className="shrink-0 rounded-md bg-sunken px-2 py-0.5 text-xs text-ink-soft">Member</span>
  );
}

const memberName = (member: TeamMember, you: boolean) => `${member.display_name}${you ? " (you)" : ""}`;

/**
 * A member on a wide screen: avatar, name and @username, role, Make admin or Make member, Remove, and History. Remove
 * asks first, and so does Make member on your own row (ux.md, Rules §1), in the confirm under the list; `asking` says
 * which of the two is open for this row. Any other role change acts at once.
 */
function MemberRow({
  team,
  member,
  you,
  asking,
  onAskRemove,
  onAskDemote,
  removeRef,
  demoteRef,
}: {
  team: Team;
  member: TeamMember;
  you: boolean;
  asking: Asking["kind"] | null;
  onAskRemove: () => void;
  onAskDemote: () => void;
  removeRef: Ref<HTMLButtonElement>;
  demoteRef: Ref<HTMLButtonElement>;
}) {
  const setRole = useSetMemberRole(team.slug);
  const [history, setHistory] = useState(false);
  const can = (action: TeamMember["allowed_actions"][number]) => member.allowed_actions.includes(action);
  const roleChange = can("promote") ? "admin" : can("demote") ? "member" : null;

  return (
    <li className="flex flex-col">
      <div className="flex items-center gap-3 px-2 py-[9px]">
        <Avatar member={member} />
        <div className="flex min-w-0 flex-1 flex-col">
          <span className="font-medium break-words">{memberName(member, you)}</span>
          <span className="text-xs break-words text-muted">@{member.username}</span>
        </div>
        <RoleBadge role={member.role} />
        {roleChange && (
          <button
            type="button"
            ref={roleChange === "member" && you ? demoteRef : undefined}
            disabled={setRole.isPending}
            aria-expanded={roleChange === "member" && you ? asking === "demote" : undefined}
            onClick={() =>
              roleChange === "member" && you
                ? onAskDemote()
                : setRole.mutate({ username: member.username, role: roleChange })
            }
            className={roleButtonClass}
          >
            {roleChange === "admin" ? "Make admin" : "Make member"}
          </button>
        )}
        {can("remove") && (
          <button
            ref={removeRef}
            type="button"
            aria-expanded={asking === "remove"}
            onClick={onAskRemove}
            className={dangerTextButtonClass}
          >
            Remove
          </button>
        )}
        <button
          type="button"
          aria-label={`History of ${member.display_name}`}
          aria-expanded={history}
          onClick={() => setHistory((open) => !open)}
          className={`${moreButtonClass} text-muted`}
        >
          <History aria-hidden="true" className="size-4" />
        </button>
      </div>
      <ProblemText error={setRole.error} className="px-2 pb-2 text-overdue-ink" />
      {history && (
        <div className="px-2 pb-2">
          <AuditHistory scope={{ team: team.slug }} entityId={member.user_id} />
        </div>
      )}
    </li>
  );
}

/**
 * A member on a phone (PhoneTeam): name and role, and a ⋯ whose sheet holds Make admin or Make member, History, and
 * Remove, which asks in the sheet. So does Make member on your own row (ux.md, Rules §1), shown as "Make member…".
 */
function PhoneMemberRow({
  team,
  member,
  you,
  removing,
  onRemove,
}: {
  team: Team;
  member: TeamMember;
  you: boolean;
  removing: { isPending: boolean; error: Error | null; reset: () => void };
  onRemove: (onDone: () => void) => void;
}) {
  const setRole = useSetMemberRole(team.slug);
  const [open, setOpen] = useState(false);
  const [asked, setAsked] = useState<Asking["kind"] | null>(null);
  const [history, setHistory] = useState(false);
  const questionId = useId();
  const can = (action: TeamMember["allowed_actions"][number]) => member.allowed_actions.includes(action);
  const roleChange = can("promote") ? "admin" : can("demote") ? "member" : null;
  // Giving up your own admin role asks first.
  const asksFirst = roleChange === "member" && you;
  // The question goes away, without moving focus, once the row no longer offers the role change.
  // Cleared too, so it doesn't come back by itself if the row offers the change again. A refused demote's error still
  // shows, under the sheet's rows.
  if (asked === "demote" && !asksFirst) setAsked(null);
  const confirming = asked === "demote" && !asksFirst ? null : asked;
  const { ref: removeRow, returnFocus: returnToRemoveRow } = useReturnFocus<HTMLButtonElement>(confirming === "remove");
  const { ref: demoteRow, returnFocus: returnToDemoteRow } = useReturnFocus<HTMLButtonElement>(confirming === "demote");
  const roleLabel = member.role === "admin" ? "Admin" : "Member";
  const name = memberName(member, you);
  const close = () => {
    setOpen(false);
    setAsked(null);
    setHistory(false);
    removing.reset();
    setRole.reset();
  };

  return (
    <li className="flex min-h-14 items-center gap-2.5 pr-1.5 pl-3.5">
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="font-medium break-words">{name}</span>
        <span className="text-[13px] text-muted">{roleLabel}</span>
      </div>
      <button
        type="button"
        aria-label={`Actions for ${name}`}
        onClick={() => setOpen(true)}
        className={moreButtonClass}
      >
        <Ellipsis aria-hidden="true" className="size-4" />
      </button>
      <Sheet
        title={member.display_name}
        isOpen={open}
        onClose={close}
        showTitle={false}
        role={confirming ? "alertdialog" : "dialog"}
        describedBy={confirming ? questionId : undefined}
      >
        <p className="px-3 pt-2.5 pb-1.5 text-[13px] break-words text-muted">
          {member.display_name} · @{member.username} · {roleLabel}
        </p>
        {confirming ? (
          <div className="flex flex-col gap-2 px-1 pb-1">
            <p id={questionId} className="px-2 pb-1 text-[15px] text-ink">
              {confirming === "remove" ? removeQuestion(member, team) : demoteQuestion(team)}
            </p>
            <button
              type="button"
              autoFocus
              disabled={confirming === "remove" ? removing.isPending : setRole.isPending}
              onClick={() =>
                confirming === "remove"
                  ? onRemove(close)
                  : setRole.mutate({ username: member.username, role: "member" }, { onSuccess: close })
              }
              className="min-h-13 rounded-xl bg-overdue-ink text-[15px] font-semibold text-surface disabled:opacity-40"
            >
              {confirming === "remove" ? "Remove" : "Give up admin"}
            </button>
            <button
              type="button"
              onClick={() => {
                // The confirm replaced the row that asked; focus goes back to it.
                if (confirming === "remove") returnToRemoveRow();
                else returnToDemoteRow();
                setAsked(null);
                removing.reset();
                setRole.reset();
              }}
              className="min-h-13 rounded-xl border border-line text-[15px] text-ink"
            >
              Cancel
            </button>
            <ProblemText
              error={confirming === "remove" ? removing.error : setRole.error}
              className="px-2 text-overdue-ink"
            />
          </div>
        ) : (
          <div className="flex flex-col">
            {roleChange && (
              <button
                type="button"
                ref={asksFirst ? demoteRow : undefined}
                disabled={setRole.isPending}
                onClick={() =>
                  asksFirst
                    ? setAsked("demote")
                    : setRole.mutate({ username: member.username, role: roleChange }, { onSuccess: close })
                }
                className={sheetRowClass}
              >
                {roleChange === "admin" ? "Make admin" : asksFirst ? "Make member…" : "Make member"}
              </button>
            )}
            <button
              type="button"
              aria-expanded={history}
              onClick={() => setHistory((shown) => !shown)}
              className={sheetRowClass}
            >
              {history ? "Hide history" : "History"}
            </button>
            {history && (
              <div className="px-3 pb-2">
                <AuditHistory scope={{ team: team.slug }} entityId={member.user_id} />
              </div>
            )}
            {can("remove") && (
              <button
                ref={removeRow}
                type="button"
                onClick={() => setAsked("remove")}
                className={`${sheetRowClass} rounded-t-none border-t border-line text-overdue-ink`}
              >
                Remove from {team.name}…
              </button>
            )}
            <ProblemText error={setRole.error} className="px-3 text-overdue-ink" />
          </div>
        )}
      </Sheet>
    </li>
  );
}

/**
 * Finds users by the start of a name or username, or by a full email, and adds them (P-member-add). The server needs
 * at least 3 characters or a whole username, and shows an email only when it was typed. The results are a listbox
 * under the field: the first one that can be added is highlighted, the arrows move, Enter or a click adds.
 */
function AddMember({ team, phone }: { team: Team; phone: boolean }) {
  const [query, setQuery] = useState("");
  const [focused, setFocused] = useState(false);
  const [active, setActive] = useState<string | null>(null);
  const found = useFindUsers(query);
  const add = useAddMember(team.slug);
  const listId = useId();
  const hintId = useId();
  const memberIds = new Set(team.members.map((m) => m.user_id));
  const addable = (user: UserMatch) => !memberIds.has(user.id);

  const users = isUserQuery(query) ? (found.data?.users ?? []) : [];
  const shown = focused && isUserQuery(query) && found.data !== undefined;
  // The previous query's results, kept on screen while this one's are on their way. They're shown dimmed and can't be
  // added, so a fast Enter never adds someone the text no longer names.
  const stale = found.isPlaceholderData;
  const choices = stale ? [] : users.filter(addable);
  // The one Enter adds: the one the arrows or the pointer picked, else the first that can be added. A phone has no
  // highlight: its keyboard's Enter mustn't add anyone, so adding is a tap on a result.
  const highlighted = phone ? undefined : (choices.find((user) => user.id === active) ?? choices[0]);
  const optionId = (user: UserMatch) => `${listId}-${user.id}`;

  const addUser = (user: UserMatch) =>
    add.mutate(user.id, {
      onSuccess: () => {
        setQuery("");
        setActive(null);
      },
    });

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if ((event.key === "ArrowDown" || event.key === "ArrowUp") && choices.length > 0) {
      event.preventDefault();
      const at = highlighted ? choices.indexOf(highlighted) : -1;
      const next = event.key === "ArrowDown" ? Math.min(at + 1, choices.length - 1) : Math.max(at - 1, 0);
      setActive(choices[next]?.id ?? null);
    } else if (event.key === "Enter" && shown && highlighted && !add.isPending) {
      event.preventDefault();
      addUser(highlighted);
    } else if (event.key === "Escape" && query !== "") {
      event.preventDefault();
      setQuery("");
    }
  };

  const expanded = shown && users.length > 0;
  return (
    <div className="flex flex-col">
      {/* The hint sits outside the label, so it describes the field without being part of its name. */}
      <div className="flex items-center gap-2.5 rounded-[10px] border border-line-strong bg-surface px-3 py-[9px] focus-within:border-accent focus-within:ring-[3px] focus-within:ring-accent-soft max-sm:h-12 max-sm:gap-2 max-sm:rounded-xl max-sm:py-0">
        <label className="flex min-w-0 flex-1 items-center gap-2.5 max-sm:gap-2">
          <Plus aria-hidden="true" className="size-4 shrink-0 text-muted" />
          <span className="sr-only">Add a member by username, name or email</span>
          <input
            type="text"
            role="combobox"
            aria-expanded={expanded}
            aria-controls={listId}
            aria-autocomplete="list"
            aria-activedescendant={expanded && highlighted ? optionId(highlighted) : undefined}
            aria-describedby={hintId}
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              setActive(null);
              add.reset();
            }}
            onKeyDown={onKeyDown}
            onFocus={() => setFocused(true)}
            onBlur={() => setFocused(false)}
            placeholder="Username, name or email"
            autoComplete="off"
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
            enterKeyHint="search"
            className="min-w-0 flex-1 bg-transparent text-sm text-ink outline-none placeholder:text-muted focus-visible:outline-none max-sm:text-base"
          />
        </label>
        <span id={hintId} className="shrink-0 text-xs text-muted max-sm:sr-only">
          They need an account first
        </span>
      </div>
      {shown && users.length === 0 && (
        <p role="status" className="mt-1.5 px-1 text-sm text-muted">
          No one matches.
        </p>
      )}
      {/* Always there, so `aria-controls` names an element; empty while there are no results. */}
      <ul
        id={listId}
        role="listbox"
        aria-label="Matching users"
        aria-busy={stale}
        className="mt-1.5 flex flex-col aria-busy:opacity-50 rounded-[10px] border border-line bg-surface p-1 shadow-[0_12px_32px_rgb(28_25_23/0.12)] empty:hidden"
      >
        {expanded &&
          users.map((user) => {
            const canAdd = addable(user) && !stale;
            const selected = user === highlighted;
            return (
              <li
                key={user.id}
                id={optionId(user)}
                role="option"
                aria-selected={selected}
                aria-disabled={!canAdd}
                // Keeps focus in the field, which the list belongs to.
                onMouseDown={(event) => event.preventDefault()}
                onMouseEnter={() => canAdd && setActive(user.id)}
                onClick={() => canAdd && !add.isPending && addUser(user)}
                className="flex min-h-9 cursor-default items-center gap-2.5 rounded-lg p-2 text-sm aria-selected:bg-accent-soft max-sm:min-h-12"
              >
                <span className="min-w-0 flex-1 break-words">
                  <span className="font-semibold">{user.display_name}</span>{" "}
                  <span className="text-muted">@{user.username}</span>
                  {user.email && <span className="text-muted"> {user.email}</span>}
                </span>
                {!addable(user) ? (
                  <span className="shrink-0 text-xs text-muted">Already a member</span>
                ) : (
                  selected && (
                    <span className="shrink-0 text-xs text-accent-ink">
                      <span aria-hidden="true">↵ </span>Add as member
                    </span>
                  )
                )}
              </li>
            );
          })}
      </ul>
      <ProblemText error={found.error ?? add.error} className="mt-1.5 text-overdue-ink" />
    </div>
  );
}

function LeaveTeam({ team }: { team: Team }) {
  const leave = useLeaveTeam(team);
  const { confirming, ask, cancel, buttonRef: leaveButton } = useConfirmation();

  if (!team.allowed_actions.includes("leave")) {
    // Only worth saying where someone else could be made an admin.
    return team.role === "admin" && team.allowed_actions.includes("add_member") ? (
      <p className="text-[13px] text-muted max-sm:text-sm">
        You are the only admin, so you can't leave until someone else is one.
      </p>
    ) : null;
  }
  if (!confirming) {
    return (
      <div>
        <button
          ref={leaveButton}
          type="button"
          onClick={ask}
          className={`${compactButtonClass.bordered} py-1.5 text-[13px] text-overdue-ink max-sm:min-h-11`}
        >
          Leave team
        </button>
      </div>
    );
  }
  return (
    <div
      role="group"
      aria-label="Leave team"
      className="flex flex-wrap items-center gap-x-3 gap-y-2 rounded-[10px] border border-overdue-ink/25 bg-overdue-soft px-3.5 py-2.5 text-[13px]"
    >
      <p className="min-w-0 flex-1 basis-60 text-overdue-ink">Leave {team.name}? You'll lose access to its heaps.</p>
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={cancel}
          className={`${compactButtonClass.quiet} py-1.5 text-[13px] max-sm:min-h-11`}
        >
          Cancel
        </button>
        <button
          type="button"
          autoFocus
          disabled={leave.isPending}
          onClick={() => leave.mutate()}
          className={`${compactButtonClass.danger} py-1.5 text-[13px] max-sm:min-h-11`}
        >
          Leave
        </button>
      </div>
      <ProblemText error={leave.error} className="text-overdue-ink" />
    </div>
  );
}
