import { useParams, useRouterState } from "@tanstack/react-router";
import { X } from "lucide-react";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";

/**
 * A message about a change just made, with a way to take it back, and sometimes the likely next step as `action`.
 * `id` tells one toast from the next.
 */
export type Toast = {
  id: number;
  message: string;
  undo?: () => void;
  action?: { label: string; run: () => void } | undefined;
  error?: boolean;
};

/** Shows a toast in place of the one on screen, if any. */
type ShowToast = (toast: Omit<Toast, "id">) => void;

const ToastContext = createContext<{ show: ShowToast; count: () => number }>({
  show: () => {
    throw new Error("useToast needs a ToastProvider around it");
  },
  count: () => 0,
});

/**
 * The app's one toast, shared by everything that shows one (the list's state changes, Add), so a new toast replaces
 * the last instead of stacking on it. A toast belongs to the heap it was shown in: going to another heap, or to a
 * page of no heap, puts it away. Opening, walking and closing items keeps it, as the list stays under them.
 */
export function ToastProvider({ children }: { children: ReactNode }) {
  const { team, heap } = useParams({ strict: false });
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const owner = team !== undefined && heap !== undefined ? `heap:${team}/${heap}` : pathname;
  const [shown, setShown] = useState<{ toast: Toast; owner: string } | null>(null);
  // How many toasts have been shown, for a toast that's only worth showing if none came since.
  const shownCount = useRef(0);
  const show = useCallback<ShowToast>(
    (toast) => {
      shownCount.current += 1;
      setShown({ toast: { ...toast, id: Date.now() }, owner });
    },
    [owner],
  );
  const value = useMemo(() => ({ show, count: () => shownCount.current }), [show]);
  const dismiss = useCallback(() => setShown(null), []);
  // Put away for good once the heap changes, so coming back doesn't bring it back.
  if (shown && shown.owner !== owner) setShown(null);
  return (
    <ToastContext.Provider value={value}>
      {children}
      <UndoToast toast={shown?.owner === owner ? shown.toast : null} onDismiss={dismiss} />
    </ToastContext.Provider>
  );
}

/** Shows a toast: from a component under the `ToastProvider`. */
export const useToast = (): ShowToast => useContext(ToastContext).show;

/** How many toasts have been shown so far, read when called. */
export const useToastCount = (): (() => number) => useContext(ToastContext).count;

/** How long a toast stays. */
const SHOW_FOR_MS = 6000;

/**
 * The toast after a change from the list (app-ux.md, The list §3): bottom centre, above the phone's tabs. It goes
 * after a few seconds, or when Undo or ✕ is pressed. Under the Add dialog and the phone's sheets, so a toast from the
 * last item added never covers the dialog's buttons.
 */
function UndoToast({ toast, onDismiss }: { toast: Toast | null; onDismiss: () => void }) {
  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(onDismiss, SHOW_FOR_MS);
    return () => window.clearTimeout(timer);
  }, [toast, onDismiss]);

  return (
    <div
      role={toast?.error ? "alert" : "status"}
      className="pointer-events-none fixed inset-x-0 bottom-[calc(5rem+env(safe-area-inset-bottom))] z-20 flex justify-center px-4 sm:bottom-6"
    >
      {toast && (
        <div
          key={toast.id}
          className={`pointer-events-auto flex max-w-md min-w-0 items-center gap-1 rounded-xl py-1.5 pr-1.5 pl-4 text-sm shadow-lg ${
            toast.error ? "bg-overdue-soft text-overdue-ink" : "bg-ink text-canvas"
          }`}
        >
          <span className="mr-2 min-w-0 flex-1">{toast.message}</span>
          {toast.undo && (
            <button
              type="button"
              onClick={() => {
                toast.undo?.();
                onDismiss();
              }}
              className="h-9 shrink-0 rounded-lg px-3 font-semibold text-accent hover:bg-white/10 pointer-coarse:h-11"
            >
              Undo
            </button>
          )}
          {toast.action && (
            <button
              type="button"
              onClick={() => {
                toast.action?.run();
                onDismiss();
              }}
              className="h-9 max-w-48 min-w-0 shrink truncate rounded-lg px-3 font-semibold text-accent hover:bg-white/10 pointer-coarse:h-11"
            >
              {toast.action.label}
            </button>
          )}
          <button
            type="button"
            onClick={onDismiss}
            aria-label="Dismiss"
            className="flex size-9 shrink-0 items-center justify-center rounded-lg opacity-70 hover:opacity-100 pointer-coarse:size-11"
          >
            <X aria-hidden="true" className="size-4" />
          </button>
        </div>
      )}
    </div>
  );
}
