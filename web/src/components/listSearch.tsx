// The list's search text (P-search), held by the app shell rather than the list, so the phone's + in the tabs can
// tell Add what the list on screen hides. It belongs to one heap's list: leaving the list clears it, as before.
import { createContext, useContext, useState, type ReactNode } from "react";

type ListSearchState = { search: string; setSearch: (search: string) => void };

const ListSearchContext = createContext<ListSearchState>({ search: "", setSearch: () => {} });

/** Holds the search text of the list `listKey` names, or of none while no list is on screen. */
export function ListSearchProvider({ listKey, children }: { listKey: string | undefined; children: ReactNode }) {
  const [stored, setStored] = useState<{ listKey: string | undefined; search: string }>({ listKey, search: "" });
  // Another list, or none: the text starts empty again.
  if (stored.listKey !== listKey) setStored({ listKey, search: "" });
  const search = stored.listKey === listKey ? stored.search : "";
  return (
    <ListSearchContext.Provider value={{ search, setSearch: (text) => setStored({ listKey, search: text }) }}>
      {children}
    </ListSearchContext.Provider>
  );
}

/** The list's search text, and the way to change it. */
export const useListSearch = (): ListSearchState => useContext(ListSearchContext);
