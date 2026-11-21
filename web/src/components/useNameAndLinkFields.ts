import { useEffect, useRef, useState } from "react";

import { ApiProblem } from "../api/client";
import { problemMessage } from "./ProblemText";

export type NameAndLinkField = "name" | "slug";

/**
 * The Name and Link name fields of a form that makes something with a link name (Create team, New heap). A problem
 * about either field shows under it and takes focus; typing in that field clears it. `slug` is the trimmed link name,
 * blank when none is given, so the server makes one from the name.
 */
export function useNameAndLinkFields(mutation: { error: Error | null; reset: () => void }) {
  const [values, setValues] = useState<Record<NameAndLinkField, string>>({ name: "", slug: "" });
  const formRef = useRef<HTMLFormElement>(null);
  const { error } = mutation;
  const errorField: NameAndLinkField | undefined =
    error instanceof ApiProblem && (error.field === "name" || error.field === "slug") ? error.field : undefined;

  // After a failed submit, focus goes to the input the problem is about.
  useEffect(() => {
    const input = errorField && formRef.current?.elements.namedItem(errorField);
    if (input instanceof HTMLInputElement) input.focus();
  }, [error, errorField]);

  const field = (name: NameAndLinkField) => ({
    name,
    value: values[name],
    error: errorField === name && error ? problemMessage(error) : undefined,
    onChange: (value: string) => {
      setValues((current) => ({ ...current, [name]: value }));
      if (errorField === name) mutation.reset();
    },
  });

  return { formRef, field, name: values.name, slug: values.slug.trim(), otherError: errorField ? null : error };
}
