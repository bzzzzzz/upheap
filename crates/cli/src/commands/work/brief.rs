//! The brief a run hands the agent tool: the instructions for the run's goal, the runner's own when `work run` was
//! given some, the heap's context, the item as `item show` prints it, and what this run is. A plain function over plain values, so it is tested without a server.

use api_types::turns::GoalDto;

use crate::agent::Access;

const COMMON: &str = include_str!("instructions/common.md");
const WITH_CONTEXT: &str = include_str!("instructions/with_context.md");
const DEFINED: &str = include_str!("instructions/defined.md");
const DONE: &str = include_str!("instructions/done.md");

/// A goal this version knows how to work towards. A turn with any other goal is not worked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KnownGoal {
    WithContext,
    Defined,
    Done,
}

impl KnownGoal {
    /// `None` for a goal a newer server added.
    pub const fn from_dto(goal: GoalDto) -> Option<Self> {
        match goal {
            GoalDto::WithContext => Some(Self::WithContext),
            GoalDto::Defined => Some(Self::Defined),
            GoalDto::Done => Some(Self::Done),
            GoalDto::Unknown => None,
        }
    }

    /// What the tool may do to reach it: prepare an item with `upheap`, files and the web, or do the task.
    pub const fn access(self) -> Access {
        match self {
            Self::WithContext | Self::Defined => Access::Prepare,
            Self::Done => Access::Do,
        }
    }

    const fn phrase(self) -> &'static str {
        match self {
            Self::WithContext => "with context",
            Self::Defined => "defined",
            Self::Done => "done",
        }
    }

    const fn instructions(self) -> &'static str {
        match self {
            Self::WithContext => WITH_CONTEXT,
            Self::Defined => DEFINED,
            Self::Done => DONE,
        }
    }
}

/// What a brief is made of.
pub(super) struct BriefParts<'a> {
    pub goal: KnownGoal,
    /// What the person running the loop wrote for every run of theirs, from `work run --instructions`.
    pub runner_instructions: Option<&'a str>,
    pub heap_name: &'a str,
    /// The heap as `team/heap`.
    pub heap_handle: &'a str,
    /// The heap's context as written; `None` or empty when nobody has written one.
    pub heap_context: Option<&'a str>,
    /// What `item show` prints for the item.
    pub item_text: &'a str,
    /// What the person told the agent with this turn.
    pub line: Option<&'a str>,
    /// How many runs the turn has had, this one included.
    pub runs: u32,
    /// How many of them in a row failed.
    pub failed_runs: u32,
}

/// The text the tool reads on its standard input.
pub(super) fn build_brief(parts: &BriefParts<'_>) -> String {
    let context = parts
        .heap_context
        .map(str::trim)
        .filter(|context| !context.is_empty())
        .unwrap_or("This heap has no context.");
    let runner = parts
        .runner_instructions
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(|text| format!("{RUNNER_INTRO}\n\n{text}\n\n"))
        .unwrap_or_default();
    let mut brief = format!(
        "{common}\n\n{goal}\n\n{runner}# The heap: {name} ({handle})\n\n{context}\n\n# The item\n\n{item}\n\n# This run\n\nGoal: {phrase}.\n",
        common = COMMON.trim_end(),
        goal = parts.goal.instructions().trim_end(),
        name = parts.heap_name,
        handle = parts.heap_handle,
        item = parts.item_text.trim_end(),
        phrase = parts.goal.phrase(),
    );
    if let Some(line) = parts.line.filter(|line| !line.is_empty()) {
        brief.push_str(&format!("The person said: \"{line}\"\n"));
    }
    if parts.runs > 1 {
        brief.push_str(&earlier_runs(parts.runs - 1, parts.failed_runs));
    }
    brief
}

/// What comes before the runner's instructions in a brief. The heap's context is shared by everyone who works the
/// heap, and these are one runner's own, so on how the work is done they come first.
const RUNNER_INTRO: &str = "# The runner's instructions

The person who runs this loop wrote what follows, for every run on this machine: how they want work done here. Follow
it. Where it and the heap's context disagree on how to do the work, it wins. The rules above still hold.";

