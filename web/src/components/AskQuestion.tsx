import { Plus, X } from "lucide-react";
import { useRef, useState, type RefObject } from "react";
import { Button, Input, Label, Radio, RadioGroup, TextField } from "react-aria-components";

import { ApiProblem } from "../api/client";
import type { AskQuestionRequest, Item } from "../api/schema";
import type { useAskQuestion } from "../api/queries/questions";
import { compactButtonClass } from "./buttons";
import { itemProblem } from "./itemProblems";
import { RequestId } from "./RequestId";
import { focusClass, labelClass, pickBoxClass, pickClass } from "./ItemQuestions";
import { LazyMarkdownEditor } from "./LazyMarkdownEditor";
import { Saving } from "./Saving";
import { useFocusAfterRender } from "./useFocusAfterRender";

// The limits the server enforces (openapi `AskQuestionRequest`); the form only keeps Ask off past them.
const TEXT_MAX = 300;
const DETAILS_MAX = 20_000;
const CHOICES_MIN = 2;
const CHOICES_MAX = 10;
// The counters show a little before the limit.
const TEXT_COUNTER_FROM = 250;
const DETAILS_COUNTER_FROM = 19_000;

const inputClass =
  "min-h-9 w-full min-w-0 rounded-lg border border-line bg-surface px-2 text-base outline-none focus:border-accent pointer-coarse:min-h-11";

type Kind = "question" | "request";
type Answer = "words" | "one" | "any";
type Choice = { id: number; text: string };

/**
 * The "Ask a question" button, and the form it opens in place. `ask` is the section's mutation, and `open` the
 * section's state, so the section stays while the form is open.
 */
