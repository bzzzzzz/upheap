import { useId, type ReactNode } from "react";

/** A titled part of the Filters panel. */
export function Section({ title, children }: { title: string; children: ReactNode }) {
  const id = useId();
  return (
    <section aria-labelledby={id} className="flex flex-col gap-3">
      <h3 id={id} className="text-xs font-semibold tracking-wide text-muted uppercase">
        {title}
      </h3>
      {children}
    </section>
  );
}
