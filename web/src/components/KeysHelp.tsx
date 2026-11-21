import { Dialog, Heading, Modal, ModalOverlay } from "react-aria-components";

import { compactButtonClass } from "./buttons";
import { Kbd } from "./Kbd";

/** Every key, by where it works. `keys` are the chips, one per alternative, shown as written. */
const GROUPS: { title: string; keys: { keys: string[]; does: string }[] }[] = [
  {
    title: "Anywhere",
    keys: [
      { keys: ["?"], does: "Show these keys" },
      { keys: ["G O"], does: "Go to Open" },
      { keys: ["G C"], does: "Go to Closed" },
    ],
  },
  {
    title: "The list",
    keys: [
      { keys: ["↑ ↓"], does: "Move between items" },
      { keys: ["↵"], does: "Open the item" },
      { keys: ["S"], does: "Start" },
      { keys: ["B"], does: "Block" },
      { keys: ["X"], does: "Mark done" },
      { keys: ["A"], does: "Send to an agent, one step" },
      { keys: ["N"], does: "Add an item" },
      { keys: ["/"], does: "Search" },
      { keys: ["Esc"], does: "Close the item" },
      { keys: ["⇧ ↑ ↓"], does: "Select items, in select mode" },
    ],
  },
  {
    title: "An open item",
    keys: [
      { keys: ["↑ ↓"], does: "Previous or next item" },
      { keys: ["Esc"], does: "Close, or leave full screen" },
    ],
  },
  {
    title: "Adding an item",
    keys: [
      { keys: ["⌘↵", "Ctrl ↵"], does: "Add" },
      { keys: ["Esc"], does: "Cancel" },
    ],
  },
];

/** The list of every key (ux.md, Keyboard), opened by `?` anywhere in the app. */
export function KeysHelp({ isOpen, onOpenChange }: { isOpen: boolean; onOpenChange: (open: boolean) => void }) {
  return (
    <ModalOverlay
      isOpen={isOpen}
      onOpenChange={onOpenChange}
      isDismissable
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 px-4 pt-[10vh]"
    >
      <Modal className="max-h-[80dvh] w-full max-w-[480px] overflow-y-auto rounded-2xl bg-surface p-[22px] text-ink shadow-xl outline-none">
        <Dialog className="flex flex-col gap-4 outline-none">
          {({ close }) => (
            <>
              <Heading slot="title" className="text-base font-semibold">
                Keyboard shortcuts
              </Heading>
              {GROUPS.map((group) => (
                <section key={group.title} aria-label={group.title}>
                  <h3 className="mb-1.5 text-xs font-medium text-muted">{group.title}</h3>
                  <dl className="flex flex-col gap-1.5 text-sm">
                    {group.keys.map(({ keys, does }) => (
                      <div key={does} className="flex items-center gap-3">
                        <dt className="flex w-32 shrink-0 items-center gap-1.5">
                          {keys.map((key, at) => (
                            <span key={key} className="flex items-center gap-1.5">
                              {at > 0 && <span className="text-xs text-muted">or</span>}
                              <Kbd readable>{key}</Kbd>
                            </span>
                          ))}
                        </dt>
                        <dd className="text-ink-soft">{does}</dd>
                      </div>
                    ))}
                  </dl>
                </section>
              ))}
              <div className="flex justify-end">
                <button type="button" autoFocus onClick={close} className={compactButtonClass.quiet}>
                  Close
                </button>
              </div>
            </>
          )}
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
