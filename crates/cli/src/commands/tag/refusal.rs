//! What a refused team tag write tells: the heap tags a merge would take in with the command that confirms it, or,
//! in a run, the heaps the run wasn't given. The CLI has no prompts, so the confirming command is printed in full, with
//! the reach it was shown.

use api_types::problem::MergeReachDto;
use api_types::teams::TeamDto;
use domain::tag::TagColor;

use crate::client::ClientError;
use crate::commands::heap_api::TeamApi;
use crate::error::shell_quoted;
use crate::lookup::HeapHandle;
use crate::{App, CliError};

/// Where a `tag edit` works, as the command that confirms it names it again.
pub(crate) enum EditScope<'a> {
    Heap(&'a HeapHandle),
    Team(&'a str),
}

/// A team tag write, as the user asked for it.
pub(crate) enum TeamTagWrite<'a> {
    /// `tag add <name> --team <team>`.
    Add {
        team: &'a str,
        name: &'a str,
        /// As typed: `-` stays `-`.
        description: Option<&'a str>,
        color: Option<TagColor>,
    },
    /// `tag promote <tag> --heap <heap>`, where the heap tag is named `tag_name`.
    Promote {
        tag: &'a str,
        tag_name: &'a str,
        heap: &'a HeapHandle,
    },
    /// `tag edit <tag>` on the team tag named `before`.
    Edit {
        tag: &'a str,
        before: &'a str,
        team: &'a str,
        scope: EditScope<'a>,
        name: Option<&'a str>,
        /// As typed: `-` stays `-`.
        description: Option<&'a str>,
        no_description: bool,
        color: Option<TagColor>,
    },
}

