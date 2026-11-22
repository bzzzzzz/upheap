/**
 * The saving ring (ux.md, Rules): shown inside the control that was pressed, or on the field or row it changed, from
 * the press until the server's answer and the refetch it starts have landed. The outer span appears after 150 ms, so a
 * fast answer shows no flicker; the inner one turns, or pulses under reduced motion. Two spans keep the pulse's
 * opacity apart from the delayed appearance.
 */
export function Saving({ pending, className = "" }: { pending: boolean; className?: string }) {
  if (!pending) return null;
  return (
    <span
      aria-hidden="true"
      className={`inline-flex shrink-0 transition-opacity delay-150 duration-0 starting:opacity-0 ${className}`}
    >
      <span className="size-3 rounded-full border-[1.5px] border-line border-t-accent motion-safe:animate-[spin_0.8s_linear_infinite] motion-reduce:animate-pulse max-sm:size-3.5" />
    </span>
  );
}
