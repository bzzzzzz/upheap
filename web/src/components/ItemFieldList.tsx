import { ChevronDown } from "lucide-react";
import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { ToggleButtonGroup } from "react-aria-components";

import type { HeapRef } from "../api/client";
import type { Item, ItemPatchRequest, ItemTag, Need, RankedItem, Size } from "../api/schema";
import { useParameters } from "../api/queries/parameters";
import { ChoiceButtons } from "./ChoiceButtons";
import { DateField } from "./DateField";
import { DuePicker, DueRows } from "./DuePicker";
import { NEED_HINTS, NEED_LABELS, NEEDS, SIZE_LABELS, SIZES } from "./labels";
import { PHONE, useMedia } from "./media";
import { NeedPicker } from "./NeedPicker";
import { ParameterPicker } from "./ParameterPicker";
import { TeamMark } from "./TeamMark";
import { PopoverSheet } from "./PopoverSheet";
import { formatDate } from "./reminders";
import { SheetChoice } from "./Sheet";
import { TagPicker } from "./TagPicker";
import { useFocusAfterRender } from "./useFocusAfterRender";

const SIZE_OPTIONS = SIZES.map((size) => ({ id: size, label: SIZE_LABELS[size] }));

/** The value a choice row stands for when it clears the field. */
const NOT_SET = "";

/**
 * The item's fields as labelled values (ux.md, The item §16): Need, Effort and Impact in three columns, then Due (a
 * reminder's Date), Tags and each parameter in a row. Each value is a button that opens its editor, when the item can be
 * edited: a popover under the value on a wide screen and a tablet, a sheet on a phone, but for Tags, which turns into
 * the tag picker in place. A change saves at once. An unset
 * effort, impact or due date the server says holds the rank back is marked, as that is where to fix it. Without `edit`
 * the values are only text.
 */
