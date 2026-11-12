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
    stdout_is_terminal: bool,
    stderr_is_terminal: bool,
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
            stdout_is_terminal: false,
            stderr_is_terminal: false,
        }
    }

    /// Says which streams are terminals, where `--json` escapes the characters a terminal would run.
    pub fn on_terminals(self, stdout: bool, stderr: bool) -> Self {
        Self {
            stdout_is_terminal: stdout,
            stderr_is_terminal: stderr,
            ..self
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

    /// Writes a line made of parts, each in its own tone, with any control characters in them made harmless.
    pub fn line_parts(&mut self, parts: &[(&str, Tone)]) -> std::io::Result<()> {
        let line: String = parts
            .iter()
            .map(|(text, tone)| self.paint(&defuse(text), *tone))
            .collect();
        writeln!(self.stdout, "{line}")
    }

    /// Writes a line of a table from [`Output::render_table`] to stdout, after `prefix`.
    pub fn table_line(&mut self, prefix: &str, line: &TableLine) -> std::io::Result<()> {
        writeln!(self.stdout, "{}{}", defuse(prefix), line.0)
    }

    /// Writes the server's body to stdout as it came, ending with a newline. On a terminal, with its DEL and C1
    /// characters escaped, which leaves the JSON value the same.
    pub fn json(&mut self, body: &str) -> std::io::Result<()> {
        let body = json_for(body.trim_end(), self.stdout_is_terminal);
        writeln!(self.stdout, "{body}")
    }

    /// Writes a line to stderr, with any control characters in it made harmless.
    pub fn note(&mut self, text: &str) -> std::io::Result<()> {
        writeln!(self.stderr, "{}", defuse(text))
    }

    /// Writes a line to stderr in `tone`, with any control characters in it made harmless first.
    pub fn note_toned(&mut self, text: &str, tone: Tone) -> std::io::Result<()> {
        let painted = self.paint(&defuse(text), tone);
        writeln!(self.stderr, "{painted}")
    }

    /// Writes the server's body to stderr as it came, ending with a newline: a problem, with `--json`. On a
    /// terminal, escaped as [`Output::json`] does.
    pub fn json_note(&mut self, body: &str) -> std::io::Result<()> {
        let body = json_for(body.trim_end(), self.stderr_is_terminal);
        writeln!(self.stderr, "{body}")
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

/// `body`, a JSON document, with DEL and the C1 controls (U+007F to U+009F) written as `\u007f` to `\u009f` when it
/// goes to a terminal, which could run them (U+009B starts an escape sequence on some). JSON allows a control
/// character only inside a string, where the escape means the same character, so the value doesn't change. Piped,
/// the body is left byte for byte as it came.
fn json_for(body: &str, terminal: bool) -> std::borrow::Cow<'_, str> {
    let is_raw_control = |c: char| ('\u{7f}'..='\u{9f}').contains(&c);
    if !terminal || !body.contains(is_raw_control) {
        return std::borrow::Cow::Borrowed(body);
    }
    let escaped = body
        .chars()
        .map(|c| {
            if is_raw_control(c) {
                format!("\\u{:04x}", u32::from(c))
            } else {
                c.to_string()
            }
        })
        .collect();
    std::borrow::Cow::Owned(escaped)
}

/// A line of a table, defused and coloured, ready to write with [`Output::table_line`].
#[derive(Debug, PartialEq, Eq)]
pub struct TableLine(String);

/// `text` with every control character (C0, DEL and C1: escape sequences, newlines, carriage returns) shown as U+FFFD.
///
/// Titles, names and labels come from other people through the server, which doesn't strip them. Printed raw, an
/// escape sequence would run in the reader's terminal (clear the screen, write the clipboard) and a newline could
/// forge rows, so human output is defused here. `--json` keeps the body's value: JSON escapes the C0 characters but
/// not DEL and C1, which [`json_for`] escapes on a terminal.
fn defuse(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '\u{FFFD}' } else { c })
        .collect()
}

/// `text` cut to `width` characters, the last one an ellipsis, when it's longer.
pub fn shorten(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let kept: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn long_titles_are_cut_with_an_ellipsis() {
        expect_that!(shorten("short", 10), eq("short"));
        expect_that!(shorten("exactly10!", 10), eq("exactly10!"));
        expect_that!(shorten("a bit too long", 10), eq("a bit too…"));
    }

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

    #[gtest]
    fn json_on_a_terminal_escapes_del_and_c1_and_means_the_same() -> googletest::Result<()> {
        let body = "{\"title\":\"a\u{9b}2Jb\u{7f}c\"}";

        let on_terminal = json_for(body, true);
        let piped = json_for(body, false);

        verify_that!(on_terminal.as_ref(), eq("{\"title\":\"a\\u009b2Jb\\u007fc\"}"))?;
        verify_that!(piped.as_ref(), eq(body))?;
        let read: serde_json::Value = serde_json::from_str(&on_terminal)?;
        let original: serde_json::Value = serde_json::from_str(body)?;
        verify_that!(read, eq(&original))
    }
}
