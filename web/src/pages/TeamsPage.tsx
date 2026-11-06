import { Link, useNavigate } from "@tanstack/react-router";
import { useState, type FormEvent } from "react";

import { useCreateTeam, useMe } from "../api/queries";
import { inputClass, primaryButtonClass } from "../components/NameForm";
import { ProblemText } from "../components/ProblemText";

/** The caller's teams, and a form to start a new one. */
export function TeamsPage() {
  const { data: me } = useMe();
  const create = useCreateTeam();
  const navigate = useNavigate();
  const [name, setName] = useState("");

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (name.trim() === "") return;
    create.mutate(name, {
      onSuccess: (team) => void navigate({ to: "/teams/$teamId", params: { teamId: team.id } }),
    });
  };

  return (
    <main>
      <h1 className="mt-6 mb-1 text-xl font-semibold tracking-tight">Teams</h1>
      <p className="mb-4 text-sm text-stone-500">
        A team shares its heaps. Admins add members, manage heaps and make others admins.
      </p>
      {me && me.teams.length > 0 && (
        <ul className="mb-6 rounded-lg border border-stone-200 bg-white px-3 dark:border-stone-800 dark:bg-stone-900">
          {me.teams.map((team) => (
            <li
              key={team.id}
              className="flex items-center gap-3 border-b border-stone-200 py-2 last:border-b-0 dark:border-stone-800"
            >
              <Link
                to="/teams/$teamId"
                params={{ teamId: team.id }}
                className="min-w-0 flex-1 text-sm font-medium hover:underline"
              >
                {team.name}
              </Link>
              <span className="text-xs text-stone-500">
                {team.heaps.length === 1 ? "1 heap" : `${team.heaps.length} heaps`}
              </span>
              <span className="w-14 text-right text-xs text-stone-500">
                {team.role === "admin" ? "Admin" : "Member"}
              </span>
            </li>
          ))}
        </ul>
      )}
      <form onSubmit={submit} className="flex flex-wrap items-center gap-2">
        <input
          aria-label="New team name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="New team name"
          maxLength={100}
          className={`${inputClass} w-56`}
        />
        <button type="submit" disabled={create.isPending || name.trim() === ""} className={primaryButtonClass}>
          Create team
        </button>
        <ProblemText error={create.error} />
      </form>
    </main>
  );
}
