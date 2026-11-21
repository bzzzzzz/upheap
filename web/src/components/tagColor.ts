import type { TagColor } from "../api/schema";

/**
 * How a tag's colour looks: border, text, text under the pointer and the dot of a suggestion. Tailwind finds a class
 * only when it is written out, so each colour's classes are literal here; the tokens they name are in index.css.
 */
const classes: Record<TagColor, { border: string; text: string; hover: string; dot: string }> = {
  blue: {
    border: "border-tag-blue",
    text: "text-tag-blue-ink",
    hover: "hover:text-tag-blue-ink-hover",
    dot: "bg-tag-blue",
  },
  sky: { border: "border-tag-sky", text: "text-tag-sky-ink", hover: "hover:text-tag-sky-ink-hover", dot: "bg-tag-sky" },
  teal: {
    border: "border-tag-teal",
    text: "text-tag-teal-ink",
    hover: "hover:text-tag-teal-ink-hover",
    dot: "bg-tag-teal",
  },
  green: {
    border: "border-tag-green",
    text: "text-tag-green-ink",
    hover: "hover:text-tag-green-ink-hover",
    dot: "bg-tag-green",
  },
  lime: {
    border: "border-tag-lime",
    text: "text-tag-lime-ink",
    hover: "hover:text-tag-lime-ink-hover",
    dot: "bg-tag-lime",
  },
  rose: {
    border: "border-tag-rose",
    text: "text-tag-rose-ink",
    hover: "hover:text-tag-rose-ink-hover",
    dot: "bg-tag-rose",
  },
  pink: {
    border: "border-tag-pink",
    text: "text-tag-pink-ink",
    hover: "hover:text-tag-pink-ink-hover",
    dot: "bg-tag-pink",
  },
  grey: {
    border: "border-tag-grey",
    text: "text-tag-grey-ink",
    hover: "hover:text-tag-grey-ink-hover",
    dot: "bg-tag-grey",
  },
};

/** The palette in the order the server picks from and the colour picker lists, with the names screens show. */
export const tagColors: { code: TagColor; label: string }[] = [
  { code: "blue", label: "Blue" },
  { code: "sky", label: "Sky" },
  { code: "teal", label: "Teal" },
  { code: "green", label: "Green" },
  { code: "lime", label: "Lime" },
  { code: "rose", label: "Rose" },
  { code: "pink", label: "Pink" },
  { code: "grey", label: "Grey" },
];

/** A chip's border and text in the tag's colour, with no fill. */
export const tagChipColor = (color: TagColor) => `${classes[color].border} ${classes[color].text}`;

/** [`tagChipColor`] and the text colour under the pointer, for a chip that is a button. */
export const tagButtonColor = (color: TagColor) => `${tagChipColor(color)} ${classes[color].hover}`;

/** The small round dot in the tag's border colour. */
export const tagDotColor = (color: TagColor) => classes[color].dot;
