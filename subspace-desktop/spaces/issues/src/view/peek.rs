//! The inspector beside the list: a peek of the selected issue, drawn from
//! the row the list already holds. One click on a row selects it and fills
//! the peek; Open, a double click or Enter opens the issue page.

use chrono::Utc;
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, Div, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, div,
};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::dates::relative_time;
use subspace_framework::dashboard::rich::plain_text_with;
use subspace_framework::dashboard::row_menu::MenuChat;
use subspace_framework::dashboard::workspace::{self, Inspector};
use subspace_framework::dashboard::{Tokens, feedback, heading_font};
use subspace_framework::lucide::IconName;

use crate::Issues;
use crate::docks::Dock;
use crate::peek::Peek;
use crate::view::lanes;

/// The inspector's title before an issue is selected.
const NO_SELECTION_TITLE: &str = "Issue";
const NO_SELECTION: &str = "Select an issue to see it here.";

pub(super) fn inspector(issues: &Issues, cx: &mut Context<Issues>) -> Inspector {
    let close = cx.listener(|this, _, _, cx| this.toggle_dock(Dock::Inspector, cx));
    let selected = issues
        .list
        .selected()
        .and_then(|key| issues.issue(key).map(|item| (key.clone(), item)));
    let Some((key, item)) = selected else {
        let tokens = Tokens::current(cx);
        return Inspector::new("issues-peek", NO_SELECTION_TITLE)
            .on_close("Close inspector", close)
            .child(
                div()
                    .id("issues-peek-empty")
                    .test_support()
                    .child(feedback::empty_line(
                        &tokens,
                        "issues-peek-empty-text",
                        NO_SELECTION,
                    )),
            );
    };
    let age = relative_time(&item.fields.last_activity_at, Utc::now());
    let peek = crate::peek::peek(item, &issues.projects, &age);
    let open = action_button(
        "issues-peek-open",
        Intent::Neutral,
        Some(IconName::ArrowUpRight),
        "Open",
    )
    .on_click(cx.listener(move |this, _, _, cx| this.open_row(&key, cx)));
    Inspector::new("issues-peek", peek.identifier.clone())
        .action(open)
        .on_close("Close inspector", close)
        .children(body(&peek, cx))
}

/// The state, title and summary, the properties, then the latest change.
fn body(peek: &Peek, cx: &App) -> Vec<AnyElement> {
    let tokens = Tokens::current(cx);
    let mut parts = vec![
        lead(peek, &tokens, cx).into_any_element(),
        properties(peek, cx),
    ];
    parts.extend(peek.activity.as_deref().map(|activity| {
        workspace::section("Latest activity", cx)
            .child(text("issues-peek-activity", activity, &tokens))
            .test_support()
            .into_any_element()
    }));
    parts
}

fn lead(peek: &Peek, tokens: &Tokens, cx: &App) -> Div {
    let (icon, tone) = lanes::state_icon(peek.state_kind);
    let colour = tokens.tone(tone);
    let state = h_flex()
        .id("issues-peek-state")
        .aria_label(peek.state.clone())
        .test_support()
        .gap_1p5()
        .text_size(tokens.secondary_size())
        .text_color(colour)
        .child(Icon::from(icon).xsmall().text_color(colour))
        .child(peek.state.clone());
    let title = div()
        .id("issues-peek-title")
        .aria_label(peek.title.clone())
        .test_support()
        .font_family(heading_font(cx))
        .text_size(tokens.heading_font)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(tokens.fg_strong)
        .child(plain_text_with(
            "issues-peek-title-text",
            &peek.title,
            MenuChat::Off,
        ));
    v_flex()
        .w_full()
        .min_w_0()
        .gap_2()
        .child(state)
        .child(title)
        .children(
            peek.summary
                .as_deref()
                .map(|summary| text("issues-peek-summary", summary, tokens)),
        )
}

/// Priority, assignee and project, then labels and the due date when the
/// issue has them.
fn properties(peek: &Peek, cx: &App) -> AnyElement {
    workspace::section("Properties", cx)
        .children(
            peek.facts
                .iter()
                .map(|(label, value)| workspace::fact(*label, value.clone(), cx).test_support()),
        )
        .test_support()
        .into_any_element()
}

/// Issue text: selectable, and never offered to chat.
fn text(id: &'static str, value: &str, tokens: &Tokens) -> Div {
    div()
        .min_w_0()
        .text_size(tokens.secondary_size())
        .text_color(tokens.fg)
        .child(plain_text_with(
            SharedString::from(id),
            value,
            MenuChat::Off,
        ))
}
