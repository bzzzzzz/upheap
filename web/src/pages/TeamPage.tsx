import { Link } from "@tanstack/react-router";
import { useState, type FormEvent } from "react";

import type { Heap, Team, TeamMember } from "../api/client";
import {
  useAddMember,
  useCreateHeap,
  isUserQuery,
  useFindUsers,
  useLeaveTeam,
  useRemoveMember,
  useSetHeapArchived,
  useSetMemberRole,
  useTeam,
  useUpdateHeap,
  useUpdateTeam,
} from "../api/queries";
import { CopyLink } from "../components/CopyLink";
import { inputClass, NameForm, primaryButtonClass, quietButtonClass } from "../components/NameForm";
import { ProblemText } from "../components/ProblemText";
import { HistoryDisclosure } from "../components/AuditHistory";
import { useConfirmation } from "../components/useConfirmation";

const sectionTitle = "mb-2 text-sm font-semibold tracking-wide text-stone-500 uppercase";
const listClass =
  "rounded-lg border border-stone-200 bg-white px-3 dark:border-stone-800 dark:bg-stone-900 [&>li]:border-b [&>li]:border-stone-200 [&>li:last-child]:border-b-0 dark:[&>li]:border-stone-800";

/**
 * A team's heaps and members, by its slug `team`. What each person can change comes from `allowed_actions`, so a
 * personal team shows no member controls and no slug field.
 */
export function TeamPage({ team: slug }: { team: string }) {
  const team = useTeam(slug);

  if (team.isPending) {
    return <p className="py-16 text-center text-stone-500">Loading…</p>;
  }
  if (team.isError) {
    return (
      <p role="alert" className="py-16 text-center text-red-700 dark:text-red-400">
        {team.error.message}
      </p>
    );
  }

  const data = team.data;
  return (
    <main>
      <TeamName team={data} />
      {data.allowed_actions.includes("change_slug") && <TeamSlug team={data} />}
      <p className="mb-6 text-sm text-stone-500">
        {data.personal
          ? "Your personal team. It's yours alone, and its link uses your username."
          : `You are ${data.role === "admin" ? "an admin" : "a member"} of this team.`}{" "}
        <Link
          to="/teams/$team/activity"
          params={{ team: data.slug }}
          className="text-sky-700 hover:underline dark:text-sky-400"
        >
          Activity
        </Link>
        <CopyLink handles={[data.slug]} className="ml-2" />
      </p>
      <section aria-labelledby="team-heaps" className="mb-8">
        <h2 id="team-heaps" className={sectionTitle}>
          Heaps
        </h2>
        {data.heaps.length === 0 ? (
          <p className="mb-3 text-sm text-stone-500">This team has no heaps yet.</p>
        ) : (
          <ul className={listClass}>
            {data.heaps.map((heap) => (
              <HeapRow key={heap.id} team={data.slug} heap={heap} />
            ))}
          </ul>
        )}
        {data.allowed_actions.includes("create_heap") && <NewHeapForm team={data.slug} />}
      </section>
      <section aria-labelledby="team-members" className="mb-8">
        <h2 id="team-members" className={sectionTitle}>
          Members
        </h2>
        <ul className={listClass}>
          {data.members.map((member) => (
            <MemberRow key={member.user_id} team={data.slug} member={member} />
          ))}
        </ul>
        {data.allowed_actions.includes("add_member") && <AddMember team={data} />}
      </section>
      <LeaveTeam team={data} />
    </main>
  );
}

function TeamName({ team }: { team: Team }) {
  const rename = useUpdateTeam();
  if (!team.allowed_actions.includes("rename")) {
    return <h1 className="mt-6 mb-1 text-xl font-semibold tracking-tight">{team.name}</h1>;
  }
  return (
    <div className="mt-6 mb-1">
      <h1 className="sr-only">{team.name}</h1>
      <NameForm
        label="Team name"
        value={team.name}
        version={team.version}
        pending={rename.isPending}
        error={rename.error}
        onSave={(name) => rename.mutate({ team, patch: { name } })}
        className="text-lg font-semibold"
      />
    </div>
  );
}