export function AskQuestion({
  item,
  canAsk,
  ask,
  open,
  onOpenChange,
}: {
  item: Item;
  canAsk: boolean;
  ask: ReturnType<typeof useAskQuestion>;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const setOpen = onOpenChange;
  const [message, setMessage] = useState("");
  const buttonRef = useRef<HTMLButtonElement>(null);
  const textRef = useRef<HTMLInputElement>(null);
  const focusAfterRender = useFocusAfterRender();

  const close = () => {
    ask.reset();
    setOpen(false);
    // Without the offer (the item was closed meanwhile) there is no button: the item takes the focus.
    focusAfterRender(() => buttonRef.current ?? document.querySelector<HTMLElement>("[data-item-pane]"));
  };

  return (
    <>
      <span role="status" className="sr-only">
        {message}
      </span>
      {open ? (
        <AskForm
          item={item}
          ask={ask}
          textRef={textRef}
          onCancel={close}
          onAsked={(text) => {
            setMessage(`Asked “${text}”.`);
            close();
          }}
        />
      ) : (
        canAsk && (
          <Button
            ref={buttonRef}
            onPress={() => {
              setMessage("");
              setOpen(true);
              focusAfterRender(() => textRef.current);
            }}
            className={`-ml-1 flex min-h-9 items-center gap-2 self-start rounded-lg px-1 text-sm text-ink-soft hover:text-ink ${focusClass} pointer-coarse:min-h-11 max-sm:w-full max-sm:justify-center`}
          >
            <Plus aria-hidden="true" className="size-4" />
            Ask a question
          </Button>
        )
      )}
    </>
  );
}

function Counter({ length, max }: { length: number; max: number }) {
  return (
    <p className={`text-xs ${length > max ? "text-overdue-ink" : "text-muted"}`}>
      {length.toLocaleString("en-US")}/{max.toLocaleString("en-US")}
    </p>
  );
}

/**
 * The form: every draft lives in this component's state, apart from the item, so a refetch never touches it. It is
 * mounted only while open, so closing it drops the draft.
 */
function AskForm({
  item,
  ask,
  textRef,
  onCancel,
  onAsked,
}: {
  item: Item;
  ask: ReturnType<typeof useAskQuestion>;
  /** The text field's, for the caller to focus it when the form opens. */
  textRef: RefObject<HTMLInputElement | null>;
  onCancel: () => void;
  onAsked: (text: string) => void;
}) {
  const [kind, setKind] = useState<Kind>("question");
  const [text, setText] = useState("");
  const [details, setDetails] = useState("");
  const [answer, setAnswer] = useState<Answer>("words");
  const [choices, setChoices] = useState<Choice[]>([
    { id: 1, text: "" },
    { id: 2, text: "" },
  ]);
  const [step, setStep] = useState("");
  const nextId = useRef(3);
  const focusAfterRender = useFocusAfterRender();

  const steps = (item.steps ?? []).filter((candidate) => candidate.state !== "dropped");
  const picking = kind === "question" && answer !== "words";
  const trimmedText = text.trim();
  // Counted in characters, as the server counts them, not in UTF-16 units.
  const textLength = [...trimmedText].length;
  const detailsLength = [...details].length;
  const named = choices.map((choice) => choice.text.trim()).filter((choice) => choice !== "");
  const duplicates = new Set(named.map((choice) => choice.toLowerCase())).size < named.length;
  const choicesValid = !picking || (named.length >= CHOICES_MIN && named.length <= CHOICES_MAX && !duplicates);
  const valid = textLength >= 1 && textLength <= TEXT_MAX && detailsLength <= DETAILS_MAX && choicesValid;
  const untouched = text.trim() === "" && details.trim() === "" && choices.every((choice) => choice.text.trim() === "");

  const edited = () => {
    if (ask.isError) ask.reset();
  };

  const send = () => {
    if (!valid || ask.isPending) return;
    const body: AskQuestionRequest = {
      kind,
      text: trimmedText,
      details: details.trim() === "" ? null : details,
      step: step === "" ? null : Number(step),
    };
    if (picking) {
      body.choices = named;
      body.many = answer === "any";
    }
    ask.mutate(body, { onSuccess: () => onAsked(trimmedText) });
  };

  const addChoiceAfter = (id: number) => {
    if (choices.length >= CHOICES_MAX) return;
    const added = nextId.current++;
    setChoices((current) => {
      const at = current.findIndex((choice) => choice.id === id);
      return [...current.slice(0, at + 1), { id: added, text: "" }, ...current.slice(at + 1)];
    });
    focusAfterRender(() => document.querySelector<HTMLInputElement>(`[data-choice="${added}"]`));
  };

  const error = ask.error;
  const problem =
    error &&
    (error instanceof ApiProblem && error.code === "item_closed"
      ? "The item was closed meanwhile."
      : itemProblem(error));

  return (
    <form
      aria-label="Ask a question"
      className="flex flex-col gap-3"
      onSubmit={(event) => {
        event.preventDefault();
        send();
      }}
      onKeyDown={(event) => {
        if (event.key !== "Escape") return;
        // Never lost to a stray key, and the item stays open: with something typed Escape does nothing.
        event.preventDefault();
        event.stopPropagation();
        if (untouched && !ask.isPending) onCancel();
      }}
    >
      <RadioGroup
        value={kind}
        onChange={(value) => {
          setKind(value as Kind);
          edited();
        }}
        orientation="horizontal"
        className="flex flex-col gap-1"
      >
        <Label className={labelClass}>Kind</Label>
        <div className="flex gap-4">
          {(["question", "request"] as const).map((value) => (
            <Radio key={value} value={value} className={pickClass}>
              {({ isSelected }) => (
                <>
                  <span aria-hidden="true" className={`${pickBoxClass} rounded-full`}>
                    {isSelected && <span className="size-2 rounded-full bg-surface" />}
                  </span>
                  {value === "question" ? "Question" : "Request"}
                </>
              )}
            </Radio>
          ))}
        </div>
      </RadioGroup>

      <TextField
        value={text}
        onChange={(value) => {
          setText(value);
          edited();
        }}
        className="flex flex-col gap-1"
      >
        <Label className={labelClass}>{kind === "question" ? "Question" : "What to do"}</Label>
        <Input ref={textRef} className={inputClass} />
        {textLength >= TEXT_COUNTER_FROM && <Counter length={textLength} max={TEXT_MAX} />}
      </TextField>

      <div className="flex flex-col gap-1">
        <LazyMarkdownEditor
          aria-label="Details, optional"
          placeholder="Details, optional (Markdown)"
          value={details}
          onChange={(value) => {
            setDetails(value);
            edited();
          }}
          className="min-h-14 w-full rounded-lg border border-line bg-surface focus-within:border-accent"
        />
        {detailsLength >= DETAILS_COUNTER_FROM && <Counter length={detailsLength} max={DETAILS_MAX} />}
      </div>

      {kind === "question" && (
        <RadioGroup
          value={answer}
          onChange={(value) => {
            setAnswer(value as Answer);
            edited();
          }}
          className="flex flex-col gap-1"
        >
          <Label className={labelClass}>Answer</Label>
          <div className="flex flex-wrap gap-x-4">
            {(
              [
                ["words", "In words"],
                ["one", "Pick one"],
                ["any", "Pick any"],
              ] as const
            ).map(([value, label]) => (
              <Radio key={value} value={value} className={pickClass}>
                {({ isSelected }) => (
                  <>
                    <span aria-hidden="true" className={`${pickBoxClass} rounded-full`}>
                      {isSelected && <span className="size-2 rounded-full bg-surface" />}
                    </span>
                    {label}
                  </>
                )}
              </Radio>
            ))}
          </div>
        </RadioGroup>
      )}

      {picking && (
        <div className="flex flex-col gap-2">
          {choices.map((choice, index) => (
            <div key={choice.id} className="flex items-center gap-1">
              <TextField
                aria-label={`Choice ${index + 1}`}
                value={choice.text}
                onChange={(value) => {
                  setChoices((current) => current.map((c) => (c.id === choice.id ? { ...c, text: value } : c)));
                  edited();
                }}
                className="min-w-0 flex-1"
              >
                <Input
                  data-choice={choice.id}
                  className={inputClass}
                  onKeyDown={(event) => {
                    // Adds the next field; never submits the form.
                    if (event.key !== "Enter") return;
                    event.preventDefault();
                    addChoiceAfter(choice.id);
                  }}
                />
              </TextField>
              {choices.length > 2 && (
                <Button
                  aria-label={`Remove choice ${index + 1}`}
                  onPress={() => {
                    // The focus goes to the choice that takes its place, else the one before it.
                    const next = choices[index + 1] ?? choices[index - 1];
                    setChoices((current) => current.filter((c) => c.id !== choice.id));
                    edited();
                    if (next)
                      focusAfterRender(() => document.querySelector<HTMLInputElement>(`[data-choice="${next.id}"]`));
                  }}
                  className={`${compactButtonClass.icon} ${focusClass}`}
                >
                  <X aria-hidden="true" className="size-4" />
                </Button>
              )}
            </div>
          ))}
          {duplicates && <p className="text-sm text-overdue-ink">Two choices are the same.</p>}
          {choices.length < CHOICES_MAX && (
            <Button
              onPress={() => {
                const last = choices[choices.length - 1];
                if (last) addChoiceAfter(last.id);
              }}
              className={`-ml-1 flex min-h-9 items-center gap-2 self-start rounded-lg px-1 text-sm text-ink-soft hover:text-ink ${focusClass} pointer-coarse:min-h-11`}
            >
              <Plus aria-hidden="true" className="size-4" />
              Add a choice
            </Button>
          )}
        </div>
      )}

      {steps.length > 0 && (
        <label className="flex flex-col gap-1">
          <span className={labelClass}>About a step</span>
          <select
            value={step}
            onChange={(event) => {
              setStep(event.target.value);
              edited();
            }}
            className="min-h-9 w-full min-w-0 rounded-lg border border-line bg-surface px-2 text-base text-ink pointer-coarse:min-h-11"
          >
            <option value="">No step</option>
            {steps.map((candidate) => (
              <option key={candidate.number} value={candidate.number}>
                {candidate.number}. {candidate.title}
              </option>
            ))}
          </select>
        </label>
      )}

      {problem && (
        <p role="alert" className="text-sm text-overdue-ink">
          {problem}
        </p>
      )}
      <RequestId error={error} />

      <div className="flex flex-col-reverse gap-2 sm:flex-row sm:self-end">
        <Button
          onPress={onCancel}
          isDisabled={ask.isPending}
          className={`${compactButtonClass.bordered} ${focusClass} max-sm:min-h-11 max-sm:w-full`}
        >
          Cancel
        </Button>
        <Button
          type="submit"
          isPending={ask.isPending}
          isDisabled={!valid}
          className={`${compactButtonClass.primary} ${focusClass} max-sm:min-h-11 max-sm:w-full`}
        >
          <Saving pending={ask.isPending} />
          Ask
        </Button>
      </div>
    </form>
  );
}
