//! Where results and messages go, and how they look: human text in aligned columns, the server's JSON, or keys alone.
//!
//! `run` builds one [`Output`] from the command's flags and hands it to the command's flow, the only code that
//! writes; API calls and lookups return values.

use std::io::Write;

use owo_colors::{OwoColorize, Style};

/// What a command prints on stdout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Text for people.
    Human,
    /// The server's response body, unchanged.
    Json,
    /// Only what a script needs next, such as keys one per line.
    Quiet,
}

/// The two streams and how to write to them. Results go to stdout; notes, warnings and errors to stderr.
pub struct Output {
    stdout: Box<dyn Write + Send>,
    stderr: Box<dyn Write + Send>,
    mode: Mode,
    color: bool,
}

/// How a cell of a table looks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tone {
    #[default]
    Plain,
    /// Secondary detail, such as headings and positions.
    Dim,
    /// What needs attention, such as an overdue deadline.
    Alert,
    /// What stands out, such as a key or the default heap.
    Strong,
}

/// One cell: its text and how it looks.
#[derive(Clone, Debug, Default)]
pub struct Cell {
    pub text: String,
    pub tone: Tone,
}

impl Cell {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Plain,
        }
    }

    pub fn toned(text: impl Into<String>, tone: Tone) -> Self {
        Self {
            text: text.into(),
            tone,
        }
    }
}

impl Output {
    /// Writes to `stdout` and `stderr` in `mode`, with colour when `color`.
    pub fn new(stdout: Box<dyn Write + Send>, stderr: Box<dyn Write + Send>, mode: Mode, color: bool) -> Self {
        Self {
            stdout,
            stderr,
            mode,
            color,
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Writes a line to stdout, with any control characters in it made harmless.
    pub fn line(&mut self, text: &str) -> std::io::Result<()> {
        writeln!(self.stdout, "{}", defuse(text))
    }

    /// Writes a line to stdout in `tone`, with any control characters in it made harmless first.
    pub fn line_toned(&mut self, text: &str, tone: Tone) -> std::io::Result<()> {
        let painted = self.paint(&defuse(text), tone);
        writeln!(self.stdout, "{painted}")
    }

    /// Writes a line of a table from [`Output::render_table`] to stdout, after `prefix`.
    pub fn table_line(&mut self, prefix: &str, line: &TableLine) -> std::io::Result<()> {
        writeln!(self.stdout, "{}{}", defuse(prefix), line.0)
    }

    /// Writes the server's body to stdout as it came, ending with a newline.
    pub fn json(&mut self, body: &str) -> std::io::Result<()> {
        writeln!(self.stdout, "{}", body.trim_end())
    }

    /// Writes a line to stderr, with any control characters in it made harmless.
    pub fn note(&mut self, text: &str) -> std::io::Result<()> {
        writeln!(self.stderr, "{}", defuse(text))
    }

    /// Writes the server's body to stderr as it came, ending with a newline: a problem, with `--json`.
    pub fn json_note(&mut self, body: &str) -> std::io::Result<()> {
        writeln!(self.stderr, "{}", body.trim_end())
    }

    /// `text` in `tone`, coloured when colour is on. `text` must have been defused already, as the colour codes are
    /// the only control characters a line may carry.
    fn paint(&self, text: &str, tone: Tone) -> String {
        if !self.color {
            return text.to_owned();
        }
        let style = match tone {
            Tone::Plain => return text.to_owned(),
            Tone::Dim => Style::new().dimmed(),
            Tone::Alert => Style::new().red(),
            Tone::Strong => Style::new().bold(),
        };
        text.style(style).to_string()
    }

    /// `rows` in aligned columns, two spaces apart, a line each, the cells defused and then coloured. The last
    /// column isn't padded.
    pub fn render_table(&self, rows: &[Vec<Cell>]) -> Vec<TableLine> {
        let rows: Vec<Vec<(String, Tone)>> = rows
            .iter()
            .map(|row| row.iter().map(|cell| (defuse(&cell.text), cell.tone)).collect())
            .collect();
        let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
        let widths: Vec<usize> = (0..columns)
            .map(|column| {
                rows.iter()
                    .filter_map(|row| row.get(column))
                    .map(|(text, _)| text.chars().count())
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        rows.iter()
            .map(|row| {
                let last = row.len().saturating_sub(1);
                let cells = row.iter().enumerate().map(|(column, (text, tone))| {
                    let pad = if column == last {
                        0
                    } else {
                        widths[column].saturating_sub(text.chars().count())
                    };
                    format!("{}{}", self.paint(text, *tone), " ".repeat(pad))
                });
                TableLine(cells.collect::<Vec<_>>().join("  ").trim_end().to_owned())
            })
            .collect()
    }
}

/// A line of a table, defused and coloured, ready to write with [`Output::table_line`].
#[derive(Debug, PartialEq, Eq)]
pub struct TableLine(String);

/// `text` with every control character (C0, DEL and C1: escape sequences, newlines, carriage returns) shown as U+FFFD.
///
/// Titles, names and labels come from other people through the server, which doesn't strip them. Printed raw, an
/// escape sequence would run in the reader's terminal (clear the screen, write the clipboard) and a newline could
/// forge rows. Human output is the only place they're interpreted, so it's defused here; `--json` prints the body as
/// it came, where JSON escapes them.
fn defuse(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '\u{FFFD}' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn columns_line_up_and_the_last_one_isnt_padded() {
        let output = Output::new(Box::new(std::io::sink()), Box::new(std::io::sink()), Mode::Human, false);
        let rows = vec![
            vec![Cell::new("1"), Cell::new("k3x9a"), Cell::new("Rotate certs")],
            vec![Cell::new("10"), Cell::new("ab"), Cell::new("x")],
        ];

        let lines = output.render_table(&rows);

        expect_that!(
            lines,
            elements_are![
                eq(&TableLine("1   k3x9a  Rotate certs".to_owned())),
                eq(&TableLine("10  ab     x".to_owned()))
            ]
        );
    }

    #[gtest]
    fn colour_is_left_out_unless_asked_for() {
        let plain = Output::new(Box::new(std::io::sink()), Box::new(std::io::sink()), Mode::Human, false);
        let colored = Output::new(Box::new(std::io::sink()), Box::new(std::io::sink()), Mode::Human, true);

        expect_that!(plain.paint("overdue", Tone::Alert), eq("overdue"));
        expect_that!(colored.paint("overdue", Tone::Alert), contains_substring("\u{1b}["));
    }

    #[gtest]
    fn control_characters_are_shown_not_run() {
        let output = Output::new(Box::new(std::io::sink()), Box::new(std::io::sink()), Mode::Human, true);
        let rows = vec![vec![Cell::toned("\u{1b}]52;c;aGk=\u{7}x\nforged", Tone::Strong)]];

        let lines = output.render_table(&rows);

        expect_that!(defuse("a\u{1b}[2Jb\r\u{9b}c"), eq("a\u{FFFD}[2Jb\u{FFFD}\u{FFFD}c"));
        // The colour codes are the only escape left: bold on, the defused text, bold off.
        expect_that!(
            lines,
            elements_are![eq(&TableLine(
                "\u{1b}[1m\u{FFFD}]52;c;aGk=\u{FFFD}x\u{FFFD}forged\u{1b}[0m".to_owned()
            ))]
        );
    }
}