export function ItemFieldList({
  heapRef,
  item,
  entry,
  canEdit,
  save,
}: {
  heapRef: HeapRef;
  item: Item;
  entry: RankedItem | undefined;
  canEdit: boolean;
  save: (patch: ItemPatchRequest) => void;
}) {
  const parameters = useParameters(heapRef);
  const reminder = item.kind === "reminder";
  const heldBack = new Set(entry?.held_back_by.map((held) => held.gap));
  const due = item.due ? (item.due.overdue ? `Overdue · ${item.due.label}` : item.due.label) : null;

  return (
    <section
      aria-label="Fields"
      data-item-fields=""
      tabIndex={-1}
      className="flex flex-col gap-3 border-t border-line pt-3 outline-none"
    >
      <dl className="grid grid-cols-3 gap-2">
        <Field
          layout="cell"
          label="Need"
          value={NEED_LABELS[item.need]}
          editable={canEdit}
          wide={(close) => (
            <NeedPicker
              value={item.need}
              onChange={(need) => {
                if (need !== item.need) save({ need });
                close();
              }}
            />
          )}
          phone={(close) => (
            <NeedRows
              value={item.need}
              onChange={(need) => {
                if (need !== item.need) save({ need });
              }}
              onDone={close}
            />
          )}
        />
        {(["effort", "impact"] as const).map((name) => {
          const label = name === "effort" ? "Effort" : "Impact";
          const current = item[name] ?? null;
          return (
            <Field
              key={name}
              layout="cell"
              label={label}
              value={current && SIZE_LABELS[current]}
              empty={`Set ${name}`}
              unset="Not set"
              gap={current === null && heldBack.has("size")}
              editable={canEdit}
              wide={(close) => (
                <ChoiceButtons
                  label={label}
                  options={SIZE_OPTIONS}
                  value={current}
                  onChange={(size) => {
                    save({ [name]: size });
                    close();
                  }}
                />
              )}
              phone={(close) => (
                <OptionRows
                  label={label}
                  options={SIZE_OPTIONS}
                  value={current}
                  onChange={(size) => save({ [name]: size as Size | null })}
                  onDone={close}
                />
              )}
            />
          );
        })}
      </dl>

      <dl className="flex flex-col gap-1">
        <Field
          layout="row"
          label={reminder ? "Date" : "Due"}
          value={reminder ? (item.due ? formatDate(item.due.date) : null) : due}
          valueClass={item.due?.overdue ? "text-overdue-ink" : undefined}
          empty="Set due"
          unset="None"
          gap={!item.due && heldBack.has("due")}
          editable={canEdit}
          wide={(close) =>
            reminder ? (
              // A reminder's date is its deadline: an exact date that can be moved but not cleared.
              <DateField value={item.due?.date ?? null} onChange={(date) => save({ due: { date, firm: true } })} />
            ) : (
              <DuePicker
                value={item.due ? { date: item.due.date, firm: item.due.firm } : null}
                current={due ?? undefined}
                onChange={(value) => {
                  if (value?.date !== item.due?.date || value?.firm !== item.due?.firm) save({ due: value });
                  // A rough choice is the whole job; an exact date is typed or picked in place.
                  if (value && !value.firm) close();
                }}
              />
            )
          }
          phone={(close) =>
            reminder ? (
              <div className="pb-2">
                <DateField
                  wide
                  label="Date"
                  value={item.due?.date ?? null}
                  onChange={(date) => save({ due: { date, firm: true } })}
                />
              </div>
            ) : (
              <DueRows
                value={item.due ? { date: item.due.date, firm: item.due.firm } : null}
                onChange={(value) => {
                  if (value?.date !== item.due?.date || value?.firm !== item.due?.firm) save({ due: value });
                }}
                onDone={close}
              />
            )
          }
        />
        <Field
          layout="row"
          label="Tags"
          value={item.tags.length > 0 ? item.tags.map((tag) => tag.name).join(", ") : null}
          display={
            <span className="flex flex-wrap gap-1">
              {item.tags.map((tag) => (
                <span key={tag.id} className="rounded-md bg-sunken px-1.5 text-xs text-ink-soft">
                  {tag.name}
                </span>
              ))}
            </span>
          }
          unset="None"
          editable={canEdit}
          inPlace={(close) => (
            <TagStrip
              heapRef={heapRef}
              value={item.tags}
              onChange={(tags) => save({ tag_ids: tags.map((tag) => tag.id) })}
              close={close}
            />
          )}
        />
        {item.parameters.map((current) => {
          const parameter = parameters.data?.parameters.find((p) => p.id === current.parameter_id);
          // An archived parameter, or an archived option a closed item keeps, can only be looked at.
          const archivedOption =
            current.option !== null && !parameter?.options.some((option) => option.id === current.option?.id);
          const label = `${current.name}${current.parameter_archived ? " (archived)" : ""}`;
          const team = [...(parameters.data?.parameters ?? []), ...(parameters.data?.archived ?? [])].some(
            (p) => p.id === current.parameter_id && p.kind === "team",
          );
          return (
            <Field
              key={current.parameter_id}
              layout="row"
              label={label}
              mark={team ? <TeamMark of="parameter" /> : undefined}
              tone="param"
              value={current.option ? `${current.option.label}${archivedOption ? " (archived)" : ""}` : null}
              empty={`Set ${current.name}`}
              unset="Not set"
              editable={canEdit && parameter !== undefined && !archivedOption}
              wide={(close) =>
                parameter && (
                  <ParameterPicker
                    parameter={parameter}
                    value={current.option?.id ?? null}
                    onChange={(option) => {
                      save({ parameter_values: { [current.parameter_id]: option } });
                      close();
                    }}
                  />
                )
              }
              phone={(close) =>
                parameter && (
                  <OptionRows
                    label={parameter.name}
                    options={parameter.options}
                    value={current.option?.id ?? null}
                    onChange={(option) => save({ parameter_values: { [current.parameter_id]: option } })}
                    onDone={close}
                  />
                )
              }
              title={current.name}
            />
          );
        })}
      </dl>
    </section>
  );
}

/**
 * One field: its label and its value. Where the item can be edited the value is a button, named "Label: value", that
 * opens `wide`'s editor in a popover under it, or `phone`'s in a sheet; both are given `close`, for an editor whose
 * pick is the whole job. An `inPlace` editor instead takes the value's place in the row, on every screen. Focus returns
 * to the value when the editor closes, unless it went elsewhere. With a mouse the button shows nothing at
 * rest, a tint and ▾ when pointed at or focused; on a touch screen the ▾ always shows and the button is 44 px tall.
 * `value` is what it reads as when set, `empty` what it asks to be
 * set to while unset, and `unset` what it reads as unset where it can't be edited. A `gap` marks it as what the
 * server says holds the item back.
 */
