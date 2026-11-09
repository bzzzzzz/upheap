import { Link, useNavigate } from "@tanstack/react-router";
import { useEffect, useRef, useState, type FormEvent } from "react";

import { ApiProblem, type Me } from "../api/client";
import { useCreateTeam, useMe } from "../api/queries";
import { compactButtonClass } from "../components/buttons";
import { Field } from "../components/Field";
import { ProblemText, problemMessage } from "../components/ProblemText";
import { plural } from "../components/words";

/** The caller's teams, and Create team, which opens a form for a new one (S-teams, WideTeams). */
export function TeamsPage() {
  const { data: me } = useMe();
  const [creating, setCreating] = useState(false);
  const createButton = useRef<HTMLButtonElement>(null);
  if (!me) return null;

  const close = () => {
    setCreating(false);
    // The form replaced nothing, but focus was in it; it goes back to what opened it.
    window.requestAnimationFrame(() => createButton.current?.focus());
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
            <TeamRow key={team.id} team={team} />
          ))}
        </ul>
      )}
    </main>
  );
}

/** "Admin · 4 members · 2 heaps", or for a personal team "Only you · 1 heap". */
function TeamRow({ team }: { team: Me["teams"][number] }) {
  const who = team.personal
    ? "Only you"
    : `${team.role === "admin" ? "Admin" : "Member"} · ${plural(team.member_count, "member")}`;
  return (
    <li className="flex items-center gap-3 px-2 py-2.5 max-sm:min-h-13 max-sm:py-1">
      <div className="flex min-w-0 flex-1 flex-col gap-0.5 sm:flex-row sm:items-center sm:gap-3">
        <span className="min-w-0 font-medium break-words sm:flex-1">{team.name}</span>
        <span className="text-[13px] text-muted">
          {who} · {plural(team.heaps.length, "heap")}
        </span>
      </div>
      <Link
        to="/teams/$team"
        params={{ team: team.slug }}
        aria-label={`Open ${team.name}`}
        className="flex shrink-0 items-center rounded-lg px-2 py-1 text-[13px] text-ink-soft hover:bg-sunken hover:text-ink max-sm:min-h-11 max-sm:px-3"
      >
        Open
      </Link>
    </li>
  );
}

/** The fields of a new team, named as the API names them, so a problem's `field` says which one it's about. */
type TeamField = "name" | "slug";

/**
 * Name and Link name for a new team. The server makes the link name from the name when it's left out, so the note
 * names the link only once one is typed.
 */
function CreateTeamForm({ onClose }: { onClose: () => void }) {
  const create = useCreateTeam();
  const navigate = useNavigate();
  const [values, setValues] = useState<Record<TeamField, string>>({ name: "", slug: "" });
  const formRef = useRef<HTMLFormElement>(null);
  const { error } = create;
  const errorField: TeamField | undefined =
    error instanceof ApiProblem && (error.field === "name" || error.field === "slug") ? error.field : undefined;

  // After a failed submit, focus goes to the input the problem is about.
  useEffect(() => {
    const input = errorField && formRef.current?.elements.namedItem(errorField);
    if (input instanceof HTMLInputElement) input.focus();
  }, [error, errorField]);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const slug = values.slug.trim();
    create.mutate(
      { name: values.name, ...(slug !== "" && { slug }) },
      { onSuccess: (team) => void navigate({ to: "/teams/$team", params: { team: team.slug } }) },
    );
  };

  const field = (name: TeamField) => ({
    name,
    value: values[name],
    error: errorField === name && error ? problemMessage(error) : undefined,
    onChange: (value: string) => {
      setValues((current) => ({ ...current, [name]: value }));
      if (errorField === name) create.reset();
    },
  });

  const slug = values.slug.trim();
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
      {error && !errorField && <ProblemText error={error} className="text-overdue-ink" />}
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
        <button type="submit" disabled={create.isPending} className={`${compactButtonClass.primary} max-sm:min-h-11`}>
          Create
        </button>
      </div>
    </form>
  );
}
