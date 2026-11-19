//! The local Activity tabs use Kit's native selection and add keyboard focus.
use crate::{
    Issues,
    detail::timeline::{TimelineFocus, TimelineMode},
};
use gpui_kit::{Context, InteractiveElement as _, KeyDownEvent, prelude::FluentBuilder as _};
use subspace_api_client::hub::IssueSnapshot;
use subspace_framework::dashboard::{
    Tokens,
    tabs::{self, TabSpec, TabStatus},
};
use subspace_framework::lucide::IconName;

pub(super) fn render(
    issues: &Issues,
    issue: &IssueSnapshot,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> tabs::TabBar {
    let selected = issues.issue_view.timeline.mode;
    let focus = issues.issue_view.timeline_focus.as_ref();
    let specs = [
        TabSpec::plain("Comments"),
        TabSpec::plain("All"),
        decisions(issue, tokens),
    ];
    tabs::tab_bar("issue-timeline-tabs")
        .selected_index(selected.tab_index())
        .children(specs.into_iter().enumerate().map(|(index, spec)| {
            focus_tab(
                tabs::tab(spec),
                TimelineMode::at(index),
                selected,
                focus,
                cx,
            )
        }))
        .on_click(cx.listener(|this, index: &usize, _, cx| {
            this.show_timeline(TimelineMode::at(*index), cx);
        }))
}

fn decisions(issue: &IssueSnapshot, tokens: &Tokens) -> TabSpec {
    TabSpec::new("Decisions", IconName::Scale).when(issue.decisions_loading(), |spec| {
        spec.status(TabStatus::busy(tokens.muted))
    })
}

// Kit's tabs otherwise only activate by pointer; a focused native tab's
// click handler accepts Enter/Space through GPUI's usual activation path.
fn focus_tab(
    tab: tabs::Tab,
    mode: TimelineMode,
    selected: TimelineMode,
    focus: Option<&TimelineFocus>,
    cx: &mut Context<Issues>,
) -> tabs::Tab {
    let Some(focus) = focus else { return tab };
    tab.track_focus(focus.tab(mode))
        .tab_index(0)
        .tab_stop(mode == selected)
        .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
            let key = &event.keystroke;
            let Some(focus) = &this.issue_view.timeline_focus else {
                return;
            };
            if !key.modifiers.modified() && matches!(key.key.as_str(), "left" | "right") {
                focus
                    .tab(mode.adjacent(key.key == "right"))
                    .focus(window, cx);
                cx.stop_propagation();
            }
        }))
}
