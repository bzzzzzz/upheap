// The Upheap mark: a pyramid seen from just above, a hollow cap over two layers. It draws in the current text color,
// like the lucide icons beside it; web/public/favicon.svg is the same drawing in the accent color.
export function Logo({ className }: { className?: string }) {
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.9}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
    >
      <path d="M12 2.9 16.7 10.4 12 12.3 7.3 10.4Z" />
      <path d="M5.2 13.3 12 16.55 18.8 13.3" />
      <path d="M3.2 16.9 12 21.1 20.8 16.9" />
    </svg>
  );
}
