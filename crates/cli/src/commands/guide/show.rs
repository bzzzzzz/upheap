//! `upheap guide show`: every command with its arguments and flags, and how to work with the tool, in one read.
//!
//! It is for an agent that meets the tool for the first time, and would otherwise run `--help` on each command.
//! The commands are read from the same clap definitions `--help` prints, so the two can't disagree; the instructions
//! are text kept here. It makes no request and reads no config, so it works before anything is set up.

use clap::{Arg, Args, Command};

use crate::output::Output;

/// What the product is and what an agent uses it for, with a pointer to the instructions, which come last so the
/// commands they name have been read by then. An agent that reads only the head of a long output still learns
/// they're there.
const INTRO: &str = include_str!("intro.txt");

/// What help can't say: how things are named, what the flags print, the exit codes and the usual order of work.
const FOR_AGENTS: &str = include_str!("agents.txt");

/// The flags of [`crate::commands::SharedArgs`], shown once rather than under every command.
const SHARED_FLAGS: [&str; 3] = ["json", "quiet", "server"];
/// The flag of [`crate::commands::HeapArgs`], shown once, with a mark on the commands that take it.
const HEAP_FLAG: &str = "heap";
const HEAP_MARK: &str = " [heap]";

#[derive(Args, Debug)]
pub struct ShowArgs {}

pub fn run(root: &Command, out: &mut Output) -> std::io::Result<()> {
    for line in render(root).lines() {
        out.line(line)?;
    }
    Ok(())
}

/// The guide for the command tree under `root`.
pub(crate) fn render(root: &Command) -> String {
    let mut lines = Vec::new();
    let name = root.get_name();
    let about = root.get_about().map(ToString::to_string).unwrap_or_default();
    lines.push(match root.get_version() {
        Some(version) => format!("{name} {version}. {about}"),
        None => format!("{name}. {about}"),
    });
    lines.push(INTRO.trim_end().to_owned());
    lines.push(String::new());
    lines.push(format!(
        "Every command is `{name} <category> <verb>`; <required> and [optional] inputs come before the flags."
    ));
    lines.push(String::new());

    let verbs: Vec<&Command> = root.get_subcommands().flat_map(Command::get_subcommands).collect();
    let shared: Vec<&Arg> = SHARED_FLAGS
        .iter()
        .filter_map(|long| verbs.iter().find_map(|verb| flag_of(verb, long)))
        .collect();
    if !shared.is_empty() {
        lines.push("Every command that talks to the server takes".to_owned());
        lines.extend(rows(&shared, "  "));
    }
    if let Some(heap) = verbs.iter().find_map(|verb| flag_of(verb, HEAP_FLAG)) {
        lines.push(format!("A command marked{HEAP_MARK} works in one heap, and takes"));
        lines.extend(rows(&[heap], "  "));
    }

    for category in root.get_subcommands() {
        lines.push(String::new());
        lines.push(with_about(category.get_name(), category));
        for verb in category.get_subcommands() {
            let mut usage = format!("  {name} {} {}", category.get_name(), verb.get_name());
            for input in verb.get_positionals() {
                usage.push(' ');
                usage.push_str(&label(input));
            }
            if flag_of(verb, HEAP_FLAG).is_some() {
                usage.push_str(HEAP_MARK);
            }
            lines.push(with_about(&usage, verb));
            let own: Vec<&Arg> = verb.get_arguments().filter(|arg| !is_shown_once(arg)).collect();
            lines.extend(rows(&own, "      "));
        }
    }

    lines.push(String::new());
    lines.push(FOR_AGENTS.trim_end().to_owned());
    lines.join("\n")
}

fn with_about(start: &str, command: &Command) -> String {
    match command.get_about() {
        Some(about) => format!("{start}: {about}"),
        None => start.to_owned(),
    }
}

fn flag_of<'a>(command: &'a Command, long: &str) -> Option<&'a Arg> {
    command.get_arguments().find(|arg| arg.get_long() == Some(long))
}

fn is_shown_once(arg: &Arg) -> bool {
    arg.is_hide_set()
        || arg
            .get_long()
            .is_some_and(|long| long == HEAP_FLAG || long == "help" || SHARED_FLAGS.contains(&long))
}

/// One line per argument, its help in a column.
fn rows(args: &[&Arg], indent: &str) -> Vec<String> {
    let labels: Vec<String> = args.iter().map(|arg| label(arg)).collect();
    let width = labels.iter().map(|label| label.chars().count()).max().unwrap_or(0);
    args.iter()
        .zip(&labels)
        .map(|(arg, label)| {
            let mut help = arg.get_help().map(ToString::to_string).unwrap_or_default();
            if let Some(aliases) = arg.get_visible_aliases().filter(|aliases| !aliases.is_empty()) {
                let aliases: Vec<String> = aliases.iter().map(|alias| format!("--{alias}")).collect();
                help.push_str(&format!(" (also {})", aliases.join(", ")));
            }
            format!("{indent}{label:<width$}  {help}").trim_end().to_owned()
        })
        .collect()
}

/// How an argument is written on the command line: `<ITEM>`, `[HEAP]`, `--due <WHEN>`, `-q, --quiet`.
fn label(arg: &Arg) -> String {
    let value = arg
        .get_value_names()
        .and_then(|names| names.first())
        .map_or_else(|| arg.get_id().as_str().to_uppercase(), ToString::to_string);
    if arg.is_positional() {
        return if arg.is_required_set() {
            format!("<{value}>")
        } else {
            format!("[{value}]")
        };
    }
    let mut label = String::new();
    if let Some(short) = arg.get_short() {
        label.push_str(&format!("-{short}, "));
    }
    label.push_str(&format!("--{}", arg.get_long().unwrap_or_default()));
    if arg.get_action().takes_values() {
        label.push_str(&format!(" <{value}>"));
    }
    label
}

#[cfg(test)]
mod tests;
