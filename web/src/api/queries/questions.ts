import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api, unwrap, type HeapRef } from "../client";
import type { AskQuestionRequest, Item, Question, QuestionReplyRequest, Waiting } from "../schema";
import { rankedKey, closedKey, itemKey, waitingKey, acrossHeapsKey, acrossClosedKey, invalidateCounts } from "./keys";

/**
 * Asks a question or a request on the item `itemId` (its key). Not optimistic: the item the server returns, with the
 * question on it and the item waiting, is written into the cache. A failure fetches the item again, since it may have
 * been closed or its heap archived meanwhile, which takes the offer to ask away. The lists say which items wait for you,
 * and Waiting for me what does, so they are fetched again.
 */
export function useAskQuestion(heap: HeapRef, itemId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: AskQuestionRequest) =>
      unwrap(
        await api.POST("/api/v1/teams/{team}/heaps/{heap}/items/{item}/questions", {
          params: { path: { ...heap, item: itemId } },
          body,
        }),
      ),
    onSuccess: (saved) => {
      queryClient.setQueryData(itemKey(heap, saved.key), saved);
    },
    onError: () => {
      void queryClient.invalidateQueries({ queryKey: itemKey(heap, itemId) });
    },
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: rankedKey(heap) });
      void queryClient.invalidateQueries({ queryKey: acrossHeapsKey });
      void invalidateCounts(queryClient);
    },
  });
}

/**
 * Replies to a question of the item `itemId` (its key). Not optimistic: the item the server returns, with the question
 * answered, is written into the cache. A failure fetches the item again, so a question someone else answered meanwhile
 * shows as answered. Each question has its own version, which only a reply moves, so replies to different questions
 * don't wait for each other. Changing a reply passes `version`, the one its form opened on, so a change someone else made
 * meanwhile is refused rather than overwritten.
 */
export function useReplyToQuestion(heap: HeapRef, itemId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({
      question,
      reply,
      version: opened,
    }: {
      question: Question;
      reply: QuestionReplyRequest;
      version?: number;
    }) => {
      const cached = queryClient
        .getQueryData<Item>(itemKey(heap, itemId))
        ?.questions?.find((candidate) => candidate.number === question.number);
      const version = opened ?? Math.max(question.version, cached?.version ?? 0);
      return unwrap(
        await api.PATCH("/api/v1/teams/{team}/heaps/{heap}/items/{item}/questions/{question}", {
          params: {
            path: { ...heap, item: itemId, question: String(question.number) },
            header: { "If-Match": `"${version}"` },
          },
          body: { reply },
        }),
      );
    },
    onSuccess: (saved, { question }) => {
      queryClient.setQueryData(itemKey(heap, saved.key), saved);
      // The Waiting page's row leaves and its count drops at once; the refetch below confirms them.
      queryClient.setQueryData<Waiting>(
        waitingKey,
        (waiting) =>
          waiting && {
            // An answered question has no row there: changing its reply leaves the count alone.
            count: question.state === "open" ? Math.max(0, waiting.count - 1) : waiting.count,
            rows: waiting.rows.filter((row) => !(row.item.id === saved.id && row.question?.number === question.number)),
          },
      );
    },
    // Not returned: the mutation would wait for the refetches, and with it the focus move and the live message of
    // the row replied to.
    onError: () => {
      void queryClient.invalidateQueries({ queryKey: itemKey(heap, itemId) });
    },
    // The lists say which items wait for you, and Waiting for me what does. A reply that closed the item (Close as done)
    // moves it into the closed list.
    onSettled: (saved) => {
      void queryClient.invalidateQueries({ queryKey: rankedKey(heap) });
      void queryClient.invalidateQueries({ queryKey: acrossHeapsKey });
      void invalidateCounts(queryClient);
      if (saved && (saved.state === "done" || saved.state === "wont_do")) {
        void queryClient.invalidateQueries({ queryKey: closedKey(heap) });
        void queryClient.invalidateQueries({ queryKey: acrossClosedKey });
      }
    },
  });
}
