use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};
use googletest::prelude::*;

use super::*;
use crate::Cli;

/// Splits a command line the way a shell would, for the plain lines the instructions use: words, double-quoted
/// strings, and a `#` comment to the end of the line.
fn words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word: Option<String> = None;
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                word.get_or_insert_default();
            }
            '#' if !quoted && word.is_none() => break,
            c if c.is_whitespace() && !quoted => words.extend(word.take()),
            c => word.get_or_insert_default().push(c),
        }
    }
    words.extend(word);
    words
}

/// The command lines the instructions name: the example's lines, and what's between backticks in the text.
fn commands_named(text: &str) -> Vec<String> {
    let lines = text.lines().map(str::trim).filter(|line| line.starts_with("upheap "));
    let quoted = text
        .split('`')
        .skip(1)
        .step_by(2)
        .filter(|code| code.starts_with("upheap "));
    lines.chain(quoted).map(str::to_owned).collect()
}

#[gtest]
fn every_command_is_listed_with_its_inputs_and_its_own_flags() {
    // Arrange
    let root = Cli::command();

    // Act
    let guide = render(&root);

    // Assert
    for category in root.get_subcommands() {
        for verb in category.get_subcommands() {
            let start = format!("  upheap {} {}", category.get_name(), verb.get_name());
            expect_that!(guide.lines().filter(|line| line.starts_with(&start)).count(), ge(1));
        }
    }
    expect_that!(
        guide,
        contains_substring(
            "  upheap item add <TITLE> [heap]: Add an item\n      <TITLE>                    What needs doing\n"
        )
    );
    expect_that!(
        guide,
        contains_substring("  upheap heap show [HEAP]: A heap's name and context\n")
    );
    expect_that!(
        guide,
        contains_substring(
            "      --no-description           Clear the item's context (its description) (also --no-context)\n"
        )
    );
    expect_that!(guide, contains_substring("      --many                 Allow picking"));
}

#[gtest]
fn the_flags_most_commands_share_are_listed_once() {
    // Arrange
    let root = Cli::command();

    // Act
    let guide = render(&root);

    // Assert
    for flag in [
        "  --json ",
        "  -q, --quiet ",
        "  --server <URL> ",
        "  --heap <HEAP> ",
        "--help",
    ] {
        let expected = usize::from(flag != "--help");
        expect_that!(
            guide.lines().filter(|line| line.starts_with(flag)).count(),
            eq(expected),
            "{flag}"
        );
    }
    expect_that!(
        guide,
        contains_substring("  upheap heap list: Every heap you can use, by team\n")
    );
    expect_that!(guide, contains_substring("  upheap tag list [heap]: The heap's tags\n"));
}

#[gtest]
fn the_instructions_follow_the_commands() {
    // Arrange
    let root = Cli::command();

    // Act
    let guide = render(&root);

    // Assert
    expect_that!(guide, ends_with("  upheap item done k3x9a"));
    expect_that!(guide, contains_substring("\n\nFor agents\n\nNaming\n"));
}

#[gtest]
fn the_head_says_what_upheap_is_and_where_the_instructions_are() {
    // Arrange
    let root = Cli::command();

    // Act
    let guide = render(&root);
    let head: Vec<&str> = guide.lines().take(12).collect();

    // Assert
    expect_that!(head, contains(starts_with("Upheap is a ranked todo list.")));
    expect_that!(head, contains(starts_with("For an AI agent")));
    expect_that!(
        head.get(1),
        some(starts_with(
            "AI agents: detailed instructions are at the end of this guide, under \"For agents\"."
        ))
    );
    expect_that!(FOR_AGENTS, starts_with("For agents\n"));
}

#[gtest]
fn every_command_the_instructions_name_exists_with_its_flags() {
    // Arrange
    let named = commands_named(&format!("{INTRO}\n{FOR_AGENTS}"));

    // Act
    let refused: Vec<String> = named
        .iter()
        .filter_map(|line| match Cli::try_parse_from(words(line)) {
            // A command named without its inputs, such as `upheap item show`, is still a command that exists.
            Ok(_) => None,
            Err(err) if err.kind() == ErrorKind::MissingRequiredArgument => None,
            Err(err) => Some(format!("{line}: {}", err.kind())),
        })
        .collect();

    // Assert
    expect_that!(named.len(), ge(20));
    expect_that!(refused, is_empty());
}

#[gtest]
fn a_command_line_is_split_into_words_quotes_and_a_comment() {
    // Arrange
    let line = r#"upheap question ask k3x9a "Which CA?" --choice "Let's Encrypt" --choice ""   # the key"#;

    // Act
    let split = words(line);

    // Assert
    expect_that!(
        split,
        elements_are![
            eq("upheap"),
            eq("question"),
            eq("ask"),
            eq("k3x9a"),
            eq("Which CA?"),
            eq("--choice"),
            eq("Let's Encrypt"),
            eq("--choice"),
            eq("")
        ]
    );
}
