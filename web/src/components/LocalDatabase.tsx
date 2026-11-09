// What the local build shows about its database, which lives in this browser (design/architecture.md, Local build). In the server
// build `localDatabase` is null and these render their children or nothing.
import { localDatabase, type DatabaseCounts, type LocalDatabase } from "#local-database";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { Check, Download } from "lucide-react";
import { useId, useRef, useState, useSyncExternalStore, type ReactNode } from "react";
import { Dialog, Heading, Modal, ModalOverlay } from "react-aria-components";

import { formatAgo, formatDay, isDownloadStale, recordDownload, useLastDownloadAt, useNow } from "./lastDownload";
import { PHONE, useMedia } from "./media";
import { borderedClass, buttonClass, compactButtonClass, unborderedClass } from "./buttons";
import { problemMessage } from "./ProblemText";
import { Sheet } from "./Sheet";

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

/**
 * Where the data lives (screens.md, G-storage-note), whether the browser may clear it, and a nudge when no copy was
 * downloaded in two weeks. Asks the browser to keep the data.
 */
export function StorageNote({ className }: { className: string }) {
  if (!localDatabase) return null;
  return <Note database={localDatabase} className={className} />;
}

function Note({ database, className }: { database: LocalDatabase; className: string }) {
  const { data: kept } = useQuery({
    queryKey: ["local-database", "kept"],
    queryFn: database.keep,
    staleTime: Infinity,
  });
  const lastAt = useLastDownloadAt();
  const now = useNow();
  const stale = isDownloadStale(lastAt, now);
  const link = (
    <Link to="/account" className="font-medium underline underline-offset-2 hover:text-ink">
      Download a copy
    </Link>
  );
  return (
    <footer className={`text-xs leading-[1.45] text-ink-soft ${className}`}>
      {kept !== false && !stale ? (
        <>Stored only in this browser · {link}</>
      ) : (
        <>
          Stored only in this browser{kept === false && ", which may clear it when space runs low"}.{" "}
          {stale && (lastAt === null ? "No copy downloaded yet. " : `Last copy downloaded ${formatAgo(lastAt, now)}. `)}
          {link}
        </>
      )}
    </footer>
  );
}

/** The Your data card's buttons, sized as in the WideAccount mockup. */
const cardButtonClass = `${buttonClass} rounded-[10px] px-3.5 py-[9px] max-sm:min-h-11`;
const downloadClass = `${cardButtonClass} ${unborderedClass} bg-ink font-semibold text-surface`;
const openClass = `${cardButtonClass} ${borderedClass}`;

/** The phone sheet's buttons: full width and 44 px tall. */
const sheetButtonClass =
  "inline-flex h-11 w-full items-center justify-center gap-2 rounded-[10px] px-4 text-sm font-medium disabled:opacity-40";
const sheetClass = {
  quiet: `${sheetButtonClass} text-ink-soft hover:bg-sunken`,
  bordered: `${sheetButtonClass} ${borderedClass}`,
  danger: `${sheetButtonClass} bg-overdue-ink font-semibold text-surface`,
};

/**
 * Numbers each picked file, so picking the same file again counts it again. Module-wide, so a count still running for
 * a pick made before the card remounted can't land under a later pick's key.
 */
let picks = 0;

/** "1 heap", "2 heaps". */
function plural(count: number, one: string, many: string): string {
  return `${count} ${count === 1 ? one : many}`;
}

/** Downloads the database as `upheap-YYYY-MM-DD.sqlite3` and remembers when, for the storage note. */
function useDownloadCopy(database: LocalDatabase) {
  return useMutation({
    mutationFn: () => database.exportFile(),
    onSuccess: (file) => {
      const link = document.createElement("a");
      link.href = URL.createObjectURL(file);
      link.download = `upheap-${new Date().toISOString().slice(0, 10)}.sqlite3`;
      link.click();
      // Revoked a moment later: revoking at once can cancel the download before it starts.
      setTimeout(() => URL.revokeObjectURL(link.href), 10_000);
      // A Save As the user cancels can't be told apart, so the click counts as the download.
      recordDownload(Date.now());
    },
  });
}

/** What the database in this browser holds, kept current by refetching whenever it's shown. */
function useCurrentCounts(database: LocalDatabase) {
  return useQuery({ queryKey: ["local-database", "count"], queryFn: () => database.count() });
}

/** The Account page's Your data card (screens.md, S-account): download a copy, or open one in place of the data. */
export function LocalDataSection() {
  if (!localDatabase) return null;
  return <DataSection database={localDatabase} />;
}

