// Display names for enum values, and which states are closed. Anything computed about an item comes from the server.
import type { AuditAction, ItemState, Size } from "../api/client";

export const SIZES: readonly Size[] = ["s", "m", "l", "xl"];

export const SIZE_LABELS: Record<Size, string> = { s: "S", m: "M", l: "L", xl: "XL" };

export const STATE_LABELS: Record<ItemState, string> = {
  open: "Open",
  in_progress: "In progress",
  blocked: "Blocked",
  done: "Done",
  wont_do: "Won't do",
};

export const STATES: readonly ItemState[] = ["open", "in_progress", "blocked", "done", "wont_do"];

/** Done and won't do are the closed states. */
export const isClosed = (state: ItemState) => state === "done" || state === "wont_do";

/** What each audit action says about its entity, read after the actor's name: "Ada edited item Rotate keys". */
export const AUDIT_ACTION_LABELS: Record<AuditAction, string> = {
  user_registered: "signed up",
  user_updated: "updated the profile of",
  team_created: "created team",
  team_updated: "renamed team",
  member_added: "added member",
  member_role_changed: "changed the role of",
  member_removed: "removed member",
  member_left: "left the team",
  heap_created: "created heap",
  heap_updated: "renamed heap",
  heap_archived: "archived heap",
  heap_unarchived: "unarchived heap",
  item_created: "added item",
  item_updated: "edited item",
  tag_created: "created tag",
  tag_updated: "edited tag",
  saved_view_created: "saved view",
  saved_view_updated: "changed view",
  saved_view_archived: "archived view",
  parameter_created: "added parameter",
  parameter_updated: "changed parameter",
  parameter_archived: "archived parameter",
  option_archived: "archived an option of",
};
