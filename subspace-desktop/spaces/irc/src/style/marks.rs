//! Links and references marked in styled text: which ranges are links or
//! references, an issue link taken as a reference, unavailable issue
//! references and the keyboard focus.

use std::ops::Range;

use super::{RunStyle, Styled, styled};
use crate::markup::{self, Mentions, Ref};

/// `text` as [`styled`] does, with `#channel`, member and issue references
/// marked where they do not overlap a link. An issue link is a reference,
/// not a link.
pub(crate) fn with_refs(text: &str, mentions: &Mentions) -> Styled {
    let mut styled = styled(text);
    let found = markup::refs(&styled.text, mentions);
    styled.links = without_issues(&styled.links, &found);
    styled.refs = without_links(found, &styled.links);
    styled.runs = with_marks(&styled.runs, &marks(&styled.links, &styled.refs));
    styled
}

/// The links that no issue or pull request reference overlaps: those are
/// references that open in their board.
fn without_issues(
    links: &[(Range<usize>, String)],
    found: &[(Range<usize>, Ref)],
) -> Vec<(Range<usize>, String)> {
    links
        .iter()
        .filter(|(link, _)| {
            !found.iter().any(|(range, found)| {
                matches!(found, Ref::Issue(_) | Ref::PullRequest(_))
                    && link.start < range.end
                    && range.start < link.end
            })
        })
        .cloned()
        .collect()
}

/// `styled` with the runs inside `range` marked as keyboard focus.
pub(crate) fn with_focus(mut styled: Styled, range: Option<Range<usize>>) -> Styled {
    let Some(focus) = range else {
        return styled;
    };
    let mut covered = false;
    for (run, style) in &mut styled.runs {
        style.focused = focus.start <= run.start && run.end <= focus.end;
        covered |= style.focused;
    }
    if !covered {
        let style = RunStyle {
            focused: true,
            ..RunStyle::default()
        };
        styled.runs.push((focus, style));
        styled.runs.sort_by_key(|(run, _)| run.start);
    }
    styled
}

/// `styled` with the issue references that `unavailable` names marked, so
/// they show muted.
pub(crate) fn with_unavailable(mut styled: Styled, unavailable: impl Fn(&str) -> bool) -> Styled {
    let muted: Vec<Range<usize>> = styled
        .refs
        .iter()
        .filter_map(|(range, found)| match found {
            Ref::Issue(identifier) if unavailable(identifier) => Some(range.clone()),
            _ => None,
        })
        .collect();
    for (range, style) in &mut styled.runs {
        style.unavailable = style.reference
            && muted
                .iter()
                .any(|m| m.start <= range.start && range.end <= m.end);
    }
    styled
}

/// The references that do not overlap a link.
fn without_links(
    found: Vec<(Range<usize>, Ref)>,
    links: &[(Range<usize>, String)],
) -> Vec<(Range<usize>, Ref)> {
    found
        .into_iter()
        .filter(|(range, _)| {
            !links
                .iter()
                .any(|(link, _)| link.start < range.end && range.start < link.end)
        })
        .collect()
}

/// Link and reference ranges, each with whether it is a reference.
pub(super) fn marks(
    links: &[(Range<usize>, String)],
    refs: &[(Range<usize>, Ref)],
) -> Vec<(Range<usize>, bool)> {
    links
        .iter()
        .map(|(range, _)| (range.clone(), false))
        .chain(refs.iter().map(|(range, _)| (range.clone(), true)))
        .collect()
}

/// The runs with every link and reference range split out and marked, so
/// a link keeps the formatting it sits in.
pub(super) fn with_marks(
    runs: &[(Range<usize>, RunStyle)],
    marks: &[(Range<usize>, bool)],
) -> Vec<(Range<usize>, RunStyle)> {
    let mut bounds: Vec<usize> = runs
        .iter()
        .flat_map(|(range, _)| [range.start, range.end])
        .chain(marks.iter().flat_map(|(range, _)| [range.start, range.end]))
        .collect();
    bounds.sort_unstable();
    bounds.dedup();
    bounds
        .windows(2)
        .filter_map(|pair| {
            let range = pair[0]..pair[1];
            let base = runs
                .iter()
                .find(|(run, _)| run.start <= range.start && range.end <= run.end)
                .map(|(_, style)| *style);
            let mark = marks
                .iter()
                .find(|(mark, _)| mark.start <= range.start && range.end <= mark.end)
                .map(|(_, reference)| *reference);
            let style = RunStyle {
                link: mark == Some(false),
                reference: mark == Some(true),
                ..base.unwrap_or_default()
            };
            (style != RunStyle::default()).then_some((range, style))
        })
        .collect()
}
