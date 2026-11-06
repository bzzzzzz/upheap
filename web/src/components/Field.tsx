import { useId, type InputHTMLAttributes } from "react";

export function Field({ label, ...input }: { label: string } & InputHTMLAttributes<HTMLInputElement>) {
  const id = useId();
  return (
    <div className="flex flex-col gap-1">
      <label htmlFor={id} className="text-sm font-medium">
        {label}
      </label>
      <input
        id={id}
        {...input}
        className="rounded-md border border-stone-300 bg-white px-3 py-2 dark:border-stone-700 dark:bg-stone-900"
      />
    </div>
  );
}