/** The team's slug, which its links use. Links with the old one stop working. */
function TeamSlug({ team }: { team: Team }) {
  const update = useUpdateTeam();
  return (
    <div className="mb-1 flex items-center gap-2 text-sm text-stone-500">
      <span aria-hidden="true">/teams/</span>
      <NameForm
        label="Team slug"
        value={team.slug}
        version={team.version}
        maxLength={39}
        pending={update.isPending}
        error={update.error}
        onSave={(slug) => update.mutate({ team, patch: { slug: slug.trim() } })}
      />
    </div>
  );
}

function HeapRow({ team, heap }: { team: string; heap: Heap }) {
  const update = useUpdateHeap(team);
  const setArchived = useSetHeapArchived(team);
  const archived = heap.archived_at !== null;
  const can = (action: Heap["allowed_actions"][number]) => heap.allowed_actions.includes(action);

  return (
    <li className="flex flex-wrap items-center gap-2 py-2">
      {can("rename") ? (
        <>
          <NameForm
            label={`Name of heap ${heap.name}`}
            value={heap.name}
            version={heap.version}
            pending={update.isPending}
            error={update.error}
            onSave={(name) => update.mutate({ heap, patch: { name } })}
          />
          <NameForm
            label={`Slug of heap ${heap.name}`}
            value={heap.slug}
            version={heap.version}
            maxLength={39}
            pending={update.isPending}
            error={null}
            onSave={(slug) => update.mutate({ heap, patch: { slug: slug.trim() } })}
            className="max-w-40 font-mono text-xs"
          />
        </>
      ) : (
        <span className="min-w-0 flex-1 text-sm font-medium">{heap.name}</span>
      )}
      {archived && (
        <span className="rounded bg-stone-100 px-1.5 py-0.5 text-xs text-stone-600 dark:bg-stone-800 dark:text-stone-300">
          Archived
        </span>
      )}
      <Link
        to="/teams/$team/heaps/$heap"
        params={{ team, heap: heap.slug }}
        className="text-sm text-sky-700 hover:underline dark:text-sky-400"
      >
        Open
      </Link>
      {(can("archive") || can("unarchive")) && (
        <button
          type="button"
          disabled={setArchived.isPending}
          onClick={() => setArchived.mutate({ heap, archived: !archived })}
          className={quietButtonClass}
        >
          {archived ? "Unarchive" : "Archive"}
        </button>
      )}
      <ProblemText error={setArchived.error} />
      {can("view_history") && (
        <div className="basis-full">
          <HistoryDisclosure scope={{ team }} entityId={heap.id} />
        </div>
      )}
    </li>
  );
}

function NewHeapForm({ team }: { team: string }) {
  const create = useCreateHeap(team);
  const [name, setName] = useState("");

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (name.trim() === "") return;
    create.mutate(name, { onSuccess: () => setName("") });
  };

  return (
    <form onSubmit={submit} className="mt-3 flex flex-wrap items-center gap-2">
      <input
        aria-label="New heap name"
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder="New heap name"
        maxLength={100}
        className={`${inputClass} w-56`}
      />
      <button type="submit" disabled={create.isPending || name.trim() === ""} className={primaryButtonClass}>
        Add heap
      </button>
      <ProblemText error={create.error} />
    </form>
  );
}

