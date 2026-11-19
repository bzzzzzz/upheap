import { Link } from "@tanstack/react-router";

import type { HeapRef } from "../api/client";
import { useMe } from "../api/queries/me";
import { useTags } from "../api/queries/tags";
import { RequestId } from "../components/RequestId";
import { teamName } from "../components/teamName";
import { plural } from "../components/words";
import { cardTitleClass, introClass } from "./settingsForm";
import { NewTagForm, TagRow, tagCardClass, type TagSettings } from "./TagSettings";

/**
 * Any member can add tags, rename them and edit their descriptions, as `allowed_actions` says (a heap's tags not in an
 * archived heap). Tags can't be deleted. Each tag reads as a row; its edit button turns the row into a form. Under
 * the heap's tags, a card lists the team's, which follow their own `allowed_actions` (these allow editing from an
 * archived heap), and links to the Team page, where they are made.
 */
export function TagsPage({ heap: heapRef }: { heap: HeapRef }) {
  const tags = useTags(heapRef);
  const { data: me } = useMe();

  // A failed refetch keeps the data, so what is on screen (and any draft in it) stays; the error shows only without it.
  if (!tags.data) {
    if (tags.isError) {
      return (
        <>
          <p role="alert" className="py-16 text-center text-overdue-ink">
            {tags.error.message}
          </p>
          <RequestId error={tags.error} className="justify-center" />
        </>
      );
    }
    return <p className="py-16 text-center text-muted">Loading…</p>;
  }

  const { heap, allowed_actions, team_tags: teamTags } = tags.data;
  const count = tags.data.tags.length;
  const team = me?.teams.find((t) => t.slug === heapRef.team);
  const name = me && team ? teamName(team, me) : heapRef.team;
  const heapSettings: TagSettings = { scope: { kind: "heap", heap: heapRef }, teamName: name };
  const teamSettings: TagSettings = { scope: { kind: "team", team: heapRef.team }, teamName: name };
  return (
    <section className="flex flex-col gap-4 sm:gap-6">
      <h2 className="sr-only">Tags</h2>
      <p className={introClass}>Tags group similar items in {heap.name}. They can be renamed but not deleted.</p>
      {allowed_actions.includes("create") && <NewTagForm settings={heapSettings} />}
      {count === 0 ? (
        <p className="py-12 text-center text-muted">No tags of this heap yet.</p>
      ) : (
        <section aria-labelledby="all-tags" className={tagCardClass}>
          <h3
            id="all-tags"
            className={`${cardTitleClass} border-b border-line px-3.5 pt-3.5 pb-2.5 sm:px-4 sm:pt-4 sm:pb-3`}
          >
            {plural(count, "tag")} of this heap
          </h3>
          <ul className="divide-y divide-line">
            {tags.data.tags.map((tag) => (
              <TagRow key={tag.id} tag={tag} settings={heapSettings} heap={heap} />
            ))}
          </ul>
        </section>
      )}
      <section aria-labelledby="team-tags-card" className={tagCardClass}>
        <div className="flex items-baseline justify-between gap-3 px-3.5 pt-3.5 pb-2.5 sm:px-4 sm:pt-4 sm:pb-3">
          <h3 id="team-tags-card" className={cardTitleClass}>
            {plural(teamTags.length, "tag")} of the team {name}
          </h3>
          {/* With no team tags, the line below links there instead. */}
          {teamTags.length > 0 && (
            <Link
              to="/teams/$team"
              params={{ team: heapRef.team }}
              hash="team-tags"
              className="shrink-0 text-sm text-ink-soft hover:text-ink hover:underline"
            >
              Team page
            </Link>
          )}
        </div>
        {teamTags.length === 0 ? (
          <p className="border-t border-line px-3.5 py-3 text-sm text-muted sm:px-4">
            {name} has no team tags yet. Make them on the{" "}
            <Link
              to="/teams/$team"
              params={{ team: heapRef.team }}
              hash="team-tags"
              className="text-ink-soft underline hover:text-ink"
            >
              Team page
            </Link>
            .
          </p>
        ) : (
          <ul className="divide-y divide-line border-t border-line">
            {teamTags.map((tag) => (
              <TagRow key={tag.id} tag={tag} settings={teamSettings} />
            ))}
          </ul>
        )}
      </section>
    </section>
  );
}
