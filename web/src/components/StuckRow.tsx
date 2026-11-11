import { ChevronRight } from "lucide-react";
import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { Button } from "react-aria-components";

import { ApiProblem, type HeapRef, type WaitingStuck } from "../api/client";
import { useRetryTurn, useTakeBack } from "../api/queries";
import { TAKE_BACK_QUESTION } from "./agentConfirm";
import { compactButtonClass } from "./buttons";
import { InlineConfirm } from "./InlineConfirm";
import { itemProblem } from "./itemProblems";
import { useConfirmation } from "./useConfirmation";

const focusClass = "outline-none data-[focus-visible]:outline-2 data-[focus-visible]:outline-accent";

const isNotStuck = (error: Error) => error instanceof ApiProblem && error.code === "turn_not_stuck";

/**
 * An item whose agent turn is stuck: 3 runs in a row failed, so it left the agents' queue and waits for the person
 * (ux.md, Waiting for me). Shown on the Waiting for me page and in the item's Waits for you, with the same two ways
 * out: Retry, which acts at once and puts the item back in the queue, and Take back, which asks first. `line` is the
 * row's own first line. The caller says what to do once the row has been acted on, or turns out to be stale.
 */
export function StuckRow({
  heapRef,
  item,
  stuck,
  line,
  onRetried,
  onTakenBack,
  onElsewhere,
}: {
  heapRef: HeapRef;
  item: { id: string; key: string; title: string };
  stuck: WaitingStuck;
  line: ReactNode;
  onRetried: () => void;
  onTakenBack: () => void;
  /** The turn was taken back or retried by someone else first. */
  onElsewhere: () => void;
}) {
  const retry = useRetryTurn(heapRef);
  const takeBack = useTakeBack(heapRef);
  const { confirming, ask, cancel, buttonRef } = useConfirmation();
  const canRetry = stuck.allowed_actions.includes("retry");
  const canTakeBack = stuck.allowed_actions.includes("take_back");

  const retryNow = () =>
    retry.mutateAsync(item).then(onRetried, (error: Error) => {
      if (isNotStuck(error)) onElsewhere();
    });
  const takeBackNow = () => takeBack.mutateAsync(item).then(onTakenBack, () => undefined);
  const error = retry.error && !isNotStuck(retry.error) ? retry.error : null;

  return (
    <li data-item={item.id} data-stuck="" className="flex flex-col gap-1.5 py-3 first:pt-0 last:pb-0">
      {line}
      <Reason failedRuns={stuck.failed_runs} reason={stuck.reason ?? null} />
      {confirming ? (
        <InlineConfirm
          question={TAKE_BACK_QUESTION}
          confirmLabel="Take back"
          pending={takeBack.isPending}
          error={takeBack.error}
          onConfirm={() => void takeBackNow()}
          onCancel={cancel}
        />
      ) : (
        (canRetry || canTakeBack) && (
          <div className="flex gap-2 max-sm:grid max-sm:grid-cols-2">
            {canRetry && (
              <Button
                isDisabled={retry.isPending}
                onPress={() => {
                  takeBack.reset();
                  retry.reset();
                  void retryNow();
                }}
                aria-label={`Retry ${item.title}`}
                className={`${compactButtonClass.primary} ${focusClass}`}
              >
                Retry
              </Button>
            )}
            {canTakeBack && (
              <Button
                ref={buttonRef}
                onPress={() => {
                  retry.reset();
                  takeBack.reset();
                  ask();
                }}
                aria-label={`Take back ${item.title}`}
                className={`${compactButtonClass.bordered} ${focusClass}`}
              >
                Take back
              </Button>
            )}
          </div>
        )
      )}
      {error && (
        <p role="alert" className="text-sm text-overdue-ink">
          {itemProblem(error)}
        </p>
      )}
    </li>
  );
}

/**
 * "3 runs in a row failed: <why the last one failed>", cut to two lines with a Show all button when the reason runs
 * longer: it is whatever the agent tool said, and can be a page of output.
 */
function Reason({ failedRuns, reason }: { failedRuns: number; reason: string | null }) {
  const text = useRef<HTMLParagraphElement>(null);
  const [shown, setShown] = useState(false);
  const [overflowing, setOverflowing] = useState(false);
  useLayoutEffect(() => {
    const paragraph = text.current;
    // Measured while cut; once shown in full, the button stays to fold it again.
    if (paragraph && !shown) setOverflowing(paragraph.scrollHeight > paragraph.clientHeight + 1);
  }, [failedRuns, reason, shown]);
  return (
    <>
      <p
        ref={text}
        data-stuck-reason=""
        className={`text-sm break-words whitespace-pre-wrap ${shown ? "" : "line-clamp-2"}`}
      >
        {`${failedRuns} runs in a row failed${reason ? `: ${reason}` : ""}`}
      </p>
      {(overflowing || shown) && (
        <Button
          onPress={() => setShown((current) => !current)}
          aria-expanded={shown}
          data-details=""
          className={`-ml-1 flex min-h-9 items-center gap-1 self-start rounded-lg px-1 text-sm text-ink-soft hover:text-ink ${focusClass} pointer-coarse:min-h-11`}
        >
          <ChevronRight
            aria-hidden="true"
            className={`size-3.5 transition-transform motion-reduce:transition-none ${shown ? "rotate-90" : ""}`}
          />
          {shown ? "Show less" : "Show all"}
        </Button>
      )}
    </>
  );
}
