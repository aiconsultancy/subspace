//! A message's Tab stops: its links and references in reading order, then
//! its attachments.

use crate::hub::Hub;
use crate::markup::{self, Mentions, Ref, Rendered};
use crate::state::Entry;
use crate::style;
use crate::text::{self, Opens};

use super::line;

/// A message's links and references in reading order, as Tab walks them.
pub(crate) fn message_targets(
    entry: &Entry,
    mentions: &Mentions,
    hub: Option<&Hub>,
) -> Vec<(std::ops::Range<usize>, Opens)> {
    let shown = line::displayed(entry, hub);
    let visible = shown.as_ref().unwrap_or(entry);
    let styled = style::message_view(visible, mentions).styled;
    let rendered = markup::rich(&visible.line.text, mentions);
    let mut found = targets(&styled, rendered.as_ref());
    // Thumbnails come from the original text: resolved references are hidden
    // from the displayed line but are still the openable targets.
    found.extend(
        subspace_api_client::chat_uploads::references(&entry.line.text)
            .into_iter()
            .map(|key| (0..0, Opens::Upload(key))),
    );
    found
}

pub(super) fn targets(
    styled: &style::Styled,
    rendered: Option<&Rendered>,
) -> Vec<(std::ops::Range<usize>, Opens)> {
    let targets = text::targets(styled, true);
    match rendered {
        Some(rendered) => with_rendered_refs(targets, &rendered.refs),
        None => targets,
    }
}

/// `targets` with their issue and pull request references replaced by
/// `rendered`'s: those outside code, each at its own place in the message,
/// a pull request with the URL as written. A link the same text names gives
/// way to the reference. All in reading order.
fn with_rendered_refs(
    targets: Vec<(std::ops::Range<usize>, Opens)>,
    rendered: &[(std::ops::Range<usize>, Ref)],
) -> Vec<(std::ops::Range<usize>, Opens)> {
    let overlaps = |range: &std::ops::Range<usize>| {
        rendered
            .iter()
            .any(|(found, _)| found.start < range.end && range.start < found.end)
    };
    let mut kept: Vec<_> = targets
        .into_iter()
        .filter(|(range, target)| {
            !matches!(target, Opens::Ref(Ref::Issue(_) | Ref::PullRequest(_))) && !overlaps(range)
        })
        .chain(
            rendered
                .iter()
                .map(|(range, found)| (range.clone(), Opens::Ref(found.clone()))),
        )
        .collect();
    kept.sort_by_key(|(range, _)| range.start);
    kept
}
