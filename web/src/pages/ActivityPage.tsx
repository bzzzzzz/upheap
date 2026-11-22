import { useHeap } from "../api/queries/heaps";
import { useMe } from "../api/queries/me";
import { useTeam } from "../api/queries/teams";
import { useAcrossAuditLog } from "../api/queries/audit";
import { acrossSubtitle } from "../components/acrossHeaps";
import { AuditEntryRow, AuditHistory, PagedHistory } from "../components/AuditHistory";
import { teamName } from "../components/teamName";
import { isNotFound, type HeapRef } from "../api/client";
import { HeapNotFound, TeamNotFound } from "../components/NotFound";

/** Everything that happened in a heap that the caller may see, newest first. */
export function HeapActivityPage({ heap: heapRef }: { heap: HeapRef }) {
  const heap = useHeap(heapRef);
  if (heap.isError && isNotFound(heap.error)) return <HeapNotFound />;
  return (
    <ActivityLayout
      title="Activity"
      subtitle={heap.data && `What changed in ${heap.data.name}, as far as you can see.`}
    >
      <AuditHistory scope={{ heap: heapRef }} showEntity />
    </ActivityLayout>
  );
}

/** What happened to a team itself, its members and its heaps as wholes, newest first. */
export function TeamActivityPage({ team: slug }: { team: string }) {
  const team = useTeam(slug);
  const { data: me } = useMe();
  if (team.isError && isNotFound(team.error)) return <TeamNotFound />;
  return (
    <ActivityLayout
      title="Team activity"
      subtitle={
        team.data &&
        `Changes to ${me ? teamName(team.data, me) : team.data.name}, its members and its heaps, as far as you can see.`
      }
    >
      <AuditHistory scope={{ team: slug }} showEntity />
    </ActivityLayout>
  );
}

/**
 * What happened in all the person's heaps, or in one team's, newest first (ux.md, All items): the heap's Activity
 * page, each entry naming its heap.
 */
export function AllActivityPage({ team }: { team: string | undefined }) {
  const { data: me } = useMe();
  const history = useAcrossAuditLog(team);
  if (history.isError && isNotFound(history.error)) return <TeamNotFound />;
  // The team is named only where the entries are of several.
  const several = team === undefined && (me?.teams.length ?? 0) > 1;
  return (
    <ActivityLayout title="Activity" subtitle={acrossSubtitle(me, team, "as far as you can see.")}>
      <PagedHistory
        history={history}
        empty="No history you can see."
        row={({ entry, heap }) => {
          const ofTeam = me?.teams.find((t) => t.slug === heap.team_slug);
          return (
            <AuditEntryRow
              key={entry.id}
              entry={entry}
              showEntity
              place={several && me && ofTeam ? `${heap.name} · ${teamName(ofTeam, me)}` : heap.name}
            />
          );
        }}
      />
    </ActivityLayout>
  );
}

/** The title, intro and card the Activity pages share, in the design tokens (ux.md, Toolkit). */
function ActivityLayout({
  title,
  subtitle,
  children,
}: {
  title: string;
  subtitle: string | undefined;
  children: React.ReactNode;
}) {
  return (
    <main>
      <h1 className="mt-6 mb-1 text-[22px] font-semibold tracking-[-0.015em]">{title}</h1>
      <p className="mb-4 min-h-5 text-[13px] break-words text-ink-soft">{subtitle}</p>
      <div className="rounded-[14px] border border-line bg-surface px-3">{children}</div>
    </main>
  );
}
