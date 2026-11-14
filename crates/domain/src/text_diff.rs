//! A word diff between two versions of a text, for showing an edit in history (ux.md, The item, point 22).
//!
//! Words and the whitespace between them are the units, so line breaks survive. A run of edits split only by spaces
//! reads as one removed phrase and one added phrase, rather than word against word. When too little stays the same,
//! a diff reads worse than the two versions in full, so there is none.

use similar::{ChangeTag, TextDiff};

/// The share of words that must stay the same for a diff, in percent of the longer version's words. Below it, both
/// versions show in full.
const MIN_KEPT_PERCENT: usize = 40;

/// Whether a piece of a diff is in both versions, only the new one or only the old one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffPieceKind {
    Same,
    Added,
    Removed,
}

/// A stretch of text that is the same, added or removed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffPiece {
    pub kind: DiffPieceKind,
    pub text: String,
}

impl DiffPiece {
    fn new(kind: DiffPieceKind, text: &str) -> Self {
        Self {
            kind,
            text: text.to_owned(),
        }
    }
}

/// The word diff from `before` to `after`, in order: the same and removed pieces make `before`, the same and added
/// pieces make `after`. Neighbouring pieces are never of the same kind. `None` when the texts are equal, when
/// neither has a word, or when fewer than [`MIN_KEPT_PERCENT`] of the longer version's words stay the same.
///
/// Size assumption: texts are at most a context's 20,000 characters and edits are mostly small, so Myers' diff over
/// words is cheap; a whole rewrite of the longest context takes some tens of millions of steps, which a page of
/// history can afford. If texts grow much longer, bound the work.
pub fn diff_words(before: &str, after: &str) -> Option<Vec<DiffPiece>> {
    if before == after {
        return None;
    }
    let diff = TextDiff::from_words(before, after);
    let tokens: Vec<(ChangeTag, &str)> = diff
        .iter_all_changes()
        .map(|change| (change.tag(), change.value()))
        .collect();

    let count_words = |keep: &dyn Fn(ChangeTag) -> bool| {
        tokens
            .iter()
            .filter(|(tag, text)| keep(*tag) && !is_blank(text))
            .count()
    };
    let kept = count_words(&|tag| tag == ChangeTag::Equal);
    let before_words = count_words(&|tag| tag != ChangeTag::Insert);
    let after_words = count_words(&|tag| tag != ChangeTag::Delete);
    let longer = before_words.max(after_words);
    if longer == 0 || kept * 100 < longer * MIN_KEPT_PERCENT {
        return None;
    }
    Some(group_pieces(&merge_tokens(&tokens)))
}

/// The tokens with neighbours of one kind joined.
fn merge_tokens(tokens: &[(ChangeTag, &str)]) -> Vec<DiffPiece> {
    let mut pieces: Vec<DiffPiece> = Vec::new();
    for (tag, text) in tokens {
        let kind = match tag {
            ChangeTag::Equal => DiffPieceKind::Same,
            ChangeTag::Insert => DiffPieceKind::Added,
            ChangeTag::Delete => DiffPieceKind::Removed,
        };
        match pieces.last_mut() {
            Some(last) if last.kind == kind => last.text.push_str(text),
            _ => pieces.push(DiffPiece::new(kind, text)),
        }
    }
    pieces
}

/// Joins each run of edits split only by whitespace into one removed and one added piece, the whitespace in both.
/// A run that only adds or only removes stays as it is.
fn group_pieces(pieces: &[DiffPiece]) -> Vec<DiffPiece> {
    let mut grouped = Vec::new();
    let mut run: Vec<&DiffPiece> = Vec::new();
    for (index, piece) in pieces.iter().enumerate() {
        let inside_run = piece.kind != DiffPieceKind::Same
            || (!run.is_empty()
                && is_blank(&piece.text)
                && pieces
                    .get(index + 1)
                    .is_some_and(|next| next.kind != DiffPieceKind::Same));
        if inside_run {
            run.push(piece);
            continue;
        }
        flush_run(&mut grouped, &mut run);
        grouped.push(piece.clone());
    }
    flush_run(&mut grouped, &mut run);
    grouped
}

