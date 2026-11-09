import { useHeap, useMe, useTeam } from "../api/queries";
import { AuditHistory } from "../components/AuditHistory";
import { teamName } from "../components/teamName";
import type { HeapRef } from "../api/client";

/** Everything that happened in a heap that the caller may see, newest first. */
export function HeapActivityPage({ heap: heapRef }: { heap: HeapRef }) {
  const heap = useHeap(heapRef);
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
      <h1 className="mt-6 mb-1 text-xl font-semibold tracking-tight">{title}</h1>
      <p className="mb-4 min-h-5 text-sm text-stone-500">{subtitle}</p>
      <div className="rounded-lg border border-stone-200 bg-white px-3 dark:border-stone-800 dark:bg-stone-900">
        {children}
      </div>
    </main>
  );
}
