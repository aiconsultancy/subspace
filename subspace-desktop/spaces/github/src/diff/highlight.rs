//! Syntax colours for the diff view, through GPUI Kit's active tree-sitter
//! highlighter. A patch holds fragments of a file, so each side (the old
//! lines, the new lines) is parsed as one text of its hunks' lines.

use std::ops::Range;

use gpui_kit::base::input::HighlightStyleResolver;
use gpui_kit::{HighlightStyle, Hsla};
pub use subspace_framework::dashboard::code::{Runs, language, line_runs as side_runs};

use super::model::{DiffLine, LineKind, code_without_cr};

/// Tabs show as four spaces; highlights and word marks use the shown text.
pub fn shown(text: &str) -> String {
    code_without_cr(text).replace('\t', "    ")
}

/// Style runs for every line of `lines` (a file's patch lines): the old
/// side colours deletions, the new side additions and context.
pub fn highlight(
    language: &str,
    lines: &[DiffLine],
    theme: &dyn HighlightStyleResolver,
) -> Vec<Runs> {
    let mut out = vec![Runs::new(); lines.len()];
    for new_side in [false, true] {
        let members: Vec<usize> = (0..lines.len())
            .filter(|&index| match lines[index].kind {
                LineKind::Context => new_side,
                LineKind::Add => new_side,
                LineKind::Del => !new_side,
            })
            .collect();
        if members.is_empty() {
            continue;
        }
        let texts: Vec<String> = members.iter().map(|&i| shown(&lines[i].text)).collect();
        for (index, runs) in members.iter().zip(side_runs(language, &texts, theme)) {
            out[*index] = runs;
        }
    }
    out
}

/// Syntax runs with changed-word backgrounds laid over them: the ranges are
/// split where either list changes, so none overlap.
pub fn overlay(syntax: &Runs, words: &[Range<usize>], word: Hsla, len: usize) -> Runs {
    let mut cuts: Vec<usize> = vec![0, len];
    for (range, _) in syntax {
        cuts.extend([range.start, range.end]);
    }
    for range in words {
        cuts.extend([range.start, range.end]);
    }
    cuts.retain(|&cut| cut <= len);
    cuts.sort_unstable();
    cuts.dedup();
    let mut out = Runs::new();
    for pair in cuts.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        let mut style = syntax
            .iter()
            .find(|(range, _)| range.start <= start && end <= range.end)
            .map(|(_, style)| *style)
            .unwrap_or_default();
        if words
            .iter()
            .any(|range| range.start <= start && end <= range.end)
        {
            style.background_color = Some(word);
        }
        if style != HighlightStyle::default() {
            out.push((start..end, style));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use gpui_kit::{HighlightStyle, Hsla};

    use super::overlay;

    #[test]
    fn word_marks_split_syntax_runs() {
        let red = Hsla::red();
        let style = HighlightStyle {
            color: Some(Hsla::blue()),
            ..HighlightStyle::default()
        };
        let words = [4..8, 20..30];
        let runs = overlay(&vec![(0..6, style)], &words[..1], red, 10);
        let ranges: Vec<_> = runs.iter().map(|(range, _)| range.clone()).collect();
        assert_eq!(ranges, [0..4, 4..6, 6..8]);
        assert_eq!(runs[1].1.color, Some(Hsla::blue()));
        assert_eq!(runs[1].1.background_color, Some(red));
        assert_eq!(runs[2].1.color, None);
    }
}
