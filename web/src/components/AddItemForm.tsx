import { useEffect, useRef, useState, type FormEvent } from "react";

import type { ItemParameter, ItemTag, Size } from "../api/client";
import { useCreateItem, useParameters } from "../api/queries";
import { ParameterSelect } from "./ParameterSelect";
import { SizeSelect } from "./SizeSelect";
import { TagPicker } from "./TagPicker";

/**
 * The fastest path in the app: type a title, press Enter, type the next one. `n` focuses it. Picking a tag brings up
 * the parameters linked to it (Req Views: Add item §3).
 */
export function AddItemForm({ heapId, disabled }: { heapId: string; disabled: boolean }) {
  const create = useCreateItem(heapId);
  const parameters = useParameters(heapId);
  const titleRef = useRef<HTMLInputElement>(null);
  const [title, setTitle] = useState("");
  const [more, setMore] = useState(false);
  const [effort, setEffort] = useState<Size | null>(null);
  const [impact, setImpact] = useState<Size | null>(null);
  const [dueDate, setDueDate] = useState("");
  const [dueFirm, setDueFirm] = useState(false);
  const [description, setDescription] = useState("");
  const [tags, setTags] = useState<ItemTag[]>([]);
  const [values, setValues] = useState<Record<string, string | null>>({});
  // Suggested: the parameters linked to a chosen tag, and any already given a value.
  const suggested = (parameters.data?.parameters ?? []).filter(
    (parameter) => parameter.tag_ids.some((id) => tags.some((tag) => tag.id === id)) || values[parameter.id],
  );

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing = target?.closest("input, textarea, select, [contenteditable]");
      if (event.key === "n" && !typing && !event.metaKey && !event.ctrlKey && !event.altKey) {
        event.preventDefault();
        titleRef.current?.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (title.trim() === "") return;
    create.mutate(
      {
        title,
        description: description || null,
        effort,
        impact,
        due: dueDate ? { date: dueDate, firm: dueFirm } : null,
        tag_ids: tags.map((tag) => tag.id),
        parameter_values: Object.fromEntries(
          suggested.flatMap((parameter) => {
            const option = values[parameter.id];
            return option ? [[parameter.id, option]] : [];
          }),
        ),
      },
      {
        onSuccess: () => {
          setTitle("");
          setDescription("");
          setEffort(null);
          setImpact(null);
          setDueDate("");
          setDueFirm(false);
          setTags([]);
          setValues({});
          titleRef.current?.focus();
        },
      },
    );
  };

  return (
    <form
      onSubmit={submit}
      className="my-6 rounded-lg border border-stone-200 bg-white p-3 shadow-sm dark:border-stone-800 dark:bg-stone-900"
    >
      <div className="flex gap-2">
        <label htmlFor="new-item-title" className="sr-only">
          New item title
        </label>
        <input
          id="new-item-title"
          ref={titleRef}
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          disabled={disabled}
          placeholder="Add an item…  (press N)"
          autoComplete="off"
          className="min-w-0 flex-1 bg-transparent px-1 py-1 outline-none placeholder:text-stone-400"
        />
        <button
          type="button"
          onClick={() => setMore(!more)}
          aria-expanded={more}
          className="text-sm text-stone-500 hover:text-stone-900 dark:hover:text-stone-100"
        >
          {more ? "Less" : "More"}
        </button>
        <button
          type="submit"
          disabled={disabled || create.isPending || title.trim() === ""}
          className="rounded-md bg-stone-900 px-3 py-1 text-sm font-medium text-white disabled:opacity-40 dark:bg-stone-100 dark:text-stone-900"
        >
          Add
        </button>
      </div>
      {more && (
        <div className="mt-3 flex flex-col gap-3 border-t border-stone-100 pt-3 dark:border-stone-800">
          <div className="flex flex-wrap items-center gap-4">
            <SizeSelect label="Effort" value={effort} onChange={setEffort} />
            <SizeSelect label="Impact" value={impact} onChange={setImpact} />
            <label className="inline-flex items-center gap-1 text-xs text-stone-500">
              Due
              <input
                type="date"
                value={dueDate}
                onChange={(e) => setDueDate(e.target.value)}
                className="rounded border border-stone-300 bg-white px-1 py-0.5 text-stone-900 dark:border-stone-700 dark:bg-stone-900 dark:text-stone-100"
              />
            </label>
            <label className="inline-flex items-center gap-1 text-xs text-stone-500">
              <input
                type="checkbox"
                checked={dueFirm}
                onChange={(e) => setDueFirm(e.target.checked)}
                disabled={!dueDate}
              />
              Exact deadline
            </label>
          </div>
          <TagPicker heapId={heapId} value={tags} onChange={setTags} disabled={disabled} />
          {suggested.length > 0 && (
            <div className="flex flex-wrap items-center gap-4" role="group" aria-label="Parameters for these tags">
              {suggested.map((parameter) => (
                <ParameterSelect
                  key={parameter.id}
                  parameter={parameter}
                  current={unsetValue(parameter.id, parameter.name)}
                  value={values[parameter.id] ?? null}
                  onChange={(option) => setValues({ ...values, [parameter.id]: option })}
                  disabled={disabled}
                />
              ))}
            </div>
          )}
          <label className="flex flex-col gap-1 text-xs text-stone-500">
            Description
            <textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={3}
              className="rounded border border-stone-300 bg-white px-2 py-1 text-sm text-stone-900 dark:border-stone-700 dark:bg-stone-900 dark:text-stone-100"
            />
          </label>
        </div>
      )}
      {create.error && (
        <p role="alert" className="mt-2 text-sm text-red-700 dark:text-red-400">
          {create.error.message}
        </p>
      )}
    </form>
  );
}

/** An unset value of a parameter in use, for a new item that has no values yet. */
const unsetValue = (parameterId: string, name: string): ItemParameter => ({
  parameter_id: parameterId,
  name,
  parameter_archived: false,
  option: null,
});