function MemberRow({ team, member }: { team: string; member: TeamMember }) {
  const setRole = useSetMemberRole(team);
  const remove = useRemoveMember(team);
  const { confirming, ask, cancel, buttonRef: removeButton } = useConfirmation();
  const can = (action: TeamMember["allowed_actions"][number]) => member.allowed_actions.includes(action);

  return (
    <li className="flex flex-wrap items-center gap-x-3 gap-y-1 py-2">
      <span className="min-w-0 flex-1">
        <span className="text-sm font-medium">{member.display_name}</span>{" "}
        <span className="text-sm text-stone-500">@{member.username}</span>{" "}
        <span className="text-sm text-stone-500">{member.email}</span>
      </span>
      <span className="text-xs text-stone-500">{member.role === "admin" ? "Admin" : "Member"}</span>
      {can("promote") && (
        <button
          type="button"
          disabled={setRole.isPending}
          onClick={() => setRole.mutate({ username: member.username, role: "admin" })}
          className={quietButtonClass}
        >
          Make admin
        </button>
      )}
      {can("demote") && (
        <button
          type="button"
          disabled={setRole.isPending}
          onClick={() => setRole.mutate({ username: member.username, role: "member" })}
          className={quietButtonClass}
        >
          Make member
        </button>
      )}
      {can("remove") && !confirming && (
        <button ref={removeButton} type="button" onClick={ask} className={quietButtonClass}>
          Remove
        </button>
      )}
      {confirming && (
        <span className="flex items-center gap-1 text-sm">
          Remove {member.display_name}?
          <button
            type="button"
            autoFocus
            disabled={remove.isPending}
            onClick={() => remove.mutate(member.username, { onError: cancel })}
            className="rounded px-1.5 py-1 font-medium text-red-700 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950"
          >
            Remove
          </button>
          <button type="button" onClick={cancel} className={quietButtonClass}>
            Cancel
          </button>
        </span>
      )}
      <ProblemText error={setRole.error ?? remove.error} />
      <div className="basis-full">
        <HistoryDisclosure scope={{ team }} entityId={member.user_id} />
      </div>
    </li>
  );
}

/**
 * Finds users by the start of a name, or of an email up to the `@` and part of the domain, and adds them as members.
 * The server needs at least 3 characters or a whole username, and shows an email only when it was typed.
 */
function AddMember({ team }: { team: Team }) {
  const [query, setQuery] = useState("");
  const found = useFindUsers(query);
  const add = useAddMember(team.slug);
  const memberIds = new Set(team.members.map((m) => m.user_id));
  const searching = isUserQuery(query);

  return (
    <div className="mt-3">
      <input
        type="search"
        aria-label="Find a user to add by username, name or email"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        placeholder="Add a member: username, name, or full email"
        className={`${inputClass} w-72 max-w-full`}
      />
      {searching && found.data && (
        <ul aria-label="Matching users" className="mt-2 flex flex-col gap-1">
          {found.data.users.length === 0 && <li className="text-sm text-stone-500">No one matches.</li>}
          {found.data.users.map((user) => (
            <li key={user.id} className="flex items-center gap-2 text-sm">
              <span className="font-medium">{user.display_name}</span>
              <span className="text-stone-500">@{user.username}</span>
              {user.email && <span className="text-stone-500">{user.email}</span>}
              {memberIds.has(user.id) ? (
                <span className="text-xs text-stone-500">Already a member</span>
              ) : (
                <button
                  type="button"
                  disabled={add.isPending}
                  onClick={() => add.mutate(user.id, { onSuccess: () => setQuery("") })}
                  className={quietButtonClass}
                >
                  Add
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
      <ProblemText error={found.error ?? add.error} />
    </div>
  );
}

function LeaveTeam({ team }: { team: Team }) {
  const leave = useLeaveTeam(team);
  const { confirming, ask, cancel, buttonRef: leaveButton } = useConfirmation();

  if (!team.allowed_actions.includes("leave")) {
    // Only worth saying where someone else could be made an admin.
    return team.role === "admin" && team.allowed_actions.includes("add_member") ? (
      <p className="text-sm text-stone-500">You are the only admin, so you can't leave until someone else is one.</p>
    ) : null;
  }
  if (!confirming) {
    return (
      <button ref={leaveButton} type="button" onClick={ask} className={quietButtonClass}>
        Leave team
      </button>
    );
  }
  return (
    <div className="flex flex-wrap items-center gap-2 text-sm">
      Leave {team.name}? You'll lose access to its heaps.
      <button
        type="button"
        autoFocus
        disabled={leave.isPending}
        onClick={() => leave.mutate()}
        className="rounded px-1.5 py-1 font-medium text-red-700 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950"
      >
        Leave
      </button>
      <button type="button" onClick={cancel} className={quietButtonClass}>
        Cancel
      </button>
      <ProblemText error={leave.error} />
    </div>
  );
}
