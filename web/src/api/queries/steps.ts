import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api, unwrap, type HeapRef } from "../client";
import type { AddStepRequest, Item, Step, StepPatch } from "../schema";
import { itemKey, invalidateHeap } from "./keys";

/**
 * The scope that makes one item's step writes run one at a time (ux.md, The item): a tick followed quickly by a title
 * edit doesn't conflict with itself, as each waits for the one before and sends the newest version it left behind.
 */
const stepScope = (heap: HeapRef, key: string) => ({ id: `step-edits:${heap.team}/${heap.heap}/${key}` });

/**
 * Adds a step to the task `taskKey`. Not optimistic: the item the server returns, with the new step, is written into
 * the cache. Adding needs no version.
 */
export function useAddStep(heap: HeapRef, taskKey: string) {
  const queryClient = useQueryClient();
  return useMutation({
    scope: stepScope(heap, taskKey),
    mutationFn: async (body: AddStepRequest) =>
      unwrap(
        await api.POST("/api/v1/teams/{team}/heaps/{heap}/items/{item}/steps", {
          params: { path: { ...heap, item: taskKey } },
          body,
        }),
      ),
    onSuccess: (saved) => queryClient.setQueryData(itemKey(heap, saved.key), saved),
    onSettled: () => invalidateHeap(queryClient, heap),
  });
}

/**
 * Saves one change to a step of the task `taskKey`: its title, context, state or place. Each sends the newest version of the
 * step it knows of, its own or the one the previous write brought back. A change by someone else still fails with a
 * conflict; either way the heap is invalidated, which fetches the item and the lists again, so the next try has fresh
 * versions and the rows count the steps.
 */
export function useUpdateStep(heap: HeapRef, taskKey: string) {
  const queryClient = useQueryClient();
  return useMutation({
    scope: stepScope(heap, taskKey),
    mutationFn: async ({ step, patch }: { step: Step; patch: StepPatch }) => {
      const cached = queryClient
        .getQueryData<Item>(itemKey(heap, taskKey))
        ?.steps?.find((candidate) => candidate.number === step.number);
      const version = Math.max(step.version, cached?.version ?? 0);
      return unwrap(
        await api.PATCH("/api/v1/teams/{team}/heaps/{heap}/items/{item}/steps/{step}", {
          params: {
            path: { ...heap, item: taskKey, step: String(step.number) },
            header: { "If-Match": `"${version}"` },
          },
          body: patch,
        }),
      );
    },
    onSuccess: (saved) => queryClient.setQueryData(itemKey(heap, saved.key), saved),
    onSettled: () => invalidateHeap(queryClient, heap),
  });
}
