import { Link, useNavigate } from "@tanstack/react-router";
import { useState, type FormEvent } from "react";

import type { Me } from "../api/schema";
import { useMe } from "../api/queries/me";
import { useCreateTeam } from "../api/queries/teams";
import { compactButtonClass } from "../components/buttons";
import { Field } from "../components/Field";
import { ProblemText } from "../components/ProblemText";
import { useReturnFocus } from "../components/useConfirmation";
import { teamName } from "../components/teamName";
import { useNameAndLinkFields } from "../components/useNameAndLinkFields";
import { plural } from "../components/words";
import { Saving } from "../components/Saving";

/** The caller's teams, and Create team, which opens a form for a new one (S-teams, WideTeams). */
export function TeamsPage() {
  const { data: me } = useMe();
  const [creating, setCreating] = useState(false);
  // Focus was in the form; it goes back to what opened it.
  const { ref: createButton, returnFocus } = useReturnFocus<HTMLButtonElement>(creating);
  if (!me) return null;

  const close = () => {
    returnFocus();
    setCreating(false);
  };

  return (
    <main className="flex flex-col gap-3.5 text-sm max-sm:text-[15px]">
      <div className="mt-6 flex items-end gap-3">
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <h1 className="text-[22px] font-semibold tracking-[-0.015em]">Teams</h1>
          <p className="text-[13px] text-ink-soft max-sm:text-sm">A team shares its heaps with everyone in it.</p>
        </div>
        {me.allowed_actions.includes("create_team") && (
          <button
            ref={createButton}
            type="button"
            aria-expanded={creating}
            onClick={() => setCreating(true)}
            className={`${compactButtonClass.primary} shrink-0 max-sm:min-h-11`}
          >
            + Create team
          </button>
        )}
      </div>
      {creating && <CreateTeamForm onClose={close} />}
      {me.teams.length > 0 && (
        <ul className="divide-y divide-line rounded-xl border border-line bg-surface p-1">
          {me.teams.map((team) => (
            <TeamRow key={team.id} team={team} label={teamName(team, me)} several={me.teams.length > 1} />
          ))}
        </ul>
      )}
    </main>
  );
}

/** "Admin · 4 members · 2 heaps", "Member · Groups · 12 members · 2 heaps", or for a personal team "Only you · 1 heap". */
function TeamRow({ team, label, several }: { team: Me["teams"][number]; label: string; several: boolean }) {
  const who = team.personal
    ? "Only you"
    : `${team.role === "admin" ? "Admin" : "Member"}${team.group_managed ? " · Groups" : ""} · ${plural(team.member_count, "member")}`;
  return (
    <li className="flex items-center gap-3 px-2 py-2.5 max-sm:min-h-13 max-sm:py-1">
      <div className="flex min-w-0 flex-1 flex-col gap-0.5 sm:flex-row sm:items-center sm:gap-3">
        <span className="min-w-0 font-medium break-words sm:flex-1">{label}</span>
        <span className="text-[13px] text-muted">
          {who} · {plural(team.heaps.length, "heap")}
        </span>
      </div>
      {several && (
        <Link
          to="/all"
          search={{ team: team.slug }}
          aria-label={`All items in ${label}`}
          className="flex shrink-0 items-center rounded-lg px-2 py-1 text-[13px] text-ink-soft hover:bg-sunken hover:text-ink max-sm:min-h-11 max-sm:px-3"
        >
          All items
        </Link>
      )}
      <Link
        to="/teams/$team"
        params={{ team: team.slug }}
        aria-label={`Open ${label}`}
        className="flex shrink-0 items-center rounded-lg px-2 py-1 text-[13px] text-ink-soft hover:bg-sunken hover:text-ink max-sm:min-h-11 max-sm:px-3"
      >
        Open
      </Link>
    </li>
  );
}

/** The fields of a new team, named as the API names them, so a problem's `field` says which one it's about. */
/**
 * Name and Link name for a new team. The server makes the link name from the name when it's left out, so the note
 * names the link only once one is typed.
 */
function CreateTeamForm({ onClose }: { onClose: () => void }) {
  const create = useCreateTeam();
  const navigate = useNavigate();
  const { formRef, field, name, slug, otherError } = useNameAndLinkFields(create);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    create.mutate(
      { name, ...(slug !== "" && { slug }) },
      { onSuccess: (team) => void navigate({ to: "/teams/$team", params: { team: team.slug } }) },
    );
  };

  return (
    <form
      ref={formRef}
      aria-label="Create a team"
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
      <div className="flex flex-wrap gap-2.5">
        <div className="min-w-0 flex-[1_1_200px]">
          <Field label="Name" {...field("name")} autoFocus autoComplete="off" maxLength={100} />
        </div>
        <div className="min-w-0 flex-[1_1_160px]">
          <Field
            label="Link name"
            {...field("slug")}
            autoComplete="off"
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
            maxLength={39}
          />
        </div>
      </div>
      {otherError && <ProblemText error={otherError} className="text-overdue-ink" />}
      <div className="flex flex-wrap items-center gap-2.5">
        <span className="min-w-0 flex-1 text-xs break-words text-muted max-sm:basis-full max-sm:text-[13px]">
          {slug === "" ? "You'll be its admin." : `/teams/${slug} · you'll be its admin`}
        </span>
        <button
          type="button"
          onClick={onClose}
          className={`${compactButtonClass.quiet} max-sm:ml-auto max-sm:min-h-11`}
        >
          Cancel
        </button>
        <button
          type="submit"
          disabled={create.isPending}
          aria-busy={create.isPending || undefined}
          className={`${compactButtonClass.primary} max-sm:min-h-11`}
        >
          <Saving pending={create.isPending} />
          Create
        </button>
      </div>
    </form>
  );
}
