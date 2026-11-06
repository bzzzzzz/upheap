import { Link } from "@tanstack/react-router";
import { useState, type FormEvent } from "react";

import type { Heap, Team, TeamMember } from "../api/client";
import {
  useAddMember,
  useCreateHeap,
  useFindUsers,
  useLeaveTeam,
  useRemoveMember,
  useRenameHeap,
  useRenameTeam,
  useSetHeapArchived,
  useSetMemberRole,
  useTeam,
} from "../api/queries";
import { inputClass, NameForm, primaryButtonClass, quietButtonClass } from "../components/NameForm";
import { ProblemText } from "../components/ProblemText";
import { useConfirmation } from "../components/useConfirmation";

const sectionTitle = "mb-2 text-sm font-semibold tracking-wide text-stone-500 uppercase";
const listClass =
  "rounded-lg border border-stone-200 bg-white px-3 dark:border-stone-800 dark:bg-stone-900 [&>li]:border-b [&>li]:border-stone-200 [&>li:last-child]:border-b-0 dark:[&>li]:border-stone-800";

/** A team's heaps and members. What each person can change comes from `allowed_actions`. */
export function TeamPage({ teamId }: { teamId: string }) {
  const team = useTeam(teamId);

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
      <p className="mb-6 text-sm text-stone-500">
        You are {data.role === "admin" ? "an admin" : "a member"} of this team.
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
              <HeapRow key={heap.id} heap={heap} />
            ))}
          </ul>
        )}
        {data.allowed_actions.includes("create_heap") && <NewHeapForm teamId={teamId} />}
      </section>
      <section aria-labelledby="team-members" className="mb-8">
        <h2 id="team-members" className={sectionTitle}>
          Members
        </h2>
        <ul className={listClass}>
          {data.members.map((member) => (
            <MemberRow key={member.user_id} teamId={teamId} member={member} />
          ))}
        </ul>
        {data.allowed_actions.includes("add_member") && <AddMember team={data} />}
      </section>
      <LeaveTeam team={data} />
    </main>
  );
}

function TeamName({ team }: { team: Team }) {
  const rename = useRenameTeam(team.id);
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
        onSave={(name) => rename.mutate({ team, name })}
        className="text-lg font-semibold"
      />
    </div>
  );
}

function HeapRow({ heap }: { heap: Heap }) {
  const rename = useRenameHeap();
  const setArchived = useSetHeapArchived();
  const archived = heap.archived_at !== null;
  const can = (action: Heap["allowed_actions"][number]) => heap.allowed_actions.includes(action);

  return (
    <li className="flex flex-wrap items-center gap-2 py-2">
      {can("rename") ? (
        <NameForm
          label={`Name of heap ${heap.name}`}
          value={heap.name}
          version={heap.version}
          pending={rename.isPending}
          error={rename.error}
          onSave={(name) => rename.mutate({ heap, name })}
        />
      ) : (
        <span className="min-w-0 flex-1 text-sm font-medium">{heap.name}</span>
      )}
      {archived && (
        <span className="rounded bg-stone-100 px-1.5 py-0.5 text-xs text-stone-600 dark:bg-stone-800 dark:text-stone-300">
          Archived
        </span>
      )}
      <Link
        to="/heaps/$heapId"
        params={{ heapId: heap.id }}
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
    </li>
  );
}

function NewHeapForm({ teamId }: { teamId: string }) {
  const create = useCreateHeap(teamId);
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

function MemberRow({ teamId, member }: { teamId: string; member: TeamMember }) {
  const setRole = useSetMemberRole(teamId);
  const remove = useRemoveMember(teamId);
  const { confirming, ask, cancel, buttonRef: removeButton } = useConfirmation();
  const can = (action: TeamMember["allowed_actions"][number]) => member.allowed_actions.includes(action);

  return (
    <li className="flex flex-wrap items-center gap-x-3 gap-y-1 py-2">
      <span className="min-w-0 flex-1">
        <span className="text-sm font-medium">{member.display_name}</span>{" "}
        <span className="text-sm text-stone-500">{member.email}</span>
      </span>
      <span className="text-xs text-stone-500">{member.role === "admin" ? "Admin" : "Member"}</span>
      {can("promote") && (
        <button
          type="button"
          disabled={setRole.isPending}
          onClick={() => setRole.mutate({ userId: member.user_id, role: "admin" })}
          className={quietButtonClass}
        >
          Make admin
        </button>
      )}
      {can("demote") && (
        <button
          type="button"
          disabled={setRole.isPending}
          onClick={() => setRole.mutate({ userId: member.user_id, role: "member" })}
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
            onClick={() => remove.mutate(member.user_id, { onError: cancel })}
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
    </li>
  );
}

/**
 * Finds users by the start of a name, or of an email up to the `@` and part of the domain, and adds them as members.
 * The server needs at least 3 characters, and shows an email only when it was typed.
 */
function AddMember({ team }: { team: Team }) {
  const [query, setQuery] = useState("");
  const found = useFindUsers(query);
  const add = useAddMember(team.id);
  const memberIds = new Set(team.members.map((m) => m.user_id));
  const searching = [...query.trim()].length >= 3;

  return (
    <div className="mt-3">
      <input
        type="search"
        aria-label="Find a user to add by name or email"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        placeholder="Add a member: name, or full email"
        className={`${inputClass} w-72 max-w-full`}
      />
      {searching && found.data && (
        <ul aria-label="Matching users" className="mt-2 flex flex-col gap-1">
          {found.data.users.length === 0 && <li className="text-sm text-stone-500">No one matches.</li>}
          {found.data.users.map((user) => (
            <li key={user.id} className="flex items-center gap-2 text-sm">
              <span className="font-medium">{user.display_name}</span>
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
    return team.role === "admin" ? (
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
