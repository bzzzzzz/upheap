import type { Heap } from "../api/client";
import { useSetHeapArchived } from "../api/queries";
import { problemMessage } from "./ProblemText";
import { useToast } from "./UndoToast";

/**
 * Archives and unarchives heaps of the team `team`. Archiving offers Undo in the app's toast (ux.md, Rules §1);
 * unarchiving is its own way back and acts without one. A failed archive is left to `mutation.error`, shown where
 * the button is; a failed Undo goes in an error toast.
 */
export function useArchiveHeap(team: string) {
  const mutation = useSetHeapArchived(team);
  const setToast = useToast();

  const unarchive = (heap: Heap) => mutation.mutate({ heap, archived: false });
  const archive = (heap: Heap) => {
    // `mutateAsync`, not `mutate` with callbacks: the toast can outlive the component that archived (it stays while
    // in the heap), and TanStack drops a call's callbacks once that component has unmounted.
    mutation.mutateAsync({ heap, archived: true }).then(
      // Undo uses the heap the server answered with, so a rename meanwhile doesn't leave it with a stale link name.
      (archived) =>
        setToast({
          message: `“${heap.name}” is archived.`,
          undo: () =>
            mutation
              .mutateAsync({ heap: archived, archived: false })
              .then(undefined, (error: Error) => setToast({ message: problemMessage(error), error: true })),
        }),
      // The error shows under the button, from `mutation.error`.
      () => undefined,
    );
  };
  return { archive, unarchive, mutation };
}
