import { ApiProblem } from "../api/client";

/** Messages for the errors a change to an item can bring back, by code. */
const PROBLEM_MESSAGES: Record<string, string> = {
  version_conflict: "Someone else changed this item meanwhile. It now shows their version.",
  blocking_cycle: "That item already waits for this one, so it can't block it.",
  blocker_closed: "That item is closed, so nothing can wait for it.",
  blocker_not_in_heap: "That item isn't in this heap any more.",
  kind_change_not_allowed: "A task can't go back to a note.",
  stage_needs_task: "Only a task can be defined.",
  item_not_blocked: "Only a blocked item can wait for another one.",
  item_closed: "This item is closed, so only its state can change.",
  option_not_in_parameter: "That option was archived meanwhile. Pick another one.",
  parameter_not_in_heap: "That parameter was archived meanwhile.",
  steps_need_task: "Only a task has steps. Make it a task first.",
  too_many_steps: "A task has at most 100 steps.",
  step_not_in_item: "That step isn't in this task any more.",
  reopen_window_expired: "This item was closed more than a week ago, so it can't be reopened.",
};

/** What to tell the user about a failed change to an item. */
export const itemProblem = (error: Error) =>
  (error instanceof ApiProblem && PROBLEM_MESSAGES[error.code]) || error.message;