function Field({
  layout,
  label,
  mark,
  tone = "ink",
  title,
  value,
  display,
  valueClass,
  empty,
  unset = "None",
  gap = false,
  editable,
  wide,
  phone,
  inPlace,
}: {
  layout: "cell" | "row";
  label: string;
  /** After the label, such as the mark of a team's parameter. */
  mark?: ReactNode;
  tone?: "ink" | "param";
  /** The editor's title, when it isn't the label. */
  title?: string;
  /** The value as text, or `null` while unset. */
  value: string | null;
  /** The value as it shows, when it is more than the text. */
  display?: ReactNode;
  valueClass?: string | undefined;
  empty?: string;
  unset?: string;
  gap?: boolean;
  editable: boolean;
} & (
  | { wide: (close: () => void) => ReactNode; phone: (close: () => void) => ReactNode; inPlace?: never }
  | { inPlace: (close: (returnFocus?: boolean) => void) => ReactNode; wide?: never; phone?: never }
)) {
  const onPhone = useMedia(PHONE);
  const focusAfterRender = useFocusAfterRender();
  const button = useRef<HTMLButtonElement>(null);
  // Focus finds the button by id, as `close` is handed to the editors and a function that holds a ref can't be.
  const buttonId = useId();
  const [open, setOpen] = useState(false);
  const shown = value ?? (editable ? (empty ?? unset) : unset);
  const close = (returnFocus = true) => {
    setOpen(false);
    if (returnFocus) focusAfterRender(() => document.getElementById(buttonId));
  };

  const valueSpan = (
    <span className={value === null ? "text-muted" : (valueClass ?? "text-ink")}>
      {value === null ? shown : (display ?? value)}
    </span>
  );
  const content = !editable ? (
    <span className="text-sm break-words">{valueSpan}</span>
  ) : inPlace && open ? (
    inPlace(close)
  ) : (
    <>
      <button
        ref={button}
        id={buttonId}
        type="button"
        aria-haspopup={inPlace ? undefined : "dialog"}
        aria-expanded={inPlace ? undefined : open}
        aria-label={`${label}: ${shown}${gap ? ", holds the rank back" : ""}`}
        data-gap={gap ? "true" : undefined}
        onClick={() => setOpen(true)}
        className={`group/value -mx-1.5 inline-flex min-h-8 max-w-full cursor-pointer items-center gap-1 rounded-md px-1.5 text-left text-sm outline-none hover:bg-sunken focus-visible:outline-2 focus-visible:outline-accent pointer-coarse:min-h-11 ${
          gap ? "rounded-full border border-accent bg-accent/10 font-medium" : ""
        }`}
      >
        {valueSpan}
        <ChevronDown
          aria-hidden="true"
          className="size-3.5 shrink-0 text-muted opacity-0 group-hover/value:opacity-100 group-focus-visible/value:opacity-100 pointer-coarse:opacity-100"
        />
      </button>
      {wide && (
        <PopoverSheet
          title={title ?? label}
          triggerRef={button}
          isOpen={open}
          onClose={close}
          placement="bottom start"
          size="narrow"
        >
          {onPhone ? phone?.(close) : wide(close)}
        </PopoverSheet>
      )}
    </>
  );

  const labelClass = `text-xs ${tone === "param" ? "text-param-ink" : "text-muted"}`;
  if (layout === "cell") {
    return (
      <div className="flex min-w-0 flex-col gap-0.5">
        <dt className={labelClass}>
          {label}
          {mark}
        </dt>
        <dd className="min-w-0">{content}</dd>
      </div>
    );
  }
  return (
    <div className="flex items-center gap-2">
      <dt className={`w-24 shrink-0 break-words ${labelClass}`}>
        {label}
        {mark}
      </dt>
      <dd className="min-w-0 flex-1">{content}</dd>
    </div>
  );
}

/**
 * The item's tags edited in place: the tag picker in the Tags row, with the box focused as it opens. Each change saves
 * at once, and a pick keeps it open for the next tag. On a phone the matches sit in the flow, as the item's sheet grows;
 * elsewhere they float under it, so the rows below stay put. It closes on Escape or Enter in the empty box, with focus
 * back on the value, and when focus leaves it, leaving focus where it went (or on the value, when it went nowhere). A
 * press inside it keeps the focus in the box, so removing a tag isn't leaving: Safari doesn't focus a pressed button,
 * and a Remove reached with Tab goes once the save lands, which would drop the focus. Left by a press elsewhere, it
 * closes when the press ends, not as it starts: the strip is taller than the value, and collapsing under a pressed
 * button below would move it from under the pointer and lose the click.
 */