function DataSection({ database }: { database: LocalDatabase }) {
  const download = useDownloadCopy(database);
  const current = useCurrentCounts(database);
  const lastAt = useLastDownloadAt();
  const now = useNow();
  const input = useRef<HTMLInputElement>(null);
  const [chosen, setChosen] = useState<{ file: File; pick: number } | null>(null);

  const close = () => {
    setChosen(null);
    if (input.current) input.current.value = "";
  };

  return (
    <section aria-labelledby="local-data" className="rounded-[14px] border border-line bg-surface p-5">
      <h2 id="local-data" className="text-base font-semibold">
        Your data
      </h2>
      <p className="mt-1.5 text-sm leading-relaxed text-ink-soft">
        Everything is kept in this browser only. Clearing site data, or a browser that removes it, loses it. Download a
        copy now and then, and open it here or in another browser to carry on.
      </p>
      <div className="mt-4 flex flex-wrap items-center gap-2.5">
        <button type="button" disabled={download.isPending} onClick={() => download.mutate()} className={downloadClass}>
          <Download aria-hidden="true" className="size-4" />
          Download a copy
        </button>
        {/* The input itself takes focus, so its label shows the focus ring. */}
        <label
          className={`${openClass} cursor-pointer has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-2 has-[:focus-visible]:outline-accent`}
        >
          Open a copy…
          <input
            ref={input}
            type="file"
            accept=".sqlite3,.sqlite,.db,application/vnd.sqlite3"
            className="sr-only"
            onChange={(event) => {
              const file = event.target.files?.[0];
              if (!file) return;
              // The confirm gives focus back to what had it when it opened, so make that the input.
              event.target.focus();
              setChosen({ file, pick: ++picks });
            }}
          />
        </label>
      </div>
      {download.error && (
        <p role="alert" className="mt-3 text-sm text-overdue-ink">
          {problemMessage(download.error)}
        </p>
      )}
      <p className="mt-3 text-xs text-muted">
        {current.data &&
          `${plural(current.data.heaps, "heap", "heaps")} · ${plural(current.data.items, "item", "items")} · `}
        {lastAt === null ? "never downloaded" : `last downloaded ${formatDay(lastAt, now)}`}
      </p>
      {chosen && (
        <ReplaceConfirm
          key={chosen.pick}
          database={database}
          file={chosen.file}
          pick={chosen.pick}
          current={current.data}
          onClose={close}
        />
      )}
    </section>
  );
}

/**
 * Asks before a copy replaces everything (screens.md, M-local-import-confirm), saying what the copy holds and what
 * goes: a modal on a wide screen and a sheet on a phone. A file that isn't an Upheap database is refused here,
 * before anything changes.
 */
function ReplaceConfirm({
  database,
  file,
  pick,
  current,
  onClose,
}: {
  database: LocalDatabase;
  file: File;
  pick: number;
  current: DatabaseCounts | undefined;
  onClose: () => void;
}) {
  const phone = useMedia(PHONE);
  const now = useNow();
  const copy = useQuery({
    queryKey: ["local-database", "copy", pick],
    queryFn: () => database.count(file),
    // A file that isn't a database won't become one.
    retry: false,
    staleTime: Infinity,
    gcTime: 0,
  });
  const download = useDownloadCopy(database);
  const replace = useMutation({
    mutationFn: () => database.importFile(file),
    // Everything on screen came from the database it replaced, so start again from the top.
    onSuccess: () => window.location.assign(import.meta.env.BASE_URL),
  });
  const title = `Replace everything here with ${file.name}?`;
  const bodyId = useId();
  const requestClose = () => {
    if (!replace.isPending) onClose();
  };

  const message = copy.isPending ? (
    <p className="text-sm text-ink-soft">Reading the copy…</p>
  ) : copy.isError ? (
    <p role="alert" className="text-sm text-overdue-ink">
      {problemMessage(copy.error)}
    </p>
  ) : (
    <p className="text-sm leading-relaxed text-ink-soft">
      The copy has {plural(copy.data.heaps, "heap", "heaps")} and {plural(copy.data.items, "item", "items")}
      {copy.data.lastChangedAt !== null && `, from ${formatDay(copy.data.lastChangedAt, now)}`}. What&apos;s in this
      browser now{current && ` (${plural(current.items, "item", "items")})`} is replaced and can&apos;t be brought back.
    </p>
  );
  // The dialog's description, and a live region, so the counts are read out when they replace "Reading the copy…".
  const body = (
    <div id={bodyId} aria-live="polite">
      {message}
    </div>
  );
  const buttonClasses = phone ? sheetClass : compactButtonClass;
  const buttons = (
    <div className={phone ? "flex flex-col-reverse gap-2 pt-3" : "flex flex-wrap items-center justify-end gap-2"}>
      <button
        type="button"
        autoFocus
        disabled={replace.isPending}
        onClick={requestClose}
        className={buttonClasses.quiet}
      >
        Cancel
      </button>
      {copy.isSuccess && (
        <>
          <button
            type="button"
            disabled={download.isPending || replace.isPending}
            onClick={() => {
              replace.reset();
              download.mutate();
            }}
            className={buttonClasses.bordered}
          >
            {download.isSuccess ? (
              <>
                <Check aria-hidden="true" className="size-4 text-done" strokeWidth={2.5} />
                Downloaded
              </>
            ) : (
              "Download current first"
            )}
          </button>
          <button
            type="button"
            disabled={replace.isPending}
            onClick={() => {
              download.reset();
              replace.mutate();
            }}
            className={buttonClasses.danger}
          >
            Replace
          </button>
        </>
      )}
    </div>
  );
  const failure = download.error ?? replace.error;
  const problem = failure && (
    <p role="alert" className={`text-sm text-overdue-ink ${phone ? "mt-3" : ""}`}>
      {problemMessage(failure)}
    </p>
  );

  if (phone) {
    return (
      <Sheet title={title} role="alertdialog" describedBy={bodyId} isOpen onClose={requestClose}>
        <div className="flex flex-col pb-1">
          {body}
          {problem}
          {buttons}
        </div>
      </Sheet>
    );
  }
  return (
    <ModalOverlay
      isOpen
      onOpenChange={(open) => !open && requestClose()}
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 px-4 pt-[18vh]"
    >
      <Modal className="w-full max-w-[440px] rounded-2xl bg-surface p-[22px] text-ink shadow-xl outline-none">
        <Dialog role="alertdialog" aria-describedby={bodyId} className="flex flex-col gap-3.5 outline-none">
          <Heading slot="title" className="text-base font-semibold break-words">
            {title}
          </Heading>
          {body}
          {problem}
          {buttons}
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
