import { useHeap, useTeam } from "../api/queries";
import { AuditHistory } from "../components/AuditHistory";

/** Everything that happened in a heap that the caller may see, newest first. */
export function HeapActivityPage({ heapId }: { heapId: string }) {
  const heap = useHeap(heapId);
  return (
    <ActivityLayout
      title="Activity"
      subtitle={heap.data && `What changed in ${heap.data.name}: items, tags and views, and its settings for admins.`}
    >
      <AuditHistory scope={{ heapId }} showEntity />
    </ActivityLayout>
  );
}

/** What happened to a team itself, its members and its heaps as wholes, newest first. */
export function TeamActivityPage({ teamId }: { teamId: string }) {
  const team = useTeam(teamId);
  return (
    <ActivityLayout
      title="Team activity"
      subtitle={team.data && `Changes to ${team.data.name}, its members and its heaps.`}
    >
      <AuditHistory scope={{ teamId }} showEntity />
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