function TagStrip({
  heapRef,
  value,
  onChange,
  close,
}: {
  heapRef: HeapRef;
  value: ItemTag[];
  onChange: (tags: ItemTag[]) => void;
  close: (returnFocus?: boolean) => void;
}) {
  const onPhone = useMedia(PHONE);
  const strip = useRef<HTMLDivElement>(null);
  const focusBox = () => strip.current?.querySelector("input")?.focus();
  // The person asked for the strip, and expects to type in it.
  useEffect(focusBox, []);
  const pressing = useRef(false);
  useEffect(() => {
    const down = () => (pressing.current = true);
    const up = () => (pressing.current = false);
    window.addEventListener("pointerdown", down, true);
    window.addEventListener("pointerup", up, true);
    window.addEventListener("pointercancel", up, true);
    return () => {
      window.removeEventListener("pointerdown", down, true);
      window.removeEventListener("pointerup", up, true);
      window.removeEventListener("pointercancel", up, true);
    };
  }, []);

  return (
    <div
      ref={strip}
      onMouseDown={(e) => {
        if (!(e.target instanceof HTMLInputElement)) e.preventDefault();
      }}
      onBlur={(e) => {
        // Switching to another window blurs it too; the strip stays for when the person comes back.
        if (strip.current?.contains(e.relatedTarget) || !document.hasFocus()) return;
        const returnFocus = e.relatedTarget === null;
        if (pressing.current) {
          window.addEventListener("pointerup", () => close(returnFocus), { once: true });
        } else {
          close(returnFocus);
        }
      }}
      // After Remove, whether clicked or pressed with the keyboard.
      onClick={focusBox}
      onKeyDown={(e) => {
        // The picker handles both keys while the box has text; in the empty box they mean done. Handled here, so
        // Escape doesn't also close the item or the sheet it is in.
        const box = e.target;
        if ((e.key === "Escape" || e.key === "Enter") && box instanceof HTMLInputElement && box.value === "") {
          e.preventDefault();
          e.stopPropagation();
          close();
        }
      }}
    >
      <TagPicker heapRef={heapRef} value={value} onChange={onChange} inline={onPhone} label={null} />
    </div>
  );
}

/** The four needs as 52 px rows with what each means under it, for a phone's sheet; a pick saves and closes. */
function NeedRows({ value, onChange, onDone }: { value: Need; onChange: (need: Need) => void; onDone: () => void }) {
  const id = useId();
  return (
    <ToggleButtonGroup
      aria-label="Need"
      selectionMode="single"
      orientation="vertical"
      selectedKeys={[value]}
      onSelectionChange={(keys) => {
        const [picked] = [...keys];
        // An empty selection is the picked row tapped again: nothing to change.
        if (picked !== undefined) onChange(String(picked) as Need);
        onDone();
      }}
      className="flex flex-col pb-1"
    >
      {NEEDS.map((need) => (
        <SheetChoice key={need} id={need} hint={{ id: `${id}-${need}`, text: NEED_HINTS[need] }}>
          {NEED_LABELS[need]}
        </SheetChoice>
      ))}
    </ToggleButtonGroup>
  );
}

/**
 * One option out of a list, as 52 px rows with a check on the picked one, and Not set to clear it. A radio group, so
 * the arrows move through it. Picking is the whole job, so `onDone` follows, also when the picked row is tapped again.
 */
function OptionRows({
  label,
  options,
  value,
  onChange,
  onDone,
}: {
  label: string;
  options: readonly { id: string; label: string }[];
  value: string | null;
  onChange: (value: string | null) => void;
  onDone: () => void;
}) {
  return (
    <ToggleButtonGroup
      aria-label={label}
      selectionMode="single"
      orientation="vertical"
      selectedKeys={[value ?? NOT_SET]}
      onSelectionChange={(keys) => {
        const [picked] = [...keys];
        // An empty selection is the picked row tapped again: nothing to change.
        if (picked !== undefined) onChange(picked === NOT_SET ? null : String(picked));
        onDone();
      }}
      className="flex flex-col pb-1"
    >
      {[...options, { id: NOT_SET, label: "Not set" }].map((option) => (
        <SheetChoice key={option.id} id={option.id} muted={option.id === NOT_SET}>
          {option.label}
        </SheetChoice>
      ))}
    </ToggleButtonGroup>
  );
}
