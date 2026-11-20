//! The explanation has no issue content and lives only while its anchor is open.
use crate::Issues;
use gpui_kit::component::{ThemeStyled as _, h_flex, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, Div, FocusHandle, Focusable, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, Styled as _, TestSupportExt as _, Window, div,
};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::lucide::IconName;

struct Explanation {
    focus: FocusHandle,
}

impl Focusable for Explanation {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Explanation {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        v_flex().id("issue-tracker-explanation").test_support().track_focus(&self.focus).popover_style(cx).w_80().gap_2().p_4()
            .child(super::section_title("How tracker status works", &tokens))
            .child(format!(
                "{0} calculates a tracker's state from its sub-issues. Subspace shows the state {0} returns.",
                subspace_api_client::service_name()
            ))
            .child("Completed sub-issues count towards done. Cancelled and duplicate sub-issues are shown separately.")
            .child(div().text_color(tokens.muted).child("Reload to read the latest status."))
    }
}

pub(super) fn render(issues: &Issues, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    let Some(anchor) = issues.issue_view.tracker_help.as_ref() else {
        return div();
    };
    let active = anchor.read(cx).is_open();
    let host = anchor.clone();
    let button = action_button(
        "issue-tracker-help",
        Intent::Quiet,
        Some(IconName::Info),
        "How tracker status works",
    )
    .on_click(move |_, window, cx| {
        if host.read(cx).is_open() {
            host.update(cx, |host, cx| host.dismiss(window, cx));
            return;
        }
        let explanation = cx.new(|cx| Explanation {
            focus: cx.focus_handle(),
        });
        host.update(cx, |host, cx| host.show(explanation, window, cx));
    })
    .latched(active);
    h_flex()
        // Main-column stops come first; the explanation and Properties follow
        // them, even though this trigger is drawn in the header (spec §10).
        .tab_group()
        .gap_2()
        .flex_wrap()
        .text_color(tokens.muted)
        .text_size(tokens.secondary_size())
        .child("Tracker status is calculated from its sub-issues.")
        .child(
            h_flex()
                .child(subspace_framework::dashboard::anchored::trigger(
                    anchor, button,
                ))
                .child(anchor.clone()),
        )
}