/// `This turn has had 2 runs before this one; 1 in a row failed. Read what is already written and carry on from it.`
fn earlier_runs(before: u32, failed: u32) -> String {
    let runs = if before == 1 {
        "1 run".to_owned()
    } else {
        format!("{before} runs")
    };
    let failed = if failed > 0 {
        format!("; {failed} in a row failed")
    } else {
        String::new()
    };
    format!("This turn has had {runs} before this one{failed}. Read what is already written and carry on from it.\n")
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    fn parts(goal: KnownGoal) -> BriefParts<'static> {
        BriefParts {
            goal,
            runner_instructions: None,
            heap_name: "Work",
            heap_handle: "ada/work",
            heap_context: Some("Our infra lives in the ops repo.\n"),
            item_text: "k3x9a  Rotate the certs\nKind   note\n",
            line: None,
            runs: 1,
            failed_runs: 0,
        }
    }

    #[gtest]
    fn the_brief_has_the_instructions_the_heap_the_item_and_the_run_in_that_order() {
        let brief = build_brief(&parts(KnownGoal::WithContext));

        expect_that!(brief, starts_with("You are working on one item in Upheap"));
        let order: Vec<usize> = [
            "Goal: the item has its context.",
            "# The heap: Work (ada/work)\n\nOur infra lives in the ops repo.\n\n# The item\n\nk3x9a  Rotate the certs\nKind   note\n\n# This run\n\nGoal: with context.\n",
        ]
        .iter()
        .filter_map(|part| brief.find(part))
        .collect();
        expect_that!(order.len(), eq(2));
        expect_that!(order.is_sorted(), eq(true));
        expect_that!(brief, ends_with("Goal: with context.\n"));
    }

    #[gtest]
    fn the_runners_instructions_come_after_upheaps_and_before_the_heap() {
        let mut with = parts(KnownGoal::Done);
        with.runner_instructions = Some("\nFor code changes, use the implement skill.\n\n");

        let brief = build_brief(&with);

        expect_that!(
            brief,
            contains_substring(
                "Where it and the heap's context disagree on how to do the work, it wins. The rules above still \
                 hold.\n\nFor code changes, use the implement skill.\n\n# The heap: Work (ada/work)\n"
            )
        );
        expect_that!(
            brief.find("A run here always ends with a request.") < brief.find("# The runner's instructions"),
            eq(true)
        );
    }

    #[gtest]
    fn without_runner_instructions_or_with_blank_ones_the_brief_has_no_part_for_them() {
        let mut blank = parts(KnownGoal::Done);
        blank.runner_instructions = Some(" \n");

        for parts in [parts(KnownGoal::Done), blank] {
            expect_that!(
                build_brief(&parts),
                not(contains_substring("The runner's instructions"))
            );
        }
    }

    #[gtest]
    fn a_heap_without_context_says_so() {
        let mut none = parts(KnownGoal::Defined);
        none.heap_context = None;
        let mut blank = parts(KnownGoal::Defined);
        blank.heap_context = Some("  \n");

        for parts in [none, blank] {
            expect_that!(
                build_brief(&parts),
                contains_substring("# The heap: Work (ada/work)\n\nThis heap has no context.\n\n# The item")
            );
        }
    }

    #[gtest]
    fn the_persons_line_is_quoted_and_left_out_when_there_is_none() {
        let mut said = parts(KnownGoal::Defined);
        said.line = Some("Use the new CA.");

        expect_that!(
            build_brief(&said),
            ends_with("Goal: defined.\nThe person said: \"Use the new CA.\"\n")
        );
        expect_that!(
            build_brief(&parts(KnownGoal::Defined)),
            not(contains_substring("The person said"))
        );
    }

    #[gtest]
    fn earlier_runs_are_counted_without_this_one_and_failures_only_when_there_are_some() {
        let mut once = parts(KnownGoal::Defined);
        once.runs = 2;
        let mut failed = parts(KnownGoal::Defined);
        failed.runs = 4;
        failed.failed_runs = 2;

        expect_that!(
            build_brief(&once),
            ends_with("This turn has had 1 run before this one. Read what is already written and carry on from it.\n")
        );
        expect_that!(
            build_brief(&failed),
            ends_with(
                "This turn has had 3 runs before this one; 2 in a row failed. Read what is already written and carry on from it.\n"
            )
        );
        expect_that!(
            build_brief(&parts(KnownGoal::Defined)),
            not(contains_substring("This turn has had"))
        );
    }

    #[gtest]
    fn each_goal_gets_its_own_part_and_no_other() {
        let marks = [
            (KnownGoal::WithContext, "Goal: the item has its context."),
            (
                KnownGoal::Defined,
                "Goal: the item is a defined task, ready to be done.",
            ),
            (KnownGoal::Done, "Goal: the task is done, as far as you can take it."),
        ];

        for (goal, mark) in marks {
            let brief = build_brief(&parts(goal));

            expect_that!(brief, contains_substring(COMMON.trim_end()));
            for (_, other) in marks {
                expect_that!(brief.contains(other), eq(other == mark), "{goal:?} and {other}");
            }
        }
    }

    #[gtest]
    fn a_goal_the_version_doesnt_know_is_not_worked_and_the_others_have_their_access() {
        expect_that!(KnownGoal::from_dto(GoalDto::Unknown), eq(None));
        expect_that!(
            KnownGoal::from_dto(GoalDto::WithContext),
            eq(Some(KnownGoal::WithContext))
        );
        expect_that!(KnownGoal::WithContext.access(), eq(Access::Prepare));
        expect_that!(KnownGoal::Defined.access(), eq(Access::Prepare));
        expect_that!(KnownGoal::Done.access(), eq(Access::Do));
    }
}