impl TeamTagWrite<'_> {
    /// The refusal `err` of this write as the error to report: with the reach and the confirming command for
    /// `tag_merge_unconfirmed`, and the heaps a run wasn't given for `run_forbidden`. Any other refusal, and a
    /// `run_forbidden` whose team can't be read, stays as the server gave it.
    pub async fn refused(&self, err: ClientError, api: &TeamApi<'_>, app: &App) -> CliError {
        let lines = match &err {
            ClientError::Problem { problem, .. } => match problem.code.as_str() {
                "tag_merge_unconfirmed" => problem.merge_reach.as_ref().map(|reach| self.merge_lines(reach)),
                "run_forbidden" => self.run_lines(&err, api, app).await,
                _ => None,
            },
            _ => None,
        };
        match lines {
            Some(lines) => CliError::WriteRefused { lines, source: err },
            None => CliError::Client(err),
        }
    }

    fn team(&self) -> &str {
        match self {
            Self::Add { team, .. } | Self::Edit { team, .. } => team,
            Self::Promote { heap, .. } => &heap.team,
        }
    }

    /// The reach of a merge and how to go ahead with it.
    fn merge_lines(&self, reach: &MergeReachDto) -> Vec<String> {
        let slugs = reach.heaps.iter().map(|merged| merged.heap.slug.chars().count());
        let slug_width = slugs.max().unwrap_or(0);
        let name_width = reach
            .heaps
            .iter()
            .map(|merged| merged.tag_name.chars().count())
            .max()
            .unwrap_or(0);
        let mut lines = vec![format!(
            "error: {} merges these heap tags into it (tag_merge_unconfirmed):",
            self.merge_phrase()
        )];
        for merged in &reach.heaps {
            let items = if merged.items == 1 {
                "1 item".to_owned()
            } else {
                format!("{} items", merged.items)
            };
            let archived = if merged.archived { "  (archived)" } else { "" };
            lines.push(format!(
                "  {:<slug_width$}  {:<name_width$}  {items}{archived}",
                merged.heap.slug, merged.tag_name
            ));
        }
        lines.push("This can't be undone. To go ahead, run:".to_owned());
        lines.push(format!("  {}", self.command(&reach.token)));
        lines
    }

    fn merge_phrase(&self) -> String {
        match self {
            Self::Add { team, name, .. } => format!("Adding team tag \"{name}\" to {team}"),
            Self::Promote { tag_name, heap, .. } => {
                format!("Promoting \"{tag_name}\" to a team tag of {}", heap.team)
            }
            Self::Edit { before, name, .. } => {
                format!("Renaming team tag \"{before}\" to \"{}\"", name.unwrap_or(before))
            }
        }
    }

    /// The command that repeats this write with `token` to confirm the merge, from the arguments as parsed. It names
    /// where it works, so it doesn't depend on the default heap, and puts a name that starts with `-` after `--`.
    fn command(&self, token: &str) -> String {
        let (verb, positional, mut flags) = match self {
            Self::Add {
                team,
                name,
                description,
                color,
            } => {
                let mut flags = vec![format!("--team {}", shell_quoted(team))];
                if let Some(text) = description {
                    flags.push(format!("--description {}", shell_quoted(text)));
                }
                if let Some(color) = color {
                    flags.push(format!("--color {}", color.code()));
                }
                ("add", *name, flags)
            }
            Self::Promote { tag, heap, .. } => ("promote", *tag, vec![format!("--heap {heap}")]),
            Self::Edit {
                tag,
                scope,
                name,
                description,
                no_description,
                color,
                ..
            } => {
                let mut flags = vec![match scope {
                    EditScope::Heap(heap) => format!("--heap {heap}"),
                    EditScope::Team(team) => format!("--team {}", shell_quoted(team)),
                }];
                if let Some(name) = name {
                    flags.push(format!("--name {}", shell_quoted(name)));
                }
                if let Some(text) = description {
                    flags.push(format!("--description {}", shell_quoted(text)));
                }
                if *no_description {
                    flags.push("--no-description".to_owned());
                }
                if let Some(color) = color {
                    flags.push(format!("--color {}", color.code()));
                }
                ("edit", *tag, flags)
            }
        };
        flags.push(format!("--confirm {}", shell_quoted(token)));
        let flags = flags.join(" ");
        if positional.starts_with('-') {
            format!("upheap tag {verb} {flags} -- {}", shell_quoted(positional))
        } else {
            format!("upheap tag {verb} {} {flags}", shell_quoted(positional))
        }
    }

    /// The message for a write a run isn't allowed: the heaps of the team besides the run's that the write reaches,
    /// and how to ask the person. `None` when the team can't be read.
    async fn run_lines(&self, refused: &ClientError, api: &TeamApi<'_>, app: &App) -> Option<Vec<String>> {
        let team = api.get::<TeamDto>(&[]).await.ok()?.value;
        let run_heap = app.env.heap.as_deref();
        let others: Vec<String> = team
            .heaps
            .iter()
            .map(|heap| format!("{}/{}", self.team(), heap.slug))
            .filter(|slug| Some(slug.as_str()) != run_heap)
            .collect();
        let (what, request) = self.run_words();
        // Single-quoted: the names come from other people, and inside double quotes a shell would run `$(...)`.
        let request = shell_quoted(&request);
        let error = if others.is_empty() {
            format!("error: {refused}")
        } else {
            format!(
                "error: {what} reaches {}, which this run wasn't given (run_forbidden).",
                others.join(", ")
            )
        };
        Some(vec![
            error,
            format!("hint: ask the person: upheap question request <key> {request}"),
        ])
    }

    /// What the write is, for the refusal, and the request that asks the person to do it.
    fn run_words(&self) -> (String, String) {
        match self {
            Self::Add { name, .. } => (
                format!("adding team tag \"{name}\""),
                format!("Add the team tag {name}"),
            ),
            Self::Promote { tag_name, heap, .. } => (
                format!("promoting \"{tag_name}\" to a team tag"),
                format!("Make the heap tag {tag_name} of {heap} a team tag"),
            ),
            Self::Edit {
                before,
                name: Some(new),
                ..
            } => (
                format!("renaming team tag \"{before}\""),
                format!("Rename the team tag {before} to {new}"),
            ),
            Self::Edit { before, .. } => (
                format!("changing team tag \"{before}\"'s description"),
                format!("Change the description of the team tag {before}"),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    fn heap() -> HeapHandle {
        HeapHandle {
            team: "ada".to_owned(),
            heap: "work".to_owned(),
        }
    }

    #[gtest]
    fn an_add_names_the_team_and_quotes_the_description() {
        let write = TeamTagWrite::Add {
            team: "ada",
            name: "Big rock",
            description: Some("it's - big"),
            color: None,
        };

        expect_that!(
            write.command("t0k"),
            eq("upheap tag add 'Big rock' --team ada --description 'it'\\''s - big' --confirm t0k")
        );
    }

    #[gtest]
    fn a_colour_given_is_repeated() {
        let write = TeamTagWrite::Add {
            team: "ada",
            name: "ops",
            description: None,
            color: Some(TagColor::Rose),
        };

        expect_that!(
            write.command("t0k"),
            eq("upheap tag add ops --team ada --color rose --confirm t0k")
        );
    }

    #[gtest]
    fn a_description_from_stdin_stays_a_dash() {
        let write = TeamTagWrite::Add {
            team: "ada",
            name: "ops",
            description: Some("-"),
            color: None,
        };

        expect_that!(
            write.command("t0k"),
            eq("upheap tag add ops --team ada --description - --confirm t0k")
        );
    }

    #[gtest]
    fn a_name_that_starts_with_a_dash_goes_after_the_dashes() {
        let write = TeamTagWrite::Add {
            team: "ada",
            name: "-ops",
            description: None,
            color: None,
        };

        expect_that!(
            write.command("t0k"),
            eq("upheap tag add --team ada --confirm t0k -- -ops")
        );
    }

    #[gtest]
    fn a_promotion_names_the_heap() {
        let heap = heap();
        let write = TeamTagWrite::Promote {
            tag: "billing",
            tag_name: "Billing",
            heap: &heap,
        };

        expect_that!(
            write.command("t0k"),
            eq("upheap tag promote billing --heap ada/work --confirm t0k")
        );
    }

    #[gtest]
    fn an_edit_names_where_it_works_and_what_it_changes() {
        let heap = heap();
        let in_heap = TeamTagWrite::Edit {
            tag: "ops",
            before: "ops",
            team: "ada",
            scope: EditScope::Heap(&heap),
            name: Some("operations"),
            description: None,
            no_description: true,
            color: None,
        };
        let in_team = TeamTagWrite::Edit {
            tag: "ops",
            before: "ops",
            team: "ada",
            scope: EditScope::Team("ada"),
            name: Some("two words"),
            description: Some("Ops work"),
            no_description: false,
            color: None,
        };

        expect_that!(
            in_heap.command("t0k"),
            eq("upheap tag edit ops --heap ada/work --name operations --no-description --confirm t0k")
        );
        expect_that!(
            in_team.command("t0k"),
            eq("upheap tag edit ops --team ada --name 'two words' --description 'Ops work' --confirm t0k")
        );
    }

    #[gtest]
    fn the_reach_lines_up_its_columns_and_counts_items() -> googletest::Result<()> {
        let heap_summary = |slug: &str| {
            serde_json::json!({
                "id": uuid::Uuid::new_v4(), "team_id": uuid::Uuid::new_v4(), "slug": slug, "name": slug, "archived": false,
            })
        };
        let reach: MergeReachDto = serde_json::from_value(serde_json::json!({
            "token": "t0k",
            "heaps": [
                { "heap": heap_summary("a"), "archived": false, "tag_id": uuid::Uuid::new_v4(), "tag_name": "Billing", "items": 1 },
                { "heap": heap_summary("long-heap"), "archived": true, "tag_id": uuid::Uuid::new_v4(), "tag_name": "billing", "items": 12 },
            ],
        }))?;
        let write = TeamTagWrite::Add {
            team: "ada",
            name: "billing",
            description: None,
            color: None,
        };

        verify_that!(
            write.merge_lines(&reach),
            elements_are![
                eq("error: Adding team tag \"billing\" to ada merges these heap tags into it (tag_merge_unconfirmed):"),
                eq("  a          Billing  1 item"),
                eq("  long-heap  billing  12 items  (archived)"),
                eq("This can't be undone. To go ahead, run:"),
                eq("  upheap tag add billing --team ada --confirm t0k"),
            ]
        )
    }

    #[gtest]
    fn what_a_run_asks_for_follows_the_write() {
        let heap = heap();
        let rename = TeamTagWrite::Edit {
            tag: "ops",
            before: "ops",
            team: "ada",
            scope: EditScope::Team("ada"),
            name: Some("operations"),
            description: None,
            no_description: false,
            color: None,
        };
        let describe = TeamTagWrite::Edit {
            tag: "ops",
            before: "ops",
            team: "ada",
            scope: EditScope::Team("ada"),
            name: None,
            description: Some("x"),
            no_description: false,
            color: None,
        };
        let promote = TeamTagWrite::Promote {
            tag: "x",
            tag_name: "x",
            heap: &heap,
        };

        expect_that!(
            rename.run_words(),
            eq(&(
                "renaming team tag \"ops\"".to_owned(),
                "Rename the team tag ops to operations".to_owned()
            ))
        );
        expect_that!(
            describe.run_words(),
            eq(&(
                "changing team tag \"ops\"'s description".to_owned(),
                "Change the description of the team tag ops".to_owned()
            ))
        );
        expect_that!(
            promote.run_words(),
            eq(&(
                "promoting \"x\" to a team tag".to_owned(),
                "Make the heap tag x of ada/work a team tag".to_owned()
            ))
        );
    }
}
