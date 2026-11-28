//! The tiles under a message: one for each reference this message owns
//! (see the module's rule), in reading order, wrapping; then the summary
//! an issue tile's disclosure opened.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{AnyElement, App, IntoElement as _, ParentElement as _, Styled as _};

use crate::markup::Ref;
use crate::state::Entry;
use crate::transcript::RowContext;

/// The tile row of `entry`, whose text names `refs` (its tile
/// candidates); `None` when it owns none of them.
pub(crate) fn row(
    entry: &Entry,
    refs: &[Ref],
    context: &RowContext,
    cx: &App,
) -> Option<AnyElement> {
    let hub = context.hub.upgrade();
    let mut owned = owned(entry.seq, refs, context);
    owned.retain(|found| match found {
        Ref::Issue(identifier) => !hub
            .as_ref()
            .is_some_and(|hub| hub.read(cx).issues.is_unavailable(identifier)),
        _ => true,
    });
    if owned.is_empty() {
        return None;
    }
    let issues: Vec<&str> = owned
        .iter()
        .filter_map(|found| match found {
            Ref::Issue(identifier) => Some(identifier.as_str()),
            _ => None,
        })
        .collect();
    Some(
        v_flex()
            .w_full()
            .min_w_0()
            .mt_1()
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .gap_2()
                    .items_start()
                    .flex_wrap()
                    .children(owned.iter().map(|found| tile(entry, found, context, cx))),
            )
            .children(crate::issues::panel(entry, &issues, context, cx))
            .into_any_element(),
    )
}

/// The references of `refs` whose tile message `seq` keeps.
fn owned<'a>(seq: i64, refs: &'a [Ref], context: &RowContext) -> Vec<&'a Ref> {
    refs.iter()
        .filter(|found| context.tiles.shows(seq, found))
        .collect()
}

fn tile(entry: &Entry, found: &Ref, context: &RowContext, cx: &App) -> AnyElement {
    match found {
        Ref::Issue(identifier) => crate::issues::slot(entry, identifier, context, cx),
        Ref::PullRequest(found) => crate::pull_requests::slot(entry, found, context, cx),
        Ref::Channel(_) | Ref::Nick(_) | Ref::Decision(_) | Ref::Thread(..) => {
            gpui_kit::div().into_any_element()
        }
    }
}
