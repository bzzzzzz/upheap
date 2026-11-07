// What the local build shows about its database, which lives in this browser (design/local-build.md). In the server
// build `localDatabase` is null and these render their children or nothing.
import { localDatabase, type LocalDatabase } from "#local-database";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useRef, useState, useSyncExternalStore, type ReactNode } from "react";

import { primaryButtonClass, quietButtonClass } from "./NameForm";
import { ProblemText } from "./ProblemText";

/** The app, or while another tab has the database, why it can't open here. */
export function LocalDatabaseGate({ children }: { children: ReactNode }) {
  if (!localDatabase) return children;
  return <Gate database={localDatabase}>{children}</Gate>;
}

function Gate({ database, children }: { database: LocalDatabase; children: ReactNode }) {
  const status = useSyncExternalStore(database.subscribe, database.status);
  if (status !== "waiting") return children;
  return (
    <main className="mx-auto max-w-md px-4 py-24 text-center">
      <h1 className="mb-3 text-xl font-semibold tracking-tight">Upheap is open in another tab</h1>
      <p className="text-sm text-stone-600 dark:text-stone-300">
        Your heaps live in one place in this browser, so Upheap runs in one tab at a time. Close the other tab, and
        Upheap opens here.
      </p>
    </main>
  );
}

/** Where the data lives, and whether the browser may clear it. Asks the browser to keep it. */
export function StorageNote() {
  if (!localDatabase) return null;
  return <Note database={localDatabase} />;
}

function Note({ database }: { database: LocalDatabase }) {
  const { data: kept } = useQuery({
    queryKey: ["local-database", "kept"],
    queryFn: database.keep,
    staleTime: Infinity,
  });
  return (
    <footer className="mt-16 border-t border-stone-200 pt-4 text-xs text-stone-500 dark:border-stone-800">
      Your heaps are stored only in this browser
      {kept === false && ", which may clear them when it runs low on space"}.{" "}
      <Link to="/account" className="underline hover:text-stone-900 dark:hover:text-stone-100">
        Download a copy
      </Link>{" "}
      to keep them safe or move them to another browser.
    </footer>
  );
}

/** The Account page's export and import of the database. */
export function LocalDataSection() {
  if (!localDatabase) return null;
  return <DataSection database={localDatabase} />;
}

function DataSection({ database }: { database: LocalDatabase }) {
  const download = useMutation({
    mutationFn: () => database.exportFile(),
    onSuccess: (file) => {
      const link = document.createElement("a");
      link.href = URL.createObjectURL(file);
      link.download = `upheap-${new Date().toISOString().slice(0, 10)}.sqlite3`;
      link.click();
      URL.revokeObjectURL(link.href);
    },
  });
  const replace = useMutation({
    mutationFn: (file: File) => database.importFile(file),
    // Everything on screen came from the database it replaced, so start again from the top.
    onSuccess: () => window.location.assign(import.meta.env.BASE_URL),
  });
  const input = useRef<HTMLInputElement>(null);
  const [chosen, setChosen] = useState<File | null>(null);

  const cancel = () => {
    setChosen(null);
    if (input.current) input.current.value = "";
  };

  return (
    <section aria-labelledby="local-data" className="mt-10 max-w-md">
      <h2 id="local-data" className="mb-1 text-sm font-medium">
        Your data
      </h2>
      <p className="mb-3 text-sm text-stone-600 dark:text-stone-300">
        Your heaps are stored only in this browser. A copy is one SQLite file that you can open here or in another
        browser.
      </p>
      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          disabled={download.isPending}
          onClick={() => download.mutate()}
          className={primaryButtonClass}
        >
          Download a copy
        </button>
        <label className={`${quietButtonClass} cursor-pointer`}>
          Open a copy…
          <input
            ref={input}
            type="file"
            accept=".sqlite3,.sqlite,.db,application/vnd.sqlite3"
            className="sr-only"
            onChange={(event) => {
              replace.reset();
              setChosen(event.target.files?.[0] ?? null);
            }}
          />
        </label>
        <ProblemText error={download.error} />
      </div>
      {chosen && (
        <div
          role="group"
          aria-label="Replace your data"
          className="mt-3 flex flex-wrap items-center gap-2 rounded-md border border-amber-300 bg-amber-50 px-3 py-2 text-sm text-amber-900 dark:border-amber-800 dark:bg-amber-950 dark:text-amber-200"
        >
          <p className="basis-full">
            Replace everything here with {chosen.name}? What's here now will be gone, so download a copy first if you
            want to keep it.
          </p>
          <button
            type="button"
            disabled={replace.isPending}
            onClick={() => replace.mutate(chosen)}
            className={primaryButtonClass}
          >
            Replace
          </button>
          <button type="button" disabled={replace.isPending} onClick={cancel} className={quietButtonClass}>
            Cancel
          </button>
          <ProblemText error={replace.error} />
        </div>
      )}
    </section>
  );
}
