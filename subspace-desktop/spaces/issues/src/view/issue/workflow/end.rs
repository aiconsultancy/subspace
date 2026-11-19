//! The end slot: the path's last stop, holding the issue's end state. Where
//! the issue can be edited it is a quiet button that opens the shared menu of
//! the project's end states, the current one checked; a pick changes the
//! state with the editor's write. Trackers, whose state Subspace API derives
//! from their sub-issues, and read-only issues show it as a plain stop.
//!
//! The board has no duplicate-of picker, so Duplicate changes the state like
//! any other end state.

use gpui_kit::component::{Icon, Sizable as _, h_flex, spinner::Spinner, v_flex};
use gpui_kit::{
    AnyElement, App, Context, Div, IntoElement as _, ParentElement as _, Styled as _, Window, phi,
};
use subspace_api_client::model::{IssueDetail, WorkflowState};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::row_button;
use subspace_framework::dashboard::menu::{MenuContents, MenuEvent, MenuOption};
use subspace_framework::lucide::IconName;

use super::FlightPath;
use super::drawing::{beacon, caption, cell, mark_colour, ring, waypoint};
use crate::Issues;

/// The end slot's button and its menu anchor.
const ANCHOR: &str = "issue-end-state";

/// The end slot's state, and whether it is current or changing.
struct Stop<'a> {
    state: &'a WorkflowState,
    current: bool,
    busy: bool,
}

pub(super) fn slot(
    issues: &Issues,
    detail: &IssueDetail,
    path: &FlightPath<'_>,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Option<AnyElement> {
    let state = path.end?;
    if !changeable(issues, detail) {
        return Some(waypoint(state, path.current_id, tokens, cx).into_any_element());
    }
    let stop = Stop {
        state,
        current: state.id == path.current_id,
        busy: issues.changing_state(&detail.fields.id),
    };
    Some(
        cell()
            .child(trigger(issues, &stop, tokens, cx))
            .into_any_element(),
    )
}

fn changeable(issues: &Issues, detail: &IssueDetail) -> bool {
    detail.fields.kind != "tracker" && issues.can_edit_issue()
}

fn trigger(
    issues: &Issues,
    stop: &Stop<'_>,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> AnyElement {
    // A quiet row button sized to the stop, so its beacon stays on the trail.
    let button = row_button(ANCHOR, label(stop), content(stop, tokens, cx))
        .w_auto()
        .h_auto()
        .loading(stop.busy)
        .on_click(cx.listener(|this, _, window, cx| open(this, window, cx)));
    issues.menu_anchors.trigger(ANCHOR, button, cx)
}

/// What assistive technology reads, such as
/// *End state: Cancelled. Change end state*.
fn label(stop: &Stop<'_>) -> String {
    let action = if stop.busy {
        "Changing end state"
    } else {
        "Change end state"
    };
    format!("End state: {}. {action}", stop.state.name)
}

fn content(stop: &Stop<'_>, tokens: &Tokens, cx: &App) -> Div {
    let mark = if stop.busy {
        ring(stop.current, tokens).child(Spinner::new().small().color(mark_colour(
            stop.current,
            tokens.muted,
            cx,
        )))
    } else {
        beacon(stop.state, stop.current, tokens, cx)
    };
    // The button's tighter line height would lift the caption above its
    // peers' on the other stops; keep the page's default.
    v_flex()
        .flex_1()
        .items_center()
        .gap_1()
        .line_height(phi())
        .child(mark)
        .child(
            h_flex()
                .gap_0p5()
                .child(caption(&stop.state.name, stop.current, tokens))
                .child(
                    Icon::from(IconName::ChevronDown)
                        .xsmall()
                        .text_color(tokens.muted),
                ),
        )
}

fn open(issues: &mut Issues, window: &mut Window, cx: &mut Context<Issues>) {
    let options = end_options(issues);
    if options.is_empty() {
        return;
    }
    let contents = MenuContents::new("End state", "Find state", options);
    let anchor = issues.menu_anchors.anchor(ANCHOR, cx);
    let Some(menu) = issues.menu_anchors.open(anchor, contents, window, cx) else {
        return;
    };
    cx.subscribe(&menu, |this, _, event: &MenuEvent, cx| {
        picked(this, event, cx)
    })
    .detach();
    issues.menu = Some(menu);
    cx.notify();
}

fn picked(issues: &mut Issues, event: &MenuEvent, cx: &mut Context<Issues>) {
    match event {
        MenuEvent::Pick(state) => {
            issues.menu = None;
            issues.change_issue_state(state.to_string(), cx);
        }
        MenuEvent::Close => issues.menu = None,
        MenuEvent::Query(_) => {}
    }
    cx.notify();
}

/// The open issue's end states, read when the menu opens.
fn end_options(issues: &Issues) -> Vec<MenuOption> {
    issues
        .issue_view
        .snapshot
        .as_ref()
        .and_then(|open| open.detail())
        .and_then(|detail| super::path(detail, &issues.projects))
        .map(|path| options(&path))
        .unwrap_or_default()
}

/// Every end state, completed then canceled then duplicate, the current
/// one checked.
fn options(path: &FlightPath<'_>) -> Vec<MenuOption> {
    path.ends
        .iter()
        .map(|state| {
            MenuOption::new(state.id.clone(), state.name.clone())
                .checked(state.id == path.current_id)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Stop, label, options};
    use crate::view::issue::workflow::FlightPath;
    use crate::view::issue::workflow::tests::states;

    #[test]
    fn the_menu_offers_every_end_state_by_type_and_checks_only_the_current_one() {
        let states = states();
        let menu = |current| {
            options(&FlightPath::new(&states, current))
                .into_iter()
                .map(|option| (option.value.to_string(), option.checked))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            menu("todo"),
            [
                ("done".into(), false),
                ("cancel".into(), false),
                ("duplicate".into(), false)
            ]
        );
        assert_eq!(
            menu("cancel"),
            [
                ("done".into(), false),
                ("cancel".into(), true),
                ("duplicate".into(), false)
            ]
        );
    }

    #[test]
    fn the_slot_names_its_state_and_its_action() {
        let states = states();
        let path = FlightPath::new(&states, "cancel");
        let stop = |busy| Stop {
            state: path.end.unwrap(),
            current: true,
            busy,
        };
        assert_eq!(
            label(&stop(false)),
            "End state: Cancelled. Change end state"
        );
        assert_eq!(
            label(&stop(true)),
            "End state: Cancelled. Changing end state"
        );
    }
}
