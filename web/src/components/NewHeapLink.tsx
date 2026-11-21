import { BASE } from "../basePath";

/** "New link: /teams/{team}/heaps/{slug}", the path a heap gets from the link name typed for it. */
export function NewHeapLink({ team, slug }: { team: string; slug: string }) {
  return (
    <>
      New link:{" "}
      <span className="font-mono break-all text-ink-soft">
        {BASE}/teams/{team}/heaps/{slug}
      </span>
    </>
  );
}
