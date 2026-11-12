import { createContext, useContext, type Dispatch, type SetStateAction } from "react";

/**
 * Saves the context's changed draft ahead of a change that would close the item, and answers whether the change may go
 * on: false when the draft could not be saved (someone else changed the text, or the save failed), with the form
 * saying why, so nothing typed is thrown away by the change.
 */
export type FlushDraft = () => Promise<boolean>;

/**
 * Where the open context form registers its `FlushDraft`, held as state by the item's fields (not a ref passed down),
 * so the state-change path can call it. `null` while no form is open.
 */
export const ContextDraft = createContext<Dispatch<SetStateAction<FlushDraft | null>> | null>(null);

export const useRegisterDraft = () => useContext(ContextDraft);