fn flush_run(grouped: &mut Vec<DiffPiece>, run: &mut Vec<&DiffPiece>) {
    let has = |kind| run.iter().any(|piece| piece.kind == kind);
    if has(DiffPieceKind::Removed) && has(DiffPieceKind::Added) {
        for kind in [DiffPieceKind::Removed, DiffPieceKind::Added] {
            let text: String = run
                .iter()
                .filter(|piece| piece.kind == kind || piece.kind == DiffPieceKind::Same)
                .map(|piece| piece.text.as_str())
                .collect();
            grouped.push(DiffPiece { kind, text });
        }
    } else {
        // Only one kind of edit: the pieces already alternate with the whitespace between them.
        grouped.extend(run.iter().map(|piece| (*piece).clone()));
    }
    run.clear();
}

fn is_blank(text: &str) -> bool {
    text.chars().all(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use proptest::prelude::*;

    use super::DiffPieceKind::{Added, Removed, Same};
    use super::*;

    fn piece(kind: DiffPieceKind, text: &str) -> DiffPiece {
        DiffPiece::new(kind, text)
    }

    fn rebuild(pieces: &[DiffPiece], side: DiffPieceKind) -> String {
        pieces
            .iter()
            .filter(|piece| piece.kind == Same || piece.kind == side)
            .map(|piece| piece.text.as_str())
            .collect()
    }

    #[gtest]
    fn a_small_edit_marks_only_the_changed_words() {
        let pieces = diff_words("Ship the web app on Friday", "Ship the web app on Monday");

        expect_that!(
            pieces,
            some(elements_are![
                eq(&piece(Same, "Ship the web app on ")),
                eq(&piece(Removed, "Friday")),
                eq(&piece(Added, "Monday")),
            ])
        );
    }

    #[gtest]
    fn a_reworded_phrase_reads_as_one_removed_and_one_added_piece() {
        let pieces = diff_words(
            "We ship it in early spring to every team",
            "We ship it after our review to every team",
        );

        expect_that!(
            pieces,
            some(elements_are![
                eq(&piece(Same, "We ship it ")),
                eq(&piece(Removed, "in early spring")),
                eq(&piece(Added, "after our review")),
                eq(&piece(Same, " to every team")),
            ])
        );
    }

    #[gtest]
    fn line_breaks_are_kept() {
        let pieces = diff_words("First line\nSecond line\nThird", "First line\nSecond row\nThird");

        expect_that!(
            pieces,
            some(elements_are![
                eq(&piece(Same, "First line\nSecond ")),
                eq(&piece(Removed, "line")),
                eq(&piece(Added, "row")),
                eq(&piece(Same, "\nThird")),
            ])
        );
    }

    #[gtest]
    fn an_added_sentence_is_one_added_piece() {
        let pieces = diff_words("Done when it ships.", "Done when it ships. Then tell the team.");

        expect_that!(
            pieces,
            some(elements_are![
                eq(&piece(Same, "Done when it ships.")),
                eq(&piece(Added, " Then tell the team.")),
            ])
        );
    }

    #[gtest]
    fn a_rewrite_has_no_diff() {
        expect_that!(diff_words("Old plan for the launch", "A wholly new idea here"), none());
        // One word of five kept is 20%, under the cut-off.
        expect_that!(diff_words("Write the docs today", "Write tests and fix bugs"), none());
    }

    #[gtest]
    fn the_cut_off_counts_the_longer_versions_words() {
        // Two of five words kept: exactly 40%.
        expect_that!(diff_words("one two", "one two three four five"), some(anything()));
        // Two of six: under.
        expect_that!(diff_words("one two", "one two three four five six"), none());
    }

    #[gtest]
    fn equal_or_wordless_texts_have_no_diff() {
        expect_that!(diff_words("Same text", "Same text"), none());
        expect_that!(diff_words(" ", "\n"), none());
    }

    #[gtest]
    fn a_spacing_change_is_a_diff_of_whitespace_only() {
        let pieces = diff_words("one two", "one\ntwo");

        expect_that!(
            pieces,
            some(elements_are![
                eq(&piece(Same, "one")),
                eq(&piece(Removed, " ")),
                eq(&piece(Added, "\n")),
                eq(&piece(Same, "two")),
            ])
        );
    }

    proptest! {
        #[test]
        fn the_pieces_rebuild_both_versions(
            before in "[ab \n]{0,40}",
            after in "[ab \n]{0,40}",
        ) {
            if let Some(pieces) = diff_words(&before, &after) {
                prop_assert_eq!(rebuild(&pieces, Removed), before);
                prop_assert_eq!(rebuild(&pieces, Added), after);
                prop_assert!(pieces.windows(2).all(|pair| pair[0].kind != pair[1].kind));
                prop_assert!(pieces.iter().all(|piece| !piece.text.is_empty()));
            }
        }
    }
}
