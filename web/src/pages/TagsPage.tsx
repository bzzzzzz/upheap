import { useState, type FormEvent } from "react";

import { ApiProblem, type Tag, type HeapRef } from "../api/client";
import { useCreateTag, useTags, useUpdateTag } from "../api/queries";
import { HistoryDisclosure } from "../components/AuditHistory";

const inputClass =
  "rounded border border-stone-300 bg-white px-2 py-1 text-sm text-stone-900 disabled:opacity-60 dark:border-stone-700 dark:bg-stone-900 dark:text-stone-100";

/** Any member can add tags, rename them and edit their descriptions. Tags can't be deleted. */
export function TagsPage({ heap: heapRef }: { heap: HeapRef }) {
  const tags = useTags(heapRef);

  if (tags.isPending) {
    return <p className="py-16 text-center text-stone-500">Loading…</p>;
  }
  if (tags.isError) {
    return (
      <p role="alert" className="py-16 text-center text-red-700 dark:text-red-400">
        {tags.error.message}
      </p>
    );
  }

  const { heap, allowed_actions } = tags.data;
  return (
    <main>
      <h1 className="mt-6 mb-1 text-xl font-semibold tracking-tight">Tags</h1>
      <p className="mb-4 text-sm text-stone-500">
        Tags group similar items in {heap.name}. They can be renamed but not deleted.
      </p>
      {allowed_actions.includes("create") && <NewTagForm heapRef={heapRef} />}
      {tags.data.tags.length === 0 ? (
        <p className="py-12 text-center text-stone-500">No tags yet.</p>
      ) : (
        <ul className="rounded-lg border border-stone-200 bg-white px-3 dark:border-stone-800 dark:bg-stone-900">
          {tags.data.tags.map((tag) => (
            <TagRow key={tag.id} tag={tag} heapRef={heapRef} />
          ))}
        </ul>
      )}
    </main>
  );
}

function NewTagForm({ heapRef }: { heapRef: HeapRef }) {
  const create = useCreateTag(heapRef);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (name.trim() === "") return;
    create.mutate(
      { name, description: description || null },
      {
        onSuccess: () => {
          setName("");
          setDescription("");
        },
      },
    );
  };

  return (
    <form onSubmit={submit} className="mb-6 flex flex-wrap items-start gap-2">
      <input
        aria-label="New tag name"
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder="Tag name"
        maxLength={50}
        className={`${inputClass} w-40`}
      />
      <input
        aria-label="New tag description"
        value={description}
        onChange={(e) => setDescription(e.target.value)}
        placeholder="Description (optional)"
        className={`${inputClass} min-w-0 flex-1`}
      />
      <button
        type="submit"
        disabled={create.isPending || name.trim() === ""}
        className="rounded-md bg-stone-900 px-3 py-1 text-sm font-medium text-white disabled:opacity-40 dark:bg-stone-100 dark:text-stone-900"
      >
        Add tag
      </button>
      <ProblemText error={create.error} />
    </form>
  );
}

function TagRow({ tag, heapRef }: { tag: Tag; heapRef: HeapRef }) {
  const update = useUpdateTag(heapRef);
  const canEdit = tag.allowed_actions.includes("edit");
  const [name, setName] = useState(tag.name);
  const [description, setDescription] = useState(tag.description ?? "");
  // When the server sends a newer version (a save, or someone else's edit), the inputs show it.
  const [shownVersion, setShownVersion] = useState(tag.version);
  if (shownVersion !== tag.version) {
    setShownVersion(tag.version);
    setName(tag.name);
    setDescription(tag.description ?? "");
  }
  const changed = name !== tag.name || description !== (tag.description ?? "");

  const save = (event: FormEvent) => {
    event.preventDefault();
    update.mutate({
      tag,
      patch: {
        ...(name !== tag.name && { name }),
        ...(description !== (tag.description ?? "") && { description: description || null }),
      },
    });
  };

  return (
    <li className="border-b border-stone-200 py-3 last:border-b-0 dark:border-stone-800">
      <form onSubmit={save} className="flex flex-wrap items-start gap-2">
        <input
          aria-label={`Name of tag ${tag.name}`}
          value={name}
          disabled={!canEdit}
          onChange={(e) => setName(e.target.value)}
          maxLength={50}
          className={`${inputClass} w-40 font-medium`}
        />
        <input
          aria-label={`Description of tag ${tag.name}`}
          value={description}
          disabled={!canEdit}
          onChange={(e) => setDescription(e.target.value)}
          placeholder="No description"
          className={`${inputClass} min-w-0 flex-1`}
        />
        {canEdit && changed && (
          <>
            <button
              type="submit"
              disabled={update.isPending || name.trim() === ""}
              className="rounded-md bg-stone-900 px-3 py-1 text-sm font-medium text-white disabled:opacity-40 dark:bg-stone-100 dark:text-stone-900"
            >
              Save
            </button>
            <button
              type="button"
              onClick={() => {
                setName(tag.name);
                setDescription(tag.description ?? "");
              }}
              className="px-1 py-1 text-sm text-stone-500 hover:text-stone-900 dark:hover:text-stone-100"
            >
              Cancel
            </button>
          </>
        )}
        <ProblemText error={update.error} />
      </form>
      <HistoryDisclosure scope={{ heap: heapRef }} entityId={tag.id} />
    </li>
  );
}

function ProblemText({ error }: { error: Error | null }) {
  if (!error) return null;
  const message =
    error instanceof ApiProblem && error.code === "tag_name_taken"
      ? "This heap already has a tag with that name."
      : error instanceof ApiProblem && error.code === "version_conflict"
        ? "Someone else changed this tag. It now shows their version."
        : error.message;
  return (
    <p role="alert" className="basis-full text-sm text-red-700 dark:text-red-400">
      {message}
    </p>
  );
}
