//! A value's label has one home, its `label` fn here in `domain`: no other crate writes it again as a literal.
//!
//! The labelled enums are listed by hand: a new `label` fn gets its enum added to `labels`. Test code is skipped, since
//! a test may assert on a label, and so are `//` comments.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use domain::item::{Need, Size};
use googletest::prelude::*;

fn labels() -> Vec<&'static str> {
    let needs = Need::ALL.into_iter().map(Need::label);
    let sizes = Size::ALL.into_iter().map(Size::label);
    needs.chain(sizes).collect()
}

#[gtest]
fn no_other_crate_writes_a_domain_label_as_a_literal() -> io::Result<()> {
    // Arrange
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let quoted: Vec<String> = labels().into_iter().map(|label| format!("\"{label}\"")).collect();
    let mut files = Vec::new();
    for entry in fs::read_dir(&crates)? {
        let dir = entry?.path();
        if dir.file_name().is_some_and(|name| name != "domain") {
            collect_sources(&dir.join("src"), &mut files)?;
        }
    }

    // Act
    let mut copies = Vec::new();
    for file in &files {
        let source = fs::read_to_string(file)?;
        let shown = file.strip_prefix(&crates).unwrap_or(file).display();
        for (number, line) in lines_outside_tests(&source) {
            for literal in quoted.iter().filter(|literal| line.contains(literal.as_str())) {
                copies.push(format!("crates/{shown}:{number}: {literal}"));
            }
        }
    }

    // Assert
    expect_that!(copies, is_empty(), "call the domain type's `label` instead");
    Ok(())
}

/// The `.rs` files under `dir`, leaving out `tests` directories and `tests.rs` files.
fn collect_sources(dir: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        if path.is_dir() {
            if name != "tests" {
                collect_sources(&path, files)?;
            }
        } else if name.ends_with(".rs") && name != "tests.rs" {
            files.push(path);
        }
    }
    Ok(())
}

/// The lines of `source` that aren't test code or a `//` comment, with their 1-based numbers.
///
/// The item after `#[cfg(test)]` is skipped: one line when it ends with `;`, otherwise up to the `}` at its own
/// indentation, which rustfmt (a required check) guarantees.
fn lines_outside_tests(source: &str) -> Vec<(usize, &str)> {
    let mut kept = Vec::new();
    let mut lines = source.lines().enumerate();
    while let Some((index, line)) = lines.next() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("#[cfg(test)]") {
            let indent = &line[..line.len() - trimmed.len()];
            let Some((_, item)) = lines.next() else { break };
            if !item.trim_end().ends_with(';') {
                let close = format!("{indent}}}");
                lines.by_ref().find(|(_, line)| line.trim_end() == close);
            }
        } else if !trimmed.starts_with("//") {
            kept.push((index + 1, line));
        }
    }
    kept
}
